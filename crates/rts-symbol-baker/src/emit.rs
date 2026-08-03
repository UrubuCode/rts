//! Emission: turn discovered declarations into the generated `.rs`.
//!
//! The output is a plain module body meant to be `include!`d by whichever crate
//! owns the JIT/AOT symbol installation. It names no Rust module paths — every
//! address comes from an `unsafe extern "C"` declaration with an explicit
//! `#[link_name]` — so it depends only on the symbols being LINKED, not on them
//! being `pub`-reachable. Its single dependency is `rts-abi`, for the shared
//! [`rts_abi::table::SymbolEntry`] type.
//!
//! ## Determinism
//!
//! Rendering is a pure function of the declaration set: entries are sorted by
//! symbol name (ties broken by source path), the internal extern identifiers are
//! assigned `__rts_sym_<i>` AFTER sorting, and nothing timestamped or
//! path-absolute is written. Two bakes of unchanged sources are byte-identical,
//! which is what makes `--check` a usable CI drift guard.
//!
//! ## The tables are `static`, not built at startup
//!
//! `SYMBOLS` / `AOT_SYMBOLS` / `SIGNATURES` are `pub static` arrays — placed in
//! `.rodata` by the linker — not functions that assemble a `Vec` with 2000+
//! `push` calls on every JIT/AOT compile. `rts_abi::table`'s own doc-comment
//! promises "no allocation, no hashing, no startup cost proportional to the
//! table"; a `static` array is what makes that literally true instead of
//! aspirational. A function address IS a legal `static` initializer (its value
//! is resolved by the linker, not computed at const-eval time), so each row
//! reads `SymbolEntry { name: "…", ptr: __rts_sym_i as *const u8 }` directly in
//! the array literal — no per-row runtime work at all.
//!
//! A `#[cfg]`-gated platform-alternative pair (e.g. posix vs win32, sharing one
//! symbol NAME) cannot sit on an array ELEMENT — `#[cfg]` on a bare expression
//! is not stable Rust — so such a group is instead emitted as one `const` per
//! alternative, all sharing the SAME identifier: exactly one definition
//! compiles per platform (the alternatives are mutually exclusive by
//! construction, see [`check_duplicates`]), and the array references that one
//! identifier. This keeps the array's LENGTH — and therefore the whole table's
//! shape — identical on every platform, which a fixed-size `static` array
//! requires. A symbol `#[cfg]`-gated with no matching alternative (no pairing)
//! cannot be represented this way — its row would have to vanish on the
//! non-matching platform, changing the array length — so [`check_duplicates`]
//! rejects it at bake time instead of silently miscompiling one platform.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::scan::Declaration;

/// Put the declarations in the order both renderings depend on, and reject the
/// sets neither can express.
///
/// Separate from [`render`] because there are now two views of one scan, and
/// they must be views of the *same* set in the *same* order. Two renderers that
/// each sorted for themselves would agree today and drift the day one of them
/// gained a tie-break — and the symptom would be a call reaching the wrong
/// function, not a build failure.
pub fn prepare(mut decls: Vec<Declaration>) -> Result<Vec<Declaration>> {
    decls.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.origin.cmp(&b.origin)));
    decls.dedup();
    check_duplicates(&decls)?;
    Ok(decls)
}

/// Sort + validate the declarations, then render the generated file.
pub fn render(decls: Vec<Declaration>) -> Result<String> {
    let decls = prepare(decls)?;
    Ok(render_prepared(&decls))
}

/// Render declarations already put in order by [`prepare`].
pub fn render_prepared(decls: &[Declaration]) -> String {
    let mut s = String::with_capacity(decls.len() * 200);
    header(&mut s, decls.len());
    externs(&mut s, decls);
    jit_table(&mut s, decls);
    aot_table(&mut s, decls);
    sig_table(&mut s, decls);
    s
}

