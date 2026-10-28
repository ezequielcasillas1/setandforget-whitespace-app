pub mod adapters;
pub mod hook_install;
pub mod signal;
pub mod settings;
pub mod state;
pub mod verses;

use adapters::{build::BuildAdapter, cursor_cloud, hooks::HookAdapter, Adapter};
use serde_json::{json, Value};
use state::{Action, Aggregator, Machine};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// Appends one line to startup.log in the app's log folder (best effort).
fn slog(dir: &Option<std::path::PathBuf>, msg: &str) {
    use std::io::Write;
    let Some(d) = dir else { return };
    let _ = std::fs::create_dir_all(d);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(d.join("startup.log")) {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = writeln!(f, "{t} {msg}");
    }
}

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

/// Tray checkmarks for the display switches (card §17), kept in sync with Settings.
struct DisplayItems { verses: CheckMenuItem<tauri::Wry>, breathing: CheckMenuItem<tauri::Wry> }

fn sync_display_items(app: &AppHandle, settings: &Value) {
    if let Some(d) = app.try_state::<DisplayItems>() {
        let _ = d.verses.set_checked(settings["display"]["verses"] != json!(false));
        let _ = d.breathing.set_checked(settings["display"]["breathing"] != json!(false));
    }
}

/// Flip one display switch from the tray and save right away.
fn toggle_display(app: &AppHandle, key: &str) {
    let shared = app.state::<Shared>().inner().clone();
    let (settings, err) = {
        let mut c = shared.lock().unwrap();
        let on = c.settings["display"][key] != json!(false);
        c.settings["display"][key] = json!(!on);
        let err = settings::save(&c.settings_path, &c.settings).err();
        (c.settings.clone(), err)
    };
    if let Some(e) = err { eprintln!("saving display switch failed: {e}"); }
    sync_display_items(app, &settings);
    let _ = app.emit("settings:display", settings["display"].clone());
}

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

/// Card §18: the whole monitor for "full", the monitor minus the taskbar for "fit".
pub(crate) fn whiteout_rect(mode: &str, full: ((i32, i32), (u32, u32)), work: ((i32, i32), (u32, u32))) -> ((i32, i32), (u32, u32)) {
    if mode == "fit" && work.1 .0 > 0 && work.1 .1 > 0 { work } else { full }
}

/// Re-size every whiteout to its monitor right before showing, so a mode change needs no restart.
fn fit_whiteouts(app: &AppHandle, mode: &str) {
    for w in whiteouts(app) {
        let Some(m) = w.current_monitor().ok().flatten() else { continue };
        let wa = m.work_area();
        let ((x, y), (wd, ht)) = whiteout_rect(mode, ((m.position().x, m.position().y), (m.size().width, m.size().height)),
            ((wa.position.x, wa.position.y), (wa.size.width, wa.size.height)));
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = w.set_size(tauri::PhysicalSize::new(wd, ht));
    }
}

