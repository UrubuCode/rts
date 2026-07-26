//! Data-driven method dispatch — the "no builtins in the engine" rule.
//!
//! The engine may name ONLY primordial classes (String/Object/Array/Function/
//! Promise/Boolean/Number/Error+subclasses). A method call `recv.method(args)`
//! resolves through class METADATA — not a per-method `if method == "..."`
//! switchboard — to a REAL runtime `__RTS_FN_*` symbol + its lowered signature,
//! which the lowering then marshals + `call`s. ONE generic emit path.
//!
//! ## Why a hand-written metadata table (and why it is still honest)
//!
//! The real runtime's class metadata lives in a `Registry` built by calling the
//! `register_*_class_spec(&mut Engine)` builders. Those builders, and the
//! `Engine`/`Registry`/`Class` types, are NOT re-exported through the
//! `rts-runtime` FACADE (only `rts_engine::abi::*` is), and the crate-layering
//! rule forbids adding `rts-engine`/`rts-primitives` as a second direct
//! dependency (see `runtime_link.rs` for the same constraint). So — exactly like
//! [`crate::value::abi_sig`] does for the string-pool symbols — this module
//! hand-writes a small static table covering the methods the lowering emits.
//!
//! It is NOT a switchboard and NOT invented symbols: every [`MethodSpec`]
//! references the ACTUAL `__RTS_FN_GL_*` extern the runtime defines, with the
//! ACTUAL `AbiType` signature the real class spec declares (verified against
//! `rts-primitives/src/string|number|array`). [`resolve_method`] is ONE generic
//! lookup keyed by `(class, method, argc)` — the same `(name, arity)` resolution
//! `Class::resolve_instance_method` performs — and the lowering ([`crate::front::run::method`])
//! marshals purely from the returned `params`/`ret` `AbiType`s. Adding a method
//! is a data row, never a code arm. At cutover this table is replaced by a real
//! registry harvest once the facade exposes one; the lowering does not change.

use rts_runtime::abi::AbiType;

/// The class implied by a receiver value, as the engine can prove it statically.
/// Only the receiver kinds the engine can name (primordials) appear here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecvClass {
    /// A `string` primitive (a `TAG_STR` PolyValue): receiver marshals as a real
    /// string handle (`POLY_TO_HANDLE`).
    String,
    /// A `number` primitive (an int32 / double): receiver marshals as `F64`.
    Number,
    /// An array (a `TAG_OBJECT` PolyValue over a real `Entry::Vec` of PolyValue
    /// words): the receiver marshals as the Vec's real handle (`POLY_TO_HANDLE`),
    /// and methods resolve to the codegen-owned `__rtsadp_arr_*` trampolines that
    /// interpret each slot as a PolyValue (see [`crate::value::arrayops`]).
    Array,
}

/// How the receiver value is passed to the real symbol at slot 0.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecvAbi {
    /// Slot 0 is the receiver's real GC handle (string/object), one `i64` slot.
    Handle,
    /// Slot 0 is the receiver coerced to `f64` (a Number primitive), one `f64`.
    F64,
    /// Slot 0 is the array's real `Entry::Vec` handle: the array PolyValue word
    /// is `POLY_TO_HANDLE`'d (same as `Handle`), one `i64` slot. Distinct from
    /// `Handle` only so the lowering knows the receiver is an array word, not a
    /// string word.
    ArrayVec,
}

