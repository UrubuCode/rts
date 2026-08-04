//! The two destinations.
//!
//! Executable memory and an object file. They differ in exactly one thing —
//! what happens to the bytes at the end — and everything before that is the same
//! pipeline, which is why they are two constructors here rather than two paths
//! through the crate.

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::ModuleError;
use cranelift_object::{ObjectBuilder, ObjectModule};

use super::{TargetError, host_isa};

/// A module that compiles into this process's own memory.
///
/// Nothing is written anywhere; the result is code that can be called as soon as
/// it is finalized. What it can call is whatever was registered with it, because
/// there is no linker in the loop to resolve a name against anything else.
pub fn executable_memory() -> Result<JITModule, TargetError> {
    executable_memory_calling(&[])
}

/// The same, able to call code this process already has.
///
/// # Why this exists separately from the entry table
///
/// The comment above says a JIT module can call "whatever was registered with
/// it", and until this function nothing could register anything: the builder was
/// constructed and consumed in one expression, so the only reachable
/// destination was one that could call nothing outside itself.
///
/// [`crate::symbols::EntryTable`] is not the same mechanism and does not replace
/// this. It serves [`crate::symbols::RtEntry`] — the operations **this layer**
/// cannot emit as instructions — and a language's own runtime is not in that
/// set, by the same rule that keeps it short. A host compiling JavaScript has to
/// hand over the address of `__rts_add`, and there was no argument to hand it
/// through.
///
/// The object-file destination needs none of this, and the asymmetry is real
/// rather than an oversight: there, an undefined symbol is the linker's to
/// resolve against an archive. Here there is no linker, so the addresses are the
/// caller's to supply.
///
/// # Safety
///
/// Each address must point at a function that is alive for as long as the module
/// is, and whose signature matches what the compiled code was told to expect. A
/// mismatch is a call through the wrong shape, which no verifier here can see —
/// the code being called was not built by this crate.
pub fn executable_memory_calling(
    symbols: &[(&str, *const u8)],
) -> Result<JITModule, TargetError> {
    let isa = host_isa()?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for (name, address) in symbols {
        builder.symbol(*name, *address);
    }
    Ok(JITModule::new(builder))
}

/// A module that compiles into an object file for a linker to resolve later.
///
/// The mirror of the above, and the reason a runtime entry point needs no table
/// on this path: an undefined symbol in an object file is resolved by the
/// linker, against the archive, using the object format's own symbol table.
/// Building a name-to-address map for this path would be solving a problem it
/// does not have.
pub fn object_file(name: &str) -> Result<ObjectModule, TargetError> {
    let isa = host_isa()?;
    let builder = ObjectBuilder::new(isa, name, cranelift_module::default_libcall_names())
        .map_err(|error| TargetError::Module(ModuleError::Backend(error.into())))?;
    Ok(ObjectModule::new(builder))
}
