//! Async work N-API — versão **síncrona** (#1548 item 3, parcial).
//!
//! O RTS ainda não tem o event loop real (#207), então `napi_queue_async_work`
//! roda o `execute` + `complete` **imediatamente, em sequência**, na thread
//! atual — em vez de agendar `execute` numa worker thread e postar `complete`
//! na thread JS. O resultado JS sai correto (ex.: `bcrypt.hash()` resolve a
//! Promise com o hash); a diferença é que não há paralelismo real (a chamada
//! "async" bloqueia até completar). Muitos addros (crypto/hash/compressão)
//! funcionam assim — o que importa pra eles é o `complete` rodar com o
//! resultado, não a concorrência.
//!
//! Quando o event loop real existir, trocar `queue` por `rt().spawn_blocking`
//! (execute) + post do complete na thread JS. Ver docs/specs/napi-implementation.md.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Mutex;

use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

/// `execute(env, data)` — roda o trabalho (no Node: worker thread, sem JS).
type ExecuteCb = unsafe extern "C" fn(env: napi_env, data: *mut c_void);
/// `complete(env, status, data)` — pós-trabalho (no Node: thread JS).
type CompleteCb = unsafe extern "C" fn(env: napi_env, status: napi_status, data: *mut c_void);

struct AsyncWork {
    env: usize,
    execute: ExecuteCb,
    complete: Option<CompleteCb>,
    data: usize,
}

// SAFETY: ponteiros opacos, usados na thread atual (execução síncrona).
unsafe impl Send for AsyncWork {}

static WORKS: Mutex<Option<HashMap<usize, AsyncWork>>> = Mutex::new(None);
static NEXT_ID: Mutex<usize> = Mutex::new(1);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_async_work(
    env: napi_env,
    _async_resource: napi_value,
    _async_resource_name: napi_value,
    execute: Option<ExecuteCb>,
    complete: Option<CompleteCb>,
    data: *mut c_void,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let Some(execute) = execute else {
        return napi_invalid_arg;
    };
    let id = {
        let mut n = NEXT_ID.lock().unwrap_or_else(|e| e.into_inner());
        let id = *n;
        *n += 1;
        id
    };
    WORKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(
            id,
            AsyncWork {
                env: env.0 as usize,
                execute,
                complete,
                data: data as usize,
            },
        );
    unsafe { *result = id as *mut c_void };
    napi_ok
}

/// Versão síncrona: roda `execute` e depois `complete(ok)` imediatamente.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_queue_async_work(
    _env: napi_env,
    work: *mut c_void,
) -> napi_status {
    let id = work as usize;
    let w = {
        let guard = WORKS.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref().and_then(|m| m.get(&id)) {
            Some(w) => (w.env, w.execute, w.complete, w.data),
            None => return napi_invalid_arg,
        }
    };
    let (env_ptr, execute, complete, data) = w;
    let env = napi_env(env_ptr as *mut c_void);
    let data = data as *mut c_void;
    // execute (trabalho pesado) — síncrono.
    unsafe { execute(env, data) };
    // complete com status ok.
    if let Some(complete) = complete {
        unsafe { complete(env, napi_ok, data) };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_async_work(
    _env: napi_env,
    work: *mut c_void,
) -> napi_status {
    WORKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .map(|m| m.remove(&(work as usize)));
    napi_ok
}

/// Cancelar: na versão síncrona o trabalho já rodou ou ainda não foi enfileirado.
/// Reportamos `napi_generic_failure` (não-cancelável) — semântica N-API quando o
/// work não está pendente.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_cancel_async_work(
    _env: napi_env,
    _work: *mut c_void,
) -> napi_status {
    napi_status::napi_generic_failure
}

/// `async_init` — cria um async context (handle opaco de tracking). Devolve um
/// id dummy não-nulo.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_async_init(
    _env: napi_env,
    _async_resource: napi_value,
    _async_resource_name: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // Context opaco — basta ser não-nulo e único o suficiente.
    let id = {
        let mut n = NEXT_ID.lock().unwrap_or_else(|e| e.into_inner());
        let id = *n;
        *n += 1;
        id
    };
    unsafe { *result = id as *mut c_void };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_async_destroy(
    _env: napi_env,
    _async_context: *mut c_void,
) -> napi_status {
    napi_ok
}

// callback scopes: no Node marcam entrada/saída do contexto async ao chamar JS
// de fora do loop. Na versão síncrona não há contexto a empilhar — no-op com
// handle dummy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_open_callback_scope(
    _env: napi_env,
    _resource_object: napi_value,
    _context: *mut c_void,
    result: *mut *mut c_void,
) -> napi_status {
    if !result.is_null() {
        unsafe { *result = 1 as *mut c_void };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_close_callback_scope(
    _env: napi_env,
    _scope: *mut c_void,
) -> napi_status {
    napi_ok
}

/// `node_api_post_finalizer` — enfileira um finalize para rodar fora do GC.
/// Como não temos um ponto de drain dedicado pós-GC ainda, executamos
/// imediatamente (o RTS é single-threaded no caminho do addon síncrono).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_post_finalizer(
    env: napi_env,
    finalize_cb: *mut c_void,
    finalize_data: *mut c_void,
    finalize_hint: *mut c_void,
) -> napi_status {
    if finalize_cb.is_null() {
        return napi_invalid_arg;
    }
    let cb = unsafe {
        std::mem::transmute::<
            *mut c_void,
            unsafe extern "C" fn(napi_env, *mut c_void, *mut c_void),
        >(finalize_cb)
    };
    unsafe { cb(env, finalize_data, finalize_hint) };
    napi_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    static EXEC: AtomicUsize = AtomicUsize::new(0);
    static DONE: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn exec(_e: napi_env, _d: *mut c_void) {
        EXEC.fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "C" fn done(_e: napi_env, status: napi_status, _d: *mut c_void) {
        assert_eq!(status, napi_ok);
        DONE.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn async_work_runs_execute_then_complete() {
        EXEC.store(0, Ordering::SeqCst);
        DONE.store(0, Ordering::SeqCst);
        let mut work: *mut c_void = ptr::null_mut();
        let name = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe {
                napi_create_async_work(
                    env(),
                    name,
                    name,
                    Some(exec),
                    Some(done),
                    ptr::null_mut(),
                    &mut work,
                )
            },
            napi_ok
        );
        assert!(!work.is_null());
        unsafe { napi_queue_async_work(env(), work) };
        assert_eq!(EXEC.load(Ordering::SeqCst), 1);
        assert_eq!(DONE.load(Ordering::SeqCst), 1);
        unsafe { napi_delete_async_work(env(), work) };
    }

    #[test]
    fn async_init_destroy() {
        let mut ctx: *mut c_void = ptr::null_mut();
        let name = napi_value(ptr::null_mut());
        unsafe { napi_async_init(env(), name, name, &mut ctx) };
        assert!(!ctx.is_null());
        assert_eq!(unsafe { napi_async_destroy(env(), ctx) }, napi_ok);
    }

    #[test]
    fn post_finalizer_runs() {
        static F: AtomicUsize = AtomicUsize::new(0);
        unsafe extern "C" fn fin(_e: napi_env, _d: *mut c_void, _h: *mut c_void) {
            F.fetch_add(1, Ordering::SeqCst);
        }
        unsafe {
            node_api_post_finalizer(
                env(),
                fin as *mut c_void,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        assert_eq!(F.load(Ordering::SeqCst), 1);
    }
}
