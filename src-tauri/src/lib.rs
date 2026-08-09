use std::fs;
use std::path::PathBuf;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::ScreenToClient;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GetClientRect, GetCursorPos, GetWindowLongPtrW, SetWindowLongPtrW,
    GWLP_WNDPROC, HTCLIENT, HTTRANSPARENT, WM_NCHITTEST, WNDPROC,
};

#[cfg(target_os = "windows")]
static ORIGINAL_WNDPROC: Mutex<Option<isize>> = Mutex::new(None);
#[cfg(target_os = "windows")]
static LOCKED_HIT_TEST: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
unsafe extern "system" fn locked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_NCHITTEST && LOCKED_HIT_TEST.load(Ordering::Relaxed) {
        let mut point = POINT::default();
        if GetCursorPos(&mut point).is_ok() {
            let mut client = point;
            if ScreenToClient(hwnd, &mut client).as_bool() {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                let unlock_area = (client.x >= width - 180 && client.y <= 64)
                    || (width <= 32)
                    || (height <= 32);
                if unlock_area {
                    return LRESULT(HTCLIENT as isize);
                }
                return LRESULT(HTTRANSPARENT as isize);
            }
        }
        return LRESULT(HTTRANSPARENT as isize);
    }

    let old = ORIGINAL_WNDPROC.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).unwrap_or(0);
    if old == 0 {
        return LRESULT(0);
    }
    let old_proc: WNDPROC = std::mem::transmute(old);
    CallWindowProcW(old_proc, hwnd, msg, wparam, lparam)
}

fn data_path(app: &AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("failed to resolve app data dir")
        .join("data.json")
}

#[tauri::command]
fn load_data(app: AppHandle) -> String {
    fs::read_to_string(data_path(&app)).unwrap_or_default()
}

#[tauri::command]
fn save_data(app: AppHandle, contents: String) -> Result<(), String> {
    let path = data_path(&app);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if path.exists() {
        let backup = path.with_extension("backup.json");
        fs::copy(&path, &backup).map_err(|error| error.to_string())?;
    }
    fs::write(&path, contents).map_err(|error| error.to_string())
}

fn place_window(window: &WebviewWindow, width: u32, dock: &str) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "no monitor found".to_string())?;
    let work = monitor.work_area();
    let height = if dock == "top" {
        (work.size.height as u32).min(680)
    } else {
        work.size.height as u32
    };
    let x = work.position.x + work.size.width as i32 - width as i32;
    window
        .set_position(PhysicalPosition::new(x, work.position.y))
        .map_err(|error| error.to_string())?;
    window
        .set_size(PhysicalSize::new(width, height))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_window_width(window: WebviewWindow, width: u32, dock: String) -> Result<(), String> {
    place_window(&window, width, &dock)
}

#[tauri::command]
fn apply_dock(window: WebviewWindow, dock: String) -> Result<(), String> {
    place_window(&window, 320, &dock)
}

#[tauri::command]
fn apply_window_state(
    window: WebviewWindow,
    collapsed: bool,
    dock: String,
    direction: String,
) -> Result<(), String> {
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "no monitor found".to_string())?;
    let work = monitor.work_area();
    let full_height = if dock == "top" {
        (work.size.height as u32).min(680)
    } else {
        work.size.height as u32
    };
    let (width, height) = if collapsed && direction == "top" {
        (320, 16)
    } else if collapsed {
        (16, full_height)
    } else {
        (320, full_height)
    };
    let x = work.position.x + work.size.width as i32 - width as i32;
    window
        .set_position(PhysicalPosition::new(x, work.position.y))
        .map_err(|error| error.to_string())?;
    window
        .set_size(PhysicalSize::new(width, height))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn notify(app: AppHandle, title: String, body: String) {
    let _ = app.notification().builder().title(title).body(body).show();
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|error| error.to_string())
    } else {
        manager.disable().map_err(|error| error.to_string())
    }
}

#[tauri::command]
fn autostart_enabled(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn enable_tool_window(window: &WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_TOOLWINDOW.0 as i32);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn enable_tool_window(_window: &WebviewWindow) {}

#[cfg(target_os = "windows")]
fn place_behind_apps(window: &WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetDesktopWindow, GetWindow, IsWindowVisible, SetWindowPos, GW_CHILD,
        GW_HWNDNEXT, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    unsafe {
        if let Ok(hwnd) = window.hwnd() {
            let mut last_regular = None;
            let mut current = GetWindow(GetDesktopWindow(), GW_CHILD).unwrap_or_default();
            while current != HWND::default() {
                if current != hwnd {
                    let mut class_name = [0u16; 256];
                    let len = GetClassNameW(current, &mut class_name);
                    if len > 0 {
                        let class = String::from_utf16_lossy(&class_name[..len as usize]);
                        if class != "Progman"
                            && class != "WorkerW"
                            && IsWindowVisible(current).as_bool()
                        {
                            last_regular = Some(current);
                        }
                    }
                }
                current = GetWindow(current, GW_HWNDNEXT).unwrap_or_default();
            }

            let insert_after = last_regular;
            let _ = SetWindowPos(
                hwnd,
                insert_after,
                0,
                0,
                0,
                0,
                SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn place_behind_apps(_window: &WebviewWindow) {}

fn toggle_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(true) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg(target_os = "windows")]
#[tauri::command]
fn set_locked(window: WebviewWindow, locked: bool) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    unsafe {
        let mut guard = ORIGINAL_WNDPROC.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if locked {
            if guard.is_none() {
                let old = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
                if old == 0 {
                    return Err("failed to read original window procedure".to_string());
                }
                *guard = Some(old);
                let _ = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, locked_wnd_proc as *const () as isize);
            }
            LOCKED_HIT_TEST.store(true, Ordering::Relaxed);
        } else {
            LOCKED_HIT_TEST.store(false, Ordering::Relaxed);
            if let Some(old) = guard.take() {
                let _ = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, old);
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn set_locked(_window: WebviewWindow, _locked: bool) -> Result<(), String> {
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_decorations(false);
                let _ = window.set_skip_taskbar(true);
                enable_tool_window(&window);
                place_behind_apps(&window);
                if let Ok(Some(monitor)) = window.current_monitor() {
                    let work = monitor.work_area();
                    let width = 320u32;
                    let height = work.size.height as u32;
                    let x = work.position.x + work.size.width as i32 - width as i32;
                    let _ = window
                        .set_position(PhysicalPosition::new(x, work.position.y));
                    let _ = window.set_size(PhysicalSize::new(width, height));
                }
            }

            let toggle_item = MenuItem::with_id(app, "toggle", "显示/隐藏", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::with_id("main")
                .tooltip("每日任务侧边栏")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "toggle" => toggle_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_window(tray.app_handle());
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }
            tray_builder.build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_data,
            save_data,
            set_window_width,
            apply_dock,
            apply_window_state,
            notify,
            set_autostart,
            autostart_enabled,
            set_locked
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
