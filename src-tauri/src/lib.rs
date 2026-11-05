pub mod adapters;
pub mod hook_install;
pub mod signal;
pub mod settings;
pub mod state;
pub mod verses;

use adapters::{build::BuildAdapter, buttons, cursor_cloud, hooks::HookAdapter, Adapter, ToolState};
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

type PxRect = ((i32, i32), (u32, u32));

/// Full covers the monitor. Fit covers the window under the cursor, clipped so the taskbar stays visible.
/// No cursor window: the work area. A cursor window that misses this monitor returns None.
pub(crate) fn whiteout_rect(mode: &str, full: PxRect, work: PxRect, cursor_window: Option<PxRect>) -> Option<PxRect> {
    if mode != "fit" { return Some(full); }
    let bounds = if work.1.0 > 0 && work.1.1 > 0 { work } else { full };
    match cursor_window { Some(win) => clip_rect(win, bounds), None => Some(bounds) }
}

pub(crate) fn clip_rect(inner: PxRect, bounds: PxRect) -> Option<PxRect> {
    let (ix, iy, iw, ih) = (inner.0.0 as i64, inner.0.1 as i64, inner.1.0 as i64, inner.1.1 as i64);
    let (bx, by, bw, bh) = (bounds.0.0 as i64, bounds.0.1 as i64, bounds.1.0 as i64, bounds.1.1 as i64);
    let (x1, y1) = (ix.max(bx), iy.max(by));
    let (x2, y2) = ((ix + iw).min(bx + bw), (iy + ih).min(by + bh));
    if x2 <= x1 || y2 <= y1 { return None; }
    Some(((x1 as i32, y1 as i32), ((x2 - x1) as u32, (y2 - y1) as u32)))
}

/// Top-level window under the pointer, in physical pixels. Our own windows are skipped.
#[cfg(windows)]
fn window_under_cursor(x: i32, y: i32) -> Option<PxRect> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{GetAncestor, GetWindowRect, GetWindowThreadProcessId, WindowFromPoint, GA_ROOT};
    unsafe {
        let hit = WindowFromPoint(POINT { x, y });
        if hit.is_invalid() { return None; }
        let root = GetAncestor(hit, GA_ROOT);
        let hwnd = if root.is_invalid() { hit } else { root };
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == GetCurrentProcessId() { return None; }
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        let (width, height) = (rect.right.saturating_sub(rect.left), rect.bottom.saturating_sub(rect.top));
        if width <= 0 || height <= 0 { return None; }
        Some(((rect.left, rect.top), (width as u32, height as u32)))
    }
}

fn cursor_window(app: &AppHandle) -> Option<PxRect> {
    let p = app.cursor_position().ok()?;
    #[cfg(windows)]
    { window_under_cursor(p.x as i32, p.y as i32) }
    #[cfg(not(windows))]
    { let _ = (p.x, p.y); None }
}

/// Place each whiteout from the monitor list, not from a hidden window's current monitor
/// (that call often fails, so a previous attempt never resized anything).
fn place_whiteouts(app: &AppHandle, mode: &str, cursor_window: Option<PxRect>) -> Vec<tauri::WebviewWindow> {
    let Ok(monitors) = app.available_monitors() else { return vec![] };
    let mut wins = whiteouts(app);
    wins.sort_by(|a, b| a.label().cmp(b.label()));
    let mut placed = vec![];
    for (i, w) in wins.into_iter().enumerate() {
        let Some(m) = monitors.get(i) else { continue };
        let wa = m.work_area();
        let full = ((m.position().x, m.position().y), (m.size().width, m.size().height));
        let work = ((wa.position.x, wa.position.y), (wa.size.width, wa.size.height));
        let Some(((x, y), (wd, ht))) = whiteout_rect(mode, full, work, cursor_window) else {
            let _ = w.set_fullscreen(false);
            continue;
        };
        let _ = w.set_fullscreen(false);
        let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
        let _ = w.set_size(tauri::PhysicalSize::new(wd, ht));
        if mode != "fit" { let _ = w.set_fullscreen(true); }
        placed.push(w);
    }
    placed
}

