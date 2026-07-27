//! `regex` namespace — regular expressions via the `regex` crate (RE2), with a
//! `fancy_regex` fallback for lookaround/backreferences (#1107).
//!
//! Compilacao retorna handle (Entry::Regex). Operacoes (test/find/replace)
//! aceitam o handle como primeiro argumento. Literais TS `/pat/flags` sao
//! desugared no codegen para `regex.compile(pat, flags)`.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use regex::RegexBuilder;
use rts_engine::abi::ty::{Bool, Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{
    Entry, RegexEngine, RtsRegex, alloc_entry, free_handle, with_entry, with_entry_mut,
};

fn alloc_string(bytes: Vec<u8>) -> u64 {
    alloc_entry(Entry::String(bytes))
}

/// Engine-agnostic accessor (RE2 or fancy) for callsites that only need a read.
fn with_engine<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&RegexEngine) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => f(&rx.engine),
        _ => default,
    })
}

/// Compila `pattern` com `flags` JS (igmsuyx). Handle, ou 0 em erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_COMPILE(
    pattern_ptr: *const u8,
    pattern_len: i64,
    flags_ptr: *const u8,
    flags_len: i64,
) -> Handle {
    let pattern = match unsafe { rts_engine::abi::str_abi::from_abi(pattern_ptr, pattern_len) } {
        Some(s) => s,
        None => return 0,
    };
    let flags = match unsafe { rts_engine::abi::str_abi::from_abi(flags_ptr, flags_len) } {
        Some(s) => s,
        None => return 0,
    };
    let mut builder = RegexBuilder::new(pattern);
    for c in flags.chars() {
        match c {
            'i' => {
                builder.case_insensitive(true);
            }
            'm' => {
                builder.multi_line(true);
            }
            's' => {
                builder.dot_matches_new_line(true);
            }
            'x' => {
                builder.ignore_whitespace(true);
            }
            _ => {}
        }
    }
    let global = flags.contains('g');
    // (#781) Flags canonicas JS em ordem alfabetica: d g i m s u y.
    let mut canon = String::new();
    for c in ['d', 'g', 'i', 'm', 's', 'u', 'y'] {
        if flags.contains(c) {
            canon.push(c);
        }
    }
    match builder.build() {
        Ok(rx) => {
            let engine = RegexEngine::Fast(rx.clone());
            alloc_entry(Entry::Regex(Box::new(RtsRegex {
                regex: rx,
                engine,
                global,
                flags: canon,
                last_index: 0,
            })))
        }
        Err(_) => {
            // (#1107) fancy_regex fallback para lookaround/backref.
            let mut prefix = String::new();
            if !pattern.starts_with("(?") {
                let mut flag_chars = String::new();
                if flags.contains('i') {
                    flag_chars.push('i');
                }
                if flags.contains('m') {
                    flag_chars.push('m');
                }
                if flags.contains('s') {
                    flag_chars.push('s');
                }
                if flags.contains('x') {
                    flag_chars.push('x');
                }
                if !flag_chars.is_empty() {
                    prefix = format!("(?{flag_chars})");
                }
            }
            let full_pat = format!("{prefix}{pattern}");
            match fancy_regex::Regex::new(&full_pat) {
                Ok(fancy) => {
                    let placeholder = regex::Regex::new("").unwrap();
                    let engine = RegexEngine::Fancy(fancy);
                    alloc_entry(Entry::Regex(Box::new(RtsRegex {
                        regex: placeholder,
                        engine,
                        global,
                        flags: canon,
                        last_index: 0,
                    })))
                }
                Err(_) => 0,
            }
        }
    }
}

/// Libera o handle do regex.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FREE(handle: Handle) {
    let _ = free_handle(handle);
}

/// `regex.test(s)` — respeita lastIndex em regex global (JS spec).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_TEST(handle: Handle, s_ptr: *const u8, s_len: i64) -> Bool {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => {
            if rx.global {
                let start = rx.last_index;
                if start > s.len() || !s.is_char_boundary(start) {
                    rx.last_index = 0;
                    return 0;
                }
                match rx.engine.find(&s[start..]) {
                    Some(m) => {
                        rx.last_index = start + m.end;
                        1
                    }
                    None => {
                        rx.last_index = 0;
                        0
                    }
                }
            } else if rx.engine.is_match(s) {
                1
            } else {
                0
            }
        }
        _ => 0,
    })
}

