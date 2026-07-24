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
        // An INVALID string handle (a runtime fn's error sentinel — e.g. digest
        // over an unknown-algorithm hasher returns handle 0, never interned):
        // `.length` reports the pool's own -1 sentinel. An EMPTY string is a
        // LIVE pool entry, so `""` still reads 0 through the code-unit path.
        let h = abi_adapter::real_handle_of(v);
        if rts_runtime::namespaces::collector::string_pool::__RTS_FN_NS_GC_STRING_LEN(h) < 0 {
            return PolyValue::from_i32(-1).raw();
        }
        // JS `String.prototype.length` is the UTF-16 CODE-UNIT count, NOT the byte
        // length (`STRING_LEN` is bytes — they differ for any non-ASCII char). We
        // OWN this trampoline, so we read the real bytes and count code units
        // (`encode_utf16`), matching bun/Node exactly (`"😀".length === 2`).
        let s = abi_adapter::resolve_poly(v);
        let units = s.chars().map(|c| c.len_utf16()).sum::<usize>();
        return PolyValue::from_i32(units as i32).raw();
    }
    // A level-B typed-array VIEW: element COUNT (buffer bytes / elem width).
    if let Some((bh, bytes, ..)) = super::taops::view_parts(recv) {
        return PolyValue::from_i32(super::taops::view_len(bh, bytes) as i32).raw();
    }
    // A legacy DICTIONARY row (`Entry::Map` — match/exec/matchAll results):
    // its own `length` entry, read straight off the IndexMap (BEFORE the
    // array arm — a Map word passes `is_array_word` and `VEC_LEN` reads 0).
    if v.is_object() {
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        let hit = rts_engine::heap::handles::with_entry(h, |e| match e {
            Some(rts_engine::heap::handles::Entry::Map(m)) => {
                Some(m.get("length").copied())
            }
            _ => None,
        });
        if let Some(len) = hit {
            return match len {
                Some(n) if (i32::MIN as i64..=i32::MAX as i64).contains(&n) => {
                    PolyValue::from_i32(n as i32).raw()
                }
                _ => undef(),
            };
        }
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
    // A FUNCTION receiver: `.length` is the Function getter — the declared
    // arity minus the partially-bound count (`f.bind(null, x)`).
    if v.is_function() {
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        let len = rts_runtime::namespaces::globals::function::ops::__RTS_FN_GL_FUNCTION_LENGTH(h);
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

/// The UTF-16 code unit at `i`, WITHOUT materializing the whole string.
///
/// Indexing used to build a full `Vec<u16>` per access (`utf16_units`), so a
/// char-by-char loop over an n-char string did O(n) work n times — O(n²), with
/// two heap copies per character. A 115 KB `JSON.parse` measured 42 s.
///
/// Fast path: an all-ASCII string has one UTF-16 unit per byte, so the unit at
/// `i` IS the byte at `i` — an O(1) read with no allocation. That covers the
/// overwhelmingly common case (source text, JSON, identifiers). Only when a
/// non-ASCII byte is present do we fall back to an exact UTF-16 walk, which is
/// still allocation-free.
///
/// Returns `None` when `i` is out of range.
fn utf16_unit_at(recv: u64, i: i64) -> Option<u16> {
    if i < 0 {
        return None;
    }
    let handle = abi_adapter::real_handle_of(PolyValue::from_raw(recv));
    abi_adapter::with_handle_bytes(handle, |bytes| {
        let idx = i as usize;
        // is_ascii() é O(n): sem memorizar, indexar num laço voltaria a ser
        // O(n²). O cache é por (ptr,len) — ver `is_ascii_cached`.
        if is_ascii_cached(bytes) {
            return bytes.get(idx).map(|b| *b as u16);
        }
        let text = std::str::from_utf8(bytes).ok()?;
        text.encode_utf16().nth(idx)
    })
}

thread_local! {
    /// Última string testada: (ptr, len, é_ascii). Uma entrada basta — o padrão
    /// de uso é varrer uma string do início ao fim.
    static ASCII_CACHE: std::cell::Cell<(usize, usize, bool)> =
        const { std::cell::Cell::new((0, 0, false)) };
}

/// `bytes.is_ascii()` memorizado por (endereço, tamanho).
#[inline]
fn is_ascii_cached(bytes: &[u8]) -> bool {
    let key = (bytes.as_ptr() as usize, bytes.len());
    ASCII_CACHE.with(|c| {
        let (p, l, a) = c.get();
        if p == key.0 && l == key.1 {
            return a;
        }
        let a = bytes.is_ascii();
        c.set((key.0, key.1, a));
        a
    })
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
    // A level-B typed-array VIEW (`__ta_buf`-shaped): element read through the
    // SHARED buffer.
    if let Some((bh, bytes, signed, float)) = super::taops::view_parts(recv) {
        return super::taops::view_get(bh, bytes, signed, float, genops_to_i64(idx));
    }
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        // Only a genuine ARRAY-INDEX key reads a code unit. A NAMED key
        // (`s["length"]`, `s["toUpperCase"]`) is a property of the autoboxed
        // String — route it through `obj_get`, which resolves `length` and walks
        // the intrinsic `String.prototype`. Coercing every key with ToNumber
        // here was wrong: `"length"` → NaN → 0 read the FIRST CHARACTER.
        if let Some(i) = array_index_key(idx) {
            // O(1) por acesso (ver `utf16_unit_at`): antes materializava a
            // string inteira em UTF-16 a cada índice, o que fazia qualquer
            // laço char-a-char virar O(n²).
            return match utf16_unit_at(recv, i) {
                Some(u) => box_str(intern_utf16_unit(u)),
                None => undef(),
            };
        }
        return super::objops::__rtsadp_obj_get(recv, idx);
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
    // A legacy DICTIONARY entry (`Entry::Map` — matchAll rows etc.) is object-
    // tagged but NOT a keyed shape-vec, so `is_array_word` would misroute it to
    // `VEC_GET`; key it through `obj_get` (whose Map branch reads the IndexMap).
    if v.is_object() && !super::objops::is_proxy_word(recv) {
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        let is_map = rts_engine::heap::handles::with_entry(h, |e| {
            matches!(e, Some(rts_engine::heap::handles::Entry::Map(_)))
        });
        if is_map {
            return super::objops::__rtsadp_obj_get(recv, idx);
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
        // A STRING key on an array: a NUMERIC string still indexes (`a["1"]`);
        // `"length"` reads the length; any other name is an absent property.
        {
            let kv = PolyValue::from_raw(idx);
            if kv.is_string() {
                let s = abi_adapter::resolve_poly(kv);
                if s == "length" {
                    return __rtsadp_dyn_length(recv);
                }
                if s.parse::<i64>().is_err() {
                    return undef();
                }
            }
        }
        let i = genops_to_i64(idx);
        if i < 0 || i >= len {
            return undef();
        }
        let elem = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64;
        // A HOLE slot (deleted / sparse element) reads as `undefined` ([[Get]]).
        if PolyValue::from_raw(elem).is_hole() {
            return undef();
        }
        // A LEGACY raw-handle element (a runtime-built Vec — e.g.
        // `Promise.allSettled`'s result rows are raw `Entry::Map` handles, not
        // words): box it by live entry kind. A boxed word / small number passes
        // verbatim; a ≥2^48 non-boxed value with a live entry is never a real
        // double in practice (the generation bits).
        const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
        if (elem & BOX_BASE) != BOX_BASE && elem >= (1u64 << 48) {
            let live = rts_engine::heap::handles::with_entry(elem, |e| e.is_some());
            if live {
                return genops::__rtsadp_box_handle_auto(elem);
            }
        }
        return elem;
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

/// `arr.push(x)` — append; returns the new length (a number word). On a NON-array
/// receiver that is an object with its OWN `push` method (a `stream.Readable`
/// subclass, any user class named-method `push`), route to that instance method
/// instead of no-op'ing: an `any`-typed receiver (`self.push(x)` inside a method,
/// where the compiler could not prove the class) must still reach the real method,
/// not the Array fast-path. Only when neither an array nor a `push`-bearing object
/// → `undefined` (unchanged).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_push(recv: u64, val: u64) -> u64 {
    if is_array_word(recv) {
        let len = arrayops::__rtsadp_arr_push(arr_handle(recv), val);
        return PolyValue::from_i32(len as i32).raw();
    }
    if PolyValue::from_raw(recv).is_object() {
        let key = abi_adapter::intern_poly("push").raw();
        let m = super::objops::__rtsadp_obj_get(recv, key);
        if PolyValue::from_raw(m).is_function() {
            let undef = undef();
            return super::funcops::__rtsadp_fn_invoke_method(m, recv, val, undef, undef, 0);
        }
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
        // O(1) via `utf16_unit_at` — materializar os units por chamada fazia um
        // laço char-a-char custar O(n²).
        let i = genops_to_i64(idx);
        return match utf16_unit_at(recv, i) {
            Some(u) => box_str(intern_utf16_unit(u)),
            None => box_str(empty_string_handle()),
        };
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
        // UTF-16 units so emoji/surrogate strings match bun. O(1) por chamada
        // (ver `utf16_unit_at`) — antes era O(n), e O(n²) num laço.
        let i = genops_to_i64(idx);
        return match utf16_unit_at(recv, i) {
            Some(u) => PolyValue::from_i32(u as i32).raw(),
            None => PolyValue::from_f64(f64::NAN).raw(),
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

/// `s.localeCompare(other)` — `-1`/`0`/`1` (locale order). String receiver only;
/// other receivers → `undefined`. The `other` arg is ToString'd (JS coerces it),
/// so `"a".localeCompare(1)` compares against `"1"`. Reuses the SAME
/// `__RTS_FN_GL_STRING_LOCALE_COMPARE` impl the proven-string typed row calls.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_locale_compare(recv: u64, other: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let r = rt_gl_str::__RTS_FN_GL_STRING_LOCALE_COMPARE(str_handle(recv), str_arg_handle(other));
        return PolyValue::from_i32(r as i32).raw();
    }
    undef()
}

/// `s.codePointAt(i)` — the Unicode CODE POINT starting at UTF-16 index `i`
/// (composing a surrogate pair), or `undefined` out of range. String receiver
/// only; other receivers → `undefined`. Returns the raw PolyValue word verbatim
/// from `__RTS_FN_GL_STRING_CODE_POINT_AT` (a number word OR `undefined`), the
/// SAME impl the proven-string typed row calls.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_code_point_at(recv: u64, idx: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        return rt_gl_str::__RTS_FN_GL_STRING_CODE_POINT_AT(str_handle(recv), genops_to_i64(idx));
    }
    undef()
}

/// `Promise.resolve(x)` with an UNPROVEN `x` — THENABLE-aware (spec
/// PromiseResolve): a keyed object whose `then` GETTER throws yields a
/// REJECTED promise carrying the thrown value; a callable `then` is ADOPTED
/// (invoked with resolve/reject settlers on a fresh pending promise); anything
/// else settles a fulfilled promise with the value. Returns the real promise
/// HANDLE (the same convention `__RTS_FN_GL_PROMISE_RESOLVE` returns).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_promise_resolve_w(word: u64) -> u64 {
    use rts_runtime::namespaces::gc::handles as rt_handles;
    use rts_runtime::namespaces::promise_slot;
    let v = PolyValue::from_raw(word);
    if v.is_object() && super::inspect::looks_like_object(v) {
        let then = super::objops::__rtsadp_obj_get(
            word,
            abi_adapter::intern_poly("then").raw(),
        );
        if super::errslot::__rtsadp_err_pending() != 0 {
            // The `then` GETTER threw — resolve() catches and REJECTS (spec).
            let e = super::errslot::__rtsadp_err_take();
            let slot = promise_slot::new_pending();
            promise_slot::mark_handled(&slot);
            promise_slot::reject(&slot, e as i64);
            return rt_handles::alloc_entry(rt_handles::Entry::PromiseAsync(slot));
        }
        if PolyValue::from_raw(then).is_function() {
            // ADOPT: run `then.call(x, res, rej)` with real settler fn values
            // over a fresh pending promise.
            let slot = promise_slot::new_pending();
            let out = rt_handles::alloc_entry(rt_handles::Entry::PromiseAsync(slot));
            let (res_fn, rej_fn) = super::funcops::settler_pair(out);
            let undef = PolyValue::undefined().raw();
            super::funcops::__rtsadp_fn_invoke_method(then, word, res_fn, rej_fn, undef, 0);
            return out;
        }
    }
    // Plain value / existing promise: the legacy resolve path (word passes as
    // handle-or-value exactly like the static marshal did).
    let raw = if v.is_object() || v.is_function() || v.is_string() {
        rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle())
    } else {
        word
    };
    let r = rts_runtime::namespaces::globals::fetch::instance::__RTS_FN_GL_PROMISE_RESOLVE(raw);
    r as u64
}

/// `recv.method(a0..a2)` — GENERIC dynamic method dispatch on a Tagged
/// receiver whose method the static table doesn't cover: read `method` off the
/// receiver (own slot or the PROTOTYPE chain — every user/`.ts` class publishes
/// its methods on the proto), and invoke it as a method (`this` = recv). This
/// is what makes `x.forEach(cb)` / `x.anyUserMethod(a)` on an `any`-typed local
/// resolve WITHOUT the engine naming or knowing the class's layout — fully
/// data-driven (the proto slot IS the class's own method). A primordial ARRAY
/// receiver routes its Array.prototype method by name (no proto object). A miss
/// throws TypeError (JS).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_method_call(
    recv: u64,
    key: u64,
    a0: u64,
    a1: u64,
    a2: u64,
) -> u64 {
    let undef = PolyValue::undefined().raw();
    // A native object-backed Registry instance (`Hash`/`Stats`/`StringDecoder` —
    // an `Entry::Map` tagged `__rts_class`) whose methods are native fns: resolve
    // via the receiver's own class tag + the harvested runtime-CI table. Tried
    // FIRST because such an instance is not a plain-object shape and would
    // otherwise be misrouted by the `is_array_word` gate below. Returns `None`
    // (falls through) for any receiver without a matching `(class, method)`.
    let js_argc = [a0, a1, a2].iter().filter(|&&w| w != undef).count();
    if let Some(out) = super::dynci::try_runtime_ci(recv, key, a0, a1, a2, js_argc) {
        return out;
    }
    // An ARRAY receiver: dispatch the primordial Array.prototype method by
    // RUNTIME name (the same table `idx_call` uses).
    if is_array_word(recv) {
        // 3 explicit args → argc word; unused slots are `undefined`.
        let argc = [a0, a1, a2]
            .iter()
            .filter(|&&w| w != undef)
            .count() as u64;
        return __rtsadp_idx_call(recv, key, a0, a1, argc);
    }
    // Read the method off the receiver (own or proto chain). The obj_get chain
    // walk finds a method published on the class prototype.
    let m = super::objops::__rtsadp_obj_get(recv, key);
    if PolyValue::from_raw(m).is_function() {
        return super::funcops::__rtsadp_fn_invoke_method(m, recv, a0, a1, a2, 0);
    }
    let name = abi_adapter::resolve_poly(PolyValue::from_raw(key));
    super::errslot::throw_js_error("TypeError", &format!("{name} is not a function"));
    undef
}

