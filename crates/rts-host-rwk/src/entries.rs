//! Which runtime function an operation is, and whether the two sides agree.
//!
//! # Why this is its own module
//!
//! `run.rs` answers "how does source text become code that runs". This answers
//! a different question — "what is on the other end of a call" — and it is the
//! only place in the workspace where the compiler's statement of the
//! entry-point set and the runtime's definition of it are both visible.
//!
//! That is what makes it the right place for the check. `rts-codegen` states a
//! symbol and an ABI shape by hand because it decides *membership*, which is a
//! language judgement, and because it must be able to target **a** runtime
//! rather than **the** one in this workspace. `rts-core-rwk` derives both from
//! each Rust signature through `#[rtse::entry]`. Neither can see the other; this
//! crate can see both.

use rts_codegen::runtime::RuntimeOp;
use rts_core_rwk::entry::CoreEntry;
use rts_cranelift::abi::AbiType;
use rts_cranelift::repr::Repr;
use rts_cranelift::symbols::RtEntry;

use crate::link::HostError;

/// Which numbered entry an operation is, and where its implementation lives.
///
/// # Why a match and not a table
///
/// Because the arm that does not exist is the point. `RuntimeOp` is stated in
/// `rts-codegen` and the implementations are exported by `rts-core-rwk`, and the
/// two are written independently — so this is where a disagreement between them
/// becomes a refusal instead of a call to whatever the linker happened to find.
///
/// Adding an operation to the compiler without adding it to the runtime fails
/// here, by name, at the moment a program first needs it.
///
/// # Why one match and not two
///
/// It was two: one answering the address and one answering the numbered entry,
/// side by side in this file, twenty-eight arms each. Both answer the same
/// question — *what is on the other end of this call* — and an arm added to one
/// and not the other is a compile error only for the address, because the other
/// is what `agree` reads and `agree` would simply not be reached.
///
/// The two answers also belong together for a reason that is not tidiness: the
/// cast in each arm IS the shape check, and the entry beside it is what that
/// shape gets compared against. Splitting them put the claim and its evidence
/// in different functions.
pub(crate) fn resolve(op: RuntimeOp) -> (CoreEntry, *const u8) {
    // Each cast names the ABI shape the compiled program was built to expect.
    // Writing it out is what makes a signature change on either side a type
    // error here rather than a corrupt call.
    match op {
        RuntimeOp::Add => (
            CoreEntry::Add,
            rts_core_rwk::entry::add as extern "C" fn(u64, u64) -> u64 as *const u8,
        ),
        RuntimeOp::StrictEquals => (CoreEntry::StrictEquals, {
            rts_core_rwk::entry::strict_equals as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::ToBoolean => (CoreEntry::ToBoolean, {
            rts_core_rwk::entry::to_boolean as extern "C" fn(u64) -> bool as *const u8
        }),
        RuntimeOp::NumberToString => (CoreEntry::NumberToString, {
            rts_core_rwk::entry::number_to_string as extern "C" fn(f64) -> u64 as *const u8
        }),
        RuntimeOp::Subtract => (CoreEntry::Subtract, {
            rts_core_rwk::entry::subtract as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Multiply => (CoreEntry::Multiply, {
            rts_core_rwk::entry::multiply as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Divide => (CoreEntry::Divide, {
            rts_core_rwk::entry::divide as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Remainder => (CoreEntry::Remainder, {
            rts_core_rwk::entry::remainder as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Less => (CoreEntry::Less, {
            rts_core_rwk::entry::less as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::LessEqual => (CoreEntry::LessEqual, {
            rts_core_rwk::entry::less_equal as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::Greater => (CoreEntry::Greater, {
            rts_core_rwk::entry::greater as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::GreaterEqual => (CoreEntry::GreaterEqual, {
            rts_core_rwk::entry::greater_equal as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::ObjectNew => (CoreEntry::ObjectNew, {
            rts_core_rwk::entry::object_new as extern "C" fn() -> u64 as *const u8
        }),
        RuntimeOp::GetProperty => (CoreEntry::GetProperty, {
            rts_core_rwk::entry::get_property as extern "C" fn(u64, i64) -> u64 as *const u8
        }),
        RuntimeOp::SetProperty => (CoreEntry::SetProperty, {
            rts_core_rwk::entry::set_property as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ClosureNew => (CoreEntry::ClosureNew, {
            rts_core_rwk::entry::closure_new as extern "C" fn(i64, u64) -> u64 as *const u8
        }),
        // The cast is the arity agreement, written out. Six parameters: the
        // callee, the receiver, and `ARGUMENT_SLOTS` arguments — and the
        // assertion below is what makes that sentence checkable rather than a
        // comment that was true once.
        RuntimeOp::Call => (CoreEntry::Call, {
            rts_core_rwk::entry::call as extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64
                as *const u8
        }),
        // The argument is which literal, not the text: an `i64` index into the
        // table the run seeds. Writing the cast out is what makes a change to
        // that decision a type error here.
        RuntimeOp::StringConst => (CoreEntry::StringConst, {
            rts_core_rwk::entry::string_const as extern "C" fn(i64) -> u64 as *const u8
        }),
        // The argument is which SITE, not a value: the pieces are compile-time
        // constants and the object is built once, so what crosses is a number.
        RuntimeOp::TemplateStrings => (CoreEntry::TemplateStrings, {
            rts_core_rwk::entry::template_strings as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::ModuleBinding => (CoreEntry::ModuleBinding, {
            rts_core_rwk::entry::module_binding as extern "C" fn(i64, i64) -> u64 as *const u8
        }),
        RuntimeOp::ModuleNamespace => (CoreEntry::ModuleNamespace, {
            rts_core_rwk::entry::module_namespace as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::TypeOf => (CoreEntry::TypeOf, {
            rts_core_rwk::entry::type_of as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::LooseEquals => (CoreEntry::LooseEquals, {
            rts_core_rwk::entry::loose_equals as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::Exponent => (CoreEntry::Exponent, {
            rts_core_rwk::entry::exponent as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitAnd => (CoreEntry::BitAnd, {
            rts_core_rwk::entry::bit_and as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitOr => (CoreEntry::BitOr, {
            rts_core_rwk::entry::bit_or as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitXor => (CoreEntry::BitXor, {
            rts_core_rwk::entry::bit_xor as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitNot => (CoreEntry::BitNot, {
            rts_core_rwk::entry::bit_not as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::ShiftLeft => (CoreEntry::ShiftLeft, {
            rts_core_rwk::entry::shift_left as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ShiftRight => (CoreEntry::ShiftRight, {
            rts_core_rwk::entry::shift_right as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ShiftRightUnsigned => (CoreEntry::ShiftRightUnsigned, {
            rts_core_rwk::entry::shift_right_unsigned as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GetIndexed => (CoreEntry::GetIndexed, {
            rts_core_rwk::entry::get_indexed as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::SetIndexed => (CoreEntry::SetIndexed, {
            rts_core_rwk::entry::set_indexed as extern "C" fn(u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::HasProperty => (CoreEntry::HasProperty, {
            rts_core_rwk::entry::has_property as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::ArrayNew => (CoreEntry::ArrayNew, {
            rts_core_rwk::entry::array_new as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::DeleteProperty => (CoreEntry::DeleteProperty, {
            rts_core_rwk::entry::delete_property as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::OwnKeys => (CoreEntry::OwnKeys, {
            rts_core_rwk::entry::own_keys as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::Construct => (CoreEntry::Construct, {
            rts_core_rwk::entry::construct as extern "C" fn(u64, u64, u64, u64, u64) -> u64
                as *const u8
        }),
        RuntimeOp::InstanceOf => (CoreEntry::InstanceOf, {
            rts_core_rwk::entry::instance_of as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::RegexNew => (CoreEntry::RegexNew, {
            rts_core_rwk::entry::regex_new as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BigIntNew => (CoreEntry::BigIntNew, {
            rts_core_rwk::entry::bigint_new as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::Negate => (CoreEntry::Negate, {
            rts_core_rwk::entry::negate as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::GetPrototype => (CoreEntry::GetPrototype, {
            rts_core_rwk::entry::get_prototype as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::SetPrototype => (CoreEntry::SetPrototype, {
            rts_core_rwk::entry::set_prototype as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::SuperConstruct => (CoreEntry::SuperConstruct, {
            rts_core_rwk::entry::super_construct
                as extern "C" fn(u64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Iterate => (CoreEntry::Iterate, {
            rts_core_rwk::entry::iterate as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::ArrayAppend => (CoreEntry::ArrayAppend, {
            rts_core_rwk::entry::array_append as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ArrayAppendAll => (CoreEntry::ArrayAppendAll, {
            rts_core_rwk::entry::array_append_all as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ConstructWithArgs => (CoreEntry::ConstructWithArgs, {
            rts_core_rwk::entry::construct_with_args as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::CallWithArgs => (CoreEntry::CallWithArgs, {
            rts_core_rwk::entry::call_with_args as extern "C" fn(u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::RestArguments => (CoreEntry::RestArguments, {
            rts_core_rwk::entry::rest_arguments
                as extern "C" fn(i64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::MarkDerived => (CoreEntry::MarkDerived, {
            rts_core_rwk::entry::mark_derived as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::DefineGetter => (CoreEntry::DefineGetter, {
            rts_core_rwk::entry::define_getter as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::DefineSetter => (CoreEntry::DefineSetter, {
            rts_core_rwk::entry::define_setter as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GlobalSet => (CoreEntry::GlobalSet, {
            rts_core_rwk::entry::global_set as extern "C" fn(i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GlobalGet => (CoreEntry::GlobalGet, {
            rts_core_rwk::entry::global_get as extern "C" fn(i64) -> u64 as *const u8
        }),
    }
}

/// The compiler and the runtime describe an operation the same way.
///
/// Checked for every operation a compilation actually declared, before anything
/// is placed. Both halves matter and they fail differently: a symbol skew is a
/// missing symbol at placement, which is loud, and a **shape** skew is a call
/// laid out one way and read another, which is not.
pub(crate) fn agree(op: RuntimeOp) -> Result<(), HostError> {
    let described = resolve(op).0.describe();
    if op.symbol() != described.symbol {
        return Err(HostError::Malformed(format!(
            "the compiler calls {:?} `{}` and the runtime defines `{}`",
            op,
            op.symbol(),
            described.symbol
        )));
    }
    // Compared position by position rather than as whole signatures, because
    // the two layers speak different vocabularies for the same fact: the IR
    // says `Repr`, the ABI says `AbiType`, and an entry point's parameters are
    // all scalars. A descriptor holding an aggregate or a slice here is not a
    // mismatch to report — it is an entry point the compiler cannot call at
    // all, so it is named separately.
    let shape = describe(op.signature().params, described.params)
        .and_then(|()| describe(op.signature().returns, described.returns));
    if let Err(reason) = shape {
        return Err(HostError::Malformed(format!(
            "the compiler and the runtime disagree about the shape of `{}`: {reason}",
            op.symbol(),
        )));
    }
    Ok(())
}

/// One side's representations against the other's ABI types.
fn describe(ours: Vec<Repr>, theirs: &[AbiType]) -> Result<(), String> {
    if ours.len() != theirs.len() {
        return Err(format!("{} positions against {}", ours.len(), theirs.len()));
    }
    for (at, (ours, theirs)) in ours.iter().zip(theirs).enumerate() {
        match theirs {
            AbiType::Scalar(theirs) if theirs == ours => {}
            AbiType::Scalar(theirs) => {
                return Err(format!("position {at} is {ours:?} against {theirs:?}"));
            }
            other => {
                return Err(format!(
                    "position {at} is {ours:?} against {other:?}, which is not a \
                     scalar and so is not something a call site can lay out"
                ));
            }
        }
    }
    Ok(())
}

/// The two sides agree about how many arguments a compiled call carries.
///
/// `rts-codegen` decides it, because which convention compiled code uses is a
/// fact about what that crate emits. `rts-core-rwk` restates it, because it is
/// what performs the call. Neither can see the other, and this crate is the one
/// that may name both — so this is where a disagreement becomes a refusal.
///
/// A `const` assertion rather than a test: a test that is not run proves
/// nothing, and this one cannot fail to be checked because the crate does not
/// compile without it.
const _: () = assert!(
    rts_codegen::runtime::ARGUMENT_SLOTS == rts_core_rwk::entry::ARGUMENT_SLOTS,
    "the compiler and the runtime disagree about how many arguments a call \
     carries, which is a jump with a corrupt stack rather than a wrong answer"
);

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
        RtEntry::Alloc => rts_core_rwk::entry::alloc as extern "C" fn(i64, i64) -> u64 as *const u8,
        RtEntry::CacheResolve => {
            rts_core_rwk::entry::cache_resolve as extern "C" fn(u64, i64, i64) -> i64 as *const u8
        }
        // A store asks a different resolver, which refuses for an object that
        // refuses to be written. Same signature, and deliberately not the same
        // function: a READ of a frozen object still resolves to its offset.
        RtEntry::CacheResolveStore => {
            rts_core_rwk::entry::cache_resolve_store as extern "C" fn(u64, i64, i64) -> i64
                as *const u8
        }
        RtEntry::WriteBarrier => {
            rts_core_rwk::entry::write_barrier as extern "C" fn(u64, u64) as *const u8
        }
        // Reached only when nothing in the throwing function caught it, which
        // ends the program with the value reported. A handler one frame up is
        // the case the language layer refuses by name rather than compiling
        // into a `catch` that would silently never run.
        RtEntry::Throw => rts_core_rwk::entry::throw as extern "C" fn(i64, u64) as *const u8,
        // The rest are emitted by instructions this compiler does not produce:
        // the promise operations by `await`. Each arrives with the phase that
        // emits it.
        RtEntry::PromiseNew | RtEntry::PromiseSettle | RtEntry::PromiseAwait => std::ptr::null(),
    }
}

#[cfg(test)]
mod tests {
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
    fn the_two_lists_are_the_same_length() {
        // `entry_of` is exhaustive over `RuntimeOp`, so an operation added to
        // the compiler cannot be forgotten here. The other direction has no
        // such check: an entry point added to the runtime and never named by
        // the language is legal — it is simply unused — but the counts being
        // equal is what says that is not the case today, so a divergence is
        // visible rather than assumed.
        assert_eq!(
            RuntimeOp::ALL.len(),
            rts_core_rwk::entry::CORE_ENTRY_COUNT,
            "the compiler names {} operations and the runtime numbers {}",
            RuntimeOp::ALL.len(),
            rts_core_rwk::entry::CORE_ENTRY_COUNT
        );
    }
}
