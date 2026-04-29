//! Global class metadata — for built-in JS classes (`Date`, `Error`, …).
//!
//! A `GlobalClassSpec` registers a class that lives in global scope without
//! an explicit import: `new Date()`, `d.getFullYear()`, etc.
//!
//! Members are tagged by `MemberKind`:
//! - `Function`      → static method  (`Date.now()`)
//! - `Constructor`   → ctor overload   (`new Date()` / `new Date(ms)`)
//! - `InstanceMethod`→ instance method (`d.getFullYear()`)
//!
//! Codegen consults `GLOBAL_CLASS_SPECS` in `lower_new` (for constructors)
//! and after the user-class dispatch path (for instance methods).

use super::member::NamespaceMember;

/// A globally-scoped JS class backed by RTS runtime symbols.
#[derive(Debug, Clone, Copy)]
pub struct GlobalClassSpec {
    /// JS class name, e.g. `"Date"`, `"Error"`.
    pub name: &'static str,
    /// Human-readable summary used by `rts apis`.
    pub doc: &'static str,
    /// All members: static functions, constructors, instance methods.
    pub members: &'static [NamespaceMember],
}

impl GlobalClassSpec {
    /// Returns all `Constructor` members, ordered by arity.
    pub fn constructors(&self) -> impl Iterator<Item = &NamespaceMember> {
        use super::member::MemberKind;
        self.members
            .iter()
            .filter(|m| m.kind == MemberKind::Constructor)
    }

    /// Returns the constructor whose arity matches `n_args`, if any.
    pub fn constructor_for_arity(&self, n_args: usize) -> Option<&NamespaceMember> {
        self.constructors().find(|m| m.args.len() == n_args)
    }

    /// Returns an instance method by name, if any.
    pub fn instance_method(&self, name: &str) -> Option<&NamespaceMember> {
        use super::member::MemberKind;
        self.members
            .iter()
            .find(|m| m.kind == MemberKind::InstanceMethod && m.name == name)
    }

    /// Returns a static method/constant by name, if any.
    pub fn static_member(&self, name: &str) -> Option<&NamespaceMember> {
        use super::member::MemberKind;
        self.members.iter().find(|m| {
            matches!(m.kind, MemberKind::Function | MemberKind::Constant) && m.name == name
        })
    }
}
