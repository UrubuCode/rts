//! A named blob of bytes, already fully known at compile time, exported from
//! an object-file destination.
//!
//! # How this differs from [`super::tables::AddressTable`]
//!
//! A table exists because an object file cannot answer "where did this function
//! end up" — that address is the linker's, so the table asks for it as a
//! relocation. A blob carries no such gap: every byte a client hands here was
//! already decided while compiling, so there is nothing for a linker to fill
//! in. That is the whole difference, and it is why this is plain data rather
//! than a description with relocations threaded through it.
//!
//! # Why the length rides inside the blob rather than beside it
//!
//! [`AddressTable`](super::tables::AddressTable)'s own header explains why ITS
//! length is the table's own first word rather than a separate file: a reader
//! that trusted a count from elsewhere could walk past what the linker actually
//! sized. The same argument applies here, with the same fix — a client that
//! wants a reader to know how many of the trailing bytes are real prepends the
//! count itself, inside `bytes`, before handing it to [`define_data_blob`].
//! This module does not impose that shape: it places exactly the bytes it is
//! given, because a client that already frames its own payload (as
//! `rts_host::object::manifest` does) would otherwise carry the count twice.

use cranelift_module::{DataDescription, Linkage, Module};

use super::TargetError;

/// A byte string, exported from the object under a fixed name, with no
/// relocations.
pub struct DataBlob<'a> {
    /// The symbol the blob is exported under.
    pub name: &'a str,
    /// The bytes themselves, copied into the object's data section verbatim —
    /// framing, if the reader needs any, is the caller's to have already
    /// written into them.
    pub bytes: &'a [u8],
}

/// Defines one blob against a destination module.
///
/// Takes `&mut dyn Module` directly rather than [`super::MachineModule`]: a
/// blob names no function, so it needs neither the function-declaration cache
/// nor the "was this id actually defined" check a relocation-bearing table
/// requires. Callable at any point after the destination exists — unlike
/// [`super::MachineModule::define_address_table`], nothing here depends on a
/// body having been compiled first.
pub(super) fn define_data_blob(
    module: &mut dyn Module,
    blob: &DataBlob<'_>,
) -> Result<(), TargetError> {
    let mut description = DataDescription::new();
    description.define(blob.bytes.to_vec().into_boxed_slice());
    // Eight bytes: the widest field a reader might decode out of the front of
    // the blob (a `u64` length prefix, as `rts_host::object::manifest`'s
    // embedding uses) — an alignment narrower than that would let the object
    // place it somewhere an unaligned `u64` read faults on a strict target.
    description.set_align(8);
    // Exported, because the reader is in another object — the archive that
    // supplies `main`, same as every [`super::tables::AddressTable`]. Read-only:
    // nothing writes it after the linker places it.
    let data = module.declare_data(blob.name, Linkage::Export, false, false)?;
    module.define_data(data, &description)?;
    Ok(())
}
