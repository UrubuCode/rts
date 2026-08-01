//! The two shapes a stdlib operation can take, as callable symbols.
//!
//! `probe_char_str` + `probe_str_eq_lit` are what a `.ts` prelude scanner
//! compiles down to: one trampoline per character, and `s[i]` yielding a fresh
//! one-character STRING because that is what JS index-on-string means
//! (`json.ts:238` — `const c = this.#s[this.#i]`, then `c === " "`).
//!
//! `probe_scan_native` is the same operation as ONE symbol: the whole document
//! consumed in LLVM-compiled Rust, one lock, no per-character allocation.

use crate::slab::{self, Entry};

/// `s.length` through the trampoline.
#[inline(never)]
pub extern "C" fn probe_str_len(h: i64) -> i64 {
    slab::sharded::with(h as u64, |e| match e {
        Some(Entry::String(s)) => s.len() as i64,
        _ => 0,
    })
}

/// `s[i]` — the UTF-16 unit at `i`, materialized as a fresh one-character
/// string with a fresh handle. `abi_adapter.rs:62-67` records that `intern_poly`
/// does NOT intern: it allocates both, every call.
#[inline(never)]
pub extern "C" fn probe_char_str(h: i64, i: i64) -> i64 {
    let b = slab::sharded::with(h as u64, |e| match e {
        Some(Entry::String(s)) => s.get(i as usize).copied().unwrap_or(0),
        _ => 0,
    });
    let s = String::from_utf16_lossy(&[u16::from(b)]);
    slab::sharded::alloc(Entry::String(s.into_bytes())) as i64
}

/// `c === "<lit>"` where the literal is a single byte passed as its code.
#[inline(never)]
pub extern "C" fn probe_str_eq_lit(h: i64, lit: i64) -> i64 {
    slab::sharded::with(h as u64, |e| match e {
        Some(Entry::String(s)) => i64::from(s.len() == 1 && i64::from(s[0]) == lit),
        _ => 0,
    })
}

/// The WHOLE scan as one native symbol: count `{` and `:` over the document.
/// One lock, borrowed bytes, no allocation — and, being a symbol, it costs the
/// compile pipeline NOTHING (its address is already in the binary).
#[inline(never)]
pub extern "C" fn probe_scan_native(h: i64) -> i64 {
    slab::sharded::with(h as u64, |e| match e {
        Some(Entry::String(s)) => {
            let mut acc = 0i64;
            for &b in s.iter() {
                acc += i64::from(b == b'{') + i64::from(b == b':');
            }
            acc
        }
        _ => 0,
    })
}
