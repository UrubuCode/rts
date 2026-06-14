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
//! ## Objects (P3.6)
//!
//! A runtime object now carries a GLOBALLY-UNIQUE shape id at **slot 0** (a tagged
//! int indexing [`crate::shape`]'s process-global registry), with the property
//! values at slot `1 + slot_index`. [`__rtsadp_inspect_object`] reads slot 0,
//! recovers the ordered keys via [`crate::shape::global_shape_keys`], and renders
//! `{ k0: v0, k1: v1 }` (Node/`util.inspect` single-line format: bare identifier
//! keys, `, ` separators, spaces inside braces, `{}` for empty, single-quoted
//! string VALUES). Nested objects/arrays recurse.
//!
//! Object-vs-array at runtime: both are `TAG_OBJECT` `Entry::Vec`s and are
//! bit-identical, so the TOP-LEVEL call already picks the object vs array
//! trampoline by STATIC kind (the lowering knows `HeapShape::Object` vs `Array`).
//! For NESTED values [`looks_like_object`] uses the slot-0 header as a runtime
//! discriminator (a tagged int that is a VALID registry id AND `vec_len ==
//! keys+1`); anything else renders as an array. This covers the supported literal
//! corpus; a pathological array whose first element coincidentally equals a live
//! shape-id with a matching length is the residual ambiguity (not reachable from
//! the current literal-only object surface).
//!
//! ## Reuse, not divergence
//!
//! Every SCALAR element is rendered by the SAME [`super::genops`] `to_string` the
//! rest of the engine uses (number formatting via the real `STRING_FROM_F64`,
//! booleans/null/undefined bare), so the only thing this module adds is the
//! BRACKETS/BRACES, SEPARATORS, KEYS, and the NESTED-string quoting. The result is
//! interned in the REAL string pool and returned as a string handle — the same
//! pool `console.log` then reads through `STRING_PTR`/`STRING_LEN`.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use crate::shape::global_shape_keys;

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

/// `__rtsadp_inspect_object(value_word, top_level)` — render a STATICALLY-OBJECT
/// `value_word` (a `TAG_OBJECT` PolyValue over an `Entry::Vec` whose slot 0 is a
/// global shape-id) to a Node-style `{ k0: v0, … }` string, returning a string
/// PolyValue WORD (interned in the real pool). The lowering calls THIS (not
/// `__rtsadp_inspect`) for an arg whose static kind is an Object, so the top-level
/// object-vs-array decision is made at compile time. `top_level` is currently
/// always 1 for a direct `console.log` object arg; an object always renders in the
/// bracketed form, so it is accepted for symmetry but does not change the output.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_inspect_object(value_word: u64, _top_level: i64) -> u64 {
    let s = render_object(PolyValue::from_raw(value_word), 0);
    abi_adapter::intern_poly(&s).raw()
}

/// Render one [`PolyValue`] to its display text.
///
/// - `TAG_OBJECT`: an OBJECT (slot-0 header is a live shape-id, [`looks_like_object`])
///   → `{ … }`; otherwise an array Vec → `[ … ]`.
/// - string → bare at top level, single-quoted (with escaping) when nested.
/// - everything else (number/bool/null/undefined/function) → the SAME
///   [`genops::to_string`] the rest of the engine uses.
fn render(v: PolyValue, top_level: bool, depth: u32) -> String {
    if v.is_object() {
        return if looks_like_object(v) {
            render_object(v, depth)
        } else {
            render_array(v, depth)
        };
    }
    if v.is_string() {
        let content = abi_adapter::resolve_poly(v);
        return if top_level { content } else { quote(&content) };
    }
    // number / bool / null / undefined / function — reuse the engine's ToString.
    genops_to_string(v)
}

/// Whether a `TAG_OBJECT` Vec is an OBJECT (vs an array): its slot 0 is a tagged
/// int that is a VALID global shape-id AND its element count equals `keys + 1`
/// (the header + one slot per key). Arrays carry no header, so their slot 0 is an
/// arbitrary element and this check fails for all but a pathological coincidence
/// not reachable from the literal-only object surface.
pub(crate) fn looks_like_object(v: PolyValue) -> bool {
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle);
    if len < 1 {
        return false;
    }
    let slot0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 0) as u64);
    if !slot0.is_int32() {
        return false;
    }
    match global_shape_keys(slot0.as_i32() as u32) {
        Some(keys) => keys.len() as i64 + 1 == len,
        None => false,
    }
}

/// Render an OBJECT Vec (`{ k0: v0, k1: v1 }`, Node format): `{}` when it has no
/// keys, otherwise `{ ` + `key: value` pairs joined by `, ` + ` }`. Keys are bare
/// identifiers; each VALUE is rendered in NESTED context (string values quoted).
/// Recurses for nested objects/arrays. The keys come from the global registry via
/// the slot-0 shape-id; the values are at slot `1 + i`.
fn render_object(v: PolyValue, depth: u32) -> String {
    if depth >= MAX_DEPTH {
        return "[Object]".to_string();
    }
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let slot0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 0) as u64);
    let keys = slot0
        .is_int32()
        .then(|| global_shape_keys(slot0.as_i32() as u32))
        .flatten()
        .unwrap_or_default();
    if keys.is_empty() {
        return "{}".to_string();
    }
    let mut out = String::from("{ ");
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(key);
        out.push_str(": ");
        let word = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 1 + i as i64) as u64;
        out.push_str(&render(PolyValue::from_raw(word), false, depth + 1));
    }
    out.push_str(" }");
    out
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
