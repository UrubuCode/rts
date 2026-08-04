//! The one entry point the machine names itself.
//!
//! # Why this is not a `RuntimeOp`
//!
//! Every other entry point in this crate is the LANGUAGE asking for something:
//! `rts-codegen` decides that `a + b` needs a call and names the symbol. This
//! one is the MACHINE asking. `Inst::Alloc` lowers to a call to `rts_alloc`
//! whatever language is being compiled, because asking a heap for space is the
//! one memory operation that is not arithmetic:
//!
//! > Reading and writing a field is arithmetic and lands here; asking a heap for
//! > space is a runtime entry point, and a runtime entry point is something to
//! > declare rather than something to emit.
//!
//! So the name is not ours to choose. `RtEntry::Alloc` says `rts_alloc`, without
//! the `__rts_` the language's own operations carry, and the attribute is given
//! that name explicitly rather than deriving one that would not match.
//!
//! # What the descriptor this emits is not
//!
//! Authoritative. `RtEntry::Alloc` states the signature —
//! `(I64, I64) -> Ref(Opaque)` — and that is what compiled code is built
//! against. The attribute derives `(Tagged, Tagged) -> Tagged` from the Rust
//! types, which agrees about the machine words and not about what they mean.
//!
//! Saying so beats inventing a Rust type whose only purpose is to make a derived
//! descriptor match a descriptor nobody reads: nothing consults this one,
//! because for a machine entry the machine's own table is the contract.

use super::with_current;

/// Asks the heap for a cell.
///
/// # What it returns when the heap is full
///
/// Zero, which is a reference to cell zero — a real object, and the wrong one.
/// That is a **known defect** and it is here rather than hidden because the
/// alternative today is worse: the signature returns a reference and has no way
/// to say "no", and a compiled program has no handler to send a failure to.
///
/// It goes when there is a collector to ask first, or protected regions to throw
/// through. Until then a program that exhausts its region is wrong, and this
/// records where.
#[rtse::entry("rts_alloc")]
pub fn alloc(size: i64, ty: i64) -> u64 {
    with_current(|context| {
        let size = u32::try_from(size).unwrap_or(u32::MAX);
        let ty = u32::try_from(ty).unwrap_or(u32::MAX);
        u64::from(context.region.alloc(size, ty).unwrap_or(0))
    })
}
