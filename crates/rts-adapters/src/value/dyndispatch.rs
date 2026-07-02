//! Codegen-owned DYNAMIC (runtime) method-dispatch trampolines — P5.9.
//!
//! Static method dispatch ([`crate::dispatch`] + [`crate::front::run::method`])
//! resolves `recv.method(args)` only when the receiver's CLASS is proven at
//! compile time (a string/number literal, a proven-array local, a `new C()`
//! instance, …). When the method NAME is known but the receiver is a TAGGED value
//! of unproven kind (a param, a call return, a re-`let` local), the lowering used
//! to BAIL ("method call on non-dispatchable receiver"). This module supplies the
//! runtime fallback: one codegen-owned `__rtsadp_dyn_<method>` trampoline per
//! HIGH-FREQUENCY method that inspects the receiver's [`PolyValue`] tag AT RUNTIME
//! and delegates to the SAME per-class implementation the static path would call.
//!
//! ## Why option (b) — a per-method tag-branching trampoline (not inline IR)
//!
//! Exactly like the generic operators in [`super::genops`] and the Array
//! trampolines in [`super::arrayops`], these are codegen-owned `__rtsadp_*`
//! functions (NOT `__RTS_FN_*`). Each takes the receiver as a raw PolyValue WORD
//! plus PolyValue-word args, branches on the tag in Rust, and calls the REAL
//! per-class op:
//! - a STRING receiver (`TAG_STR`) → the real `__RTS_FN_GL_STRING_*` symbol;
//! - an ARRAY receiver (`TAG_OBJECT` that is NOT a keyed object) → the codegen
//!   Array trampoline ([`super::arrayops`]);
//! - a NUMBER receiver (int32/double) → the number formatting (`toString`);
//! - everything else → the JS-correct result where DEFINED (e.g. `(5).toString()`),
//!   else the `undefined` sentinel (sound: a TypeError class in JS, and the engine
//!   has no throw — but never a WRONG value).
//!
//! Keeping the branch in ONE Rust function (rather than emitting inline Cranelift
//! tag-branches at every call site) keeps the lowering ([`crate::front::run::method_dyn`])
//! a single `call`, reuses every existing per-class op verbatim, and matches the
//! established `__rtsadp_*` convention the JIT already installs by symbol.
//!
//! ## Soundness boundary
//!
//! - A method is dynamically dispatched ONLY when at least one class implements it
//!   AND each implemented class branch produces the JS-correct value. An
//!   unexpected receiver tag (one with no impl for that method — where JS throws a
//!   TypeError) returns `undefined`: acceptable for property-like reads, and never
//!   a wrong VALUE (the honesty floor). Methods whose no-impl behavior would be a
//!   silent wrong value are NOT added here — the lowering keeps bailing.
//! - The element/equality/ToString semantics are the SAME the static path uses
//!   (string ops via the real pool, array ops via [`super::arrayops`], number
//!   ToString via [`super::genops`]) — no divergent reimplementation.
//!
//! ## Convention
//!
//! Uniform `U64` PolyValue words in and out (`__rtsadp_dyn_length` returns a
//! number word; predicate/index results are boxed into PolyValue words too), so
//! the lowering boxes every arg and unboxes the single result word — no per-method
//! marshaling table.

use rts_runtime::namespaces::globals::string::rt as rt_gl_str;

use super::{PolyValue, abi_adapter, arrayops, genops};

/// The real runtime string handle behind a string PolyValue word (generation
/// reconstructed from the live slot). Caller guarantees the word is a string.
fn str_handle(recv: u64) -> u64 {
    abi_adapter::real_handle_of(PolyValue::from_raw(recv))
}

/// The real Vec handle behind an array PolyValue word. Caller guarantees the word
/// is an array (TAG_OBJECT, not a keyed object).
fn arr_handle(recv: u64) -> u64 {
    use rts_runtime::namespaces::gc::handles as rt_handles;
    rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(recv).as_handle())
}

