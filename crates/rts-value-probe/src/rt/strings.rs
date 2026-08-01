//! String representation under test.
//!
//! The current one: `Entry::String(Vec<u8>)` in the sharded slab, reached by
//! handle. Concatenation goes through `__RTS_FN_NS_GC_STRING_CONCAT`
//! (`heap/string_pool/alloc.rs:136`), and that function does NOT read the bytes
//! in place — it goes through the SNAPSHOT layer:
//!
//! ```text
//! snapshot_entry(a)  -> EntrySnap::Str(s.clone())     // copy #1 of a
//! snapshot_entry(b)  -> EntrySnap::Str(s.clone())     // copy #1 of b
//! snapshot_to_bytes(&snap_a) -> b.clone()             // copy #2 of a
//! snapshot_to_bytes(&snap_b) -> b.clone()             // copy #2 of b
//! out.extend_from_slice(..)                           // copy #3 of b
//! alloc_entry(Entry::String(out))                     // + the slab lock
//! ```
//!
//! Equality (`__RTS_FN_NS_GC_STRING_EQ`, `alloc.rs:96`) is a CONTENT compare
//! under the shard lock, with only a handle-identity short-circuit — there is no
//! interning, so two equal strings built by different paths always memcmp.

use crate::slab::{self, Entry};

// --- what the snapshot layer actually does ---------------------------------

enum Snap {
    Str(Vec<u8>),
    None,
}

fn snapshot(payload: u64) -> Snap {
    slab::sharded::with(payload, |e| match e {
        Some(Entry::String(s)) => Snap::Str(s.clone()), // copy #1
        _ => Snap::None,
    })
}

fn snapshot_to_bytes(s: &Snap) -> Vec<u8> {
    match s {
        Snap::Str(b) => b.clone(), // copy #2
        Snap::None => Vec::new(),
    }
}

// --- D0: today -------------------------------------------------------------

/// Byte-for-byte the shape of `__RTS_FN_NS_GC_STRING_CONCAT`.
pub fn concat_today(a: u64, b: u64) -> u64 {
    let snap_a = snapshot(a);
    let snap_b = snapshot(b);
    let mut out = snapshot_to_bytes(&snap_a);
    out.extend_from_slice(&snapshot_to_bytes(&snap_b));
    slab::sharded::alloc(Entry::String(out))
}

// --- D1: same contract, no snapshot layer ----------------------------------

/// Identical OBSERVABLE behaviour — a fresh immutable string handle — with the
/// snapshot round-trip removed: read each side under its own lock, size the
/// output once, copy each side exactly once.
pub fn concat_direct(a: u64, b: u64) -> u64 {
    let av = slab::sharded::with(a, |e| match e {
        Some(Entry::String(s)) => s.clone(),
        _ => Vec::new(),
    });
    let out = slab::sharded::with(b, |e| match e {
        Some(Entry::String(s)) => {
            let mut o = Vec::with_capacity(av.len() + s.len());
            o.extend_from_slice(&av);
            o.extend_from_slice(s);
            o
        }
        _ => av.clone(),
    });
    slab::sharded::alloc(Entry::String(out))
}

// --- D2: append in place ---------------------------------------------------

/// What a mutable accumulator / rope / string builder does: append into the
/// EXISTING buffer. Amortised O(len(b)) per append instead of O(len(a)+len(b)),
/// so the classic `s += x` loop stops being quadratic.
///
/// This is NOT drop-in: JS strings are immutable, so an engine may only do this
/// when it can prove the old value is dead (single-use accumulator). That proof
/// is the same escape/liveness analysis the object path wants.
pub fn append_in_place(a: u64, b: u64) -> u64 {
    let bv = slab::sharded::with(b, |e| match e {
        Some(Entry::String(s)) => s.clone(),
        _ => Vec::new(),
    });
    slab::sharded::with_mut(a, |e| {
        if let Some(Entry::String(s)) = e {
            s.extend_from_slice(&bv);
        }
    });
    a
}

// --- equality --------------------------------------------------------------

/// `__RTS_FN_NS_GC_STRING_EQ`: handle identity, else content compare under the
/// `with_two_entries` lock — no intermediate copy, exactly like the real one.
pub fn eq_today(a: u64, b: u64) -> bool {
    if a == b {
        return true;
    }
    slab::sharded::with_two(a, b, |ea, eb| match (ea, eb) {
        (Some(Entry::String(sa)), Some(Entry::String(sb))) => sa == sb,
        _ => false,
    })
}

/// With INTERNING, equal content implies equal handle, so `===` is one integer
/// compare and never touches the heap. The cost moves to construction (a hash
/// lookup per string built) — which is why this is a tradeoff, not a free win.
pub fn eq_interned(a: u64, b: u64) -> bool {
    a == b
}

/// Content compare with the strings already in hand — the floor.
pub fn eq_raw(a: &[u8], b: &[u8]) -> bool {
    a == b
}

pub fn new_string(bytes: &[u8]) -> u64 {
    slab::sharded::alloc(Entry::String(bytes.to_vec()))
}

pub fn len_of(payload: u64) -> usize {
    slab::sharded::with(payload, |e| match e {
        Some(Entry::String(s)) => s.len(),
        _ => 0,
    })
}
