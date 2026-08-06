//! Lowering: this crate's representation, in the code generator's.
//!
//! **This is the only module permitted to construct code-generator
//! instructions.** Every other module in the crate, and every other crate in the
//! workspace, manipulates the representation instead. That is not a style rule:
//! if a client can reach one layer past the boundary it will, precisely in the
//! cases that matter, and the decisions are then back at the call sites — which
//! is the disease this crate was built to cure.
//!
//! # What lowering does not do
//!
//! It does not decide anything. Every choice was made before it ran: what
//! representation a value has, where a barrier goes, which values are live at a
//! point that can collect, whether a tail call is legal. Lowering translates.
//! When it is tempted to choose, a capability is missing upstream and the fix
//! belongs there.
//!
//! # What it refuses
//!
//! What cannot be emitted is refused by name, with the capability it needs, so a
//! program that needs it fails visibly instead of producing code that is quietly
//! incomplete.
//!
//! Allocation is worth separating from the rest of memory. Reading and writing a
//! field is arithmetic and lands here; asking a heap for space is a runtime entry
//! point, and a runtime entry point is something to declare rather than something
//! to emit.
//!
//! A bare suspension is refused, and correctly: it is rewritten away by the frame
//! transformation before lowering should ever see one, so reaching here means
//! that rewrite did not run.
//!
//! One further refusal is a finding rather than a gap, documented where it is
//! raised: a 64-bit integer cannot be widened without a heap box.
//!
//! A second used to sit beside it — that a guard cannot establish which kind of
//! reference a value holds, because the encoding does not carry one. That is
//! still true of the encoding, and is no longer a limit: the kind is in the
//! object, so a type guard reads it there. The two guards compose.

mod body;
mod cleanup;
mod error;
mod memory;
mod terminator;
mod types;
mod value;

pub use body::Outside;
pub use error::{Capability, LowerError};
pub use memory::Heap;
pub use types::{is_word, machine_type};

use cranelift_codegen::ir::{AbiParam, UserFuncName};

use crate::repr::Repr;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use crate::ir::{Function, Signature};

/// Which discipline the code generator should use for one of our conventions.
///
/// The three we declare were collapsing into one here, which meant a function
/// that said it permitted tail calls was compiled under a convention that does
/// not — and the code generator rejected the tail call, correctly, with a
/// message about a convention nothing in our vocabulary had chosen.
///
/// The stable convention is the target's, because that is what "stable across a
/// library boundary" means and it is not ours to pick. The other two are ours:
/// one for speed between functions we own, one that permits replacing a frame.
pub fn machine_call_conv(
    convention: crate::abi::Convention,
    target_default: cranelift_codegen::isa::CallConv,
) -> cranelift_codegen::isa::CallConv {
    use cranelift_codegen::isa::CallConv;

    match convention {
        // The target's, not the code generator's "fast" one. That exists, and it
        // is explicitly not stable across a version of the compiler — so using it
        // buys an unmeasured speedup in exchange for a convention that can change
        // underneath compiled code, and for internal functions no longer being
        // callable from the host that compiled them.
        //
        // Rule twelve of this crate: unproven behaviour fails safely, and raising
        // it is explicit. Nothing has measured a difference yet. When something
        // does, this is the line to change, and the cost above is what it costs.
        crate::abi::Convention::Internal => target_default,
        crate::abi::Convention::InternalTail => CallConv::Tail,
        crate::abi::Convention::Foreign => target_default,
    }
}

/// The code generator's signature for one of ours.
///
/// Takes the target's stable convention rather than the one to use: which one to
/// use follows from what the signature says about itself, and a caller passing it
/// separately is a caller who can pass the wrong one.
pub fn machine_signature(
    signature: &Signature,
    target_default: cranelift_codegen::isa::CallConv,
) -> cranelift_codegen::ir::Signature {
    let call_conv = machine_call_conv(signature.convention, target_default);
    let mut lowered = cranelift_codegen::ir::Signature::new(call_conv);
    lowered.params.extend(
        signature
            .params
            .iter()
            .map(|&r| AbiParam::new(machine_type(r))),
    );
    let foreign = signature.convention == crate::abi::Convention::Foreign;
    lowered.returns.extend(
        signature
            .returns
            .iter()
            .map(|&r| AbiParam::new(abi_return_type(r, foreign))),
    );
    lowered
}

