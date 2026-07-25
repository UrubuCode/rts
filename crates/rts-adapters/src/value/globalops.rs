//! Codegen-owned GLOBAL constant/function + Array/String STATIC trampolines (P5.2).
//!
//! The global coercion functions (`Number(x)`, `String(x)`, `Boolean(x)`,
//! `parseInt`, `parseFloat`, `isNaN`, `isFinite`), the Array statics
//! (`Array.isArray`, `Array.of`, `Array.from`, `Array(n)`), and the String
//! statics (`String.fromCharCode`, `String.fromCodePoint`, `str.split`) all need
//! to produce / inspect the engine's OWN value representation: a [`PolyValue`]
//! word and, for the array-producing ones, a real `Entry::Vec` of boxed PolyValue
//! WORDS (NOT the runtime's raw-i64 element convention). So — exactly like
//! [`super::arrayops`] / [`super::genops`] — these are codegen-owned `__rtsadp_*`
//! trampolines (NOT `__RTS_FN_*`), reusing the SAME genops coercions
//! (`to_number`/`to_string`/`to_boolean`) so number formatting / coercion never
//! diverge from the `+`/`===` path, and the SAME real string pool for bytes.
//!
//! Convention (matching the rest of the `__rtsadp_*` surface): PolyValue words
//! cross as raw `u64`; string args/results cross as real string handles (`u64`); a
//! returned array is a fresh `Entry::Vec` boxed as a `TAG_OBJECT` PolyValue word.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::genops::{to_boolean, to_number};
use super::{PolyValue, abi_adapter, genops};

/// Box a fresh real Vec handle as a `TAG_OBJECT` array PolyValue word (the engine's
/// array representation), matching [`super::arrayops`]'s `slice` result.
fn box_vec_as_array(vec_handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(vec_handle)).raw()
}

/// JS `ToString` of a PolyValue, interned in the REAL pool, returned as a string
/// PolyValue WORD (the same path `__rtsadp_to_string` uses).
fn to_string_word(v: u64) -> u64 {
    genops::__rtsadp_to_string(v)
}

// ===========================================================================
// Global coercion functions — `Number(x)`, `String(x)`, `Boolean(x)`.
// ===========================================================================

/// `setTimeout(cb, ms)` — schedule a FUNCTION value on the runtime's ordered
/// macrotask queue (`GL_TIMERS_SET_TIMEOUT`, pumped by `promise.wait` /
/// `time.sleep_ms` / the post-main drain). The fn WORD's real Entry::Function
/// handle is what the queue stores — the pump detects it and invokes through
/// `INVOKE_AUTO` (bound env + kinds), so a CAPTURING arrow works. Returns the
/// timer id as a number word. A non-function `cb` returns id 0 (no-op).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_timeout(cb_word: u64, ms_word: u64) -> u64 {
    schedule_timer(cb_word, ms_word, false)
}

/// `setInterval(cb, ms)` — like [`__rtsadp_set_timeout`] but periodic.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_interval(cb_word: u64, ms_word: u64) -> u64 {
    schedule_timer(cb_word, ms_word, true)
}

fn schedule_timer(cb_word: u64, ms_word: u64, periodic: bool) -> u64 {
    use rts_runtime::namespaces::globals::timers::instance as rt_timers;
    let v = PolyValue::from_raw(cb_word);
    if !v.is_function() {
        return PolyValue::from_i32(0).raw();
    }
    let real = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let ms = genops::__rtsadp_word_to_abi_i64(ms_word);
    let id = if periodic {
        rt_timers::__RTS_FN_GL_TIMERS_SET_INTERVAL(real, ms)
    } else {
        rt_timers::__RTS_FN_GL_TIMERS_SET_TIMEOUT(real, ms)
    };
    PolyValue::from_f64(id as f64).raw()
}

/// `clearTimeout(id)` / `clearInterval(id)` — cancel by the numeric id.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_clear_timer(id_word: u64) -> u64 {
    use rts_runtime::namespaces::globals::timers::instance as rt_timers;
    let id = genops::__rtsadp_word_to_abi_i64(id_word);
    if id > 0 {
        rt_timers::__RTS_FN_GL_TIMERS_CLEAR_TIMEOUT(id as u64);
    }
    PolyValue::undefined().raw()
}

