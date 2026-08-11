//! `napi_add_finalizer` — a callback for an object's death, without owning it.
//!
//! # Why this is not `napi_wrap` with a null pointer
//!
//! `napi_wrap` claims the object: one pointer, one finalizer, and a second
//! `napi_wrap` on the same object is refused. That exclusivity is the whole
//! point of it — the addon that wrapped an object owns what is behind it.
//!
//! `napi_add_finalizer` deliberately has none of that. Several may be added to
//! one object, and one may be added to an object that is ALSO wrapped, which is
//! the ordinary case: a class instance wrapped by the addon, with an extra
//! finalizer for a buffer it also allocated. So this keeps its own table rather
//! than the single foreign slot `crate::wrap` spends.
//!
//! # What is not implemented, and why the answer is still `napi_ok`
//!
//! The ABI's `result` out-parameter is a `napi_ref` for the object, which an
//! addon may keep and delete to run the finalizer early. This writes nothing
//! there and says so: a null it never reads is harmless, and a handle that
//! could not delete anything would be worse than absent.

use core::cell::RefCell;
use core::ffi::c_void;

use crate::abi::{napi_env, napi_finalize, napi_ref, napi_status, napi_value};
use crate::handles::value_of;

use napi_status::{napi_invalid_arg, napi_object_expected, napi_ok};

/// One registered finalizer.
struct Added {
    /// The addon's callback.
    finalize: napi_finalize,
    /// The first word it is handed.
    data: *mut c_void,
    /// The second.
    hint: *mut c_void,
    /// The weak watch that says whether the object is still there.
    watch: u32,
    /// Which environment registered it.
    owner: *mut c_void,
}

thread_local! {
    /// Every added finalizer on this thread, by slot.
    ///
    /// Holes rather than compaction, for the reason `crate::wrap` records: the
    /// slot number is what the death registration carries, and shifting would
    /// renumber registrations the collector already holds.
    static ADDED: RefCell<Vec<Option<Added>>> = const { RefCell::new(Vec::new()) };
}

/// What the collector calls.
///
/// `data` is the slot; the addon's own two words are read out of the record,
/// which is what makes this different from `crate::wrap`'s — there is no
/// pointer that has to be read before the object goes, because this finalizer
/// never owned one.
extern "C" fn on_collected(slot: usize, _hint: usize) {
    let added = ADDED.with_borrow_mut(|added| match added.get_mut(slot) {
        Some(entry) => entry.take(),
        None => None,
    });
    if let Some(added) = added {
        run(added);
    }
}

/// Calls one finalizer and gives up its watch.
fn run(added: Added) {
    rts_core::entry::weak_forget(added.watch);
    let Some(finalize) = added.finalize else {
        return;
    };
    // SAFETY: the addon's own function, called with the environment it
    // registered under and the two words it supplied.
    unsafe { finalize(napi_env(added.owner), added.data, added.hint) };
}

/// Runs and forgets every finalizer an environment added.
///
/// Called from [`crate::env::destroy`], beside `crate::wrap::forget`, so an
/// addon torn down without a collection still sees its callbacks.
pub fn forget(owner: napi_env) {
    let mine: Vec<Added> = ADDED.with_borrow_mut(|added| {
        let mut mine = Vec::new();
        for slot in added.iter_mut() {
            if slot.as_ref().is_some_and(|one| one.owner == owner.0)
                && let Some(one) = slot.take()
            {
                mine.push(one);
            }
        }
        mine
    });
    for one in mine {
        run(one);
    }
}

/// `napi_add_finalizer`.
///
/// # Safety
///
/// The ABI's, and `finalize_data` must stay valid until the finalizer runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn napi_add_finalizer(
    env: napi_env,
    js_object: napi_value,
    finalize_data: *mut c_void,
    finalize_cb: napi_finalize,
    finalize_hint: *mut c_void,
    _result: *mut napi_ref,
) -> napi_status {
    // SAFETY: the caller's contract.
    let Some(word) = (unsafe { value_of(js_object) }) else {
        return napi_invalid_arg;
    };
    let Some(watch) = rts_core::entry::weak_watch(word) else {
        return napi_object_expected;
    };
    let slot = ADDED.with_borrow_mut(|added| {
        let one = Added {
            finalize: finalize_cb,
            data: finalize_data,
            hint: finalize_hint,
            watch,
            owner: env.0,
        };
        match added.iter().position(Option::is_none) {
            Some(free) => {
                added[free] = Some(one);
                free
            }
            None => {
                added.push(Some(one));
                added.len() - 1
            }
        }
    });
    rts_core::entry::with_runtime(|context| {
        rts_core::entry::on_death(
            context,
            word,
            rts_core::entry::OnDeathCall {
                code: on_collected,
                data: slot,
                hint: 0,
            },
        )
    });
    napi_ok
}
