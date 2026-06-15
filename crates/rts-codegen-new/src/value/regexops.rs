//! Codegen-owned RegExp + string-regex-method trampolines (P5.12) — PolyValue-native.
//!
//! `RegExp` is a RUNTIME/Registry class — the engine does NOT name it directly
//! (PRIMORDIAL-vs-Registry doctrine). Like [`super::mapset`] (Map/Set) these
//! `__rtsadp_re_*` trampolines bridge the engine's [`PolyValue`] value model to the
//! REAL runtime `__RTS_FN_NS_REGEX_*` symbols (`rts-shared regex`), so:
//!
//! - a RegExp INSTANCE is a `TAG_OBJECT` [`PolyValue`] over the REAL regex handle
//!   (an `Entry::Regex`), tagged class `"RegExp"` for method dispatch;
//! - a regex LITERAL `/pat/flags` and `new RegExp(pat, flags)` both lower to
//!   [`__rtsadp_re_compile`], which calls the REAL `REGEX_COMPILE` (so the `regex`
//!   crate / fancy_regex fallback decides all matching semantics — never a
//!   reimplementation);
//! - `.test` / `.search` / `.replace` / `.replaceAll` return scalars/strings via
//!   the REAL externs directly; `.match` / `.split` return ARRAYS, built
//!   codegen-side over boxed PolyValue words from the REAL
//!   `REGEX_MATCH_ALL`/`REGEX_SPLIT` Vec-of-string-handles result (exactly the
//!   P5.2 `__rtsadp_str_split` rebox pattern — the runtime stores RAW string
//!   handles, the engine needs PolyValue words).
//!
//! JS-vs-Rust regex caveats (the backend is the `regex` crate / RE2, NOT V8):
//! `\d`/`\w`/`\b` are Unicode-aware (broadly compatible); lookaround/backrefs go
//! through the `fancy_regex` fallback (#1107); named groups and `\1` backrefs in
//! the *replacement* are NOT translated. The divergent forms (a function
//! replacer, capture-group `.exec` extraction) BAIL at the lowering — never a
//! wrong value.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::globals::regexp as rt_regexp;
use rts_runtime::namespaces::regex as rt_re;

use super::{PolyValue, abi_adapter, genops};

// ── handle <-> PolyValue helpers ────────────────────────────────────────────

/// Box a real regex runtime handle as a `TAG_OBJECT` PolyValue word (the RegExp
/// instance representation — same boxing as Map/Set/array objects).
fn box_re(handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle)).raw()
}

/// The real regex runtime handle behind a `TAG_OBJECT` RegExp instance word.
fn unbox_re(word: u64) -> u64 {
    rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(word).as_handle())
}

/// The real string handle behind a string PolyValue word.
fn str_handle(word: u64) -> u64 {
    abi_adapter::real_handle_of(PolyValue::from_raw(word))
}

/// Box a real string handle (`0` ⇒ null) as a PolyValue word.
fn box_str_or_null(handle: u64) -> u64 {
    if handle == 0 {
        PolyValue::null().raw()
    } else {
        abi_adapter::poly_from_real_handle(handle).raw()
    }
}

/// Box a Vec handle of RAW string handles into a fresh `Entry::Vec` of PolyValue
/// WORDS (the new engine's array representation), returning the array word. Mirror
/// of the P5.2 `__rtsadp_str_split` rebox: the runtime returns a Vec whose slots
/// are raw string handles; each must become a boxed `TAG_STR` PolyValue word.
fn rebox_string_vec_as_array(raw_vec: u64) -> u64 {
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(raw_vec).max(0);
    for i in 0..len {
        let raw_str = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(raw_vec, i) as u64;
        let word = abi_adapter::poly_from_real_handle(raw_str).raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, word);
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out)).raw()
}

/// Read the UTF-8 text of a string handle (for the extern's `(ptr,len)` ABI).
fn handle_str(h: u64) -> String {
    abi_adapter::real_handle_to_string(h)
}

// ===========================================================================
// Compile (regex literal `/pat/flags` + `new RegExp(pat, flags)`).
// ===========================================================================

/// Compile `pattern` with `flags` (both string PolyValue words) into a RegExp
/// instance word. Calls the REAL `REGEX_COMPILE` (RE2, fancy fallback). A compile
/// error yields `0` from the runtime; we box it as an object word anyway so a
/// `.test` on it simply never matches (the runtime's `with_engine` default).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_compile(pat_word: u64, flags_word: u64) -> u64 {
    let pat = handle_str(str_handle(pat_word));
    let flags = handle_str(str_handle(flags_word));
    let h = rt_re::__RTS_FN_NS_REGEX_COMPILE(
        pat.as_ptr(),
        pat.len() as i64,
        flags.as_ptr(),
        flags.len() as i64,
    );
    box_re(h)
}

// ===========================================================================
// RegExp instance methods / properties.
// ===========================================================================

/// `re.test(s)` — a PolyValue bool word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_test(re_word: u64, subj_word: u64) -> u64 {
    let s = handle_str(str_handle(subj_word));
    let yes = rt_re::__RTS_FN_NS_REGEX_TEST(unbox_re(re_word), s.as_ptr(), s.len() as i64) != 0;
    PolyValue::bool(yes).raw()
}