/// `setImmediate(cb)` — enqueue on the check-phase queue (after microtasks).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_immediate(cb_word: u64) -> u64 {
    use rts_runtime::namespaces::globals::timers::instance as rt_timers;
    let v = PolyValue::from_raw(cb_word);
    if !v.is_function() {
        return PolyValue::from_i32(0).raw();
    }
    let real = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let id = rt_timers::__RTS_FN_GL_TIMERS_SET_IMMEDIATE(real);
    PolyValue::from_f64(id as f64).raw()
}

/// `queueMicrotask(cb)` — enqueue a FUNCTION value on the microtask queue
/// (drained at sync end / after each macrotask). Same handle convention as
/// [`__rtsadp_set_timeout`].
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_queue_microtask(cb_word: u64) -> u64 {
    use rts_runtime::namespaces::globals::text_encoding::instance as rt_micro;
    let v = PolyValue::from_raw(cb_word);
    if v.is_function() {
        let real = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        rt_micro::__RTS_FN_GL_TEXTENC_QUEUE_MICROTASK(real);
    }
    PolyValue::undefined().raw()
}

/// `Number(x)` — JS `ToNumber`, the SAME coercion `genops::to_number` performs
/// (so `Number("42")` → 42, `Number(true)` → 1, `Number("x")` → NaN). The result
/// is re-tightened to int32 when exact-in-range (via [`genops::number_result`]).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_number(a: u64) -> u64 {
    genops::number_result(to_number(PolyValue::from_raw(a))).raw()
}

/// `String(x)` — JS `ToString`, returning a string PolyValue word (real pool).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_string(a: u64) -> u64 {
    to_string_word(a)
}

/// `Boolean(x)` — JS `ToBoolean`, returning a PolyValue bool (the empty-string
/// case resolved on the heap, like the rest of the engine).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_boolean(a: u64) -> u64 {
    PolyValue::bool(to_boolean(PolyValue::from_raw(a))).raw()
}

// ===========================================================================
// `parseInt(s, radix)` / `parseFloat(s)` / `isNaN` / `isFinite`.
// ===========================================================================

/// `parseInt(value, radix)` — JS spec subset: ToString the value, trim leading
/// whitespace, an optional sign, an optional `0x`/`0X` prefix when radix is 16 or
/// 0, then consume the longest run of digits valid in the (defaulted) radix. An
/// empty / no-leading-digit string yields `NaN`. `radix` 0 means "auto" (16 if a
/// `0x` prefix, else 10). This matches Node/Bun for the common decimal/binary/hex
/// fixtures; the full grammar (radix-2..36 with letters past `z`) is covered.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_parse_int(value: u64, radix: u64) -> u64 {
    let s = poly_to_string(value);
    // `radix` is a raw PolyValue WORD — ToNumber it (NaN/non-integer → 0 = auto).
    let r = to_number(PolyValue::from_raw(radix));
    let radix_i = if r.is_finite() { r as i64 } else { 0 };
    let f = parse_int_str(&s, radix_i);
    genops::number_result(f).raw()
}

/// `parseFloat(value)` — ToString, trim leading whitespace, then parse the longest
/// leading run that is a valid JS float literal (`[+-]?(Infinity | digits . digits
/// e±digits)`). Trailing garbage is ignored (`parseFloat("3.14x")` → 3.14). No
/// leading float → `NaN`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_parse_float(value: u64) -> u64 {
    let s = poly_to_string(value);
    genops::number_result(parse_float_str(&s)).raw()
}

/// `isNaN(x)` — JS global: `Number.isNaN(ToNumber(x))`. Returns a PolyValue bool.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_is_nan(a: u64) -> u64 {
    PolyValue::bool(to_number(PolyValue::from_raw(a)).is_nan()).raw()
}

/// `isFinite(x)` — JS global: `ToNumber(x)` is a finite number. Returns a bool.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_is_finite(a: u64) -> u64 {
    PolyValue::bool(to_number(PolyValue::from_raw(a)).is_finite()).raw()
}