/// `recv.method(a0..a2)` resolved ONLY via the object-backed runtime-CI table
/// (the receiver's `__rts_class` tag), returning `undefined` on a miss instead of
/// throwing. This is the SENTINEL-preserving variant used as the default arm of
/// the user-class virtual dispatch: when no user shape and no own function-valued
/// property matched `method`, the receiver may still be a native object-backed
/// instance (`Hash`/`StringDecoder`/…) whose method name collides with some user
/// class's method — dispatch it natively, else keep the `undefined` sentinel (a
/// TypeError-class marker, never a wrong value) the virtual path already used.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_ci_or_undef(recv: u64, key: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let undef = PolyValue::undefined().raw();
    let js_argc = [a0, a1, a2].iter().filter(|&&w| w != undef).count();
    super::dynci::try_runtime_ci(recv, key, a0, a1, a2, js_argc).unwrap_or(undef)
}

/// The `@@<name>` HOOK method of an OBJECT arg (`matcher[Symbol.match]`), or
/// `None` — string protocol hooks (match/replace/search/split).
fn wellknown_hook(arg: u64, name: &str) -> Option<u64> {
    let v = PolyValue::from_raw(arg);
    if !v.is_object() {
        return None;
    }
    let m = super::objops::__rtsadp_obj_get(arg, abi_adapter::intern_poly(name).raw());
    PolyValue::from_raw(m).is_function().then_some(m)
}

