//! ArrayBuffer / TypedArray / DataView N-API (#1548 item 1). Usa o
//! `Entry::ArrayBuffer` do engine (ptr estável). Views (typedarray/dataview)
//! são modeladas como `Entry::Map` com chaves reservadas apontando o buffer +
//! offset + length + tipo (o RTS não tem TypedArray nativo). Ver
//! docs/specs/napi-implementation.md.

use std::ffi::{c_char, c_void};

use rts_engine::heap::handles::{
    alloc_arraybuffer, alloc_entry, alloc_external_arraybuffer, arraybuffer_detach,
    arraybuffer_is_detached, arraybuffer_len, arraybuffer_ptr, is_arraybuffer, with_entry,
    with_entry_mut, Entry,
};

use crate::env::{handle_from_value, value_from_handle};
use crate::types::{napi_env, napi_status, napi_value};

use napi_status::{napi_invalid_arg, napi_ok};

// Chaves reservadas que descrevem uma view (typedarray/dataview) sobre um buffer.
const VIEW_BUF: &str = "__napi_view_buf__";
const VIEW_OFFSET: &str = "__napi_view_offset__";
const VIEW_LEN: &str = "__napi_view_len__"; // nº de elementos
const VIEW_TYPE: &str = "__napi_view_type__"; // napi_typedarray_type (i32), -1 = dataview