/// The number value of `p` WITHOUT coercion: `Some(f64)` iff `p` is a genuine JS
/// number (inline double or tagged int32), else `None`. Backs the `Number.is*`
/// static predicates, which (unlike the global `isNaN`/`isFinite`) do NOT coerce
/// — `Number.isNaN("NaN")` is `false`, not `true`.
#[inline]
fn poly_number(p: PolyValue) -> Option<f64> {
    if p.is_double() || p.is_int32() {
        Some(p.number_as_f64())
    } else {
        None
    }
}

/// `Number.isNaN(x)` — no coercion: `true` iff `x` is the number `NaN`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_num_is_nan(a: u64) -> u64 {
    PolyValue::bool(poly_number(PolyValue::from_raw(a)).is_some_and(|n| n.is_nan())).raw()
}

/// `Number.isFinite(x)` — no coercion: `true` iff `x` is a finite number.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_num_is_finite(a: u64) -> u64 {
    PolyValue::bool(poly_number(PolyValue::from_raw(a)).is_some_and(|n| n.is_finite())).raw()
}

/// `Number.isInteger(x)` — no coercion: `true` iff `x` is a finite integer-valued
/// number.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_num_is_integer(a: u64) -> u64 {
    PolyValue::bool(
        poly_number(PolyValue::from_raw(a)).is_some_and(|n| n.is_finite() && n.fract() == 0.0),
    )
    .raw()
}

/// `Number.isSafeInteger(x)` — no coercion: an integer in `[-(2^53-1), 2^53-1]`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_num_is_safe_integer(a: u64) -> u64 {
    PolyValue::bool(poly_number(PolyValue::from_raw(a)).is_some_and(|n| {
        n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0
    }))
    .raw()
}

// ===========================================================================
// `Array.isArray(x)` — the array-vs-object runtime discriminator.
// ===========================================================================

/// `Array.isArray(x)` — `1` iff `x` is a `TAG_OBJECT` PolyValue that is NOT a
/// runtime OBJECT (an object literal / class instance carries a slot-0 shape-id
/// header — see [`super::inspect::looks_like_object`]). Everything else (numbers,
/// strings, null, functions, plain objects) is `0`. Reuses the SAME discriminator
/// the inspect path uses, so the two never disagree.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_is_array(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    // A real array is an OBJECT word over an `Entry::Vec` that is not a keyed
    // shape. A non-Vec backend instance (`Entry::Rtse` [Date/…], `Entry::Buffer`, …)
    // is NOT an array — the object-and-not-keyed test alone wrongly said `true`
    // for those (a `structuredClone(new Date())` then cloned it as `[]`).
    let is_array = v.is_object() && !super::inspect::looks_like_object(v) && {
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        rts_engine::heap::handles::with_entry(h, |e| {
            matches!(e, Some(rts_engine::heap::handles::Entry::Vec(_)))
        })
    };
    PolyValue::bool(is_array).raw()
}

// ===========================================================================
// `Array.of(...)` / `Array.from(...)` / `new Array(n)` / `Array(n)`.
// ===========================================================================

/// `import.meta` — a fresh `{ url }` object whose `url` is the entry module's
/// `file:` URL, resolved from the PROCESS invocation (`rts run <file>` /
/// `run-new` / an AOT binary's argv[0]). One canonical object shape; the
/// value is real (the absolutized entry path), never a mock.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_import_meta() -> u64 {
    let args: Vec<String> = std::env::args().collect();
    // `rts run <file>` / `rts run-new <file>` → the file arg; an AOT binary →
    // its own argv[0].
    let entry = args
        .iter()
        .position(|a| a == "run" || a == "run-new" || a == "test" || a == "compile")
        .and_then(|i| args.get(i + 1))
        .or_else(|| args.first())
        .cloned()
        .unwrap_or_default();
    let abs = std::fs::canonicalize(&entry)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| entry.replace('\\', "/"));
    let abs = abs.trim_start_matches("//?/").to_string();
    let url = if abs.starts_with('/') {
        format!("file://{abs}")
    } else {
        format!("file:///{abs}")
    };
    let obj = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let shape = crate::shape::intern_global_shape(&["url".to_string()]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(obj, PolyValue::from_i32(shape as i32).raw() as i64);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(
        obj,
        super::abi_adapter::intern_poly(&url).raw() as i64,
    );
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(obj)).raw()
}

