pub mod adapters;
pub mod settings;
pub mod state;
pub mod verses;

use adapters::{build::BuildAdapter, cursor::CursorAdapter, Adapter};
use serde_json::{json, Value};
use state::{Action, Aggregator, Machine};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

struct Core {
    machine: Machine,
    agg: Aggregator,
    origin: Instant,
    paused: bool,
    settings: Value,
    settings_path: std::path::PathBuf,
    adapters: Vec<Box<dyn Adapter>>,
    verses: Option<verses::client::Client>,
}
type Shared = Arc<Mutex<Core>>;

fn now_ms(c: &Core) -> u64 { c.origin.elapsed().as_millis() as u64 }

fn whiteouts(app: &AppHandle) -> Vec<tauri::WebviewWindow> {
    app.webview_windows().into_iter().filter(|(l, _)| l.starts_with("whiteout-")).map(|(_, w)| w).collect()
}

fn create_whiteouts(app: &AppHandle) -> tauri::Result<()> {
    for (i, m) in app.available_monitors()?.into_iter().enumerate() {
        let label = format!("whiteout-{i}");
        if app.get_webview_window(&label).is_some() { continue; }
        let pos = m.position().to_logical::<f64>(m.scale_factor());
        let size = m.size().to_logical::<f64>(m.scale_factor());
        WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
            .decorations(false).transparent(true).always_on_top(true).skip_taskbar(true)
            .resizable(false).visible(false).position(pos.x, pos.y).inner_size(size.width, size.height)
            .build()?;
    }
    Ok(())
}

fn apply(app: &AppHandle, core: &Core, action: Action) {
    match action {
        Action::None => {}
        Action::Show => {
            let seed: u32 = rand::random();
            let only_active = core.settings["monitors"] == "active";
            let active = app.cursor_position().ok().and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten());
            for w in whiteouts(app) {
                let on_active = match (&active, w.current_monitor().ok().flatten()) {
                    (Some(a), Some(m)) => a.position() == m.position(),
                    _ => true,
                };
                if only_active && !on_active { continue; }
                let _ = w.show();
                let _ = w.set_always_on_top(true);
                if on_active { let _ = w.set_focus(); }
                let _ = w.emit("whiteout:show", json!({ "seed": seed }));
            }
        }
        Action::Clear(r) => {
            let _ = app.emit("whiteout:clear", json!({ "reason": r.as_str(), "chime": r.chime(), "instant": r.instant() }));
            // Safety net: hide even if a webview never answers whiteout_faded.
            let app2 = app.clone();
            std::thread::spawn(move || { std::thread::sleep(Duration::from_millis(2500)); for w in whiteouts(&app2) { let _ = w.hide(); } });
        }
    }
}

fn with_core<R>(app: &AppHandle, f: impl FnOnce(&mut Core) -> (Action, R)) -> R {
    let shared = app.state::<Shared>().inner().clone();
    let mut c = shared.lock().unwrap();
    let (a, r) = f(&mut c);
    apply(app, &c, a);
    r
}

fn register_hotkey(app: &AppHandle, combo: &str) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    gs.on_shortcut(settings::to_shortcut(combo).as_str(), |app, _s, ev| {
        if ev.state() == ShortcutState::Pressed { with_core(app, |c| (c.machine.panic(), ())); }
    }).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_settings(core: tauri::State<Shared>) -> Value { core.lock().unwrap().settings.clone() }

/// Returns a warning string for Settings to show (hotkey conflict), or null.
#[tauri::command]
fn save_settings(app: AppHandle, settings: Value) -> Result<Option<String>, String> {
    let combo = settings["panicHotkey"].as_str().unwrap_or("Ctrl+Alt+Shift+Esc").to_string();
    {
        let shared = app.state::<Shared>().inner().clone();
        let mut c = shared.lock().unwrap();
        settings::save(&c.settings_path, &settings)?;
        for a in c.adapters.iter_mut() { a.configure(&settings); }
        c.settings = settings;
    }
    Ok(register_hotkey(&app, &combo).err().map(|_| "This shortcut is in use by another app. The hotkey will still exit the whiteout.".into()))
}

#[tauri::command]
fn request_clear(app: AppHandle, reason: String) {
    with_core(&app, |c| (if reason == "exit-button" { c.machine.exit_button() } else { c.machine.panic() }, ()));
}