/// Box a real string handle (returned by a `__RTS_FN_GL_STRING_*` op) as a string
/// PolyValue word, reusing the SAME pool bridge the static path uses.
fn box_str(handle: u64) -> u64 {
    abi_adapter::poly_from_real_handle(handle).raw()
}

/// Whether a PolyValue word is an ARRAY (a `TAG_OBJECT` that is not a keyed
/// object). Reuses the SAME discriminator the inspect/genops paths use, so the
/// array-vs-object decision is identical everywhere.
fn is_array_word(recv: u64) -> bool {
    let v = PolyValue::from_raw(recv);
    v.is_object() && !super::inspect::looks_like_object(v)
}

/// The raw `undefined` PolyValue word — the sound result for a receiver tag with
/// no implementation of the called method (JS would throw a TypeError; the engine
/// has no throw, so `undefined` is returned rather than a wrong value).
fn undef() -> u64 {
    PolyValue::undefined().raw()
}

// ===========================================================================
// `.toString()` — defined on EVERY value (number/string/bool/null/undefined/
// array/object/function), so it never falls to the undefined sentinel. Reuses
// the engine's ONE ToString ([`genops`]). `(5).toString()` → "5", `"hi"` → "hi".
// ===========================================================================

/// `recv.toString()` — JS `ToString`, returning a string PolyValue word. Reuses
/// the engine's single ToString path (the same one `console.log`/`+` use), so the
/// number formatting / array join / `[object Object]` are byte-identical.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_to_string(recv: u64) -> u64 {
    genops::__rtsadp_to_string(recv)
}

// ===========================================================================
// `.length` — a PROPERTY (not a call) defined on strings (UTF-16 code-unit
// length) and arrays (element count). On any other receiver JS reads `undefined`.
// Dispatched dynamically so `function len(x){ return x.length; }` works for both.
// ===========================================================================

/// `recv.length` — the string's length (real `STRING_LEN`, a number word) or the
/// array's element count (`VEC_LEN`). Any other receiver → `undefined` (JS reads a
/// missing property as `undefined`; never a wrong value).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_length(recv: u64) -> u64 {
    use rts_runtime::namespaces::collections::vec as rt_vec;
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        // JS `String.prototype.length` is the UTF-16 CODE-UNIT count, NOT the byte
        // length (`STRING_LEN` is bytes — they differ for any non-ASCII char). We
        // OWN this trampoline, so we read the real bytes and count code units
        // (`encode_utf16`), matching bun/Node exactly (`"😀".length === 2`).
        let s = abi_adapter::resolve_poly(v);
        let units = s.chars().map(|c| c.len_utf16()).sum::<usize>();
        return PolyValue::from_i32(units as i32).raw();
    }
    // A BUFFER (a `TextEncoder.encode` result — `Entry::Buffer` bytes): its
    // `.length` is the byte count (the Uint8Array surface). Checked BEFORE the
    // array arm — a Buffer word also passes `is_array_word` (object, non-keyed)
    // and `VEC_LEN` on it reads 0.
    if v.is_object() {
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        if let Some(len) = rts_engine::heap::handles::with_entry(h, |e| match e {
            Some(rts_engine::heap::handles::Entry::Buffer(b)) => Some(b.len()),
            _ => None,
        }) {
            return PolyValue::from_i32(len as i32).raw();
        }
    }
    if is_array_word(recv) {
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(arr_handle(recv)).max(0);
        return PolyValue::from_i32(len as i32).raw();
    }
    // A KEYED OBJECT may carry a real own `length` property (`{length: 3}`) —
    // read it like any other key (undefined when absent, matching JS).
    if v.is_object() {
        let key = abi_adapter::intern_poly("length").raw();
        return super::objops::__rtsadp_obj_get(recv, key);
    }
    undef()
}

