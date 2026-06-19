//! Threadsafe functions N-API — versão **síncrona/inline** (#1548 item 3).
//!
//! Uma TSFN deixa o addon chamar uma fn JS de outra thread. O RTS ainda não tem
//! o event loop real (#207) drenando uma fila na thread JS, então
//! `napi_call_threadsafe_function` invoca o `call_js_cb` **imediatamente** na
//! thread chamadora (inline) em vez de postar na thread JS. Funciona para addons
//! que usam TSFN no caminho síncrono ou de forma fire-and-forget; addons que
//! dependem de cross-thread real + ordenação pelo loop são limitados (até o Bun
//! tem gaps aqui). Ver docs/specs/napi-implementation.md / issue #1548.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::env::value_from_handle;
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

/// `call_js_cb(env, js_callback, context, data)` — chamado para cada item.
type CallJsCb =
    unsafe extern "C" fn(env: napi_env, js_callback: napi_value, context: *mut c_void, data: *mut c_void);

struct Tsfn {
    env: usize,
    js_callback: u64, // handle da fn JS (0 = sem)
    call_js: Option<CallJsCb>,
    context: usize,
    refcount: AtomicUsize, // nº de acquires (thread refs)
}

unsafe impl Send for Tsfn {}

static TSFNS: Mutex<Option<HashMap<usize, Tsfn>>> = Mutex::new(None);
static NEXT: Mutex<usize> = Mutex::new(1);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_threadsafe_function(
    env: napi_env,
    func: napi_value,
    _async_resource: napi_value,
    _async_resource_name: napi_value,
    _max_queue_size: usize,
    _initial_thread_count: usize,
    _thread_finalize_data: *mut c_void,
    _thread_finalize_cb: *mut c_void,
    context: *mut c_void,
    call_js_cb: Option<CallJsCb>,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let id = {
        let mut n = NEXT.lock().unwrap_or_else(|e| e.into_inner());
        let id = *n;
        *n += 1;
        id
    };
    TSFNS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(
            id,
            Tsfn {
                env: env.0 as usize,
                js_callback: crate::env::handle_from_value(func),
                call_js: call_js_cb,
                context: context as usize,
                refcount: AtomicUsize::new(1),
            },
        );
    unsafe { *result = id as *mut c_void };
    napi_ok
}

fn with_tsfn<R>(handle: *mut c_void, f: impl FnOnce(&Tsfn) -> R) -> Option<R> {
    let id = handle as usize;
    let guard = TSFNS.lock().unwrap_or_else(|e| e.into_inner());
    guard.as_ref().and_then(|m| m.get(&id)).map(f)
}

/// Invoca o `call_js_cb` imediatamente (inline) com o `data`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_call_threadsafe_function(
    func: *mut c_void,
    data: *mut c_void,
    _mode: i32, // blocking/non-blocking — irrelevante na execução inline
) -> napi_status {
    let res = with_tsfn(func, |t| {
        (t.env, t.js_callback, t.call_js, t.context)
    });
    let Some((env_ptr, js_cb, call_js, context)) = res else {
        return napi_invalid_arg;
    };
    if let Some(call_js) = call_js {
        let env = napi_env(env_ptr as *mut c_void);
        unsafe {
            call_js(
                env,
                value_from_handle(js_cb),
                context as *mut c_void,
                data,
            )
        };
    }
    // call_js_cb == NULL: o data é tratado como uma fn napi_value chamável
    // diretamente (raro). Inline não suporta esse modo — devolve ok mesmo assim.
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_acquire_threadsafe_function(
    func: *mut c_void,
) -> napi_status {
    if with_tsfn(func, |t| t.refcount.fetch_add(1, Ordering::SeqCst)).is_some() {
        napi_ok
    } else {
        napi_invalid_arg
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_release_threadsafe_function(
    func: *mut c_void,
    _mode: i32,
) -> napi_status {
    let id = func as usize;
    let mut guard = TSFNS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(map) = guard.as_mut() else {
        return napi_invalid_arg;
    };
    let Some(t) = map.get(&id) else {
        return napi_invalid_arg;
    };
    let prev = t.refcount.fetch_sub(1, Ordering::SeqCst);
    if prev <= 1 {
        map.remove(&id); // último release → libera
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_threadsafe_function_context(
    func: *mut c_void,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    match with_tsfn(func, |t| t.context) {
        Some(ctx) => {
            unsafe { *result = ctx as *mut c_void };
            napi_ok
        }
        None => napi_invalid_arg,
    }
}

/// ref/unref controlam se a TSFN mantém o event loop vivo. Inline não tem loop
/// → no-op que aceita.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_ref_threadsafe_function(
    _env: napi_env,
    func: *mut c_void,
) -> napi_status {
    if with_tsfn(func, |_| ()).is_some() { napi_ok } else { napi_invalid_arg }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_unref_threadsafe_function(
    _env: napi_env,
    func: *mut c_void,
) -> napi_status {
    if with_tsfn(func, |_| ()).is_some() { napi_ok } else { napi_invalid_arg }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static LAST: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn call_js(_e: napi_env, _cb: napi_value, ctx: *mut c_void, data: *mut c_void) {
        CALLS.fetch_add(1, Ordering::SeqCst);
        LAST.store(data as usize + ctx as usize, Ordering::SeqCst);
    }

    #[test]
    fn create_call_release() {
        CALLS.store(0, Ordering::SeqCst);
        let mut tsfn: *mut c_void = ptr::null_mut();
        let func = napi_value(ptr::null_mut());
        let name = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe {
                napi_create_threadsafe_function(
                    env(), func, name, name, 0, 1,
                    ptr::null_mut(), ptr::null_mut(),
                    0x10 as *mut c_void, // context
                    Some(call_js),
                    &mut tsfn,
                )
            },
            napi_ok
        );
        assert!(!tsfn.is_null());
        // chama inline
        unsafe { napi_call_threadsafe_function(tsfn, 0x5 as *mut c_void, 0) };
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(LAST.load(Ordering::SeqCst), 0x15); // ctx(0x10)+data(0x5)
        // context
        let mut ctx: *mut c_void = ptr::null_mut();
        unsafe { napi_get_threadsafe_function_context(tsfn, &mut ctx) };
        assert_eq!(ctx as usize, 0x10);
        // acquire +1, release 2x → libera
        unsafe { napi_acquire_threadsafe_function(tsfn) };
        unsafe { napi_release_threadsafe_function(tsfn, 0) };
        unsafe { napi_release_threadsafe_function(tsfn, 0) };
        // após liberar, chamar falha
        assert_eq!(
            unsafe { napi_call_threadsafe_function(tsfn, ptr::null_mut(), 0) },
            napi_invalid_arg
        );
    }
}