/// A resolved instance-method target: the real runtime symbol + the explicit
/// (non-receiver) argument `AbiType`s + the return `AbiType`. The lowering reads
/// these to marshal each PolyValue arg and the result. The receiver convention is
/// `recv_abi`; the symbol's slot 0 is always the receiver.
#[derive(Clone, Copy, Debug)]
pub struct MethodSpec {
    /// The real `__RTS_FN_*` extern symbol the lowering `call`s.
    pub symbol: &'static str,
    /// How slot 0 (the receiver) is marshaled.
    pub recv_abi: RecvAbi,
    /// The explicit argument `AbiType`s, in order (slots after the receiver).
    /// A method with optional trailing args is registered once per supported
    /// arity (so `argc` matches a row exactly — no default injection here).
    pub args: &'static [AbiType],
    /// The return `AbiType` (`Handle` ⇒ a string/object handle the lowering
    /// re-boxes; `I64`/`F64` ⇒ a number; `Bool` ⇒ a boolean singleton).
    pub ret: AbiType,
    /// The callback shape, for Array methods that take a callback function VALUE
    /// (`map`/`filter`/`reduce`/…). `None` for every non-callback method — those
    /// marshal `args` literally. See [`CbShape`].
    pub cb: Option<CbShape>,
    /// Whether the return value is an ARRAY word (vs an element/scalar). `ret` alone
    /// is ambiguous: a `U64` return is a PolyValue word that may be a whole array
    /// (`slice`/`map`/`toSorted`) OR a single element (`at`/`pop`/`shift`). This bit
    /// is the SOURCE OF TRUTH for "chaining `.length`/`.join()` on the result is an
    /// array op" — `is_array_returning_method` reads it instead of a parallel list.
    pub ret_is_array: bool,
}

/// The callback marshaling shape of an Array callback method (P4.7). The lowering
/// reifies the (single) callback argument to a `TAG_FUNCTION` PolyValue word and
/// passes it after the receiver; `Reduce` additionally appends the optional
/// initial-accumulator word + a `has_init` flag.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CbShape {
    /// `cb(element, index, array)` predicate/map family: trampoline takes
    /// `(vec_handle, cb_word)`. The user call has exactly one (callback) arg.
    Predicate,
    /// `reduce(cb, init?)`: trampoline takes `(vec_handle, cb_word, init_word,
    /// has_init)`. The user call has the callback + an OPTIONAL initial value.
    Reduce,
}

/// Whether `method` (at ANY registered arity) returns an ARRAY word — the SOURCE
/// OF TRUTH for "chaining on the result is an array op". Reads `ret_is_array` off
/// the `ARRAY_ROWS` registrar so there is no parallel hardcoded list to drift.
///
/// `splice`/`toSpliced` are handled by the variadic front path BEFORE `resolve_method`
/// (they never reach `ARRAY_ROWS`), so they are listed here explicitly — these are
/// methods DELIBERATELY absent from the registrar, not a duplicated registrar entry.
/// (`push`/`unshift` are also front-intercepted but return a LENGTH, not an array, so
/// they are correctly NOT here; `concat` keeps a 1-arg `ARRAY_ROWS` row with
/// `ret_is_array`, so it is covered by the table scan above.)
pub fn array_method_returns_array(method: &str) -> bool {
    if matches!(method, "splice" | "toSpliced") {
        return true;
    }
    // `s.match(re)` / `re.exec(s)` results are RegExpExecArray-shaped Map rows
    // for a NON-global regex (numeric slots + index/input/groups) and a plain
    // array only for a global one - the static Array shape is UNSAFE for the
    // receiver-class inference (a Map row read through VEC_GET yields garbage).
    // `matchAll` rows are Maps too. Keep them DYNAMIC.
    if matches!(method, "match" | "matchAll") {
        return false;
    }
    // Includes the STRING rows too: `s.match(p)`/`s.matchAll(p)`/`s.split(sep)`
    // yield an array, so chaining `.join()`/`.length` on the result is an array
    // dispatch (the caller's receiver-class inference reads this bit).
    ARRAY_ROWS
        .iter()
        .chain(STRING_ROWS.iter())
        .any(|(name, _, spec)| *name == method && spec.ret_is_array)
}

