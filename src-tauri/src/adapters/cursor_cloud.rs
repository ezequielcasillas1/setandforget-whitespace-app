//! Cursor cloud agents (card 03c). Polls Cursor's Cloud Agents API about every 15 s on its own thread
//! and reports Working while any agent is ACTIVE. Errors never white out. The key lives only in the
//! Windows Credential Manager; it is never logged, written to settings, or sent to the frontend.
use super::{Adapter, ToolState};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const API: &str = "https://api.cursor.com";
#[cfg_attr(not(windows), allow(dead_code))]
const SERVICE: &str = "dev.cheke.ai-overlay";
#[cfg_attr(not(windows), allow(dead_code))]
const USER: &str = "cursor-api";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome { Active, Idle, Rejected, Unreachable, RateLimited }

/// Status codes Settings maps to the UX packet's wording.
pub fn status_code(enabled: bool, has_key: bool, last: Option<Outcome>) -> &'static str {
    match (enabled, has_key, last) {
        (false, _, _) => "off",
        (_, false, _) => "no-key",
        (_, _, Some(Outcome::Active)) => "working",
        (_, _, Some(Outcome::Rejected)) => "rejected",
        (_, _, Some(Outcome::Unreachable | Outcome::RateLimited)) => "unreachable",
        _ => "watching",
    }
}

pub fn classify(code: u16, body: &str) -> Outcome {
    match code {
        200..=299 => {
            let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
            let any = v["agents"].as_array().map_or(false, |a| a.iter().any(|x| x["status"] == "ACTIVE"));
            if any { Outcome::Active } else { Outcome::Idle }
        }
        401 | 403 => Outcome::Rejected,
        429 => Outcome::RateLimited,
        _ => Outcome::Unreachable,
    }
}

pub fn fetch(base: &str, key: &str) -> Outcome {
    let Ok(c) = reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build() else { return Outcome::Unreachable };
    match c.get(format!("{base}/v1/agents?limit=100&includeArchived=false")).basic_auth(key, Some("")).send() {
        Ok(r) => { let code = r.status().as_u16(); classify(code, &r.text().unwrap_or_default()) }
        Err(_) => Outcome::Unreachable,
    }
}

/// Seconds to wait before the next check: 15 normally, doubling to 60 on errors.
pub fn next_delay(o: Outcome, prev: u64) -> u64 {
    match o { Outcome::Active | Outcome::Idle => 15, _ => (prev.max(15) * 2).min(60) }
}

// ---- key storage ----
#[cfg(windows)]
mod store {
    fn entry() -> Result<keyring::Entry, String> { keyring::Entry::new(super::SERVICE, super::USER).map_err(|_| "Couldn't open Credential Manager".to_string()) }
    pub fn get() -> Option<String> { entry().ok()?.get_password().ok().filter(|k| !k.is_empty()) }
    pub fn set(k: &str) -> Result<(), String> { entry()?.set_password(k).map_err(|_| "Couldn't save the key".into()) }
    pub fn clear() -> Result<(), String> { match entry()?.delete_credential() { Ok(_) | Err(keyring::Error::NoEntry) => Ok(()), Err(_) => Err("Couldn't remove the key".into()) } }
}
#[cfg(not(windows))]
mod store {
    // Non-Windows (tests/dev): memory only, never written to disk.
    use std::sync::Mutex;
    static K: Mutex<Option<String>> = Mutex::new(None);
    pub fn get() -> Option<String> { K.lock().unwrap().clone() }
    pub fn set(k: &str) -> Result<(), String> { *K.lock().unwrap() = Some(k.to_string()); Ok(()) }
    pub fn clear() -> Result<(), String> { *K.lock().unwrap() = None; Ok(()) }
}

#[derive(Default)]
pub struct Shared { pub enabled: bool, pub has_key: bool, pub last: Option<Outcome>, pub wake: bool }
pub type Handle = Arc<Mutex<Shared>>;

pub fn set_key(h: &Handle, key: &str) -> Result<(), String> {
    let k = key.trim();
    if k.is_empty() { return Err("Paste a key first".into()); }
    store::set(k)?;
    let mut s = h.lock().unwrap(); s.has_key = true; s.last = None; s.wake = true; Ok(())
}
pub fn clear_key(h: &Handle) -> Result<(), String> {
    store::clear()?;
    let mut s = h.lock().unwrap(); s.has_key = false; s.last = None; Ok(())
}
pub fn status(h: &Handle) -> serde_json::Value {
    let s = h.lock().unwrap();
    serde_json::json!({ "present": s.has_key, "status": status_code(s.enabled, s.has_key, s.last) })
}

pub struct CursorCloudAdapter { h: Handle, was_active: bool, last: Instant }

impl CursorCloudAdapter {
    /// Starts the polling thread against `base` (the real API in the app; a mock in tests).
    pub fn start(base: String) -> (Self, Handle) {
        let h: Handle = Arc::new(Mutex::new(Shared { has_key: store::get().is_some(), ..Default::default() }));
        let th = h.clone();
        std::thread::spawn(move || {
            let mut delay = 15u64;
            let mut next = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let (enabled, wake) = { let mut s = th.lock().unwrap(); (s.enabled, std::mem::take(&mut s.wake)) };
                if !enabled { th.lock().unwrap().last = None; next = Instant::now(); continue; }
                if !wake && Instant::now() < next { continue; }
                let Some(key) = store::get() else { let mut s = th.lock().unwrap(); s.has_key = false; s.last = None; next = Instant::now() + Duration::from_secs(5); continue };
                let o = fetch(&base, &key);
                drop(key);
                { let mut s = th.lock().unwrap(); s.has_key = true; if s.enabled { s.last = Some(o); } }
                delay = next_delay(o, delay);
                next = Instant::now() + Duration::from_secs(delay);
            }
        });
        (Self { h: h.clone(), was_active: false, last: Instant::now() }, h)
    }
}

