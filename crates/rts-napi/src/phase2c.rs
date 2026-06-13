//! Fase 2c: mais fns N-API implementáveis com o engine atual — Promise
//! (deferred), coerce_to_string, type tags, add_finalizer, run_script,
//! fatal/syntax errors, cleanup hooks, prototype, type checks restantes.
//! Ver docs/specs/napi-implementation.md.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::Mutex;

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry, NapiExternalData};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_generic_failure, napi_invalid_arg, napi_ok};

const UNDEFINED: u64 = (i64::MIN + 2) as u64;

// Símbolos de Promise (rts-std) resolvidos no link do bin. Stub em test.
#[cfg(not(test))]
unsafe extern "C" {
    fn __RTS_FN_NS_PROMISE_NEW_PENDING() -> u64;
    fn __RTS_FN_NS_PROMISE_RESOLVE(promise: u64, value: i64) -> i64;
    fn __RTS_FN_NS_PROMISE_REJECT(promise: u64, error: i64) -> i64;
}
#[cfg(test)]
unsafe fn __RTS_FN_NS_PROMISE_NEW_PENDING() -> u64 {
    // Em teste, simula com um Map (não exercitamos a Promise real aqui).
    alloc_entry(Entry::Map(Box::new(indexmap::IndexMap::new())))
}
#[cfg(test)]
unsafe fn __RTS_FN_NS_PROMISE_RESOLVE(_p: u64, _v: i64) -> i64 {
    0
}
#[cfg(test)]
unsafe fn __RTS_FN_NS_PROMISE_REJECT(_p: u64, _e: i64) -> i64 {
    0
}

// ── Promise / deferred ───────────────────────────────────────────────────────
// O `napi_deferred` é o lado de resolução. Mapeamos o deferred ao MESMO handle
// da Promise (1:1): create devolve (deferred, promise) com o mesmo handle, e
// resolve/reject_deferred operam sobre ele. Registramos quais handles são
// deferreds válidos.

static DEFERREDS: Mutex<Option<HashMap<usize, u64>>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_promise(
    _env: napi_env,
    deferred: *mut *mut c_void,
    promise: *mut napi_value,
) -> napi_status {
    if deferred.is_null() || promise.is_null() {
        return napi_invalid_arg;
    }
    let p = unsafe { __RTS_FN_NS_PROMISE_NEW_PENDING() };
    // deferred opaco = um índice; mapeia pro handle da promise.
    let mut guard = DEFERREDS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let id = map.len() + 1;
    map.insert(id, p);
    unsafe {
        *deferred = id as *mut c_void;
        *promise = value_from_handle(p);
    }
    napi_ok
}

unsafe fn deferred_promise(deferred: *mut c_void) -> Option<u64> {
    let id = deferred as usize;
    DEFERREDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .and_then(|m| m.get(&id).copied())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_resolve_deferred(
    _env: napi_env,
    deferred: *mut c_void,
    resolution: napi_value,
) -> napi_status {
    let Some(p) = (unsafe { deferred_promise(deferred) }) else {
        return napi_invalid_arg;
    };
    unsafe { __RTS_FN_NS_PROMISE_RESOLVE(p, handle_from_value(resolution) as i64) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reject_deferred(
    _env: napi_env,
    deferred: *mut c_void,
    rejection: napi_value,
) -> napi_status {
    let Some(p) = (unsafe { deferred_promise(deferred) }) else {
        return napi_invalid_arg;
    };
    unsafe { __RTS_FN_NS_PROMISE_REJECT(p, handle_from_value(rejection) as i64) };
    napi_ok
}

// ── coerce_to_string ─────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_string(
    _env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = handle_from_value(value);
    let s = to_string_repr(h);
    let out = alloc_entry(Entry::String(s.into_bytes()));
    unsafe { *result = value_from_handle(out) };
    napi_ok
}

fn to_string_repr(h: u64) -> String {
    match h {
        x if x == (i64::MIN) as u64 => "false".into(),
        x if x == (i64::MIN + 1) as u64 => "true".into(),
        x if x == (i64::MIN + 2) as u64 => "undefined".into(),
        x if x == (i64::MIN + 3) as u64 => "null".into(),
        0 => "null".into(),
        _ => with_entry(h, |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            Some(Entry::FloatPrim(f)) | Some(Entry::NumberBox(f)) => fmt_num(*f),
            Some(Entry::Vec(_)) => "[object Array]".into(),
            Some(Entry::Map(_)) => "[object Object]".into(),
            Some(Entry::Symbol { .. }) => "Symbol()".into(),
            Some(_) => "[object Object]".into(),
            None => "undefined".into(),
        }),
    }
}

fn fmt_num(f: f64) -> String {
    if f.is_nan() {
        "NaN".into()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity".into() } else { "-Infinity".into() }
    } else if f == f.trunc() && f.abs() < 1e21 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

// ── type checks que faltavam (sem suporte real → false) ──────────────────────

macro_rules! always_false_check {
    ($name:ident) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            _env: napi_env,
            _value: napi_value,
            result: *mut bool,
        ) -> napi_status {
            if result.is_null() {
                return napi_invalid_arg;
            }
            // RTS não tem ArrayBuffer/TypedArray/DataView distintos ainda
            // (follow-up engine). Reportar false é seguro: addons checam e usam
            // o caminho alternativo (ex.: Buffer).
            unsafe { *result = false };
            napi_ok
        }
    };
}

