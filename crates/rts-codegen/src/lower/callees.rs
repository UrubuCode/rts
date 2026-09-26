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
    /// The tagged-template SITE minted for the template written at each position.
    ///
    /// Minted by whoever compiles the function, because a site is a row of the
    /// program's template table and one per emission -- `emit/template.rs` says why
    /// it cannot be keyed by position across a program. Within one function's
    /// lowering it can: every position is one node of one tree.
    templates: std::collections::BTreeMap<rts_cranelift::fault::Position, u32>,
    /// The functions a call by name may be SUBSTITUTED for -- `substitute.rs` -- as the
    /// compiler that proved each one handed them over.
    substitutes: std::collections::BTreeMap<crate::names::Name, super::Substitute>,
    /// Whether the whole program leaves `Math` as the language defines it --
    /// `intrinsic.rs`.
    math_primordial: bool,
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
            templates: std::collections::BTreeMap::new(),
            substitutes: std::collections::BTreeMap::new(),
            math_primordial: false,
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
            templates: std::collections::BTreeMap::new(),
            substitutes: std::collections::BTreeMap::new(),
            math_primordial: false,
            by_binding: std::collections::BTreeMap::new(),
            by_position: positions
                .iter()
                .enumerate()
                .map(|(at, held)| (*held, rts_mir::cfg::FuncId(at as u32)))
                .collect(),
        }
    }

    /// The same map, with a site for each tagged template this function writes.
    pub fn with_templates(
        mut self,
        sites: std::collections::BTreeMap<rts_cranelift::fault::Position, u32>,
    ) -> Self {
        self.templates = sites;
        self
    }

    /// The same map, with the functions a call by name may be substituted for.
    pub fn with_substitutes(
        mut self,
        substitutes: std::collections::BTreeMap<crate::names::Name, super::Substitute>,
    ) -> Self {
        self.substitutes = substitutes;
        self
    }

    /// The same map, saying whether the program leaves `Math` alone.
    pub fn with_math_primordial(mut self, untouched: bool) -> Self {
        self.math_primordial = untouched;
        self
    }

    /// Whether the program leaves `Math` alone.
    pub fn math_primordial(&self) -> bool {
        self.math_primordial
    }

    /// What a call to `name` may be substituted for, where something proved one.
    pub fn substitute(&self, name: crate::names::Name) -> Option<&super::Substitute> {
        self.substitutes.get(&name)
    }

    /// The site minted for the tagged template written at that position.
    pub fn template_site(&self, at: rts_cranelift::fault::Position) -> Option<u32> {
        self.templates.get(&at).copied()
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
