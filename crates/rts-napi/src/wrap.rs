//! An addon's own `struct`, behind a JavaScript object.
//!
//! P5. `napi_wrap` puts a C pointer on an object the program already has;
//! `napi_create_external` makes an object that is nothing BUT a pointer. Both
//! are how a class implemented in C keeps its instance state.
//!
//! # Where the pointer lives
//!
//! `rts_core::entry::foreign` — one word beside the cell, in an `Aside`, which
//! is how that crate already says "state beside a cell" eighteen times over.
//! The engine being replaced answered this with a heap-entry kind
//! (`Entry::NapiExternal`); this heap has cells and shapes and no variant to
//! add, which is why the mechanism differs rather than being ported.
//!
//! # Where the finalizer lives, and when it runs
//!
//! Here, not in the runtime, and it has three triggers: `napi_remove_wrap`,
//! `env::destroy`, and — since P6 — the collector, through
//! `rts_core::entry::finalize`. Exactly one of them fires for a given wrap:
//! the first two withdraw the death registration, and the third IS it.
//!
//! The collector's is not immediate and must not be. The sweep runs with the
//! runtime's borrow held and a finalizer calls out; so the sweep queues and the
//! next drain calls, which is what Node does and for the same reason. An addon
//! must not assume a finalizer has run just because its object is unreachable —
//! and a program that ends before the next drain runs none of them, which is
//! what every JavaScript engine says about finalizers.
//!
//! Separate from all of that, and stronger: the pointer never outlives the
//! object. `entry::foreign` is cleared by the same sweep, so a wrap can never be
//! read against a cell that has become something else.
//!
//! # Why `napi_typeof` needs this module
//!
//! The ABI has `napi_external` and the language has no word for it — `typeof`
//! answers `"object"`, correctly, for the thing an external is made of. So the
//! externals are recorded here and [`is_external`] is what `values::napi_typeof`
//! asks. Recorded with a WATCH (`entry::weak`), not with a bare cell number: a
//! cell is reused after collection, and a stale entry would report somebody
//! else's object as an external.

use core::cell::RefCell;
use core::ffi::c_void;

use crate::abi::{napi_env, napi_finalize, napi_status, napi_value};
use crate::handles::{env_of, value_of, write_out};

use napi_status::{napi_invalid_arg, napi_object_expected, napi_ok};

/// What an addon attached, beyond the pointer the runtime keeps.
struct Wrapped {
    /// What to call when the wrap goes away, if the addon asked for one.
    finalize: napi_finalize,
    /// The second pointer that call receives.
    hint: *mut c_void,
    /// The watch that says whether the object is still alive, and which one it
    /// was. See the module doc: a bare cell number would be reused.
    watch: u32,
    /// The registration that runs [`on_collected`] when the object dies.
    ///
    /// Withdrawn when a wrap goes away by either of the other two routes, so
    /// the addon's finalizer runs exactly once however the wrap ends.
    death: Option<u32>,
    /// Whether `napi_create_external` made the object, rather than the program.
    external: bool,
    /// Which environment attached it.
    owner: *mut c_void,
}

/// What the collector calls, through `rts_core::entry::finalize`.
///
/// The runtime knows nothing about an environment, so identity travels in the
/// two words a registration carries — the same trick the callable trampoline
/// uses, and for the same reason. `data` is the slot in [`WRAPS`]; `hint` is
/// the pointer the addon wrapped, read before the object was freed because it
/// cannot be read afterwards.
extern "C" fn on_collected(slot: usize, data: usize) {
    let wrapped = WRAPS.with_borrow_mut(|wraps| match wraps.get_mut(slot) {
        Some(entry) => entry.take(),
        None => None,
    });
    let Some(wrapped) = wrapped else {
        return;
    };
    // Its own death is what brought us here, so the registration is spent —
    // cancelling it would be cancelling nothing.
    let owner = napi_env(wrapped.owner);
    finish(owner, wrapped, data as *mut c_void, false);
}

thread_local! {
    /// Every wrap on this thread, by slot.
    ///
    /// Holes rather than a compacting list, because the slot number is what a
    /// death registration carries — see [`on_collected`]. Removing by shifting
    /// would renumber registrations already lodged with the collector.
    static WRAPS: RefCell<Vec<Option<Wrapped>>> = const { RefCell::new(Vec::new()) };
}

/// Runs a wrap's finalizer, if it has one, and gives up what it was holding.
///
/// `withdraw` is false only when the collector is what brought us here: the
/// registration has already fired and cancelling it would cancel nothing.
fn finish(env: napi_env, wrapped: Wrapped, data: *mut c_void, withdraw: bool) {
    rts_core::entry::weak_forget(wrapped.watch);
    if withdraw && let Some(death) = wrapped.death {
        rts_core::entry::with_runtime(|context| {
            rts_core::entry::cancel_on_death(context, death)
        });
    }
    let Some(finalize) = wrapped.finalize else {
        return;
    };
    // SAFETY: the addon's own function, called with the environment it
    // registered under and the two pointers it gave us. The ABI's contract in
    // both directions.
    unsafe { finalize(env, data, wrapped.hint) };
}

/// Whether `napi_create_external` made what this value names.
///
/// Asked by [`crate::values::napi_typeof`], which is the only caller: the ABI
/// distinguishes an external from an object and the language does not.
pub fn is_external(value: u64) -> bool {
    WRAPS.with_borrow(|wraps| {
        wraps.iter().flatten().any(|wrapped| {
            wrapped.external
                && rts_core::entry::weak_peek(wrapped.watch).flatten() == Some(value)
        })
    })
}

