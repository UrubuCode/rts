//! `RegExp` reached WITHOUT `new` — as a bare call and as a first-class VALUE.
//!
//! `new RegExp(p, f)` and the `/re/` literal both lower straight to
//! [`super::regexops::__rtsadp_re_compile`]. This module covers the other two
//! forms the engine had no model for:
//!
//! - `RegExp(pattern[, flags])` — a call with no `new`. Per the spec `RegExp` is
//!   one of the constructors that behave the same called either way, with ONE
//!   documented difference: when the argument ALREADY is a RegExp and `flags` is
//!   `undefined`, the call form returns THAT SAME object (no copy), while `new`
//!   copies. [`__rtsadp_re_call`] implements exactly that; everything else
//!   delegates to `__rtsadp_re_compile`, so the two spellings can never diverge
//!   in their compilation semantics.
//! - `const f = RegExp; new f("ab+c")` / `xs.map(RegExp)` — `RegExp` read as a
//!   VALUE. [`__rtsadp_regexp_fn_value`] hands back a real callable over the same
//!   [`__rtsadp_re_call`].
//!
//! Copying a RegExp must go through the original's `source`/`flags`, never
//! `ToString(re)` (which yields `/ab+c/` WITH the slashes, so the copy stops
//! matching what the original matched) — `__rtsadp_re_compile` already owns that
//! rule; nothing here re-implements it.
//!
//! `RegExp` is PRIMORDIAL (it has native `/re/` syntax), so naming it is allowed
//! by the doctrine. Split into its own module rather than appended to
//! [`super::regexops`], which is already over the 500-line ceiling.

use rts_runtime::namespaces::gc::handles::{Entry, FunctionData, alloc_entry};

use super::PolyValue;
use super::abi_adapter::intern_poly;
use super::regexops::__rtsadp_re_compile;

/// Whether `word` is a live RegExp INSTANCE (a `TAG_OBJECT` word over an
/// `Entry::Regex`). Any other object — a plain object, an array, a Date — is not,
/// so the identity rule below can never hand back a non-RegExp unchanged.
fn is_regexp(word: u64) -> bool {
    use rts_engine::heap::handles::with_entry;
    let pv = PolyValue::from_raw(word);
    if !pv.is_object() {
        return false;
    }
    let real = rts_runtime::namespaces::gc::handles::__rtsn_poly_to_handle(pv.as_handle());
    with_entry(real, |e| matches!(e, Some(Entry::Regex(_))))
}

/// `RegExp(pattern[, flags])` — the call form (no `new`).
///
/// An omitted `flags` arrives as the `undefined` word. Two cases:
///
/// - `pattern` is already a RegExp AND `flags` is `undefined` → the SAME object
///   (spec identity: `RegExp(re) === re`).
/// - anything else → the ordinary compile path, with an `undefined` pattern or
///   `undefined` flags normalized to the empty string. Both normalizations are
///   spec behaviour (`RegExp()` is `/(?:)/`, an omitted `flags` is `""`) AND a
///   safety requirement: an `undefined` word reaching `__rtsadp_re_compile`'s
///   string decode would read a bogus handle, and letting it fall into
///   `ToString` would compile the literal pattern `"undefined"`.
#[rtse::abi]
pub fn rtsadp_re_call(pat_word: u64, flags_word: u64) -> u64 {
    let no_flags = PolyValue::from_raw(flags_word).is_undefined();
    if no_flags && is_regexp(pat_word) {
        return pat_word;
    }
    let empty = intern_poly("").raw();
    let pat_word = if PolyValue::from_raw(pat_word).is_undefined() {
        empty
    } else {
        pat_word
    };
    let flags_word = if no_flags { empty } else { flags_word };
    __rtsadp_re_compile(pat_word, flags_word)
}

/// Collect the actual argument words of a uniform-ABI call: `a0..a3` truncated to
/// the argc the `rest` slot carries, or `a0..a3` plus the overflow array's
/// elements when `rest` is an ARRAY (>4 args). `RegExp` takes at most 2, so only
/// the leading slots are ever read — but the argc IS load-bearing: it is what
/// distinguishes `RegExp(re)` (identity) from `RegExp(re, undefined)`… which the
/// spec treats identically anyway, so a missing slot simply reads `undefined`.
fn arg_at(i: usize, slots: [u64; 4], rest: u64) -> u64 {
    let rv = PolyValue::from_raw(rest);
    let argc = if rv.is_int32() {
        rv.as_i32().clamp(0, 4) as usize
    } else {
        4
    };
    if i < argc {
        slots[i]
    } else {
        PolyValue::undefined().raw()
    }
}

/// Uniform-ABI thunk for the `RegExp` FUNCTION VALUE.
///
/// KNOWN, NARROW DIVERGENCE: `new f(re)` through a first-class `RegExp` value
/// runs this same thunk (the uniform invoker cannot tell a `new` call from a
/// plain one), so it yields the identity rather than a copy. `new RegExp(re)` by
/// NAME — the form real code writes — is lowered by the engine straight to the
/// ctor path and copies correctly.
extern "C" fn regexp_thunk(_env: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64 {
    let slots = [a0, a1, a2, a3];
    __rtsadp_re_call(arg_at(0, slots, rest), arg_at(1, slots, rest))
}

/// `RegExp` read as a first-class FUNCTION VALUE (`const f = RegExp`,
/// `xs.map(RegExp)`): a real callable over [`__rtsadp_re_call`]. Its thunk
/// address is registered as a constructor so `new f(..)` takes the direct
/// construct path instead of the not-a-constructor throw.
#[rtse::abi]
pub fn rtsadp_regexp_fn_value() -> u64 {
    let addr = regexp_thunk as *const () as usize as u64;
    super::ctorval::__rtsadp_register_ctor_thunk(addr);
    let data = FunctionData {
        fn_ptr: addr,
        // `RegExp.length` is 2 per the spec.
        arity: 2,
        name: Box::<str>::from("RegExp"),
        bound_this: 0,
        has_bound_this: false,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: true,
    };
    let h = alloc_entry(Entry::Function(Box::new(data)));
    PolyValue::from_function_handle(h & super::PAYLOAD_MASK).raw()
}
