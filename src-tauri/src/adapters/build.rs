//! Terminal builds adapter (card §15.1). Watches process names / command lines from the watch list
//! (card §15.3) and maps a watched process's exit code to Done (0) or NeedsYou (non-zero).
//!
//! The matching and tracking logic is pure and runs against the `ProcessSource` trait so it is
//! unit-tested on any OS. `SysProcessSource` is the real source: `sysinfo` for the process list
//! (current user only) and, on Windows, a held `OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION)`
//! handle plus `GetExitCodeProcess` for the exit code. The Windows path is not verified on Linux CI.
//! Reads process name, command line and exit code only (card §15.6).
use super::{Adapter, ToolState};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind { Process, Command }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEntry {
    #[serde(rename = "match")]
    pub pattern: String,
    pub kind: Kind,
}

impl WatchEntry {
    pub fn new(p: &str, kind: Kind) -> Self { Self { pattern: p.to_string(), kind } }
    /// `process`: exe name exact, case-insensitive. `command`: whole words, case-insensitive — the entry's
    /// words must appear consecutively in the command line's words (card §15.3), so `tsc` ≠ `tsconfig`.
    pub fn matches(&self, name: &str, cmdline: &str) -> bool {
        let p = self.pattern.trim().to_lowercase();
        if p.is_empty() { return false; }
        match self.kind {
            Kind::Process => name.to_lowercase() == p,
            Kind::Command => {
                let want = words(&p);
                if want.is_empty() { return false; }
                let have = words(cmdline);
                have.windows(want.len()).any(|w| w == want.as_slice())
            }
        }
    }
}

/// Lowercase words: runs of letters/digits; everything else (spaces, `\`, `/`, `.`, `-`, quotes) separates.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// Never a build (card §15.3): the long-lived Gradle daemon, and `tsc` in watch mode.
pub fn is_excluded(name: &str, cmdline: &str) -> bool {
    let c = cmdline.to_lowercase();
    if c.contains("gradledaemon") || c.contains("org.gradle.launcher.daemon") { return true; }
    let n = name.to_lowercase();
    let is_tsc = n == "tsc" || n == "tsc.exe" || words(&c).iter().any(|w| w == "tsc");
    is_tsc && c.split_whitespace().any(|a| a == "--watch" || a == "-w" || a.starts_with("--watch="))
}

/// Card §15.3 default list (rev with Sol's edits): `.exe` entries are `process` (`msbuild.exe`, `tsc.exe`),
/// everything else is a whole-word `command`.
pub fn default_watch() -> Vec<WatchEntry> {
    ["cargo build", "cargo run", "npm run build", "pnpm build", "yarn build", "gradle", "dotnet build"]
        .iter().map(|p| WatchEntry::new(p, Kind::Command))
        .chain(["msbuild.exe", "tsc.exe"].iter().map(|p| WatchEntry::new(p, Kind::Process)))
        .chain(std::iter::once(WatchEntry::new("vite build", Kind::Command)))
        .collect()
}

const SHELLS: [&str; 5] = ["cmd", "powershell", "pwsh", "bash", "node"];

/// Interactive shells can't be `process` entries (card §15.3). Blocked with or without `.exe` (S2.5 TBD 3).
pub fn is_blocked_shell(e: &WatchEntry) -> bool {
    if e.kind != Kind::Process { return false; }
    let p = e.pattern.trim().to_lowercase();
    let base = p.strip_suffix(".exe").unwrap_or(&p);
    SHELLS.contains(&base)
}

/// Loaded-settings cleanup: trims, drops empty and blocked entries, removes duplicates
/// (same text + kind, case-insensitive) silently, keeping the first.
pub fn normalize_watch(list: Vec<WatchEntry>) -> Vec<WatchEntry> {
    let mut seen = HashSet::new();
    list.into_iter()
        .map(|e| WatchEntry { pattern: e.pattern.trim().to_string(), kind: e.kind })
        .filter(|e| !e.pattern.is_empty() && !is_blocked_shell(e))
        .filter(|e| seen.insert((e.pattern.to_lowercase(), e.kind)))
        .collect()
}

/// Parse `settings.watch`; a missing or malformed field means the defaults.
pub fn watch_from_settings(v: &serde_json::Value) -> Vec<WatchEntry> {
    match serde_json::from_value::<Vec<WatchEntry>>(v["watch"].clone()) {
        Ok(l) => normalize_watch(l),
        Err(_) => default_watch(),
    }
}

