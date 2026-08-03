//! The second rendering: entry points reached by INDEX, not by name.
//!
//! Same declarations, same scan, same order. What differs is what a call site
//! has to hold to reach one.
//!
//! # Why a second rendering rather than a second source
//!
//! `#[rtse::*]` declares; a baker links. The declaration is the asset and the
//! generated file is one *view* of it — so an engine that wants a different
//! linkage gets another view, not another list to keep in step. Nothing about
//! [`symbol_table`](crate::emit) changes, and the engine that reads it does not
//! learn that this exists.
//!
//! # What is actually different
//!
//! A name is a **linker** concept: it exists so something *outside* can find the
//! thing. Three populations hide inside one table, and only one of them needs a
//! name:
//!
//! | population | needs a name |
//! |---|---|
//! | runtime helpers only the codegen calls | no — the address is known when the code is emitted |
//! | the AOT link surface | one per relocation target, which need not be one per entry |
//! | a foreign contract (N-API) | yes, permanently: the name *is* the interface |
//!
//! For the first, a string is a detour: `"io.print"` → hash → address, when the
//! address was already known. An index is the direct thing, and it is what
//! `rts-cranelift` already does with [`RtEntry`] — an explicitly numbered enum
//! whose discriminant keys a dense array, so no string is hashed on the path
//! that emits code.
//!
//! [`RtEntry`]: https://docs.rs/rts-cranelift
//!
//! This file is that shape, generated: `ENTRIES` is a dense array indexed by
//! `EntryIndex`, holding the address and the ABI signature of each declaration.
//!
//! # The danger, and the guard
//!
//! Names fail loudly. If the codegen and the runtime archive disagree about a
//! symbol, the link fails and nothing runs.
//!
//! Indices fail **quietly**. A codegen built against one table calling into an
//! archive built from another dispatches to the wrong function — same address
//! space, different signature, arguments read as the wrong types. No crash at
//! the call, just a wrong program.
//!
//! That is a worse failure mode than the one being replaced, so it is not
//! traded away for free: [`TABLE_HASH`] is a digest of the whole table, emitted
//! into this file and therefore compiled into both sides. Comparing it once,
//! at startup, turns a silent mismatch back into a loud one. **Indices as
//! linkage without a version check is exchanging a loud bug for a quiet one**,
//! which is the opposite of the trade that makes this worth doing.
//!
//! # Determinism
//!
//! The index of an entry is its position in the same strictly-ascending
//! name order [`crate::emit`] uses. So the numbering is a fact about the set,
//! not about the order someone wrote things in, and a renumbering is visible in
//! the diff of a checked-in artefact — the property that makes an index safe to
//! be a linkage at all.

use std::collections::BTreeMap;

use anyhow::Result;

use crate::scan::Declaration;

/// Default output path, relative to the workspace root.
pub const DEFAULT_OUT: &str = "crates/rts-symbol-baker/generated/entries.rs";

/// Render the index-addressed view of `decls`.
///
/// Takes the declarations already sorted and validated by [`crate::emit`], so
/// the two files describe the same set in the same order by construction rather
/// than by two sorts that could drift.
pub fn render(decls: &[Declaration]) -> Result<String> {
    let groups = group_by_symbol(decls);
    let hash = table_hash(&groups);

    let mut out = String::with_capacity(groups.len() * 160);
    header(&mut out, groups.len(), hash);
    externs(&mut out, &groups);
    entry_table(&mut out, &groups);
    Ok(out)
}

/// One row per distinct symbol name, keeping `#[cfg]` alternatives together.
///
/// Grouped for the same reason [`crate::emit`] groups: a platform pair shares a
/// name, exactly one alternative compiles, and both must occupy **one** index
/// so the array's length — and every index in it — is the same on every target.
/// An index that moved with the platform would not be a linkage.
fn group_by_symbol(decls: &[Declaration]) -> Vec<Vec<&Declaration>> {
    let mut by_name: BTreeMap<&str, Vec<&Declaration>> = BTreeMap::new();
    for declaration in decls {
        by_name
            .entry(declaration.symbol.as_str())
            .or_default()
            .push(declaration);
    }
    by_name.into_values().collect()
}