#[tauri::command]
fn whiteout_faded(window: tauri::WebviewWindow) { let _ = window.hide(); }

#[tauri::command]
fn show_whiteout_test(app: AppHandle) { with_core(&app, |c| { let n = now_ms(c); (c.machine.show_now(n), ()) }); }

fn client(core: &tauri::State<Shared>) -> Result<verses::client::Client, String> {
    let c = core.lock().unwrap();
    match &c.verses { Some(_) => verses::client::Client::from_env(), None => Err("verse API not configured".into()) }
}
#[tauri::command]
async fn verse_refs(core: tauri::State<'_, Shared>) -> Result<Vec<String>, String> {
    let cl = client(&core)?; tauri::async_runtime::spawn_blocking(move || cl.refs()).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn verse_translations(core: tauri::State<'_, Shared>) -> Result<Vec<verses::client::ApiTranslation>, String> {
    let cl = client(&core)?; tauri::async_runtime::spawn_blocking(move || cl.translations()).await.map_err(|e| e.to_string())?
}
#[tauri::command]
async fn verse_get(core: tauri::State<'_, Shared>, reference: String, translation: String) -> Result<verses::client::Verse, String> {
    let cl = client(&core)?; tauri::async_runtime::spawn_blocking(move || cl.verse(&reference, &translation)).await.map_err(|e| e.to_string())?
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") { let _ = w.show(); let _ = w.set_focus(); return; }
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Whitespace settings").inner_size(640.0, 820.0).build();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::load(&settings_path);
            let shared: Shared = Arc::new(Mutex::new(Core {
                machine: Machine::default(), agg: Aggregator::default(), origin: Instant::now(), paused: false, settings: settings.clone(), settings_path,
                adapters: {
                    let mut v: Vec<Box<dyn Adapter>> = vec![Box::new(CursorAdapter::new()), Box::new(BuildAdapter::new())];
                    for a in v.iter_mut() { a.configure(&settings); }
                    v
                }, verses: verses::client::Client::from_env().ok(),
            }));
            app.manage(shared.clone());
            create_whiteouts(&handle)?;
            if register_hotkey(&handle, settings["panicHotkey"].as_str().unwrap_or("Ctrl+Alt+Shift+Esc")).is_err() {
                eprintln!("panic hotkey registration failed; exit button, cap and quiet timeout still apply");
            }

            let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let pause_i = CheckMenuItem::with_id(app, "pause", "Pause adapters", true, false, None::<&str>)?;
            let test_i = MenuItem::with_id(app, "test", "Show whiteout now (test)", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &pause_i, &test_i, &quit_i])?;
            TrayIconBuilder::new().icon(app.default_window_icon().unwrap().clone()).tooltip("Whitespace").menu(&menu)
                .on_menu_event(move |app, ev| match ev.id().as_ref() {
                    "settings" => open_settings(app),
                    "pause" => with_core(app, |c| { c.paused = !c.paused; (if c.paused { c.machine.disable() } else { Action::None }, ()) }),
                    "test" => show_whiteout_test(app.clone()),
                    "quit" => app.exit(0),
                    _ => {}
                }).build(app)?;

            // Core loop: poll adapters, run timers (3 s delay, 20 min cap, 90 s quiet).
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(250));
                let mut c = shared.lock().unwrap();
                let now = now_ms(&c);
                let mut actions = vec![];
                if !c.paused {
                    let enabled = c.settings["adapters"].clone();
                    let mut reports = vec![];
                    for a in c.adapters.iter_mut() {
                        if enabled[a.id()].as_bool() == Some(false) { a.reset(); reports.push((a.id(), None)); continue; }
                        if let Some(s) = a.poll() { reports.push((a.id(), Some(s))); }
                    }
                    let Core { machine, agg, .. } = &mut *c;
                    for (id, s) in reports {
                        actions.push(match s { Some(s) => agg.report(machine, id, s, now), None => agg.forget(machine, id, now) });
                    }
                }
                actions.push(c.machine.tick(now));
                for a in actions { apply(&handle, &c, a); }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, request_clear, whiteout_faded, show_whiteout_test, verse_refs, verse_translations, verse_get])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, ev| { if let tauri::RunEvent::ExitRequested { api, code, .. } = ev { if code.is_none() { api.prevent_exit(); } } });
}
