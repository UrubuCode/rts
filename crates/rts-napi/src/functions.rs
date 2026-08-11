//! Calling in both directions.
//!
//! P3. A JS program calls a function the addon wrote, and the addon calls a
//! function the program wrote. The second is a forwarder like everything in
//! P2; the first is the phase's real decision.
//!
//! # Where a callback's identity lives
//!
//! `rts-core` makes a callable out of a bare `extern "C"` function pointer, and
//! a bare pointer carries no identity: one trampoline shared by every addon
//! function could not tell which of them it was standing in for. What it DOES
//! carry is an environment — `closure_new(code, environment)` stores a value
//! beside the code and hands it back as the call's first argument, which is how
//! a closure finds what it closed over.
//!
//! So the environment is the identity: a number naming a slot in [`registry`],
//! and the trampoline reads it exactly the way a compiled closure reads its
//! captured scope. Nothing is keyed by the function's value, which is the
//! design this crate's PLAN ruled out — a map from callable to callback would
//! need the callable's identity to be stable through a collection, and would
//! answer a lookup per call for something the call already carries.
//!
//! # Why the registry is thread-local rather than global
//!
//! Because a `Context` is. An addon's callback can only run against the context
//! it was registered under, that context is reached through a thread-local
//! (`rts-core`'s `entry` module says why), and so a callback registered on one
//! thread could never legally fire on another. A global would be a lock around
//! something with one legal user.
//!
//! # What a slot is not
//!
//! Reference counted, and not reused while an addon might still hold the
//! callable. Slots come back when the [`crate::env::Env`] that made them is
//! destroyed — which is the point at which the addon is unloaded and its
//! function pointers stop being callable at all.

use core::cell::RefCell;
use core::ffi::c_void;

use crate::abi::{napi_callback, napi_callback_info, napi_env, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_function_expected, napi_invalid_arg, napi_ok};

/// One registered addon function.
#[derive(Clone, Copy)]
struct Registered {
    /// What the addon gave us to call.
    code: napi_callback,
    /// The pointer it asked to have handed back.
    data: *mut c_void,
    /// Which environment registered it, so its slots can be freed together.
    owner: *mut c_void,
}

thread_local! {
    /// Every addon function registered on this thread.
    ///
    /// A `Vec` of holes rather than a map: the slot number IS the key and the
    /// caller already holds it, so there is nothing to look up by.
    static REGISTRY: RefCell<Vec<Option<Registered>>> = const { RefCell::new(Vec::new()) };
}

/// Registers `code`/`data` and answers the slot naming them.
fn register(code: napi_callback, data: *mut c_void, owner: *mut c_void) -> usize {
    REGISTRY.with_borrow_mut(|slots| {
        let entry = Registered { code, data, owner };
        match slots.iter().position(Option::is_none) {
            Some(free) => {
                slots[free] = Some(entry);
                free
            }
            None => {
                slots.push(Some(entry));
                slots.len() - 1
            }
        }
    })
}

/// Forgets every function an environment registered.
///
/// Called when the environment is destroyed, which is when the addon is
/// unloaded and its code stops existing. Freeing earlier would let a slot be
/// reused while a callable still names it, and a call would land on the wrong
/// addon function rather than failing.
pub fn forget(owner: napi_env) {
    REGISTRY.with_borrow_mut(|slots| {
        for slot in slots.iter_mut() {
            if slot.is_some_and(|entry| entry.owner == owner.0) {
                *slot = None;
            }
        }
    });
}

/// What a call hands its callback, and what `napi_get_cb_info` reads back.
///
/// Lives on the trampoline's own stack for exactly the length of the call,
/// which is the ABI's lifetime for a `napi_callback_info`: valid inside the
/// callback and undefined afterwards.
struct CallInfo {
    arguments: [napi_value; crate::env::ARGUMENTS],
    given: usize,
    this: napi_value,
    data: *mut c_void,
}

