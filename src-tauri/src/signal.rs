//! Hook bridge core (card 03 §3), shared by `ws-signal.exe` and the app's tests.
//! Keeps only the event name, a session id, and (Cursor `stop` only) the status word. Never reads prompts or code.
use std::io::Write;
use std::path::{Path, PathBuf};

/// `%LOCALAPPDATA%\Whitespace\signals` (temp dir elsewhere, for tests and Linux).
pub fn signals_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Whitespace").join("signals")
}

pub fn is_known_tool(t: &str) -> bool { matches!(t, "claude" | "codex" | "copilot" | "cursor" | "windsurf") }

/// Builds the one JSON line for a hook call. `stdin` may be empty or not JSON.
pub fn line(tool: &str, event: &str, stdin: &str, t_ms: u128) -> String {
    let v: serde_json::Value = serde_json::from_str(stdin.trim()).unwrap_or(serde_json::Value::Null);
    let session = ["session_id", "sessionId", "conversation_id", "trajectory_id", "execution_id"].iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()).map(str::to_string)).unwrap_or_default();
    let mut ev = event.to_string();
    if tool == "cursor" && event == "stop" && v.get("status").and_then(|s| s.as_str()) == Some("error") { ev = "stop:error".into(); }
    serde_json::json!({ "t": t_ms as u64, "tool": tool, "event": ev, "session": session }).to_string()
}

/// Appends one line. Errors are returned but the bridge ignores them (it must never fail the tool's hook).
pub fn record(dir: &Path, tool: &str, event: &str, stdin: &str, t_ms: u128) -> std::io::Result<()> {
    if !is_known_tool(tool) || event.is_empty() { return Ok(()); }
    std::fs::create_dir_all(dir)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(format!("{tool}.jsonl")))?;
    writeln!(f, "{}", line(tool, event, stdin, t_ms))
}
