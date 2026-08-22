//! Whether the compiler's statement of the entry-point set and the runtime's
//! definition of it agree.
//!
//! # Why these are here and not beside the code
//!
//! `entries.rs` was 580 lines against this crate's 500-line ceiling — and it was
//! already 530 before these tests were added, so this is not a violation these
//! introduced, it is one they made visible. Rule 6 calls a growing glue file the
//! symptom of a semantic decision drifting in. Here it is not that: the file is
//! one exhaustive `match` over `RuntimeOp` whose length IS the entry-point set,
//! so it cannot be split by subject without splitting that set.
//!
//! What CAN move is the checking, which is a different job from the mapping —
//! `resolve` answers "what is on the other end of this call" and this answers
//! "do the two sides agree about it".

use super::*;


#[test]
fn every_operation_the_compiler_names_is_the_one_the_runtime_defines() {
    // Checked for ALL of them rather than for what some program happened to
    // call, because the failure this catches is in the operation nobody
    // exercised yet: the compiler states a symbol and a shape by hand, the
    // runtime derives both from a Rust signature, and until this existed
    // nothing compared them.
    //
    // A symbol skew is loud — a missing symbol at placement. A shape skew is
    // not: the call is laid out to the compiler's answer and the callee
    // reads its arguments to a different one, which is a corrupt call that
    // links and runs.
    for op in RuntimeOp::ALL {
        agree(*op).unwrap_or_else(|error| {
            panic!("{op:?} does not match what the runtime defines: {error:?}")
        });
    }
}

#[test]
fn every_operation_exempted_from_the_throw_check_still_exists() {
    // `rts-codegen`'s `runtime::raising` states which entry points cannot
    // record a throw, and the emitter omits the load/compare/branch after
    // them on the strength of it. That claim is about a `rts-core` BODY, and
    // `rts-codegen` cannot see one — rule 1 of its README forbids the
    // dependency. This crate can see both, which is rule 2 of this one.
    //
    // What this checks is the half a machine can check: that each exempt
    // operation still resolves to a runtime function and still agrees about
    // its shape. That catches the exemption outliving what it named — an
    // operation renamed, re-pointed at a different implementation, or given
    // a new signature — which is the mechanical way this list goes stale.
    //
    // What it CANNOT check is the half that matters most: whether the body
    // on the other end still refrains from reaching `entry::throw`. Nothing
    // here can read a call graph. That is why each entry in `CANNOT_RAISE`
    // names the file it was read against and why the list is short —
    // re-verification is a human reading eight bodies, and the list is sized
    // to make that cheap enough to actually happen.
    for op in rts_codegen::runtime::CANNOT_RAISE {
        agree(*op).unwrap_or_else(|error| {
            panic!(
                "{op:?} is exempt from the throw check but no longer agrees \
                 with the runtime: {error:?} — re-read its rts-core body \
                 before deciding whether the exemption still holds"
            )
        });
    }
}

#[test]
fn the_check_itself_is_not_claimed_to_be_unable_to_raise() {
    // The two lists in `runtime::raising` are exempt for different reasons
    // and must not merge: `Thrown` and `TakeThrown` skip the check because
    // asking after them recurses, NOT because a throw cannot be in flight
    // when they run — one of them exists precisely to collect it.
    //
    // If either ever appeared in `CANNOT_RAISE`, a future reader would
    // reasonably conclude their bodies had been verified closed. They have
    // not been.
    for op in rts_codegen::runtime::IS_THE_CHECK {
        assert!(
            !rts_codegen::runtime::CANNOT_RAISE.contains(op),
            "{op:?} is in both lists — see rts-codegen's runtime::raising \
             for why the two exemptions are not the same claim"
        );
    }
}

#[test]
fn the_two_lists_are_the_same_length() {
    // `entry_of` is exhaustive over `RuntimeOp`, so an operation added to
    // the compiler cannot be forgotten here. The other direction has no
    // such check: an entry point added to the runtime and never named by
    // the language is legal — it is simply unused — but the counts being
    // equal is what says that is not the case today, so a divergence is
    // visible rather than assumed.
    assert_eq!(
        RuntimeOp::ALL.len(),
        rts_core::entry::CORE_ENTRY_COUNT,
        "the compiler names {} operations and the runtime numbers {}",
        RuntimeOp::ALL.len(),
        rts_core::entry::CORE_ENTRY_COUNT
    );
}
