use std::fs;
use std::path::PathBuf;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(target_os = "windows")]
use windows::core::{w, BOOL};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAKED};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::MapWindowPoints;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, EnumChildWindows, EnumWindows, FindWindowExW, FindWindowW, GetClassNameW,
    GetParent, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindow,
    IsWindowVisible, SendMessageTimeoutW, SetLayeredWindowAttributes, SetParent,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, GWLP_WNDPROC,
    HTTRANSPARENT, HWND_BOTTOM, LWA_ALPHA, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_RESTORE, SW_SHOW, WM_NCHITTEST, WM_SIZE, WNDPROC, WS_CHILD,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TRANSPARENT, WS_POPUP,
};

#[cfg(target_os = "windows")]
static ORIGINAL_WNDPROC: Mutex<Option<isize>> = Mutex::new(None);
#[cfg(target_os = "windows")]
static LOCKED_HIT_TEST: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static WINDOW_LOCKED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static CURRENT_WINDOW_STATE: Mutex<Option<(u32, u32, i32, i32, bool)>> = Mutex::new(None);
#[cfg(target_os = "windows")]
static ATTACHED_TO_DESKTOP: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static ORIGINAL_WINDOW_STYLE: Mutex<Option<(isize, isize)>> = Mutex::new(None);
#[cfg(target_os = "windows")]
static ATTACHED_WEBVIEW_HWND: Mutex<Option<isize>> = Mutex::new(None);
#[cfg(target_os = "windows")]
static ATTACHED_WEBVIEW_RECT: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

#[cfg(target_os = "windows")]
fn window_locked() -> bool {
    WINDOW_LOCKED.load(Ordering::Relaxed)
}

#[cfg(not(target_os = "windows"))]
fn window_locked() -> bool {
    false
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn locked_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_SIZE && window_locked() && wparam.0 == 1 {
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_NOTOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
        };
        let _ = ShowWindow(hwnd, SW_RESTORE);
        if let Ok(guard) = CURRENT_WINDOW_STATE.lock() {
            if let Some((width, height, x, y, _collapsed)) = *guard {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_NOTOPMOST),
                    x,
                    y,
                    width as i32,
                    height as i32,
                    SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE,
                );
            }
        }
        return LRESULT(0);
    }

    if msg == WM_NCHITTEST && LOCKED_HIT_TEST.load(Ordering::Relaxed) {
        return LRESULT(HTTRANSPARENT as isize);
    }

    let old = ORIGINAL_WNDPROC.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).unwrap_or(0);
    if old == 0 {
        return LRESULT(0);
    }
    let old_proc: WNDPROC = std::mem::transmute(old);
    CallWindowProcW(old_proc, hwnd, msg, wparam, lparam)
}

#[cfg(target_os = "windows")]
fn install_wnd_proc(window: &WebviewWindow) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    unsafe {
        let mut guard = ORIGINAL_WNDPROC.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_none() {
            let old = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
            if old == 0 {
                return Err("failed to read original window procedure".to_string());
            }
            *guard = Some(old);
            let _ = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, locked_wnd_proc as *const () as isize);
        }
    }
    Ok(())
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