/// `s.match(arg)` with an UNPROVEN (object) arg — a custom `[Symbol.match]`
/// hook runs (`hook.call(arg, s)`, spec); a hook-less object coerces ToString
/// into a fresh regex (spec `new RegExp(arg)`), delegating to the real match.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_match_w(recv: u64, arg: u64) -> u64 {
    let undef = PolyValue::undefined().raw();
    if let Some(h) = wellknown_hook(arg, "@@match") {
        return super::funcops::__rtsadp_fn_invoke_method(h, arg, recv, undef, undef, 0);
    }
    let pat = genops::__rtsadp_to_string(arg);
    let re = super::regexops::__rtsadp_re_compile(pat, undef);
    super::regexops::__rtsadp_re_str_match(recv, re)
}

/// `s.replace(pat, rep)` with an UNPROVEN pattern — the `[Symbol.replace]`
/// hook runs (`hook.call(pat, s, rep)`, spec); hook-less delegates to the
/// plain string replace on `ToString(pat)`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_replace_w(recv: u64, pat: u64, rep: u64) -> u64 {
    let undef = PolyValue::undefined().raw();
    if let Some(h) = wellknown_hook(pat, "@@replace") {
        return super::funcops::__rtsadp_fn_invoke_method(h, pat, recv, rep, undef, 0);
    }
    let sh = abi_adapter::real_handle_of(PolyValue::from_raw(genops::__rtsadp_to_string(recv)));
    let ph = abi_adapter::real_handle_of(PolyValue::from_raw(genops::__rtsadp_to_string(pat)));
    let rh = abi_adapter::real_handle_of(PolyValue::from_raw(genops::__rtsadp_to_string(rep)));
    box_str(rt_gl_str::__RTS_FN_GL_STRING_REPLACE(sh, ph, rh))
}

