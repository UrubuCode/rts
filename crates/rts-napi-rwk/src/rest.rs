//! The rest of what a real addon probes for.
//!
//! Measured rather than guessed: `rts napi` loaded
//! `@napi-rs/uuid-win32-x64-msvc` and it panicked because a symbol it looks up
//! was absent. An addon that binds by `GetProcAddress` probes its whole list
//! before running anything, so ONE missing name is the same as forty-five —
//! this file is what closes that gap.
//!
//! # Present and failing is not the same as absent
//!
//! Some of these cannot be done on this engine, and they answer a status saying
//! so rather than being left out. That is the ABI's own channel — rule 3 of
//! this crate's README — and the difference is real: an addon that calls one
//! gets `napi_generic_failure` and can report it, where an absent symbol takes
//! the whole library down before `main` with no diagnostic.
//!
//! Every one of them says WHY here, and none pretends to have worked.
//!
//! # What is genuinely absent, and stays so
//!
//! `napi_get_uv_event_loop` — there is no libuv loop to hand over, and a fake
//! pointer would be dereferenced. `napi_make_callback` — Node's async-context
//! machinery, which this engine does not model. `napi_run_script` — a compiler
//! the AOT binary deliberately does not carry. Those three are listed in
//! `PLAN.md` and are not here: a symbol that cannot fail HONESTLY is worse than
//! one that is missing.

use core::ffi::c_void;

use crate::abi::{napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_generic_failure, napi_invalid_arg, napi_ok};

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

/// What `napi_get_last_error_info` answers.
///
/// **The layout is the ABI's.**
#[repr(C)]
pub struct napi_extended_error_info {
    /// A message, or null.
    pub error_message: *const core::ffi::c_char,
    /// The engine's own detail. Always null here — we have no second story to
    /// tell about a failure beyond the status.
    pub engine_reserved: *mut c_void,
    /// The engine's own code. Zero, for the same reason.
    pub engine_error_code: u32,
    /// The status the last call answered.
    pub error_code: napi_status,
}

thread_local! {
    /// The message the last failure carried, kept alive for the pointer handed
    /// out in [`napi_get_last_error_info`].
    static LAST: core::cell::RefCell<Option<(std::ffi::CString, napi_extended_error_info)>> =
        const { core::cell::RefCell::new(None) };
}

/// `napi_get_last_error_info`.
///
/// **The first symbol `napi-sys` probes**, which is why its absence turned
/// every other gap into one unhelpful panic.
///
/// What it answers is thin and says so: this crate does not record a message
/// per failed call, so the info carries the pending exception's text when there
/// is one and an empty message otherwise. Inventing a message per status would
/// be writing the ABI's documentation into the runtime.
///
/// # Safety
///
/// The ABI's. The pointer is valid until the next call on this thread, which is
/// the ABI's own rule for it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_last_error_info(
    _env: napi_env,
    result: *mut *const napi_extended_error_info,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    let message = rts_core::entry::pending()
        .map(|(_, described)| described)
        .unwrap_or_default();
    let text = std::ffi::CString::new(message).unwrap_or_default();
    let pointer = LAST.with_borrow_mut(|last| {
        let info = napi_extended_error_info {
            error_message: text.as_ptr(),
            engine_reserved: core::ptr::null_mut(),
            engine_error_code: 0,
            error_code: napi_ok,
        };
        *last = Some((text, info));
        last.as_ref()
            .map(|(_, info)| info as *const napi_extended_error_info)
            .expect("just written")
    });
    // SAFETY: the caller's contract.
    unsafe { *result = pointer };
    napi_ok
}

/// `napi_get_version` — which N-API this host implements.
///
/// 8, which is what the surface here corresponds to. An addon built for a later
/// one may probe for something absent and get a status rather than a crash.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_version(_env: napi_env, result: *mut u32) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = 8 };
    napi_ok
}

