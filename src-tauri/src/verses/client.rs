//! Client for the local KBS verse API (card rev 2 §7). Talks only to 127.0.0.1; never to HelloAO.
//! Dev: `KBS_VERSE_API_URL` (default http://127.0.0.1:8787) and optional `KBS_VERSE_API_TOKEN`.
//! The packaged sidecar (port + token on stdout) is not wired: the kbs-verse-api project doesn't exist yet.
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranslationInfo { pub abbr: String, pub name: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Verse { pub r#ref: String, pub display: String, pub text: String, pub translation: TranslationInfo, pub source: String }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiTranslation { pub abbr: String, pub name: String, #[serde(default)] pub testaments: Vec<String> }

pub struct Client { base: String, token: Option<String>, http: reqwest::blocking::Client }

pub fn is_loopback(url: &str) -> bool {
    let rest = url.strip_prefix("http://").unwrap_or("");
    rest.starts_with("127.0.0.1:") || rest.starts_with("localhost:") || rest == "127.0.0.1" || rest.starts_with("127.0.0.1/")
}

impl Client {
    pub fn from_env() -> Result<Self, String> {
        let base = std::env::var("KBS_VERSE_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".into());
        Self::new(base.trim_end_matches('/').to_string(), std::env::var("KBS_VERSE_API_TOKEN").ok())
    }
    pub fn new(base: String, token: Option<String>) -> Result<Self, String> {
        if !is_loopback(&base) { return Err(format!("refusing non-local verse API url: {base}")); }
        let http = reqwest::blocking::Client::builder().timeout(Duration::from_secs(3)).no_proxy().build().map_err(|e| e.to_string())?;
        Ok(Self { base, token, http })
    }
    fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T, String> {
        let mut req = self.http.get(format!("{}{}", self.base, path));
        if let Some(t) = &self.token { req = req.bearer_auth(t); }
        let res = req.send().map_err(|e| e.to_string())?;
        let status = res.status();
        if !status.is_success() { return Err(format!("verse API {}", status.as_u16())); }
        res.json::<T>().map_err(|e| e.to_string())
    }
    pub fn refs(&self) -> Result<Vec<String>, String> { self.get("/v1/refs") }
    pub fn translations(&self) -> Result<Vec<ApiTranslation>, String> { self.get("/v1/translations") }
    pub fn verse(&self, reference: &str, translation: &str) -> Result<Verse, String> {
        let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
        if !ok(reference) || !ok(translation) { return Err("bad ref or translation".into()); }
        self.get(&format!("/v1/verse/{reference}?translation={translation}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn fake_api(status: &'static str, body: &'static str) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let _ = write!(s, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    #[test] fn only_loopback() {
        assert!(Client::new("https://bible.helloao.org/api".into(), None).is_err());
        assert!(Client::new("http://127.0.0.1:8787".into(), None).is_ok());
    }

    #[test] fn parses_verse() {
        let url = fake_api("200 OK", r#"{"ref":"EPH.2.8-9","display":"Ephesians 2:8-9","text":"For by grace","translation":{"abbr":"KJV","name":"King James Version"},"source":"Free Use Bible API (HelloAO)"}"#);
        let v = Client::new(url, None).unwrap().verse("EPH.2.8-9", "KJV").unwrap();
        assert_eq!(v.translation.abbr, "KJV");
        assert_eq!(v.display, "Ephesians 2:8-9");
    }

    #[test] fn maps_404() {
        let url = fake_api("404 Not Found", "{}");
        assert_eq!(Client::new(url, None).unwrap().verse("GEN.1.1", "KJV").unwrap_err(), "verse API 404");
    }

    #[test] fn api_down_is_an_error() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
        drop(l);
        assert!(Client::new(url, None).unwrap().refs().is_err());
    }

    #[test] fn rejects_injection() {
        assert!(Client::new("http://127.0.0.1:1".into(), None).unwrap().verse("JHN.3.16?x=1", "KJV").is_err());
    }
}
