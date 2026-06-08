//! `regex` namespace — regular expressions via the `regex` crate (RE2), with a
//! `fancy_regex` fallback for lookaround/backreferences (#1107).
//!
//! Compilacao retorna handle (Entry::Regex). Operacoes (test/find/replace)
//! aceitam o handle como primeiro argumento. Literais TS `/pat/flags` sao
//! desugared no codegen para `regex.compile(pat, flags)`.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use regex::RegexBuilder;
use rts_abi::ty::{Bool, Handle, I64};
use rts_macro::rts_namespace;

use crate::namespaces::gc::handles::{
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

/// Regex engine (`regex` crate + fancy_regex fallback).
#[rts_namespace(regex)]
impl RegexNs {
    /// Compila `pattern` com `flags` JS (igmsuyx). Handle, ou 0 em erro.
    #[rts_fn]
    pub fn compile(pattern: Str, flags: Str) -> Handle {
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
    #[rts_fn]
    pub fn free(handle: Handle) {
        let _ = free_handle(handle);
    }

    /// `regex.test(s)` — respeita lastIndex em regex global (JS spec).
    #[rts_fn]
    pub fn test(handle: Handle, s: Str) -> Bool {
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
    #[rts_fn]
    pub fn find(handle: Handle, s: Str) -> Handle {
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
    #[rts_fn]
    pub fn find_at(handle: Handle, s: Str) -> I64 {
        with_engine(handle, -1i64, |eng| {
            eng.find(s).map(|m| m.start as i64).unwrap_or(-1)
        })
    }

    /// Substitui o primeiro match por `replacement`. Retorna string handle.
    #[rts_fn]
    pub fn replace(handle: Handle, s: Str, replacement: Str) -> Handle {
        let out = with_engine(handle, s.to_string(), |eng| {
            eng.replace_first(s, replacement)
        });
        alloc_string(out.into_bytes())
    }

    /// Substitui todos os matches por `replacement`. Retorna string handle.
    #[rts_fn]
    pub fn replace_all(handle: Handle, s: Str, replacement: Str) -> Handle {
        let out = with_engine(handle, s.to_string(), |eng| eng.replace_all(s, replacement));
        alloc_string(out.into_bytes())
    }

    /// Numero de matches de `s`.
    #[rts_fn]
    pub fn match_count(handle: Handle, s: Str) -> I64 {
        with_engine(handle, 0i64, |eng| eng.find_all(s).len() as i64)
    }
}
