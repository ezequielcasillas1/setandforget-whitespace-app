//! Hook-driven adapters (card 03 §3–§4). `HookTracker` is pure and runs on a `SignalSource`, so it is
//! unit-tested on any OS. The real source tails `%LOCALAPPDATA%\Whitespace\signals\<tool>.jsonl`.
use super::{Adapter, ToolState};
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ev { Working, NeedsYou, Done }

/// Event table per tool: the tool's own event name → what it means.
pub fn table(tool: &str, event: &str) -> Option<Ev> {
    use Ev::*;
    Some(match (tool, event) {
        ("claude", "UserPromptSubmit") => Working,
        ("claude", "PermissionRequest" | "Notification" | "StopFailure") => NeedsYou,
        ("claude", "Stop" | "SessionEnd") => Done,
        ("codex", "UserPromptSubmit") => Working,
        ("codex", "PermissionRequest") => NeedsYou,
        ("codex", "Stop" | "Interrupt" | "SessionEnd") => Done,
        ("copilot", "userPromptSubmitted") => Working,
        ("copilot", "notification" | "errorOccurred") => NeedsYou,
        ("copilot", "agentStop" | "sessionEnd") => Done,
        ("cursor", "beforeSubmitPrompt") => Working,
        ("cursor", "stop:error") => NeedsYou,
        ("cursor", "stop" | "sessionEnd") => Done,
        ("windsurf", "pre_user_prompt") => Working,
        ("windsurf", "post_cascade_response") => Done,
        _ => return None,
    })
}

pub trait SignalSource: Send {
    /// New complete lines since the last call.
    fn read_new(&mut self) -> Vec<String>;
}

#[derive(Default)]
pub struct HookTracker { working: HashMap<String, ()>, needs: bool, was_working: bool, pub seen_any: bool }

impl HookTracker {
    /// Feed lines for `tool`; returns what the adapter reports this tick. `process_gone` drops all sessions.
    pub fn step(&mut self, tool: &str, lines: &[String], process_gone: bool) -> Option<ToolState> {
        for l in lines {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(l) else { continue };
            let (Some(ev), session) = (v["event"].as_str(), v["session"].as_str().unwrap_or("").to_string()) else { continue };
            self.seen_any = true;
            match table(tool, ev) {
                Some(Ev::Working) => { self.working.insert(session, ()); }
                Some(Ev::NeedsYou) => { self.working.remove(&session); self.needs = true; }
                Some(Ev::Done) => { self.working.remove(&session); }
                None => {}
            }
        }
        if process_gone { self.working.clear(); }
        if self.needs { self.needs = false; self.was_working = false; self.working.clear(); return Some(ToolState::NeedsYou); }
        if !self.working.is_empty() { self.was_working = true; return Some(ToolState::Working); }
        if std::mem::take(&mut self.was_working) { return Some(ToolState::Done); }
        None
    }
    pub fn reset(&mut self) { self.working.clear(); self.needs = false; self.was_working = false; }
}

/// Tails one jsonl file. Truncates it at startup and above 1 MB (card §3).
pub struct FileSource { path: PathBuf, pos: u64, partial: String }
impl FileSource {
    pub fn new(path: PathBuf) -> Self {
        if let Some(d) = path.parent() { let _ = std::fs::create_dir_all(d); }
        let _ = std::fs::write(&path, b"");
        Self { path, pos: 0, partial: String::new() }
    }
}
impl SignalSource for FileSource {
    fn read_new(&mut self) -> Vec<String> {
        let Ok(mut f) = std::fs::File::open(&self.path) else { return vec![] };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.pos { self.pos = 0; self.partial.clear(); }
        if len == self.pos { return vec![]; }
        let mut buf = String::new();
        if f.seek(SeekFrom::Start(self.pos)).is_err() || f.read_to_string(&mut buf).is_err() { return vec![]; }
        self.pos += buf.len() as u64;
        self.partial.push_str(&buf);
        let mut out: Vec<String> = self.partial.split('\n').map(str::to_string).collect();
        self.partial = out.pop().unwrap_or_default();
        if self.pos > 1 << 20 && self.partial.is_empty() { let _ = std::fs::write(&self.path, b""); self.pos = 0; }
        out.into_iter().filter(|l| !l.trim().is_empty()).collect()
    }
}

