//! `ws-signal.exe <tool> <event>`: run by an AI tool's hook. Appends one line, prints `{}`, exits 0.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[path = "../signal.rs"]
mod signal;
use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut input = String::new();
    let _ = std::io::stdin().take(1 << 20).read_to_string(&mut input);
    if args.len() >= 3 {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let _ = signal::record(&signal::signals_dir(), &args[1], &args[2], &input, t);
    }
    println!("{{}}");
}