/// The one function every addon callable is made out of.
///
/// Reads its slot from the environment, the way a compiled closure reads its
/// captured scope, and stands in for the addon function that slot names.
extern "C" fn trampoline(environment: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let undefined = rts_core::entry::undefined_value();
    let Some(slot) = rts_core::entry::number_of(environment) else {
        return undefined;
    };
    let Some(entry) = REGISTRY.with_borrow(|slots| {
        slots
            .get(slot as usize)
            .copied()
            .flatten()
    }) else {
        return undefined;
    };
    let Some(code) = entry.code else {
        return undefined;
    };

    // A scope of its own, opened for the call and closed after it: every handle
    // the callback makes belongs to the call, which is what the ABI promises
    // and what keeps a long-running program from accumulating roots.
    //
    // SAFETY: the owner pointer came from `Env::into_raw` and the environment
    // it names is alive as long as the addon is loaded — `forget` clears the
    // slot when it is not, and this reads the slot first.
    let Some(env) = (unsafe { env_of(napi_env(entry.owner)) }) else {
        return undefined;
    };
    env.open();

    let mut info = CallInfo {
        arguments: [
            env.current().handle(a0),
            env.current().handle(a1),
            env.current().handle(a2),
            env.current().handle(a3),
        ],
        given: given_count(&[a0, a1, a2, a3], undefined),
        this: env.current().handle(this),
        data: entry.data,
    };

    // SAFETY: `code` is the addon's own function, called with the environment
    // it registered under and a `napi_callback_info` valid for this call. That
    // is the ABI's contract in both directions, and nothing further can be
    // checked from this side.
    let produced = unsafe {
        code(
            napi_env(entry.owner),
            napi_callback_info((&mut info as *mut CallInfo).cast()),
        )
    };
    // Read BEFORE the scope closes: the handle the addon answered belongs to
    // that scope, and reading it afterwards would read a released slot.
    // SAFETY: a handle the addon got from this crate, or null.
    let answer = unsafe { value_of(produced) }.unwrap_or(undefined);

    // SAFETY: re-derived rather than held across the call, because the callback
    // may have registered functions and grown the registry.
    if let Some(env) = unsafe { env_of(napi_env(entry.owner)) } {
        env.close();
    }
    answer
}

/// How many of the four slots the caller actually passed.
///
/// The convention carries four words and pads the rest with `undefined`, so a
/// trailing `undefined` is indistinguishable from an omitted argument — and the
/// ABI's `argc` is what an addon branches on. Counting to the last non-padding
/// word is the honest reading available: `f(1, undefined)` reports one
/// argument, which is wrong in a way no addon has ever depended on, and
/// reporting four always would be wrong for every call.
fn given_count(words: &[u64], undefined: u64) -> usize {
    words
        .iter()
        .rposition(|&word| word != undefined)
        .map(|last| last + 1)
        .unwrap_or(0)
}

/// A callable value over an addon callback, as an engine word.
///
/// Public because a class is made of these: `napi_define_class` turns a method
/// descriptor into one of these per method, and building them there would mean
/// a second copy of the registration and the environment trick. The handle is
/// the caller's to make — some of these are hung on a prototype rather than
/// handed to the addon.
pub fn callable_word(env: napi_env, cb: napi_callback, data: *mut c_void) -> u64 {
    let slot = register(cb, data, env.0);
    let environment = rts_core::entry::make_number(slot as f64);
    // Through a pointer rather than straight to an integer: casting a function
    // item to `usize` is what `function_casts_as_integer` warns about, and the
    // two-step spelling is the one that says "this is an address".
    rts_core::entry::closure_new(trampoline as *const () as usize as i64, environment)
}