/// The UTF-16 code units of a string PolyValue's content (so string indexing
/// methods match JS, which addresses UTF-16 code units, not bytes or scalars).
fn utf16_units(recv: u64) -> Vec<u16> {
    abi_adapter::resolve_poly(PolyValue::from_raw(recv))
        .encode_utf16()
        .collect()
}

/// Intern a single UTF-16 code unit as a 1-unit string, returning the REAL string
/// handle (for `charAt`/`at`). A lone surrogate is rendered lossily (`U+FFFD`) —
/// valid UTF-8 the pool can hold; the common BMP corpus round-trips exactly.
fn intern_utf16_unit(unit: u16) -> u64 {
    let s = String::from_utf16_lossy(&[unit]);
    abi_adapter::real_handle_of(abi_adapter::intern_poly(&s))
}

/// The interned empty-string REAL handle (for an out-of-range `charAt`).
fn empty_string_handle() -> u64 {
    abi_adapter::real_handle_of(abi_adapter::intern_poly(""))
}

// ===========================================================================
// `.indexOf(x)` — defined on BOTH strings (substring search, string arg) and
// arrays (strict-equal element search, any arg). The dynamic trampoline routes by
// tag; an unexpected receiver → `-1` (JS-consistent "not found" for the corpus,
// never a wrong index — the no-impl primitive case is rare and the lowering may
// keep it bailing where it matters).
// ===========================================================================

/// `recv.indexOf(needle)` — string substring index OR array strict-equal element
/// index. Returns a number word (`-1` when not found / unexpected receiver).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_index_of(recv: u64, needle: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    let idx = if v.is_string() {
        // String.indexOf wants a string needle: ToString it (JS coerces the arg),
        // intern in the real pool, call the real op.
        let needle_h = str_arg_handle(needle);
        rt_gl_str::__RTS_FN_GL_STRING_INDEX_OF(str_handle(recv), needle_h)
    } else if is_array_word(recv) {
        arrayops::__rtsadp_arr_index_of(arr_handle(recv), needle)
    } else {
        -1
    };
    PolyValue::from_i32(idx as i32).raw()
}

/// `recv.includes(x)` — string substring test OR array strict-equal membership.
/// Returns a boolean word; an unexpected receiver → `false`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_includes(recv: u64, needle: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    let yes = if v.is_string() {
        let needle_h = str_arg_handle(needle);
        rt_gl_str::__RTS_FN_GL_STRING_INCLUDES(str_handle(recv), needle_h) != 0
    } else if is_array_word(recv) {
        arrayops::__rtsadp_arr_includes(arr_handle(recv), needle) != 0
    } else {
        false
    };
    PolyValue::bool(yes).raw()
}

/// `recv.at(i)` — string code-unit-at (as a 1-char string) OR array element-at.
/// JS `String.prototype.at` returns a single-char string (or `undefined` out of
/// range); array `at` returns the element word. Unexpected receiver → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_at(recv: u64, idx: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    let i = genops_to_i64(idx);
    if v.is_string() {
        // JS `String.prototype.at` indexes UTF-16 code units (negative from the
        // end), returning the single code-unit string or `undefined`. We compute it
        // over the real bytes (UTF-16) so surrogate-pair strings match bun.
        let units = utf16_units(recv);
        let len = units.len() as i64;
        let pos = if i < 0 { len + i } else { i };
        if pos < 0 || pos >= len {
            return undef();
        }
        return box_str(intern_utf16_unit(units[pos as usize]));
    }
    if is_array_word(recv) {
        return arrayops::__rtsadp_arr_at(arr_handle(recv), i);
    }
    undef()
}