/// `Array(n)` / `new Array(n)` — a fresh SPARSE array of length `n`: the slots
/// hold the HOLE singleton (reads map to `undefined`; `i in arr` is `false`;
/// join renders empty — real JS hole semantics). `n` is a real JS number word; a
/// non-integer / negative / out-of-range `n` yields an empty array (the runtime
/// would throw a RangeError — a later increment; an empty array is the safe,
/// never-wrong fallback the lowering only reaches for an integer literal anyway).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_new_sized(n_word: u64) -> u64 {
    let n = to_number(PolyValue::from_raw(n_word));
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if n.is_finite() && n.fract() == 0.0 && n >= 0.0 && n <= (1u64 << 31) as f64 {
        let hole = PolyValue::hole().raw() as i64;
        for _ in 0..(n as i64) {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, hole);
        }
    }
    box_vec_as_array(vec)
}

/// `Array.from(arrayLike)` — supports the two reachable shapes: a STRING (→ a char
/// array, each element a one-char string PolyValue) and an ARRAY (→ a shallow
/// copy). Any other value (Map/Set/iterator/typed-array/array-like-with-length)
/// yields the special sentinel [`FROM_UNSUPPORTED`] so the LOWERING bails (the
/// honesty floor: never a wrong-but-closer value). The lowering checks the result
/// against the sentinel and emits an explicit `Unsupported` when seen.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_from(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    // A level-B typed-array VIEW: materialize the elements through the shared
    // buffer (same as the iteration hook).
    if let Some((bh, bytes, signed, float)) = super::taops::view_parts(a) {
        let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        let n = super::taops::view_len(bh, bytes);
        for i in 0..n {
            let w = super::taops::view_get(bh, bytes, signed, float, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, w as i64);
        }
        return box_vec_as_array(vec);
    }
    if v.is_string() {
        let s = abi_adapter::resolve_poly(v);
        let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for ch in s.chars() {
            let word = abi_adapter::intern_poly(&ch.to_string()).raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
        }
        return box_vec_as_array(vec);
    }
    if v.is_object() && !super::inspect::looks_like_object(v) {
        // A real array → shallow copy of its boxed element words.
        let src = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        // An `Entry::Buffer` source (a `TextEncoder.encode()` result / raw
        // ArrayBuffer bytes): each byte is one element — `VEC_LEN` on a Buffer
        // reads 0 and yielded `[]` (58_text_encoding's `hex=` was empty).
        let buf_bytes: Option<Vec<u8>> = {
            use rts_engine::heap::handles::{Entry, with_entry};
            with_entry(src, |e| match e {
                Some(Entry::Buffer(b)) => Some(b.clone()),
                _ => None,
            })
        };
        if let Some(b) = buf_bytes {
            let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            for byte in b {
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(
                    vec,
                    PolyValue::from_i32(byte as i32).raw() as i64,
                );
            }
            return box_vec_as_array(vec);
        }
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(src).max(0);
        let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for i in 0..len {
            let mut w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(src, i);
            // Array.from DENSIFIES: a sparse HOLE materializes as `undefined`.
            if PolyValue::from_raw(w as u64).is_hole() {
                w = PolyValue::undefined().raw() as i64;
            }
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, w);
        }
        return box_vec_as_array(vec);
    }
    // ARRAY-LIKE keyed object (`Array.from({ length: n })`, `{0:'a', length:1}`):
    // build `[ source[0], …, source[length-1] ]`, an absent index reading
    // `undefined`. REQUIRE a finite non-negative numeric `length` — without it we
    // fall through to the honest FROM_UNSUPPORTED sentinel, so a Map/Set/plain
    // object (no numeric `length`) never silently becomes `[]` (a wrong result).
    if v.is_object() && super::inspect::looks_like_object(v) {
        // `__rtsadp_obj_get` takes the key as a PolyValue WORD (a boxed string), NOT a
        // raw GC handle — `intern_poly(s).raw()` produces it.
        let len_key = abi_adapter::intern_poly("length").raw();
        let len_num = to_number(PolyValue::from_raw(super::objops::__rtsadp_obj_get(a, len_key)));
        if len_num.is_finite() && len_num >= 0.0 {
            let len = len_num as i64;
            let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            for i in 0..len {
                let k = abi_adapter::intern_poly(&i.to_string()).raw();
                let elem = super::objops::__rtsadp_obj_get(a, k);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, elem as i64);
            }
            return box_vec_as_array(vec);
        }
    }
    FROM_UNSUPPORTED
}

