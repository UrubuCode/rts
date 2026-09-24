//! Which function a call or a function value names, numbered once per module.
//!
//! Apart from `mod.rs` because the ceiling said so, and along a seam: this is a map
//! built BEFORE lowering, and nothing in it lowers anything.

use super::{BindingId, Function, Resolution};

/// Which binding of a module calls which function.
///
/// Built by [`crate::lower_module`] over the whole module before any of it is
/// lowered, because a call may name a function written later in the file --
/// `function a() { return b(); } function b() {}` is ordinary code.
///
/// Keyed by [`BindingId`] and not by a spelling, which is the whole of why this is
/// a map and not a search: two functions may be called `step` in one module, and
/// the binding says which one a given call reaches.
#[derive(Debug, Default)]
pub struct Callees {
    by_binding: std::collections::BTreeMap<BindingId, rts_mir::cfg::FuncId>,
    /// Which function was written at which position.
    ///
    /// A function EXPRESSION usually binds no name, so the binding map cannot reach
    /// it — and a function VALUE has to be nameable whether or not anything holds
    /// it. The position is what the two views share, which `bound_expressions`
    /// already relied on.
    by_position: std::collections::BTreeMap<rts_cranelift::fault::Position, rts_mir::cfg::FuncId>,
}

impl Callees {
    /// The map for one module.
    pub fn of(
        items: &[crate::syntax::ModuleItem],
        functions: &[&Function],
        resolution: &Resolution,
    ) -> Self {
        let mut held = crate::lower_module::callee_map(functions, resolution);
        held.extend(crate::lower_module::bound_expressions(
            items, functions, resolution,
        ));
        Self {
            by_binding: held,
            by_position: functions
                .iter()
                .enumerate()
                .map(|(at, function)| (function.at, rts_mir::cfg::FuncId(at as u32)))
                .collect(),
        }
    }

    /// A map naming only the functions written at these positions, numbered in order --
    /// for a lowering that makes closures of exactly those and calls none by name.
    pub fn of_positions(positions: &[rts_cranelift::fault::Position]) -> Self {
        Self {
            by_binding: std::collections::BTreeMap::new(),
            by_position: positions
                .iter()
                .enumerate()
                .map(|(at, held)| (*held, rts_mir::cfg::FuncId(at as u32)))
                .collect(),
        }
    }

    /// Which function was written at that position, if the module numbered one.
    pub fn of_position(&self, at: rts_cranelift::fault::Position) -> Option<rts_mir::cfg::FuncId> {
        self.by_position.get(&at).copied()
    }

    /// Which function that binding is, if it is one.
    pub fn of_binding(&self, binding: BindingId) -> Option<rts_mir::cfg::FuncId> {
        self.by_binding.get(&binding).copied()
    }
}
