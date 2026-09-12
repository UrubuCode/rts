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
/// [`crate::symbols::EntryImports`] is not the same mechanism and does not replace
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
pub fn executable_memory_calling(symbols: &[(&str, *const u8)]) -> Result<JITModule, TargetError> {
    Ok(JITModule::new(builder_calling(symbols)?))
}

/// Executable memory for one program, all of it inside ONE reservation of
/// `bytes`, resolving the same names as [`executable_memory_calling`].
///
/// `target/arena.rs` says why a program of any size needs its code in one
/// place: a call between two functions of the same program is a 32-bit
/// displacement, and the default provider's chunks can land further apart
/// than that. Reserving is also committing on Windows, so `bytes` is a cost
/// and the caller sizes it from the program rather than guessing large.
pub fn executable_memory_in_arena(
    symbols: &[(&str, *const u8)],
    bytes: usize,
) -> Result<JITModule, TargetError> {
    let mut builder = builder_calling(symbols)?;
    let arena = cranelift_jit::ArenaMemoryProvider::new_with_size(bytes).map_err(|error| {
        TargetError::Module(ModuleError::Backend(
            anyhow::Error::new(error).context(format!("reserving {bytes} bytes of executable memory")),
        ))
    })?;
    builder.memory_provider(Box::new(arena));
    Ok(JITModule::new(builder))
}

/// The builder both executable destinations start from.
fn builder_calling(symbols: &[(&str, *const u8)]) -> Result<JITBuilder, TargetError> {
    let isa = host_isa()?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for (name, address) in symbols {
        builder.symbol(*name, *address);
    }
    Ok(builder)
}

/// A module that compiles into an object file for a linker to resolve later.
///
/// The mirror of the above, and the reason a runtime entry point needs no table
/// on this path: an undefined symbol in an object file is resolved by the
/// linker, against the archive, using the object format's own symbol table.
/// Building a name-to-address map for this path would be solving a problem it
/// does not have.
pub fn object_file(name: &str) -> Result<ObjectModule, TargetError> {
    // Addressed as this platform's loader requires, which is NOT the same answer
    // the in-memory destination gives — see `target::isa_with`. Code placed in
    // this process stays where it was relocated; code in an object file is
    // linked into an image the loader may put anywhere.
    let isa = super::isa_with(super::object_addressing(), super::Priority::CodeQuality)?;
    let builder = ObjectBuilder::new(isa, name, cranelift_module::default_libcall_names())
        .map_err(|error| TargetError::Module(ModuleError::Backend(error.into())))?;
    Ok(ObjectModule::new(builder))
}