/// Window work only. Never call while holding the Core lock: window calls on
/// Windows wait for the main thread, which may itself be waiting on the lock.
fn apply(app: &AppHandle, only_active: bool, action: Action) {
    match action {
        Action::None => {}
        Action::Show => {
            let mode = app.state::<Shared>().lock().map(|c| c.settings["display"]["screenMode"].as_str().unwrap_or("full").to_string()).unwrap_or_else(|_| "full".into());
            fit_whiteouts(app, &mode);
            let seed: u32 = rand::random();
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
    let (a, only_active, r) = {
        let mut c = shared.lock().unwrap();
        let (a, r) = f(&mut c);
        (a, c.settings["monitors"] == "active", r)
    };
    apply(app, only_active, a);
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
async fn get_settings(core: tauri::State<'_, Shared>) -> Result<Value, String> { Ok(core.lock().unwrap().settings.clone()) }

/// Returns a warning string for Settings to show (hotkey conflict), or null.
#[tauri::command]
async fn save_settings(app: AppHandle, settings: Value) -> Result<Option<String>, String> {
    let combo = settings["panicHotkey"].as_str().unwrap_or("Ctrl+Alt+Shift+Esc").to_string();
    {
        let shared = app.state::<Shared>().inner().clone();
        let mut c = shared.lock().unwrap();
        settings::save(&c.settings_path, &settings)?;
        for a in c.adapters.iter_mut() { a.configure(&settings); }
        c.settings = settings;
    }
    { let s = app.state::<Shared>().lock().unwrap().settings.clone(); sync_display_items(&app, &s); }
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

#[tauri::command]
async fn hook_status() -> Vec<Value> { hook_install::status_all() }
#[tauri::command]
async fn install_hook(id: String) -> Result<Vec<Value>, String> { hook_install::install(&id)?; Ok(hook_install::status_all()) }
#[tauri::command]
async fn remove_hook(id: String) -> Result<Vec<Value>, String> { hook_install::remove(&id)?; Ok(hook_install::status_all()) }

#[tauri::command]
async fn cursor_key_status(h: tauri::State<'_, cursor_cloud::Handle>) -> Result<Value, String> { Ok(cursor_cloud::status(&h)) }
#[tauri::command]
async fn set_cursor_key(h: tauri::State<'_, cursor_cloud::Handle>, key: String) -> Result<Value, String> { cursor_cloud::set_key(&h, &key)?; Ok(cursor_cloud::status(&h)) }
#[tauri::command]
async fn clear_cursor_key(h: tauri::State<'_, cursor_cloud::Handle>) -> Result<Value, String> { cursor_cloud::clear_key(&h)?; Ok(cursor_cloud::status(&h)) }

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
        // Must be first: a second launch hands off to the running app and exits.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_settings(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let handle = app.handle().clone();
            let logdir = app.path().app_log_dir().ok();
            slog(&logdir, &format!("start v{}", app.package_info().version));
            { let ld = logdir.clone(); let prev = std::panic::take_hook();
              std::panic::set_hook(Box::new(move |i| { slog(&ld, &format!("panic: {i}")); prev(i); })); }
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::load(&settings_path);
            let (cloud, cloud_h) = cursor_cloud::CursorCloudAdapter::start(cursor_cloud::API.to_string());
            app.manage(cloud_h);
            let shared: Shared = Arc::new(Mutex::new(Core {
                machine: Machine::default(), agg: Aggregator::default(), origin: Instant::now(), paused: false, settings: settings.clone(), settings_path,
                adapters: {
                    let mut v: Vec<Box<dyn Adapter>> = vec![
                        Box::new(BuildAdapter::new()),
                        Box::new(HookAdapter::real("claude", "Claude Code")), Box::new(HookAdapter::real("codex", "OpenAI Codex")),
                        Box::new(HookAdapter::real("copilot", "GitHub Copilot")), Box::new(HookAdapter::real("cursor", "Cursor")),
                        Box::new(HookAdapter::real("windsurf", "Windsurf")), Box::new(cloud),
                    ];
                    for a in v.iter_mut() { a.configure(&settings); }
                    v
                }, verses: verses::client::Client::from_env().ok(),
            }));
            app.manage(shared.clone());
            slog(&logdir, "state ready");
            if let Err(e) = create_whiteouts(&handle) { slog(&logdir, &format!("whiteouts failed: {e}")); return Err(e.into()); }
            slog(&logdir, "whiteouts created");
            if register_hotkey(&handle, settings["panicHotkey"].as_str().unwrap_or("Ctrl+Alt+Shift+Esc")).is_err() {
                eprintln!("panic hotkey registration failed; exit button, cap and quiet timeout still apply");
            }

            let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let pause_i = CheckMenuItem::with_id(app, "pause", "Pause adapters", true, false, None::<&str>)?;
            let test_i = MenuItem::with_id(app, "test", "Show whiteout now (test)", true, None::<&str>)?;
            let verses_i = CheckMenuItem::with_id(app, "show-verses", "Show verses", true, settings["display"]["verses"] != json!(false), None::<&str>)?;
            let breathing_i = CheckMenuItem::with_id(app, "show-breathing", "Show breathing", true, settings["display"]["breathing"] != json!(false), None::<&str>)?;
            app.manage(DisplayItems { verses: verses_i.clone(), breathing: breathing_i.clone() });
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_i, &pause_i, &test_i, &verses_i, &breathing_i, &quit_i])?;
            TrayIconBuilder::new().icon(app.default_window_icon().unwrap().clone()).tooltip("Whitespace").menu(&menu)
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev { open_settings(tray.app_handle()); }
                })
                .on_menu_event(move |app, ev| match ev.id().as_ref() {
                    "settings" => open_settings(app),
                    "pause" => with_core(app, |c| { c.paused = !c.paused; (if c.paused { c.machine.disable() } else { Action::None }, ()) }),
                    "test" => show_whiteout_test(app.clone()),
                    "show-verses" => toggle_display(app, "verses"),
                    "show-breathing" => toggle_display(app, "breathing"),
                    "quit" => app.exit(0),
                    _ => {}
                }).build(app)?;
            slog(&logdir, "tray ready");

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
                let only_active = c.settings["monitors"] == "active";
                drop(c);
                for a in actions { apply(&handle, only_active, a); }
            });
            slog(&logdir, "loop started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, request_clear, whiteout_faded, show_whiteout_test, verse_refs, verse_translations, verse_get, hook_status, install_hook, remove_hook, cursor_key_status, set_cursor_key, clear_cursor_key])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, ev| { if let tauri::RunEvent::ExitRequested { api, code, .. } = ev { if code.is_none() { api.prevent_exit(); } } });
}

#[cfg(test)]
mod screen_mode_tests {
    use super::whiteout_rect;
    const FULL: ((i32, i32), (u32, u32)) = ((0, 0), (1920, 1080));
    const WORK: ((i32, i32), (u32, u32)) = ((0, 0), (1920, 1040));
    #[test] fn full_covers_monitor() { assert_eq!(whiteout_rect("full", FULL, WORK), FULL); }
    #[test] fn fit_uses_work_area() { assert_eq!(whiteout_rect("fit", FULL, WORK), WORK); }
    #[test] fn unknown_or_empty_work_area_falls_back_to_full() {
        assert_eq!(whiteout_rect("weird", FULL, WORK), FULL);
        assert_eq!(whiteout_rect("fit", FULL, ((0, 0), (0, 0))), FULL);
    }
}