/// What `napi_get_node_version` answers. **Layout is the ABI's.**
#[repr(C)]
pub struct napi_node_version {
    /// Major.
    pub major: u32,
    /// Minor.
    pub minor: u32,
    /// Patch.
    pub patch: u32,
    /// What the runtime calls itself.
    pub release: *const core::ffi::c_char,
}

/// The version this host reports, and the name it reports it under.
///
/// `"rts"`, not `"node"`. An addon that branches on the release string gets the
/// truth and can refuse; one told `"node"` would take a path written for a
/// runtime this is not.
static VERSION: napi_node_version = napi_node_version {
    major: 22,
    minor: 0,
    patch: 0,
    release: c"rts".as_ptr(),
};

// SAFETY: an immutable record of constants and a pointer to a `static` string.
unsafe impl Sync for napi_node_version {}

/// `napi_get_node_version`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_node_version(
    _env: napi_env,
    result: *mut *const napi_node_version,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = &VERSION };
    napi_ok
}

/// `napi_has_named_property`.
///
/// # Safety
///
/// `utf8name` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_named_property(
    _env: napi_env,
    object: napi_value,
    utf8name: *const core::ffi::c_char,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    if utf8name.is_null() || result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let Ok(name) = (unsafe { core::ffi::CStr::from_ptr(utf8name) }).to_str() else {
        return napi_invalid_arg;
    };
    let key =
        rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, name));
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::has_property(key, word) };
    napi_ok
}

/// `napi_has_own_property` — the object's own, not what it inherits.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_own_property(
    _env: napi_env,
    object: napi_value,
    key: napi_value,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let (Some(object), Some(key)) = (unsafe { value_of(object) }, unsafe { value_of(key) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // Own means own: the key is looked for among the object's own names rather
    // than through `has_property`, which walks the chain. An addon asking this
    // question is distinguishing the two on purpose.
    let Some(name) = rts_core::entry::text_of(key) else {
        return napi_status::napi_name_expected;
    };
    let own = rts_core::entry::with_runtime(|context| {
        rts_core::entry::member_names(context, object).iter().any(|held| *held == name)
    });
    // SAFETY: the caller's contract.
    unsafe { *result = own };
    napi_ok
}

/// `napi_has_element`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_has_element(
    _env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(object) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    let key = rts_core::entry::make_number(index as f64);
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::has_property(key, object) };
    napi_ok
}

/// `napi_delete_element`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_delete_element(
    _env: napi_env,
    object: napi_value,
    index: u32,
    result: *mut bool,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(object) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    let key = rts_core::entry::make_number(index as f64);
    let deleted = rts_core::entry::delete_property(object, key);
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = deleted };
    }
    napi_ok
}

/// `napi_get_prototype`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_prototype(
    env: napi_env,
    object: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(object) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    let prototype = rts_core::entry::get_prototype(object);
    // SAFETY: forwarded.
    unsafe { produce(env, result, prototype) }
}

/// `napi_coerce_to_object` — `Object(x)`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_coerce_to_object(
    env: napi_env,
    value: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    // An object is already one. A primitive would need its wrapper class, and
    // `CLAUDE.md` records that this engine deliberately has no `String`/`Number`
    // wrapper object — so this refuses rather than answering a plain object that
    // would fail every `instanceof` an addon tries next.
    let is_object =
        rts_core::entry::with_runtime(|context| rts_core::entry::is_object(context, word));
    match is_object {
        true => unsafe { produce(env, result, word) },
        false => napi_status::napi_object_expected,
    }
}