/// A symbol may appear twice ONLY when every occurrence is `#[cfg]`-gated —
/// the platform-alternative shape (`posix` vs `win32`). Two ungated definitions
/// of one name are a real collision: the linker would pick one and the table
/// would silently install the wrong address.
///
/// A symbol `#[cfg]`-gated with NO alternative (declared once, under a single
/// `#[cfg]`) is also rejected: the baked tables are fixed-size `static` arrays
/// now, so a row's presence cannot vary by platform. Only a complete
/// alternative SET (2+ declarations sharing one symbol name, every one
/// `#[cfg]`-gated) can be merged into one row that compiles identically-sized
/// on every platform — see the module doc.
fn check_duplicates(decls: &[Declaration]) -> Result<()> {
    let mut groups: BTreeMap<&str, Vec<&Declaration>> = BTreeMap::new();
    for d in decls {
        groups.entry(d.symbol.as_str()).or_default().push(d);
    }
    for (name, group) in groups {
        if group.len() > 1 && group.iter().any(|d| d.cfgs.is_empty()) {
            let where_ = group
                .iter()
                .map(|d| d.origin.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("symbol `{name}` is declared more than once without a #[cfg] gate: {where_}");
        }
        if group.len() == 1 && !group[0].cfgs.is_empty() {
            bail!(
                "symbol `{name}` (at {}) is #[cfg]-gated with no platform-alternative \
                 pair. The baked table is a fixed-size `static` array, so a row's \
                 presence cannot vary by platform: give it a matching #[cfg]-gated \
                 alternative under the SAME symbol name (so exactly one compiles per \
                 platform), or drop the #[cfg].",
                group[0].origin
            );
        }
    }
    Ok(())
}

fn header(s: &mut String, n: usize) {
    s.push_str(
        "// @generated by `rts-symbol-baker` — DO NOT EDIT BY HAND.\n\
         //\n\
         // Regenerate with `cargo run -p rts-symbol-baker`; verify with\n\
         // `cargo run -p rts-symbol-baker -- --check` (CI runs the latter).\n\
         //\n\
         // ORDERING INVARIANT: entries are in strictly ascending byte order of the\n\
         // symbol name, so lookup is a binary search and every scope (`__rtsn_`,\n\
         // `__rtsm_node_fs_`, …) is ONE contiguous range. See `rts_abi::table`.\n\
         //\n\
         // STATIC, not built at startup: SYMBOLS / AOT_SYMBOLS / SIGNATURES below are\n\
         // `static` arrays in `.rodata`, not `Vec`s assembled by a `push` loop at\n\
         // JIT/AOT startup. `lookup` is therefore a real O(log n) binary search over a\n\
         // slice already in memory — no allocation, no hashing, nothing proportional\n\
         // to the table's size on every compile.\n\
         //\n",
    );
    s.push_str(&format!("// {n} symbols.\n\n"));
}

fn externs(s: &mut String, decls: &[Declaration]) {
    s.push_str(
        "// Addresses are taken through the LINKER name, not a Rust module path: that\n\
         // reaches every `#[no_mangle]` symbol regardless of Rust visibility, and makes\n\
         // a missing one a link error instead of a runtime crash.\n\
         //\n\
         // Every row is declared as a bare `fn()` because only its ADDRESS is ever\n\
         // taken — the table hands `*const u8` to `JITBuilder::symbol`, and nothing\n\
         // here ever CALLS through these declarations, so the parameter list is\n\
         // irrelevant and would only duplicate (and be able to drift from) the real\n\
         // signature, which `SymbolDesc` already owns. rustc cannot know that, so it\n\
         // warns when the host crate also declares the same symbol with its true\n\
         // signature; the allow below is scoped to this generated block and is the\n\
         // reason this file is emitted rather than hand-written.\n\
         #[allow(clashing_extern_declarations)]\n\
         unsafe extern \"C\" {\n",
    );
    for (i, d) in decls.iter().enumerate() {
        for cfg in &d.cfgs {
            s.push_str(&format!("    {cfg}\n"));
        }
        s.push_str(&format!(
            "    #[link_name = \"{}\"]\n    fn __rts_sym_{i}();\n",
            d.symbol
        ));
    }
    s.push_str("}\n\n");
}

/// Group an already symbol-sorted slice into contiguous runs sharing the same
/// `symbol` name — a `#[cfg]`-gated platform-alternative set lands in one run.
/// Requires `decls` sorted by symbol, which `render`'s initial sort guarantees.
fn group_by_symbol(decls: &[Declaration]) -> Vec<Vec<&Declaration>> {
    let mut groups: Vec<Vec<&Declaration>> = Vec::new();
    for d in decls {
        match groups.last_mut() {
            Some(g) if g[0].symbol == d.symbol => g.push(d),
            _ => groups.push(vec![d]),
        }
    }
    groups
}

/// Same grouping, over a slice of references (the `with_sig` subset) rather
/// than owned `Declaration`s.
fn group_ref_slice_by_symbol<'a>(items: &[&'a Declaration]) -> Vec<Vec<&'a Declaration>> {
    let mut groups: Vec<Vec<&Declaration>> = Vec::new();
    for d in items {
        match groups.last_mut() {
            Some(g) if g[0].symbol == d.symbol => g.push(*d),
            _ => groups.push(vec![*d]),
        }
    }
    groups
}

