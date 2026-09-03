//! The numbers that travel beside an object file, and their format.
//!
//! # Why any of this is outside the object
//!
//! [`super`]'s own doc comment has the reason: the key, literal and template
//! tables are per-COMPILATION data the language names by NUMBER, and the entry
//! must be called with those tables already seeded. This is the seed, written
//! next to the executable and read by the facade's `main`.
//!
//! # Why the format is hand-rolled
//!
//! The reader is an `extern "C" fn` in `rts-runtime`, a crate this one does not
//! depend on and which does not depend on this one — so there is no shared type
//! to derive a serializer against, and adding one would make the AOT facade
//! depend on the JIT host. The two sides are kept honest by a test that writes
//! a manifest here and reads it back with the reader's own rules, not by a
//! derive.
//!
//! # The format
//!
//! Little-endian throughout, in this order:
//!
//! ```text
//! u32 x3                      the singletons: undefined, null, hole
//! u8 x2, then 2 bytes padding the kinds: symbol, bigint
//! strings                     every property key text, in key order
//! units                       every string literal, as UTF-16 code units
//! templates                   u32 count, then per site u32 len + u32 x len
//! u32                         how many module bodies run before the entry
//! frames                      u32 count, then per frame the shape below
//! functions                   u32 count, then per function name/arity/flags
//! metas                       u32 count, then per module specifier/url/main
//! resolutions                 u32 count, then per entry three strings
//! ```
//!
//! where `strings` is a `u32` count followed by `u32` byte length + UTF-8
//! bytes per entry, and `units` is a `u32` count followed by `u32` unit count +
//! that many `u16` per entry. A **literal** is units and not text because
//! `"\uD83D"` is a legal string with no UTF-8 spelling, which is the same
//! reason `rts_core::entry::declare_literals` takes units.
//!
//! A frame is `ty`, `size`, `slots`, `label_field`, `resumed_field`,
//! `mode_field` as `u32`, then a `u32` count of parameter fields and that many
//! `u32`, then a `u32` that is 1 or 0 for whether a return field follows and a
//! `u32` that is it (0 when it does not). A function is its name as a string,
//! then `u32` arity, then `u8` has-prototype, `u8` constructs and 2 bytes of
//! padding. A meta is its specifier and url as strings, then `u8` main and 3
//! bytes of padding.
//!
//! # What is deliberately NOT in here
//!
//! An ADDRESS. Not one, anywhere — a frame's code and a function's entry point
//! are both addresses, and an address does not exist until a linker places the
//! object. Those cross as relocations in
//! [`rts_cranelift::target::AddressTable`], which is the linker answering a
//! question this process cannot. The manifest carries what the compiler knew
//! and the tables carry what only the linker knows, and neither restates the
//! other.

use super::ObjectProgram;

/// Writes everything but the object bytes, in the format `rts-runtime`'s `main`
/// reads.
pub fn write(path: &std::path::Path, program: &ObjectProgram) -> std::io::Result<()> {
    let mut out = Vec::new();
    for singleton in program.singletons {
        out.extend_from_slice(&singleton.to_le_bytes());
    }
    out.push(program.kinds[0]);
    out.push(program.kinds[1]);
    out.extend_from_slice(&[0u8; 2]);

    strings(&mut out, &program.keys);
    units(&mut out, &program.literals);

    count(&mut out, program.templates.len());
    for template in &program.templates {
        count(&mut out, template.len());
        for value in template {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    // How many entries of the module table run before the entry. The addresses
    // are the linker's; how many of them there are is the compiler's, and this
    // is where the two meet.
    out.extend_from_slice(&program.modules.to_le_bytes());

    count(&mut out, program.frames.len());
    for shape in &program.frames {
        for number in [
            shape.ty,
            shape.size,
            shape.slots,
            shape.label_field,
            shape.resumed_field,
            shape.mode_field,
        ] {
            out.extend_from_slice(&number.to_le_bytes());
        }
        count(&mut out, shape.param_fields.len());
        for field in &shape.param_fields {
            out.extend_from_slice(&field.to_le_bytes());
        }
        // Present-or-not as its own word rather than a sentinel index: field
        // zero is a real field, and a body that returns into it would be
        // indistinguishable from one that returns nothing.
        let (present, field) = match shape.return_field {
            Some(field) => (1u32, field),
            None => (0, 0),
        };
        out.extend_from_slice(&present.to_le_bytes());
        out.extend_from_slice(&field.to_le_bytes());
    }

    count(&mut out, program.function_names.len());
    for (name, arity, has_prototype, constructs) in &program.function_names {
        string(&mut out, name);
        out.extend_from_slice(&arity.to_le_bytes());
        out.push(u8::from(*has_prototype));
        out.push(u8::from(*constructs));
        out.extend_from_slice(&[0u8; 2]);
    }

    count(&mut out, program.module_metas.len());
    for meta in &program.module_metas {
        string(&mut out, &meta.specifier);
        string(&mut out, &meta.url);
        out.push(u8::from(meta.main));
        out.extend_from_slice(&[0u8; 3]);
    }

    count(&mut out, program.resolutions.len());
    for (from, written, resolved) in &program.resolutions {
        string(&mut out, from);
        string(&mut out, written);
        string(&mut out, resolved);
    }

    std::fs::write(path, out)
}

/// A count, as the `u32` every table in this format starts with.
fn count(out: &mut Vec<u8>, many: usize) {
    out.extend_from_slice(&(many as u32).to_le_bytes());
}

/// One length-prefixed UTF-8 string.
fn string(out: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    count(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// A table of length-prefixed strings.
fn strings(out: &mut Vec<u8>, texts: &[String]) {
    count(out, texts.len());
    for text in texts {
        string(out, text);
    }
}

/// A table of code-unit sequences.
///
/// A separate writer rather than encoding the units as UTF-8 first: the whole
/// point of carrying units is that a lone surrogate has no UTF-8 spelling, and
/// an AOT binary that lost one where a JIT run kept it would be this crate's
/// rule 4 broken — the two destinations differ about the destination, never
/// about what was compiled.
fn units(out: &mut Vec<u8>, literals: &[Vec<u16>]) {
    count(out, literals.len());
    for units in literals {
        count(out, units.len());
        for unit in units {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }
}