/// Primeiro match como string handle (respeita lastIndex em global). 0 se sem match.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FIND(handle: Handle, s_ptr: *const u8, s_len: i64) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let bytes = with_entry_mut(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => {
            if rx.global {
                let start = rx.last_index;
                if start > s.len() || !s.is_char_boundary(start) {
                    rx.last_index = 0;
                    return None;
                }
                match rx.engine.find(&s[start..]) {
                    Some(m) => {
                        rx.last_index = start + m.end;
                        Some(m.text.into_bytes())
                    }
                    None => {
                        rx.last_index = 0;
                        None
                    }
                }
            } else {
                rx.engine.find(s).map(|m| m.text.into_bytes())
            }
        }
        _ => None,
    });
    match bytes {
        Some(b) => alloc_string(b),
        None => 0,
    }
}

/// Byte offset do primeiro match, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FIND_AT(handle: Handle, s_ptr: *const u8, s_len: i64) -> I64 {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    with_engine(handle, -1i64, |eng| {
        eng.find(s).map(|m| m.start as i64).unwrap_or(-1)
    })
}

/// Substitui o primeiro match por `replacement`. Retorna string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_REPLACE(
    handle: Handle,
    s_ptr: *const u8,
    s_len: i64,
    replacement_ptr: *const u8,
    replacement_len: i64,
) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let replacement =
        match unsafe { rts_engine::abi::str_abi::from_abi(replacement_ptr, replacement_len) } {
            Some(s) => s,
            None => return 0,
        };
    let out = with_engine(handle, s.to_string(), |eng| {
        eng.replace_first(s, replacement)
    });
    alloc_string(out.into_bytes())
}

/// Substitui todos os matches por `replacement`. Retorna string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_REPLACE_ALL(
    handle: Handle,
    s_ptr: *const u8,
    s_len: i64,
    replacement_ptr: *const u8,
    replacement_len: i64,
) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let replacement =
        match unsafe { rts_engine::abi::str_abi::from_abi(replacement_ptr, replacement_len) } {
            Some(s) => s,
            None => return 0,
        };
    let out = with_engine(handle, s.to_string(), |eng| eng.replace_all(s, replacement));
    alloc_string(out.into_bytes())
}

/// Numero de matches de `s`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_MATCH_COUNT(
    handle: Handle,
    s_ptr: *const u8,
    s_len: i64,
) -> I64 {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    with_engine(handle, 0i64, |eng| eng.find_all(s).len() as i64)
}

/// (P5.12 / rts-codegen-new) Split `s` on every (non-overlapping) match of the
/// regex, returning a `Vec` handle whose i64 slots are the REAL string handles of
/// the parts — JS `String.prototype.split(regexp)` semantics (the runtime `regex`
/// crate decides the matches). The new engine reboxes each slot into a PolyValue
/// word codegen-side (the slots are RAW string handles, not PolyValue words). 0 on
/// a bad regex/subject handle. Empty pattern ⇒ a single part (the whole string),
/// matching the conservative JS-vs-Rust note (Rust's empty match would split per
/// char; we keep the whole string to avoid a silent divergence here).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_SPLIT(handle: Handle, s_ptr: *const u8, s_len: i64) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let parts: Vec<String> = with_engine(handle, vec![s.to_string()], |eng| {
        let matches = eng.find_all(s);
        if matches.is_empty() {
            return vec![s.to_string()];
        }
        let mut out = Vec::with_capacity(matches.len() + 1);
        let mut last = 0usize;
        for m in &matches {
            // A zero-width match would loop / split per char (Rust semantics);
            // skip zero-width matches so behavior stays predictable (JS-vs-Rust).
            if m.end == m.start {
                continue;
            }
            out.push(s[last..m.start].to_string());
            last = m.end;
        }
        out.push(s[last..].to_string());
        out
    });
    let vec = alloc_entry(Entry::Vec(Box::new(
        parts
            .into_iter()
            .map(|p| alloc_string(p.into_bytes()) as i64)
            .collect::<Vec<i64>>(),
    )));
    vec
}

/// (P5.12 / rts-codegen-new) Every match of the regex in `s` (whole-match text,
/// group 0) as a `Vec` handle of REAL string handles — JS `String.prototype.match`
/// for a GLOBAL regex (returns all matches). 0 on a bad handle. An EMPTY result
/// (no match) returns 0 (the lowering maps it to `null`, matching JS). The new
/// engine reboxes each slot into a PolyValue word codegen-side.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_MATCH_ALL(
    handle: Handle,
    s_ptr: *const u8,
    s_len: i64,
) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let texts: Vec<String> = with_engine(handle, Vec::new(), |eng| {
        eng.find_all(s).into_iter().map(|m| m.text).collect()
    });
    if texts.is_empty() {
        return 0;
    }
    alloc_entry(Entry::Vec(Box::new(
        texts
            .into_iter()
            .map(|t| alloc_string(t.into_bytes()) as i64)
            .collect::<Vec<i64>>(),
    )))
}