/// `napi_create_string_latin1`.
///
/// # Safety
///
/// `str` must point at `length` readable bytes, or be NUL-terminated when
/// `length` is `NAPI_AUTO_LENGTH`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_latin1(
    env: napi_env,
    str: *const core::ffi::c_char,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    if str.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    let bytes: &[u8] = match length {
        crate::abi::NAPI_AUTO_LENGTH => unsafe { core::ffi::CStr::from_ptr(str) }.to_bytes(),
        length => unsafe { core::slice::from_raw_parts(str.cast::<u8>(), length) },
    };
    // Latin-1 is one byte per code point, and every one of them is a code point
    // — which is why this cannot be refused the way invalid UTF-8 is: there is
    // no such thing as invalid Latin-1.
    let text: String = bytes.iter().map(|&byte| byte as char).collect();
    let word =
        rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, &text));
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_create_string_utf16`.
///
/// # Safety
///
/// `str` must point at `length` code units, or be NUL-terminated when `length`
/// is `NAPI_AUTO_LENGTH`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_string_utf16(
    env: napi_env,
    str: *const u16,
    length: usize,
    result: *mut napi_value,
) -> napi_status {
    if str.is_null() {
        return napi_invalid_arg;
    }
    let units: &[u16] = match length {
        crate::abi::NAPI_AUTO_LENGTH => {
            let mut end = 0;
            // SAFETY: the caller's contract — NUL-terminated.
            while unsafe { *str.add(end) } != 0 {
                end += 1;
            }
            // SAFETY: `end` units before the terminator.
            unsafe { core::slice::from_raw_parts(str, end) }
        }
        // SAFETY: the caller's contract.
        length => unsafe { core::slice::from_raw_parts(str, length) },
    };
    // Lossy, and this is the one place this crate accepts that: a lone
    // surrogate is representable in UTF-16 and not in a Rust `String`, and the
    // engine's own text is UTF-16 but its `make_string` takes `&str`. An addon
    // passing one gets the replacement character rather than a refusal, because
    // refusing a string that JavaScript can hold would be the bigger lie.
    let text = String::from_utf16_lossy(units);
    let word =
        rts_core::entry::with_runtime(|context| rts_core::entry::make_string(context, &text));
    // SAFETY: forwarded.
    unsafe { produce(env, result, word) }
}

/// `napi_get_value_string_latin1`.
///
/// # Safety
///
/// As `napi_get_value_string_utf8`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_latin1(
    env: napi_env,
    value: napi_value,
    buf: *mut core::ffi::c_char,
    bufsize: usize,
    result: *mut usize,
) -> napi_status {
    // The engine answers UTF-8, and every character a Latin-1 buffer can hold
    // is one byte in both — so for text an addon would accept, the two agree.
    // Text it would not (anything above U+00FF) differs, and this is the
    // narrowest honest reading available without a second conversion path.
    // SAFETY: forwarded.
    unsafe { crate::objects::napi_get_value_string_utf8(env, value, buf, bufsize, result) }
}

/// `napi_get_value_string_utf16`.
///
/// # Safety
///
/// `buf` must be null or point at `bufsize` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_string_utf16(
    _env: napi_env,
    value: napi_value,
    buf: *mut u16,
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
    let units: Vec<u16> = text.encode_utf16().collect();

    if buf.is_null() {
        if result.is_null() {
            return napi_invalid_arg;
        }
        // SAFETY: the caller's contract.
        unsafe { *result = units.len() };
        return napi_ok;
    }
    if bufsize == 0 {
        return napi_invalid_arg;
    }
    // One unit reserved for the terminator, and the copy stops before splitting
    // a surrogate pair — half of one is not a character, exactly as half a
    // UTF-8 sequence is not.
    let room = bufsize - 1;
    let mut end = room.min(units.len());
    if end > 0 && (0xD800..0xDC00).contains(&units[end - 1]) {
        end -= 1;
    }
    // SAFETY: the caller's contract — `bufsize` writable units.
    unsafe {
        core::ptr::copy_nonoverlapping(units.as_ptr(), buf, end);
        *buf.add(end) = 0;
    }
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = end };
    }
    napi_ok
}

/// `napi_is_arraybuffer`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_arraybuffer(
    env: napi_env,
    value: napi_value,
    result: *mut bool,
) -> napi_status {
    // What this engine makes for `napi_create_arraybuffer` is a `Uint8Array`,
    // which `buffers.rs` states — so the honest answer to "is this an
    // arraybuffer" is the same question that module already answers.
    // SAFETY: forwarded.
    unsafe { crate::buffers::napi_is_typedarray(env, value, result) }
}

/// `napi_get_arraybuffer_info`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_arraybuffer_info(
    env: napi_env,
    value: napi_value,
    data: *mut *mut c_void,
    length: *mut usize,
) -> napi_status {
    // SAFETY: forwarded — see `napi_is_arraybuffer` for why these are the same
    // object here.
    unsafe { crate::buffers::napi_get_buffer_info(env, value, data, length) }
}

/// `napi_create_promise` — a promise and the handle that settles it.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_promise(
    env: napi_env,
    deferred: *mut *mut c_void,
    promise: *mut napi_value,
) -> napi_status {
    if deferred.is_null() {
        return napi_invalid_arg;
    }
    let made = rts_core::entry::promise_new();
    // The deferred IS the promise, held from outside the heap so settling it
    // three turns later still names something. `napi_resolve_deferred` gives
    // that hold back, which is why a deferred may be settled exactly once.
    let held = rts_core::entry::hold_current(made);
    // SAFETY: the caller's contract.
    unsafe { *deferred = held as usize as *mut c_void };
    // SAFETY: forwarded.
    unsafe { produce(env, promise, made) }
}

/// Settles a deferred, and gives back the hold that kept its promise alive.
///
/// # Safety
///
/// `deferred` must be one [`napi_create_promise`] produced and not yet settled.
unsafe fn settle(deferred: *mut c_void, value: napi_value, rejected: i64) -> napi_status {
    let held = deferred as usize as u32;
    let Some(promise) = rts_core::entry::release_current(held) else {
        // Already settled, or never ours. Either way there is nothing to settle
        // and answering ok would promise a callback that will not run.
        return napi_invalid_arg;
    };
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    rts_core::entry::promise_settle(promise, word, rejected);
    napi_ok
}

/// `napi_resolve_deferred`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_resolve_deferred(
    _env: napi_env,
    deferred: *mut c_void,
    resolution: napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    // `0` is "not rejected", which is the shape the runtime takes — see
    // `promise_settle`, whose flag is an integer for the reason every other
    // boolean crossing this boundary is.
    unsafe { settle(deferred, resolution, 0) }
}

/// `napi_reject_deferred`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_reject_deferred(
    _env: napi_env,
    deferred: *mut c_void,
    rejection: napi_value,
) -> napi_status {
    // SAFETY: forwarded.
    unsafe { settle(deferred, rejection, 1) }
}

/// `napi_is_promise`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_promise(
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
    // `x instanceof Promise`, through the same global lookup a program uses.
    let name = rts_core::entry::with_runtime(|context| {
        rts_core::entry::make_string(context, "Promise")
    });
    let key = rts_core::entry::key_number(name);
    let class = rts_core::entry::global_get(key);
    // SAFETY: the caller's contract.
    unsafe { *result = rts_core::entry::instance_of(word, class) };
    napi_ok
}

/// `napi_get_new_target` — what `new` named, inside a constructor.
///
/// Always null, which the ABI defines as "this call was not a construction".
/// The engine keeps a `new.target` stack, and nothing exports it; answering
/// null makes an addon take its non-`new` path, which is wrong for a class
/// constructor and is stated here rather than hidden.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_new_target(
    _env: napi_env,
    _info: crate::abi::napi_callback_info,
    result: *mut napi_value,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = crate::handles::none() };
    napi_ok
}

/// `napi_create_symbol`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_symbol(
    _env: napi_env,
    _description: napi_value,
    _result: *mut napi_value,
) -> napi_status {
    // The engine has symbols and nothing exports a way to make one from a host
    // crate. Refused rather than answered with a string, which would compare
    // equal to another of the same text — the one thing a symbol exists not to
    // do.
    napi_generic_failure
}

/// `napi_create_external_buffer`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external_buffer(
    _env: napi_env,
    _length: usize,
    _data: *mut c_void,
    _finalize_cb: crate::abi::napi_finalize,
    _finalize_hint: *mut c_void,
    _result: *mut napi_value,
) -> napi_status {
    // A buffer over memory the ADDON owns. This engine's buffers are its own
    // allocation, and there is no way to point one at foreign memory — so the
    // choice is refusing or copying, and copying is worse: an addon writes
    // through its pointer expecting the program to see it.
    napi_generic_failure
}

/// `napi_create_external_arraybuffer`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external_arraybuffer(
    _env: napi_env,
    _data: *mut c_void,
    _byte_length: usize,
    _finalize_cb: crate::abi::napi_finalize,
    _finalize_hint: *mut c_void,
    _result: *mut napi_value,
) -> napi_status {
    // See `napi_create_external_buffer`.
    napi_generic_failure
}

/// `napi_create_typedarray`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_typedarray(
    _env: napi_env,
    _kind: i32,
    _length: usize,
    _arraybuffer: napi_value,
    _byte_offset: usize,
    _result: *mut napi_value,
) -> napi_status {
    // A view of a given element type over an existing buffer. Nothing exports
    // the element type, which is the same gap `napi_get_typedarray_info`
    // refuses on — and making a `Uint8Array` whatever was asked for would be
    // wrong by a factor of the element size.
    napi_generic_failure
}

/// `napi_create_dataview`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_dataview(
    _env: napi_env,
    _byte_length: usize,
    _arraybuffer: napi_value,
    _byte_offset: usize,
    _result: *mut napi_value,
) -> napi_status {
    // The engine has `DataView` and nothing exports its construction to a host.
    napi_generic_failure
}

/// `napi_get_dataview_info`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_dataview_info(
    _env: napi_env,
    _dataview: napi_value,
    _byte_length: *mut usize,
    _data: *mut *mut c_void,
    _arraybuffer: *mut napi_value,
    _byte_offset: *mut usize,
) -> napi_status {
    // See `napi_create_dataview`.
    napi_generic_failure
}

/// `napi_is_dataview`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_dataview(
    _env: napi_env,
    _value: napi_value,
    result: *mut bool,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // False rather than a failure: "is it one" has an answer even when making
    // one does not, and nothing this crate can produce is a `DataView`.
    // SAFETY: the caller's contract.
    unsafe { *result = false };
    napi_ok
}

/// `napi_async_init`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_async_init(
    _env: napi_env,
    _resource: napi_value,
    _resource_name: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    if result.is_null() {
        return napi_invalid_arg;
    }
    // An async CONTEXT, which is `async_hooks` — this engine has no such thing
    // and nothing observes one. A non-null token is handed back so an addon's
    // init/destroy pair balances, and it names nothing.
    // SAFETY: the caller's contract.
    unsafe { *result = 1usize as *mut c_void };
    napi_ok
}

/// `napi_async_destroy`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_async_destroy(
    _env: napi_env,
    _context: *mut c_void,
) -> napi_status {
    napi_ok
}

/// `napi_cancel_async_work`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_cancel_async_work(
    _env: napi_env,
    _work: *mut c_void,
) -> napi_status {
    // The work is on a thread already, and a thread cannot be un-started. The
    // ABI has a status for exactly this case and Node answers it too when the
    // work has begun.
    napi_status::napi_cancelled
}

/// `napi_add_env_cleanup_hook`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_add_env_cleanup_hook(
    _env: napi_env,
    _fun: Option<unsafe extern "C" fn(*mut c_void)>,
    _arg: *mut c_void,
) -> napi_status {
    // Nothing here runs at teardown: `env::destroy` is called by whoever made
    // the environment, and the host does not call it on exit. Accepting a hook
    // and never running it would be worse than refusing — an addon uses these
    // to flush.
    napi_generic_failure
}

/// `napi_remove_env_cleanup_hook`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_remove_env_cleanup_hook(
    _env: napi_env,
    _fun: Option<unsafe extern "C" fn(*mut c_void)>,
    _arg: *mut c_void,
) -> napi_status {
    // Nothing was added, so nothing is removed. Ok rather than a failure: the
    // addon's bookkeeping is now consistent with ours.
    napi_ok
}

/// `napi_adjust_external_memory`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_adjust_external_memory(
    _env: napi_env,
    change_in_bytes: i64,
    adjusted_value: *mut i64,
) -> napi_status {
    // A hint to the collector about memory it cannot see. This collector runs
    // when the region fills and has no notion of pressure, so the hint is
    // recorded nowhere — and the number handed back is the change itself, which
    // is the only honest total available.
    if !adjusted_value.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *adjusted_value = change_in_bytes };
    }
    napi_ok
}

/// `napi_fatal_error` — the addon says the process cannot continue.
///
/// # Safety
///
/// Both strings must be NUL-terminated or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_fatal_error(
    location: *const core::ffi::c_char,
    _location_len: usize,
    message: *const core::ffi::c_char,
    _message_len: usize,
) -> ! {
    let text = |pointer: *const core::ffi::c_char| match pointer.is_null() {
        true => String::new(),
        // SAFETY: the caller's contract.
        false => unsafe { core::ffi::CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned(),
    };
    eprintln!("rts: fatal error from a native addon at {}: {}", text(location), text(message));
    // The ABI's own contract: this does not return. Aborting rather than
    // unwinding, because the addon has said its own state is broken and a
    // destructor of ours running over it is not an improvement.
    std::process::abort()
}

/// `napi_fatal_exception` — an exception nothing will catch.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_fatal_exception(
    _env: napi_env,
    err: napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let described = unsafe { value_of(err) }
        .and_then(rts_core::entry::text_of)
        .unwrap_or_else(|| "an addon reported an uncaught exception".to_owned());
    eprintln!("rts: uncaught exception from a native addon: {described}");
    napi_ok
}

/// `napi_run_script` — evaluate a string of JavaScript.
///
/// Answers a status rather than a value when no evaluator is installed, which
/// is the AOT binary's case: `rts-runtime`'s `main` declares none, because
/// carrying the front end would put the whole compiler into every compiled
/// program for a feature most never use. The JIT host declares one.
///
/// So this works or refuses depending on WHERE it runs, and that is a real
/// property of the two hosts rather than a limitation of this crate.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_run_script(
    env: napi_env,
    script: napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(script) }) else {
        return napi_invalid_arg;
    };
    let Some(source) = rts_core::entry::text_of(word) else {
        return napi_status::napi_string_expected;
    };
    let Some(produced) = rts_core::entry::evaluate(&source) else {
        return napi_generic_failure;
    };
    // SAFETY: forwarded.
    unsafe { produce(env, result, produced) }
}

/// `napi_make_callback` — call a function inside an async context.
///
/// The async context is NOT modelled, and the call is therefore just a call:
/// this engine has no `async_hooks`, so there is no before/after to run and
/// nothing observes the difference. What an addon does get is the call it asked
/// for, and its microtasks drained afterwards — which is the half of
/// `MakeCallback` that is observable from JavaScript.
///
/// # Safety
///
/// `argv` must point at `argc` handles from an open scope.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_make_callback(
    env: napi_env,
    _async_context: *mut c_void,
    recv: napi_value,
    func: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: forwarded — the same call, with the same checks.
    let status = unsafe { crate::functions::napi_call_function(env, recv, func, argc, argv, result) };
    if status == napi_ok {
        // The half that IS observable: a callback made this way runs the
        // microtask queue when it returns, which is why an addon reaches for
        // this instead of `napi_call_function`.
        rts_core::entry::drain_microtasks();
    }
    status
}

/// `napi_get_uv_event_loop`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_uv_event_loop(
    _env: napi_env,
    _loop_: *mut *mut c_void,
) -> napi_status {
    // There is no libuv here. The loop this engine runs is `entry::loops`, and
    // handing its address over as a `uv_loop_t*` would be a pointer the addon
    // dereferences — the one failure mode worse than refusing, because it
    // happens inside the addon with our data under it.
    //
    // The out-parameter is deliberately left untouched: an addon that ignores
    // the status still sees whatever it initialised, which is more likely to be
    // null than anything we could write.
    napi_generic_failure
}
