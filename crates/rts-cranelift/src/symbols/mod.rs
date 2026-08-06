//! Runtime entry points.
//!
//! The small, closed set of operations this layer cannot emit as instructions,
//! because they touch the heap, the operating system, or global mutable state.
//! Everything else is instructions.
//!
//! # Why this list is short, and hand-written
//!
//! The rule that decides membership is grepable: *an entry point exists if and
//! only if the operation touches the heap, the operating system, or global
//! mutable state.* Pure computation is instructions. That rule is what keeps
//! this list at a size a person can hold — allocation, the barrier's slow path,
//! collection, thread creation, the few string operations that genuinely copy,
//! handler registration, the scheduler's park and enqueue.
//!
//! At that size, an explicitly numbered list in source is the right mechanism,
//! and the same list at several hundred entries would not be — which is exactly
//! the distinction that made a generated table necessary elsewhere. A closed set
//! a reviewer can read in one screen is not the failure mode that motivated
//! generation; an open-ended one is.
//!
//! # One side needs a table, the other does not
//!
//! Compiling to an object file needs no name-to-address map at all: an undefined
//! symbol is resolved by the linker against the runtime archive, using the
//! object format's own symbol table. Only compiling into memory needs a map,
//! because there is no linker in the loop. Machinery that built one for both
//! would be solving a problem one of them does not have.
//!
//! # Declared once, and all at once
//!
//! Every entry point is declared into a module before any function is lowered,
//! and referred to afterwards by the identifier that produced, held in a dense
//! array keyed by the discriminant below. No string is hashed on the path that
//! emits code, and — because the set is closed and its signatures are
//! program-independent — nothing about lowering can add to it. That is what lets
//! lowering run on a pool without handing out a number; see [`table`].

mod table;

pub use table::{EntryImports, EntryTable};

use crate::ir::Signature;
use crate::repr::{RefKind, Repr};

/// An operation the runtime performs on the program's behalf.
///
/// Numbered explicitly. The numbers are the cache's keys, so they are a fact
/// about this list rather than about the order someone happened to write it in —
/// and a reader comparing two versions can see that an entry kept its place.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum RtEntry {
    /// Asks a heap for space, and returns a reference to it.
    ///
    /// The one memory operation that is not arithmetic. Everything else about an
    /// object — where its fields are, how to reach them — is computed; obtaining
    /// the object in the first place is not.
    Alloc = 0,

    /// Records that a store put a reference somewhere a collector must learn of.
    ///
    /// The slow path only. Whether the fast path can skip it is a question about
    /// ownership, which this layer does not yet have an answer to — see the note
    /// where the barrier is emitted.
    WriteBarrier = 1,

    /// Creates a pending promise, owned by the current region's scheduler.
    PromiseNew = 2,

    /// Settles a promise, making everything waiting on it runnable.
    ///
    /// Which waiters run where is the scheduler's business, and deliberately not
    /// visible here: a compiled program says a promise settled, and does not say
    /// what should happen next.
    PromiseSettle = 3,

    /// Parks the current frame until a promise settles.
    PromiseAwait = 4,

    /// Finds where a property sits in an object, and remembers it at the site.
    ///
    /// Called only when the site's memory of the last layout it saw does not
    /// match this object's. Refills the cell, so that the load after it reads the
    /// answer this call just wrote — which is why there is one load rather than
    /// one on each path.
    ///
    /// Reports a byte offset, or a negative number for a property that cannot be
    /// read this way: absent, or held in a form a machine word does not hold.
    CacheResolve = 6,

    /// The same question, asked by a site that is about to **write**.
    ///
    /// # Why a read and a write cannot share one answer
    ///
    /// Because they can disagree, and only about one thing: whether the runtime
    /// permits the store at all. A layout says where a property is; it does not
    /// say whether the object accepts being changed, and that is a fact this
    /// layer has no vocabulary for — nor should, since it is the client's.
    ///
    /// The alternative was a flag on `CacheResolve`. It was rejected because a
    /// flag is a value a caller supplies, and the caller here is the lowering of
    /// two different terminators: which one is asking is already known where the
    /// call is emitted, so making it a parameter turns a fact into something
    /// that can be passed wrongly. That is rule 8 — derive what a client would
    /// otherwise have to remember.
    ///
    /// Reports a byte offset, or a negative number when the store cannot be done
    /// by writing — which now includes "this object refuses stores", where the
    /// read of the same property still answers an offset.
    CacheResolveStore = 7,

    /// Throws a value that no handler in this function catches.
    ///
    /// Only for the escaping case. Where a handler *is* in this function, the
    /// destination is known while compiling and control simply goes there — the
    /// runtime is not asked a question whose answer was already computed.
    Throw = 5,
}

