//! Back out of the whiteout when a dev tool is showing a button the user has to execute
//! (Accept, Run, Start, Allow, and the same kind of control). A lone toolbar Run does not count:
//! Run / Start / Continue only count when a sibling is the other choice (Skip, Reject, Cancel).

use std::sync::atomic::{AtomicBool, Ordering};

const SOLO: &[&str] = &[
    "accept", "accept all", "accept file", "run command", "run anyway", "allow", "allow once",
    "allow for this chat", "allow for this session", "approve", "apply", "confirm", "resume",
    "retry", "keep", "keep all", "keep file", "submit",
];

const PAIRED: &[(&str, &[&str])] = &[
    ("run", &["skip", "reject", "cancel", "don't run", "deny"]),
    ("start", &["skip", "cancel", "reject", "deny"]),
    ("continue", &["skip", "cancel", "stop", "reject"]),
];

fn norm(s: &str) -> String { s.trim().to_lowercase() }

fn solo(label: &str) -> bool {
    if SOLO.contains(&label) { return true; }
    let words: Vec<&str> = label.split_whitespace().collect();
    matches!(words.first().copied(), Some("accept")) && words.len() <= 4
}

/// `labels` are the button names that share one parent. True when one of them is waiting on the user.
pub fn prompt_from_labels(labels: &[impl AsRef<str>]) -> bool {
    let names: Vec<String> = labels.iter().map(|s| norm(s.as_ref())).filter(|s| !s.is_empty()).collect();
    if names.iter().any(|n| solo(n)) { return true; }
    PAIRED.iter().any(|(exec, alts)| names.iter().any(|n| n == exec) && alts.iter().any(|a| names.iter().any(|n| n == a)))
}

static PROMPT: AtomicBool = AtomicBool::new(false);

pub fn executable_prompt_visible() -> bool { PROMPT.load(Ordering::Relaxed) }

pub fn start_watcher() {
    #[cfg(windows)]
    std::thread::spawn(|| {
        let auto = init_automation();
        loop {
            let hit = auto.as_ref().is_some_and(scan);
            PROMPT.store(hit, Ordering::Relaxed);
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
    });
}

#[cfg(windows)]
fn init_automation() -> Option<windows::Win32::UI::Accessibility::IUIAutomation> {
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None::<&windows::core::IUnknown>, CLSCTX_INPROC_SERVER).ok()
    }
}

#[cfg(windows)]
fn scan(auto: &windows::Win32::UI::Accessibility::IUIAutomation) -> bool {
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::System::Variant::{VARIANT, VARIANT_0_0_0, VT_I4};
    use windows::Win32::UI::Accessibility::{IUIAutomationElement, IUIAutomationTreeWalker, TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_ControlTypePropertyId};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};
    use windows::core::BOOL;

    unsafe fn i4(n: i32) -> VARIANT {
        let mut v: VARIANT = core::mem::zeroed();
        let slot = &mut *v.Anonymous.Anonymous;
        core::ptr::write(&mut slot.vt, VT_I4);
        core::ptr::write(&mut slot.Anonymous, VARIANT_0_0_0 { lVal: n });
        v
    }

    fn exe_name(pid: u32) -> Option<String> {
        const TOOLS: &[&str] = &["cursor.exe", "windsurf.exe", "code.exe", "claude.exe", "codex.exe"];
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 520];
            let mut n = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), windows::core::PWSTR(buf.as_mut_ptr()), &mut n);
            let _ = CloseHandle(h);
            ok.ok()?;
            let path = String::from_utf16_lossy(&buf[..n as usize]);
            let name = path.rsplit(['\\', '/']).next().unwrap_or("").to_lowercase();
            TOOLS.contains(&name.as_str()).then_some(name)
        }
    }

    unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let list = &mut *(lparam.0 as *mut Vec<HWND>);
        if IsWindowVisible(hwnd).as_bool() { list.push(hwnd); }
        BOOL(1)
    }

    fn sibling_labels(walker: &IUIAutomationTreeWalker, button: &IUIAutomationElement) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let Ok(parent) = walker.GetParentElement(button) else { return out };
            let mut child = walker.GetFirstChildElement(&parent).ok();
            let mut n = 0;
            while let Some(c) = child {
                if n > 24 { break; }
                n += 1;
                if c.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false)
                    && !c.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(true)
                {
                    if let Ok(name) = c.CurrentName() {
                        let s = name.to_string();
                        if !s.trim().is_empty() { out.push(s); }
                    }
                }
                child = walker.GetNextSiblingElement(&c).ok();
            }
        }
        out
    }

    let mut hwnds = Vec::new();
    unsafe {
        if EnumWindows(Some(enum_cb), LPARAM(&mut hwnds as *mut Vec<HWND> as isize)).is_err() { return false; }
        let me = GetCurrentProcessId();
        let Ok(cond) = auto.CreatePropertyCondition(UIA_ControlTypePropertyId, &i4(UIA_ButtonControlTypeId.0)) else { return false };
        let Ok(walker) = auto.ControlViewWalker() else { return false };
        let mut seen = 0;
        for hwnd in hwnds {
            if seen >= 6 { break; }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 || pid == me || exe_name(pid).is_none() { continue; }
            seen += 1;
            let Ok(root) = auto.ElementFromHandle(hwnd) else { continue };
            let Ok(found) = root.FindAll(TreeScope_Descendants, &cond) else { continue };
            let count = found.Length().unwrap_or(0).min(48);
            for i in 0..count {
                let Ok(el) = found.GetElement(i) else { continue };
                if !el.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false) { continue; }
                if el.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(true) { continue; }
                let labels = sibling_labels(&walker, &el);
                if prompt_from_labels(&labels) { return true; }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::prompt_from_labels;

    #[test] fn accept_run_and_start_back_out() {
        assert!(prompt_from_labels(&["Accept"]));
        assert!(prompt_from_labels(&["Accept all changes"]));
        assert!(prompt_from_labels(&["Allow once"]));
        assert!(prompt_from_labels(&["Run", "Skip"]));
        assert!(prompt_from_labels(&["Start", "Cancel"]));
        assert!(prompt_from_labels(&["Continue", "Reject"]));
    }
    #[test] fn a_lone_toolbar_run_does_not() {
        assert!(!prompt_from_labels(&["Run"]));
        assert!(!prompt_from_labels(&["Start"]));
        assert!(!prompt_from_labels(&["Continue"]));
        assert!(!prompt_from_labels(&["File", "Edit", "Run"]));
        assert!(!prompt_from_labels(&[] as &[&str]));
    }
}