/// Resolve `recv_class.method(argc explicit args)` to a real [`MethodSpec`], or
/// `None` when the metadata has no matching `(method, arity)` row. ONE generic
/// path: a linear scan of the class's static method table for a name + exact
/// explicit-arity match (mirroring `Class::resolve_instance_method`'s exact-arity
/// rule). `None` makes the lowering BAIL explicitly — never a guess.
pub fn resolve_method(recv_class: RecvClass, method: &str, argc: usize) -> Option<MethodSpec> {
    let rows: &[(&'static str, usize, MethodSpec)] = match recv_class {
        RecvClass::String => STRING_ROWS,
        RecvClass::Number => NUMBER_ROWS,
        RecvClass::Array => ARRAY_ROWS,
    };
    rows.iter()
        .find(|(name, arity, _)| *name == method && *arity == argc)
        .map(|(_, _, spec)| *spec)
}

// ===========================================================================
// String — instance methods (receiver = real string handle, slot 0).
//
// Symbols + signatures verified against rts-primitives/src/string/{mod,rt}.rs
// (register_string_class_spec). `Handle` args are real string handles; `I64`
// args are indices/counts; returns are `Handle` (string) / `I64` / `Bool`.
// Methods with optional trailing args are listed once per supported arity.
// ===========================================================================

use AbiType::{Bool, Handle, I64, U64};

/// A String instance-method row: `(jsName, explicitArity, spec)`. Receiver is a
/// real string handle. Listed once per supported arity (an arity a real default
/// covers is listed explicitly so `argc` matches without default injection).
///
/// DRAINED: the entire non-regex String method surface (`toUpperCase`/
/// `toLowerCase`/`trim*`/`charAt`/`charCodeAt`/`codePointAt`/`at`/`repeat`/`slice`/
/// `substring`/`substr`/`indexOf`/`lastIndexOf`/`includes`/`startsWith`/`endsWith`/
/// `padStart`/`padEnd`/`concat`/`replace`/`replaceAll`/`localeCompare`) now lives
/// in the primordial `String` value-class (`rts-primitives/src/string/value_class.rs`,
/// `#[rtse::class("String", value)]`), routed via
/// `method::try_primitive_class_method(.., "String", ..)` BEFORE this table — each
/// method computes directly via `strops`, no `__RTS_FN_GL_STRING_*` extern.
///
/// KEPT here: only the regex-argument family, whose runtime string-vs-regex
/// dispatch the value-class macro cannot yet express — `match`/`matchAll` (return a
/// Vec handle) + `search`. `split` (returns an ARRAY) + the regex-LITERAL first-arg
/// form stay on `method::try_string_special`/`try_string_regex_method` (run before
/// the value-class).
const STRING_ROWS: &[(&str, usize, MethodSpec)] = &[
    // ---- match/search/matchAll with a STRING pattern arg: the `_AUTO` symbols
    // dispatch string-vs-regex at runtime (Entry::Regex probe), so ONE row per
    // method covers both; the regex-LITERAL first-arg form short-circuits earlier
    // in `try_string_regex_method`. `match`/`matchAll` return a Vec handle (0 ⇒
    // JS `null`); `search` a number. ----
    (
        "match",
        1,
        sm_arr("__RTS_FN_NS_STRING_MATCH_AUTO", &[Handle], Handle),
    ),
    (
        "search",
        1,
        sm("__RTS_FN_NS_STRING_SEARCH_AUTO", &[Handle], I64),
    ),
    (
        "matchAll",
        1,
        sm_arr("__RTS_FN_NS_STRING_MATCH_ALL_AUTO", &[Handle], Handle),
    ),
];

// ===========================================================================
// Number — instance methods (receiver = the f64 primitive, slot 0).
//
// DRAINED: the Number method surface (`toFixed`/`toPrecision`/`toExponential`/
// `toString(radix)` + `valueOf`/`toLocaleString`) now lives in the prelude `.ts`
// `class Number` (`rts-primitives/src/number.ts`), routed via
// `method::try_primitive_class_method(.., "Number", ..)` BEFORE this table. The
// `.ts` bodies call the irreducible Rust formatters through the private
// `engine.num_*` bridge (one source of truth) — so the engine no longer carries a
// hardcoded `__RTS_FN_GL_NUMBER_*` row per method here. The table is kept (empty)
// so a numeric receiver with a method the `.ts` class does NOT cover still BAILS
// explicitly (`resolve_method` → `None`), never a guess. The `__RTS_FN_GL_NUMBER_*`
// formatters + `register_number_class_spec` stay for the frozen old engine + the
// `new Number(x)` wrapper path.
// ===========================================================================

/// Number instance-method rows. EMPTY — drained to the prelude `.ts` `class
/// Number` (see the comment above). Kept as a table so a not-yet-covered method on
/// a numeric receiver resolves to `None` (an explicit bail) rather than panicking.
const NUMBER_ROWS: &[(&str, usize, MethodSpec)] = &[];

// ===========================================================================
// Array — instance methods WITHOUT callbacks (receiver = the array's real Vec
// handle, slot 0). These resolve to the codegen-owned `__rtsadp_arr_*`
// trampolines (NOT `__RTS_FN_*`): the runtime's own Array methods read raw i64
// slots, but the new engine stores boxed PolyValue WORDS, so element
// interpretation is the engine's own (see `crate::value::arrayops`).
//
// Arg/return ABI convention (see `crate::value::abi_sig`):
// - `U64` arg  = a raw PolyValue WORD (the boxed value, passed verbatim) — used
//   for `indexOf`/`includes`/`push` so element equality matches storage.
// - `I64` arg  = an index/range bound, unboxed to i64.
// - `Handle` arg = a real string handle (the `join` separator).
// - `U64` ret  = a raw PolyValue word (`at`/`pop`) or a TAG_OBJECT array word
//   (`slice`); `Handle` ret = a string handle (`join`); `I64`/`Bool` as usual.
// Callback methods (.map/.filter/.reduce/.forEach/.sort-with-cmp) are NOT here —
// they need function VALUES and BAIL at the lowering.
// ===========================================================================

/// An Array instance-method row. Receiver is the array's real Vec handle.
const ARRAY_ROWS: &[(&str, usize, MethodSpec)] = &[
    ("indexOf", 1, am("__rtsadp_arr_index_of", &[U64], I64)),
    ("includes", 1, am("__rtsadp_arr_includes", &[U64], Bool)),
    ("at", 1, am("__rtsadp_arr_at_w", &[U64], U64)),
    ("join", 0, am("__rtsadp_arr_join0", &[], Handle)),
    ("join", 1, am("__rtsadp_arr_join", &[Handle], Handle)),
    ("push", 1, am("__rtsadp_arr_push", &[U64], I64)),
    ("pop", 0, am("__rtsadp_arr_pop", &[], U64)),
    ("slice", 2, am_arr("__rtsadp_arr_slice", &[I64, I64], U64)),
    // ---- P5.2 non-callback methods over boxed PolyValue words ----
    (
        "lastIndexOf",
        1,
        am("__rtsadp_arr_last_index_of", &[U64], I64),
    ),
    ("reverse", 0, am_arr("__rtsadp_arr_reverse", &[], U64)),
    ("fill", 1, am_arr("__rtsadp_arr_fill", &[U64], U64)),
    ("fill", 2, am_arr("__rtsadp_arr_fill2", &[U64, I64], U64)),
    ("fill", 3, am_arr("__rtsadp_arr_fill3", &[U64, I64, I64], U64)),
    ("copyWithin", 1, am_arr("__rtsadp_arr_copy_within1", &[I64], U64)),
    ("concat", 1, am_arr("__rtsadp_arr_concat", &[U64], U64)),
    ("flat", 0, am_arr("__rtsadp_arr_flat", &[], U64)),
    ("flat", 1, am_arr("__rtsadp_arr_flat_depth", &[I64], U64)),
    ("shift", 0, am("__rtsadp_arr_shift", &[], U64)),
    ("unshift", 1, am("__rtsadp_arr_unshift", &[U64], I64)),
    // ---- arity variants of the search/slice methods (the `fromIndex` / 1-arg
    //      forms) + the ES2023 non-mutating copies + default sort ----
    ("indexOf", 2, am("__rtsadp_arr_index_of_from", &[U64, I64], I64)),
    ("includes", 2, am("__rtsadp_arr_includes_from", &[U64, I64], Bool)),
    (
        "lastIndexOf",
        2,
        am("__rtsadp_arr_last_index_of_from", &[U64, I64], I64),
    ),
    ("slice", 1, am_arr("__rtsadp_arr_slice1", &[I64], U64)),
    ("slice", 0, am_arr("__rtsadp_arr_slice0", &[], U64)),
    ("toString", 0, am("__rtsadp_arr_to_string", &[], Handle)),
    // toLocaleString without locale data == toString (the runtime has no Intl).
    ("toLocaleString", 0, am("__rtsadp_arr_to_string", &[], Handle)),
    ("toReversed", 0, am_arr("__rtsadp_arr_to_reversed", &[], U64)),
    // entries/keys/values: materialized arrays (for-of consumes them like JS's
    // iterators for the dominant shapes; the lazy protocol is a later increment).
    ("entries", 0, am_arr("__rtsadp_arr_entries", &[], U64)),
    ("keys", 0, am_arr("__rtsadp_arr_keys", &[], U64)),
    ("values", 0, am_arr("__rtsadp_arr_values", &[], U64)),
    ("with", 2, am_arr("__rtsadp_arr_with", &[I64, U64], U64)),
    ("sort", 0, am_arr("__rtsadp_arr_sort", &[], U64)),
    // sort(cmp): a 1-callback method. The CbShape is Predicate only to satisfy the
    // "exactly one callback arg" marshaling; the `__rtsadp_arr_sort_cmp` trampoline
    // invokes the cb as a 2-arg comparator `(a, b)`, not as `(elem, i, arr)`.
    ("sort", 1, cm("__rtsadp_arr_sort_cmp", U64, CbShape::Predicate)),
    ("toSorted", 0, am_arr("__rtsadp_arr_to_sorted", &[], U64)),
    // toSorted(cmp): like sort(cmp) but non-mutating (a new array). 1-callback
    // method; the trampoline invokes the cb as a 2-arg comparator.
    ("toSorted", 1, cm_arr("__rtsadp_arr_to_sorted_cmp", U64, CbShape::Predicate)),
    ("toSpliced", 2, am_arr("__rtsadp_arr_to_spliced", &[I64, I64], U64)),
    ("copyWithin", 2, am_arr("__rtsadp_arr_copy_within2", &[I64, I64], U64)),
    (
        "copyWithin",
        3,
        am_arr("__rtsadp_arr_copy_within", &[I64, I64, I64], U64),
    ),
    // ---- callback methods (P4.7): exactly one callback arg (`cb(elem, i, arr)`)
    // for the predicate/map family; `reduce` has the callback + an OPTIONAL init.
    // The lowering reifies the callback to a TAG_FUNCTION word (see CbShape).
    ("map", 1, cm_arr("__rtsadp_arr_map", U64, CbShape::Predicate)),
    (
        "filter",
        1,
        cm_arr("__rtsadp_arr_filter", U64, CbShape::Predicate),
    ),
    (
        "forEach",
        1,
        cm("__rtsadp_arr_for_each", U64, CbShape::Predicate),
    ),
    ("find", 1, cm("__rtsadp_arr_find", U64, CbShape::Predicate)),
    (
        "findIndex",
        1,
        cm("__rtsadp_arr_find_index", I64, CbShape::Predicate),
    ),
    ("some", 1, cm("__rtsadp_arr_some", Bool, CbShape::Predicate)),
    (
        "every",
        1,
        cm("__rtsadp_arr_every", Bool, CbShape::Predicate),
    ),
    // reduce: argc 1 (no init) and argc 2 (with init) both resolve to the same
    // trampoline; the lowering supplies init_word + has_init.
    ("reduce", 1, cm("__rtsadp_arr_reduce", U64, CbShape::Reduce)),
    ("reduce", 2, cm("__rtsadp_arr_reduce", U64, CbShape::Reduce)),
    // ES2023 / additional callback methods
    (
        "findLast",
        1,
        cm("__rtsadp_arr_find_last", U64, CbShape::Predicate),
    ),
    (
        "findLastIndex",
        1,
        cm("__rtsadp_arr_find_last_index", I64, CbShape::Predicate),
    ),
    (
        "flatMap",
        1,
        cm_arr("__rtsadp_arr_flat_map", U64, CbShape::Predicate),
    ),
    (
        "reduceRight",
        1,
        cm("__rtsadp_arr_reduce_right", U64, CbShape::Reduce),
    ),
    (
        "reduceRight",
        2,
        cm("__rtsadp_arr_reduce_right", U64, CbShape::Reduce),
    ),
];

/// Build a String instance-method spec (receiver = real string handle).
const fn sm(symbol: &'static str, args: &'static [AbiType], ret: AbiType) -> MethodSpec {
    MethodSpec {
        symbol,
        recv_abi: RecvAbi::Handle,
        args,
        ret,
        cb: None,
        ret_is_array: false,
    }
}

/// Like [`sm`] but the result IS an array word (`match`/`matchAll` — the AUTO
/// dispatchers return a Vec-of-strings handle, or 0 for `null`).
const fn sm_arr(symbol: &'static str, args: &'static [AbiType], ret: AbiType) -> MethodSpec {
    MethodSpec {
        ret_is_array: true,
        ..sm(symbol, args, ret)
    }
}

/// Build an Array instance-method spec (receiver = the array's real Vec handle)
/// whose result is NOT an array (an element/scalar — `at`/`pop`/`indexOf`/…).
const fn am(symbol: &'static str, args: &'static [AbiType], ret: AbiType) -> MethodSpec {
    MethodSpec {
        symbol,
        recv_abi: RecvAbi::ArrayVec,
        args,
        ret,
        cb: None,
        ret_is_array: false,
    }
}

/// Like [`am`] but the result IS an array word (`slice`/`reverse`/`toSorted`/…) —
/// so chaining `.length`/`.join()` on it is an array op.
const fn am_arr(symbol: &'static str, args: &'static [AbiType], ret: AbiType) -> MethodSpec {
    MethodSpec {
        ret_is_array: true,
        ..am(symbol, args, ret)
    }
}

/// Build an Array CALLBACK method spec (P4.7): receiver = the array's real Vec
/// handle, `args` is empty (the callback + reduce's init are synthesized by the
/// lowering from `cb`), `ret` is the trampoline's return ABI. `ret_array` flags the
/// callback methods whose result is a NEW array (`map`/`filter`/`flatMap`/sorted).
const fn cm(symbol: &'static str, ret: AbiType, cb: CbShape) -> MethodSpec {
    cm_r(symbol, ret, cb, false)
}

const fn cm_arr(symbol: &'static str, ret: AbiType, cb: CbShape) -> MethodSpec {
    cm_r(symbol, ret, cb, true)
}

const fn cm_r(symbol: &'static str, ret: AbiType, cb: CbShape, ret_is_array: bool) -> MethodSpec {
    MethodSpec {
        symbol,
        recv_abi: RecvAbi::ArrayVec,
        args: &[],
        ret,
        cb: Some(cb),
        ret_is_array,
    }
}

/// Whether `method` is an Array.prototype member (any dispatch row) — the
/// `in` operator's array proto probe (`"push" in []`). Data off the SAME rows
/// the method dispatch reads.
pub fn array_has_method(method: &str) -> bool {
    ARRAY_ROWS.iter().any(|(name, _, _)| *name == method)
}