/// Exit code mapping (card §15.1): 0 → Done, anything else → NeedsYou (the build failed).
pub fn exit_to_state(code: i64) -> ToolState { if code == 0 { ToolState::Done } else { ToolState::NeedsYou } }

#[derive(Debug, Clone)]
pub struct ProcInfo { pub pid: u32, pub ppid: Option<u32>, pub name: String, pub cmdline: String }

/// OS boundary. Everything behind it is the part that can't be verified without Windows.
pub trait ProcessSource: Send {
    /// Current user's processes.
    fn list(&mut self) -> Vec<ProcInfo>;
    /// Start holding the process (Windows: OpenProcess). false = can't open (e.g. elevated) → skipped.
    fn open(&mut self, pid: u32) -> bool;
    /// None while running; Some(exit code) once it has ended. Releases the handle on exit.
    fn exit_code(&mut self, pid: u32) -> Option<i64>;
    /// Stop holding the process without reading its exit (Windows: CloseHandle).
    fn release(&mut self, pid: u32);
}

/// Pure tracker over a `ProcessSource`.
pub struct Tracker<S: ProcessSource> {
    pub source: S,
    pub watch: Vec<WatchEntry>,
    tracked: HashMap<u32, ()>,
}

impl<S: ProcessSource> Tracker<S> {
    pub fn new(source: S, watch: Vec<WatchEntry>) -> Self { Self { source, watch, tracked: HashMap::new() } }
    pub fn tracked_count(&self) -> usize { self.tracked.len() }

    /// Drop every tracked process and its handle (Terminal builds turned off, or the watch list changed),
    /// so no stale Done/NeedsYou or chime comes later.
    pub fn reset(&mut self) {
        for pid in std::mem::take(&mut self.tracked).into_keys() { self.source.release(pid); }
    }

    /// One poll. Some(NeedsYou) on the poll a watched process exits non-zero, even if another is still
    /// running (card §15.2); otherwise Some(Working) while any lives (each poll is a signal, card §15.1),
    /// Some(Done) once the last one ends, None when idle.
    pub fn poll(&mut self) -> Option<ToolState> {
        let had_any = !self.tracked.is_empty();
        let mut failed = false;
        // Exits first, so a reused PID is not mistaken for the old process.
        let pids: Vec<u32> = self.tracked.keys().copied().collect();
        for pid in pids {
            if let Some(code) = self.source.exit_code(pid) {
                self.tracked.remove(&pid);
                if exit_to_state(code) == ToolState::NeedsYou { failed = true; }
            }
        }
        let procs = self.source.list();
        // Nested matches count once: skip a match whose parent (or any ancestor) is tracked.
        let parent: HashMap<u32, Option<u32>> = procs.iter().map(|p| (p.pid, p.ppid)).collect();
        let mut new = vec![];
        for p in &procs {
            if self.tracked.contains_key(&p.pid) { continue; }
            if is_excluded(&p.name, &p.cmdline) || !self.watch.iter().any(|w| w.matches(&p.name, &p.cmdline)) { continue; }
            new.push(p.pid);
        }
        let mut sorted = new.clone();
        sorted.sort_unstable();
        let newset: HashSet<u32> = new.iter().copied().collect();
        for pid in sorted {
            let mut anc = parent.get(&pid).copied().flatten();
            let mut nested = false;
            let mut hops = 0;
            while let Some(a) = anc {
                if self.tracked.contains_key(&a) || (newset.contains(&a) && a != pid) { nested = true; break; }
                hops += 1; if hops > 64 { break; }
                anc = parent.get(&a).copied().flatten();
            }
            if nested { continue; }
            if self.source.open(pid) { self.tracked.insert(pid, ()); }
            else { eprintln!("build adapter: can't open pid {pid}, skipped"); }
        }
        if failed { return Some(ToolState::NeedsYou); }
        if !self.tracked.is_empty() { return Some(ToolState::Working); }
        if had_any { return Some(ToolState::Done); }
        None
    }
}

/// Real source. Process list via sysinfo; exit codes via a held handle on Windows.
pub struct SysProcessSource {
    sys: sysinfo::System,
    me: Option<sysinfo::Uid>,
    alive: HashSet<u32>,
    #[cfg(windows)]
    handles: HashMap<u32, windows::Win32::Foundation::HANDLE>,
}

// HANDLE is a raw pointer; it is only used from the core loop thread under the Core mutex.
unsafe impl Send for SysProcessSource {}