// ── ArrayBuffer ──────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_arraybuffer(
    _env: napi_env,
    byte_length: usize,
    data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let h = alloc_arraybuffer(byte_length);
    if !data.is_null() {
        unsafe { *data = arraybuffer_ptr(h) as *mut c_void };
    }
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external_arraybuffer(
    _env: napi_env,
    external_data: *mut c_void,
    byte_length: usize,
    finalize_cb: *mut c_void,
    finalize_hint: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
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
    let h = alloc_external_arraybuffer(
        external_data as *mut u8,
        byte_length,
        finalize,
        finalize_hint,
    );
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_arraybuffer_info(
    _env: napi_env,
    arraybuffer: napi_value,
    data: *mut *mut c_void,
    byte_length: *mut usize,
) -> napi_status {
    let h = handle_from_value(arraybuffer);
    if !is_arraybuffer(h) {
        return napi_status::napi_arraybuffer_expected;
    }
    if !data.is_null() {
        unsafe { *data = arraybuffer_ptr(h) as *mut c_void };
    }
    if !byte_length.is_null() {
        unsafe { *byte_length = arraybuffer_len(h) };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_arraybuffer(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = is_arraybuffer(handle_from_value(value)) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_detach_arraybuffer(
    _env: napi_env,
    arraybuffer: napi_value,
) -> napi_status {
    if arraybuffer_detach(handle_from_value(arraybuffer)) {
        napi_ok
    } else {
        napi_status::napi_arraybuffer_expected
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_detached_arraybuffer(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    unsafe { *result = arraybuffer_is_detached(handle_from_value(value)) };
    napi_ok
}

// ── TypedArray / DataView (views via Map) ────────────────────────────────────

fn make_view(buf: u64, ty: i32, length: usize, byte_offset: usize) -> u64 {
    let mut m = indexmap::IndexMap::new();
    m.insert(VIEW_BUF.to_string(), buf as i64);
    m.insert(VIEW_OFFSET.to_string(), byte_offset as i64);
    m.insert(VIEW_LEN.to_string(), length as i64);
    m.insert(VIEW_TYPE.to_string(), ty as i64);
    alloc_entry(Entry::Map(Box::new(m)))
}

fn read_view(h: u64) -> Option<(u64, i32, usize, usize)> {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => {
            let buf = *m.get(VIEW_BUF)? as u64;
            let ty = *m.get(VIEW_TYPE)? as i32;
            let len = *m.get(VIEW_LEN)? as usize;
            let off = *m.get(VIEW_OFFSET)? as usize;
            Some((buf, ty, len, off))
        }
        _ => None,
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_typedarray(
    _env: napi_env,
    type_: i32,
    length: usize,
    arraybuffer: napi_value,
    byte_offset: usize,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let buf = handle_from_value(arraybuffer);
    if !is_arraybuffer(buf) {
        return napi_status::napi_arraybuffer_expected;
    }
    let h = make_view(buf, type_, length, byte_offset);
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_typedarray_info(
    _env: napi_env,
    typedarray: napi_value,
    type_: *mut i32,
    length: *mut usize,
    data: *mut *mut c_void,
    arraybuffer: *mut napi_value,
    byte_offset: *mut usize,
) -> napi_status {
    let Some((buf, ty, len, off)) = read_view(handle_from_value(typedarray)) else {
        return napi_invalid_arg;
    };
    if ty < 0 {
        return napi_invalid_arg; // é um dataview, não typedarray
    }
    if !type_.is_null() {
        unsafe { *type_ = ty };
    }
    if !length.is_null() {
        unsafe { *length = len };
    }
    if !data.is_null() {
        let base = arraybuffer_ptr(buf);
        unsafe { *data = if base.is_null() { base } else { base.add(off) } as *mut c_void };
    }
    if !arraybuffer.is_null() {
        unsafe { *arraybuffer = value_from_handle(buf) };
    }
    if !byte_offset.is_null() {
        unsafe { *byte_offset = off };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_dataview(
    _env: napi_env,
    byte_length: usize,
    arraybuffer: napi_value,
    byte_offset: usize,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let buf = handle_from_value(arraybuffer);
    if !is_arraybuffer(buf) {
        return napi_status::napi_arraybuffer_expected;
    }
    let h = make_view(buf, -1, byte_length, byte_offset); // ty=-1 → dataview
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_dataview_info(
    _env: napi_env,
    dataview: napi_value,
    byte_length: *mut usize,
    data: *mut *mut c_void,
    arraybuffer: *mut napi_value,
    byte_offset: *mut usize,
) -> napi_status {
    let Some((buf, ty, len, off)) = read_view(handle_from_value(dataview)) else {
        return napi_invalid_arg;
    };
    if ty != -1 {
        return napi_invalid_arg; // não é dataview
    }
    if !byte_length.is_null() {
        unsafe { *byte_length = len };
    }
    if !data.is_null() {
        let base = arraybuffer_ptr(buf);
        unsafe { *data = if base.is_null() { base } else { base.add(off) } as *mut c_void };
    }
    if !arraybuffer.is_null() {
        unsafe { *arraybuffer = value_from_handle(buf) };
    }
    if !byte_offset.is_null() {
        unsafe { *byte_offset = off };
    }
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_typedarray(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let is = read_view(handle_from_value(value)).map(|(_, ty, ..)| ty >= 0).unwrap_or(false);
    unsafe { *result = is };
    napi_ok
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_dataview(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let is = read_view(handle_from_value(value)).map(|(_, ty, ..)| ty == -1).unwrap_or(false);
    unsafe { *result = is };
    napi_ok
}

// ── external_buffer + buffer_from_arraybuffer ────────────────────────────────

/// `napi_create_external_buffer`: como o RTS não tem Buffer com ptr externo
/// emprestado, criamos um ArrayBuffer borrowed (mesma semântica: ptr + len +
/// finalizer). Addons tratam o resultado como Buffer-like.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external_buffer(
    _env: napi_env,
    length: usize,
    data: *mut c_void,
    finalize_cb: *mut c_void,
    finalize_hint: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
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
    let h = alloc_external_arraybuffer(data as *mut u8, length, finalize, finalize_hint);
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

/// `node_api_create_buffer_from_arraybuffer`: cria um Buffer (Entry::Buffer)
/// copiando os bytes do arraybuffer no range [offset, offset+len).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_buffer_from_arraybuffer(
    _env: napi_env,
    arraybuffer: napi_value,
    byte_offset: usize,
    byte_length: usize,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let buf = handle_from_value(arraybuffer);
    if !is_arraybuffer(buf) {
        return napi_status::napi_arraybuffer_expected;
    }
    let bytes = {
        let base = arraybuffer_ptr(buf);
        let total = arraybuffer_len(buf);
        if base.is_null() || byte_offset + byte_length > total {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(base.add(byte_offset), byte_length).to_vec() }
        }
    };
    let h = alloc_entry(Entry::Buffer(bytes));
    unsafe { *result = value_from_handle(h) };
    napi_ok
}

/// `node_api_create_sharedarraybuffer`: o RTS não tem memória compartilhada
/// entre threads via SAB; cria um ArrayBuffer normal (backing igual). Atomics
/// sobre ele funcionam single-process.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn node_api_create_sharedarraybuffer(
    env: napi_env,
    byte_length: usize,
    data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    unsafe { napi_create_arraybuffer(env, byte_length, data, result) }
}

// suprime warning de import não-usado em alguns paths
#[allow(unused_imports)]
use with_entry_mut as _wem;
#[allow(unused_imports)]
use c_char as _cc;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    fn env() -> napi_env {
        napi_env(ptr::null_mut())
    }

    #[test]
    fn arraybuffer_create_write_read() {
        let mut data: *mut c_void = ptr::null_mut();
        let mut ab = napi_value(ptr::null_mut());
        unsafe { napi_create_arraybuffer(env(), 16, &mut data, &mut ab) };
        assert!(!data.is_null());
        unsafe {
            *(data as *mut u8) = 0x42;
        }
        // is_arraybuffer
        let mut is = false;
        unsafe { napi_is_arraybuffer(env(), ab, &mut is) };
        assert!(is);
        // get_info devolve o mesmo ptr + len
        let mut d2: *mut c_void = ptr::null_mut();
        let mut len = 0usize;
        unsafe { napi_get_arraybuffer_info(env(), ab, &mut d2, &mut len) };
        assert_eq!(len, 16);
        assert_eq!(d2, data);
        unsafe {
            assert_eq!(*(d2 as *const u8), 0x42);
        }
    }

    #[test]
    fn typedarray_view_over_buffer() {
        let mut data: *mut c_void = ptr::null_mut();
        let mut ab = napi_value(ptr::null_mut());
        unsafe { napi_create_arraybuffer(env(), 32, &mut data, &mut ab) };
        // Uint8Array (type 1, p.ex.) de 10 elementos com offset 4.
        let mut ta = napi_value(ptr::null_mut());
        unsafe { napi_create_typedarray(env(), 1, 10, ab, 4, &mut ta) };
        let mut is = false;
        unsafe { napi_is_typedarray(env(), ta, &mut is) };
        assert!(is);
        let mut ty = 0i32;
        let mut len = 0usize;
        let mut tdata: *mut c_void = ptr::null_mut();
        let mut off = 0usize;
        unsafe {
            napi_get_typedarray_info(env(), ta, &mut ty, &mut len, &mut tdata, ptr::null_mut(), &mut off)
        };
        assert_eq!(ty, 1);
        assert_eq!(len, 10);
        assert_eq!(off, 4);
        // data aponta para base+4
        assert_eq!(tdata as usize, data as usize + 4);
    }

    #[test]
    fn detach_arraybuffer() {
        let mut ab = napi_value(ptr::null_mut());
        unsafe { napi_create_arraybuffer(env(), 8, ptr::null_mut(), &mut ab) };
        let mut det = true;
        unsafe { napi_is_detached_arraybuffer(env(), ab, &mut det) };
        assert!(!det);
        unsafe { napi_detach_arraybuffer(env(), ab) };
        unsafe { napi_is_detached_arraybuffer(env(), ab, &mut det) };
        assert!(det);
    }
}