/// The machine type a **returned** representation actually occupies.
///
/// # Why this is not [`machine_type`]
///
/// Because a boolean crossing a stable boundary is one byte, and saying
/// otherwise is a claim about the callee that the callee does not honour. The C
/// convention this crate targets defines only the low byte of the return
/// register for a one-byte type; the rest is whatever the callee last had there.
///
/// So a foreign signature returning [`Repr::Bool`] declares `I8`, and the call
/// site extends. Declaring `I64` reads the undefined bits as part of the value —
/// and it did: `strict_equals` answered *true* for two different strings in an
/// optimised build and false in an unoptimised one, because the register happened
/// to be clean in the second. Nothing rejected it, because nothing had run the
/// optimised build.
///
/// # Why the representation is still a word everywhere else
///
/// A value inside this crate's own IR occupies a word, for the reason
/// [`machine_type`] gives — every boundary it has moves words. This is the one
/// place the *other* side of a call has an opinion, so it is the one place that
/// differs, rather than narrowing booleans everywhere and re-widening them.
///
/// # What was verified and what was not
///
/// Read from the code generator's own ABI lowering: a return declared `I8` is
/// taken from the low byte of the return register on x86-64 and AArch64. Not
/// verified: the same question for a `Bool` **parameter**, which is why the
/// verifier refuses one on a foreign signature rather than this function
/// guessing — rule 12.
fn abi_return_type(repr: Repr, foreign: bool) -> cranelift_codegen::ir::Type {
    match (repr, foreign) {
        (Repr::Bool, true) => cranelift_codegen::ir::types::I8,
        _ => machine_type(repr),
    }
}

/// What a function is allowed to reach beyond itself.
///
/// Every field is a shared reference, so this is `Send` and `Sync` whenever its
/// contents are — which is what makes lowering a whole program at once on a pool
/// possible at all.
///
/// Both halves are optional and independent: a function can read memory without
/// calling anything, and can call without touching memory. One flag covering
/// both would make each unavailable whenever the other is.
#[derive(Default)]
pub struct Environment<'a> {
    /// The module this function may name things in.
    pub outside: Option<Outside<'a>>,
    /// The heap this function may read and write.
    pub heap: Option<Heap<'a>>,
}

/// Lowers one function in the environment it is allowed to reach.
///
/// Takes a verified function. Lowering re-checks only what it cannot proceed
/// without — a block that does not end, a field that does not exist — and trusts
/// the rest, because checking twice in two vocabularies is how the two come to
/// disagree about which is authoritative.
pub fn lower_in<'a>(
    func: &'a Function,
    target_default: cranelift_codegen::isa::CallConv,
    environment: Environment<'a>,
) -> Result<Lowered, LowerError> {
    let signature = machine_signature(&func.signature, target_default);
    let mut lowered =
        cranelift_codegen::ir::Function::with_name_signature(UserFuncName::default(), signature);

    let mut context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut lowered, &mut context);
    let entries = body::Body::lower_with(func, &mut builder, environment)?;
    builder.finalize();
    Ok(Lowered {
        func: lowered,
        entries,
    })
}

/// One lowered function, and what it named outside itself.
///
/// The entry points come back rather than being written into a table as they are
/// reached, because there is no longer a table to write into while lowering: the
/// seven are declared before any body is lowered, so which ones a program
/// *reaches* is a result of lowering rather than a side effect of it. That is
/// also what lets a caller collect the set serially from a batch lowered in
/// parallel, in the order it prepared them.
pub struct Lowered {
    /// The body, in the code generator's representation.
    pub func: cranelift_codegen::ir::Function,
    /// The runtime entry points it names.
    pub entries: crate::symbols::EntryTable,
}

