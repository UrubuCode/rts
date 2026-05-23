//! Regex runtime operations — backend `regex` crate.

use super::super::gc::handles::{Entry, alloc_entry, free_handle, with_entry};
use regex::{Regex, RegexBuilder};

unsafe fn slice_from(ptr: *const u8, len: i64) -> &'static [u8] {
    if ptr.is_null() || len <= 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(ptr, len as usize) }
}

unsafe fn str_from(ptr: *const u8, len: i64) -> &'static str {
    let bytes = unsafe { slice_from(ptr, len) };
    std::str::from_utf8(bytes).unwrap_or("")
}

fn alloc_string(bytes: Vec<u8>) -> u64 {
    alloc_entry(Entry::String(bytes))
}

fn with_regex<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Regex) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => f(&rx.regex),
        _ => default,
    })
}

/// (#1107) Helper agnostico ao engine — usar quando o callsite precisar
/// suportar lookaround/backref. Closure recebe `&RegexEngine`.
fn with_engine<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&crate::namespaces::gc::handles::RegexEngine) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => f(&rx.engine),
        _ => default,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_COMPILE(
    pat_ptr: *const u8,
    pat_len: i64,
    flag_ptr: *const u8,
    flag_len: i64,
) -> u64 {
    let pattern = unsafe { str_from(pat_ptr, pat_len) };
    let flags = unsafe { str_from(flag_ptr, flag_len) };

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
            // 'u' Unicode ja e default; 'y' (sticky) nao mapeia em RE2.
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
            let engine = crate::namespaces::gc::handles::RegexEngine::Fast(rx.clone());
            alloc_entry(Entry::Regex(Box::new(crate::namespaces::gc::handles::RtsRegex {
                regex: rx,
                engine,
                global,
                flags: canon,
                last_index: 0,
            })))
        }
        Err(_) => {
            // (#1107) Fallback fancy-regex para patterns que RE2 rejeita
            // (lookbehind, lookahead, backreferences). Preserva flags JS
            // via prefixo inline (?ims) quando aplicavel.
            let mut prefix = String::new();
            let has_inline_flags = pattern.starts_with("(?");
            if !has_inline_flags {
                let mut flag_chars = String::new();
                if flags.contains('i') { flag_chars.push('i'); }
                if flags.contains('m') { flag_chars.push('m'); }
                if flags.contains('s') { flag_chars.push('s'); }
                if flags.contains('x') { flag_chars.push('x'); }
                if !flag_chars.is_empty() {
                    prefix = format!("(?{flag_chars})");
                }
            }
            let full_pat = format!("{prefix}{pattern}");
            match fancy_regex::Regex::new(&full_pat) {
                Ok(fancy) => {
                    // Placeholder vazio para o campo `regex` legado (so' usado
                    // por callsites pre-1107 que nao foram migrados; chamadas
                    // sob fancy devem rotear pelo `engine`).
                    let placeholder = regex::Regex::new("").unwrap();
                    let engine = crate::namespaces::gc::handles::RegexEngine::Fancy(fancy);
                    alloc_entry(Entry::Regex(Box::new(crate::namespaces::gc::handles::RtsRegex {
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FREE(handle: u64) {
    let _ = free_handle(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_TEST(handle: u64, ptr: *const u8, len: i64) -> i64 {
    let s_full = unsafe { str_from(ptr, len) };
    // (#673/240) JS spec: regex global usa `lastIndex` como cursor em
    // `test`; avanca em match e reseta para 0 em miss. Regex nao-global
    // sempre busca do 0 e nao mexe em `lastIndex`.
    super::super::gc::handles::with_entry_mut(handle, |entry| match entry {
        Some(super::super::gc::handles::Entry::Regex(rx)) => {
            if rx.global {
                let start = rx.last_index;
                if start > s_full.len() || !s_full.is_char_boundary(start) {
                    rx.last_index = 0;
                    return 0;
                }
                match rx.engine.find(&s_full[start..]) {
                    Some(m) => {
                        rx.last_index = start + m.end;
                        1
                    }
                    None => {
                        rx.last_index = 0;
                        0
                    }
                }
            } else if rx.engine.is_match(s_full) {
                1
            } else {
                0
            }
        }
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FIND(handle: u64, ptr: *const u8, len: i64) -> u64 {
    let s_full = unsafe { str_from(ptr, len) };
    // (#782) JS spec: regex global usa `lastIndex` como cursor; avanca em
    // match e reseta para 0 em miss ou quando passa do fim. Regex
    // nao-global ignora `lastIndex` e sempre comeca do 0.
    let bytes = super::super::gc::handles::with_entry_mut(handle, |entry| {
        match entry {
            Some(super::super::gc::handles::Entry::Regex(rx)) => {
                if rx.global {
                    let start = rx.last_index;
                    if start > s_full.len() {
                        rx.last_index = 0;
                        return None;
                    }
                    // Find a partir de byte offset; precisa que start esteja
                    // em fronteira UTF-8 (caso contrario reset).
                    if !s_full.is_char_boundary(start) {
                        rx.last_index = 0;
                        return None;
                    }
                    match rx.engine.find(&s_full[start..]) {
                        Some(m) => {
                            let abs_end = start + m.end;
                            rx.last_index = abs_end;
                            Some(m.text.into_bytes())
                        }
                        None => {
                            rx.last_index = 0;
                            None
                        }
                    }
                } else {
                    rx.engine.find(s_full).map(|m| m.text.into_bytes())
                }
            }
            _ => None,
        }
    });
    match bytes {
        Some(b) => alloc_string(b),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FIND_AT(handle: u64, ptr: *const u8, len: i64) -> i64 {
    let s = unsafe { str_from(ptr, len) };
    with_engine(handle, -1i64, |eng| {
        eng.find(s).map(|m| m.start as i64).unwrap_or(-1)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_REPLACE(
    handle: u64,
    ptr: *const u8,
    len: i64,
    rep_ptr: *const u8,
    rep_len: i64,
) -> u64 {
    let s = unsafe { str_from(ptr, len) };
    let rep = unsafe { str_from(rep_ptr, rep_len) };
    let out = with_engine(handle, s.to_string(), |eng| eng.replace_first(s, rep));
    alloc_string(out.into_bytes())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_REPLACE_ALL(
    handle: u64,
    ptr: *const u8,
    len: i64,
    rep_ptr: *const u8,
    rep_len: i64,
) -> u64 {
    let s = unsafe { str_from(ptr, len) };
    let rep = unsafe { str_from(rep_ptr, rep_len) };
    let out = with_engine(handle, s.to_string(), |eng| eng.replace_all(s, rep));
    alloc_string(out.into_bytes())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_MATCH_COUNT(handle: u64, ptr: *const u8, len: i64) -> i64 {
    let s = unsafe { str_from(ptr, len) };
    with_engine(handle, 0i64, |eng| eng.find_all(s).len() as i64)
}
