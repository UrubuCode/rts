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
mod error;
mod memory;
mod types;
mod value;

pub use body::Outside;
pub use error::{Capability, LowerError};
pub use memory::Heap;
pub use types::{is_word, machine_type};

use cranelift_codegen::ir::{AbiParam, UserFuncName};
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
    lowered.returns.extend(
        signature
            .returns
            .iter()
            .map(|&r| AbiParam::new(machine_type(r))),
    );
    lowered
}

/// What a function is allowed to reach beyond itself.
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
) -> Result<cranelift_codegen::ir::Function, LowerError> {
    let signature = machine_signature(&func.signature, target_default);
    let mut lowered =
        cranelift_codegen::ir::Function::with_name_signature(UserFuncName::default(), signature);

    let mut context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut lowered, &mut context);
    body::Body::lower_with(func, &mut builder, environment)?;
    builder.finalize();
    Ok(lowered)
}

/// Lowers one function into a module, so that it may call what the module has
/// declared.
pub fn lower_into<'a>(
    func: &'a Function,
    declarations: &'a crate::target::Declarations,
    entries: &'a mut crate::symbols::EntryTable,
    caches: &'a [cranelift_module::DataId],
    module: &'a mut dyn cranelift_module::Module,
    call_conv: cranelift_codegen::isa::CallConv,
    heap: Option<Heap<'a>>,
) -> Result<cranelift_codegen::ir::Function, LowerError> {
    lower_in(
        func,
        call_conv,
        Environment {
            outside: Some(Outside {
                module,
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
    lower_in(func, call_conv, Environment::default())
}