/// Lowers one function against what a module has already been told, so that it
/// may name what the module declared.
///
/// Every argument is shared, and that is the interface rather than an accident:
/// nothing here can declare anything, so a whole program's bodies can be lowered
/// at once on a pool without any of them assigning an identifier — which rule 13
/// (`rts-cranelift/README.md`) forbids. The module itself is deliberately absent;
/// [`crate::target::func_ref`] documents why it was never needed.
#[allow(clippy::too_many_arguments)]
pub fn lower_into<'a>(
    func: &'a Function,
    machine: &'a cranelift_module::ModuleDeclarations,
    declarations: &'a crate::target::Declarations,
    entries: &'a crate::symbols::EntryImports,
    caches: &'a [cranelift_module::DataId],
    call_conv: cranelift_codegen::isa::CallConv,
    heap: Option<Heap<'a>>,
) -> Result<Lowered, LowerError> {
    lower_in(
        func,
        call_conv,
        Environment {
            outside: Some(Outside {
                machine,
                declarations,
                entries,
                caches,
            }),
            heap,
        },
    )
}

/// Lowers one function on its own, naming nothing outside itself.
///
/// Useful for reasoning about a body in isolation, and for testing that what we
/// emit is accepted without a module in the way. A program that calls anything
/// is refused here rather than half-lowered.
pub fn lower_function(
    func: &Function,
    call_conv: cranelift_codegen::isa::CallConv,
) -> Result<cranelift_codegen::ir::Function, LowerError> {
    // The entry-point set is dropped rather than returned: a function that names
    // nothing outside itself cannot have reached one, so the answer is always
    // empty and returning it would be an output with one value.
    lower_in(func, call_conv, Environment::default()).map(|lowered| lowered.func)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::Convention;
    use cranelift_codegen::ir::types;
    use cranelift_codegen::isa::CallConv;

    fn returning(repr: Repr, convention: Convention) -> cranelift_codegen::ir::Signature {
        machine_signature(
            &Signature {
                params: Vec::new(),
                returns: vec![repr],
                convention,
                ..Signature::default()
            },
            CallConv::SystemV,
        )
    }

    #[test]
    fn a_boolean_returned_across_a_stable_boundary_is_one_byte() {
        // The C convention defines only the low byte of the return register for
        // a one-byte type. Declaring a word reads whatever the callee last had
        // in the rest of it — which is not a slow answer but a wrong one, and it
        // was: `===` said two different strings were equal in an optimised build
        // and not in an unoptimised one.
        assert_eq!(
            returning(Repr::Bool, Convention::Foreign).returns[0].value_type,
            types::I8,
            "a foreign boolean return is a byte, because that is what the callee \
             actually defines"
        );
    }

    #[test]
    fn a_boolean_inside_this_layer_is_still_a_word() {
        // The narrowing is the boundary's, not the representation's. Every
        // boundary this crate has of its own — a block parameter, a spill, an
        // internal call — moves words, and narrowing booleans everywhere to
        // re-widen them at each use would be paying for one callee's ABI
        // everywhere it is not the question.
        for convention in [Convention::Internal, Convention::InternalTail] {
            assert_eq!(
                returning(Repr::Bool, convention).returns[0].value_type,
                types::I64,
                "{convention:?} is ours, so nothing else has an opinion about it"
            );
        }
        assert_eq!(machine_type(Repr::Bool), types::I64);
    }

    #[test]
    fn nothing_but_a_boolean_changes_at_the_boundary() {
        // The narrowing is one fact about one representation. A blanket rule
        // that narrowed every return would break a reference, whose payload is a
        // table index and needs the whole word.
        for repr in [Repr::I64, Repr::Tagged, Repr::F64, Repr::Ref(crate::repr::RefKind::Opaque)] {
            assert_eq!(
                returning(repr, Convention::Foreign).returns[0].value_type,
                machine_type(repr),
                "{repr:?} crosses as what it is"
            );
        }
    }
}
