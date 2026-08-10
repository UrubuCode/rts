//! Placing a program into an object file instead of this process's memory.
//!
//! # What this shares with [`crate::run`], and what it does not
//!
//! Rule 4 of this crate's `README.md`: "both destinations, or neither... the
//! difference is stated and is about the destination — not about what was
//! compiled." Parsing, emission, the generator rewrite, the runtime-operation
//! agreement check and the verifier are [`crate::run::front_end`] and
//! [`crate::run::prepare`] — shared, unchanged, called from here exactly as
//! [`crate::run::compile_for`] calls them. Everything in *this* file is about
//! where the bytes end up and what a linker needs to finish the job.
//!
//! # The region base
//!
//! An in-memory run bakes the heap's address into the code as an immediate,
//! because the host that allocates the region already knows where it put it.
//! An object file cannot do that — the region does not exist until the binary
//! runs — so the heap here is [`RegionBase::Symbol`]: a named cell the object
//! leaves undefined, for the archive that defines `main` to fill in before the
//! first allocation. See `rts-runtime/src/aot.rs` for the write, and for why
//! its ordering is the one thing about this design that cannot be gotten wrong
//! quietly.
//!
//! # What crosses beside the object bytes, and why it is not IN them
//!
//! Property keys, string literals and template pieces are per-COMPILATION data
//! that the language names by NUMBER — `declare_keys`/`declare_literals`/
//! `declare_templates` seed the very tables those numbers index into, and this
//! is the same seeding [`crate::run::run_region`] does for a JIT run, just
//! before the entry is ever called. An AOT binary needs the identical seed at
//! the identical moment, and it cannot come from Cranelift-declared data without
//! solving a problem this increment does not need to solve (frame shapes and
//! function-name tables have the same requirement and the same absence, and
//! both are already the stated gap — see [`Manifest`]'s doc comment).
//!
//! So the numbers travel as a small sidecar file next to the executable,
//! written by [`write_manifest`] and read by the facade's own `main` before it
//! calls the compiled entry. This is the one place this design is not fully
//! self-contained in a single file, and it is named here rather than left to be
//! discovered: moving an AOT binary without its `.rtsdata` sidecar breaks it.
//! The alternative — encoding the same bytes as Cranelift-declared data,
//! exported under a name — was rejected for this increment because a `String`
//! table has no relocation problem but [`crate::run::assemble`]'s function-name
//! and frame-shape tables (deferred here, see [`Manifest`]) do: each entry names
//! a CODE ADDRESS, which does not exist until the object is placed by a linker
//! that this crate never runs. Splitting the two into "data that can ship inside
//! the object" and "data that cannot without a linker in this process" would be
//! two mechanisms for one manifest; one file for all of it is the one that
//! exists today.

use rts_cranelift::mem::{RegionBase, RegionBases};
use rts_cranelift::target::{Placing, Visibility, object_file, place_in_object};

use crate::link::HostError;
use crate::run::{Prepared, front_end, prepare};

/// The name of the cell compiled code reads the region's base address from.
///
/// Declared into the object as [`RegionBase::Symbol`], left undefined there, and
/// defined by `rts-runtime`'s `main` — see this module's own doc comment.
pub const REGION_BASE_SYMBOL: &str = "__rts_region_base";

/// A compiled program, placed into an object file's bytes, plus what the
/// facade's `main` has to seed before calling `__rts_script`.
pub struct ObjectProgram {
    /// The relocatable object. Undefined names: every `RuntimeOp` this program
    /// calls, every `RtEntry` the machine emits, and [`REGION_BASE_SYMBOL`] —
    /// all left for a linker to resolve against the `rts-runtime` archive.
    pub bytes: Vec<u8>,
    /// What the compiler numbered the singletons — copied out because a linked
    /// binary has no [`ValueModel`] to ask; see [`crate::link::singletons_for`].
    pub singletons: [u32; 3],
    /// The kinds the language declared for itself — see
    /// [`crate::link::kinds_for`].
    pub kinds: [u8; 2],
    /// Every property key text the compilation minted, in key order. Seeded by
    /// `declare_keys` before the entry runs — same requirement `run_region`
    /// states, same order.
    pub keys: Vec<String>,
    /// Every string literal, by position. Seeded by `declare_literals`.
    pub literals: Vec<String>,
    /// Every tagged-template site, by literal position. Seeded by
    /// `declare_templates`, after the literals.
    pub templates: Vec<Vec<u32>>,
}