/// Runs and forgets every wrap an environment made.
///
/// The second of a finalizer's three triggers; the others are
/// `napi_remove_wrap` and the collector.
pub fn forget(owner: napi_env) {
    let mine: Vec<Wrapped> = WRAPS.with_borrow_mut(|wraps| {
        let mut mine = Vec::new();
        for slot in wraps.iter_mut() {
            if slot.as_ref().is_some_and(|wrapped| wrapped.owner == owner.0)
                && let Some(wrapped) = slot.take()
            {
                mine.push(wrapped);
            }
        }
        mine
    });
    for wrapped in mine {
        // The object may already be gone, in which case the runtime dropped the
        // pointer with it and there is nothing left to hand the finalizer but
        // the hint.
        let data = rts_core::entry::weak_peek(wrapped.watch)
            .flatten()
            .and_then(rts_core::entry::foreign_attached)
            .unwrap_or(0);
        finish(owner, wrapped, data as *mut c_void, true);
    }
}

/// `napi_wrap` — the addon's pointer, behind `js_object`.
///
/// # Safety
///
/// The ABI's, and `native_object` must stay valid until its finalizer runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_wrap(
    env: napi_env,
    js_object: napi_value,
    native_object: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    _result: *mut crate::abi::napi_ref,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(js_object) }) else {
        return napi_invalid_arg;
    };
    wrap_word(env, word, native_object, finalize_cb, finalize_hint, false)
}

/// The half [`napi_wrap`] and [`napi_create_external`] share.
fn wrap_word(
    env: napi_env,
    word: u64,
    data: *mut c_void,
    finalize: napi_finalize,
    hint: *mut c_void,
    external: bool,
) -> napi_status {
    let Some(watch) = rts_core::entry::weak_watch(word) else {
        // A number has nowhere to keep a pointer, which is the ABI's
        // `napi_object_expected` rather than a generic failure.
        return napi_object_expected;
    };
    // Refused rather than replaced: the ABI says an object may be wrapped once,
    // and overwriting would strand a pointer the addon still owns with no
    // finalizer ever called for it.
    if rts_core::entry::foreign_attached(word).is_some() {
        rts_core::entry::weak_forget(watch);
        return napi_status::napi_invalid_arg;
    }
    rts_core::entry::foreign_attach(word, data as usize);
    let slot = WRAPS.with_borrow_mut(|wraps| {
        let wrapped = Wrapped {
            finalize,
            hint,
            watch,
            death: None,
            external,
            owner: env.0,
        };
        match wraps.iter().position(Option::is_none) {
            Some(free) => {
                wraps[free] = Some(wrapped);
                free
            }
            None => {
                wraps.push(Some(wrapped));
                wraps.len() - 1
            }
        }
    });
    // The third trigger: the collector. Registered with the slot and the
    // pointer, because the runtime carries two words and knows what neither of
    // them means — and because the pointer cannot be read after the object it
    // was attached to is gone.
    let death = rts_core::entry::with_runtime(|context| {
        rts_core::entry::on_death(
            context,
            word,
            rts_core::entry::OnDeathCall {
                code: on_collected,
                data: slot,
                hint: data as usize,
            },
        )
    });
    WRAPS.with_borrow_mut(|wraps| {
        if let Some(Some(wrapped)) = wraps.get_mut(slot) {
            wrapped.death = death;
        }
    });
    napi_ok
}

/// `napi_unwrap` — the pointer back.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_unwrap(
    _env: napi_env,
    js_object: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(js_object) }) else {
        return napi_invalid_arg;
    };
    let Some(data) = rts_core::entry::foreign_attached(word) else {
        return napi_status::napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = data as *mut c_void };
    napi_ok
}

/// `napi_remove_wrap` — the pointer back, and the wrap gone.
///
/// The finalizer runs here, because after this the addon owns the pointer again
/// and nothing else will ever call it.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_remove_wrap(
    env: napi_env,
    js_object: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(js_object) }) else {
        return napi_invalid_arg;
    };
    let Some(data) = rts_core::entry::foreign_detach(word) else {
        return napi_status::napi_invalid_arg;
    };
    let wrapped = WRAPS.with_borrow_mut(|wraps| {
        let at = wraps.iter().position(|slot| {
            slot.as_ref().is_some_and(|wrapped| {
                rts_core::entry::weak_peek(wrapped.watch).flatten() == Some(word)
            })
        })?;
        wraps[at].take()
    });
    if let Some(wrapped) = wrapped {
        finish(env, wrapped, data as *mut c_void, true);
    }
    if !result.is_null() {
        // SAFETY: the caller's contract.
        unsafe { *result = data as *mut c_void };
    }
    napi_ok
}

/// `napi_create_external` — an object that is nothing but a pointer.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_create_external(
    env: napi_env,
    data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    result: *mut napi_value,
) -> napi_status {
    let word = rts_core::entry::with_runtime(rts_core::entry::make_object);
    let status = wrap_word(env, word, data, finalize_cb, finalize_hint, true);
    if status != napi_ok {
        return status;
    }
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

/// `napi_get_value_external`.
///
/// # Safety
///
/// The ABI's.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_get_value_external(
    _env: napi_env,
    value: napi_value,
    result: *mut *mut c_void,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(value) }) else {
        return napi_invalid_arg;
    };
    // Refused for an ordinary wrapped object: `napi_get_value_external` is
    // asking what an EXTERNAL holds, and answering for a wrap would let an
    // addon read a pointer it never put there through the wrong door.
    if !is_external(word) {
        return napi_status::napi_invalid_arg;
    }
    let Some(data) = rts_core::entry::foreign_attached(word) else {
        return napi_status::napi_invalid_arg;
    };
    if result.is_null() {
        return napi_invalid_arg;
    }
    // SAFETY: the caller's contract.
    unsafe { *result = data as *mut c_void };
    napi_ok
}
