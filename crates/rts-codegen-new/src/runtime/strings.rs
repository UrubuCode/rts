//! A tiny global string interner — the heap backing for `PolyValue` strings.
//!
//! A `PolyValue` string carries a 48-bit handle slot ([`super::super::value::
//! PolyValue::from_str_handle`]); the actual UTF-8 bytes live here, indexed by
//! that slot. This is the P1 stand-in for the real `rts-engine` `HandleTable`
//! string pool: minimal, thread-safe, but a genuine handle→bytes mapping so the
//! PolyValue boundary is exercised for real.

use std::sync::{Mutex, OnceLock};

use crate::value::PolyValue;

/// The global interner. Strings are append-only (P1 has no GC sweep), so a slot
/// once handed out stays valid for the process lifetime.
fn pool() -> &'static Mutex<Vec<String>> {
    static POOL: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(Vec::new()))
}

/// Intern `s`, returning its 48-bit slot index. Deduplicates so equal strings
/// share a slot (makes string `===` a pure tag+payload compare downstream).
pub fn intern(s: &str) -> u64 {
    let mut guard = pool().lock().expect("string pool poisoned");
    if let Some(idx) = guard.iter().position(|existing| existing == s) {
        return idx as u64;
    }
    let idx = guard.len() as u64;
    guard.push(s.to_string());
    idx
}

/// Resolve a slot back to its `String`. Panics on an out-of-range slot (a bug:
/// a string PolyValue must always point at a live slot).
pub fn get(slot: u64) -> String {
    let guard = pool().lock().expect("string pool poisoned");
    guard
        .get(slot as usize)
        .cloned()
        .unwrap_or_else(|| panic!("string slot {slot} out of range (len {})", guard.len()))
}

/// Convenience: intern `s` and wrap it as a string `PolyValue`.
pub fn intern_poly(s: &str) -> PolyValue {
    PolyValue::from_str_handle(intern(s))
}

/// Resolve a string `PolyValue` to its text. Debug-asserts the value is a string.
pub fn resolve_poly(v: PolyValue) -> String {
    debug_assert!(v.is_string(), "resolve_poly on a non-string PolyValue");
    get(v.as_handle())
}
