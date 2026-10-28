//! settings.json in the app config dir, local only (card §8).
use serde_json::{json, Value};
use std::path::PathBuf;

pub fn defaults() -> Value {
    json!({ "exercise": "coherence", "verseIntervalSec": [45, 60], "verseFirstAtStart": true,
      "adapters": { "cursor": true }, "fallbackAccessibility": false, "dustTheme": false,
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