pub struct HookAdapter { id: &'static str, name: &'static str, src: Box<dyn SignalSource>, t: HookTracker, last: Instant }
impl HookAdapter {
    pub fn new(id: &'static str, name: &'static str, src: Box<dyn SignalSource>) -> Self { Self { id, name, src, t: HookTracker::default(), last: Instant::now() } }
    pub fn real(id: &'static str, name: &'static str) -> Self { Self::new(id, name, Box::new(FileSource::new(crate::signal::signals_dir().join(format!("{id}.jsonl"))))) }
}
impl Adapter for HookAdapter {
    fn id(&self) -> &'static str { self.id }
    fn display_name(&self) -> &'static str { self.name }
    // Process gates wait for spike S3 to confirm exe names; the 20 min cap is the backstop (card §3).
    fn is_target_running(&self) -> bool { false }
    fn poll(&mut self) -> Option<ToolState> {
        let lines = self.src.read_new();
        if !lines.is_empty() {
            self.last = Instant::now();
            if self.id == "codex" { let _ = std::fs::write(crate::signal::signals_dir().join("codex.seen"), b"1"); }
        }
        self.t.step(self.id, &lines, false)
    }
    fn last_signal_at(&self) -> Instant { self.last }
    fn reset(&mut self) { let _ = self.src.read_new(); self.t.reset(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ToolState::*;
    fn l(ev: &str, s: &str) -> String { format!(r#"{{"t":1,"tool":"x","event":"{ev}","session":"{s}"}}"#) }
    fn step(t: &mut HookTracker, tool: &str, ls: &[String]) -> Option<ToolState> { t.step(tool, ls, false) }

    #[test] fn claude_work_then_done() {
        let mut t = HookTracker::default();
        assert_eq!(step(&mut t, "claude", &[l("UserPromptSubmit", "a")]), Some(Working));
        assert_eq!(step(&mut t, "claude", &[]), Some(Working), "heartbeat keeps the quiet timer fed");
        assert_eq!(step(&mut t, "claude", &[l("Stop", "a")]), Some(Done));
        assert_eq!(step(&mut t, "claude", &[]), None);
    }
    #[test] fn two_sessions_done_only_when_both_stop() {
        let mut t = HookTracker::default();
        step(&mut t, "codex", &[l("UserPromptSubmit", "a"), l("UserPromptSubmit", "b")]);
        assert_eq!(step(&mut t, "codex", &[l("Stop", "a")]), Some(Working));
        assert_eq!(step(&mut t, "codex", &[l("Stop", "b")]), Some(Done));
    }
    #[test] fn needs_you_once_then_waits_for_new_work() {
        let mut t = HookTracker::default();
        step(&mut t, "copilot", &[l("userPromptSubmitted", "a")]);
        assert_eq!(step(&mut t, "copilot", &[l("notification", "a")]), Some(NeedsYou));
        assert_eq!(step(&mut t, "copilot", &[]), None);
        assert_eq!(step(&mut t, "copilot", &[l("userPromptSubmitted", "a")]), Some(Working));
    }
    #[test] fn cursor_error_is_needs_you_and_windsurf_has_none() {
        let mut t = HookTracker::default();
        step(&mut t, "cursor", &[l("beforeSubmitPrompt", "a")]);
        assert_eq!(step(&mut t, "cursor", &[l("stop:error", "a")]), Some(NeedsYou));
        let mut w = HookTracker::default();
        assert_eq!(step(&mut w, "windsurf", &[l("pre_user_prompt", "")]), Some(Working));
        assert_eq!(step(&mut w, "windsurf", &[l("post_cascade_response", "")]), Some(Done));
    }
    #[test] fn process_gone_drops_to_done() {
        let mut t = HookTracker::default();
        step(&mut t, "claude", &[l("UserPromptSubmit", "a")]);
        assert_eq!(t.step("claude", &[], true), Some(Done));
    }
    #[test] fn bad_lines_and_other_tools_events_are_skipped() {
        let mut t = HookTracker::default();
        assert_eq!(step(&mut t, "claude", &["{not json".into(), l("beforeSubmitPrompt", "a"), "".into()]), None);
    }
    #[test] fn reset_drops_work() {
        let mut t = HookTracker::default();
        step(&mut t, "claude", &[l("UserPromptSubmit", "a")]);
        t.reset();
        assert_eq!(step(&mut t, "claude", &[]), None);
    }
    #[test] fn file_source_tails_handles_partial_lines_and_truncation() {
        let p = std::env::temp_dir().join(format!("ws-hooks-{}.jsonl", std::process::id()));
        let mut s = FileSource::new(p.clone());
        assert!(s.read_new().is_empty());
        std::fs::write(&p, "a\nb").unwrap();
        assert_eq!(s.read_new(), vec!["a".to_string()]);
        std::fs::write(&p, "a\nb\nc\n").unwrap();
        assert_eq!(s.read_new(), vec!["b".to_string(), "c".to_string()]);
        std::fs::write(&p, "z\n").unwrap();
        assert_eq!(s.read_new(), vec!["z".to_string()]);
        let _ = std::fs::remove_file(p);
    }
    #[test] fn signal_line_keeps_only_ids() {
        let v: serde_json::Value = serde_json::from_str(&crate::signal::line("claude", "Stop", r#"{"session_id":"s1","prompt":"secret"}"#, 5)).unwrap();
        assert_eq!(v, serde_json::json!({"t":5,"tool":"claude","event":"Stop","session":"s1"}));
        let c: serde_json::Value = serde_json::from_str(&crate::signal::line("cursor", "stop", r#"{"conversation_id":"c","status":"error"}"#, 1)).unwrap();
        assert_eq!(c["event"], "stop:error");
        let e: serde_json::Value = serde_json::from_str(&crate::signal::line("codex", "Stop", "", 1)).unwrap();
        assert_eq!(e["session"], "");
    }
    #[test] fn signal_record_creates_folder_and_ignores_unknown_tools() {
        let d = std::env::temp_dir().join(format!("ws-sig-{}", std::process::id()));
        crate::signal::record(&d, "claude", "Stop", "", 1).unwrap();
        crate::signal::record(&d, "evil/../x", "Stop", "", 1).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("claude.jsonl")).unwrap().lines().count(), 1);
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(d);
    }
}
