//! Objects, their properties, and arrays.
//!
//! P2. Everything here is a forwarder to `rts-core`: `napi_get_property` asks
//! the runtime what a property is, walks no prototype chain of its own, and
//! decides no semantic — rule 5 of this crate's README.
//!
//! # Named, keyed, and indexed are three doors to one room
//!
//! The ABI has `napi_get_named_property` (a C string), `napi_get_property` (a
//! `napi_value` key) and `napi_get_element` (a `u32`). They are not three
//! operations: the second is the general one, and the other two differ only in
//! how the key arrives. So the key is turned into a value at the door and one
//! implementation answers all three — which is also what keeps `o.x`, `o["x"]`
//! and `o[0]` finding the same property from an addon as they do from a
//! program.
//!
//! # Why a missing property is `napi_ok` with `undefined`
//!
//! Because that is what the language says a missing property is, and the ABI
//! agrees: `napi_get_property` answers ok. An addon distinguishes absent from
//! present-and-undefined with `napi_has_property`, which is the same pair of
//! questions a program asks with `in`.

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_object_expected, napi_ok};

/// Puts an engine word in a handle of `env`'s innermost scope.
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

/// The two words a property operation needs: the object and the key.
///
/// # Safety
///
/// Both handles must come from an open scope.
unsafe fn pair(object: napi_value, key: napi_value) -> Option<(u64, u64)> {
    // SAFETY: the caller's contract.
    unsafe { Some((value_of(object)?, value_of(key)?)) }
}

/// The text a C string holds, or `None` when it is not UTF-8.
///
/// # Safety
///
/// `name` must be NUL-terminated.
unsafe fn name_of<'a>(name: *const core::ffi::c_char) -> Option<&'a str> {
    match name.is_null() {
        true => None,
        // SAFETY: the caller's contract.
        false => unsafe { core::ffi::CStr::from_ptr(name) }.to_str().ok(),
    }
}

/// `napi_create_object`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_object(env: napi_env, result: *mut napi_value) -> napi_status {
    let word = rts_core::entry::with_runtime(rts_core::entry::make_object);
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_set_property` — `object[key] = value`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_property(
    _env: napi_env,
    object: napi_value,
    key: napi_value,
    value: napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let (Some((object, key)), Some(value)) =
        (unsafe { pair(object, key) }, unsafe { value_of(value) })
    else {
        return napi_invalid_arg;
    };
    rts_core::entry::set_indexed(object, key, value, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    napi_ok
}

/// `napi_get_property` — `object[key]`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_property(
    env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some((object, key)) = (unsafe { pair(object, key) }) else {
        return napi_invalid_arg;
    };
    let word = rts_core::entry::get_indexed(object, key);
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_has_property` — `key in object`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_property(
    _env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some((object, key)) = (unsafe { pair(object, key) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract — `result` writable.
    unsafe { *result = rts_core::entry::has_property(key, object) };
    napi_ok
}

/// `napi_delete_property` — `delete object[key]`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_property(
    _env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some((object, key)) = (unsafe { pair(object, key) }) else {
        return napi_invalid_arg;
    };
    let deleted = rts_core::entry::delete_property(object, key);
    // The out-parameter is OPTIONAL here, unlike everywhere else: `delete` is
    // performed for its effect and most addons ignore the answer.
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = deleted };
    }
    napi_ok
}

/// `napi_set_named_property` — the same, with the key as a C string.
///
/// # Safety
///
/// `utf8name` must be NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_named_property(
    _env: napi_env,
    object: napi_value,
    utf8name: *const core::ffi::c_char,
    value: napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let (Some(object), Some(value), Some(name)) = (
        unsafe { value_of(object) },
        unsafe { value_of(value) },
        unsafe { name_of(utf8name) },
    ) else {
        return napi_invalid_arg;
    };
    // Through the SAME door `napi_set_property` uses, and this is the fix to a
    // claim this module made and did not keep. `put_member` writes a data
    // property directly; `set_indexed` is `o[k] = v`, which runs a setter if
    // there is one. With `put_member` here, a property defined by
    // `napi_define_properties` with a setter was silently overwritten by a plain
    // value — three doors, two rooms.
    let key = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_string(context, name)
    });
    rts_core::entry::set_indexed(object, key, value, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    napi_ok
}