always_false_check!(napi_is_arraybuffer);
always_false_check!(napi_is_typedarray);
always_false_check!(napi_is_dataview);
always_false_check!(napi_is_detached_arraybuffer);

// ── type tags (object branding) ──────────────────────────────────────────────
// Um type tag é um par u64 (lower, upper). Guardamos numa chave reservada do Map.

#[repr(C)]
pub struct napi_type_tag {
    pub lower: u64,
    pub upper: u64,
}

const TAG_KEY_LO: &str = "__napi_tag_lo__";
const TAG_KEY_HI: &str = "__napi_tag_hi__";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_type_tag_object(
    _env: napi_env,
    object: napi_value,
    type_tag: *const napi_type_tag,
) -> napi_status {
    if type_tag.is_null() {
        return napi_invalid_arg;
    }
    let tag = unsafe { &*type_tag };
    let ok = with_entry_mut(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => {
            m.insert(TAG_KEY_LO.to_string(), tag.lower as i64);
            m.insert(TAG_KEY_HI.to_string(), tag.upper as i64);
            true
        }
        _ => false,
    });
    if ok { napi_ok } else { napi_status::napi_object_expected }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_check_object_type_tag(
    _env: napi_env,
    object: napi_value,
    type_tag: *const napi_type_tag,
    result: *mut bool,
) -> napi_status {
    if type_tag.is_null() || result.is_null() {
        return napi_invalid_arg;
    }
    let tag = unsafe { &*type_tag };
    let matches = with_entry(handle_from_value(object), |e| match e {
        Some(Entry::Map(m)) => {
            m.get(TAG_KEY_LO).copied() == Some(tag.lower as i64)
                && m.get(TAG_KEY_HI).copied() == Some(tag.upper as i64)
        }
        _ => false,
    });
    unsafe { *result = matches };
    napi_ok
}

// ── add_finalizer (enfileira via NapiExternal) ───────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_add_finalizer(
    _env: napi_env,
    _js_object: napi_value,
    native_object: *mut c_void,
    finalize_cb: *mut c_void,
    finalize_hint: *mut c_void,
    _result: *mut c_void,
) -> napi_status {
    // Cria um Entry::NapiExternal carregando o finalizer; quando coletado, o
    // cleanup_entry enfileira o disparo (drain pelo runtime). Não anexado ao
    // js_object (precisa de slot escondido — follow-up engine), mas o finalizer
    // roda na coleta do external.
    let finalize = if finalize_cb.is_null() {
        None
    } else {
        Some(unsafe {
            std::mem::transmute::<
                *mut c_void,
                unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
            >(finalize_cb)
        })
    };
    alloc_entry(Entry::NapiExternal(Box::new(NapiExternalData {
        data: native_object,
        finalize,
        finalize_hint,
    })));
    napi_ok
}