/// Same grouping, additionally carrying each declaration's position in the
/// FULL, flat `decls` slice — the index `externs()` used for its `__rts_sym_i`
/// identifier. Needed only by the JIT table: it is the only table that
/// addresses an extern (`AOT_SYMBOLS`/`SIGNATURES` are pure data, no `ptr`).
fn group_by_symbol_indexed(decls: &[Declaration]) -> Vec<Vec<(usize, &Declaration)>> {
    let mut groups: Vec<Vec<(usize, &Declaration)>> = Vec::new();
    for (i, d) in decls.iter().enumerate() {
        match groups.last_mut() {
            Some(g) if g[0].1.symbol == d.symbol => g.push((i, d)),
            _ => groups.push(vec![(i, d)]),
        }
    }
    groups
}

fn jit_table(s: &mut String, decls: &[Declaration]) {
    s.push_str(
        "/// Every RTS symbol with its address, for `JITBuilder::symbol`.\n\
         ///\n\
         /// A `static` array in `.rodata`, sorted ascending by name, one row per\n\
         /// unique symbol name. A `#[cfg]`-gated platform-alternative pair shares ONE\n\
         /// row via a same-named `const` defined once per branch — exactly one\n\
         /// definition compiles per platform, so the row count never varies.\n",
    );
    let groups = group_by_symbol_indexed(decls);
    let mut consts = String::new();
    let mut elems: Vec<String> = Vec::with_capacity(groups.len());
    for (k, group) in groups.iter().enumerate() {
        if let [(i, d)] = group.as_slice() {
            debug_assert!(
                d.cfgs.is_empty(),
                "a lone #[cfg]-gated row must have been rejected by check_duplicates"
            );
            elems.push(format!(
                "::rts_abi::table::SymbolEntry {{ name: \"{}\", ptr: __rts_sym_{i} as *const u8 }}",
                d.symbol
            ));
        } else {
            let name = format!("__RTS_SYM_ROW_{k}");
            for (i, d) in group {
                for cfg in &d.cfgs {
                    consts.push_str(cfg);
                    consts.push('\n');
                }
                consts.push_str(&format!(
                    "const {name}: ::rts_abi::table::SymbolEntry = ::rts_abi::table::SymbolEntry {{ name: \"{}\", ptr: __rts_sym_{i} as *const u8 }};\n",
                    d.symbol
                ));
            }
            elems.push(name);
        }
    }
    s.push_str(&consts);
    s.push_str(&format!(
        "pub static SYMBOLS: [::rts_abi::table::SymbolEntry; {}] = [\n",
        groups.len()
    ));
    for e in &elems {
        s.push_str(&format!("    {e},\n"));
    }
    s.push_str("];\n\n");
    s.push_str(
        "/// Slice accessor, kept for source compatibility with callers written\n\
         /// against the previous `Vec`-returning `symbols()` — now a zero-cost borrow\n\
         /// of the `static` table above, not a fresh allocation.\n\
         pub fn symbols() -> &'static [::rts_abi::table::SymbolEntry] { &SYMBOLS }\n\n",
    );
}

fn aot_table(s: &mut String, decls: &[Declaration]) {
    s.push_str(
        "/// The same symbols as names only, for the AOT object module's declaration\n\
         /// list. Same order as `SYMBOLS`; the two cannot diverge — both are grouped\n\
         /// from the same declarations. A `static` array in `.rodata`, like `SYMBOLS`.\n",
    );
    let groups = group_by_symbol(decls);
    let mut consts = String::new();
    let mut elems: Vec<String> = Vec::with_capacity(groups.len());
    for (k, group) in groups.iter().enumerate() {
        if let [d] = group.as_slice() {
            debug_assert!(
                d.cfgs.is_empty(),
                "a lone #[cfg]-gated row must have been rejected by check_duplicates"
            );
            elems.push(format!("\"{}\"", d.symbol));
        } else {
            // Every alternative in a platform-alternative group shares the SAME
            // symbol NAME (that is what makes them a group), so the row's payload
            // never actually varies by platform here — only `SYMBOLS`' address
            // does. Still gated per branch, for the same reason `SYMBOLS` is: it
            // keeps "exactly one definition compiles" a single mechanical rule
            // rather than a special case for the (currently never exercised) shape
            // where a future alternative's exported name really does differ.
            let name = format!("__RTS_AOT_ROW_{k}");
            for d in group {
                for cfg in &d.cfgs {
                    consts.push_str(cfg);
                    consts.push('\n');
                }
                consts.push_str(&format!("const {name}: &'static str = \"{}\";\n", d.symbol));
            }
            elems.push(name);
        }
    }
    s.push_str(&consts);
    s.push_str(&format!(
        "pub static AOT_SYMBOLS: [&'static str; {}] = [\n",
        groups.len()
    ));
    for e in &elems {
        s.push_str(&format!("    {e},\n"));
    }
    s.push_str("];\n\n");
    s.push_str(
        "/// Slice accessor returning the shared [`rts_abi::table::AotSymbols`] type —\n\
         /// the same alias the AOT declaration list is meant to consume.\n\
         pub fn aot_symbols() -> ::rts_abi::table::AotSymbols { &AOT_SYMBOLS }\n\n",
    );
}