/// Sentinel returned by [`__rtsadp_arr_from`] for an unsupported arrayLike (Map/
/// Set/iterator/etc). A raw `u64` that is NOT a valid array word — the lowering
/// compares the result against it and bails. (It is the `empty` singleton word,
/// which never escapes as a user value, so it is an unambiguous marker.)
pub const FROM_UNSUPPORTED: u64 = PolyValue::empty().raw();

// ===========================================================================
// String statics — `String.fromCharCode(...)`, `String.fromCodePoint(...)`.
// ===========================================================================

/// `String.fromCharCode(code)` for ONE code unit — JS truncates to a u16. The
/// lowering calls this once per argument and concatenates the results through the
/// generic `+` (real `STRING_CONCAT`), so the variadic form falls out of the
/// monadic primitive. Returns a string PolyValue WORD. Reuses the runtime's own
/// `__RTS_FN_GL_STRING_FROM_CHAR_CODE` for the byte production (UTF-16→UTF-8
/// lossy, lone-surrogate handling), then boxes its handle as a PolyValue.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_from_char_code(code: u64) -> u64 {
    let n = to_number(PolyValue::from_raw(code)) as i64;
    let handle = rts_runtime::namespaces::globals::string::rt::__RTS_FN_GL_STRING_FROM_CHAR_CODE(n);
    abi_adapter::poly_from_real_handle(handle).raw()
}

/// `String.fromCharCode(...codes)` over a spread ARRAY (`fromCharCode(...arr)`):
/// concatenate `fromCharCode` of each element. `arr_word` is a proven array; each
/// element is `ToNumber`'d to a code unit. Returns a string PolyValue WORD (real
/// pool). The bytes go through the runtime's own `FROM_CHAR_CODE` per element, so
/// surrogate handling matches the scalar path.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_from_char_code_arr(arr_word: u64) -> u64 {
    let v = PolyValue::from_raw(arr_word);
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
    let mut out = String::new();
    for i in 0..len {
        let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
        let code_word = __rtsadp_str_from_char_code(w);
        out.push_str(&abi_adapter::resolve_poly(PolyValue::from_raw(code_word)));
    }
    abi_adapter::intern_poly(&out).raw()
}

/// `String.fromCodePoint(code)` for ONE code point. Same shape as
/// [`__rtsadp_str_from_char_code`] but full Unicode code points.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_from_code_point(code: u64) -> u64 {
    let n = to_number(PolyValue::from_raw(code)) as i64;
    let handle =
        rts_runtime::namespaces::globals::string::rt::__RTS_FN_GL_STRING_FROM_CODE_POINT(n);
    abi_adapter::poly_from_real_handle(handle).raw()
}

// ===========================================================================
// `str.split(sep, limit)` — PolyValue-aware (boxes each substring).
// ===========================================================================