/// `s.split(sep, limit?)` with an UNPROVEN separator — the `[Symbol.split]`
/// hook runs (`hook.call(sep, s, limit)`, spec); hook-less delegates to the
/// plain string split on `ToString(sep)` (limit via the dyn split).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_split_w(recv: u64, sep: u64, limit: u64) -> u64 {
    let undef = PolyValue::undefined().raw();
    if let Some(h) = wellknown_hook(sep, "@@split") {
        return super::funcops::__rtsadp_fn_invoke_method(h, sep, recv, limit, undef, 0);
    }
    let sep_s = genops::__rtsadp_to_string(sep);
    __rtsadp_dyn_split(recv, sep_s)
}

/// `recv[key](a0..)` — a CALL whose callee is a COMPUTED index (`arr[k](x)`,
/// the obfuscator staple `arr["pu"+"sh"](x)`). A NUMERIC key invokes the
/// element as a function value; a STRING key on an ARRAY dispatches the
/// primordial Array method by name (the same trampolines the static rows
/// use); a keyed OBJECT reads the property and invokes it as a method
/// (`this` = recv). Unknown name/receiver → TypeError (pending error).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_idx_call(recv: u64, key: u64, a0: u64, a1: u64, argc: u64) -> u64 {
    let kv = PolyValue::from_raw(key);
    let is_num_key = kv.is_int32() || kv.is_double();
    if is_num_key {
        let elem = __rtsadp_idx_get(recv, key);
        return super::funcops::__rtsadp_fn_invoke(
            elem,
            a0,
            a1,
            undef(),
            undef(),
            PolyValue::from_i32(argc as i32).raw(),
        );
    }
    let name = if kv.is_string() {
        abi_adapter::resolve_poly(kv)
    } else {
        abi_adapter::resolve_poly(PolyValue::from_raw(genops::__rtsadp_to_string(key)))
    };
    if is_array_word(recv) {
        let h = arr_handle(recv);
        let n = argc as usize;
        // Primordial Array rows by RUNTIME name — the same impls the static
        // dispatch resolves; the hot obfuscator set.
        match (name.as_str(), n) {
            ("push", 1) => return PolyValue::from_i32(
                arrayops::__rtsadp_arr_push(h, a0) as i32,
            )
            .raw(),
            ("pop", 0) => return arrayops::__rtsadp_arr_pop(h),
            ("shift", 0) => return arrayops::__rtsadp_arr_shift(h),
            ("join", 0) => return box_str(arrayops::__rtsadp_arr_join0(h)),
            ("join", 1) => {
                let sep = abi_adapter::real_handle_of(PolyValue::from_raw(
                    genops::__rtsadp_to_string(a0),
                ));
                return box_str(arrayops::__rtsadp_arr_join(h, sep));
            }
            ("indexOf", 1) => {
                return PolyValue::from_i32(arrayops::__rtsadp_arr_index_of(h, a0) as i32).raw();
            }
            ("includes", 1) => {
                return PolyValue::bool(arrayops::__rtsadp_arr_includes(h, a0) != 0).raw();
            }
            _ => {}
        }
    }
    // Keyed object (or anything else): property read + method invoke.
    let m = super::objops::__rtsadp_obj_get(recv, key);
    if PolyValue::from_raw(m).is_function() {
        return super::funcops::__rtsadp_fn_invoke_method(m, recv, a0, a1, undef(), 0);
    }
    super::errslot::throw_js_error(
        "TypeError",
        &format!("{name} is not a function"),
    );
    undef()
}

