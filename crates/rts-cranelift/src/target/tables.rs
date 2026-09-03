//! A name holding the addresses of functions this compilation also placed.
//!
//! # What this is for, and why nothing above could do it
//!
//! An in-memory placement hands every address back — [`super::InMemory::address_of`]
//! answers one per function, and a host reads them after `finalize_definitions`.
//! An object file has no such moment: the addresses do not exist until a linker
//! places the object, in a process this crate never runs. So a reader that needs
//! to know *where a function ended up* has, on that destination, nothing to ask.
//!
//! That absence is not abstract. It is why an object file's client could not
//! carry a table keyed by a code address — a parked frame's shape, or what a
//! function is called — and had to ship an empty one instead.
//!
//! A table closes it by asking the LINKER the question this crate cannot answer:
//! a data symbol of N pointer-sized words, each one a relocation against a
//! function symbol in the same object. The linker fills them in as it places
//! the code, which is the same mechanism that resolves an undefined `__rts_add`
//! against the runtime archive — the one difference between the destinations
//! that `rts-host`'s rule 4 already names and calls "about the destination, not
//! about what was compiled".
//!
//! # Why the table says how long it is
//!
//! The first word is the ENTRY COUNT, written by this crate, and the addresses
//! follow it. A reader could instead be told the count some other way, and the
//! first version of this was: the count travelled in `rts-host`'s manifest, a
//! separate file beside the executable.
//!
//! Two files are two things that can disagree. A manifest written by an earlier,
//! smaller compilation of the same output path — a link that succeeded and a
//! manifest write that then failed, or an executable moved without its sidecar,
//! which that design already warns about — would tell the reader to read PAST
//! the end of the table the linker sized. What it would then read is whatever
//! the linker put next in the image, and for the module table that word is
//! transmuted into a function pointer and CALLED.
//!
//! So the length is in the same file as the thing it measures, and the reader
//! checks the two against each other. One word per table, against a class of
//! failure whose symptom is a jump to arbitrary code.
//!
//! # Why the table is data and not a function that answers
//!
//! A function returning the i-th address would be a switch over
//! [`crate::ir::Inst::FuncAddr`], which is a body to emit, verify and lower for
//! something a relocation states directly. Data also costs nothing at run time:
//! a reader indexes an array, where a call costs a call.
//!
//! # Why an empty table is still a table
//!
//! Because a reader links against the name unconditionally. A program with no
//! generators would otherwise leave the symbol undefined and fail to LINK — a
//! whole-program failure for a property the program does not have. The count
//! word is there either way, and it reads zero.

use cranelift_module::{DataDescription, Linkage, Module};

use super::{MachineModule, TargetError};
use crate::ir::FuncId;

/// A name a destination exports, holding one address per listed function.
///
/// The order is the reader's index: entry `n` is the address of
/// `functions[n]`, and a reader that wants a different order asks for a
/// different list rather than sorting one this crate wrote.
///
/// # Layout
///
/// One pointer-sized word of COUNT, then that many addresses. See this module's
/// own doc comment for why the length travels with the table rather than beside
/// it.
pub struct AddressTable<'a> {
    /// The symbol the table is exported under.
    pub name: &'a str,
    /// Whose addresses it holds, in the order a reader indexes them.
    ///
    /// Every one of them must be a function this same placement DEFINES. A
    /// function that is only DECLARED — a runtime import, say — has no address
    /// in this object for a linker to write, and is refused rather than
    /// silently given the import's own address; see
    /// [`TargetError::UndeclaredFunction`].
    pub functions: &'a [FuncId],
}

impl MachineModule<'_> {
    /// Defines one table against this module.
    ///
    /// Called after every body has been compiled, for two reasons: a relocation
    /// names a function the module must already have an identifier for, and the
    /// refusal above needs to know which functions were DEFINED, which is not
    /// settled until they are.
    pub(super) fn define_address_table(
        &mut self,
        table: &AddressTable<'_>,
    ) -> Result<(), TargetError> {
        // The pointer width of the machine being compiled FOR, not of the one
        // compiling: a reader indexes this array by the same width the linker
        // writes into it, and both are the target's.
        let width = self.module.isa().pointer_bytes() as usize;

        // Bytes, and NOT `define_zeroinit`, which is the same size and the
        // wrong thing: zero-init means an UNINITIALIZED section — `.bss` — and
        // a section with no contents in the file has nowhere for a relocation
        // to be written. Measured rather than reasoned about: with
        // `define_zeroinit` the object still carried one relocation per entry
        // — `tests/target.rs`'s object-file test passes either way — the link
        // still succeeded, and every entry read back as the image base at run
        // time. Nothing but running it said otherwise.
        let mut bytes = vec![0u8; (table.functions.len() + 1) * width];
        // The count, in the first word. Written here rather than carried in a
        // second file, for the reason this module's header gives.
        let count = table.functions.len() as u64;
        let counted = width.min(size_of::<u64>());
        bytes[..counted].copy_from_slice(&count.to_le_bytes()[..counted]);

        let mut description = DataDescription::new();
        description.define(bytes.into_boxed_slice());
        description.set_align(width as u64);
        for (at, id) in table.functions.iter().enumerate() {
            // DEFINED, not merely declared. `Declarations::machine_id` answers
            // for a runtime import too — it is recorded at declaration — so
            // asking it alone would put the import's own address in a table
            // whose reader believes every entry is a function of this program.
            if !self.defined.contains(id) {
                return Err(TargetError::UndeclaredFunction(*id));
            }
            let machine_id = self
                .declarations
                .machine_id(*id)
                .ok_or(TargetError::UndeclaredFunction(*id))?;
            let reference = self.module.declare_func_in_data(machine_id, &mut description);
            // Past the count word.
            description.write_function_addr(((at + 1) * width) as u32, reference);
        }
        // Exported, because the reader is in another object — the archive that
        // supplies `main`. Read-only: nothing writes it after the linker does.
        let data = self
            .module
            .declare_data(table.name, Linkage::Export, false, false)?;
        self.module.define_data(data, &description)?;
        Ok(())
    }
}
