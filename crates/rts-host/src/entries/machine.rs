//! The machine's own entry points, apart from the language's.
//!
//! Its own file because it is its own contract — `RtEntry`, which
//! `rts-cranelift` states and emits, against `RuntimeOp`, which `rts-codegen`
//! states and `super::resolve` answers — and because `mod.rs` had passed this
//! crate's 500-line ceiling with both in it.

use rts_cranelift::symbols::RtEntry;

/// The runtime's implementation of one of the machine's own entry points.
///
/// Separate from `address_of` because the two are different contracts. That one
/// serves `RuntimeOp`, which `rts-codegen` states and this crate resolves; this
/// one serves `RtEntry`, which `rts-cranelift` states and emits itself. A match
/// missing an arm there means the language named something the runtime lacks; a
/// match missing an arm here means the machine emits an instruction whose entry
/// point nobody supplied, which is a crash in compiled code.
pub(crate) fn machine_entry(entry: RtEntry) -> *const u8 {
    match entry {
        RtEntry::Alloc => rts_core::entry::alloc as extern "C" fn(i64, i64) -> u64 as *const u8,
        RtEntry::CacheResolve => {
            rts_core::entry::cache_resolve as extern "C" fn(u64, i64, i64) -> i64 as *const u8
        }
        // A store asks a different resolver, which refuses for an object that
        // refuses to be written. Same signature, and deliberately not the same
        // function: a READ of a frozen object still resolves to its offset.
        RtEntry::CacheResolveStore => {
            rts_core::entry::cache_resolve_store as extern "C" fn(u64, i64, i64) -> i64
                as *const u8
        }
        // A third, for a site whose answer may live in the cell its receiver
        // inherits from. The same three operands and the same return, and a
        // separate function for the same reason the store is one: what it may
        // answer differs — this one may report an address, and reporting one is
        // a claim about that address outliving the read.
        RtEntry::CacheResolveIndirect => {
            rts_core::entry::cache_resolve_indirect as extern "C" fn(u64, i64, i64) -> i64
                as *const u8
        }
        // A fourth, and the only one whose SIGNATURE differs: the middle operand
        // is the key as a value rather than as a number, because the site was
        // handed one and never told which property it reads. The cast written
        // out here is what makes that a compile error rather than a corrupt call
        // if either side ever changes its mind.
        RtEntry::CacheResolveKeyed => {
            rts_core::entry::cache_resolve_keyed as extern "C" fn(u64, u64, i64) -> i64 as *const u8
        }
        RtEntry::WriteBarrier => {
            rts_core::entry::write_barrier as extern "C" fn(u64, u64) as *const u8
        }
        // Reached only when nothing in the throwing function caught it, which
        // ends the program with the value reported. A handler one frame up is
        // the case the language layer refuses by name rather than compiling
        // into a `catch` that would silently never run.
        RtEntry::Throw => rts_core::entry::throw as extern "C" fn(i64, u64) as *const u8,
        // The rest are emitted by instructions this compiler does not produce:
        // the promise operations by `await`. Each arrives with the phase that
        // emits it.
        // The three `await` compiles into. They answered a NULL POINTER until
        // the runtime half existed, so a compiled `await` called address zero —
        // which is why the language layer refused an async function rather than
        // emitting one. The casts are the shape check, written out.
        RtEntry::PromiseNew => {
            rts_core::entry::promise_new as extern "C" fn() -> u64 as *const u8
        }
        RtEntry::PromiseSettle => {
            rts_core::entry::promise_settle as extern "C" fn(u64, u64, i64) as *const u8
        }
        RtEntry::PromiseAwait => {
            rts_core::entry::promise_await as extern "C" fn(u64) -> u64 as *const u8
        }
    }
}