/// `str.split(sepHandle, limit)` — split `recv` (a real string handle) on the
/// string separator `sep` (a real string handle), producing a fresh array
/// (`Entry::Vec`) whose elements are STRING PolyValue WORDS (NOT the runtime's
/// raw-handle convention — so `.length`/`.join`/`[i]` on the result work like any
/// engine array). `limit < 0` means "no limit". An empty separator splits into
/// individual chars (JS spec). The bytes go through the REAL pool. This is the
/// PolyValue-native analogue of `__RTS_FN_GL_STRING_SPLIT` (whose Vec holds raw
/// handles the engine could not interpret).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_split(recv: u64, sep: u64, limit: i64) -> u64 {
    let s = abi_adapter::real_handle_to_string(recv);
    let delim = abi_adapter::real_handle_to_string(sep);
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let take = if limit < 0 {
        usize::MAX
    } else {
        limit as usize
    };
    let push = |part: &str| {
        let word = abi_adapter::intern_poly(part).raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
    };
    if delim.is_empty() {
        // JS: split("") = individual chars (no leading/trailing empties).
        for ch in s.chars().take(take) {
            push(&ch.to_string());
        }
    } else {
        for part in s.split(delim.as_str()).take(take) {
            push(part);
        }
    }
    box_vec_as_array(vec)
}

// ===========================================================================
// Spread support — fold a variadic numeric op over a spread array; append one
// array's elements into another (array-literal spread `[...a, ...b]`).
// ===========================================================================

/// `__rtsadp_math_reduce(arr_word, op)` — fold `Math.min/max/hypot` over the
/// elements of the array `arr_word` (spread `Math.max(...xs)`), returning an f64.
/// `op`: `0` = min, `1` = max, `2` = hypot. Each element is `ToNumber`'d (the same
/// coercion the scalar path uses). An empty array yields the JS identity:
/// `Math.min()` = `+Infinity`, `Math.max()` = `-Infinity`, `Math.hypot()` = `0`.
/// A `NaN` element makes min/max `NaN` (JS NaN-propagation).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_math_reduce(arr_word: u64, op: i64) -> f64 {
    let v = PolyValue::from_raw(arr_word);
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
    if op == 2 {
        // hypot needs ALL elements up front (the scale factor is the max
        // magnitude) — a pairwise `acc.hypot(x)` fold drifted 1 ULP from
        // V8/JSC on `Math.hypot(1, 1, 1)`.
        let mut xs = Vec::with_capacity(len as usize);
        for i in 0..len {
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
            xs.push(to_number(PolyValue::from_raw(w)));
        }
        return hypot_n(&xs);
    }
    let mut acc = match op {
        0 => f64::INFINITY,
        _ => f64::NEG_INFINITY,
    };
    for i in 0..len {
        let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
        let x = to_number(PolyValue::from_raw(w));
        acc = match op {
            0 => acc.min(x),
            _ => acc.max(x),
        };
        // JS min/max propagate NaN (Rust's f64::min/max do NOT — guard).
        if x.is_nan() {
            return f64::NAN;
        }
    }
    acc
}

/// N-ary `Math.hypot` the way V8/JSC compute it: ±Infinity dominates (even over
/// NaN, JS spec), then NaN; otherwise scale every element by the max magnitude,
/// Neumaier-sum the squared ratios, and return `sqrt(sum) * max` — exact in the
/// normal range and immune to overflow/underflow of the naive sum of squares.
fn hypot_n(xs: &[f64]) -> f64 {
    if xs.iter().any(|x| x.is_infinite()) {
        return f64::INFINITY;
    }
    if xs.iter().any(|x| x.is_nan()) {
        return f64::NAN;
    }
    let max = xs.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0f64;
    let mut comp = 0.0f64;
    for &x in xs {
        let r = x / max;
        let sq = r * r;
        let t = sum + sq;
        comp += if sum.abs() >= sq.abs() {
            (sum - t) + sq
        } else {
            (sq - t) + sum
        };
        sum = t;
    }
    (sum + comp).sqrt() * max
}

/// `__rtsadp_canon_double(word)` — `ToNumber` the PolyValue `word` and return it
/// as a GUARANTEED inline-double PolyValue word (never a tagged int32). The
/// front-end uses this to normalize a spread array element to a word the pure
/// `emit_unbox_double` bitcast handles, before coercing to a native numeric param
/// (a boxed-int32 element would otherwise bitcast to a bogus f64). NaN-canonical.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_canon_double(word: u64) -> u64 {
    PolyValue::from_f64(to_number(PolyValue::from_raw(word))).raw()
}