impl Adapter for CursorCloudAdapter {
    fn id(&self) -> &'static str { "cursorCloud" }
    fn display_name(&self) -> &'static str { "Cursor cloud agents" }
    fn is_target_running(&self) -> bool { false }
    fn poll(&mut self) -> Option<ToolState> {
        let active = self.h.lock().unwrap().last == Some(Outcome::Active);
        if active { self.last = Instant::now(); self.was_active = true; return Some(ToolState::Working); }
        if std::mem::take(&mut self.was_active) { return Some(ToolState::Done); }
        None
    }
    fn last_signal_at(&self) -> Instant { self.last }
    fn configure(&mut self, settings: &serde_json::Value) {
        let on = settings["adapters"]["cursorCloud"] == serde_json::json!(true);
        let mut s = self.h.lock().unwrap();
        if on && !s.enabled { s.wake = true; }
        s.enabled = on;
    }
    fn reset(&mut self) { self.was_active = false; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn mock(code: u16, body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(vec![]));
        let s2 = seen.clone();
        std::thread::spawn(move || for c in l.incoming() {
            let Ok(mut c) = c else { continue };
            let mut b = [0u8; 4096]; let n = c.read(&mut b).unwrap_or(0);
            s2.lock().unwrap().push(String::from_utf8_lossy(&b[..n]).to_string());
            let _ = write!(c, "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        });
        (base, seen)
    }

    #[test] fn classify_cases() {
        assert_eq!(classify(200, r#"{"agents":[{"status":"FINISHED"},{"status":"ACTIVE"}]}"#), Outcome::Active);
        assert_eq!(classify(200, r#"{"agents":[{"status":"FINISHED"}]}"#), Outcome::Idle);
        assert_eq!(classify(200, "garbage"), Outcome::Idle);
        assert_eq!(classify(401, ""), Outcome::Rejected);
        assert_eq!(classify(429, ""), Outcome::RateLimited);
        assert_eq!(classify(503, ""), Outcome::Unreachable);
    }
    #[test] fn fetch_uses_basic_auth_and_the_list_endpoint() {
        let (base, seen) = mock(200, r#"{"agents":[{"status":"ACTIVE"}]}"#);
        assert_eq!(fetch(&base, "k123"), Outcome::Active);
        let req = seen.lock().unwrap()[0].clone();
        assert!(req.starts_with("GET /v1/agents?limit=100&includeArchived=false"));
        assert!(req.contains("Basic azEyMzo=")); // base64("k123:")
    }
    #[test] fn rejected_and_unreachable_never_white_out() {
        let (base, _) = mock(401, "");
        assert_eq!(fetch(&base, "bad"), Outcome::Rejected);
        assert_eq!(fetch("http://127.0.0.1:1", "k"), Outcome::Unreachable);
        let h: Handle = Arc::new(Mutex::new(Shared { enabled: true, has_key: true, last: Some(Outcome::Rejected), wake: false }));
        let mut a = CursorCloudAdapter { h, was_active: false, last: Instant::now() };
        assert_eq!(a.poll(), None);
    }
    #[test] fn adapter_working_then_done() {
        let h: Handle = Arc::new(Mutex::new(Shared { enabled: true, has_key: true, last: Some(Outcome::Active), wake: false }));
        let mut a = CursorCloudAdapter { h: h.clone(), was_active: false, last: Instant::now() };
        assert_eq!(a.poll(), Some(ToolState::Working));
        h.lock().unwrap().last = Some(Outcome::Idle);
        assert_eq!(a.poll(), Some(ToolState::Done));
        assert_eq!(a.poll(), None);
    }
    #[test] fn status_codes_and_backoff() {
        assert_eq!(status_code(false, true, None), "off");
        assert_eq!(status_code(true, false, None), "no-key");
        assert_eq!(status_code(true, true, None), "watching");
        assert_eq!(status_code(true, true, Some(Outcome::Idle)), "watching");
        assert_eq!(status_code(true, true, Some(Outcome::Active)), "working");
        assert_eq!(status_code(true, true, Some(Outcome::Rejected)), "rejected");
        assert_eq!(status_code(true, true, Some(Outcome::Unreachable)), "unreachable");
        assert_eq!(next_delay(Outcome::Unreachable, 15), 30);
        assert_eq!(next_delay(Outcome::RateLimited, 60), 60);
        assert_eq!(next_delay(Outcome::Idle, 60), 15);
    }
    #[test] fn key_roundtrip_and_status_never_contains_it() {
        let h: Handle = Arc::new(Mutex::new(Shared { enabled: true, ..Default::default() }));
        assert!(set_key(&h, "   ").is_err());
        set_key(&h, " secret-key-xyz ").unwrap();
        let st = status(&h).to_string();
        assert!(!st.contains("secret-key-xyz"));
        assert_eq!(status(&h)["present"], true);
        clear_key(&h).unwrap();
        assert_eq!(status(&h)["status"], "no-key");
    }
    #[test] fn toggling_on_wakes_and_off_disables() {
        let h: Handle = Arc::new(Mutex::new(Shared::default()));
        let mut a = CursorCloudAdapter { h: h.clone(), was_active: false, last: Instant::now() };
        a.configure(&serde_json::json!({"adapters":{"cursorCloud":true}}));
        assert!(h.lock().unwrap().enabled && h.lock().unwrap().wake);
        a.configure(&serde_json::json!({"adapters":{"cursorCloud":false}}));
        assert!(!h.lock().unwrap().enabled);
    }
}