/// The literal expression for one signature row: `("name", &[Type, …], Ret)`.
fn sig_row_expr(d: &Declaration) -> String {
    let (params, ret) = d.sig.as_ref().expect("filtered to Some");
    let ps: Vec<String> = params.iter().map(|p| abi_path_of(*p)).collect();
    format!(
        "(\"{}\", &[{}], {})",
        d.symbol,
        ps.join(", "),
        abi_path_of(*ret)
    )
}

/// The ABI SIGNATURES, for every `#[rtse::abi]` declaration whose types all have
/// an ABI spelling.
///
/// This is what lets a consumer stop hand-writing `(symbol → params, ret)` rows:
/// the signature is DERIVED from the Rust one through [`rts_abi::tymap`], the
/// same mapping `#[rtse::abi]` itself uses, so the emitted row and the
/// macro-emitted `SymbolDesc` cannot disagree — there is one derivation, read
/// twice.
///
/// A symbol with no entry here is simply not yet declared with `#[rtse::abi]`
/// (or uses a type the mapping does not cover). An ABSENT row, never a guessed
/// one: the caller falls back to whatever it used before.
fn sig_table(s: &mut String, decls: &[Declaration]) {
    let with_sig: Vec<&Declaration> = decls.iter().filter(|d| d.sig.is_some()).collect();
    let groups = group_ref_slice_by_symbol(&with_sig);
    s.push_str(&format!(
        "\n/// ABI signatures derived from the Rust signatures of `#[rtse::abi]` fns.\n\
         ///\n\
         /// A `static` array in `.rodata`, sorted ascending by name like `SYMBOLS` — a\n\
         /// lookup is a binary search, no `HashMap` build at startup. {} of {} symbols\n\
         /// carry one; the rest are not yet declared with `#[rtse::abi]`.\n",
        with_sig.len(),
        decls.len(),
    ));
    let mut consts = String::new();
    let mut elems: Vec<String> = Vec::with_capacity(groups.len());
    for (k, group) in groups.iter().enumerate() {
        if let [d] = group.as_slice() {
            // `group: &Vec<&Declaration>`, so the slice pattern binds `d` one
            // reference deeper than the element type (`&&Declaration`) —
            // deref once to match `sig_row_expr`'s `&Declaration` explicitly
            // rather than lean on call-site deref coercion.
            let d: &Declaration = *d;
            debug_assert!(
                d.cfgs.is_empty(),
                "a lone #[cfg]-gated row must have been rejected by check_duplicates"
            );
            elems.push(sig_row_expr(d));
        } else {
            let name = format!("__RTS_SIG_ROW_{k}");
            for d in group {
                let d: &Declaration = *d;
                for cfg in &d.cfgs {
                    consts.push_str(cfg);
                    consts.push('\n');
                }
                consts.push_str(&format!(
                    "const {name}: (&'static str, &'static [::rts_abi::AbiType], ::rts_abi::AbiType) = {};\n",
                    sig_row_expr(d)
                ));
            }
            elems.push(name);
        }
    }
    s.push_str(&consts);
    s.push_str(&format!(
        "pub static SIGNATURES: [(&'static str, &'static [::rts_abi::AbiType], ::rts_abi::AbiType); {}] = [\n",
        groups.len()
    ));
    for e in &elems {
        s.push_str(&format!("    {e},\n"));
    }
    s.push_str("];\n\n");
    s.push_str(
        "pub fn signatures() -> &'static [(&'static str, &'static [::rts_abi::AbiType], ::rts_abi::AbiType)] { &SIGNATURES }\n",
    );
}