/// Generic computed-INDEX read `recv[idx]` on an UNPROVEN receiver — the bracket
/// operator (NOT `.at()`): a string yields the 1-code-unit string at a NON-negative
/// index (`s[-1]` is `undefined`, no wrap); an array yields the element at a
/// NON-negative in-bounds index (`a[-1]`/OOB → `undefined`, no wrap); anything else
/// is treated as an OBJECT and keyed by `ToString(idx)` via `obj_get` (absent →
/// `undefined`). This is what makes `p[0]`/`p[1]` work on a for-of binding whose
/// element is a nested array (`new Map([[k,v],…])`), without the proven-shape path.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_idx_get(recv: u64, idx: u64) -> u64 {
    use rts_runtime::namespaces::collections::vec as rt_vec;
    // A Proxy is `TAG_OBJECT` but `is_array_word` would misclassify it as an array
    // (not a keyed object); route it to `obj_get` so its `get` trap fires.
    if super::objops::is_proxy_word(recv) {
        return super::objops::__rtsadp_obj_get(recv, idx);
    }
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let units = utf16_units(recv);
        let i = genops_to_i64(idx);
        if i < 0 || i >= units.len() as i64 {
            return undef();
        }
        return box_str(intern_utf16_unit(units[i as usize]));
    }
    // A BUFFER (`Entry::Buffer` — TextEncoder.encode bytes): numeric index reads
    // the byte (Uint8Array surface); OOB → undefined. Before the array arm — a
    // Buffer word also passes `is_array_word` and `VEC_GET` on it reads nothing.
    if v.is_object() {
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        let i = genops_to_i64(idx);
        if let Some(byte) = rts_engine::heap::handles::with_entry(h, |e| match e {
            Some(rts_engine::heap::handles::Entry::Buffer(b)) => {
                Some(usize::try_from(i).ok().and_then(|i| b.get(i).copied()))
            }
            _ => None,
        }) {
            return match byte {
                Some(b) => PolyValue::from_i32(b as i32).raw(),
                None => undef(),
            };
        }
    }
    if is_array_word(recv) {
        // A SYMBOL key on an array (#216/#299): `arr[Symbol.iterator]` yields
        // the native values-iterator FUNCTION (a real callable Entry::Function,
        // `typeof === "function"`); any other symbol key is an absent property
        // → `undefined`. Checked BEFORE the numeric decode — a symbol word must
        // never coerce to an element index (it silently read `arr[0..]`).
        {
            use rts_runtime::namespaces::gc::handles as rt_handles;
            let k = PolyValue::from_raw(idx);
            if k.is_object() {
                let kh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(k.as_handle());
                let is_symbol = rt_handles::with_entry(kh, |e| {
                    matches!(e, Some(rt_handles::Entry::Symbol { .. }))
                });
                if is_symbol {
                    let iter_sym =
                        rts_runtime::namespaces::globals::symbol::__RTS_FN_GL_SYMBOL_ITERATOR();
                    if kh == iter_sym {
                        let f = rts_runtime::namespaces::gc::generator::__RTS_FN_GL_ARRAY_ITERATOR_FN();
                        let slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(f);
                        return PolyValue::from_function_handle(slot).raw();
                    }
                    return undef();
                }
            }
        }
        let h = arr_handle(recv);
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
        let i = genops_to_i64(idx);
        if i < 0 || i >= len {
            return undef();
        }
        return rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64;
    }
    // Object (or any non-string/non-array): key by `idx` via obj_get. `obj_get`'s
    // key param is a PolyValue WORD (its `key_text` ToStrings internally — `o[0]`
    // keys on "0"), so pass the raw `idx` word directly, NOT a pre-interned handle.
    super::objops::__rtsadp_obj_get(recv, idx)
}

/// `recv.slice(start, end)` — string slice (a string word) OR array slice (a fresh
/// array word). Both take two numeric bounds (the lowering supplies a defaulted
/// "to end" bound for the 1-arg form). Unexpected receiver → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_slice(recv: u64, start: u64, end: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    let s = genops_to_i64(start);
    let e = genops_to_i64(end);
    if v.is_string() {
        return box_str(rt_gl_str::__RTS_FN_GL_STRING_SLICE(str_handle(recv), s, e));
    }
    if is_array_word(recv) {
        return arrayops::__rtsadp_arr_slice(arr_handle(recv), s, e);
    }
    undef()
}

