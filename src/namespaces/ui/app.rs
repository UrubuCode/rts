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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_FREE(handle: u64) {
    free_entry(handle);
}