/// A digest of what this table *is*.
///
/// Covers the index, the name and the ABI signature of every entry — everything
/// a caller relies on. Deliberately not the file's bytes: a comment or a
/// formatting change would alter those and would not alter what a call means,
/// and a version check that fires on cosmetics is a version check people learn
/// to bypass.
///
/// FNV-1a, 64-bit. Chosen because it is eight lines, has no dependency, and
/// needs no collision resistance — it is guarding against a stale build, not an
/// adversary. A cryptographic digest here would imply a threat model that does
/// not exist.
fn table_hash(groups: &[Vec<&Declaration>]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };

    for (index, group) in groups.iter().enumerate() {
        feed(&(index as u64).to_le_bytes());
        feed(group[0].symbol.as_bytes());
        match &group[0].sig {
            None => feed(b"?"),
            Some((params, ret)) => {
                feed(b"(");
                for parameter in params {
                    feed(&[*parameter as u8]);
                }
                feed(b")");
                feed(&[*ret as u8]);
            }
        }
        feed(b"\x1e");
    }
    hash
}

fn header(out: &mut String, count: usize, hash: u64) {
    out.push_str(
        "// @generated by rts-symbol-baker. Do not edit.\n\
         //\n\
         // Entry points addressed by INDEX. The companion file, symbol_table.rs,\n\
         // describes the same declarations addressed by NAME; both are rendered\n\
         // from one scan, in one order, so they cannot describe different sets.\n\
         //\n\
         // An index is a linkage here, which means a caller compiled against one\n\
         // version of this table and a runtime built from another would dispatch\n\
         // to the wrong function silently. TABLE_HASH exists to stop that: it is\n\
         // compiled into both sides, and comparing it once at startup turns a\n\
         // silent mismatch back into a loud one.\n\n",
    );

    out.push_str("/// How many entry points there are.\n");
    out.push_str(&format!("pub const ENTRY_COUNT: usize = {count};\n\n"));

    out.push_str(
        "/// Identifies this table's contents: every index, name and signature.\n\
         ///\n\
         /// Not a digest of the file — a comment does not change what a call\n\
         /// means, and a check that fires on cosmetics is a check people learn to\n\
         /// bypass.\n",
    );
    out.push_str(&format!("pub const TABLE_HASH: u64 = 0x{hash:016x};\n\n"));

    out.push_str(
        "/// A position in [`ENTRIES`].\n\
         ///\n\
         /// The number is a fact about the sorted set of declarations, not about\n\
         /// the order anything was written in, so a renumbering shows up in the\n\
         /// diff of this checked-in file.\n\
         pub type EntryIndex = u32;\n\n\
         /// What a call site needs to reach one entry point.\n\
         #[derive(Clone, Copy)]\n\
         pub struct Entry {\n\
         \x20   /// The linker name. Kept for diagnostics — a backtrace naming\n\
         \x20   /// `rts_alloc` is readable and one naming index 47 is not — and\n\
         \x20   /// deliberately not used to reach the entry.\n\
         \x20   pub name: &'static str,\n\
         \x20   /// Where it is. Resolved by the linker, so this is a legal static\n\
         \x20   /// initializer and costs nothing at startup.\n\
         \x20   pub addr: *const u8,\n\
         \x20   /// Parameter types, or empty when the declaration had no ABI\n\
         \x20   /// spelling. An absent signature is absent, never guessed.\n\
         \x20   pub params: &'static [rts_abi::AbiType],\n\
         \x20   /// Return type, if the signature is known.\n\
         \x20   pub ret: Option<rts_abi::AbiType>,\n\
         }\n\n\
         // SAFETY: the pointer is a function address fixed by the linker. It is\n\
         // never dereferenced as data and never written through.\n\
         unsafe impl Sync for Entry {}\n\n",
    );
}

fn externs(out: &mut String, groups: &[Vec<&Declaration>]) {
    out.push_str("unsafe extern \"C\" {\n");
    for (index, group) in groups.iter().enumerate() {
        for declaration in group {
            for cfg in &declaration.cfgs {
                out.push_str(&format!("    #[cfg({cfg})]\n"));
            }
            out.push_str(&format!(
                "    #[link_name = \"{}\"]\n    fn __rts_entry_{index}();\n",
                declaration.symbol
            ));
        }
    }
    out.push_str("}\n\n");
}