impl SysProcessSource {
    pub fn new() -> Self {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_user(UpdateKind::Always));
        let me = sysinfo::get_current_pid().ok().and_then(|p| sys.process(p)).and_then(|p| p.user_id().cloned());
        Self { sys, me, alive: HashSet::new(), #[cfg(windows)] handles: HashMap::new() }
    }
}

impl ProcessSource for SysProcessSource {
    fn list(&mut self) -> Vec<ProcInfo> {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, UpdateKind};
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true,
            ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_user(UpdateKind::Always));
        let mut out = vec![];
        self.alive.clear();
        for (pid, p) in self.sys.processes() {
            self.alive.insert(pid.as_u32());
            if self.me.is_some() && p.user_id() != self.me.as_ref() { continue; }
            let cmdline = p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ");
            out.push(ProcInfo { pid: pid.as_u32(), ppid: p.parent().map(|x| x.as_u32()), name: p.name().to_string_lossy().into_owned(), cmdline });
        }
        out
    }

    #[cfg(windows)]
    fn open(&mut self, pid: u32) -> bool {
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE};
        match unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
            Ok(h) => { self.handles.insert(pid, h); true }
            Err(_) => false,
        }
    }
    #[cfg(not(windows))]
    fn open(&mut self, _pid: u32) -> bool { true }

    #[cfg(windows)]
    fn exit_code(&mut self, pid: u32) -> Option<i64> {
        use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
        let h = *self.handles.get(&pid)?;
        if unsafe { WaitForSingleObject(h, 0) } != WAIT_OBJECT_0 { return None; }
        let mut code = 0u32;
        let ok = unsafe { GetExitCodeProcess(h, &mut code) }.is_ok();
        unsafe { let _ = CloseHandle(h); }
        self.handles.remove(&pid);
        Some(if ok { code as i32 as i64 } else { 1 })
    }
    #[cfg(windows)]
    fn release(&mut self, pid: u32) {
        if let Some(h) = self.handles.remove(&pid) { unsafe { let _ = windows::Win32::Foundation::CloseHandle(h); } }
    }
    #[cfg(not(windows))]
    fn release(&mut self, _pid: u32) {}
    /// Non-Windows (dev only): no handle, so the exit code is unknown and an ended process counts as 0.
    #[cfg(not(windows))]
    fn exit_code(&mut self, pid: u32) -> Option<i64> { if self.alive.contains(&pid) { None } else { Some(0) } }
}

pub struct BuildAdapter<S: ProcessSource = SysProcessSource> { tracker: Tracker<S>, last: Instant }

impl BuildAdapter<SysProcessSource> {
    pub fn new() -> Self { Self { tracker: Tracker::new(SysProcessSource::new(), default_watch()), last: Instant::now() } }
}