/// Window work only. Never call while holding the Core lock: window calls on
/// Windows wait for the main thread, which may itself be waiting on the lock.
fn apply(app: &AppHandle, only_active: bool, action: Action) {
    match action {
        Action::None => {}
        Action::Show => {
            let mode = app.state::<Shared>().lock().map(|c| c.settings["display"]["screenMode"].as_str().unwrap_or("full").to_string()).unwrap_or_else(|_| "full".into());
            let cursor = if mode == "fit" { cursor_window(app) } else { None };
            let mut placed = place_whiteouts(app, &mode, cursor);
            if placed.is_empty() && mode == "fit" { placed = place_whiteouts(app, "fit", None); }
            let show_labels: Vec<String> = placed.iter().map(|w| w.label().to_string()).collect();
            let seed: u32 = rand::random();
            let practice = app.state::<Shared>().lock().map(|c| c.machine.is_held()).unwrap_or(false);
            let active = app.cursor_position().ok().and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten());
            for w in whiteouts(app) {
                let on_active = match (&active, w.current_monitor().ok().flatten()) {
                    (Some(a), Some(m)) => a.position() == m.position(),
                    _ => true,
                };
                if !show_labels.iter().any(|l| l == w.label()) || (only_active && !on_active) {
                    let _ = w.hide();
                    continue;
                }
                let _ = w.show();
                let _ = w.set_always_on_top(true);
                if on_active { let _ = w.set_focus(); }
                let _ = w.emit("whiteout:show", json!({ "seed": seed, "practice": practice }));
            }
        }
        Action::Clear(r) => {
            let _ = app.emit("whiteout:clear", json!({ "reason": r.as_str(), "chime": r.chime(), "instant": r.instant() }));
            // Close here. Waiting on the webview left the overlay up when the page never answered.
            for w in whiteouts(app) {
                let _ = w.set_fullscreen(false);
                let _ = w.set_always_on_top(false);
                let _ = w.hide();
            }
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

/// Card §19: one place registers both global combos (unregister_all would otherwise drop the other).
/// Returns (panic_failed, quit_failed). A quit combo equal to panic is skipped.
fn register_hotkeys(app: &AppHandle, panic: &str, quit: &str) -> (bool, bool) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let p = gs.on_shortcut(settings::to_shortcut(panic).as_str(), |app, _s, ev| {
        if ev.state() == ShortcutState::Pressed { with_core(app, |c| (dismiss(c, false), ())); }
    }).is_err();
    let q = if settings::to_shortcut(quit) == settings::to_shortcut(panic) { true } else {
        gs.on_shortcut(settings::to_shortcut(quit).as_str(), |_app, _s, ev| {
            if ev.state() == ShortcutState::Pressed { std::process::exit(0); }
        }).is_err()
    };
    (p, q)
}

fn combos(v: &Value) -> (String, String) {
    (v["panicHotkey"].as_str().unwrap_or("Ctrl+Alt+Shift+Esc").to_string(), v["quitHotkey"].as_str().unwrap_or("Ctrl+Alt+Shift+Q").to_string())
}

/// Card §19: same full exit as the tray's Quit.
#[tauri::command]
fn quit_app(_app: AppHandle) { std::process::exit(0); }

#[tauri::command]
async fn get_settings(core: tauri::State<'_, Shared>) -> Result<Value, String> { Ok(core.lock().unwrap().settings.clone()) }

/// Returns a warning string for Settings to show (hotkey conflict), or null.
#[tauri::command]
async fn save_settings(app: AppHandle, mut settings: Value) -> Result<Option<String>, String> {
    let (panic, quit) = combos(&settings);
    let prev_quit;
    {
        let shared = app.state::<Shared>().inner().clone();
        let mut c = shared.lock().unwrap();
        prev_quit = combos(&c.settings).1;
        settings["paused"] = serde_json::json!(c.paused);
        settings::save(&c.settings_path, &settings)?;
        for a in c.adapters.iter_mut() { a.configure(&settings); }
        c.settings = settings;
    }
    { let s = app.state::<Shared>().lock().unwrap().settings.clone(); sync_display_items(&app, &s); }
    let (p_err, q_err) = register_hotkeys(&app, &panic, &quit);
    if q_err && prev_quit != quit {
        // Taken: keep the old quit combo working and saved (UX §19 addendum).
        let _ = register_hotkeys(&app, &panic, &prev_quit);
        let mut settings = app.state::<Shared>().lock().unwrap().settings.clone();
        settings["quitHotkey"] = json!(prev_quit);
        let shared = app.state::<Shared>().inner().clone();
        let mut c = shared.lock().unwrap();
        let _ = settings::save(&c.settings_path, &settings);
        c.settings = settings;
        return Ok(Some("That combo is in use. Pick another.".into()));
    }
    if p_err { return Ok(Some("This shortcut is in use by another app. The hotkey will still exit the whiteout.".into())); }
    if q_err { return Ok(Some("That combo is in use. Pick another.".into())); }
    Ok(None)
}

fn dismiss(c: &mut Core, exit_button: bool) -> Action {
    if !c.machine.is_held() { c.agg.hold(); }
    if exit_button { c.machine.exit_button() } else { c.machine.panic() }
}

#[tauri::command]
fn request_clear(app: AppHandle, reason: String) {
    with_core(&app, |c| (dismiss(c, reason == "exit-button"), ()));
}

#[tauri::command]
fn whiteout_faded(window: tauri::WebviewWindow) { let _ = window.hide(); }

#[tauri::command]
fn show_whiteout_test(app: AppHandle) {
    with_core(&app, |c| { let n = now_ms(c); (c.machine.show_preview(n), ()) });
}

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

/// Settings is the taskbar window. A tray-only process has nothing for the taskbar pin to
/// activate, so Windows opens the hidden-icons flyout instead of this app.
#[cfg(windows)]
fn restore_and_focus_hwnd(w: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{BringWindowToTop, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW};
    if let Ok(raw) = w.hwnd() {
        unsafe {
            let hwnd = HWND(raw.0);
            let _ = ShowWindow(hwnd, if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW });
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        #[cfg(windows)]
        restore_and_focus_hwnd(&w);
        let _ = w.set_focus();
        return;
    }
    if let Ok(w) = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Whitespace settings").inner_size(640.0, 820.0).skip_taskbar(false).visible(true).focused(true).build()
    {
        let _ = w.unminimize();
        let _ = w.show();
        #[cfg(windows)]
        restore_and_focus_hwnd(&w);
        let _ = w.set_focus();
    }
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
            let paused = settings["paused"].as_bool().unwrap_or(false);
            let (cloud, cloud_h) = cursor_cloud::CursorCloudAdapter::start(cursor_cloud::API.to_string());
            app.manage(cloud_h);
            let shared: Shared = Arc::new(Mutex::new(Core {
                machine: Machine::default(), agg: Aggregator::default(), origin: Instant::now(), paused, settings: settings.clone(), settings_path,
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
            let (hp, hq) = combos(&settings);
            let (pe, qe) = register_hotkeys(&handle, &hp, &hq);
            if pe { eprintln!("panic hotkey registration failed; exit button, cap and quiet timeout still apply"); }
            if qe { eprintln!("quit hotkey registration failed; tray Quit still works"); }

            let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let pause_i = CheckMenuItem::with_id(app, "pause", "Pause adapters", true, paused, None::<&str>)?;
            let test_i = MenuItem::with_id(app, "test", "WhiteSpace me", true, None::<&str>)?;
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
                    "pause" => with_core(app, |c| {
                        c.paused = !c.paused;
                        c.settings["paused"] = serde_json::json!(c.paused);
                        let _ = settings::save(&c.settings_path, &c.settings);
                        (if c.paused { c.machine.disable() } else { Action::None }, ())
                    }),
                    "test" => show_whiteout_test(app.clone()),
                    "show-verses" => toggle_display(app, "verses"),
                    "show-breathing" => toggle_display(app, "breathing"),
                    "quit" => std::process::exit(0),
                    _ => {}
                }).build(app)?;
            // Open Settings on launch so the taskbar button shows this window.
            // Closing Settings (title bar or taskbar preview) exits the process.
            open_settings(&handle);
            slog(&logdir, "tray ready");

            buttons::start_watcher();
            // Core loop: poll adapters, run timers (1 s delay, 20 min cap, 90 s quiet).
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(100));
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
                    // Accept / Run / Start / Allow (and the same kind of button) drop the cover
                    // on their own, including while it is still waiting to appear.
                    if machine.covers_or_will() && buttons::executable_prompt_visible() {
                        actions.push(agg.report(machine, "buttons", ToolState::NeedsYou, now));
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
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                match event {
                    tauri::WindowEvent::CloseRequested { .. } => {
                        // Title-bar X and the taskbar preview X both land here.
                        // app.exit() only posts a request, and a codeless ExitRequested
                        // is cancelled below, so the window stays and the taskbar keeps
                        // its open underline. Leave the process immediately.
                        std::process::exit(0);
                    }
                    tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Focused(true) => {
                        if !window.is_minimized().unwrap_or(false) {
                            let _ = window.unminimize();
                        }
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![get_settings, save_settings, request_clear, whiteout_faded, show_whiteout_test, verse_refs, verse_translations, verse_get, hook_status, install_hook, remove_hook, cursor_key_status, set_cursor_key, clear_cursor_key, quit_app])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, ev| { if let tauri::RunEvent::ExitRequested { api, code, .. } = ev { if code.is_none() { api.prevent_exit(); } } });
}

#[cfg(test)]
mod screen_mode_tests {
    use super::{clip_rect, whiteout_rect};
    const FULL: ((i32, i32), (u32, u32)) = ((0, 0), (1920, 1080));
    const WORK: ((i32, i32), (u32, u32)) = ((0, 0), (1920, 1040));
    #[test] fn full_covers_monitor() { assert_eq!(whiteout_rect("full", FULL, WORK, None), Some(FULL)); }
    #[test] fn fit_uses_work_area_when_no_window() { assert_eq!(whiteout_rect("fit", FULL, WORK, None), Some(WORK)); }
    #[test] fn fit_uses_the_window_under_the_cursor() {
        let cursor = ((100, 80), (800, 600));
        assert_eq!(whiteout_rect("fit", FULL, WORK, Some(cursor)), Some(cursor));
    }
    #[test] fn fit_clips_a_maximized_window_so_the_taskbar_stays_visible() {
        let maximized = ((-8, -8), (1936, 1056));
        assert_eq!(whiteout_rect("fit", FULL, WORK, Some(maximized)), Some(WORK));
    }
    #[test] fn fit_skips_a_monitor_the_window_misses() {
        let other = ((2000, 100), (800, 600));
        assert_eq!(whiteout_rect("fit", FULL, WORK, Some(other)), None);
        assert_eq!(clip_rect(other, WORK), None);
    }
    #[test] fn unknown_or_empty_work_area_falls_back_to_full() {
        assert_eq!(whiteout_rect("weird", FULL, WORK, None), Some(FULL));
        assert_eq!(whiteout_rect("fit", FULL, ((0, 0), (0, 0)), None), Some(FULL));
    }
}
