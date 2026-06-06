//! (cross-runtime #359) Custom own-properties on user functions.
//!
//! In JS a function is an object: `(f as any).toString = () => "..."` installs an
//! own property that shadows `Function.prototype.toString`. RTS represents user
//! functions as code addresses / reified `Entry::Function` handles with no
//! stable per-function object, so property writes had nowhere to live and
//! `f.toString()` fell back to the native string.
//!
//! This side-table maps `"<fnName>\0<prop>" -> value handle`. The codegen
//! assign path writes it; `function_to_string_dyn` reads the `toString`
//! override and invokes it. Keyed by function *name* (stable across the
//! reified-handle churn) — global (not thread-local) so a write on the main
//! thread is visible to a read on a tokio worker (async bodies).

use std::collections::HashMap;
use std::sync::Mutex;

use super::ops::__RTS_FN_RT_INVOKE_AUTO;
use crate::namespaces::gc::string_pool::__RTS_FN_RT_TO_STRING_HANDLE;

static FN_PROPS: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

fn key(name_ptr: *const u8, name_len: i64, prop_ptr: *const u8, prop_len: i64) -> Option<String> {
    let name = read_str(name_ptr, name_len)?;
    let prop = read_str(prop_ptr, prop_len)?;
    Some(format!("{name}\u{0}{prop}"))
}

fn read_str(ptr: *const u8, len: i64) -> Option<String> {
    if ptr.is_null() || len <= 0 {
        return None;
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    Some(String::from_utf8_lossy(slice).into_owned())
}

/// `(<fn> as any).<prop> = value` — installs an own property on the function
/// named `name`. `value == 0` removes it.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_FUNCTION_SET_PROP(
    name_ptr: *const u8,
    name_len: i64,
    prop_ptr: *const u8,
    prop_len: i64,
    value: i64,
) {
    let Some(k) = key(name_ptr, name_len, prop_ptr, prop_len) else {
        return;
    };
    let mut g = FN_PROPS.lock().unwrap();
    let map = g.get_or_insert_with(HashMap::new);
    if value == 0 {
        map.remove(&k);
    } else {
        map.insert(k, value);
    }
}

/// Reads an installed own property (`0` if absent).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_FUNCTION_GET_PROP(
    name_ptr: *const u8,
    name_len: i64,
    prop_ptr: *const u8,
    prop_len: i64,
) -> i64 {
    let Some(k) = key(name_ptr, name_len, prop_ptr, prop_len) else {
        return 0;
    };
    FN_PROPS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(&k).copied())
        .unwrap_or(0)
}

/// `<fn>.toString()` — if a `toString` own property was installed, invokes it
/// (zero args) and returns its result; otherwise the native function string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_FUNCTION_TO_STRING_DYN(
    name_ptr: *const u8,
    name_len: i64,
    fn_handle: i64,
) -> u64 {
    let prop = b"toString";
    let override_h =
        __RTS_FN_RT_FUNCTION_GET_PROP(name_ptr, name_len, prop.as_ptr(), prop.len() as i64);
    if override_h != 0 {
        // Invoke the override (no args / no this). INVOKE_AUTO detects whether
        // the stored value is a Function handle or a raw func_addr (a lifted
        // arrow stored as its code address).
        let r = __RTS_FN_RT_INVOKE_AUTO(override_h, 0, 0);
        return r as u64;
    }
    __RTS_FN_RT_TO_STRING_HANDLE(fn_handle as u64)
}