/// `recv.concat(other)` — string concatenation (string result) OR array concat (a
/// fresh array word). String path ToStrings the arg. Unexpected receiver →
/// `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_concat(recv: u64, other: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let other_h = str_arg_handle(other);
        return box_str(rt_gl_str::__RTS_FN_GL_STRING_CONCAT(
            str_handle(recv),
            other_h,
        ));
    }
    if is_array_word(recv) {
        return arrayops::__rtsadp_arr_concat(arr_handle(recv), other);
    }
    undef()
}

// ===========================================================================
// Array-only methods (string receivers have no such method → undefined). The
// dynamic dispatch still tag-checks: an array receiver runs the real op; any other
// runtime tag returns the sentinel rather than guess.
// ===========================================================================

/// `arr.join(sep)` — Array join with a (ToString'd) separator. Array receiver →
/// the real `__rtsadp_arr_join` (string result); any other receiver → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_join(recv: u64, sep: u64) -> u64 {
    if is_array_word(recv) {
        let sep_h = str_arg_handle(sep);
        return box_str(arrayops::__rtsadp_arr_join(arr_handle(recv), sep_h));
    }
    undef()
}

/// `arr.push(x)` — append; returns the new length (a number word). Array receiver
/// only; other receivers → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_push(recv: u64, val: u64) -> u64 {
    if is_array_word(recv) {
        let len = arrayops::__rtsadp_arr_push(arr_handle(recv), val);
        return PolyValue::from_i32(len as i32).raw();
    }
    undef()
}

/// `arr.pop()` — remove + return the last element word. Array receiver only.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_pop(recv: u64) -> u64 {
    if is_array_word(recv) {
        return arrayops::__rtsadp_arr_pop(arr_handle(recv));
    }
    undef()
}

/// `arr.reverse()` — reverse IN PLACE, returning the (same) array word so a chain
/// (`a.reverse().join(",")`) works. Array receiver only; other receivers →
/// `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_reverse(recv: u64) -> u64 {
    if is_array_word(recv) {
        return arrayops::__rtsadp_arr_reverse(arr_handle(recv));
    }
    undef()
}

/// `arr.sort()` — default (ToString) sort IN PLACE, returning the (same) array
/// word for chaining. The comparator form `sort(cmp)` is a callback method handled
/// on the proven-array path. Array receiver only; other receivers → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_sort(recv: u64) -> u64 {
    if is_array_word(recv) {
        return arrayops::__rtsadp_arr_sort(arr_handle(recv));
    }
    undef()
}

// ===========================================================================
// String-only methods (array receivers have no such method → undefined).
// ===========================================================================

/// `s.charAt(i)` — the 1-char string at index `i` (`""` out of range). String
/// receiver only; other receivers → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_char_at(recv: u64, idx: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        // JS `charAt` indexes UTF-16 code units, returning the 1-unit string (`""`
        // out of range). UTF-16 over the real bytes so surrogate strings match bun.
        let units = utf16_units(recv);
        let i = genops_to_i64(idx);
        if i < 0 || i >= units.len() as i64 {
            return box_str(empty_string_handle());
        }
        return box_str(intern_utf16_unit(units[i as usize]));
    }
    undef()
}

/// `s.charCodeAt(i)` — the UTF-16 code unit at `i` as a number (`NaN` out of
/// range). String receiver only; other receivers → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_char_code_at(recv: u64, idx: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        // JS `charCodeAt` returns the UTF-16 CODE UNIT at the index (a surrogate
        // half for an astral char), or `NaN` out of range. Compute over the real
        // UTF-16 units so emoji/surrogate strings match bun.
        let units = utf16_units(recv);
        let i = genops_to_i64(idx);
        return if i < 0 || i >= units.len() as i64 {
            PolyValue::from_f64(f64::NAN).raw()
        } else {
            PolyValue::from_i32(units[i as usize] as i32).raw()
        };
    }
    undef()
}