/// `re.source` — the pattern string as a PolyValue string word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_source(re_word: u64) -> u64 {
    let h = rt_regexp::__RTS_FN_GL_REGEXP_SOURCE(unbox_re(re_word));
    box_str_or_null(h)
}

/// `re.flags` — the canonical flags string as a PolyValue string word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_flags(re_word: u64) -> u64 {
    let h = rt_regexp::__RTS_FN_GL_REGEXP_FLAGS(unbox_re(re_word));
    box_str_or_null(h)
}

/// `re.global` — a PolyValue bool word (flag 'g' set?).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_global(re_word: u64) -> u64 {
    let yes = rt_regexp::__RTS_FN_GL_REGEXP_GLOBAL(unbox_re(re_word)) != 0;
    PolyValue::bool(yes).raw()
}

/// `re.ignoreCase` — a PolyValue bool word (flag 'i' set?).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_ignore_case(re_word: u64) -> u64 {
    let yes = rt_regexp::__RTS_FN_GL_REGEXP_IGNORE_CASE(unbox_re(re_word)) != 0;
    PolyValue::bool(yes).raw()
}

/// `re.multiline` — a PolyValue bool word (flag 'm' set?).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_multiline(re_word: u64) -> u64 {
    let yes = rt_regexp::__RTS_FN_GL_REGEXP_MULTILINE(unbox_re(re_word)) != 0;
    PolyValue::bool(yes).raw()
}

/// `re.lastIndex` — a PolyValue number word (the regex's current lastIndex).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_last_index(re_word: u64) -> u64 {
    let n = rt_regexp::__RTS_FN_GL_REGEXP_LAST_INDEX_GET(unbox_re(re_word));
    genops::number_result(n as f64).raw()
}

// ===========================================================================
// String-with-regex methods (s.method(regex)).
// ===========================================================================

/// `s.match(re)` — for a NON-global regex JS returns `[match]` (just the whole
/// match here — capture-group extraction BAILS at the lowering); for a GLOBAL
/// regex it returns every match. We always return ALL matches as an array (the
/// non-global single-match case is the 1-element array, whose `[0]` is the match —
/// the same first element JS yields). `null` (PolyValue) on no match.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_match(subj_word: u64, re_word: u64) -> u64 {
    let s = handle_str(str_handle(subj_word));
    let raw_vec = rt_re::__RTS_FN_NS_REGEX_MATCH_ALL(unbox_re(re_word), s.as_ptr(), s.len() as i64);
    if raw_vec == 0 {
        return PolyValue::null().raw();
    }
    rebox_string_vec_as_array(raw_vec)
}

/// `s.replace(re, repl)` — replace the FIRST match (a non-global regex) with the
/// literal replacement string. Returns a PolyValue string word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_replace(subj_word: u64, re_word: u64, repl_word: u64) -> u64 {
    let s = handle_str(str_handle(subj_word));
    let repl = handle_str(str_handle(repl_word));
    let h = rt_re::__RTS_FN_NS_REGEX_REPLACE(
        unbox_re(re_word),
        s.as_ptr(),
        s.len() as i64,
        repl.as_ptr(),
        repl.len() as i64,
    );
    box_str_or_null(h)
}

/// `s.replace(re/g, repl)` / `s.replaceAll(re, repl)` — replace ALL matches.
/// JS `s.replace` with a global regex replaces all; the lowering routes a global
/// regex here. Returns a PolyValue string word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_replace_all(subj_word: u64, re_word: u64, repl_word: u64) -> u64 {
    let s = handle_str(str_handle(subj_word));
    let repl = handle_str(str_handle(repl_word));
    let h = rt_re::__RTS_FN_NS_REGEX_REPLACE_ALL(
        unbox_re(re_word),
        s.as_ptr(),
        s.len() as i64,
        repl.as_ptr(),
        repl.len() as i64,
    );
    box_str_or_null(h)
}

/// `s.split(re)` — split the subject on each regex match; an array of boxed string
/// words (built codegen-side from the REAL `REGEX_SPLIT` Vec-of-string-handles).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_split(subj_word: u64, re_word: u64) -> u64 {
    let s = handle_str(str_handle(subj_word));
    let raw_vec = rt_re::__RTS_FN_NS_REGEX_SPLIT(unbox_re(re_word), s.as_ptr(), s.len() as i64);
    if raw_vec == 0 {
        // Bad handle: JS `split` of a string with no match yields `[s]`. Build it.
        let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, subj_word as i64);
        return PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out))
            .raw();
    }
    rebox_string_vec_as_array(raw_vec)
}

/// `s.search(re)` — the byte index of the first match, or `-1`. A PolyValue number
/// word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_search(subj_word: u64, re_word: u64) -> u64 {
    let s = handle_str(str_handle(subj_word));
    let idx = rt_re::__RTS_FN_NS_REGEX_FIND_AT(unbox_re(re_word), s.as_ptr(), s.len() as i64);
    genops::number_result(idx as f64).raw()
}
