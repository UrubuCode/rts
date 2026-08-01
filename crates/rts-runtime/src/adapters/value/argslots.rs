//! ARGUMENT LAYOUT for the uniform-thunk ABI — the one place a call's logical
//! argument list is re-split into the slots the CALLEE actually reads.
//!
//! The uniform thunk takes `(env, a0..a3, rest)`: four positional PolyValue words
//! in registers plus an OVERFLOW array. Its body reads a positional param at
//! index `pos` from `a[pos]` when `pos < 4` and from `rest[pos - 4]` otherwise
//! (`front/run/thunk.rs`). Crucially, `pos` counts the callee's OWN slot layout —
//! and a THIS-FIRST callee spends slot 0 on the receiver, so the same call
//! `o.m(1,2,3,4)` needs `4` in `a3` for a plain callee but in `rest[0]` for a
//! this-first one.
//!
//! Every invoker used to hardcode ONE of those two layouts, which is issue #2039:
//! `__rtsadp_fn_invoke_method` reserved slot 0 for `this` and forced `a3 =
//! undefined`, so a PLAIN callee reached through it (`o.m.apply(o,[1,2,3,4])`,
//! `g().m(1,2,3,4)`) read its 4th param from an empty slot and its 5th from where
//! the 4th was — `[1,2,3,undefined,4]` instead of `[1,2,3,4,undefined]`. No error,
//! no crash, just a wrong-and-plausible number downstream, which is the most
//! expensive bug class in this repo.
//!
//! The fix is to stop hardcoding: an invoker builds the callee's FULL logical slot
//! list (bound args, then the receiver if the callee is this-first, then the call
//! arguments) and calls [`pack`], which cuts it at the register/overflow boundary
//! exactly where that particular callee expects it.
//!
//! ## Why this does not pre-pack a `...rest`
//!
//! The overflow array is the RAW tail, never a packed `...rest` value. The THUNK
//! is the single packer (it calls `__rtsadp_pack_rest` itself); packing here too
//! would double-wrap the tail — a bug this repo already shipped once, where
//! `(...args)` read `[[2,3]]`. See the same warning in
//! [`rts_primitives::function::ops`]'s uniform branch.
//!
//! Split into its own module rather than appended to [`super::funcops`], which is
//! already far over the 500-line ceiling.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;

/// Positional slots the uniform thunk ABI carries in registers (`a0..a3`).
/// Everything past these rides the overflow array.
pub const POSITIONAL: usize = 4;

/// The overflow arguments a `rest` word carries: the elements of the array when
/// the word IS an array, empty otherwise.
///
/// A non-array `rest` is not an error — for a call that fits in the registers the
/// slot carries the ARG COUNT as an `INT32` word instead (or a plain `0` from a
/// call site that never had an overflow to describe). Neither shape names any
/// argument, so both read as "no overflow".
pub fn overflow(rest_word: u64) -> Vec<u64> {
    let rv = PolyValue::from_raw(rest_word);
    if !rv.is_object() {
        return Vec::new();
    }
    let h = rt_handles::__rtsn_poly_to_handle(rv.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
    (0..len)
        .map(|i| rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64)
        .collect()
}

/// Wrap `items` (the RAW argument tail — see the module doc on double-wrapping)
/// as a fresh overflow array word.
pub fn overflow_word(items: &[u64]) -> u64 {
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    for &w in items {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w as i64);
    }
    PolyValue::from_object_handle(rt_handles::__rtsn_poly_from_handle(out)).raw()
}

/// Cut a callee's FULL logical slot list at the uniform ABI's register/overflow
/// boundary: the first four entries become `a0..a3` (missing ones `undefined`),
/// everything past them becomes a fresh overflow array word.
///
/// `empty_rest` is the word to hand back when NOTHING overflows — pass the
/// invoker's own incoming `rest` word when it carried no arguments (forwarding it
/// verbatim keeps the argc-word convention intact for callees that inspect it),
/// or `undefined` when its contents were absorbed into `full`.
pub fn pack(full: &[u64], empty_rest: u64) -> ([u64; 4], u64) {
    let mut a = [PolyValue::undefined().raw(); POSITIONAL];
    for (slot, &w) in a.iter_mut().zip(full.iter()) {
        *slot = w;
    }
    let rest = if full.len() > POSITIONAL {
        overflow_word(&full[POSITIONAL..])
    } else {
        empty_rest
    };
    (a, rest)
}

/// Build a callee's uniform-ABI `(a0..a3, rest)` from the three pieces every
/// invoker has: its LEAD slots (the bound args and/or the receiver that occupy
/// the callee's first slots), the call's own register args, and the incoming
/// overflow word.
///
/// This is [`pack`] plus the bookkeeping that must not be re-derived per call
/// site: an overflow word whose contents were absorbed into the list must NOT be
/// forwarded as well (its arguments would be delivered twice, once shifted), and
/// an overflow word that carried nothing IS forwarded verbatim so the argc-word
/// convention survives.
pub fn relayout(lead: &[u64], regs: &[u64], rest_word: u64) -> ([u64; 4], u64) {
    let ov = overflow(rest_word);
    let mut full: Vec<u64> = Vec::with_capacity(lead.len() + regs.len() + ov.len());
    full.extend_from_slice(lead);
    full.extend_from_slice(regs);
    full.extend(&ov);
    let empty_rest = if ov.is_empty() {
        rest_word
    } else {
        PolyValue::undefined().raw()
    };
    pack(&full, empty_rest)
}

/// Whether the 4th register slot of an invoker that carries `a0..a3` holds a REAL
/// argument (as opposed to `undefined` padding for an absent one).
///
/// Exact when the `rest` word is the `INT32` arg count the compile-time call path
/// emits for a call that fits in the registers; otherwise it falls back to reading
/// the slot, which cannot tell an absent argument from an explicitly-passed
/// `undefined` — the documented ambiguity of an ABI that carries no argc.
pub fn slot3_is_arg(a3: u64, rest_word: u64) -> bool {
    let rv = PolyValue::from_raw(rest_word);
    if rv.is_int32() {
        return rv.as_i32() > 3;
    }
    if rv.is_object() {
        // There IS an overflow, so every register slot before it is filled.
        return true;
    }
    a3 != PolyValue::undefined().raw()
}