/// The real PROMISE handle behind a Tagged receiver word (`Entry::PromiseAsync`)
/// — `None` for any other receiver. Promise is PRIMORDIAL, so the tag
/// inspection here is doctrine-clean.
fn promise_handle(recv: u64) -> Option<u64> {
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let v = PolyValue::from_raw(recv);
    // Same dual detection the polymorphic `await` uses: an OBJECT-tagged word
    // (box_handle_auto) reconstructs the handle from the live slot; a NUMBER
    // word ≥ 2^48 may carry a RAW handle (a legacy producer / marshal path).
    let h = if v.is_object() {
        rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle())
    } else {
        let f = if v.is_double() {
            v.as_f64()
        } else if v.is_int32() {
            return None;
        } else {
            return None;
        };
        const HANDLE_MIN: f64 = 281_474_976_710_656.0; // 2^48
        if !(f.is_finite() && f >= HANDLE_MIN && f <= u64::MAX as f64 && f.fract() == 0.0) {
            return None;
        }
        f as u64
    };
    let is_p = rt_handles::with_entry(h, |e| {
        matches!(e, Some(rt_handles::Entry::PromiseAsync(_)))
    });
    is_p.then_some(h)
}

/// `p.then(onFul)` on an UNPROVEN receiver — routes the primordial Promise
/// instance method when the word wraps a live promise; otherwise `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_p_then(recv: u64, cb: u64) -> u64 {
    use rts_runtime::namespaces::globals::fetch::instance as p;
    match promise_handle(recv) {
        Some(h) => genops::__rtsadp_box_handle_auto(p::__RTS_FN_GL_PROMISE_THEN2(h, cb, 0)),
        None => undef(),
    }
}