/// `String.prototype.match` result as a `Vec` of REAL string handles. A NON-global
/// regex returns the FIRST match's `[fullMatch, group1, group2, …]` (capture
/// groups, JS spec — a group that did not participate is the `0`/null sentinel). A
/// GLOBAL (`/g`) regex returns all full matches (group 0 only), like `match_all`.
/// `0` on a bad handle / NO MATCH (the lowering maps it to `null`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_MATCH_GROUPS(
    handle: Handle,
    s_ptr: *const u8,
    s_len: i64,
) -> Handle {
    let s = match unsafe { rts_engine::abi::str_abi::from_abi(s_ptr, s_len) } {
        Some(s) => s,
        None => return 0,
    };
    let result: Option<Vec<Option<String>>> = with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => {
            if rx.global {
                let texts = rx.engine.find_all(s);
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.into_iter().map(|m| Some(m.text)).collect())
                }
            } else {
                rx.engine
                    .captures(s)
                    .map(|c| c.groups.into_iter().map(|g| g.map(|m| m.text)).collect())
            }
        }
        _ => None,
    });
    match result {
        None => 0,
        Some(groups) => alloc_entry(Entry::Vec(Box::new(
            groups
                .into_iter()
                .map(|g| match g {
                    Some(t) => alloc_string(t.into_bytes()) as i64,
                    None => 0,
                })
                .collect::<Vec<i64>>(),
        ))),
    }
}

/// Função `regex.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registra a namespace `regex` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("regex")
        .doc("Regex engine (`regex` crate + fancy_regex fallback).")
        .member(func(
            "compile",
            "__RTS_FN_NS_REGEX_COMPILE",
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Handle),
            "compile(pattern: string, flags: string): number",
            "Compila `pattern` com `flags` JS (igmsuyx). Handle, ou 0 em erro.",
            __RTS_FN_NS_REGEX_COMPILE as *const u8,
        ))
        .member(func(
            "free",
            "__RTS_FN_NS_REGEX_FREE",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "free(handle: number): void",
            "Libera o handle do regex.",
            __RTS_FN_NS_REGEX_FREE as *const u8,
        ))
        .member(func(
            "test",
            "__RTS_FN_NS_REGEX_TEST",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Bool),
            "test(handle: number, s: string): boolean",
            "`regex.test(s)` — respeita lastIndex em regex global (JS spec).",
            __RTS_FN_NS_REGEX_TEST as *const u8,
        ))
        .member(func(
            "find",
            "__RTS_FN_NS_REGEX_FIND",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "find(handle: number, s: string): string",
            "Primeiro match como string handle (respeita lastIndex em global). 0 se sem match.",
            __RTS_FN_NS_REGEX_FIND as *const u8,
        ))
        .member(func(
            "find_at",
            "__RTS_FN_NS_REGEX_FIND_AT",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::I64),
            "find_at(handle: number, s: string): number",
            "Byte offset do primeiro match, ou -1.",
            __RTS_FN_NS_REGEX_FIND_AT as *const u8,
        ))
        .member(func(
            "replace",
            "__RTS_FN_NS_REGEX_REPLACE",
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Handle,
            ),
            "replace(handle: number, s: string, replacement: string): string",
            "Substitui o primeiro match por `replacement`. Retorna string handle.",
            __RTS_FN_NS_REGEX_REPLACE as *const u8,
        ))
        .member(func(
            "replace_all",
            "__RTS_FN_NS_REGEX_REPLACE_ALL",
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Handle,
            ),
            "replace_all(handle: number, s: string, replacement: string): string",
            "Substitui todos os matches por `replacement`. Retorna string handle.",
            __RTS_FN_NS_REGEX_REPLACE_ALL as *const u8,
        ))
        .member(func(
            "match_count",
            "__RTS_FN_NS_REGEX_MATCH_COUNT",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::I64),
            "match_count(handle: number, s: string): number",
            "Numero de matches de `s`.",
            __RTS_FN_NS_REGEX_MATCH_COUNT as *const u8,
        ))
        .member(func(
            "split",
            "__RTS_FN_NS_REGEX_SPLIT",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "split(handle: number, s: string): number",
            "Divide `s` em cada match do regex; Vec handle de string handles.",
            __RTS_FN_NS_REGEX_SPLIT as *const u8,
        ))
        .member(func(
            "match_all",
            "__RTS_FN_NS_REGEX_MATCH_ALL",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "match_all(handle: number, s: string): number",
            "Todos os matches (group 0) de `s`; Vec handle de string handles, ou 0.",
            __RTS_FN_NS_REGEX_MATCH_ALL as *const u8,
        ))
        .done();
}
