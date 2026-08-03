//! Where compiled code goes.
//!
//! Two destinations — executable memory now, an object file to link later — and
//! one pipeline. They are not two compilers: everything before this module is
//! identical for both, and the difference is entirely in what happens to the
//! bytes at the end. Treating them as two paths is how the two come to disagree
//! about what a program means.
//!
//! # Why a module is what unblocks the rest
//!
//! Lowering can emit anything that stays inside one function. The moment a
//! program calls something, allocates, throws or awaits, it names something that
//! is not in the function — and naming things outside is what a module is for.
//! That is why the refusals in [`crate::lower`] all point here.
//!
//! # Declaring once
//!
//! A function is declared once and referenced by the identifier that declaration
//! returned. This matters more than it sounds: the code generator's own
//! declaration is keyed by name, allocates a string, and probes a table, so a
//! program that re-declares its callee at every call site pays that per site.
//! [`Declarations`] is the cache that makes it per callee instead.

mod declare;
mod destination;

pub use declare::{Declarations, FunctionRefs};
pub use destination::{executable_memory, object_file};

use cranelift_codegen::Context;
use cranelift_codegen::isa::{CallConv, OwnedTargetIsa};
use cranelift_module::{Linkage, Module, ModuleError};

use crate::ir::{FuncId, FuncRegistry, Function};
use crate::lower::{LowerError, machine_signature};

/// Why a program could not be placed into a module.
#[derive(Debug)]
pub enum TargetError {
    /// Lowering refused.
    Lower(LowerError),
    /// The module or the code generator refused.
    ///
    /// Kept as the underlying error rather than flattened to a string: what
    /// failed is often the only thing that says which of several plausible
    /// causes it was.
    Module(ModuleError),
    /// A function was defined without being declared.
    ///
    /// Declaring is what produces the identifier a call site refers to, so
    /// defining first means some call site is already naming something that does
    /// not exist.
    UndeclaredFunction(FuncId),
}

impl From<LowerError> for TargetError {
    fn from(error: LowerError) -> Self {
        TargetError::Lower(error)
    }
}

impl From<ModuleError> for TargetError {
    fn from(error: ModuleError) -> Self {
        TargetError::Module(error)
    }
}

/// One compilation, going to one destination.
///
/// Borrows the module rather than owning it, because the two destinations own
/// very different things — executable memory that must outlive what runs in it,
/// or a buffer that becomes a file — and pretending otherwise would put that
/// difference back into every caller.
pub struct MachineModule<'a> {
    module: &'a mut dyn Module,
    declarations: Declarations,
    call_conv: CallConv,
}

impl<'a> MachineModule<'a> {
    /// Prepares a compilation against a module.
    pub fn new(module: &'a mut dyn Module) -> Self {
        let call_conv = module.isa().default_call_conv();
        Self {
            module,
            declarations: Declarations::new(),
            call_conv,
        }
    }

    /// The convention this destination uses for anything crossing a boundary.
    pub fn call_conv(&self) -> CallConv {
        self.call_conv
    }

    /// What has been declared so far.
    pub fn declarations(&self) -> &Declarations {
        &self.declarations
    }

    /// Declares a function so that call sites can name it.
    ///
    /// Declaring is separate from defining because a program can call something
    /// before it is built — including itself. Requiring definition first would
    /// make recursion unexpressible.
    pub fn declare(
        &mut self,
        id: FuncId,
        name: &str,
        linkage: Linkage,
        funcs: &FuncRegistry,
    ) -> Result<(), TargetError> {
        let signature = funcs
            .signature_of(id)
            .ok_or(TargetError::UndeclaredFunction(id))?;
        let lowered = machine_signature(signature, self.call_conv);
        let declared = self.module.declare_function(name, linkage, &lowered)?;
        self.declarations.record(id, declared, lowered);
        Ok(())
    }

    /// Compiles a function body into the module.
    pub fn define(
        &mut self,
        id: FuncId,
        func: &Function,
        funcs: &FuncRegistry,
    ) -> Result<(), TargetError> {
        let declared = self
            .declarations
            .machine_id(id)
            .ok_or(TargetError::UndeclaredFunction(id))?;

        let mut context = Context::new();
        self.record_shapes(func, funcs);

        // Destructured so that the module and the cache are borrowed separately:
        // lowering needs both at once, and they are disjoint parts of this.
        let Self {
            module,
            declarations,
            call_conv,
        } = self;
        context.func = crate::lower::lower_into(func, declarations, *module, *call_conv)?;

        self.module.define_function(declared, &mut context)?;
        Ok(())
    }
}

impl MachineModule<'_> {
    /// Records every shape this function's indirect calls expect.
    ///
    /// An indirect call names a shape rather than a callee, and a shape has to
    /// exist in the module's vocabulary before a call site can refer to it.
    /// Deriving that from the function itself means a client never has to
    /// remember to declare a shape it already stated at the call site.
    fn record_shapes(&mut self, func: &Function, funcs: &FuncRegistry) {
        use crate::ir::{Inst, Terminator};

        let mut record = |sig: crate::ir::SigId| {
            if let Some(shape) = funcs.signature(sig) {
                let lowered = machine_signature(shape, self.call_conv);
                self.declarations.record_signature(sig, lowered);
            }
        };

        for (_, block) in func.blocks() {
            for &inst in &block.insts {
                if let Some(data) = func.inst(inst)
                    && let Inst::CallIndirect { sig, .. } = data.inst
                {
                    record(sig);
                }
            }
            if let Some(Terminator::TailCallIndirect { sig, .. }) = block.terminator {
                record(sig);
            }
        }
    }
}

/// The architecture this process runs on.
///
/// Exposed because both destinations need one and neither should have to know
/// how to ask for it.
pub fn host_isa() -> Result<OwnedTargetIsa, TargetError> {
    let mut flags = cranelift_codegen::settings::builder();
    // Frame pointers are what makes a stack walkable, and a stack walk is how
    // the collector finds a frame with no descriptor. Turning them off would
    // save a register and cost the fallback that makes the migration possible.
    cranelift_codegen::settings::Configurable::set(&mut flags, "preserve_frame_pointers", "true")
        .expect("a real setting");

    let flags = cranelift_codegen::settings::Flags::new(flags);
    let builder = cranelift_native::builder()
        .map_err(|message| TargetError::Module(ModuleError::Backend(anyhow_from(message))))?;
    builder
        .finish(flags)
        .map_err(|error| TargetError::Module(ModuleError::Backend(anyhow_from(error.to_string()))))
}

/// Wraps a message the code generator gave us as an error it can carry.
fn anyhow_from(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::msg(message.into())
}