// ── errors (fatal / syntax) ──────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_fatal_error(
    location: *const c_char,
    _location_len: usize,
    message: *const c_char,
    _message_len: usize,
) -> ! {
    let loc = unsafe { cstr_lossy(location) };
    let msg = unsafe { cstr_lossy(message) };
    eprintln!("[napi fatal] {loc}: {msg}");
    std::process::abort();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_fatal_exception(
    _env: napi_env,
    _err: napi_value,
) -> napi_status {
    // Loga e segue (não aborta — diferente de fatal_error).
    eprintln!("[napi] fatal exception reported by addon");
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_syntax_error(
    _env: napi_env,
    _code: napi_value,
    msg: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let message = with_entry(handle_from_value(msg), |e| match e {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    });
    let h = alloc_entry(Entry::ErrorObj {
        message,
        name: "SyntaxError".into(),
        cause: 0,
    });
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_throw_syntax_error(
    env: napi_env,
    _code: *const c_char,
    msg: *const c_char,
) -> napi_status {
    let message = unsafe { cstr_lossy(msg) };
    let h = alloc_entry(Entry::ErrorObj {
        message,
        name: "SyntaxError".into(),
        cause: 0,
    });
    unsafe { crate::errors::napi_throw(env, value_from_handle(h)) }
}

// ── misc ─────────────────────────────────────────────────────────────────────

/// Caminho do módulo — devolve string vazia (Fase 2). napi-sys usa pra debug.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_get_module_file_name(
    _env: napi_env,
    result: *mut *const c_char,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = c"".as_ptr() };
    napi_ok
}

/// Protótipo de um objeto — Fase 2 devolve null (sem cadeia modelada).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_prototype(
    _env: napi_env,
    _object: napi_value,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = value_from_handle((i64::MIN + 3) as u64) }; // null
    napi_ok
}

/// `adjust_external_memory` — no-op (RTS não rastreia memória externa p/ GC
/// pressure). Devolve o valor passado como novo total.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_adjust_external_memory(
    _env: napi_env,
    change_in_bytes: i64,
    adjusted_value: *mut i64,
) -> napi_status {
    if !adjusted_value.is_null() {
        unsafe { *adjusted_value = change_in_bytes };
    }
    napi_ok
}

// cleanup hooks: no-op (RTS não tem env teardown hooks ainda). Aceitar p/ não
// quebrar addons que registram por higiene.
macro_rules! noop_ok {
    ($name:ident ( $($arg:ident : $ty:ty),* )) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($($arg : $ty),*) -> napi_status {
            $( let _ = $arg; )*
            napi_ok
        }
    };
}

noop_ok!(napi_add_env_cleanup_hook(_env: napi_env, _fun: *mut c_void, _arg: *mut c_void));
noop_ok!(napi_remove_env_cleanup_hook(_env: napi_env, _fun: *mut c_void, _arg: *mut c_void));

/// `run_script` — RTS não tem eval de string JS arbitrária via N-API (o eval
/// existe mas é outra superfície). Devolve falha clara.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_run_script(
    _env: napi_env,
    _script: napi_value,
    _result: *mut napi_value,
) -> napi_status {
    napi_generic_failure
}

unsafe fn cstr_lossy(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf8_lossy(std::slice::from_raw_parts(p as *const u8, len)).into_owned()
    }
}

#[allow(unused_imports)]
use UNDEFINED as _U;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn coerce_to_string_works() {
        let mut out = napi_value(ptr::null_mut());
        let n = value_from_handle(alloc_entry(Entry::FloatPrim(42.0)));
        unsafe { napi_coerce_to_string(env(), n, &mut out) };
        let s = with_entry(handle_from_value(out), |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        });
        assert_eq!(s, "42");
    }

    #[test]
    fn type_tag_roundtrip() {
        let obj = value_from_handle(alloc_entry(Entry::Map(Box::new(
            indexmap::IndexMap::new(),
        ))));
        let tag = napi_type_tag {
            lower: 0xAABB,
            upper: 0xCCDD,
        };
        unsafe { napi_type_tag_object(env(), obj, &tag) };
        let mut matches = false;
        unsafe { napi_check_object_type_tag(env(), obj, &tag, &mut matches) };
        assert!(matches);
        // tag diferente → não bate
        let other = napi_type_tag { lower: 1, upper: 2 };
        unsafe { napi_check_object_type_tag(env(), obj, &other, &mut matches) };
        assert!(!matches);
    }

    #[test]
    fn is_arraybuffer_false() {
        let mut b = true;
        let buf = value_from_handle(alloc_entry(Entry::Buffer(vec![1, 2])));
        unsafe { napi_is_arraybuffer(env(), buf, &mut b) };
        assert!(!b);
    }

    #[test]
    fn syntax_error_has_name() {
        let mut err = napi_value(ptr::null_mut());
        let msg = value_from_handle(alloc_entry(Entry::String(b"bad".to_vec())));
        let code = napi_value(ptr::null_mut());
        unsafe { node_api_create_syntax_error(env(), code, msg, &mut err) };
        with_entry(handle_from_value(err), |e| {
            assert!(matches!(e, Some(Entry::ErrorObj { name, .. }) if name == "SyntaxError"));
        });
    }
}