/// `napi_create_function` — a JS callable whose body is the addon's.
///
/// # Safety
///
/// `utf8name` must be null or NUL-terminated, and `cb` must stay callable for
/// as long as `env` lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_function(
    env: napi_env,
    utf8name: *const core::ffi::c_char,
    _length: usize,
    cb: napi_callback,
    data: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let _ = utf8name;
    if cb.is_none() {
        return napi_invalid_arg;
    }
    let word = callable_word(env, cb, data);

    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(word);
    // SAFETY: the caller's contract.
    match unsafe { write_out(result, handle) } {
        true => napi_ok,
        false => napi_invalid_arg,
    }
}

/// `napi_get_cb_info` — what this call was given.
///
/// Every out-parameter is optional, which is how addons use it: most ask for
/// `argc`/`argv`, some for `this`, few for `data`.
///
/// # Safety
///
/// `cbinfo` must be the one the current callback was handed, and each non-null
/// out-parameter writable. `argv` must have room for `*argc` handles.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_cb_info(
    _env: napi_env,
    cbinfo: napi_callback_info,
    argc: *mut usize,
    argv: *mut napi_value,
    this: *mut napi_value,
    data: *mut *mut c_void,
) -> napi_status {
    if cbinfo.0.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract — the pointer the trampoline handed over.
    let info = unsafe { &*cbinfo.0.cast::<CallInfo>() };

    if !argc.is_null() {
        // SAFETY: the caller's contract.
        let room = unsafe { *argc };
        if !argv.is_null() {
            // The ABI's rule: fill up to `room`, pad the rest with `undefined`,
            // and report how many there WERE — not how many were copied, which
            // is what tells an addon it under-sized its buffer.
            for at in 0..room {
                let handle = match info.arguments.get(at) {
                    Some(handle) if at < info.given => *handle,
                    _ => crate::handles::none(),
                };
                // SAFETY: the caller's contract — `room` writable handles.
                unsafe { *argv.add(at) = handle };
            }
        }
        // SAFETY: the caller's contract.
        unsafe { *argc = info.given };
    }
    if !this.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *this = info.this };
    }
    if !data.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *data = info.data };
    }
    napi_ok
}

/// `napi_call_function` — the addon calling a JS function.
///
/// # Safety
///
/// `argv` must point at `argc` handles from an open scope.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_call_function(
    env: napi_env,
    recv: napi_value,
    func: napi_value,
    argc: usize,
    argv: *const napi_value,
    result: *mut napi_value,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(callee) = (unsafe { value_of(func) }) else {
        return napi_invalid_arg;
    };
    let this = match unsafe { value_of(recv) } {
        Some(word) => word,
        None => rts_core::entry::undefined_value(),
    };
    if !rts_core::entry::with_runtime(|context| rts_core::entry::is_callable_in(context, callee)) {
        return napi_function_expected;
    }

    let mut words = Vec::with_capacity(argc);
    for at in 0..argc {
        if argv.is_null() {
            return napi_invalid_arg;
        }
        // SAFETY: the caller's contract — `argc` readable handles.
        let handle = unsafe { *argv.add(at) };
        // SAFETY: a handle from an open scope.
        match unsafe { value_of(handle) } {
            Some(word) => words.push(word),
            None => return napi_invalid_arg,
        }
    }
    let arguments = rts_core::entry::make_array(words);
    let produced = rts_core::entry::call_with_args(callee, this, arguments);

    // Rule 8 of `rts-core`'s README, from the outside: a call that left a throw
    // behind produced no answer, and handing one back would be handing back
    // `undefined` as though the call had succeeded.
    if rts_core::entry::pending().is_some() {
        return napi_status::napi_pending_exception;
    }

    // SAFETY: the caller's contract.
    let Some(env) = (unsafe { env_of(env) }) else {
        return napi_invalid_arg;
    };
    let handle = env.current().handle(produced);
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = handle };
    }
    napi_ok
}

/// `napi_is_callable`, spelled `napi_is_function` by some headers.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_is_callable(
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
    let callable =
        rts_core::entry::with_runtime(|context| rts_core::entry::is_callable_in(context, word));
    // SAFETY: the caller's contract.
    unsafe { *result = callable };
    napi_ok
}
