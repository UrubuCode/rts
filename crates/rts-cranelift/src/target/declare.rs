//! The declaration cache.
//!
//! A call site refers to a callee by the identifier its declaration produced.
//! Producing that identifier costs a string allocation and a table probe in the
//! code generator, so doing it per call site rather than per callee is a cost
//! proportional to how often something is called — which is exactly backwards.
//!
//! The cache is keyed by our own identifier, which is a dense index, so a lookup
//! is an array read and no string is hashed on the path that emits code.

use cranelift_codegen::ir::{
    ExtFuncData, ExternalName, Function as ClFunction, GlobalValue, GlobalValueData, SigRef,
    Signature, UserExternalName, immediates::Imm64,
};
use cranelift_module::{DataId, FuncId as MachineFuncId, ModuleDeclarations};

use crate::ir::{FuncId, SigId};
use crate::symbols::{EntryImports, EntryTable, RtEntry};

/// A reference to a declared function, usable inside one function being built.
///
/// # Why this is written here rather than asked of the module
///
/// `cranelift_module::Module::declare_func_in_func` does exactly this and takes
/// `&mut self`. Reading its body (`cranelift-module-0.131.0/src/module.rs:904`)
/// says the `&mut` is the trait's shape and not a requirement: it only *reads*
/// `self.declarations()`, and everything it writes it writes into the
/// `ir::Function` being built. Its data counterpart on the next line is already
/// `&self`, which is the same observation made by whoever wrote it.
///
/// That `&mut` was the last thing forcing lowering to hold the module, and
/// holding the module is what forced lowering onto one thread. Taking
/// [`ModuleDeclarations`] — immutable, plain data, `Sync` — instead is what lets
/// a worker lower a function without being able to declare anything.
///
/// The rejected alternative was to keep calling the module behind a lock. A lock
/// makes the write safe and leaves the *order* of anything declared inside it up
/// to the scheduler, which is the bug rule 13 forbids rather than the race.
///
/// Verified by reading the code generator's source at the version above; not
/// verified against a future version, which is why the numbering (`0` for
/// functions, `1` for data) is the one thing to re-check on an upgrade.
pub fn func_ref(
    machine: &ModuleDeclarations,
    func: &mut ClFunction,
    id: MachineFuncId,
) -> cranelift_codegen::ir::FuncRef {
    let decl = machine.get_function_decl(id);
    let signature = func.import_signature(decl.signature.clone());
    let name = func.declare_imported_user_function(UserExternalName {
        namespace: 0,
        index: id.as_u32(),
    });
    func.import_function(ExtFuncData {
        name: ExternalName::user(name),
        signature,
        colocated: decl.linkage.is_final(),
        patchable: false,
    })
}

/// A reference to a declared data object, usable inside one function.
///
/// The same as [`func_ref`] for the other namespace. The module's own version of
/// this is already `&self`, so this exists only so that lowering names one thing
/// — the declarations — rather than a module for data and declarations for
/// functions.
pub fn data_ref(machine: &ModuleDeclarations, func: &mut ClFunction, id: DataId) -> GlobalValue {
    let decl = machine.get_data_decl(id);
    let name = func.declare_imported_user_function(UserExternalName {
        namespace: 1,
        index: id.as_u32(),
    });
    func.create_global_value(GlobalValueData::Symbol {
        name: ExternalName::user(name),
        offset: Imm64::new(0),
        colocated: decl.linkage.is_final(),
        tls: decl.tls,
    })
}

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
pub struct FunctionRefs {
    callees: Vec<Option<cranelift_codegen::ir::FuncRef>>,
    shapes: Vec<Option<SigRef>>,
    entries: [Option<cranelift_codegen::ir::FuncRef>; RtEntry::COUNT],
}

impl Default for FunctionRefs {
    fn default() -> Self {
        Self {
            callees: Vec::new(),
            shapes: Vec::new(),
            entries: [None; RtEntry::COUNT],
        }
    }
}

impl FunctionRefs {
    /// No references yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// A reference to a declared callee, made on first use.
    ///
    /// Built from the module's declarations rather than by assembling a name.
    /// How a declared function is named inside a compilation is the module's
    /// business; [`func_ref`] is that convention read out of the module's source
    /// once, in one place, rather than a second copy spread over call sites.
    pub fn callee(
        &mut self,
        machine: &ModuleDeclarations,
        func: &mut ClFunction,
        declarations: &Declarations,
        id: FuncId,
    ) -> Option<cranelift_codegen::ir::FuncRef> {
        grow(&mut self.callees, id.index());
        if let Some(existing) = self.callees[id.index()] {
            return Some(existing);
        }

        let machine_id = declarations.machine_id(id)?;
        let reference = func_ref(machine, func, machine_id);
        self.callees[id.index()] = Some(reference);
        Some(reference)
    }

    /// A reference to a runtime entry point, made on first use.
    ///
    /// Infallible, unlike [`callee`](Self::callee): the seven are declared before
    /// any function is lowered, so "not declared yet" is a state that no longer
    /// exists rather than an error every emission site has to carry.
    pub fn entry(
        &mut self,
        machine: &ModuleDeclarations,
        imports: &EntryImports,
        func: &mut ClFunction,
        entry: RtEntry,
    ) -> cranelift_codegen::ir::FuncRef {
        if let Some(existing) = self.entries[entry.index()] {
            return existing;
        }
        let reference = func_ref(machine, func, imports.machine_id(entry));
        self.entries[entry.index()] = Some(reference);
        reference
    }

    /// Which entry points this function ended up naming.
    ///
    /// Derived from the references it asked for, which is the same set by
    /// construction — as opposed to a flag each emission site would have to
    /// remember to set, which is rule 8's reason for computing it here.
    pub fn entries_used(&self) -> EntryTable {
        let mut used = EntryTable::new();
        for &entry in RtEntry::ALL {
            if self.entries[entry.index()].is_some() {
                used.mark(entry);
            }
        }
        used
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
