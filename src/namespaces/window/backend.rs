use std::collections::BTreeMap;
use std::sync::Arc;

use minifb::{Key, Window, WindowOptions};

use crate::namespaces::state::{self, Mutex};

// ---------------------------------------------------------------------------
// Window state
// ---------------------------------------------------------------------------

struct WindowEntry {
    window: Window,
    buffer: Vec<u32>, // 0x00RRGGBB per pixel
    width: usize,
    height: usize,
}

// SAFETY: Window is used only from the main thread. The Mutex serializes access.
unsafe impl Send for WindowEntry {}
unsafe impl Sync for WindowEntry {}

struct WindowState {
    windows: BTreeMap<u64, WindowEntry>,
    next_id: u64,
}

impl Default for WindowState {
    fn default() -> Self {
        Self { windows: BTreeMap::new(), next_id: 0 }
    }
}

fn lock_win() -> std::sync::MutexGuard<'static, WindowState> {
    let state = Mutex.get_or_init("window", std::sync::Mutex::new(WindowState::default()));
    let leaked: &'static std::sync::Mutex<WindowState> = unsafe { &*Arc::as_ptr(&state) };
    state::lock_or_recover(leaked)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[inline]
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn create(title: &str, width: usize, height: usize) -> Result<u64, String> {
    let window = Window::new(
        title,
        width,
        height,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    ).map_err(|e| format!("window.create: {e}"))?;

    let mut state = lock_win();
    state.next_id += 1;
    let id = state.next_id;
    state.windows.insert(id, WindowEntry {
        window,
        buffer: vec![0u32; width * height],
        width,
        height,
    });
    Ok(id)
}

pub fn show(_id: u64) -> Result<(), String> {
    // minifb windows are visible on creation
    Ok(())
}

pub fn hide(_id: u64) -> Result<(), String> {
    // minifb doesn't support hide/show toggle
    Ok(())
}

pub fn close(id: u64) {
    let mut state = lock_win();
    state.windows.remove(&id);
    // Drop closes the window
}

pub fn set_title(id: u64, title: &str) -> Result<(), String> {
    let mut state = lock_win();
    let e = state.windows.get_mut(&id).ok_or("invalid handle")?;
    e.window.set_title(title);
    Ok(())
}

pub fn set_size(id: u64, width: usize, height: usize) -> Result<(), String> {
    let mut state = lock_win();
    let e = state.windows.get_mut(&id).ok_or("invalid handle")?;
    e.width = width;
    e.height = height;
    e.buffer.resize(width * height, 0);
    Ok(())
}

pub fn is_open(id: u64) -> bool {
    let state = lock_win();
    state.windows.get(&id)
        .map(|e| e.window.is_open())
        .unwrap_or(false)
}

pub fn poll_event(id: u64) -> String {
    let mut state = lock_win();
    let Some(e) = state.windows.get_mut(&id) else {
        return "close".to_string();
    };

    if !e.window.is_open() {
        return "close".to_string();
    }

    // Just process events without blitting (present does the blit)
    e.window.update();

    if e.window.is_key_down(Key::Escape) {
        return "keydown:27".to_string();
    }

    if let Some((mx, my)) = e.window.get_mouse_pos(minifb::MouseMode::Clamp) {
        if e.window.get_mouse_down(minifb::MouseButton::Left) {
            return format!("mousedown:{},{},left", mx as i32, my as i32);
        }
        if e.window.get_mouse_down(minifb::MouseButton::Right) {
            return format!("mousedown:{},{},right", mx as i32, my as i32);
        }
    }

    "none".to_string()
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

pub fn clear(id: u64, r: u8, g: u8, b: u8) -> Result<(), String> {
    let mut state = lock_win();
    let e = state.windows.get_mut(&id).ok_or("invalid handle")?;
    let color = rgb(r, g, b);
    e.buffer.fill(color);
    Ok(())
}

pub fn fill_rect(id: u64, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8) -> Result<(), String> {
    let mut state = lock_win();
    let e = state.windows.get_mut(&id).ok_or("invalid handle")?;
    let color = rgb(r, g, b);
    let x0 = x.max(0) as usize;
    let y0 = y.max(0) as usize;
    let x1 = ((x + w) as usize).min(e.width);
    let y1 = ((y + h) as usize).min(e.height);
    for py in y0..y1 {
        let start = py * e.width + x0;
        let end = py * e.width + x1;
        e.buffer[start..end].fill(color);
    }
    Ok(())
}

pub fn set_pixel(id: u64, x: i32, y: i32, r: u8, g: u8, b: u8) -> Result<(), String> {
    let mut state = lock_win();
    let e = state.windows.get_mut(&id).ok_or("invalid handle")?;
    if x >= 0 && y >= 0 && (x as usize) < e.width && (y as usize) < e.height {
        let idx = (y as usize) * e.width + (x as usize);
        e.buffer[idx] = rgb(r, g, b);
    }
    Ok(())
}

pub fn draw_text(_id: u64, _text: &str, _x: i32, _y: i32, _r: u8, _g: u8, _b: u8) -> Result<(), String> {
    // minifb doesn't have text rendering — would need a font rasterizer
    // For now, this is a no-op. Text can be added later with a bitmap font.
    Ok(())
}

pub fn present(id: u64) -> Result<(), String> {
    let mut state = lock_win();
    let e = state.windows.get_mut(&id).ok_or("invalid handle")?;
    e.window.update_with_buffer(&e.buffer, e.width, e.height)
        .map_err(|err| format!("present: {err}"))?;
    Ok(())
}
