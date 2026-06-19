//! `napi_external` — embrulha um ponteiro nativo opaco visível ao JS só como
//! handle. Usa `Entry::NapiExternal` (Etapa 2). Ver
//! docs/specs/napi-implementation.md (Etapa 12).

use std::ffi::c_void;

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry, NapiExternalData};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_finalize, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external(
    _env: napi_env,
    data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // `napi_finalize` é `Option<unsafe extern "C" fn(napi_env, data, hint)>`; o
    // engine guarda a forma `(env_opaco, data, hint)`. Os tipos batem (env é um
    // ponteiro opaco), então transmutamos a assinatura do callback.
    let finalize = finalize_cb.map(|cb| unsafe {
        std::mem::transmute::<
            unsafe extern "C" fn(napi_env, *mut c_void, *mut c_void),
            unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
        >(cb)
    });
    let h = alloc_entry(Entry::NapiExternal(Box::new(NapiExternalData {
        data,
        finalize,
        finalize_hint,
    })));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_external(
    _env: napi_env,
    value: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let data = with_entry(handle_from_value(value), |e| match e {
        Some(Entry::NapiExternal(ext)) => Some(ext.data),
        _ => None,
    });
    match data {
        Some(d) => {
            unsafe { *result = d };
            napi_ok
        }
        None => napi_status::napi_invalid_arg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::napi_valuetype;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn external_roundtrip() {
        let ptr_val = 0xDEAD_BEEF_usize as *mut c_void;
        let mut v = napi_value(ptr::null_mut());
        assert_eq!(
            unsafe { napi_create_external(env(), ptr_val, None, ptr::null_mut(), &mut v) },
            napi_ok
        );
        // typeof → external
        let mut t = napi_valuetype::napi_undefined;
        unsafe { crate::values::napi_typeof(env(), v, &mut t) };
        assert_eq!(t, napi_valuetype::napi_external);
        // get_value_external devolve o mesmo ponteiro
        let mut out: *mut c_void = ptr::null_mut();
        assert_eq!(
            unsafe { napi_get_value_external(env(), v, &mut out) },
            napi_ok
        );
        assert_eq!(out, ptr_val);
    }

    #[test]
    fn get_external_on_non_external_fails() {
        let mut d = napi_value(ptr::null_mut());
        unsafe { crate::values::napi_create_double(env(), 1.0, &mut d) };
        let mut out: *mut c_void = ptr::null_mut();
        assert_eq!(
            unsafe { napi_get_value_external(env(), d, &mut out) },
            napi_invalid_arg
        );
    }
}