// What is deliberately not carried across this boundary yet: a generator
// body's `FrameShape` and a stack trace's function-name table. Both name a
// CODE ADDRESS, which does not exist until a linker places the archive — in a
// process this crate never runs — so neither can be filled the way
// `crate::run::assemble` fills them for a JIT run. Closing this needs the
// object to export those tables as *relocations* against its own function
// symbols, which `rts-cranelift`'s object destination does not do today.
// Until then, an AOT-compiled generator places and links but a resumed frame
// reads a zeroed shape, and an uncaught throw's message carries no `at name`
// lines — neither silent: the first is a wrong answer a generator fixture
// would find, and the second is stated in this increment's own report.

/// Compiles source text into an object file, ready to link against the
/// `rts-runtime` archive.
pub fn compile_to_object(source: &str) -> Result<ObjectProgram, HostError> {
    let front = front_end(source)?;
    let model = front.model;
    let names = front.names;
    let Prepared {
        emitted,
        funcs,
        types,
        script: _,
        frames: _,
        expected,
        names_for_placing,
    } = prepare(front.emitted, front.funcs, front.types, front.calls)?;

    let mut placing: Vec<Placing<'_>> = expected
        .iter()
        .map(|(op, id)| Placing {
            id: *id,
            name: op.symbol(),
            visibility: Visibility::Expected,
            body: None,
        })
        .collect();
    for ((id, body), name) in emitted.functions.iter().zip(&names_for_placing) {
        placing.push(Placing {
            id: *id,
            name,
            visibility: Visibility::Exported,
            body: Some(body),
        });
    }

    // Single region, symbolic base: the address is not known until this binary
    // runs. Stride is a machine constant, not a fact about any particular
    // `Region` — see `rts_core::heap::STRIDE`'s own doc comment.
    let bases = RegionBases::single(
        RegionBase::Symbol(REGION_BASE_SYMBOL.to_owned()),
        rts_core::heap::STRIDE,
    );

    let object = object_file("rts_program")?;
    let bytes = place_in_object(object, &placing, &funcs, &types, Some(bases))?;

    let singletons = crate::link::singletons_for(&model);
    let kinds = crate::link::kinds_for(&model);

    Ok(ObjectProgram {
        bytes,
        singletons: [singletons.undefined, singletons.null, singletons.hole],
        kinds: [kinds.symbol, kinds.bigint],
        keys: names
            .keyed_texts()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        literals: emitted.literals,
        templates: emitted.templates,
    })
}

/// Writes everything but the object bytes to `path`, in the format
/// `rts-runtime`'s `main` reads.
///
/// # Format
///
/// Little-endian throughout. Three `u32` singletons, then two `u8` kinds
/// (padded to 4 bytes), then three tables — keys, literals, templates — each a
/// `u32` count followed by its entries. A key or literal entry is a `u32` byte
/// length followed by UTF-8 bytes. A template entry is a `u32` length followed
/// by that many `u32` values.
///
/// This is hand-rolled rather than an existing serializer because the reader is
/// a `#![no_std]`-shaped `extern "C" fn` in a different crate with no shared
/// type to derive against — the two sides are kept honest by
/// `rts-runtime/tests` reading a manifest this function wrote, not by a
/// shared derive.
pub fn write_manifest(path: &std::path::Path, program: &ObjectProgram) -> std::io::Result<()> {
    let mut out = Vec::new();
    for singleton in program.singletons {
        out.extend_from_slice(&singleton.to_le_bytes());
    }
    out.push(program.kinds[0]);
    out.push(program.kinds[1]);
    out.extend_from_slice(&[0u8; 2]);

    write_strings(&mut out, &program.keys);
    write_strings(&mut out, &program.literals);

    out.extend_from_slice(&(program.templates.len() as u32).to_le_bytes());
    for template in &program.templates {
        out.extend_from_slice(&(template.len() as u32).to_le_bytes());
        for value in template {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    std::fs::write(path, out)
}

fn write_strings(out: &mut Vec<u8>, strings: &[String]) {
    out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    for text in strings {
        let bytes = text.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
}
