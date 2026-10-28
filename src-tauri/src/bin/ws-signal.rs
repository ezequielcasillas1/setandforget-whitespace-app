//! `ws-signal.exe <tool> <event>`: run by an AI tool's hook. Appends one line, prints a "go ahead" reply, exits 0.
//! It never waits on the tool: stdin is read for at most ~1 s, then we answer and exit even if the pipe is still open.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[path = "../signal.rs"]
mod signal;
use std::io::{Read, Write};
use std::time::Duration;

const STDIN_WAIT: Duration = Duration::from_millis(1000);

fn read_stdin_bounded() -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut input = String::new();
        let _ = std::io::stdin().take(1 << 20).read_to_string(&mut input);
        let _ = tx.send(input);
    });
    rx.recv_timeout(STDIN_WAIT).unwrap_or_default()
}

/// Reply the tool expects. Cursor's beforeSubmitPrompt wants an explicit continue.
fn reply(tool: &str, event: &str) -> &'static str {
    match (tool, event) {
        ("cursor", "beforeSubmitPrompt") => "{\"continue\":true}",
        _ => "{}",
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = read_stdin_bounded();
    let (tool, event) = (args.get(1).map(String::as_str).unwrap_or(""), args.get(2).map(String::as_str).unwrap_or(""));
    if args.len() >= 3 {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let _ = signal::record(&signal::signals_dir(), tool, event, &input, t);
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", reply(tool, event));
    let _ = out.flush();
    // Exit hard so a stdin reader still blocked on an open pipe can't keep the process alive.
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn cursor_prompt_gets_continue() {
        assert_eq!(reply("cursor", "beforeSubmitPrompt"), "{\"continue\":true}");
        assert_eq!(reply("cursor", "stop"), "{}");
        assert_eq!(reply("claude", "UserPromptSubmit"), "{}");
    }
}
