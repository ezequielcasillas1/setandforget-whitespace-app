//! One-click hook install/remove (card 03 §3, UX B1/B2). Edits only the tool's user-level hook file,
//! backs it up first, never touches other entries, and leaves the file unchanged on any error.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const TOOLS: [&str; 5] = ["claude", "codex", "copilot", "cursor", "windsurf"];
const MARK: &str = "ws-signal";

fn home() -> PathBuf { std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default() }

/// The tool's own folder (its presence = "installed on this PC") and its user-level hook file.
pub fn paths(tool: &str, home: &Path) -> Option<(PathBuf, PathBuf)> {
    let (dir, file) = match tool {
        "claude" => (home.join(".claude"), "settings.json"),
        "codex" => (home.join(".codex"), "hooks.json"),
        "copilot" => (home.join(".copilot"), "hooks/whitespace.json"),
        "cursor" => (home.join(".cursor"), "hooks.json"),
        "windsurf" => (home.join(".codeium").join("windsurf"), "hooks.json"),
        _ => return None,
    };
    let f = dir.join(file);
    Some((dir, f))
}

fn events(tool: &str) -> &'static [&'static str] {
    match tool {
        "claude" => &["UserPromptSubmit", "PermissionRequest", "Notification", "Stop", "StopFailure", "SessionEnd"],
        "codex" => &["UserPromptSubmit", "PermissionRequest", "Stop", "Interrupt", "SessionEnd"],
        "copilot" => &["userPromptSubmitted", "notification", "errorOccurred", "agentStop", "sessionEnd"],
        "cursor" => &["beforeSubmitPrompt", "stop", "sessionEnd"],
        "windsurf" => &["pre_user_prompt", "post_cascade_response"],
        _ => &[],
    }
}

fn entry(tool: &str, ev: &str, exe: &Path) -> Value {
    let exe = exe.display().to_string();
    let cmd = format!("\"{exe}\" {tool} {ev}");
    match tool {
        "claude" | "codex" => {
            let mut g = json!({ "hooks": [{ "type": "command", "command": cmd, "timeout": 5 }] });
            if ev == "Notification" { g["matcher"] = json!("permission_prompt|elicitation_dialog"); }
            g
        }
        "copilot" => json!({ "type": "command", "powershell": format!("& '{exe}' {tool} {ev}") }),
        _ => json!({ "command": cmd }),
    }
}

fn read(file: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(file) {
        Ok(t) if t.trim().is_empty() => Ok(json!({})),
        Ok(t) => serde_json::from_str::<Value>(&t).ok().filter(Value::is_object).ok_or_else(|| "not valid JSON".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.to_string()),
    }
}

fn strip(v: &mut Value) {
    if let Some(h) = v.get_mut("hooks").and_then(Value::as_object_mut) {
        for arr in h.values_mut() {
            if let Some(a) = arr.as_array_mut() { a.retain(|e| !e.to_string().contains(MARK)); }
        }
        h.retain(|_, arr| arr.as_array().map_or(true, |a| !a.is_empty()));
    }
}

