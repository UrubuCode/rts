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
    // NON-global `s.match(re)` is spec'd as the SAME RegExpExecArray `exec`
    // yields (numeric slots + index/input/groups) — route the exec builder. A
    // GLOBAL regex returns the plain array of all full matches (no props).
    let is_global = rts_engine::heap::handles::with_entry(unbox_re(re_word), |e| match e {
        Some(rts_engine::heap::handles::Entry::Regex(rx)) => rx.global,
        _ => false,
    });
    if !is_global {
        return __rtsadp_re_exec(re_word, subj_word);
    }
    let s = handle_str(str_handle(subj_word));
    let raw_vec =
        rt_re::__RTS_FN_NS_REGEX_MATCH_GROUPS(unbox_re(re_word), s.as_ptr(), s.len() as i64);
    if raw_vec == 0 {
        return PolyValue::null().raw();
    }
    rebox_string_vec_as_array(raw_vec)
}

/// `re.exec(s)` — JS spec: a RegExpExecArray for the FIRST match, or `null`.
/// Built as the SAME array-like `Entry::Map` rows `matchAll` produces
/// (numeric "0".."N" slots + `length`/`index`/`input`/`groups`), so
/// `m[1]`/`m.index`/`m.groups.name` all read through the dynamic Map paths.
/// (Global-regex `lastIndex` statefulness is the separate lastindex cluster.)
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_exec(re_word: u64, subj_word: u64) -> u64 {
    use rts_runtime::namespaces::globals::string::search as rt_search;
    let s = handle_str(str_handle(subj_word));
    let rows = rt_search::match_all_regex(s.as_ptr(), s.len() as i64, unbox_re(re_word));
    let first = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(rows, 0);
    if first == 0 {
        return PolyValue::null().raw();
    }
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(first as u64)).raw()
}

/// `s.replace(re, repl)` / `s.replaceAll(re, repl)` — the ONE polymorphic
/// replacement trampoline (replaced the pair of literal-string trampolines that
/// delegated to the regex crate's own `$`-semantics): the replacement is
/// tag-dispatched at RUNTIME — a STRING runs the JS `GetSubstitution` token
/// expansion per match ($$, $&, $`, $', $n, $<name>); a FUNCTION is invoked per
/// match with `(match, cap1.., offset, string)` and its ToString spliced; any
/// other value ToStrings once. `all != 0` replaces every match (global regex /
/// replaceAll); else only the first. Offsets are UTF-16 code-unit indices (JS),
/// converted from the engine's byte offsets. Match data is SNAPSHOTTED before
/// any callback runs — no entry lock across invokes.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_replace_fn(
    subj_word: u64,
    re_word: u64,
    fn_word: u64,
    all: i64,
) -> u64 {
    use rts_engine::heap::handles::{Entry, with_entry};
    let s = handle_str(str_handle(subj_word));
    let re_h = unbox_re(re_word);
    // Snapshot: byte range of group 0 + every group's text (None = unmatched)
    // + the declared group NAMES — no entry lock across callback invokes.
    let (matches, names): (Vec<(usize, usize, Vec<Option<String>>)>, Vec<Option<String>>) =
        with_entry(re_h, |e| match e {
            Some(Entry::Regex(rx)) => (
                rx.engine
                    .captures_all(&s)
                    .into_iter()
                    .filter_map(|caps| {
                        let m0 = caps.groups.first().cloned().flatten()?;
                        let texts =
                            caps.groups.iter().map(|g| g.clone().map(|m| m.text)).collect();
                        Some((m0.start, m0.end, texts))
                    })
                    .collect(),
                rx.engine.capture_names(),
            ),
            _ => (Vec::new(), Vec::new()),
        });
    let is_fn = PolyValue::from_raw(fn_word).is_function();
    // A non-function replacement: ToString once, then the JS `GetSubstitution`
    // token expansion runs per match ($$, $&, $`, $', $n, $<name>).
    let plain: Option<String> = if is_fn {
        None
    } else {
        Some(genops::to_string(PolyValue::from_raw(fn_word)))
    };
    let mut out = String::new();
    let mut last = 0usize;
    for (start, end, groups) in matches {
        out.push_str(&s[last..start]);
        if let Some(p) = &plain {
            out.push_str(&js_substitution(p, &s, start, end, &groups, &names));
        } else {
            // JS arg layout: (match, cap1..capN, offset, string) — offset in
            // UTF-16 units. The uniform invoke carries 4 positional slots; a
            // callback declaring more params reads `undefined` for the tail.
            let undef = PolyValue::undefined().raw();
            let mut words: Vec<u64> = Vec::with_capacity(groups.len() + 2);
            for g in &groups {
                words.push(match g {
                    Some(t) => abi_adapter::intern_poly(t).raw(),
                    None => undef,
                });
            }
            let off16 = s[..start].encode_utf16().count();
            words.push(PolyValue::from_i32(off16 as i32).raw());
            words.push(subj_word);
            let a = |i: usize| words.get(i).copied().unwrap_or(undef);
            let r = super::funcops::__rtsadp_fn_invoke(fn_word, a(0), a(1), a(2), a(3), undef);
            out.push_str(&genops::to_string(PolyValue::from_raw(r)));
        }
        last = end;
        if all == 0 {
            break;
        }
    }
    out.push_str(&s[last..]);
    abi_adapter::intern_poly(&out).raw()
}

