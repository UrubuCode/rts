//! Bytes an addon reads and writes in place.
//!
//! `napi_create_buffer` and `napi_get_buffer_info` are how a compression or
//! crypto addon works at all: it is handed a pointer and fills it. Everything
//! here is about that pointer being real.
//!
//! # Why this cannot use the copying accessor
//!
//! `rts_core::entry::bytes_of` answers a `Vec<u8>`, deliberately — a slice
//! borrowed out of the runtime is alive only while the context is. An addon
//! handed a copy writes into a temporary and the program never sees it, which
//! is not a slower answer but a wrong one. So this uses
//! `rts_core::entry::bytes_pointer`, which was added for exactly this caller
//! and states its contract there.
//!
//! # What an addon must do to keep it valid
//!
//! Node's rule, unchanged here: the pointer lives as long as the BUFFER does.
//! An addon holding one across a turn of the loop must hold a `napi_ref` to the
//! buffer too — otherwise the collector is entitled to take it, and this engine
//! will.
//!
//! # `Buffer` and `Uint8Array` are not the same object here
//!
//! `rts-core` has both and they are observably different — `Buffer.isBuffer`
//! answers false for a plain `Uint8Array`, and so does every instance method.
//! `napi_create_buffer` makes a `Buffer` because that is what Node's does;
//! `napi_create_arraybuffer` makes the byte store a typed array is a view of.

use core::ffi::c_void;

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_ok};

/// The bytes a value names, as an address the addon may write.
fn window(value: u64) -> Option<(*mut u8, usize)> {
    rts_core::entry::with_runtime(|context| rts_core::entry::bytes_pointer(context, value))
}

/// Hands a word back as a handle in the innermost scope.
///
/// # Safety
///
/// `env` live, `out` writable.
unsafe fn produce(env: napi_env, out: *mut napi_value, word: u64) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(word);
    // SAFETY: the caller's contract.
    match unsafe { write_out(out, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_create_buffer` — `length` zero bytes, and the address of them.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_buffer(
    env: napi_env,
    length: usize,
    data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let word = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_buffer(context, &vec![0u8; length])
    });
    if !data.is_null() {
        let Some((pointer, _)) = window(word) else {
            return napi_status::napi_generic_failure;
        };
        // SAFETY: the caller's contract — `data` writable.
        unsafe { *data = pointer.cast() };
    }
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_create_buffer_copy` — a buffer holding a copy of what the addon has.
///
/// # Safety
///
/// `data` must point at `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_buffer_copy(
    env: napi_env,
    length: usize,
    data: *const c_void,
    result_data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    if data.is_null() && length != 0 {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract — `length` readable bytes.
    let source = match length {
        0 => &[][..],
        _ => unsafe { core::slice::from_raw_parts(data.cast::<u8>(), length) },
    };
    let word =
        rts_core::entry::with_runtime(|context| rts_core::entry::make_buffer(context, source));
    if !result_data.is_null() {
        let Some((pointer, _)) = window(word) else {
            return napi_status::napi_generic_failure;
        };
        // SAFETY: the caller's contract.
        unsafe { *result_data = pointer.cast() };
    }
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_get_buffer_info` — where the bytes are, and how many.
///
/// Both out-parameters are optional, which is how addons use it: some want only
/// the length.
///
/// # Safety
///
/// The ABI's, and see the module doc for how long the pointer lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_buffer_info(
    _env: napi_env,
    value: napi_value,
    data: *mut *mut c_void,
    length: *mut usize,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some((pointer, count)) = window(word) else {
        return napi_status::napi_invalid_arg;
    };
    if !data.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *data = pointer.cast() };
    }
    if !length.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *length = count };
    }
    napi_ok
}

/// `napi_is_buffer`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_buffer(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // Asked as a program asks — `x instanceof Buffer` — because a `Buffer` and
    // a plain `Uint8Array` are different objects here and an addon branches on
    // which it got. Reading "does it have bytes" would answer true for both.
    let class = rts_core::entry::with_runtime(rts_core::entry::buffer_class);
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::instance_of(word, class) };
    napi_ok
}

/// `napi_is_typedarray` — anything with a byte window, `Buffer` included.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_typedarray(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = window(word).is_some() };
    napi_ok
}

/// `napi_get_typedarray_info`, the part this engine can answer.
///
/// The element type and the offset are NOT answered: `rts-core`'s view records
/// them, and nothing exports either yet. Rather than guess `napi_uint8_array`
/// for everything — which would be right for a `Buffer` and wrong for a
/// `Float64Array`, silently — the two out-parameters are left untouched and the
/// call answers `napi_generic_failure` when an addon asked for them.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_typedarray_info(
    _env: napi_env,
    value: napi_value,
    kind: *mut i32,
    length: *mut usize,
    data: *mut *mut c_void,
    arraybuffer: *mut napi_value,
    byte_offset: *mut usize,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some((pointer, count)) = window(word) else {
        return napi_status::napi_invalid_arg;
    };
    if !data.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *data = pointer.cast() };
    }
    if !length.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *length = count };
    }
    // Asked for what this cannot answer. Saying so beats answering
    // `napi_uint8_array` for a `Float64Array`, which an addon would then read
    // eight times too many elements from.
    if !kind.is_null() || !arraybuffer.is_null() || !byte_offset.is_null() {
        return napi_status::napi_generic_failure;
    }
    napi_ok
}

/// `napi_create_arraybuffer`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_arraybuffer(
    env: napi_env,
    byte_length: usize,
    data: *mut *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    // A `Uint8Array` over fresh bytes rather than the bare byte store: this
    // engine's `ArrayBuffer` cell has no window of its own that
    // `bytes_pointer` can answer for, and an addon that asked for an
    // arraybuffer wants somewhere to write. The difference is observable —
    // `x instanceof ArrayBuffer` is false — and is named here rather than
    // hidden, because pretending would be worse than the honest mismatch.
    let word = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_bytes(context, &vec![0u8; byte_length])
    });
    if !data.is_null() {
        let Some((pointer, _)) = window(word) else {
            return napi_status::napi_generic_failure;
        };
        // SAFETY: the caller's contract.
        unsafe { *data = pointer.cast() };
    }
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_detach_arraybuffer` — give the bytes back.
///
/// Follows a view to the buffer behind it, because `napi_create_arraybuffer`
/// here answers a `Uint8Array` (see its own note) and an addon detaching what
/// it was given means the store either way.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_detach_arraybuffer(
    _env: napi_env,
    arraybuffer: napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(arraybuffer) }) else {
        return napi_status::napi_invalid_arg;
    };
    match rts_core::entry::detach_buffer(word) {
        true => napi_status::napi_ok,
        // Already detached, or never a buffer. The ABI has a status for the
        // second and none for the first, and they are the same sentence from
        // the addon's side: there are no bytes here to take away.
        false => napi_status::napi_arraybuffer_expected,
    }
}

/// `napi_is_detached_arraybuffer`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_detached_arraybuffer(
    _env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_status::napi_invalid_arg;
    };
    if result.is_null() {
        return napi_status::napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::buffer_detached(word) };
    napi_status::napi_ok
}