impl<S: ProcessSource> Adapter for BuildAdapter<S> {
    fn id(&self) -> &'static str { "build" }
    fn display_name(&self) -> &'static str { "Terminal builds" }
    fn is_target_running(&self) -> bool { self.tracker.tracked_count() > 0 }
    fn poll(&mut self) -> Option<ToolState> {
        let r = self.tracker.poll();
        if r.is_some() { self.last = Instant::now(); }
        r
    }
    fn last_signal_at(&self) -> Instant { self.last }
    /// Turning Terminal builds off or changing the watch list drops everything tracked (Kit bug 7).
    fn configure(&mut self, settings: &serde_json::Value) {
        let watch = watch_from_settings(settings);
        let enabled = settings["adapters"]["build"].as_bool() != Some(false);
        if !enabled || watch != self.tracker.watch { self.tracker.reset(); }
        self.tracker.watch = watch;
    }
    fn reset(&mut self) { self.tracker.reset(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Kind::*;

    #[derive(Default)]
    struct Fake { procs: Vec<ProcInfo>, exits: HashMap<u32, i64>, deny: HashSet<u32>, opened: Vec<u32>, released: Vec<u32> }
    impl Fake {
        fn spawn(&mut self, pid: u32, ppid: Option<u32>, name: &str, cmd: &str) { self.procs.push(ProcInfo { pid, ppid, name: name.into(), cmdline: cmd.into() }); }
        fn end(&mut self, pid: u32, code: i64) { self.procs.retain(|p| p.pid != pid); self.exits.insert(pid, code); }
    }
    impl ProcessSource for Fake {
        fn list(&mut self) -> Vec<ProcInfo> { self.procs.clone() }
        fn open(&mut self, pid: u32) -> bool { if self.deny.contains(&pid) { false } else { self.opened.push(pid); true } }
        fn exit_code(&mut self, pid: u32) -> Option<i64> { self.exits.remove(&pid) }
        fn release(&mut self, pid: u32) { self.released.push(pid); }
    }
    fn tracker() -> Tracker<Fake> { Tracker::new(Fake::default(), default_watch()) }

    #[test] fn process_kind_is_exact_case_insensitive() {
        let e = WatchEntry::new("cargo.exe", Process);
        assert!(e.matches("CARGO.EXE", ""));
        assert!(!e.matches("cargo.exe.bak", ""));
        assert!(!e.matches("cargo", "cargo.exe build"));
    }
    #[test] fn command_kind_is_whole_words_case_insensitive() {
        let e = WatchEntry::new("run build", Command);
        assert!(e.matches("npm.exe", "C:\\node\\NPM RUN BUILD --prod"));
        assert!(!e.matches("run build", "npm run dev"));
        assert!(!e.matches("x", "npm run builder"));
        assert!(!WatchEntry::new("tsc", Command).matches("node", "node tsx --tsconfig tsconfig.json"));
        assert!(WatchEntry::new("gradle", Command).matches("java", "C:\\gradle\\bin\\gradle.bat build"));
        assert!(!WatchEntry::new("gradle", Command).matches("x", "gradlew build"));
        assert!(!WatchEntry::new("  ", Command).matches("x", "anything"));
    }
    #[test] fn blocked_shells_with_and_without_exe_only_for_process() {
        for s in ["cmd.exe", "powershell.exe", "pwsh.exe", "bash.exe", "node.exe", "PWSH", "node"] { assert!(is_blocked_shell(&WatchEntry::new(s, Process)), "{s}"); }
        assert!(!is_blocked_shell(&WatchEntry::new("node.exe", Command)));
        assert!(!is_blocked_shell(&WatchEntry::new("cargo.exe", Process)));
    }
    #[test] fn normalize_dedupes_trims_and_drops_blocked() {
        let l = normalize_watch(vec![WatchEntry::new(" tsc ", Command), WatchEntry::new("TSC", Command), WatchEntry::new("tsc", Process),
            WatchEntry::new("", Command), WatchEntry::new("pwsh.exe", Process)]);
        assert_eq!(l, vec![WatchEntry::new("tsc", Command), WatchEntry::new("tsc", Process)]);
    }
    #[test] fn settings_parse_and_fallback() {
        let v = serde_json::json!({ "watch": [{ "match": "make", "kind": "command" }, { "match": "MAKE", "kind": "command" }] });
        assert_eq!(watch_from_settings(&v), vec![WatchEntry::new("make", Command)]);
        assert_eq!(watch_from_settings(&serde_json::json!({})), default_watch());
        assert_eq!(watch_from_settings(&serde_json::json!({ "watch": [] })), vec![]);
    }
    #[test] fn defaults_are_valid_and_unique() { assert_eq!(normalize_watch(default_watch()), default_watch()); assert_eq!(default_watch().len(), 10); }
    #[test] fn exit_code_mapping() {
        assert_eq!(exit_to_state(0), ToolState::Done);
        assert_eq!(exit_to_state(1), ToolState::NeedsYou);
        assert_eq!(exit_to_state(-1073741510), ToolState::NeedsYou);
    }
    #[test] fn watched_build_working_then_done() {
        let mut t = tracker();
        assert_eq!(t.poll(), None);
        t.source.spawn(10, Some(1), "cargo.exe", "cargo build --release");
        assert_eq!(t.poll(), Some(ToolState::Working));
        assert_eq!(t.poll(), Some(ToolState::Working));
        t.source.end(10, 0);
        assert_eq!(t.poll(), Some(ToolState::Done));
        assert_eq!(t.poll(), None);
    }
    #[test] fn failed_build_needs_you() {
        let mut t = tracker();
        t.source.spawn(10, None, "cargo.exe", "cargo build");
        t.poll();
        t.source.end(10, 101);
        assert_eq!(t.poll(), Some(ToolState::NeedsYou));
        // the failure flag doesn't leak into the next run
        t.source.spawn(11, None, "cargo.exe", "cargo build");
        t.poll(); t.source.end(11, 0);
        assert_eq!(t.poll(), Some(ToolState::Done));
    }
    #[test] fn tsx_watch_with_tsconfig_is_not_working() {
        let mut t = tracker();
        t.source.spawn(30, None, "node.exe", "node tsx watch --tsconfig tsconfig.json");
        assert_eq!(t.poll(), None);
    }
    #[test] fn tsc_process_counts_but_not_in_watch_mode() {
        let mut t = tracker();
        t.source.spawn(31, None, "tsc.exe", "tsc --watch");
        t.source.spawn(32, None, "tsc.exe", "tsc -w -p .");
        assert_eq!(t.poll(), None);
        t.source.spawn(33, None, "tsc.exe", "tsc -b");
        assert_eq!(t.poll(), Some(ToolState::Working));
    }
    #[test] fn gradle_daemon_is_not_working() {
        let mut t = tracker();
        t.source.spawn(40, None, "java.exe", "java -cp C:\\gradle\\lib\\gradle-launcher.jar org.gradle.launcher.daemon.bootstrap.GradleDaemon 8.10");
        assert_eq!(t.poll(), None);
        t.source.spawn(41, None, "java.exe", "java -cp gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain build");
        assert_eq!(t.poll(), Some(ToolState::Working));
    }
    #[test] fn failure_reports_needs_you_while_another_still_runs() {
        let mut t = tracker();
        t.source.spawn(1, None, "a", "cargo build");
        t.source.spawn(2, None, "b", "dotnet build");
        t.poll();
        t.source.end(1, 101);
        assert_eq!(t.poll(), Some(ToolState::NeedsYou));
        assert_eq!(t.poll(), Some(ToolState::Working));
        t.source.end(2, 0);
        assert_eq!(t.poll(), Some(ToolState::Done));
    }
    #[test] fn disabling_or_changing_watch_drops_tracked_with_no_stale_report() {
        let mut a = BuildAdapter { tracker: Tracker::new(Fake::default(), default_watch()), last: Instant::now() };
        a.tracker.source.spawn(1, None, "cargo", "cargo build");
        assert_eq!(a.poll(), Some(ToolState::Working));
        a.configure(&serde_json::json!({ "adapters": { "build": false } }));
        assert_eq!(a.tracker.source.released, vec![1]);
        a.tracker.source.end(1, 101);
        assert_eq!(a.poll(), None, "no stale NeedsYou after turning off");
        // watch-list change
        a.configure(&serde_json::json!({}));
        a.tracker.source.spawn(2, None, "cargo", "cargo build");
        assert_eq!(a.poll(), Some(ToolState::Working));
        a.configure(&serde_json::json!({ "watch": [{ "match": "make", "kind": "command" }] }));
        assert_eq!(a.tracker.source.released, vec![1, 2]);
        a.tracker.source.end(2, 0);
        assert_eq!(a.poll(), None, "no stale Done after a list change");
        // unchanged settings keep tracking
        a.tracker.source.spawn(3, None, "make", "make all");
        a.poll();
        a.configure(&serde_json::json!({ "watch": [{ "match": "make", "kind": "command" }] }));
        assert_eq!(a.tracker.source.released, vec![1, 2]);
    }
    #[test] fn unwatched_processes_do_nothing() {
        let mut t = tracker();
        t.source.spawn(5, None, "PING.EXE", "ping -n 10 localhost");
        t.source.spawn(6, None, "pwsh.exe", "pwsh.exe");
        assert_eq!(t.poll(), None);
    }
    #[test] fn nested_matches_count_once() {
        let mut t = tracker();
        t.source.spawn(20, None, "npm.cmd", "npm run build");
        t.source.spawn(21, Some(20), "node.exe", "node vite build");
        t.source.spawn(22, Some(21), "tsc.exe", "node tsc -b");
        t.poll();
        assert_eq!(t.source.opened, vec![20]);
        assert_eq!(t.tracked_count(), 1);
    }
    #[test] fn two_builds_done_only_when_both_end() {
        let mut t = tracker();
        t.source.spawn(1, None, "a", "cargo build");
        t.source.spawn(2, None, "b", "dotnet build");
        t.poll();
        t.source.end(1, 0);
        assert_eq!(t.poll(), Some(ToolState::Working));
        t.source.end(2, 0);
        assert_eq!(t.poll(), Some(ToolState::Done));
    }
    #[test] fn unopenable_process_is_skipped() {
        let mut t = tracker();
        t.source.deny.insert(7);
        t.source.spawn(7, None, "msbuild.exe", "msbuild.exe x.sln");
        assert_eq!(t.poll(), None);
    }
    #[test] fn adapter_identity_and_configure() {
        let mut a = BuildAdapter { tracker: Tracker::new(Fake::default(), default_watch()), last: Instant::now() };
        assert_eq!((a.id(), a.display_name()), ("build", "Terminal builds"));
        a.configure(&serde_json::json!({ "watch": [] }));
        a.tracker.source.spawn(1, None, "cargo", "cargo build");
        assert_eq!(a.poll(), None);
    }
}
