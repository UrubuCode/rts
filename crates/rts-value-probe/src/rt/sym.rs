//! Precompiled native "superinstruction" symbols.
//!
//! These are the STRONGEST form of the "let Cranelift only order calls to
//! precompiled symbols" architecture: ordinary Rust, compiled by LLVM at
//! `opt-level = 3` (this crate pins it), doing K elements of work per call and
//! taking the shard lock ONCE for the whole chunk rather than once per element.
//!
//! Nothing here is handicapped. If the architecture can win, it wins with these.

use crate::slab::{self, Entry};

/// Sum `k` elements starting at `start` into `acc`, in ONE call, under ONE lock.
///
/// `acc` and the return are raw `f64` bits, so the value never leaves a register
/// pair across the boundary — the cheapest ABI shape available for this.
#[inline(never)]
pub extern "C" fn probe_chunk_add(acc_bits: i64, payload: i64, start: i64, k: i64) -> i64 {
    let mut s = f64::from_bits(acc_bits as u64);
    slab::sharded::with(payload as u64, |e| {
        if let Some(Entry::Vec(v)) = e {
            let begin = (start.max(0) as usize).min(v.len());
            let end = ((start + k).max(0) as usize).min(v.len());
            for w in &v[begin..end] {
                s += f64::from_bits(*w as u64);
            }
        }
    });
    s.to_bits() as i64
}

/// The same work with NO lock and a raw base pointer — the ceiling of the
/// superinstruction idea, with the container cost removed entirely.
///
/// # Safety
/// `base` must point at `start + k` readable `i64` words. The probe's fixture
/// guarantees it; this is not a shipped API.
#[inline(never)]
pub extern "C" fn probe_chunk_add_raw(acc_bits: i64, base: i64, start: i64, k: i64) -> i64 {
    let mut s = f64::from_bits(acc_bits as u64);
    let p = base as *const f64;
    for i in start..(start + k) {
        // SAFETY: see the doc comment — the caller's fixture owns the range.
        s += unsafe { *p.add(i as usize) };
    }
    s.to_bits() as i64
}
