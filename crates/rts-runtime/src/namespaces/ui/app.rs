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

/// (#224) Non-blocking poll — processa eventos pendentes sem bloquear.
/// Retorna `true` enquanto a app continuar viva (window aberta), `false`
/// quando o user pediu pra fechar. Usado em loops com trabalho intensivo
/// onde o programa quer manter a UI responsiva sem chamar wait().
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_CHECK(_handle: u64) -> i64 {
    if app::check() { 1 } else { 0 }
}

/// (#224) Agenda `cb` para ser chamada uma vez apos `tm_secs` segundos.
/// `cb` deve ser handle Function (reify de user fn ou arrow). Callback
/// dispara no main thread via FLTK timer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_ADD_TIMEOUT(tm_secs: f64, cb: i64) {
    if cb == 0 {
        return;
    }
    app::add_timeout(tm_secs, move || {
        invoke_no_args(cb);
    });
}

/// (#224) Agenda `cb` para ser chamada periodicamente a cada `tm_secs`
/// segundos. Callback deve re-agendar via `repeat_timeout` se quiser
/// continuar (FLTK semantica) — em RTS v0, fazemos auto-repeat.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_REPEAT_TIMEOUT(tm_secs: f64, cb: i64) {
    if cb == 0 {
        return;
    }
    schedule_repeat(tm_secs, cb);
}

fn schedule_repeat(tm_secs: f64, cb: i64) {
    app::add_timeout(tm_secs, move || {
        invoke_no_args(cb);
        // Auto-reagenda. Em FLTK puro o user chama repeat_timeout no body
        // do callback — aqui fazemos transparente.
        schedule_repeat(tm_secs, cb);
    });
}

/// (#224) Agenda `cb` pra ser chamado quando o event loop fica idle
/// (entre eventos). Util para animacoes e progresso incremental.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_ADD_IDLE(cb: i64) {
    if cb == 0 {
        return;
    }
    app::add_idle(move || {
        invoke_no_args(cb);
    });
}

/// Helper: invoca um handle/fn_ptr sem args, no main thread.
/// Usado pelos timers/idle callbacks. Erro silencioso (callback ruim
/// nao para o event loop).
///
/// O codegen passa `cb` como fn_ptr raw para callbacks de UI (mesmo
/// padrao de \`widget_set_callback\`); a fn user com address-taken usa
/// default_call_conv (extern \"C\"-compativel), entao transmutar pra
/// `extern \"C\" fn()` funciona. Se cb apontar pra handle Function reify
/// (path Function.bind/INVOKE_AUTO), tambem funciona porque a primeira
/// instrucao de uma fn reify e' callable como extern \"C\" — embora seja
/// menos comum aqui.
fn invoke_no_args(cb: i64) {
    if cb == 0 {
        return;
    }
    unsafe {
        let f: unsafe extern "C" fn() = std::mem::transmute(cb as usize);
        f();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_APP_FREE(handle: u64) {
    free_entry(handle);
}
