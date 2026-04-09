use std::collections::BTreeMap;
use std::sync::Arc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

fn colorref(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

use crate::namespaces::state::{self, Mutex};

// ---------------------------------------------------------------------------
// Window state — stores raw isize for Send+Sync safety
// ---------------------------------------------------------------------------

struct WindowEntry {
    hwnd_raw: isize,
}

impl WindowEntry {
    fn hwnd(&self) -> HWND {
        HWND(self.hwnd_raw)
    }
}

struct WindowState {
    windows: BTreeMap<u64, WindowEntry>,
    next_id: u64,
    class_registered: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            windows: BTreeMap::new(),
            next_id: 0,
            class_registered: false,
        }
    }
}

fn lock_win() -> std::sync::MutexGuard<'static, WindowState> {
    let state = Mutex.get_or_init("window", std::sync::Mutex::new(WindowState::default()));
    let leaked: &'static std::sync::Mutex<WindowState> = unsafe { &*Arc::as_ptr(&state) };
    state::lock_or_recover(leaked)
}

fn alloc_window(hwnd: HWND) -> u64 {
    let mut state = lock_win();
    state.next_id = state.next_id.saturating_add(1);
    let id = state.next_id;
    state.windows.insert(id, WindowEntry { hwnd_raw: hwnd.0 });
    id
}

// ---------------------------------------------------------------------------
// Win32 window class + wndproc
// ---------------------------------------------------------------------------

const CLASS_NAME: PCWSTR = w!("RtsWindowClass");

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn ensure_class_registered() -> Result<(), String> {
    let mut state = lock_win();
    if state.class_registered {
        return Ok(());
    }

    unsafe {
        let hinstance = GetModuleHandleW(None)
            .map_err(|e| format!("GetModuleHandle: {e:?}"))?;

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };

        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            return Err("RegisterClassExW failed".to_string());
        }

        state.class_registered = true;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Public operations
// ---------------------------------------------------------------------------

pub fn create(title: &str, width: i32, height: i32) -> Result<u64, String> {
    ensure_class_registered()?;

    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let hinstance = GetModuleHandleW(None)
            .map_err(|e| format!("GetModuleHandle: {e:?}"))?;

        // Adjust for window chrome (title bar, borders) so client area is exactly width x height
        let style = WS_OVERLAPPEDWINDOW;
        let mut rect = RECT { left: 0, top: 0, right: width, bottom: height };
        let _ = AdjustWindowRectEx(&mut rect, style, false, WINDOW_EX_STYLE::default());
        let adj_w = rect.right - rect.left;
        let adj_h = rect.bottom - rect.top;

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            PCWSTR(title_wide.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            adj_w,
            adj_h,
            HWND::default(),
            HMENU::default(),
            hinstance,
            None,
        );

        if hwnd.0 == 0 {
            return Err("CreateWindowExW failed".to_string());
        }

        Ok(alloc_window(hwnd))
    }
}

pub fn show(window_id: u64) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.show: invalid handle".to_string())?;
    unsafe {
        ShowWindow(entry.hwnd(), SW_SHOW);
        let _ = UpdateWindow(entry.hwnd());
    }
    Ok(())
}

pub fn hide(window_id: u64) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.hide: invalid handle".to_string())?;
    unsafe { ShowWindow(entry.hwnd(), SW_HIDE); }
    Ok(())
}

pub fn close(window_id: u64) {
    let mut state = lock_win();
    if let Some(entry) = state.windows.remove(&window_id) {
        unsafe { let _ = DestroyWindow(entry.hwnd()); }
    }
}

pub fn set_title(window_id: u64, title: &str) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.set_title: invalid handle".to_string())?;
    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        SetWindowTextW(entry.hwnd(), PCWSTR(title_wide.as_ptr()))
            .map_err(|e| format!("SetWindowTextW: {e}"))?;
    }
    Ok(())
}

