//! Freezing, sealing, and asking for every key.
//!
//! # Why these three are together
//!
//! They are the operations that ask about an object's SHAPE rather than its
//! contents, and each of them is answered by the language rather than by
//! reaching into the engine: `Object.freeze`, `Object.seal` and the prototype
//! chain, through the same door a program uses. Rule 5 again — an ABI crate
//! that decided what freezing means would be a second answer to a question
//! `rts-core` already answers, and the two would drift.
//!
//! # `objects.rs` is full
//!
//! The file ceiling in `CLAUDE.md` is 500 lines and `objects.rs` is at 466, so
//! these land in a small module of their own rather than on the end of a file
//! that is already at its limit.

use crate::abi::{napi_env, napi_status, napi_value};
use crate::dates::global;
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_object_expected, napi_ok};

/// Whether to walk the prototype chain. **The order is the ABI.**
#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum napi_key_collection_mode {
    napi_key_include_prototypes = 0,
    napi_key_own_only,
}

/// Which keys to include. A bit set, so not an enum. **The values are the ABI.**
pub mod napi_key_filter {
    /// Every key, whatever its attributes.
    pub const ALL_PROPERTIES: u32 = 0;
    /// Only writable ones.
    pub const WRITABLE: u32 = 1;
    /// Only enumerable ones.
    pub const ENUMERABLE: u32 = 2;
    /// Only configurable ones.
    pub const CONFIGURABLE: u32 = 4;
    /// Leave out keys that are strings.
    pub const SKIP_STRINGS: u32 = 8;
    /// Leave out keys that are symbols.
    pub const SKIP_SYMBOLS: u32 = 16;
}

/// How to spell the keys that come back. **The order is the ABI.**
#[allow(missing_docs)]
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum napi_key_conversion {
    napi_key_keep_numbers = 0,
    napi_key_numbers_to_strings,
}

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

/// Calls `Object.<name>(object)` and answers whether the call was made.
///
/// Three separate borrows, for the reason `dates.rs` states: each of these
/// entry points reaches the thread's context itself.
fn object_call(name: &str, word: u64) -> bool {
    let object_class = global("Object");
    let method = crate::dates::member(object_class, name);
    if !rts_core::entry::with_runtime(|context| rts_core::entry::is_callable_in(context, method)) {
        return false;
    }
    let arguments = rts_core::entry::make_array(vec![word]);
    rts_core::entry::call_with_args(method, object_class, arguments);
    true
}

/// `napi_object_freeze`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_object_freeze(_env: napi_env, object: napi_value) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    match object_call("freeze", word) {
        true => napi_ok,
        false => napi_status::napi_generic_failure,
    }
}

/// `napi_object_seal`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_object_seal(_env: napi_env, object: napi_value) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    match object_call("seal", word) {
        true => napi_ok,
        false => napi_status::napi_generic_failure,
    }
}

/// `napi_get_all_property_names`.
///
/// # What of the three parameters is honoured
///
/// `key_mode` is: own-only stops at the object, and include-prototypes walks
/// the chain and drops repeats, nearest wins — which is the order `for-in`
/// produces.
///
/// `key_filter` is honoured for the two SKIP bits, which are the ones that
/// change which keys exist. `WRITABLE`, `ENUMERABLE` and `CONFIGURABLE` are
/// read and NOT applied: the engine holds those attributes
/// (`entry/integrity.rs`) and does not expose them per key to this crate, so
/// filtering here would answer a plausible list rather than the real one.
/// Stated rather than silently dropped — an addon asking for enumerable-only
/// gets everything, which is a superset and not a lie about which keys exist.
///
/// `key_conversion` needs nothing: `member_names` already answers strings, so
/// both spellings coincide.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_all_property_names(
    env: napi_env,
    object: napi_value,
    key_mode: napi_key_collection_mode,
    key_filter: u32,
    _key_conversion: napi_key_conversion,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(object) }) else {
        return napi_invalid_arg;
    };
    if key_filter & napi_key_filter::SKIP_STRINGS != 0 {
        // Every key this engine reports is a string, so skipping strings leaves
        // nothing. An empty array rather than a failure: the question is
        // answerable and the answer is none.
        // SAFETY: forwarded.
        return unsafe { produce(env, result, rts_core::entry::make_array(Vec::new())) };
    }

    let mut names: Vec<String> = Vec::new();
    let mut level = word;
    loop {
        let held = rts_core::entry::with_runtime(|context| match rts_core::entry::is_object(context, level) {
            true => Some(rts_core::entry::member_names(context, level)),
            false => None,
        });
        let Some(held) = held else {
            // The first level not being an object is the addon's error; a later
            // one is just the end of the chain.
            match names.is_empty() && level == word {
                true => return napi_object_expected,
                false => break,
            }
        };
        for name in held {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        if key_mode == napi_key_collection_mode::napi_key_own_only {
            break;
        }
        let above = rts_core::entry::get_prototype(level);
        if above == level || above == rts_core::entry::null_value() {
            break;
        }
        level = above;
    }

    let words = rts_core::entry::with_runtime(|context| {
        names
            .iter()
            .map(|name| rts_core::entry::make_string(context, name))
            .collect::<Vec<u64>>()
    });
    // SAFETY: forwarded.
    unsafe { produce(env, result, rts_core::entry::make_array(words)) }
}
