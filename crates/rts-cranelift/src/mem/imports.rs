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