/// `__rtsadp_arr_spread_append(dst_word, src_word)` — push every element of the
/// array `src_word` onto the array `dst_word` (array-literal spread `[...src]`).
/// Reads the raw element WORDS through the real Vec (preserving each boxed
/// PolyValue), so the spread copy is shallow and representation-faithful. A
/// non-array `src` is a no-op (the lowering only routes proven arrays here).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_spread_append(dst_word: u64, src_word: u64) {
    let src = PolyValue::from_raw(src_word);
    if !(src.is_object() && !super::inspect::looks_like_object(src)) {
        return;
    }
    let dst = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(dst_word).as_handle());
    let s = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(src.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(s).max(0);
    for i in 0..len {
        let mut w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(s, i);
        // Spread iterates ([[Get]] per element) and DENSIFIES: a sparse HOLE
        // materializes as `undefined`.
        if PolyValue::from_raw(w as u64).is_hole() {
            w = PolyValue::undefined().raw() as i64;
        }
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(dst, w);
    }
}

// ===========================================================================
// String parsing helpers (parseInt / parseFloat grammar).
// ===========================================================================

/// ToString a PolyValue word for the parse functions (a string stays itself;
/// numbers/etc. go through the engine's ToString — JS `parseInt(42)` parses
/// `"42"`).
fn poly_to_string(v: u64) -> String {
    let pv = PolyValue::from_raw(v);
    if pv.is_string() {
        abi_adapter::resolve_poly(pv)
    } else {
        let s_word = genops::__rtsadp_to_string(v);
        abi_adapter::resolve_poly(PolyValue::from_raw(s_word))
    }
}

/// JS `parseInt(s, radix)` returning the parsed `f64` (`NaN` on failure). `radix`
/// 0 = auto (16 with a `0x` prefix, else 10); 2..=36 fixes the base. Leading
/// whitespace + an optional sign are consumed; the longest valid-digit run is
/// parsed; trailing garbage is ignored.
fn parse_int_str(s: &str, radix: i64) -> f64 {
    let t = s.trim_start();
    let (neg, rest) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let mut base = radix;
    let mut digits = rest;
    if base == 16 || base == 0 {
        if let Some(r) = digits
            .strip_prefix("0x")
            .or_else(|| digits.strip_prefix("0X"))
        {
            digits = r;
            base = 16;
        }
    }
    if base == 0 {
        base = 10;
    }
    if !(2..=36).contains(&base) {
        return f64::NAN;
    }
    let radix_u = base as u32;
    let valid: String = digits
        .chars()
        .take_while(|c| c.to_digit(radix_u).is_some())
        .collect();
    if valid.is_empty() {
        return f64::NAN;
    }
    // Accumulate in f64 to keep large magnitudes (JS parseInt returns a double).
    let mut acc = 0.0f64;
    for c in valid.chars() {
        acc = acc * base as f64 + c.to_digit(radix_u).unwrap() as f64;
    }
    if neg { -acc } else { acc }
}

/// JS `parseFloat(s)` returning the parsed `f64` (`NaN` on no leading float). The
/// longest leading run matching a JS float literal is parsed; trailing garbage is
/// ignored. `Infinity`/`+Infinity`/`-Infinity` are recognized.
fn parse_float_str(s: &str) -> f64 {
    let t = s.trim_start();
    for spell in ["Infinity", "+Infinity"] {
        if t.starts_with(spell) {
            return f64::INFINITY;
        }
    }
    if t.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    let bytes = t.as_bytes();
    let mut end = 0usize;
    // optional sign
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut saw_digit = false;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
        saw_digit = true;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
            saw_digit = true;
        }
    }
    if !saw_digit {
        return f64::NAN;
    }
    // optional exponent
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut e = end + 1;
        if e < bytes.len() && (bytes[e] == b'+' || bytes[e] == b'-') {
            e += 1;
        }
        let mut saw_exp = false;
        while e < bytes.len() && bytes[e].is_ascii_digit() {
            e += 1;
            saw_exp = true;
        }
        if saw_exp {
            end = e;
        }
    }
    t[..end].parse::<f64>().unwrap_or(f64::NAN)
}
