//! The declaration cache.
//!
//! A call site refers to a callee by the identifier its declaration produced.
//! Producing that identifier costs a string allocation and a table probe in the
//! code generator, so doing it per call site rather than per callee is a cost
//! proportional to how often something is called — which is exactly backwards.
//!
//! The cache is keyed by our own identifier, which is a dense index, so a lookup
//! is an array read and no string is hashed on the path that emits code.

use cranelift_codegen::ir::{Function as ClFunction, SigRef, Signature};
use cranelift_module::{FuncId as MachineFuncId, Module};

use crate::ir::{FuncId, SigId};

/// What one declaration produced.
#[derive(Clone)]
struct Declared {
    machine_id: MachineFuncId,
    signature: Signature,
}

/// Every declaration made against one module.
///
/// Bound to that module: the identifiers are not portable between modules, so
/// reusing a cache across one is how a call comes to name a function in a module
/// that is no longer there. A fresh module gets a fresh cache.
#[derive(Default)]
pub struct Declarations {
    functions: Vec<Option<Declared>>,
    signatures: Vec<Option<Signature>>,
}

impl Declarations {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records what a declaration produced.
    pub(super) fn record(&mut self, id: FuncId, machine_id: MachineFuncId, signature: Signature) {
        grow(&mut self.functions, id.index());
        self.functions[id.index()] = Some(Declared {
            machine_id,
            signature,
        });
    }

    /// Records the shape an indirect call site expects.
    pub fn record_signature(&mut self, id: SigId, signature: Signature) {
        grow(&mut self.signatures, id.index());
        self.signatures[id.index()] = Some(signature);
    }

    /// The module's identifier for a declared function.
    pub fn machine_id(&self, id: FuncId) -> Option<MachineFuncId> {
        self.functions
            .get(id.index())
            .and_then(|slot| slot.as_ref())
            .map(|declared| declared.machine_id)
    }

    /// The signature a declared function was given.
    pub fn signature_of(&self, id: FuncId) -> Option<&Signature> {
        self.functions
            .get(id.index())
            .and_then(|slot| slot.as_ref())
            .map(|declared| &declared.signature)
    }

    /// A recorded indirect-call shape.
    pub fn signature(&self, id: SigId) -> Option<&Signature> {
        self.signatures
            .get(id.index())
            .and_then(|slot| slot.as_ref())
    }

    /// How many functions have been declared.
    pub fn len(&self) -> usize {
        self.functions.iter().filter(|slot| slot.is_some()).count()
    }

    /// Whether nothing has been declared.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// References to declarations, valid inside one function being built.
///
/// The code generator wants a per-function reference to anything a function
/// mentions, and producing one is not free either — so it is also produced once
/// per callee per function rather than once per call site.
#[derive(Default)]
pub struct FunctionRefs {
    callees: Vec<Option<cranelift_codegen::ir::FuncRef>>,
    shapes: Vec<Option<SigRef>>,
}

impl FunctionRefs {
    /// No references yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A reference to a declared callee, made on first use.
    /// Asks the module for the reference, rather than assembling a name for it.
    /// How a declared function is named inside a compilation is the module's
    /// business, and reproducing that convention here would be a second copy of
    /// it to keep in agreement.
    pub fn callee(
        &mut self,
        module: &mut dyn Module,
        func: &mut ClFunction,
        declarations: &Declarations,
        id: FuncId,
    ) -> Option<cranelift_codegen::ir::FuncRef> {
        grow(&mut self.callees, id.index());
        if let Some(existing) = self.callees[id.index()] {
            return Some(existing);
        }

        let machine_id = declarations.machine_id(id)?;
        let reference = module.declare_func_in_func(machine_id, func);
        self.callees[id.index()] = Some(reference);
        Some(reference)
    }

    /// A reference to a shape an indirect call expects, made on first use.
    pub fn shape(
        &mut self,
        func: &mut ClFunction,
        declarations: &Declarations,
        id: SigId,
    ) -> Option<SigRef> {
        grow(&mut self.shapes, id.index());
        if let Some(existing) = self.shapes[id.index()] {
            return Some(existing);
        }

        let signature = declarations.signature(id)?.clone();
        let reference = func.import_signature(signature);
        self.shapes[id.index()] = Some(reference);
        Some(reference)
    }
}

/// Makes room up to and including an index.
fn grow<T: Clone>(slots: &mut Vec<Option<T>>, index: usize) {
    if slots.len() <= index {
        slots.resize(index + 1, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ir::{FuncRegistry, Signature as OurSignature};

    #[test]
    fn an_undeclared_function_has_no_identifier() {
        let mut funcs = FuncRegistry::new();
        let shape = funcs.declare_signature(OurSignature::default());
        let id = funcs.declare_function(shape);

        let declarations = Declarations::new();
        assert!(declarations.machine_id(id).is_none());
        assert!(declarations.is_empty());
    }
}