/// The path spelling of an `AbiType` variant, for the generated source.
/// The path an `AbiType` is written as in generated code.
///
/// Shared with [`crate::entries`], which renders the same declarations under a
/// different linkage — two spellings of one mapping is a drift waiting to be
/// found by a wrong call rather than by a build.
pub fn abi_path_of(t: rts_abi::AbiType) -> String {
    let v = match t {
        rts_abi::AbiType::Void => "Void",
        rts_abi::AbiType::Bool => "Bool",
        rts_abi::AbiType::I32 => "I32",
        rts_abi::AbiType::I64 => "I64",
        rts_abi::AbiType::U64 => "U64",
        rts_abi::AbiType::F64 => "F64",
        rts_abi::AbiType::StrPtr => "StrPtr",
        rts_abi::AbiType::Handle => "Handle",
        rts_abi::AbiType::PolyValue => "PolyValue",
    };
    format!("::rts_abi::AbiType::{v}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl(symbol: &str, cfgs: &[&str]) -> Declaration {
        Declaration {
            symbol: symbol.into(),
            cfgs: cfgs.iter().map(|c| c.to_string()).collect(),
            sig: None,
            origin: "crates/x/src/a.rs".into(),
        }
    }

    /// A `#[rtse::abi]` declaration carries a derived signature into the emitted
    /// `SIGNATURES` table; a `#[no_mangle]` one contributes no row at all
    /// (absent, never guessed).
    #[test]
    fn signatures_table_holds_only_declarations_that_have_one() {
        let mut with = decl("__rtsadp_a", &[]);
        with.sig = Some((
            vec![rts_abi::AbiType::U64, rts_abi::AbiType::StrPtr],
            rts_abi::AbiType::Handle,
        ));
        let out = render(vec![with, decl("__rtsadp_b", &[])]).expect("render");
        assert!(
            out.contains(
                "(\"__rtsadp_a\", &[::rts_abi::AbiType::U64, \
                 ::rts_abi::AbiType::StrPtr], ::rts_abi::AbiType::Handle)"
            ),
            "the derived signature row is missing:\n{out}"
        );
        assert!(
            !out.contains("(\"__rtsadp_b\""),
            "a declaration with no signature must not get a row in SIGNATURES:\n{out}"
        );
    }

    #[test]
    fn output_is_sorted_and_deterministic() {
        let input = vec![decl("__rtsn_b", &[]), decl("__rtsm_a_a", &[])];
        let out = render(input.clone()).unwrap();
        assert!(out.find("__rtsm_a_a").unwrap() < out.find("__rtsn_b").unwrap());
        // Re-rendering a permutation gives the identical file.
        let mut rev = input;
        rev.reverse();
        assert_eq!(out, render(rev).unwrap());
    }

    #[test]
    fn ungated_duplicate_is_an_error() {
        let d = vec![
            decl("__rtsn_x", &[]),
            Declaration {
                origin: "crates/y/src/b.rs".into(),
                ..decl("__rtsn_x", &[])
            },
        ];
        assert!(render(d).is_err());
    }

    #[test]
    fn cfg_gated_duplicate_is_allowed_and_gated_in_output() {
        let d = vec![
            decl("__rtsn_x", &["#[cfg(unix)]"]),
            Declaration {
                origin: "crates/y/src/b.rs".into(),
                ..decl("__rtsn_x", &["#[cfg(windows)]"])
            },
        ];
        let out = render(d).unwrap();
        assert!(out.contains("#[cfg(unix)]"));
        assert!(out.contains("#[cfg(windows)]"));
    }

    /// The counterpart to the test above: a `#[cfg]`-gated symbol with NO
    /// alternative cannot size a fixed-length `static` array (its row would
    /// have to vanish on the non-matching platform), so it is a bake-time
    /// error rather than a silent per-platform table-shape difference.
    #[test]
    fn lone_cfg_without_platform_alternative_is_an_error() {
        let d = vec![decl("__rtsn_lonely", &["#[cfg(unix)]"])];
        assert!(render(d).is_err());
    }

    /// The tables are real `static` arrays, not `Vec`-building functions — the
    /// whole point of this emitter shape (see the module doc).
    #[test]
    fn tables_are_static_arrays_not_runtime_built_vecs() {
        let out = render(vec![decl("__rtsn_only", &[])]).unwrap();
        assert!(out.contains("pub static SYMBOLS: ["), "{out}");
        assert!(out.contains("pub static AOT_SYMBOLS: ["), "{out}");
        assert!(out.contains("pub static SIGNATURES: ["), "{out}");
        assert!(!out.contains("Vec::with_capacity"), "{out}");
        assert!(!out.contains("out.push"), "{out}");
    }
}