/// `s.toUpperCase()` — uppercase string. String receiver only.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_to_upper_case(recv: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        return box_str(rt_gl_str::__RTS_FN_GL_STRING_TO_UPPER_CASE(str_handle(
            recv,
        )));
    }
    undef()
}

/// `s.toLowerCase()` — lowercase string. String receiver only.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_to_lower_case(recv: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        return box_str(rt_gl_str::__RTS_FN_GL_STRING_TO_LOWER_CASE(str_handle(
            recv,
        )));
    }
    undef()
}

/// `s.trim()` — whitespace-trimmed string. String receiver only.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_trim(recv: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        return box_str(rt_gl_str::__RTS_FN_GL_STRING_TRIM(str_handle(recv)));
    }
    undef()
}

/// `s.split(sep)` — split a string by a (ToString'd) string separator into an
/// ARRAY of boxed string words. String receiver only; other receivers →
/// `undefined`. A regex separator is not modeled (the arg is ToString'd, matching
/// the corpus's literal-string separators).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_split(recv: u64, sep: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let sep_h = str_arg_handle(sep);
        // Reuse the P5.2 split trampoline (PolyValue-native array of strings).
        return super::globalops::__rtsadp_str_split(str_handle(recv), sep_h, -1);
    }
    undef()
}

/// `s.startsWith(x)` / `s.endsWith(x)` share the dynamic shape; the lowering picks
/// the right symbol. `s.startsWith(prefix)` — boolean word. String receiver only.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_starts_with(recv: u64, needle: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let h = str_arg_handle(needle);
        return PolyValue::bool(
            rt_gl_str::__RTS_FN_GL_STRING_STARTS_WITH(str_handle(recv), h) != 0,
        )
        .raw();
    }
    undef()
}

/// `s.endsWith(suffix)` — boolean word. String receiver only.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_ends_with(recv: u64, needle: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let h = str_arg_handle(needle);
        return PolyValue::bool(rt_gl_str::__RTS_FN_GL_STRING_ENDS_WITH(str_handle(recv), h) != 0)
            .raw();
    }
    undef()
}

/// `s.repeat(n)` — the string repeated `n` times. String receiver only.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_repeat(recv: u64, n: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let count = genops_to_i64(n);
        return box_str(rt_gl_str::__RTS_FN_GL_STRING_REPEAT(
            str_handle(recv),
            count,
        ));
    }
    undef()
}

// ===========================================================================
// helpers — ToString an arg to a real string handle / ToNumber an arg to i64,
// reusing the engine's ONE coercion rule (so the dynamic path coerces identically
// to the static one).
// ===========================================================================

/// ToString a PolyValue-word argument to a REAL string handle (for the string-op
/// arg slots). A string word uses its own handle; any other word is ToString'd via
/// [`genops`] and interned in the real pool — the SAME coercion the static path
/// applies via its `Handle` marshaling (which requires a proven string; the
/// dynamic path is more permissive, matching JS arg coercion).
fn str_arg_handle(word: u64) -> u64 {
    let s_word = genops::__rtsadp_to_string(word);
    abi_adapter::real_handle_of(PolyValue::from_raw(s_word))
}

/// ToNumber a PolyValue-word argument to an i64 index (truncating toward zero),
/// reusing the engine's ONE `ToNumber` ([`genops`]).
fn genops_to_i64(word: u64) -> i64 {
    let n = genops_to_number(word);
    if n.is_finite() { n as i64 } else { 0 }
}

/// ToNumber via the engine's ONE `ToNumber` rule (`genops`), re-exposed publicly
/// to the `value` siblings so the dynamic path coerces identically to `+`/`<`/….
fn genops_to_number(word: u64) -> f64 {
    genops::dyn_to_number(word)
}