/// JS spec `GetSubstitution` — expand the replacement-string tokens for ONE
/// match: `$$` → `$`; `$&` → the match; `` $` `` → the text BEFORE the match;
/// `$'` → the text AFTER; `$n`/`$nn` → capture n ("" when unmatched; the
/// two-digit form only when group nn exists); `$<name>` → the named capture
/// ("" when the name exists but did not participate; left VERBATIM when the
/// regex has no such name — JS keeps unrecognized tokens as-is).
fn js_substitution(
    repl: &str,
    s: &str,
    start: usize,
    end: usize,
    groups: &[Option<String>],
    names: &[Option<String>],
) -> String {
    let bytes = repl.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'$' || i + 1 >= bytes.len() {
            // Copy the full UTF-8 char starting here.
            let ch_len = utf8_len(bytes[i]);
            out.push_str(&repl[i..i + ch_len]);
            i += ch_len;
            continue;
        }
        match bytes[i + 1] {
            b'$' => {
                out.push('$');
                i += 2;
            }
            b'&' => {
                out.push_str(&s[start..end]);
                i += 2;
            }
            b'`' => {
                out.push_str(&s[..start]);
                i += 2;
            }
            b'\'' => {
                out.push_str(&s[end..]);
                i += 2;
            }
            b'<' => {
                // `$<name>` — find the closing `>`; an unterminated/unknown name
                // stays verbatim.
                if let Some(close) = repl[i + 2..].find('>') {
                    let name = &repl[i + 2..i + 2 + close];
                    if let Some(gi) = names
                        .iter()
                        .position(|n| n.as_deref() == Some(name))
                    {
                        if let Some(Some(t)) = groups.get(gi) {
                            out.push_str(t);
                        }
                        i += 2 + close + 1;
                        continue;
                    }
                }
                out.push('$');
                i += 1;
            }
            c @ b'0'..=b'9' => {
                // Prefer the two-digit group when it EXISTS (spec), else one digit.
                let d1 = (c - b'0') as usize;
                let two = if i + 2 < bytes.len() && bytes[i + 2].is_ascii_digit() {
                    Some(d1 * 10 + (bytes[i + 2] - b'0') as usize)
                } else {
                    None
                };
                if let Some(nn) = two {
                    if nn >= 1 && nn < groups.len() {
                        if let Some(Some(t)) = groups.get(nn) {
                            out.push_str(t);
                        }
                        i += 3;
                        continue;
                    }
                }
                if d1 >= 1 && d1 < groups.len() {
                    if let Some(Some(t)) = groups.get(d1) {
                        out.push_str(t);
                    }
                    i += 2;
                } else {
                    // No such group: the token stays verbatim (JS).
                    out.push('$');
                    i += 1;
                }
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}

/// Byte length of the UTF-8 char whose first byte is `b`.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// `s.split(re, limit?)` — the JS-spec regex split, in full: CAPTURE GROUPS are
/// spliced into the result (`"abc".split(/(b)/)` → `["a","b","c"]`, an unmatched
/// group as `undefined`), `limit` is ToUint32 (`undefined` → 2³²−1, `0` → `[]`),
/// and EMPTY matches split per position without consuming (`"xxx".split(/x*/)`
/// → `["",""]`; a match at/after the end never splits). Match data is
/// snapshotted like the replace trampoline. Returns a fresh array word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_split(subj_word: u64, re_word: u64, limit_word: u64) -> u64 {
    use rts_engine::heap::handles::{Entry, with_entry};
    let s = handle_str(str_handle(subj_word));
    let lim: u32 = {
        let v = PolyValue::from_raw(limit_word);
        if v.is_undefined() || v.is_hole() {
            u32::MAX
        } else {
            // JS ToUint32 (wraps negatives / truncates).
            let f = genops::dyn_to_number(limit_word);
            if f.is_finite() { (f as i64) as u32 } else { 0 }
        }
    };
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let finish =
        |h: u64| PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h)).raw();
    if lim == 0 {
        return finish(out);
    }
    let re_h = unbox_re(re_word);
    let matches: Vec<(usize, usize, Vec<Option<String>>)> = with_entry(re_h, |e| match e {
        Some(Entry::Regex(rx)) => rx
            .engine
            .captures_all(&s)
            .into_iter()
            .filter_map(|caps| {
                let m0 = caps.groups.first().cloned().flatten()?;
                let texts = caps.groups.iter().map(|g| g.clone().map(|m| m.text)).collect();
                Some((m0.start, m0.end, texts))
            })
            .collect(),
        _ => Vec::new(),
    });
    // Empty subject: a regex that matches the empty string yields `[]`, else `[s]`.
    if s.is_empty() {
        if matches.is_empty() {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, subj_word as i64);
        }
        return finish(out);
    }
    let undef = PolyValue::undefined().raw() as i64;
    let mut push = |w: i64| -> bool {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(out) as u64 >= lim as u64
    };
    let mut p = 0usize;
    for (start, end, groups) in matches {
        // A match at/after the end never splits; an EMPTY match exactly at the
        // current cursor advances without splitting (spec `e == p`).
        if start >= s.len() || (start == end && end == p) {
            continue;
        }
        if push(abi_adapter::intern_poly(&s[p..start]).raw() as i64) {
            return finish(out);
        }
        for g in groups.iter().skip(1) {
            let w = match g {
                Some(t) => abi_adapter::intern_poly(t).raw() as i64,
                None => undef,
            };
            if push(w) {
                return finish(out);
            }
        }
        p = end;
    }
    push(abi_adapter::intern_poly(&s[p..]).raw() as i64);
    finish(out)
}

/// `s.search(re)` — the byte index of the first match, or `-1`. A PolyValue number
/// word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_re_str_search(subj_word: u64, re_word: u64) -> u64 {
    let s = handle_str(str_handle(subj_word));
    let idx = rt_re::__RTS_FN_NS_REGEX_FIND_AT(unbox_re(re_word), s.as_ptr(), s.len() as i64);
    genops::number_result(idx as f64).raw()
}
