use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::image::Image;

static TOPMOST_PAUSE_UNTIL: AtomicU64 = AtomicU64::new(0);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn pause_topmost_for(secs: u64) {
    TOPMOST_PAUSE_UNTIL.store(now_secs() + secs, Ordering::Relaxed);
}

fn resume_topmost() {
    TOPMOST_PAUSE_UNTIL.store(0, Ordering::Relaxed);
}

fn topmost_paused() -> bool {
    now_secs() < TOPMOST_PAUSE_UNTIL.load(Ordering::Relaxed)
}
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WindowEvent, Wry};

fn default_count() -> u32 { 10 }
fn default_pixel() -> u32 { 3 }

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    #[serde(default = "default_count")]
    count: u32,
    #[serde(default = "default_pixel")]
    pixel: u32,
    #[serde(default)]
    transparent: bool,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
    #[serde(default)]
    width: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            count: default_count(),
            pixel: default_pixel(),
            transparent: false,
            x: None,
            y: None,
            width: None,
        }
    }
}

struct AppState {
    settings: Mutex<Settings>,
}

#[tauri::command]
fn move_window(window: tauri::WebviewWindow, dx: i32, dy: i32) {
    if let Ok(pos) = window.outer_position() {
        let _ = window.set_position(PhysicalPosition {
            x: pos.x + dx,
            y: pos.y + dy,
        });
    }
}

#[tauri::command]
fn get_settings(state: State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn show_context_menu(app: AppHandle, window: tauri::WebviewWindow) -> Result<(), String> {
    let menu = build_menu(&app).map_err(|e| e.to_string())?;
    pause_topmost_for(3);
    window.popup_menu(&menu).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            move_window,
            get_settings,
            show_context_menu
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let settings = load_settings(&handle);
            app.manage(AppState {
                settings: Mutex::new(settings),
            });

            let window = app.get_webview_window("main").unwrap();
            apply_initial_bounds(&handle, &window);

            // トレイアイコン
            let tray_icon = Image::from_bytes(include_bytes!("../icons/build/icon.png"))?;
            let tray_menu = build_menu(&handle)?;
            TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&tray_menu)
                .tooltip("Taskbar Aquarium")
                .build(app)?;

            app.on_menu_event(|app, event| {
                handle_menu_event(app, event.id().as_ref());
            });

            // ウィンドウ移動・リサイズで bounds を保存
            let h_for_event = handle.clone();
            window.on_window_event(move |event| {
                match event {
                    WindowEvent::Moved(pos) => {
                        update_settings(&h_for_event, |s| {
                            s.x = Some(pos.x);
                            s.y = Some(pos.y);
                        });
                    }
                    WindowEvent::Resized(size) => {
                        update_settings(&h_for_event, |s| {
                            s.width = Some(size.width);
                        });
                    }
                    _ => {}
                }
            });

            // 押し負け復帰：1秒ごとに Z オーダーを最前面に再設定
            spawn_topmost_refresh(&window);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn apply_initial_bounds(app: &AppHandle, window: &tauri::WebviewWindow) {
    let Some(monitor) = window.current_monitor().ok().flatten() else {
        return;
    };
    let mon_size = monitor.size();
    let mon_pos = monitor.position();

    let taskbar_h = get_taskbar_height(mon_size.height).unwrap_or(48);
    let state: State<AppState> = app.state();
    let s = state.settings.lock().unwrap();

    let win_w = s.width.unwrap_or(480);
    let win_h = taskbar_h;

    let (x, y) = match (s.x, s.y) {
        (Some(x), Some(y)) => (x, y),
        _ => (
            mon_pos.x + (mon_size.width as i32 - win_w as i32) / 2,
            mon_pos.y + mon_size.height as i32 - win_h as i32,
        ),
    };
    drop(s);

    let _ = window.set_size(PhysicalSize {
        width: win_w,
        height: win_h,
    });
    let _ = window.set_position(PhysicalPosition { x, y });
}

#[cfg(windows)]
fn spawn_topmost_refresh(window: &tauri::WebviewWindow) {
    let hwnd_addr: isize = window
        .hwnd()
        .ok()
        .map(|h| h.0 as isize)
        .unwrap_or(0);
    if hwnd_addr == 0 {
        return;
    }
    std::thread::spawn(move || {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        let hwnd = HWND(hwnd_addr as *mut std::ffi::c_void);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if topmost_paused() {
                continue;
            }
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                );
            }
        }
    });
}

#[cfg(not(windows))]
fn spawn_topmost_refresh(_window: &tauri::WebviewWindow) {}

#[cfg(windows)]
fn get_taskbar_height(monitor_height: u32) -> Option<u32> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };

    let mut rect = RECT::default();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() {
        return None;
    }
    let work_h = (rect.bottom - rect.top) as u32;
    if monitor_height > work_h {
        Some(monitor_height - work_h)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn get_taskbar_height(_monitor_height: u32) -> Option<u32> {
    None
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("config.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    config_path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &Settings) {
    if let Some(path) = config_path(app) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = fs::write(path, json);
        }
    }
}

fn update_settings<F: FnOnce(&mut Settings)>(app: &AppHandle, f: F) {
    let state: State<AppState> = app.state();
    let mut s = state.settings.lock().unwrap();
    f(&mut s);
    let cloned = s.clone();
    drop(s);
    save_settings(app, &cloned);
    let _ = app.emit("settings", &cloned);
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state: State<AppState> = app.state();
    let settings = state.settings.lock().unwrap().clone();

    let fish_plus = MenuItem::with_id(app, "fish_plus", "魚を増やす (+1)", true, None::<&str>)?;
    let fish_minus = MenuItem::with_id(app, "fish_minus", "魚を減らす (-1)", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;

    let pixel_2 = CheckMenuItem::with_id(app, "pixel_2", "細 (2)", true, settings.pixel == 2, None::<&str>)?;
    let pixel_3 = CheckMenuItem::with_id(app, "pixel_3", "中 (3)", true, settings.pixel == 3, None::<&str>)?;
    let pixel_4 = CheckMenuItem::with_id(app, "pixel_4", "粗 (4)", true, settings.pixel == 4, None::<&str>)?;
    let pixel_submenu = Submenu::with_items(app, "ピクセルサイズ", true, &[&pixel_2, &pixel_3, &pixel_4])?;

    let transparent = CheckMenuItem::with_id(app, "transparent", "背景透過", true, settings.transparent, None::<&str>)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;

    Menu::with_items(
        app,
        &[
            &fish_plus,
            &fish_minus,
            &sep1,
            &pixel_submenu,
            &transparent,
            &sep2,
            &quit,
        ],
    )
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    resume_topmost();
    match id {
        "fish_plus" => update_settings(app, |s| s.count = (s.count + 1).min(40)),
        "fish_minus" => update_settings(app, |s| s.count = s.count.saturating_sub(1).max(1)),
        "pixel_2" => update_settings(app, |s| s.pixel = 2),
        "pixel_3" => update_settings(app, |s| s.pixel = 3),
        "pixel_4" => update_settings(app, |s| s.pixel = 4),
        "transparent" => update_settings(app, |s| s.transparent = !s.transparent),
        "quit" => app.exit(0),
        _ => {}
    }
}
