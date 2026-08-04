//! Placing a program somewhere it can be called, in this crate's own words.
//!
//! # Why this exists
//!
//! Rule 1 says no other crate in the workspace may depend on `cranelift-*`. The
//! placement surface underneath this module contradicted it: [`super::MachineModule`]
//! takes a `cranelift_module::Linkage` and `executable_memory` hands back a
//! `cranelift_jit::JITModule`, so a host trying to run a compiled program had to
//! name the code generator to do it.
//!
//! That was found the way these are usually found — by writing the crate that
//! could not obey both, and reaching for the exception instead of the cause. The
//! rule was right. The surface was not.
//!
//! So this says the same thing without the vocabulary leaking: which names are
//! visible, which are expected from elsewhere, and where the result ended up.
//!
//! # Why it is one call rather than a builder
//!
//! Declarations live in the `MachineModule` and the module borrows the
//! destination, so a handle that owned both would be self-referential. It is
//! also the wrong shape for the job: a host compiles a batch, and everything a
//! batch names has to be declared before any of it is defined.

use cranelift_module::Linkage;

use super::{MachineModule, TargetError, destination::executable_memory_calling};
use crate::ir::{FuncId, FuncRegistry, Function};
use crate::types::TypeRegistry;

/// How visible a name is where the program is placed.
///
/// This crate's word for what the code generator calls linkage. Three cases,
/// and the third is the one that matters here: a program that calls its runtime
/// names something it does not contain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Visibility {
    /// Callable from outside the program.
    Exported,
    /// Callable only from within it.
    Internal,
    /// Named here, defined elsewhere.
    Expected,
}

impl Visibility {
    /// The code generator's spelling.
    fn linkage(self) -> Linkage {
        match self {
            Visibility::Exported => Linkage::Export,
            Visibility::Internal => Linkage::Local,
            Visibility::Expected => Linkage::Import,
        }
    }
}

/// A function to place, and what it is called.
pub struct Placing<'a> {
    /// Which function.
    pub id: FuncId,
    /// The name it is placed under.
    pub name: &'a str,
    /// How visible it is.
    pub visibility: Visibility,
    /// Its body, absent when it is expected from elsewhere.
    pub body: Option<&'a Function>,
}

/// A program in this process's memory, with the addresses of what it defines.
///
/// # Why the module is kept rather than dropped
///
/// It owns the pages the code lives in. Dropping it while an address is still
/// held would leave a pointer into unmapped memory, so the handle keeps it and
/// the addresses stay valid exactly as long as the handle does.
pub struct InMemory {
    module: cranelift_jit::JITModule,
    placed: Vec<(FuncId, *const u8)>,
}

impl InMemory {
    /// Where a placed function ended up.
    pub fn address_of(&self, id: FuncId) -> Option<*const u8> {
        self.placed
            .iter()
            .find(|(placed, _)| *placed == id)
            .map(|(_, address)| *address)
    }

    /// How many functions were placed.
    pub fn len(&self) -> usize {
        self.placed.len()
    }

    /// Whether nothing was placed.
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }
}

impl Drop for InMemory {
    fn drop(&mut self) {
        // The module frees the pages it allocated. Nothing that came out of
        // `address_of` may be called after this, which is why the handle is what
        // a caller holds rather than the addresses alone.
        //
        // SAFETY: the caller is dropping this, which is their statement that no
        // address obtained from it is still in use.
        unsafe {
            let module = std::mem::replace(
                &mut self.module,
                executable_memory_calling(&[]).expect("this machine hosts its own code"),
            );
            module.free_memory();
        }
    }
}

/// Compiles a batch of functions into this process's memory.
///
/// `outside` gives the address of everything the program expects but does not
/// contain — its runtime. There is no linker on this path, so nothing else can
/// supply them; the object-file destination needs none of it, because there an
/// undefined name is resolved against an archive.
///
/// # Safety
///
/// Every address in `outside` must point at a function that outlives the
/// returned handle and whose signature matches what the program was compiled to
/// expect. Neither is checkable here: the code being called was not built by
/// this crate, so nothing in it can be verified by it.
pub unsafe fn place_in_memory(
    program: &[Placing<'_>],
    outside: &[(&str, *const u8)],
    funcs: &FuncRegistry,
    types: &TypeRegistry,
    heap: Option<crate::mem::RegionBases>,
) -> Result<InMemory, TargetError> {
    let mut jit = executable_memory_calling(outside)?;

    let mut machine_ids = Vec::new();
    {
        // How a reference becomes an address is a property of the heap rather
        // than of any function compiled against it, which is why it is given to
        // the module once and not passed at each definition.
        let mut module = MachineModule::new(&mut jit);
        if let Some(bases) = heap {
            module = module.with_heap(bases);
        }

        // Every name first, then every body. A program whose functions call
        // each other needs all of them declared before any is defined, and
        // doing it in two passes is what makes the order they were listed in
        // irrelevant.
        for placing in program {
            module.declare(
                placing.id,
                placing.name,
                placing.visibility.linkage(),
                funcs,
            )?;
        }
        for placing in program {
            if let Some(body) = placing.body {
                module.define(placing.id, body, funcs, types)?;
            }
        }

        for placing in program {
            if placing.body.is_some() {
                let machine_id = module
                    .declarations()
                    .machine_id(placing.id)
                    .ok_or(TargetError::UndeclaredFunction(placing.id))?;
                machine_ids.push((placing.id, machine_id));
            }
        }
    }

    jit.finalize_definitions()
        .map_err(|error| TargetError::Module(error))?;

    let placed = machine_ids
        .into_iter()
        .map(|(id, machine_id)| (id, jit.get_finalized_function(machine_id)))
        .collect();

    Ok(InMemory { module: jit, placed })
}
