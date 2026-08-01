//! The one predicate the GENERATOR state machine needs over the lazy for-of
//! cursor.
//!
//! The ordinary (non-generator) for-of lowering tests exhaustion in Cranelift
//! IR: it compares [`super::iterops::__rtsadp_iter_next`]'s result word against
//! the EMPTY sentinel with an `icmp`. The generator state machine cannot do
//! that — its loop condition is a JS EXPRESSION written by the parser's
//! desugar (`generator_sm_iter`), and no JS expression can name EMPTY.
//!
//! So the comparison gets a trampoline. Keeping it a separate call (rather than
//! folding "done" into `iter_next`'s result) preserves the property the whole
//! protocol rests on: `iter_next` returns the VALUE, and a user value can never
//! be EMPTY, so nothing is reserved out of the value space.

use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;

/// [`super::iterops::__rtsadp_iter_open`] cursor kind for a source that IS a
/// lazy generator (`Entry::GenState`).
pub(super) const CURSOR_GENSTATE: i32 = 2;

/// The `Entry::GenState` handle behind `word`, when the for-of SOURCE is itself
/// a lazy generator (`for (const x of g())` where `g` is a `function*`).
///
/// The ordinary loop lowering recognizes this STATICALLY (`try_lazy_gen_source_
/// word`, keyed on the callee's `ret_lazy_gen` flag) and DRAINS the generator to
/// an array. Inside a generator state machine there is no such static callee —
/// the source arrives as an opaque word — so it has to be recognized by TAG at
/// run time. Draining would also be the wrong answer here: a generator
/// iterating another generator is exactly where an INFINITE source is normal,
/// and draining one hangs. Hence a cursor kind of its own, stepped lazily.
pub(super) fn genstate_handle(word: u64) -> Option<u64> {
    let v = PolyValue::from_raw(word);
    if !(v.is_object() || v.is_function()) {
        return None;
    }
    let h = rt_handles::__rtsn_poly_to_handle(v.as_handle());
    rt_handles::with_entry(h, |e| {
        matches!(e, Some(rt_handles::Entry::GenState(_)))
    })
    .then_some(h)
}

/// One step of a [`CURSOR_GENSTATE`] cursor: the generator's next VALUE word, or
/// the EMPTY sentinel when it finishes.
pub(super) fn genstate_next(h: u64) -> u64 {
    let res = rts_runtime::namespaces::collector::generator::__rtsn_gen_sm_next(h);
    // `gen_sm_next` hands back the RAW handle of the shaped `{value, done}`
    // object, not a boxed PolyValue word — so it is read with the reader that
    // owns that layout, never with `obj_get` over a word (which would see a
    // non-object, read `done` as `undefined` → falsy, and loop FOREVER).
    // A handle that is not that shape means the step did not happen: report
    // exhaustion, because a short walk is a visible wrong answer and an
    // infinite one is a hang.
    let Some((value, done)) =
        rts_runtime::namespaces::collector::generator::read_result_parts(res)
    else {
        return PolyValue::empty().raw();
    };
    if done {
        return PolyValue::empty().raw();
    }
    value as u64
}

/// Early exit over a [`CURSOR_GENSTATE`] cursor — `gen.return(undefined)`, the
/// spec IteratorClose for a generator (it runs any pending `finally`).
pub(super) fn genstate_close(h: u64) {
    let undef = PolyValue::undefined().raw();
    let _ = rts_runtime::namespaces::collector::generator::__rtsn_gen_sm_return(h, undef as i64);
}

/// `__rtsadp_iter_done(word)` — 1 when `word` is the EMPTY sentinel
/// [`super::iterops::__rtsadp_iter_next`] returns on exhaustion, else 0.
///
/// A plain integer (not a boolean PolyValue) because the state machine drops it
/// straight into an `if` the codegen lowers as a truthy test.
#[rtse::abi]
pub fn rtsadp_iter_done(word: u64) -> u64 {
    if PolyValue::from_raw(word).is_empty() {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_done_and_ordinary_values_are_not() {
        assert_eq!(rtsadp_iter_done(PolyValue::empty().raw()), 1);
        assert_eq!(rtsadp_iter_done(PolyValue::undefined().raw()), 0);
        assert_eq!(rtsadp_iter_done(PolyValue::from_i32(0).raw()), 0);
        assert_eq!(rtsadp_iter_done(PolyValue::from_f64(0.0).raw()), 0);
    }
}
