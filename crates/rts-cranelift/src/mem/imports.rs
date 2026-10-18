//! Declaring a heap's symbolic base into a module.
//!
//! A heap addressed by [`RegionBase::Immediate`] needs nothing declared: the
//! address is baked into compiled code as a constant, because the host that
//! allocated the region already knows it. One addressed by
//! [`RegionBase::Symbol`] cannot do that — the address is not known until the
//! region exists, which for an ahead-of-time binary is only once it runs — so
//! the base has to be a cell the runtime writes at startup and compiled code
//! reads. Reading a cell needs it declared in the module first, exactly the
//! requirement [`crate::symbols::EntryImports`] states for the seven runtime
//! entry points, and for the same reason: an identifier handed out while a
//! worker lowers a function is an identifier whose number depends on which
//! worker got there first, which rule 13 of `rts-cranelift/README.md`
//! forbids. So this is declared once, before any function is lowered, exactly
//! where [`EntryImports`](crate::symbols::EntryImports) is.
//!
//! # Why this lives beside [`RegionBases`] rather than in `target::declare`
//!
//! What to declare is a fact about the heap — how many symbols, and what they
//! are named — not about the module. `target::declare` supplies the
//! mechanism ([`crate::target::data_ref`]) that turns a declared identifier
//! into something a function body can read; this module is the one place that
//! decides which symbols a given [`RegionBases`] needs.

use cranelift_module::{DataId, Linkage, Module, ModuleError};

use super::{Addressing, RegionBase, RegionBases};

/// The module's identifier for a heap's base, if it has a symbolic one.
///
/// At most one symbol exists per heap: [`Addressing::Single`] names its own
/// base, and [`Addressing::Sharded`] names the table of region bases — never
/// both, because a heap has one [`RegionBase`] field regardless of which form
/// it uses.
pub struct HeapImports {
    base: Option<DataId>,
}

impl HeapImports {
    /// Declares this heap's symbolic base, if it has one.
    ///
    /// A heap addressed by [`RegionBase::Immediate`] declares nothing and
    /// this returns an empty [`HeapImports`] — not an error, because a heap
    /// with no symbol to declare is an ordinary heap, not a malformed one.
    pub fn declare(module: &mut dyn Module, bases: &RegionBases) -> Result<Self, ModuleError> {
        let symbol = match bases.addressing() {
            Addressing::Single {
                base: RegionBase::Symbol(name),
                ..
            } => Some(name.as_str()),
            Addressing::Sharded {
                table: RegionBase::Symbol(name),
                ..
            } => Some(name.as_str()),
            _ => None,
        };

        let base = match symbol {
            // Imported: the runtime defines this cell and writes the region's
            // real address into it before any compiled code runs. Neither the
            // writing nor the resolving of the name against an address (JIT)
            // or an archive (object file) is this module's business — the
            // same split `EntryImports` makes for the seven entry points.
            Some(name) => Some(module.declare_data(name, Linkage::Import, false, false)?),
            None => None,
        };
        Ok(Self { base })
    }

    /// The module's identifier for the base, if this heap has a symbolic one.
    pub fn base(&self) -> Option<DataId> {
        self.base
    }
}

/// The symbol a baked heap base is read from, in relocatable-immediate mode.
pub const IMM_BASE: &str = "rts.imm.base";
/// The symbol the cell cursor's address is read from, in the same mode.
pub const IMM_CURSOR: &str = "rts.imm.cursor";
/// The symbol the run table's address is read from, in the same mode.
pub const IMM_RUNS: &str = "rts.imm.runs";

impl RegionBases {
    /// The addresses lowering would bake into code as 64-bit immediates, each
    /// with the symbol that stands for it in relocatable-immediate mode.
    ///
    /// The ONE list of such addresses. `lower/` bakes exactly these three
    /// (`memory::materialize`, `cell_alloc`, `run_read`), and a fourth baked
    /// address added there without being added here would be a number frozen
    /// into code the compiled-code cache replays in another process. The
    /// lowering side panics rather than bake when it is in relocatable mode and
    /// finds no symbol, so the omission is loud.
    pub fn immediate_values(&self) -> Vec<(&'static str, u64)> {
        let mut values = Vec::new();
        match self.addressing() {
            Addressing::Single {
                base: RegionBase::Immediate(address),
                ..
            }
            | Addressing::Sharded {
                table: RegionBase::Immediate(address),
                ..
            } => values.push((IMM_BASE, *address)),
            _ => {}
        }
        if let Some(cursor) = self.cursor() {
            values.push((IMM_CURSOR, cursor));
        }
        if let Some(runs) = self.runs() {
            values.push((IMM_RUNS, runs));
        }
        values
    }
}

/// The module's identifiers for the symbols standing in for baked addresses.
///
/// Declared once, before any function is lowered, for the reason
/// [`HeapImports`] is: an identifier handed out on a worker would depend on
/// scheduling.
#[derive(Clone, Copy, Default)]
pub struct ImmediateIds {
    /// Stands for the heap base (or the table of region bases).
    pub base: Option<DataId>,
    /// Stands for the cell cursor's address.
    pub cursor: Option<DataId>,
    /// Stands for the run table's address.
    pub runs: Option<DataId>,
}

impl ImmediateIds {
    /// Declares one import per address `bases` would have baked.
    pub fn declare(module: &mut dyn Module, bases: &RegionBases) -> Result<Self, ModuleError> {
        let mut ids = Self::default();
        for (name, _) in bases.immediate_values() {
            let id = module.declare_data(name, Linkage::Import, false, false)?;
            match name {
                IMM_BASE => ids.base = Some(id),
                IMM_CURSOR => ids.cursor = Some(id),
                _ => ids.runs = Some(id),
            }
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_module::Module;

    fn module() -> cranelift_jit::JITModule {
        crate::target::executable_memory().expect("this machine hosts its own code")
    }

    #[test]
    fn an_immediate_heap_declares_nothing() {
        let mut jit = module();
        let bases = RegionBases::single(RegionBase::Immediate(0x1000), 8);
        let imports = HeapImports::declare(&mut jit, &bases).expect("declares");
        assert!(
            imports.base().is_none(),
            "an immediate base names nothing to declare"
        );
    }

    #[test]
    fn a_symbolic_heap_declares_one_importable_symbol() {
        let mut jit = module();
        let bases = RegionBases::single(RegionBase::Symbol("test_region_base".into()), 8);
        let imports = HeapImports::declare(&mut jit, &bases).expect("declares");
        let data = imports.base().expect("a symbolic base declares itself");

        // The module now knows a name for this identifier, and it is the one
        // the heap asked for — proving the declaration reaches the module
        // rather than merely constructing a `DataId` locally.
        let decl = jit.declarations().get_data_decl(data);
        assert_eq!(decl.linkage, Linkage::Import);
    }

    #[test]
    fn a_sharded_heaps_table_is_the_symbol_declared() {
        let mut jit = module();
        let bases = RegionBases::sharded(RegionBase::Symbol("test_table".into()), 4, 8)
            .expect("power of two");
        let imports = HeapImports::declare(&mut jit, &bases).expect("declares");
        assert!(
            imports.base().is_some(),
            "a sharded heap's table is the symbol this declares"
        );
    }
}
