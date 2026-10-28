//! settings.json in the app config dir, local only (card §8).
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn defaults() -> Value {
    json!({ "exercise": "coherence", "verseIntervalSec": [45, 60], "verseFirstAtStart": true,
      "adapters": { "cursor": true, "build": true }, "watch": crate::adapters::build::default_watch(), "fallbackAccessibility": false, "dustTheme": false,
      "reducedMotion": "system", "monitors": "all", "panicHotkey": "Ctrl+Alt+Shift+Esc",
      "adapterQuietSec": 90, "chime": { "muted": false, "volume": 0.25 } })
}

pub fn load(path: &PathBuf) -> Value {
    let mut base = defaults();
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(Value::Object(saved)) = serde_json::from_str::<Value>(&text) {
            for (k, v) in saved { base[k] = v; }
        }
    }
    // Missing adapter keys default on (Terminal builds is on by default, card §15.3).
    if let Some(a) = base["adapters"].as_object_mut() { a.entry("build").or_insert(json!(true)); }
    // Silently dedupe / drop blocked entries from a loaded settings.json (UX S2.2).
    base["watch"] = json!(crate::adapters::build::watch_from_settings(&base));
    base
}

pub fn save(path: &PathBuf, v: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
    std::fs::write(path, serde_json::to_string_pretty(v).unwrap()).map_err(|e| e.to_string())
}

/// "Ctrl+Alt+Shift+Esc" → the global-shortcut plugin's syntax.
pub fn to_shortcut(s: &str) -> String {
    s.split('+').map(|p| match p.trim() {
        "Ctrl" => "Control".to_string(), "Win" => "Super".to_string(), "Esc" => "Escape".to_string(), "Del" => "Delete".to_string(),
        other => other.to_string(),
    }).collect::<Vec<_>>().join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn load_dedupes_watch_and_defaults_build_on() {
        let p = std::env::temp_dir().join(format!("ws-settings-{}.json", std::process::id()));
        std::fs::write(&p, r#"{"adapters":{"cursor":false},"watch":[{"match":"make","kind":"command"},{"match":"Make ","kind":"command"}]}"#).unwrap();
        let v = load(&p);
        let _ = std::fs::remove_file(&p);
        assert_eq!(v["adapters"]["build"], json!(true));
        assert_eq!(v["watch"], json!([{ "match": "make", "kind": "command" }]));
        assert_eq!(defaults()["watch"].as_array().unwrap().len(), 11);
    }
    #[test] fn shortcut_mapping() { assert_eq!(to_shortcut("Ctrl+Alt+Shift+Esc"), "Control+Alt+Shift+Escape"); }
    #[test] fn load_merges_defaults() {
        let p = std::env::temp_dir().join(format!("aio-{}.json", std::process::id()));
        std::fs::write(&p, r#"{"dustTheme":true}"#).unwrap();
        let v = load(&p);
        assert_eq!(v["dustTheme"], true);
        assert_eq!(v["panicHotkey"], "Ctrl+Alt+Shift+Esc");
        let _ = std::fs::remove_file(p);
    }
}
