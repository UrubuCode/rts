//! Regex runtime operations — backend `regex` crate.

use super::super::gc::handles::{Entry, table};
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
    table().lock().unwrap().alloc(Entry::String(bytes))
}

fn with_regex<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Regex) -> R,
{
    let guard = table().lock().unwrap();
    if let Some(Entry::Regex(rx)) = guard.get(handle) {
        f(rx)
    } else {
        default
    }
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
            // 'g', 'u', 'y' nao alteram o builder: 'g' e tratado pelo
            // caller (replace_all vs replace), 'u' Unicode ja e default,
            // 'y' (sticky) nao mapeia em RE2.
            _ => {}
        }
    }
    match builder.build() {
        Ok(rx) => table().lock().unwrap().alloc(Entry::Regex(Box::new(rx))),
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FREE(handle: u64) {
    let _ = table().lock().unwrap().free(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_TEST(handle: u64, ptr: *const u8, len: i64) -> i64 {
    let s = unsafe { str_from(ptr, len) };
    with_regex(handle, 0i64, |rx| if rx.is_match(s) { 1 } else { 0 })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FIND(handle: u64, ptr: *const u8, len: i64) -> u64 {
    let s = unsafe { str_from(ptr, len) };
    let bytes = with_regex(handle, None, |rx| {
        rx.find(s).map(|m| m.as_str().as_bytes().to_vec())
    });
    match bytes {
        Some(b) => alloc_string(b),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_FIND_AT(handle: u64, ptr: *const u8, len: i64) -> i64 {
    let s = unsafe { str_from(ptr, len) };
    with_regex(handle, -1i64, |rx| {
        rx.find(s).map(|m| m.start() as i64).unwrap_or(-1)
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
    let out = with_regex(handle, s.to_string(), |rx| rx.replace(s, rep).into_owned());
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
    let out = with_regex(handle, s.to_string(), |rx| {
        rx.replace_all(s, rep).into_owned()
    });
    alloc_string(out.into_bytes())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_REGEX_MATCH_COUNT(handle: u64, ptr: *const u8, len: i64) -> i64 {
    let s = unsafe { str_from(ptr, len) };
    with_regex(handle, 0i64, |rx| rx.find_iter(s).count() as i64)
}