fn write(file: &Path, v: &Value) -> Result<(), String> {
    if file.exists() {
        let bak = file.with_extension("json.whitespace-backup");
        std::fs::copy(file, &bak).map_err(|e| e.to_string())?;
    }
    if let Some(d) = file.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    let tmp = file.with_extension("json.whitespace-tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(v).unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, file).map_err(|e| { let _ = std::fs::remove_file(&tmp); e.to_string() })
}

pub fn is_installed(file: &Path) -> bool { std::fs::read_to_string(file).map(|t| t.contains(MARK)).unwrap_or(false) }

pub fn install_at(tool: &str, home: &Path, exe: &Path) -> Result<(), String> {
    let (dir, file) = paths(tool, home).ok_or("unknown tool")?;
    if !dir.is_dir() { return Err("tool not found".into()); }
    let mut v = read(&file)?;
    strip(&mut v);
    if tool == "cursor" || tool == "copilot" { v.as_object_mut().unwrap().entry("version").or_insert(json!(1)); }
    let h = v.as_object_mut().unwrap().entry("hooks").or_insert(json!({}));
    let h = h.as_object_mut().ok_or("hooks is not an object")?;
    for ev in events(tool) {
        let arr = h.entry(ev.to_string()).or_insert(json!([]));
        arr.as_array_mut().ok_or("hook list is not an array")?.push(entry(tool, ev, exe));
    }
    write(&file, &v)
}

pub fn remove_at(tool: &str, home: &Path) -> Result<(), String> {
    let (_, file) = paths(tool, home).ok_or("unknown tool")?;
    if !is_installed(&file) { return Ok(()); }
    if tool == "copilot" { return std::fs::remove_file(&file).map_err(|e| e.to_string()); } // our own file
    let mut v = read(&file)?;
    strip(&mut v);
    write(&file, &v)
}

pub fn status_at(tool: &str, home: &Path) -> Value {
    let Some((dir, file)) = paths(tool, home) else { return json!(null) };
    let trust_pending = tool == "codex" && !crate::signal::signals_dir().join("codex.seen").exists();
    json!({ "id": tool, "found": dir.is_dir(), "installed": is_installed(&file), "trustPending": trust_pending })
}

pub fn exe() -> PathBuf {
    let name = if cfg!(windows) { "ws-signal.exe" } else { "ws-signal" };
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(name))).unwrap_or_else(|| PathBuf::from(name))
}
pub fn install(tool: &str) -> Result<(), String> { install_at(tool, &home(), &exe()) }
pub fn remove(tool: &str) -> Result<(), String> { remove_at(tool, &home()) }
pub fn status_all() -> Vec<Value> { TOOLS.iter().map(|t| status_at(t, &home())).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(n: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("ws-hi-{n}-{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); d }
    const EXE: &str = r"C:\Program Files\Whitespace\ws-signal.exe";

    #[test] fn claude_keeps_other_entries_backs_up_is_idempotent_and_removes_cleanly() {
        let h = tmp("claude"); std::fs::create_dir_all(h.join(".claude")).unwrap();
        let f = h.join(".claude/settings.json");
        let orig = r#"{"model":"x","hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#;
        std::fs::write(&f, orig).unwrap();
        install_at("claude", &h, Path::new(EXE)).unwrap();
        install_at("claude", &h, Path::new(EXE)).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["model"], "x");
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2, "theirs + ours, once");
        assert_eq!(v["hooks"]["Notification"][0]["matcher"], "permission_prompt|elicitation_dialog");
        assert!(is_installed(&f));
        assert!(h.join(".claude/settings.json.whitespace-backup").exists());
        remove_at("claude", &h).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v, serde_json::from_str::<Value>(orig).unwrap());
    }
    #[test] fn bad_json_leaves_file_untouched() {
        let h = tmp("bad"); std::fs::create_dir_all(h.join(".cursor")).unwrap();
        let f = h.join(".cursor/hooks.json"); std::fs::write(&f, "{oops").unwrap();
        assert!(install_at("cursor", &h, Path::new(EXE)).is_err());
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "{oops");
    }
    #[test] fn missing_tool_is_an_error_and_writes_nothing() {
        let h = tmp("none"); std::fs::create_dir_all(&h).unwrap();
        assert!(install_at("windsurf", &h, Path::new(EXE)).is_err());
        assert!(!h.join(".codeium").exists());
        assert_eq!(status_at("windsurf", &h)["found"], false);
    }
    #[test] fn copilot_uses_its_own_file_with_powershell() {
        let h = tmp("cop"); std::fs::create_dir_all(h.join(".copilot")).unwrap();
        install_at("copilot", &h, Path::new(EXE)).unwrap();
        let f = h.join(".copilot/hooks/whitespace.json");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["version"], 1);
        assert!(v["hooks"]["agentStop"][0]["powershell"].as_str().unwrap().starts_with("& '"));
        assert_eq!(status_at("copilot", &h)["installed"], true);
        remove_at("copilot", &h).unwrap();
        assert!(!f.exists());
    }
    #[test] fn cursor_and_windsurf_shapes() {
        let h = tmp("cw");
        std::fs::create_dir_all(h.join(".cursor")).unwrap(); std::fs::create_dir_all(h.join(".codeium/windsurf")).unwrap();
        install_at("cursor", &h, Path::new(EXE)).unwrap(); install_at("windsurf", &h, Path::new(EXE)).unwrap();
        let c: Value = serde_json::from_str(&std::fs::read_to_string(h.join(".cursor/hooks.json")).unwrap()).unwrap();
        assert_eq!(c["version"], 1);
        assert_eq!(c["hooks"]["stop"][0]["command"], format!("\"{EXE}\" cursor stop"));
        let w: Value = serde_json::from_str(&std::fs::read_to_string(h.join(".codeium/windsurf/hooks.json")).unwrap()).unwrap();
        assert_eq!(w["hooks"].as_object().unwrap().len(), 2);
    }
}
