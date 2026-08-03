//! Declaring entry points into a module, once each.
//!
//! Keyed by the discriminant, which is a dense index, so a lookup is an array
//! read. The alternative — declaring by name wherever one is called — allocates
//! a string and probes a hash table per call site, which makes the cost
//! proportional to how often something is called rather than to how many things
//! there are.

use cranelift_codegen::ir::{FuncRef, Function as ClFunction};
use cranelift_module::{FuncId as MachineFuncId, Linkage, Module, ModuleError};

use super::RtEntry;
use crate::lower::machine_signature;

/// Which entry points a module has been told about.
///
/// Bound to that module, like every other identifier the module issues. A fresh
/// module needs a fresh table; reusing one is how a call comes to name something
/// in a module that is no longer there.
#[derive(Default)]
pub struct EntryTable {
    declared: [Option<MachineFuncId>; RtEntry::COUNT],
}

impl EntryTable {
    /// A table that has told the module nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares an entry point, or returns what a previous declaration produced.
    ///
    /// Declaring lazily rather than all at once means a program that never
    /// allocates leaves no reference to an allocator — which matters for an
    /// object file, where every undefined symbol is something a linker must find.
    pub fn declare(
        &mut self,
        module: &mut dyn Module,
        entry: RtEntry,
    ) -> Result<MachineFuncId, ModuleError> {
        if let Some(existing) = self.declared[entry.index()] {
            return Ok(existing);
        }

        let signature = machine_signature(&entry.signature(), module.isa().default_call_conv());
        // Imported: the runtime provides it. On one path a linker resolves that
        // against an archive; on the other an address was registered before
        // anything was compiled. Neither is this module's business.
        let id = module.declare_function(entry.symbol(), Linkage::Import, &signature)?;
        self.declared[entry.index()] = Some(id);
        Ok(id)
    }

    /// A reference to an entry point, usable inside one function.
    pub fn reference(
        &mut self,
        module: &mut dyn Module,
        func: &mut ClFunction,
        entry: RtEntry,
    ) -> Result<FuncRef, ModuleError> {
        let id = self.declare(module, entry)?;
        Ok(module.declare_func_in_func(id, func))
    }

    /// Whether an entry point has been declared into the module.
    pub fn is_declared(&self, entry: RtEntry) -> bool {
        self.declared[entry.index()].is_some()
    }

    /// How many entry points this module has been told about.
    pub fn len(&self) -> usize {
        self.declared.iter().filter(|slot| slot.is_some()).count()
    }

    /// Whether the module has been told about none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