/// `p.then(onFul, onRej)` — the 2-arg spec form.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_p_then2(recv: u64, on_ful: u64, on_rej: u64) -> u64 {
    use rts_runtime::namespaces::globals::fetch::instance as p;
    match promise_handle(recv) {
        Some(h) => {
            genops::__rtsadp_box_handle_auto(p::__RTS_FN_GL_PROMISE_THEN2(h, on_ful, on_rej))
        }
        None => undef(),
    }
}

/// `p.catch(onRej)` on an unproven receiver.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_p_catch(recv: u64, cb: u64) -> u64 {
    use rts_runtime::namespaces::globals::fetch::instance as p;
    match promise_handle(recv) {
        Some(h) => genops::__rtsadp_box_handle_auto(p::__RTS_FN_GL_PROMISE_CATCH(h, cb)),
        None => undef(),
    }
}

/// `p.finally(onFin)` on an unproven receiver.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_p_finally(recv: u64, cb: u64) -> u64 {
    use rts_runtime::namespaces::globals::fetch::instance as p;
    match promise_handle(recv) {
        Some(h) => genops::__rtsadp_box_handle_auto(p::__RTS_FN_GL_PROMISE_FINALLY(h, cb)),
        None => undef(),
    }
}

/// `s.trimStart()` — leading-whitespace strip on a dynamic string receiver.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_trim_start(recv: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let s = abi_adapter::resolve_poly(v);
        return abi_adapter::intern_poly(s.trim_start()).raw();
    }
    undef()
}