impl RtEntry {
    /// Every entry point, in numbering order.
    ///
    /// Listed rather than derived so that adding one is a visible edit here, and
    /// so that the tests can assert the numbering has not shifted underneath a
    /// compiled artefact.
    pub const ALL: &'static [RtEntry] = &[
        RtEntry::Alloc,
        RtEntry::WriteBarrier,
        RtEntry::PromiseNew,
        RtEntry::PromiseSettle,
        RtEntry::PromiseAwait,
        RtEntry::Throw,
        RtEntry::CacheResolve,
        RtEntry::CacheResolveStore,
    ];

    /// How many entry points exist.
    pub const COUNT: usize = Self::ALL.len();

    /// This entry point's number.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The name the runtime exports it under.
    ///
    /// A name is needed on both paths, for different reasons: to look up an
    /// address when compiling into memory, and to leave an undefined reference
    /// for the linker when compiling to an object file.
    pub fn symbol(self) -> &'static str {
        match self {
            RtEntry::Alloc => "rts_alloc",
            RtEntry::WriteBarrier => "rts_write_barrier",
            RtEntry::PromiseNew => "rts_promise_new",
            RtEntry::PromiseSettle => "rts_promise_settle",
            RtEntry::PromiseAwait => "rts_promise_await",
            RtEntry::Throw => "rts_throw",
            RtEntry::CacheResolve => "rts_cache_resolve",
            RtEntry::CacheResolveStore => "rts_cache_resolve_store",
        }
    }

    /// What it accepts and returns.
    ///
    /// Stated here rather than at call sites, for the reason every other shape in
    /// this crate is: a site that restated it could restate it wrongly, and the
    /// two would then disagree about an interface that is only checked by
    /// crashing.
    pub fn signature(self) -> Signature {
        match self {
            // Size in bytes and the type identifier the header records, in;
            // a reference out. The size is passed rather than derived from the
            // type so that the runtime does not need this layer's layout rules —
            // which would be the same rules, computed twice.
            RtEntry::Alloc => Signature {
                params: vec![Repr::I64, Repr::I64],
                returns: vec![Repr::Ref(RefKind::Opaque)],
                ..Signature::default()
            },

            // The object written into, the value written. The field's offset is
            // not passed: a collector that needs to know which field changed can
            // read the object's type, and passing it would make every barrier one
            // argument wider for the benefit of collectors that do not.
            RtEntry::WriteBarrier => Signature {
                params: vec![Repr::Ref(RefKind::Opaque), Repr::Tagged],
                returns: vec![],
                ..Signature::default()
            },

            // Nothing in, a promise out. Which scheduler owns it is decided by
            // where it was created, which the runtime knows and a compiled
            // program has no way to say.
            RtEntry::PromiseNew => Signature {
                returns: vec![Repr::Ref(RefKind::Opaque)],
                ..Signature::default()
            },

            // The promise, what it carries, and whether this is a failure. The
            // last is a value rather than two entry points because a client that
            // settles conditionally would otherwise have to branch to choose one.
            RtEntry::PromiseSettle => Signature {
                params: vec![Repr::Ref(RefKind::Opaque), Repr::Tagged, Repr::Bool],
                returns: vec![],
                ..Signature::default()
            },

            // The promise waited on; what it carried, out. A frame that parks
            // resumes here, so from the caller's side this looks like a call that
            // took a long time — which is exactly what it should look like.
            RtEntry::PromiseAwait => Signature {
                params: vec![Repr::Ref(RefKind::Opaque)],
                returns: vec![Repr::Tagged],
                ..Signature::default()
            },

            // The object, which property, and where the site keeps what it last
            // saw. The cell is passed rather than its contents because filling it
            // in is the point — a resolver that only answered would leave the
            // site as slow on the second read as on the first.
            // The same three, because it is the same question with one more
            // thing it may refuse. Written out rather than delegated to the
            // arm above: two entry points sharing a signature by construction
            // is a coupling that would silently survive one of them changing.
            RtEntry::CacheResolve | RtEntry::CacheResolveStore => Signature {
                params: vec![Repr::Ref(RefKind::Opaque), Repr::I64, Repr::I64],
                returns: vec![Repr::I64],
                convention: crate::abi::Convention::Foreign,
                ..Signature::default()
            },

            // The tag and the value. Nothing comes back: this is the escaping
            // case, so control leaves the function and does not return to it.
            RtEntry::Throw => Signature {
                params: vec![Repr::I64, Repr::Tagged],
                returns: vec![],
                ..Signature::default()
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_point_is_listed_at_its_own_number() {
        for (position, entry) in RtEntry::ALL.iter().enumerate() {
            assert_eq!(
                entry.index(),
                position,
                "the numbers are the cache's keys, so a gap or a swap is a bug"
            );
        }
    }

    #[test]
    fn no_two_entry_points_share_a_name() {
        for (i, a) in RtEntry::ALL.iter().enumerate() {
            for b in &RtEntry::ALL[i + 1..] {
                assert_ne!(a.symbol(), b.symbol());
            }
        }
    }

    #[test]
    fn the_two_resolvers_are_one_shape_asked_of_two_functions() {
        // The store lowering passes the same three operands to a different
        // symbol, which only works while the signatures agree — and the whole
        // point of the second entry is that the runtime may answer differently.
        assert_eq!(
            RtEntry::CacheResolve.signature().params,
            RtEntry::CacheResolveStore.signature().params,
            "a store passes what a read passes; a signature that drifted would \
             be a call with the wrong stack rather than a wrong answer"
        );
        assert_ne!(
            RtEntry::CacheResolve.symbol(),
            RtEntry::CacheResolveStore.symbol(),
            "one symbol for both is the flag this pair exists instead of"
        );
    }

    #[test]
    fn allocation_returns_a_reference_and_the_barrier_returns_nothing() {
        assert_eq!(RtEntry::Alloc.signature().returns.len(), 1);
        assert!(RtEntry::WriteBarrier.signature().returns.is_empty());
    }
}
