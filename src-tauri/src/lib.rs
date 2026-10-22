//! Dot Launcher — native shell.
//!
//! The UI lives in `../src` (plain HTML/JS). This file only does what the
//! webview can't do on its own:
//!   * persist settings to a JSON file and broadcast changes to both windows
//!   * read Dot Clock's settings so the two widgets can match
//!   * show/hide the settings window
//!   * quit / recentre the widget

use std::{fs, path::PathBuf, sync::Mutex};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

/// The Dot Clock app's `identifier` (see its tauri.conf.json). If you change
/// it there, change it here too or "Match Dot Clock" won't find it.
const WORLD_TIME_ID: &str = "app.worldtime.dotmatrix";

struct Store {
    path: PathBuf,
    json: Mutex<String>,
}

/// Dot Clock's saved settings JSON, or `None` if it isn't installed / never run.
#[tauri::command]
fn read_world_time(app: AppHandle) -> Option<String> {
    let path = app
        .path()
        .config_dir()
        .ok()?
        .join(WORLD_TIME_ID)
        .join("state.json");
    fs::read_to_string(path).ok()
}

/// Returns the saved settings JSON, or `"null"` on first run.
#[tauri::command]
fn load_state(store: State<'_, Store>) -> String {
    store.json.lock().unwrap().clone()
}

/// Validates, saves (atomically) and broadcasts the settings JSON to every window.
#[tauri::command]
fn save_state(app: AppHandle, store: State<'_, Store>, json: String) -> Result<(), String> {
    serde_json::from_str::<serde_json::Value>(&json).map_err(|e| e.to_string())?;
    let tmp = store.path.with_extension("json.tmp");
    fs::write(&tmp, &json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &store.path).map_err(|e| e.to_string())?;
    *store.json.lock().unwrap() = json.clone();
    app.emit("state-changed", json).map_err(|e| e.to_string())
}

/// Shows the settings window, optionally jumping to a section (e.g. "map").
#[tauri::command]
fn open_settings(app: AppHandle, section: Option<String>) -> Result<(), String> {
    let win = app
        .get_webview_window("settings")
        .ok_or("settings window missing")?;
    win.show().map_err(|e| e.to_string())?;
    let _ = win.unminimize();
    let _ = win.set_focus();
    if let Some(section) = section {
        app.emit_to("settings", "open-section", section)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Moves the widget back to the middle of the screen (rescue if it's lost off-screen).
#[tauri::command]
fn recenter_widget(app: AppHandle) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("main window missing")?;
    win.center().map_err(|e| e.to_string())?;
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
fn quit(app: AppHandle) {
    let _ = app.save_window_state(StateFlags::POSITION);
    app.exit(0);
}

/// Puts the app in the system tray (Windows) / menu bar (macOS) instead of the
/// taskbar / Dock. Left-click the icon to bring the widget back; right-click
/// for the menu.
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Show / hide widget", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let centre = MenuItem::with_id(app, "centre", "Move widget to centre", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &settings, &centre, &sep, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id("tray")
        .tooltip(app.package_info().name.clone())
        .menu(&menu)
        .menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle" => {
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
            // The widget owns the setting, so it flips it and saves.
            "settings" => {
                let _ = open_settings(app.clone(), None);
            }
            "centre" => {
                let _ = recenter_widget(app.clone());
            }
            "quit" => quit(app.clone()),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(w) = tray.app_handle().get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

/// Opens a launcher item (app, file, folder or website) by its id. The target is
/// looked up in the saved settings, so only things you added can be opened.
#[tauri::command]
fn launch(store: State<'_, Store>, id: String) -> Result<(), String> {
    let json = store.json.lock().unwrap().clone();
    let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let target = v["items"]
        .as_array()
        .and_then(|items| items.iter().find(|i| i["id"].as_str() == Some(id.as_str())))
        .and_then(|i| i["target"].as_str())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or("that item isn't in the launcher any more")?;
    open_target(&target)
}

#[cfg(windows)]
fn open_target(target: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // Explorer opens apps, shortcuts (.lnk), files, folders and web links with their usual program.
    std::process::Command::new("explorer.exe")
        .arg(target)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn open_target(target: &str) -> Result<(), String> {
    // `open` handles .app bundles, files, folders and web links.
    let status = std::process::Command::new("/usr/bin/open").arg(target).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("couldn't open {}", target)) }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn open_target(target: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open").arg(target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Shows the system's file picker. `kind`: "app", "file" or "folder". Returns the path, or None if cancelled.
#[tauri::command]
fn pick_path(kind: String) -> Option<String> {
    let d = rfd::FileDialog::new();
    let picked = match kind.as_str() {
        "folder" => d.set_title("Choose a folder").pick_folder(),
        "app" => {
            #[cfg(windows)]
            let d = d
                .set_title("Choose an app (or its shortcut)")
                .add_filter("Apps and shortcuts", &["exe", "lnk", "url", "appref-ms"])
                .set_directory(std::env::var("ProgramData").map(|p| format!("{}\\Microsoft\\Windows\\Start Menu\\Programs", p)).unwrap_or_default());
            #[cfg(target_os = "macos")]
            let d = d.set_title("Choose an app").add_filter("Apps", &["app"]).set_directory("/Applications");
            d.pick_file()
        }
        _ => d.set_title("Choose a file").pick_file(),
    };
    picked.map(|p| p.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Launching the app again just brings the existing widget forward
        // (must be the first plugin registered).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(
            // Remember where the user left the widget.
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION)
                .with_denylist(&["settings"])
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            fs::create_dir_all(&dir)?;
            let path = dir.join("state.json");
            let json = fs::read_to_string(&path)
                .ok()
                .filter(|s| serde_json::from_str::<serde_json::Value>(s).is_ok())
                .unwrap_or_else(|| "null".to_string());
            app.manage(Store {
                path,
                json: Mutex::new(json),
            });
            // The widget starts hidden so it doesn't flash in the centre before the
            // window-state plugin moves it back to where the user left it.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }
            // Live in the tray / menu bar rather than the taskbar / Dock.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                match window.label() {
                    // Closing settings just hides it so it opens instantly next time.
                    "settings" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    // Closing the widget quits the app.
                    "main" => {
                        let app = window.app_handle();
                        let _ = app.save_window_state(StateFlags::POSITION);
                        app.exit(0);
                    }
                    _ => {}
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_state,
            save_state,
            read_world_time,
            launch,
            pick_path,
            open_settings,
            recenter_widget,
            quit
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dot Launcher");
}