/// `napi_get_named_property`.
///
/// # Safety
///
/// `utf8name` must be NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_named_property(
    env: napi_env,
    object: napi_value,
    utf8name: *const core::ffi::c_char,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let (Some(object), Some(name)) = (unsafe { value_of(object) }, unsafe { name_of(utf8name) })
    else {
        return napi_invalid_arg;
    };
    // `get_indexed`, not `get_member`, and the difference is a getter:
    // `get_member` reads a data property and cannot run user code, because it
    // holds the runtime's borrow while it looks. Reading a property defined by
    // `napi_define_properties` with a getter answered `undefined` through this
    // door and 7 through the keyed one — which is precisely the claim this
    // module's own documentation makes and would have been breaking.
    let key = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_string(context, name)
    });
    let word = rts_core::entry::get_indexed(object, key);
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_get_property_names` — the object's own enumerable keys, as an array.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_property_names(
    env: napi_env,
    object: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(object) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    let words = rts_core::entry::with_runtime(|context| {
        if !rts_core::entry::is_object(context, object) {
            return None;
        }
        let names = rts_core::entry::member_names(context, object);
        Some(
            names
                .iter()
                .map(|name| rts_core::entry::make_string(context, name))
                .collect::<Vec<u64>>(),
        )
    });
    let Some(words) = words else {
        return napi_object_expected;
    };
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::make_array(words)) }
}

/// `napi_create_array`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_array(env: napi_env, result: *mut napi_value) -> napi_status {
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::make_array(Vec::new())) }
}

/// `napi_create_array_with_length` — `length` holes, as `new Array(n)` makes.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_array_with_length(
    env: napi_env,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    // Filled with `undefined` rather than left as holes. The ABI's own wording
    // is that the elements are "unset", and this engine's array is a list of
    // words — a hole is a singleton it would have to invent a producer for here,
    // and an addon that reads before writing gets the same `undefined` either
    // way.
    let undefined = rts_core::entry::undefined_value();
    let words = vec![undefined; length];
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::make_array(words)) }
}

/// `napi_is_array`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_array(
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
    unsafe { *result = rts_core::entry::is_array(word) };
    napi_ok
}

/// `napi_get_array_length`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_array_length(
    _env: napi_env,
    value: napi_value,
    result: *mut u32,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    if !rts_core::entry::is_array(word) {
        return napi_status::napi_array_expected;
    }
    let length = rts_core::entry::with_runtime(|context| {
        rts_core::entry::get_member(context, word, "length")
    });
    let Some(length) = rts_core::entry::number_of(length) else {
        return napi_status::napi_array_expected;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = length as u32 };
    napi_ok
}

/// `napi_set_element` — `object[index] = value`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_set_element(
    _env: napi_env,
    object: napi_value,
    index: u32,
    value: napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let (Some(object), Some(value)) = (unsafe { value_of(object) }, unsafe { value_of(value) })
    else {
        return napi_invalid_arg;
    };
    let key = rts_core::entry::make_number(index as f64);
    rts_core::entry::set_indexed(object, key, value, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    napi_ok
}

/// `napi_get_element` — `object[index]`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_element(
    env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(object) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    let key = rts_core::entry::make_number(index as f64);
    let word = rts_core::entry::get_indexed(object, key);
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_get_value_string_utf8` — the text of a string value, as UTF-8.
///
/// With a null `buf` it measures: `result` is the byte length, NOT counting the
/// terminator. With a buffer it copies as much as fits and always terminates,
/// which is the ABI's contract and the reason a caller sizes first.
///
/// Here rather than in `values.rs` because it is the one value operation that
/// writes into memory the ADDON owns, and that is the property worth filing it
/// under: everything else hands back a handle.
///
/// # Safety
///
/// `buf` must be null or point at `bufsize` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_utf8(
    _env: napi_env,
    value: napi_value,
    buf: *mut core::ffi::c_char,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    let Some(text) = rts_core::entry::text_of(word) else {
        return napi_status::napi_string_expected;
    };
    let bytes = text.as_bytes();

    if buf.is_null() {
        if result.is_null() {
            return napi_invalid_arg;
        }
        // SAFETY: the caller's contract.
        unsafe { *result = bytes.len() };
        return napi_ok;
    }
    if bufsize == 0 {
        return napi_invalid_arg;
    }

    // One byte reserved for the terminator, and the copy truncated at a
    // CHARACTER boundary rather than at a byte: half of a multi-byte sequence
    // is not UTF-8, and an addon printing it gets a replacement character or
    // worse from its own C library.
    let room = bufsize - 1;
    let mut end = room.min(bytes.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    // SAFETY: `buf` has `bufsize` writable bytes and `end < bufsize`.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf.cast::<u8>(), end);
        *buf.add(end) = 0;
    }
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = end };
    }
    napi_ok
}
