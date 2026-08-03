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
//! Suspension, scheduling, unwinding and allocation are refused by name, with
//! the capability each needs, so a program that needs them fails visibly instead
//! of producing code that is quietly incomplete.
//!
//! Allocation is worth separating from the rest of memory. Reading and writing a
//! field is arithmetic and lands here; asking a heap for space is a runtime entry
//! point, and a runtime entry point is something to declare rather than something
//! to emit.
//!
//! Two further refusals are findings rather than gaps, and are documented where
//! they are raised: a 64-bit integer cannot be widened without a heap box, and a
//! guard cannot establish *which* kind of reference a value holds, because the
//! encoding does not carry one.

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

/// The code generator's signature for one of ours.
pub fn machine_signature(
    signature: &Signature,
    call_conv: cranelift_codegen::isa::CallConv,
) -> cranelift_codegen::ir::Signature {
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
    call_conv: cranelift_codegen::isa::CallConv,
    environment: Environment<'a>,
) -> Result<cranelift_codegen::ir::Function, LowerError> {
    let signature = machine_signature(&func.signature, call_conv);
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
    func: &Function,
    declarations: &'a crate::target::Declarations,
    module: &'a mut dyn cranelift_module::Module,
    call_conv: cranelift_codegen::isa::CallConv,
) -> Result<cranelift_codegen::ir::Function, LowerError> {
    lower_in(
        func,
        call_conv,
        Environment {
            outside: Some(Outside {
                module,
                declarations,
            }),
            heap: None,
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