pub fn set_size(window_id: u64, width: i32, height: i32) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.set_size: invalid handle".to_string())?;
    unsafe {
        SetWindowPos(entry.hwnd(), HWND::default(), 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER)
            .map_err(|e| format!("SetWindowPos: {e}"))?;
    }
    Ok(())
}

pub fn poll_event() -> String {
    unsafe {
        let mut msg = MSG::default();
        if PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);

            match msg.message {
                WM_QUIT => "close".to_string(),
                WM_SIZE => {
                    let lparam = msg.lParam.0 as usize;
                    let w = lparam & 0xFFFF;
                    let h = (lparam >> 16) & 0xFFFF;
                    format!("resize:{w}x{h}")
                }
                WM_KEYDOWN => format!("keydown:{}", msg.wParam.0),
                WM_KEYUP => format!("keyup:{}", msg.wParam.0),
                WM_MOUSEMOVE => {
                    let lparam = msg.lParam.0 as usize;
                    let x = lparam & 0xFFFF;
                    let y = (lparam >> 16) & 0xFFFF;
                    format!("mousemove:{x},{y}")
                }
                WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
                    let lparam = msg.lParam.0 as usize;
                    let x = lparam & 0xFFFF;
                    let y = (lparam >> 16) & 0xFFFF;
                    let btn = match msg.message {
                        WM_LBUTTONDOWN => "left",
                        WM_RBUTTONDOWN => "right",
                        _ => "middle",
                    };
                    format!("mousedown:{x},{y},{btn}")
                }
                WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
                    let lparam = msg.lParam.0 as usize;
                    let x = lparam & 0xFFFF;
                    let y = (lparam >> 16) & 0xFFFF;
                    let btn = match msg.message {
                        WM_LBUTTONUP => "left",
                        WM_RBUTTONUP => "right",
                        _ => "middle",
                    };
                    format!("mouseup:{x},{y},{btn}")
                }
                _ => "none".to_string(),
            }
        } else {
            "none".to_string()
        }
    }
}

pub fn fill_rect(window_id: u64, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.fill_rect: invalid handle".to_string())?;
    unsafe {
        let hdc = GetDC(entry.hwnd());
        let brush = CreateSolidBrush(colorref(r, g, b));
        let rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
        ReleaseDC(entry.hwnd(), hdc);
    }
    Ok(())
}

pub fn draw_text(window_id: u64, text: &str, x: i32, y: i32, r: u8, g: u8, b: u8) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.draw_text: invalid handle".to_string())?;
    let mut text_wide: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let hdc = GetDC(entry.hwnd());
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colorref(r, g, b));
        let mut rect = RECT { left: x, top: y, right: x + 1000, bottom: y + 50 };
        DrawTextW(hdc, &mut text_wide, &mut rect, DT_LEFT | DT_TOP | DT_NOCLIP);
        ReleaseDC(entry.hwnd(), hdc);
    }
    Ok(())
}

pub fn set_pixel(window_id: u64, x: i32, y: i32, r: u8, g: u8, b: u8) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.set_pixel: invalid handle".to_string())?;
    unsafe {
        let hdc = GetDC(entry.hwnd());
        SetPixel(hdc, x, y, colorref(r, g, b));
        ReleaseDC(entry.hwnd(), hdc);
    }
    Ok(())
}

pub fn clear(window_id: u64, r: u8, g: u8, b: u8) -> Result<(), String> {
    let state = lock_win();
    let entry = state.windows.get(&window_id)
        .ok_or_else(|| "window.clear: invalid handle".to_string())?;
    unsafe {
        let hdc = GetDC(entry.hwnd());
        let mut rect = RECT::default();
        GetClientRect(entry.hwnd(), &mut rect);
        let brush = CreateSolidBrush(colorref(r, g, b));
        FillRect(hdc, &rect, brush);
        DeleteObject(brush);
        ReleaseDC(entry.hwnd(), hdc);
    }
    Ok(())
}
