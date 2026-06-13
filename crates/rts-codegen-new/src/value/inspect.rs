//! `console.log` PRETTY-PRINT of whole ARRAYS — Bun/Node `util.inspect` format
//! (P3.5).
//!
//! `console.log` of a scalar already goes through [`super::genops`] `to_string` +
//! the real IO_PRINT (bare text). But `console.log([1, 2, 3])` must print
//! `[ 1, 2, 3 ]` (spaces inside the brackets, `, ` separators, nested strings
//! single-quoted) — NOT the runtime's `[object Object]` / `.join(",")`. P3
//! deferred this and BAILED whole-collection logs; this module implements the
//! ARRAY case.
//!
//! ## What this implements (and what it bails)
//!
//! - **Arrays** (`TAG_OBJECT` over a real `Entry::Vec`): `[ e0, e1, … ]`, each
//!   element inspected in NESTED context (top_level=0, so a string element is
//!   single-quoted). Empty array → `[]`. Nested arrays recurse with the same
//!   rules. This is always doable — an array needs no key recovery.
//! - **Objects**: BAILED at lowering (see `front/run/call.rs`). A runtime object is
//!   a slot `Entry::Vec` with NO runtime shape-id, and the per-function
//!   [`crate::shape::ShapeTable`] mints function-local ids — there is no
//!   process-global shape-id→keys registry, and a runtime object word is
//!   bit-identical to an array word (both `TAG_OBJECT` Vecs). Recovering the KEYS
//!   from a bare word is therefore impossible without a runtime shape-id, which is
//!   too invasive for this increment. So `__rtsadp_inspect` renders every
//!   `TAG_OBJECT` as an ARRAY, and object logging stays a lowering-time bail. The
//!   honesty floor: a near-miss (an object printed as `[ … ]`) would diverge, so
//!   the lowering refuses object logs rather than reach this code with one.
//!
//! ## Reuse, not divergence
//!
//! Every SCALAR element is rendered by the SAME [`super::genops`] `to_string` the
//! rest of the engine uses (number formatting via the real `STRING_FROM_F64`,
//! booleans/null/undefined bare), so the only thing this module adds is the
//! BRACKETS, SEPARATORS, and the NESTED-string quoting. The result is interned in
//! the REAL string pool and returned as a string handle — the same pool
//! `console.log` then reads through `STRING_PTR`/`STRING_LEN`.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::abi_adapter;
use super::{genops, PolyValue};

/// Recursion-depth guard: a pathological deeply-nested / cyclic structure renders
/// `[Array]` past this depth rather than recurse forever (Node uses a default
/// inspect depth of 2; we go deeper but still bound it for safety).
const MAX_DEPTH: u32 = 8;

/// `__rtsadp_inspect(value_word, top_level)` — render `value_word` (a raw
/// [`PolyValue`] word) to a Bun/Node-style display string, returning a string
/// `PolyValue` WORD (interned in the real pool). The lowering joins/prints it via
/// the SAME `__rtsadp_to_string`/`__rtsadp_add`/print path as a scalar arg —
/// tagged-in / tagged-out, like every other `__rtsadp_*` operator.
///
/// `top_level` is `1` for a direct `console.log` argument (a string prints BARE),
/// `0` for a NESTED value (a string is single-quoted). Arrays/objects always use
/// the inspect (bracketed) form regardless of `top_level`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_inspect(value_word: u64, top_level: i64) -> u64 {
    let s = render(PolyValue::from_raw(value_word), top_level != 0, 0);
    abi_adapter::intern_poly(&s).raw()
}

/// Render one [`PolyValue`] to its display text.
///
/// - `TAG_OBJECT` (an array/object Vec) → `[ … ]` (see the module note on why an
///   object reaching here is impossible for supported inputs).
/// - string → bare at top level, single-quoted (with escaping) when nested.
/// - everything else (number/bool/null/undefined/function) → the SAME
///   [`genops::to_string`] the rest of the engine uses.
fn render(v: PolyValue, top_level: bool, depth: u32) -> String {
    if v.is_object() {
        return render_array(v, depth);
    }
    if v.is_string() {
        let content = abi_adapter::resolve_poly(v);
        return if top_level { content } else { quote(&content) };
    }
    // number / bool / null / undefined / function — reuse the engine's ToString.
    genops_to_string(v)
}

/// Render a `TAG_OBJECT` array word as `[ e0, e1, … ]` (Bun/Node spacing): `[]`
/// when empty, otherwise `[ ` + elements joined by `, ` + ` ]`. Each element is
/// rendered in NESTED context (strings quoted). Recurses for nested arrays.
fn render_array(v: PolyValue, depth: u32) -> String {
    if depth >= MAX_DEPTH {
        return "[Array]".to_string();
    }
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
    if len == 0 {
        return "[]".to_string();
    }
    let mut out = String::from("[ ");
    for i in 0..len {
        if i > 0 {
            out.push_str(", ");
        }
        let word = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
        out.push_str(&render(PolyValue::from_raw(word), false, depth + 1));
    }
    out.push_str(" ]");
    out
}

/// `genops::to_string` is `pub(super)` within `value`; call it through the public
/// `__rtsadp_to_string` trampoline (which returns a string PolyValue handle) and
/// read the text back. This guarantees IDENTICAL formatting to every other
/// ToString in the engine — no divergent reimplementation.
fn genops_to_string(v: PolyValue) -> String {
    let s_word = genops::__rtsadp_to_string(v.raw());
    abi_adapter::resolve_poly(PolyValue::from_raw(s_word))
}

/// Single-quote a string for NESTED display, escaping backslash and single-quote
/// (Bun/Node use single quotes inside collections: `[ 'a', 'b' ]`). Other control
/// characters are left verbatim — the common fixture strings are plain text; full
/// control-char escaping is a later increment if a fixture needs it.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            _ => out.push(c),
        }
    }
    out.push('\'');
    out
}