#[cfg(target_os = "windows")]
fn set_window_layer(window: &WebviewWindow, topmost: bool) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE,
        SWP_NOMOVE, SWP_NOSIZE,
    };

    unsafe {
        if let Ok(hwnd) = window.hwnd() {
            let _ = SetWindowPos(
                hwnd,
                Some(if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST }),
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
fn set_window_layer(_window: &WebviewWindow, _topmost: bool) {}

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
        .map_err(|error| error.to_string())?;

    if !window_locked() {
        if collapsed {
            set_window_layer(&window, true);
        } else {
            place_behind_apps(&window);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(mut guard) = CURRENT_WINDOW_STATE.lock() {
            *guard = Some((width, height, x, work.position.y, collapsed));
        }
    }
    #[cfg(target_os = "windows")]
    if window_locked() {
        assert_desktop_attached(&window);
    } else {
        restore_outer_window(&window);
    }
    Ok(())
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
#[derive(Clone, Copy)]
struct DesktopTarget {
    progman: HWND,
    shell_def_view: HWND,
    worker_w: HWND,
    raised: bool,
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_desktop_targets(window: HWND, param: LPARAM) -> BOOL {
    let shell_def_view = FindWindowExW(Some(window), None, w!("SHELLDLL_DefView"), None)
        .unwrap_or_default();
    if shell_def_view != HWND::default() {
        let worker_w =
            FindWindowExW(None, Some(window), w!("WorkerW"), None).unwrap_or_default();
        let targets = &mut *(param.0 as *mut (HWND, HWND));
        targets.0 = shell_def_view;
        targets.1 = worker_w;
        return BOOL(0);
    }
    BOOL(1)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_first_child(window: HWND, param: LPARAM) -> BOOL {
    let target = &mut *(param.0 as *mut HWND);
    if *target == HWND::default() {
        *target = window;
    }
    BOOL(0)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_webview_child(window: HWND, param: LPARAM) -> BOOL {
    let state = &mut *(param.0 as *mut (HWND, u32));
    if state.0 != HWND::default() {
        return BOOL(0);
    }
    let mut class_name = [0u16; 256];
    let len = GetClassNameW(window, &mut class_name);
    if len > 0 {
        let class = String::from_utf16_lossy(&class_name[..len as usize]);
        if class == "WRY_WEBVIEW" || class == "Chrome_WidgetWin_0" {
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid == state.1 {
                state.0 = window;
                return BOOL(0);
            }
        }
    }
    BOOL(1)
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_webview_top(window: HWND, param: LPARAM) -> BOOL {
    let state = &mut *(param.0 as *mut (HWND, u32));
    if state.0 == HWND::default() {
        let _ = EnumChildWindows(
            Some(window),
            Some(enum_webview_child),
            LPARAM(state as *mut (HWND, u32) as isize),
        );
    }
    BOOL(if state.0 == HWND::default() { 1 } else { 0 })
}

#[cfg(target_os = "windows")]
fn find_webview_host_anywhere() -> Option<HWND> {
    unsafe {
        let mut state = (HWND::default(), std::process::id());
        let _ = EnumWindows(
            Some(enum_webview_top),
            LPARAM(&mut state as *mut (HWND, u32) as isize),
        );
        if state.0 == HWND::default() {
            None
        } else {
            Some(state.0)
        }
    }
}

#[cfg(target_os = "windows")]
fn webview_host_hwnd(window: &WebviewWindow) -> Result<HWND, String> {
    let stored = ATTACHED_WEBVIEW_HWND
        .lock()
        .map(|state| state.unwrap_or_default())
        .unwrap_or_default();
    if stored != 0 {
        let hwnd = HWND(stored as *mut c_void);
        if unsafe { IsWindow(Some(hwnd)).as_bool() } {
            return Ok(hwnd);
        }
    }

    let outer = window.hwnd().map_err(|error| error.to_string())?;
    unsafe {
        let wry = FindWindowExW(Some(outer), None, w!("WRY_WEBVIEW"), None).unwrap_or_default();
        if wry != HWND::default() {
            if let Ok(mut guard) = ATTACHED_WEBVIEW_HWND.lock() {
                *guard = Some(wry.0 as isize);
            }
            return Ok(wry);
        }
        let chrome =
            FindWindowExW(Some(outer), None, w!("Chrome_WidgetWin_0"), None).unwrap_or_default();
        if chrome != HWND::default() {
            if let Ok(mut guard) = ATTACHED_WEBVIEW_HWND.lock() {
                *guard = Some(chrome.0 as isize);
            }
            return Ok(chrome);
        }
        let mut first = HWND::default();
        let _ = EnumChildWindows(
            Some(outer),
            Some(enum_first_child),
            LPARAM(&mut first as *mut HWND as isize),
        );
        if first != HWND::default() {
            if let Ok(mut guard) = ATTACHED_WEBVIEW_HWND.lock() {
                *guard = Some(first.0 as isize);
            }
            Ok(first)
        } else {
            if let Some(hwnd) = find_webview_host_anywhere() {
                if let Ok(mut guard) = ATTACHED_WEBVIEW_HWND.lock() {
                    *guard = Some(hwnd.0 as isize);
                }
                Ok(hwnd)
            } else {
                Err("WebView2 host window not found".to_string())
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn send_desktop_message(progman: HWND) {
    unsafe {
        let _ = SendMessageTimeoutW(
            progman,
            0x052C,
            WPARAM(0xD),
            LPARAM(0x1),
            SMTO_NORMAL,
            1000,
            None,
        );
    }
}

#[cfg(target_os = "windows")]
fn probe_desktop_targets() -> Result<DesktopTarget, String> {
    unsafe {
        let progman = FindWindowW(w!("Progman"), None).map_err(|error| error.to_string())?;
        if progman == HWND::default() {
            return Err("Progman window not found".to_string());
        }

        let raised = (GetWindowLongPtrW(progman, GWL_EXSTYLE)
            & (WS_EX_NOREDIRECTIONBITMAP.0 as isize))
            != 0;

        let (shell_def_view, worker_w) = if raised {
            (
                FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None)
                    .unwrap_or_default(),
                FindWindowExW(Some(progman), None, w!("WorkerW"), None).unwrap_or_default(),
            )
        } else {
            let mut targets = (HWND::default(), HWND::default());
            let _ = EnumWindows(
                Some(enum_desktop_targets),
                LPARAM(&mut targets as *mut (HWND, HWND) as isize),
            );
            targets
        };

        Ok(DesktopTarget {
            progman,
            shell_def_view,
            worker_w,
            raised,
        })
    }
}

#[cfg(target_os = "windows")]
fn find_desktop_targets() -> Result<DesktopTarget, String> {
    let target = probe_desktop_targets()?;
    let needs_worker_w = if target.raised {
        target.shell_def_view == HWND::default() || target.worker_w == HWND::default()
    } else {
        target.worker_w == HWND::default()
    };
    if needs_worker_w {
        send_desktop_message(target.progman);
        probe_desktop_targets()
    } else {
        Ok(target)
    }
}

#[cfg(target_os = "windows")]
fn uncloak_window(hwnd: HWND) {
    let value: u32 = 0;
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &value as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

#[cfg(target_os = "windows")]
fn restore_outer_window(window: &WebviewWindow) {
    if let Ok(outer) = window.hwnd() {
        uncloak_window(outer);
        if unsafe { IsIconic(outer).as_bool() } {
            let _ = unsafe { ShowWindow(outer, SW_RESTORE) };
        }
    }
}

#[cfg(target_os = "windows")]
fn reposition_desktop_window(window: &WebviewWindow) {
    let stored = ATTACHED_WEBVIEW_RECT
        .lock()
        .map(|guard| *guard)
        .unwrap_or(None);
    let (width, height, x, y): (i32, i32, i32, i32) = if let Some((x, y, width, height)) = stored {
        (width, height, x, y)
    } else if let Some(state) = CURRENT_WINDOW_STATE
        .lock()
        .map(|state| state.map(|(width, height, x, y, _)| (width, height, x, y)))
        .unwrap_or(None)
    {
        (state.0 as i32, state.1 as i32, state.2, state.3)
    } else if let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) {
        (
            size.width as i32,
            size.height as i32,
            position.x,
            position.y,
        )
    } else {
        return;
    };

    let Ok(hwnd) = webview_host_hwnd(window) else {
        return;
    };
    let parent = unsafe { GetParent(hwnd).unwrap_or_default() };
    if parent == HWND::default() {
        return;
    }

    let mut point = POINT { x, y };
    unsafe {
        MapWindowPoints(
            None,
            Some(parent),
            std::slice::from_mut(&mut point),
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            point.x,
            point.y,
            width as i32,
            height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
    }
}

#[cfg(target_os = "windows")]
fn attach_window_to_desktop(window: &WebviewWindow) -> Result<(), String> {
    // Raised desktop (24H2+) hangs the window under Progman's DefView;
    // older Windows uses the WorkerW wallpaper host instead.
    if ATTACHED_TO_DESKTOP.load(Ordering::Relaxed) {
        return Ok(());
    }
    let hwnd = webview_host_hwnd(window)?;
    let target = find_desktop_targets()?;

    let mut child_rect = RECT::default();
    let has_rect = unsafe { GetWindowRect(hwnd, &mut child_rect).is_ok() }
        && child_rect.right > child_rect.left
        && child_rect.bottom > child_rect.top;
    let (width, height, x, y): (i32, i32, i32, i32) = if has_rect {
        (
            child_rect.right - child_rect.left,
            child_rect.bottom - child_rect.top,
            child_rect.left,
            child_rect.top,
        )
    } else if let Some(state) = CURRENT_WINDOW_STATE
        .lock()
        .map(|state| state.map(|(width, height, x, y, _)| (width, height, x, y)))
        .unwrap_or(None)
    {
        (state.0 as i32, state.1 as i32, state.2, state.3)
    } else {
        let position = window.outer_position().map_err(|error| error.to_string())?;
        let size = window.outer_size().map_err(|error| error.to_string())?;
        (
            size.width as i32,
            size.height as i32,
            position.x,
            position.y,
        )
    };
    if has_rect {
        if let Ok(mut guard) = ATTACHED_WEBVIEW_RECT.lock() {
            *guard = Some((x, y, width, height));
        }
    }

    unsafe {
        let mut original = ORIGINAL_WINDOW_STYLE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if original.is_none() {
            *original = Some((
                GetWindowLongPtrW(hwnd, GWL_STYLE),
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
            ));
        }
        drop(original);

        if target.raised {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let child_style = (style | WS_CHILD.0 as isize) & !(WS_POPUP.0 as isize);
            let layered_style = ex_style | WS_EX_LAYERED.0 as isize;
            SetWindowLongPtrW(hwnd, GWL_STYLE, child_style);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, layered_style);
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            let _ = SetParent(hwnd, Some(target.progman));
        } else {
            if target.worker_w == HWND::default() {
                return Err("WorkerW desktop layer not found".to_string());
            }
            let _ = SetParent(hwnd, Some(target.worker_w));
        }
    }

    let parent = unsafe { GetParent(hwnd).unwrap_or_default() };
    let expected = if target.raised {
        target.progman
    } else {
        target.worker_w
    };
    if parent != expected {
        return Err("failed to attach window to desktop layer".to_string());
    }

    if target.raised {
        if target.shell_def_view != HWND::default() {
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    Some(target.shell_def_view),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
        }
        if target.worker_w != HWND::default() {
            let _ = unsafe {
                SetWindowPos(
                    target.worker_w,
                    Some(HWND_BOTTOM),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
        }
    }

    let mut point = POINT { x, y };
    unsafe {
        MapWindowPoints(
            None,
            Some(parent),
            std::slice::from_mut(&mut point),
        );
        let _ = SetWindowPos(
            hwnd,
            None,
            point.x,
            point.y,
            width as i32,
            height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
    }

    ATTACHED_TO_DESKTOP.store(true, Ordering::Relaxed);
    Ok(())
}

#[cfg(target_os = "windows")]
fn detach_window_from_desktop(window: &WebviewWindow) -> Result<(), String> {
    if !ATTACHED_TO_DESKTOP.load(Ordering::Relaxed) {
        return Ok(());
    }
    let outer = window.hwnd().map_err(|error| error.to_string())?;
    let hwnd = webview_host_hwnd(window)?;

    unsafe {
        let _ = SetParent(hwnd, Some(outer));
    }
    if unsafe { GetParent(hwnd).unwrap_or_default() } != outer {
        return Err("failed to detach window from desktop layer".to_string());
    }

    unsafe {
        let original = ORIGINAL_WINDOW_STYLE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    if let Some((style, ex_style)) = original {
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
        } else {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) & !(WS_CHILD.0 as isize);
            let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE)
                & !(WS_EX_LAYERED.0 as isize)
                & !(WS_EX_TRANSPARENT.0 as isize)
                & !(WS_EX_NOACTIVATE.0 as isize);
            SetWindowLongPtrW(hwnd, GWL_STYLE, style | WS_POPUP.0 as isize);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
        }
        let stored_rect = ATTACHED_WEBVIEW_RECT
            .lock()
            .map(|guard| *guard)
            .unwrap_or(None);
        if let Some((sx, sy, sw, sh)) = stored_rect {
            let mut point = POINT { x: sx, y: sy };
            MapWindowPoints(
                None,
                Some(outer),
                std::slice::from_mut(&mut point),
            );
            let _ = SetWindowPos(
                hwnd,
                None,
                point.x,
                point.y,
                sw,
                sh,
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
    }

    if let Ok(mut guard) = ATTACHED_WEBVIEW_RECT.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = ATTACHED_WEBVIEW_HWND.lock() {
        *guard = None;
    }
    ATTACHED_TO_DESKTOP.store(false, Ordering::Relaxed);
    Ok(())
}

#[cfg(target_os = "windows")]
fn assert_desktop_attached(window: &WebviewWindow) {
    if !window_locked() {
        return;
    }
    restore_outer_window(window);
    if !ATTACHED_TO_DESKTOP.load(Ordering::Relaxed) {
        let _ = attach_window_to_desktop(window);
        return;
    }

    let Ok(hwnd) = webview_host_hwnd(window) else {
        return;
    };
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return;
    }

    let Ok(target) = probe_desktop_targets() else {
        return;
    };
    let parent = unsafe { GetParent(hwnd).unwrap_or_default() };
    let expected = if target.raised {
        target.progman
    } else {
        target.worker_w
    };

    if parent != expected {
        let _ = attach_window_to_desktop(window);
        return;
    }

    if target.raised {
        if target.shell_def_view != HWND::default() {
            let _ = unsafe {
                SetWindowPos(
                    hwnd,
                    Some(target.shell_def_view),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
        }
        if target.worker_w != HWND::default() {
            let _ = unsafe {
                SetWindowPos(
                    target.worker_w,
                    Some(HWND_BOTTOM),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                )
            };
        }
    }

    reposition_desktop_window(window);
    if !unsafe { IsWindowVisible(hwnd).as_bool() } {
        let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };
    }
}

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

#[cfg(target_os = "windows")]
fn apply_locked_state(window: &WebviewWindow, locked: bool) -> Result<(), String> {
    if locked {
        if attach_window_to_desktop(window).is_err() {
            place_behind_apps(window);
        }
        window
            .set_ignore_cursor_events(true)
            .map_err(|error| error.to_string())?;
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE)
                    | WS_EX_TRANSPARENT.0 as isize
                    | WS_EX_NOACTIVATE.0 as isize;
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex_style);
            }
        }
        let _ = window.hide();
        LOCKED_HIT_TEST.store(true, Ordering::Relaxed);
        WINDOW_LOCKED.store(true, Ordering::Relaxed);
    } else {
        detach_window_from_desktop(window)?;
        restore_outer_window(window);
        let _ = window.show();
        let _ = window.unminimize();
        window
            .set_ignore_cursor_events(false)
            .map_err(|error| error.to_string())?;
        LOCKED_HIT_TEST.store(false, Ordering::Relaxed);
        WINDOW_LOCKED.store(false, Ordering::Relaxed);
        let collapsed = CURRENT_WINDOW_STATE
            .lock()
            .map(|state| state.map(|(_, _, _, _, value)| value).unwrap_or(false))
            .unwrap_or(false);
        if collapsed {
            set_window_layer(window, true);
        } else {
            place_behind_apps(window);
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn set_locked(_app: AppHandle, window: WebviewWindow, locked: bool) -> Result<(), String> {
    apply_locked_state(&window, locked)
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn set_locked(_app: AppHandle, _window: WebviewWindow, _locked: bool) -> Result<(), String> {
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
                let _ = install_wnd_proc(&window);
                place_behind_apps(&window);
                let monitor_window = window.clone();
                #[cfg(target_os = "windows")]
                std::thread::spawn(move || loop {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    if window_locked() {
                        assert_desktop_attached(&monitor_window);
                    } else {
                        if !monitor_window.is_visible().unwrap_or(true) {
                            let _ = monitor_window.show();
                        }
                        let collapsed = CURRENT_WINDOW_STATE
                            .lock()
                            .map(|state| state.map(|(_, _, _, _, value)| value).unwrap_or(false))
                            .unwrap_or(false);
                        if collapsed {
                            set_window_layer(&monitor_window, true);
                        } else {
                            place_behind_apps(&monitor_window);
                        }
                    }
                });
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

            let unlock_item = MenuItem::with_id(app, "unlock", "解锁", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&unlock_item, &quit_item])?;

            let mut tray_builder = TrayIconBuilder::with_id("main")
                .tooltip("待办桌面助手")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "unlock" => {
                        if let Some(window) = app.get_webview_window("main") {
                            #[cfg(target_os = "windows")]
                            let _ = apply_locked_state(&window, false);
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.emit("tray-unlock", ());
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
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