/// `s.trimEnd()` — trailing-whitespace strip on a dynamic string receiver.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_trim_end(recv: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    if v.is_string() {
        let s = abi_adapter::resolve_poly(v);
        return abi_adapter::intern_poly(s.trim_end()).raw();
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

/// The NON-NEGATIVE array-index a key word denotes, or `None` when the key is a
/// NAME (a property, not an index). A numeric key must be a non-negative integer
/// (`s[1.5]` / `s[-1]` are names in JS, hence absent properties); a STRING key
/// counts only when it is the canonical decimal form of one (`s["1"]` indexes,
/// `s["01"]`/`s["length"]` do not). Used to keep the code-unit read of a string
/// receiver from swallowing named members via ToNumber coercion.
/// A NON-NEGATIVE integer index straight off a key PolyValue, allocation-free.
///
/// An `int32` or an integral finite non-negative `double` yields its value; a
/// STRING key is decoded only in its canonical integer form (`"5"`, not `"05"`/
/// `" 5"`/`"+5"`, which are property NAMES per JS ToPropertyKey). Anything else
/// is `None`, so the caller falls back to the general keyed path. Shared with the
/// typed-array element paths in [`super::objops`], which must never stringify a
/// numeric key.
pub(crate) fn array_index_key(word: u64) -> Option<i64> {
    let k = PolyValue::from_raw(word);
    if k.is_int32() {
        let i = k.as_i32() as i64;
        return (i >= 0).then_some(i);
    }
    if k.is_double() {
        let d = k.as_f64();
        let i = d as i64;
        return (d >= 0.0 && d.is_finite() && i as f64 == d).then_some(i);
    }
    if k.is_string() {
        let s = abi_adapter::resolve_poly(k);
        let i = s.parse::<i64>().ok()?;
        // Canonical form only: "01"/"+1"/" 1" are names, not indices.
        return (i >= 0 && i.to_string() == s).then_some(i);
    }
    None
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

/// `recv.toString(radix)` — tag-dispatched: a NUMBER receiver formats in the
/// radix (`(255).toString(16)` → "ff"); any other receiver ignores the arg
/// (JS String/Array/Object toString take none) and falls to the plain
/// ToString. Radix outside 2..=36 clamps to 10 (never a wrong panic).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_dyn_to_string_radix(recv: u64, radix: u64) -> u64 {
    let v = PolyValue::from_raw(recv);
    let is_num = v.is_int32() || v.is_double();
    if is_num {
        let r = {
            let rv = PolyValue::from_raw(radix);
            if rv.is_int32() {
                rv.as_i32() as i64
            } else if rv.is_double() {
                rv.as_f64() as i64
            } else {
                10
            }
        };
        let r = if (2..=36).contains(&r) { r } else { 10 };
        let h = rts_runtime::namespaces::engine::__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX(
            if v.is_int32() { v.as_i32() as f64 } else { v.as_f64() },
            r,
        );
        return box_str(h);
    }
    __rtsadp_dyn_to_string(recv)
}