fn entry_table(out: &mut String, groups: &[Vec<&Declaration>]) {
    out.push_str(
        "/// Every entry point, indexed by [`EntryIndex`].\n\
         ///\n\
         /// A `static` array in `.rodata`, not a `Vec` assembled at startup: the\n\
         /// promise that a lookup is an array read is only true if there is\n\
         /// nothing to build first.\n",
    );
    out.push_str(&format!("pub static ENTRIES: [Entry; ENTRY_COUNT] = [\n"));

    for (index, group) in groups.iter().enumerate() {
        let declaration = group[0];
        let (params, ret) = match &declaration.sig {
            Some((params, ret)) => (
                format!(
                    "&[{}]",
                    params
                        .iter()
                        .map(|t| crate::emit::abi_path_of(*t))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                format!("Some({})", crate::emit::abi_path_of(*ret)),
            ),
            None => ("&[]".to_owned(), "None".to_owned()),
        };

        out.push_str(&format!(
            "    Entry {{\n\
             \x20       name: {:?},\n\
             \x20       addr: __rts_entry_{index} as *const u8,\n\
             \x20       params: {params},\n\
             \x20       ret: {ret},\n\
             \x20   }},\n",
            declaration.symbol
        ));
    }

    out.push_str("];\n\n");

    out.push_str(
        "/// The index of a named entry, for the resolver that turns an import\n\
         /// into an index at compile time.\n\
         ///\n\
         /// A binary search over a sorted array, and the ONLY place a name is\n\
         /// matched. It runs while compiling, never while running: a call site\n\
         /// holds the index this returned.\n\
         pub fn index_of(name: &str) -> Option<EntryIndex> {\n\
         \x20   ENTRIES\n\
         \x20       .binary_search_by(|entry| entry.name.cmp(name))\n\
         \x20       .ok()\n\
         \x20       .map(|index| index as EntryIndex)\n\
         }\n",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_abi::AbiType;

    fn declaration(symbol: &str, sig: Option<(Vec<AbiType>, AbiType)>) -> Declaration {
        Declaration {
            symbol: symbol.to_owned(),
            cfgs: Vec::new(),
            sig,
            origin: "test".to_owned(),
        }
    }

    #[test]
    fn the_hash_changes_when_a_signature_changes() {
        let one = declaration("__RTS_FN_A", Some((vec![AbiType::I64], AbiType::Void)));
        let other = declaration("__RTS_FN_A", Some((vec![AbiType::F64], AbiType::Void)));

        assert_ne!(
            table_hash(&[vec![&one]]),
            table_hash(&[vec![&other]]),
            "a caller that passes an i64 where the runtime now reads an f64 must \
             not be told the tables agree"
        );
    }

    #[test]
    fn the_hash_changes_when_an_entry_moves() {
        let a = declaration("__RTS_FN_A", None);
        let b = declaration("__RTS_FN_B", None);

        let forward = vec![vec![&a], vec![&b]];
        let reversed = vec![vec![&b], vec![&a]];

        assert_ne!(
            table_hash(&forward),
            table_hash(&reversed),
            "the index IS the linkage, so a reordering is a different table"
        );
    }

    #[test]
    fn the_hash_ignores_where_a_declaration_came_from() {
        let here = declaration("__RTS_FN_A", None);
        let mut elsewhere = declaration("__RTS_FN_A", None);
        elsewhere.origin = "moved/to/another/file.rs".to_owned();

        assert_eq!(
            table_hash(&[vec![&here]]),
            table_hash(&[vec![&elsewhere]]),
            "moving a function between files does not change what a call means"
        );
    }

    #[test]
    fn a_platform_pair_occupies_one_index() {
        let mut posix = declaration("__RTS_FN_A", None);
        posix.cfgs = vec!["unix".to_owned()];
        let mut windows = declaration("__RTS_FN_A", None);
        windows.cfgs = vec!["windows".to_owned()];

        let both = [posix, windows];
        let groups = group_by_symbol(&both);
        assert_eq!(
            groups.len(),
            1,
            "an index that moved with the target would not be a linkage"
        );
        assert_eq!(groups[0].len(), 2, "and both alternatives are kept");
    }
}
