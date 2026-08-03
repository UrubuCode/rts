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
    let isa = host_isa()?;
    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
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
