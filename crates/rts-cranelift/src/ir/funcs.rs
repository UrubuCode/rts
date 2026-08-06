//! Function and signature declarations.
//!
//! A call site needs to know what it calls accepts, what it returns, and which
//! discipline it follows. All of that comes from here, and none of it is stated
//! at the site — a site that restated it could restate it wrongly, and the
//! failure would be an interface mismatch surviving until something crashes.

use std::collections::HashMap;

use super::func::Signature;

/// Identifies a signature.
///
/// Separate from a function because an indirect call knows the shape of what it
/// calls without knowing which function it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct SigId(pub(crate) u32);

/// Identifies a declared function.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct FuncId(pub(crate) u32);

impl SigId {
    /// The registry index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl FuncId {
    /// The registry index.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// What is known about a declared function.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FuncDecl {
    /// Its shape.
    pub sig: SigId,
}

/// Every signature and function one compilation knows about.
///
/// Signatures are interned by content, so two functions of the same shape share
/// one identifier — which is what lets an indirect call site name the shape it
/// expects without naming a particular callee.
#[derive(Default)]
pub struct FuncRegistry {
    signatures: Vec<Signature>,
    functions: Vec<FuncDecl>,
    // A membership/lookup index over `signatures`, not a second source of
    // truth: `signatures` (a `Vec`) is what assigns a `SigId` its number, by
    // insertion order, and that order is call order, not hash order — nothing
    // here iterates this map to produce output, so rule 13 does not apply to
    // it. Before this, `declare_signature` scanned `signatures` linearly with
    // `position`, which is what made it O(N^2) across a compilation: every
    // function and closure the language layer emits declares at least one
    // signature, so this ran once per declaration over everything declared
    // so far. Measured on the language-layer side of this same class of
    // problem (60 `try`/`finally` costing 29.32 ms against 300 `if`/`else`
    // costing 7.55 ms) — a linear scan inside a loop that runs once per
    // declaration is exactly that shape.
    by_signature: HashMap<Signature, SigId>,
}

impl FuncRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns a signature.
    pub fn declare_signature(&mut self, signature: Signature) -> SigId {
        if let Some(&id) = self.by_signature.get(&signature) {
            return id;
        }
        let id = SigId(self.signatures.len() as u32);
        self.signatures.push(signature.clone());
        self.by_signature.insert(signature, id);
        id
    }

    /// Declares a function of a given shape.
    pub fn declare_function(&mut self, sig: SigId) -> FuncId {
        let id = FuncId(self.functions.len() as u32);
        self.functions.push(FuncDecl { sig });
        id
    }

    /// A declared signature.
    pub fn signature(&self, id: SigId) -> Option<&Signature> {
        self.signatures.get(id.index())
    }

    /// The signature of a declared function.
    pub fn signature_of(&self, id: FuncId) -> Option<&Signature> {
        let decl = self.functions.get(id.index())?;
        self.signature(decl.sig)
    }

    /// Whether this function can park its own frame.
    ///
    /// Note what this does *not* imply: calling it does not park the caller.
    /// Suspension here is stackless — a function that can park returns a promise,
    /// and a caller parks, if at all, at its own await. So this is information a
    /// client may want, not a constraint on who may call it.
    ///
    /// Returns `false` for an unknown function so that a caller asking the
    /// question gets an answer rather than a panic; the verifier is what reports
    /// the unknown callee, once, with the site that named it.
    pub fn may_suspend(&self, id: FuncId) -> bool {
        self.signature_of(id).is_some_and(|s| s.may_suspend)
    }

    /// How many functions are declared.
    pub fn len(&self) -> usize {
        self.functions.len()
    }

    /// Whether none are declared.
    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repr::Repr;

    #[test]
    fn identical_shapes_share_one_signature() {
        let mut registry = FuncRegistry::new();
        let shape = Signature {
            params: vec![Repr::I64],
            ..Signature::default()
        };

        let first = registry.declare_signature(shape.clone());
        let second = registry.declare_signature(shape);
        assert_eq!(
            first, second,
            "an indirect site names a shape, not a callee, so shapes must be comparable"
        );
    }

    #[test]
    fn a_function_reports_whether_calling_it_can_park_the_caller() {
        let mut registry = FuncRegistry::new();
        let plain = registry.declare_signature(Signature::default());
        let parking = registry.declare_signature(Signature {
            may_suspend: true,
            ..Signature::default()
        });

        let a = registry.declare_function(plain);
        let b = registry.declare_function(parking);
        assert!(!registry.may_suspend(a));
        assert!(registry.may_suspend(b));
    }
}
