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
//! rather than **the** one in this workspace. `rts-core` derives both from
//! each Rust signature through `#[rtse::entry]`. Neither can see the other; this
//! crate can see both.

use rts_codegen::runtime::RuntimeOp;
use rts_core::entry::CoreEntry;
use rts_cranelift::abi::AbiType;
use rts_cranelift::repr::Repr;
use rts_cranelift::symbols::RtEntry;

use crate::link::HostError;

/// Which numbered entry an operation is, and where its implementation lives.
///
/// # Why a match and not a table
///
/// Because the arm that does not exist is the point. `RuntimeOp` is stated in
/// `rts-codegen` and the implementations are exported by `rts-core`, and the
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
            rts_core::entry::add as extern "C" fn(u64, u64) -> u64 as *const u8,
        ),
        RuntimeOp::StrictEquals => (CoreEntry::StrictEquals, {
            rts_core::entry::strict_equals as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::ToBoolean => (CoreEntry::ToBoolean, {
            rts_core::entry::to_boolean as extern "C" fn(u64) -> bool as *const u8
        }),
        RuntimeOp::NumberToString => (CoreEntry::NumberToString, {
            rts_core::entry::number_to_string as extern "C" fn(f64) -> u64 as *const u8
        }),
        RuntimeOp::Subtract => (CoreEntry::Subtract, {
            rts_core::entry::subtract as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Multiply => (CoreEntry::Multiply, {
            rts_core::entry::multiply as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Divide => (CoreEntry::Divide, {
            rts_core::entry::divide as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Remainder => (CoreEntry::Remainder, {
            rts_core::entry::remainder as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        // `f64` in and `f64` out, where the row above is `u64` both ways. The
        // cast is the check: the two entries compute the same arithmetic and
        // differ only in shape, so a mix-up would be invisible everywhere
        // except here.
        RuntimeOp::NumberRemainder => (CoreEntry::NumberRemainder, {
            rts_core::entry::number_remainder as extern "C" fn(f64, f64) -> f64 as *const u8
        }),
        RuntimeOp::Less => (CoreEntry::Less, {
            rts_core::entry::less as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::LessEqual => (CoreEntry::LessEqual, {
            rts_core::entry::less_equal as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::Greater => (CoreEntry::Greater, {
            rts_core::entry::greater as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::GreaterEqual => (CoreEntry::GreaterEqual, {
            rts_core::entry::greater_equal as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::TemplateJoin => (CoreEntry::TemplateJoin, {
            rts_core::entry::template_join
                as extern "C" fn(i64, i64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::MathRandom => (CoreEntry::MathRandom, {
            rts_core::entry::math_random as extern "C" fn() -> f64 as *const u8
        }),
        RuntimeOp::StringOf => (CoreEntry::StringOf, {
            rts_core::entry::string_of as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::ArrayOf => (CoreEntry::ArrayOf, {
            rts_core::entry::array_of
                as extern "C" fn(i64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ObjectNew => (CoreEntry::ObjectNew, {
            rts_core::entry::object_new as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::GetProperty => (CoreEntry::GetProperty, {
            rts_core::entry::get_property as extern "C" fn(u64, i64) -> u64 as *const u8
        }),
        RuntimeOp::SetProperty => (CoreEntry::SetProperty, {
            rts_core::entry::set_property as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ClosureNew => (CoreEntry::ClosureNew, {
            rts_core::entry::closure_new as extern "C" fn(i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GeneratorNew => (CoreEntry::GeneratorNew, {
            rts_core::entry::generator_new
                as extern "C" fn(i64, u64, u64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::AsyncStart => (CoreEntry::AsyncStart, {
            rts_core::entry::async_start
                as extern "C" fn(i64, u64, u64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GeneratorYield => (CoreEntry::GeneratorYield, {
            rts_core::entry::generator_yield as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::DelegateStep => (CoreEntry::DelegateStep, {
            rts_core::entry::delegate_step as extern "C" fn(u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ModulePublishAll => (CoreEntry::ModulePublishAll, {
            rts_core::entry::module_publish_all as extern "C" fn(i64, i64) -> u64 as *const u8
        }),
        // The two a throw crosses a frame on. Emitted after every operation that
        // can raise one, so the frame above learns that the frame below left by
        // throwing — which is what a `try` protecting a call needs and what the
        // language refused for want of.
        RuntimeOp::Thrown => (CoreEntry::Thrown, {
            rts_core::entry::thrown as extern "C" fn() -> i64 as *const u8
        }),
        RuntimeOp::ElementAt => (CoreEntry::ElementAt, {
            rts_core::entry::element_at as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ElementsBase => (CoreEntry::ElementsBase, {
            rts_core::entry::elements_base as extern "C" fn(u64) -> i64 as *const u8
        }),
        RuntimeOp::ThrownAddress => (CoreEntry::ThrownAddress, {
            rts_core::entry::thrown_address as extern "C" fn() -> i64 as *const u8
        }),
        RuntimeOp::TakeThrown => (CoreEntry::TakeThrown, {
            rts_core::entry::take_thrown as extern "C" fn() -> u64 as *const u8
        }),
        RuntimeOp::RunningFunction => (CoreEntry::RunningFunction, {
            rts_core::entry::running_function as extern "C" fn() -> u64 as *const u8
        }),
        RuntimeOp::EvalDirect => (CoreEntry::EvalDirect, {
            rts_core::entry::eval_direct as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ObjectSpread => (CoreEntry::ObjectSpread, {
            rts_core::entry::object_spread as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::KeyNumber => (CoreEntry::KeyNumber, {
            rts_core::entry::key_number as extern "C" fn(u64) -> i64 as *const u8
        }),
        // The cast is the arity agreement, written out. Six parameters: the
        // callee, the receiver, and `ARGUMENT_SLOTS` arguments — and the
        // assertion below is what makes that sentence checkable rather than a
        // comment that was true once.
        RuntimeOp::Call => (CoreEntry::Call, {
            rts_core::entry::call_counted
                as extern "C" fn(u64, u64, i64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        // The argument is which literal, exactly as `StringConst`'s is: an
        // `i64` index into the table the run seeds, not the text itself.
        RuntimeOp::SetCallName => (CoreEntry::SetCallName, {
            rts_core::entry::set_call_name as extern "C" fn(i64) -> u64 as *const u8
        }),
        // The argument is which literal, not the text: an `i64` index into the
        // table the run seeds. Writing the cast out is what makes a change to
        // that decision a type error here.
        RuntimeOp::StringConst => (CoreEntry::StringConst, {
            rts_core::entry::string_const as extern "C" fn(i64) -> u64 as *const u8
        }),
        // The argument is which SITE, not a value: the pieces are compile-time
        // constants and the object is built once, so what crosses is a number.
        RuntimeOp::TemplateStrings => (CoreEntry::TemplateStrings, {
            rts_core::entry::template_strings as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::ModuleBinding => (CoreEntry::ModuleBinding, {
            rts_core::entry::module_binding as extern "C" fn(i64, i64) -> u64 as *const u8
        }),
        RuntimeOp::ModuleNamespace => (CoreEntry::ModuleNamespace, {
            rts_core::entry::module_namespace as extern "C" fn(i64) -> u64 as *const u8
        }),
        // Which literal holds the asking module's own specifier, exactly as
        // `ModulePublish`'s first argument is.
        RuntimeOp::ImportMeta => (CoreEntry::ImportMeta, {
            rts_core::entry::import_meta as extern "C" fn(i64) -> u64 as *const u8
        }),
        // The specifier is a VALUE and the referrer a literal index, and the
        // cast is where that pair is checked rather than assumed.
        RuntimeOp::ModuleImport => (CoreEntry::ModuleImport, {
            rts_core::entry::module_import as extern "C" fn(u64, i64) -> u64 as *const u8
        }),
        RuntimeOp::ModulePublish => (CoreEntry::ModulePublish, {
            rts_core::entry::module_publish as extern "C" fn(i64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::TypeOf => (CoreEntry::TypeOf, {
            rts_core::entry::type_of as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::LooseEquals => (CoreEntry::LooseEquals, {
            rts_core::entry::loose_equals as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::Exponent => (CoreEntry::Exponent, {
            rts_core::entry::exponent as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitAnd => (CoreEntry::BitAnd, {
            rts_core::entry::bit_and as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitOr => (CoreEntry::BitOr, {
            rts_core::entry::bit_or as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitXor => (CoreEntry::BitXor, {
            rts_core::entry::bit_xor as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BitNot => (CoreEntry::BitNot, {
            rts_core::entry::bit_not as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::ShiftLeft => (CoreEntry::ShiftLeft, {
            rts_core::entry::shift_left as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ShiftRight => (CoreEntry::ShiftRight, {
            rts_core::entry::shift_right as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ShiftRightUnsigned => (CoreEntry::ShiftRightUnsigned, {
            rts_core::entry::shift_right_unsigned as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GetIndexed => (CoreEntry::GetIndexed, {
            rts_core::entry::get_indexed as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::SetIndexed => (CoreEntry::SetIndexed, {
            rts_core::entry::set_indexed as extern "C" fn(u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::HasProperty => (CoreEntry::HasProperty, {
            rts_core::entry::has_property as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::WithHas => (CoreEntry::WithHas, {
            rts_core::entry::with_has as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::ArrayNew => (CoreEntry::ArrayNew, {
            rts_core::entry::array_new as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::DeleteProperty => (CoreEntry::DeleteProperty, {
            rts_core::entry::delete_property as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::OwnKeys => (CoreEntry::OwnKeys, {
            rts_core::entry::own_keys as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::EnumerateKeys => (CoreEntry::EnumerateKeys, {
            rts_core::entry::enumerate_keys as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::DefineMethod => (CoreEntry::DefineMethod, {
            rts_core::entry::define_method as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::NewTarget => (CoreEntry::NewTarget, {
            rts_core::entry::new_target as extern "C" fn() -> u64 as *const u8
        }),
        RuntimeOp::Construct => (CoreEntry::Construct, {
            rts_core::entry::construct as extern "C" fn(u64, u64, u64, u64, u64) -> u64
                as *const u8
        }),
        RuntimeOp::InstanceOf => (CoreEntry::InstanceOf, {
            rts_core::entry::instance_of as extern "C" fn(u64, u64) -> bool as *const u8
        }),
        RuntimeOp::RegexNew => (CoreEntry::RegexNew, {
            rts_core::entry::regex_new as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::BigIntNew => (CoreEntry::BigIntNew, {
            rts_core::entry::bigint_new as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::Negate => (CoreEntry::Negate, {
            rts_core::entry::negate as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::GetSuperProperty => (CoreEntry::GetSuperProperty, {
            rts_core::entry::get_super_property
                as extern "C" fn(u64, u64, i64) -> u64 as *const u8
        }),
        RuntimeOp::SetSuperProperty => (CoreEntry::SetSuperProperty, {
            rts_core::entry::set_super_property
                as extern "C" fn(u64, u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GetPrototype => (CoreEntry::GetPrototype, {
            rts_core::entry::get_prototype as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::SetPrototype => (CoreEntry::SetPrototype, {
            rts_core::entry::set_prototype as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::SuperConstruct => (CoreEntry::SuperConstruct, {
            rts_core::entry::super_construct
                as extern "C" fn(u64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::Iterate => (CoreEntry::Iterate, {
            rts_core::entry::iterate as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::ArrayAppend => (CoreEntry::ArrayAppend, {
            rts_core::entry::array_append as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ArrayAppendAll => (CoreEntry::ArrayAppendAll, {
            rts_core::entry::array_append_all as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ConstructWithArgs => (CoreEntry::ConstructWithArgs, {
            rts_core::entry::construct_with_args as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::SuperConstructWithArgs => (CoreEntry::SuperConstructWithArgs, {
            rts_core::entry::super_construct_with_args
                as extern "C" fn(u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::CallWithArgs => (CoreEntry::CallWithArgs, {
            rts_core::entry::call_with_args as extern "C" fn(u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::RestArguments => (CoreEntry::RestArguments, {
            rts_core::entry::rest_arguments
                as extern "C" fn(i64, u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::ArgumentsObject => (CoreEntry::ArgumentsObject, {
            rts_core::entry::arguments_object
                as extern "C" fn(u64, u64, u64, u64) -> u64 as *const u8
        }),
        RuntimeOp::MarkDerived => (CoreEntry::MarkDerived, {
            rts_core::entry::mark_derived as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::MarkClassConstructor => (CoreEntry::MarkClassConstructor, {
            rts_core::entry::mark_class_constructor as extern "C" fn(u64) -> u64 as *const u8
        }),
        RuntimeOp::DefineGetter => (CoreEntry::DefineGetter, {
            rts_core::entry::define_getter as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::DefineSetter => (CoreEntry::DefineSetter, {
            rts_core::entry::define_setter as extern "C" fn(u64, i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GlobalSet => (CoreEntry::GlobalSet, {
            rts_core::entry::global_set as extern "C" fn(i64, u64) -> u64 as *const u8
        }),
        RuntimeOp::GlobalGet => (CoreEntry::GlobalGet, {
            rts_core::entry::global_get as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::UnboundGlobalGet => (CoreEntry::GlobalGetUnbound, {
            rts_core::entry::global_get_unbound as extern "C" fn(i64) -> u64 as *const u8
        }),
        RuntimeOp::SloppyThis => (CoreEntry::SloppyThis, {
            rts_core::entry::sloppy_this as extern "C" fn(u64) -> u64 as *const u8
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
/// fact about what that crate emits. `rts-core` restates it, because it is
/// what performs the call. Neither can see the other, and this crate is the one
/// that may name both — so this is where a disagreement becomes a refusal.
///
/// A `const` assertion rather than a test: a test that is not run proves
/// nothing, and this one cannot fail to be checked because the crate does not
/// compile without it.
const _: () = assert!(
    rts_codegen::runtime::ARGUMENT_SLOTS == rts_core::entry::ARGUMENT_SLOTS,
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


#[cfg(test)]
mod agreement;
