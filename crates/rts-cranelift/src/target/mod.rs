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
mod hosted;

pub use declare::{Declarations, FunctionRefs};
pub use destination::{executable_memory, executable_memory_calling, object_file};
pub use hosted::{InMemory, Placing, Visibility, place_in_memory};

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
    entries: crate::symbols::EntryTable,
    heap: Option<crate::mem::RegionBases>,
    faults: std::collections::HashMap<FuncId, crate::fault::FaultTable>,
    positions: std::collections::HashMap<FuncId, crate::observe::PositionMap>,
    /// What each function is called and how much code it became.
    ///
    /// Kept so that placing a function in a map does not ask a caller to restate
    /// either. A length restated by hand is a length that can be wrong, and a
    /// wrong length names the wrong function for every address past the end.
    emitted: std::collections::HashMap<FuncId, (String, usize)>,
    call_conv: CallConv,
}

impl<'a> MachineModule<'a> {
    /// Prepares a compilation against a module.
    pub fn new(module: &'a mut dyn Module) -> Self {
        let call_conv = module.isa().default_call_conv();
        Self {
            module,
            declarations: Declarations::new(),
            entries: crate::symbols::EntryTable::new(),
            heap: None,
            faults: std::collections::HashMap::new(),
            positions: std::collections::HashMap::new(),
            emitted: std::collections::HashMap::new(),
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

    /// Where a compiled function can stop, and where each stop came from.
    ///
    /// Empty until the function is defined, because it is read out of what was
    /// compiled rather than predicted before compiling.
    pub fn faults(&self, id: FuncId) -> Option<&crate::fault::FaultTable> {
        self.faults.get(&id)
    }

    /// Hands over what this compilation learned about its functions.
    ///
    /// Consumes the compilation, which is the point: what comes back is only
    /// usable once the destination has been finalized, and finalizing is not
    /// possible while a compilation is still holding it.
    pub fn into_placements(self) -> Placements {
        Placements {
            emitted: self.emitted,
            positions: self.positions,
        }
    }

    /// Where each run of a compiled function's code came from.
    ///
    /// The answer a profiler needs. Empty until the function is defined, and
    /// empty afterwards if nothing said where anything came from — which is a
    /// truthful empty rather than a silent one.
    pub fn positions(&self, id: FuncId) -> Option<&crate::observe::PositionMap> {
        self.positions.get(&id)
    }

    /// Which runtime entry points this compilation has needed.
    ///
    /// Worth being able to ask: entry points are declared on first use, so this
    /// says what the compiled code actually reaches for — which is a structural
    /// fact, and cheaper to check than arranging to observe a side effect.
    pub fn entries(&self) -> &crate::symbols::EntryTable {
        &self.entries
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
        self.emitted.insert(id, (name.to_owned(), 0));
        Ok(())
    }

    /// Gives this compilation a heap to read, write and allocate in.
    ///
    /// Optional because a compilation that never touches an object needs none,
    /// and because how a reference becomes an address is a property of the heap
    /// rather than of any function compiled against it.
    pub fn with_heap(mut self, heap: crate::mem::RegionBases) -> Self {
        self.heap = Some(heap);
        self
    }

    /// Compiles a function body into the module.
    pub fn define(
        &mut self,
        id: FuncId,
        func: &Function,
        funcs: &FuncRegistry,
        types: &crate::types::TypeRegistry,
    ) -> Result<(), TargetError> {
        let declared = self
            .declarations
            .machine_id(id)
            .ok_or(TargetError::UndeclaredFunction(id))?;

        let mut context = Context::new();
        self.record_shapes(func, funcs);
        let caches = self.declare_caches(id, func)?;

        // Destructured so that the module and the cache are borrowed separately:
        // lowering needs both at once, and they are disjoint parts of this.
        let Self {
            module,
            declarations,
            entries,
            heap,
            call_conv,
            ..
        } = self;
        context.func = crate::lower::lower_into(
            func,
            declarations,
            entries,
            &caches,
            *module,
            *call_conv,
            heap.as_ref()
                .map(|heap| crate::lower::Heap { bases: heap, types }),
        )?;

        self.module.define_function(declared, &mut context)?;

        // Read out before the context is dropped: the correspondence between
        // addresses and the program exists in what was just compiled, and
        // nowhere else afterwards.
        if let Some(code) = context.compiled_code() {
            self.faults.insert(id, crate::fault::FaultTable::of(code));
            self.positions
                .insert(id, crate::observe::PositionMap::of(code));
            if let Some(emitted) = self.emitted.get_mut(&id) {
                emitted.1 = code.code_info().total_size as usize;
            }
        }
        Ok(())
    }
}

impl MachineModule<'_> {
    /// Declares somewhere for each of a function's sites to remember what it saw.
    ///
    /// Writable, and initialized to a layout no object has. It cannot start at
    /// zero: zero is a real layout, and a site that had never run would claim to
    /// recognize the first one ever declared — and then read a field of an object
    /// of some other shape entirely, at an offset that happened to be there.
    ///
    /// Data rather than patched instructions, so that the same emission works
    /// whether the result runs from memory or is written to an object file. Code
    /// that rewrites itself cannot be written to a file at all.
    fn declare_caches(
        &mut self,
        id: FuncId,
        func: &Function,
    ) -> Result<Vec<cranelift_module::DataId>, TargetError> {
        let name = self
            .emitted
            .get(&id)
            .map(|(name, _)| name.clone())
            .ok_or(TargetError::UndeclaredFunction(id))?;

        let mut cold = Vec::with_capacity(16);
        cold.extend_from_slice(&(-1i64).to_ne_bytes());
        cold.extend_from_slice(&0i64.to_ne_bytes());

        let mut declared = Vec::with_capacity(func.cache_count());
        for site in 0..func.cache_count() {
            let data = self.module.declare_data(
                &format!("{name}.cache.{site}"),
                Linkage::Local,
                true,
                false,
            )?;
            let mut description = cranelift_module::DataDescription::new();
            description.define(cold.clone().into_boxed_slice());
            self.module.define_data(data, &description)?;
            declared.push(data);
        }
        Ok(declared)
    }

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

/// What a compilation knows about its functions, once it is done with them.
///
/// Handed over rather than asked for while compiling, because placing a function
/// needs its address and nothing has one until the destination is finalized — by
/// which point the compilation is over. The borrow checker says the same thing,
/// which is how the ordering was noticed.
#[derive(Default)]
pub struct Placements {
    emitted: std::collections::HashMap<FuncId, (String, usize)>,
    positions: std::collections::HashMap<FuncId, crate::observe::PositionMap>,
}

impl Placements {
    /// Puts a compiled function on a map, at the address it ended up at.
    ///
    /// The address is the caller's to supply, because only the destination knows
    /// one. Everything else — the name, how much code it became, where each run
    /// of it came from — is already known, so none of it is restated. A length
    /// restated by hand is a length that can be wrong, and a wrong one names the
    /// wrong function for every address past the end.
    ///
    /// Reports whether there was anything to place. A function declared and never
    /// defined has a name and no code, and silently mapping zero bytes of it puts
    /// a hole in the map at a real address.
    pub fn place(&self, map: &mut crate::observe::CodeMap, id: FuncId, address: usize) -> bool {
        let Some((name, length)) = self.emitted.get(&id) else {
            return false;
        };
        if *length == 0 {
            return false;
        }

        let positions = self.positions.get(&id).cloned().unwrap_or_default();
        map.record(name, address, *length, positions);
        true
    }

    /// Every function that has code to place.
    pub fn defined(&self) -> impl Iterator<Item = FuncId> + '_ {
        self.emitted
            .iter()
            .filter(|(_, (_, length))| *length > 0)
            .map(|(&id, _)| id)
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
