use fltk::app;

use super::store::{UiEntry, alloc_entry, free_entry};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_NEW() -> u64 {
    // App is a ZST in fltk-rs; we store a sentinel entry so the handle
    // remains valid and free-able, but we don't hold the App itself
    // (holding it would borrow UI_STORE during run, blocking all other calls).
    let _ = app::App::default();
    alloc_entry(UiEntry::App)
}

/// Runs the FLTK event loop. Does NOT hold UI_STORE borrow during the loop
/// so widget callbacks can call back into the ui namespace without panic.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_RUN(_handle: u64) {
    let _ = app::App::default().run();
}

/// Non-blocking event tick. Processes pending FLTK events and returns
/// `true` while at least one window is still visible, `false` once all
/// windows have been closed (mirroring `fltk::app::wait`).
///
/// Use as the spine of the main thread when the program needs to interleave
/// UI updates with shared state read from another thread (HTTP server,
/// worker, etc.). Always call this on the same thread that created the
/// widgets — FLTK is single-threaded.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_WAIT(_handle: u64) -> i64 {
    if app::wait() { 1 } else { 0 }
}

/// Same as `app_wait` but blocks up to `timeout_secs` seconds waiting for
/// the next event. Returning `false` here also means "no windows left".
/// Useful to keep the UI thread cheap when there is nothing to redraw.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_WAIT_FOR(_handle: u64, timeout_secs: f64) -> i64 {
    match app::wait_for(timeout_secs) {
        Ok(true) => 1,
        _ => 0,
    }
}

/// Wakes the FLTK event loop from another thread. Safe to call from
/// HTTP handlers / workers — FLTK queues an internal awake message and
/// the next `app_wait` returns immediately.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_AWAKE() {
    app::awake();
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_FREE(handle: u64) {
    free_entry(handle);
}
