//! Discovery: find every exported `extern "C"` declaration in the runtime
//! crates' sources and derive its linker symbol.
//!
//! Two shapes are recognized, and they are recognized by the SAME rule the
//! compiler uses, not by a name pattern:
//!
//! 1. **`#[rtse::abi(...)]`** — the authoring surface. The attribute's arguments
//!    are parsed into an [`rts_abi::Naming`] and the symbol is derived by
//!    [`rts_abi::symbol_for`], the one implementation of the naming rule. The
//!    baker never re-derives a name of its own.
//! 2. **`#[unsafe(no_mangle)] extern "C" fn __…`** — the transitional shape.
//!    `no_mangle` means the linker name IS the Rust identifier, so the symbol is
//!    the identifier verbatim. These entries disappear from the scan on their own
//!    as each group converts to `#[rtse::abi]`; nothing has to be deleted here.
//! 3. **`#[rtse::class("Name")]` structs/impls and `#[rtse::constant]` consts** —
//!    symbols that exist only after macro expansion. See the `class` submodule.
//!
//! # What is EXCLUDED, and why (a symbol in the table must be LINKED)
//!
//! The generated table takes each address through an `extern "C"` declaration,
//! so a row for a symbol that is not in the linked image is not a silent
//! mistake — it is a LINK ERROR for every consumer of the table. Two classes of
//! declaration exist in the sources but not in the image, and both are skipped:
//!
//! - **`#[cfg(test)]`** — on a module OR on the item itself (a struct/impl/fn/
//!   const). Test-only classes exist to exercise the macro; they are never
//!   registered and never linked into a normal build.
//! - **Cargo-FEATURE gates** — `#[cfg(feature = "…")]`, whether on the item or
//!   inherited from the `mod x;` declaration that pulls the file in. A feature
//!   is per-crate: `rts-std`'s `asio` feature is not a feature of the crate that
//!   HOSTS the generated table, so the predicate could not even be replayed
//!   correctly, and the symbol is absent unless that feature is on. Optional
//!   surface stays out of the table.
//!
//! A NON-feature `#[cfg(...)]` (`unix`/`windows`/`target_arch`) is different: it
//! is evaluated identically in any crate, so it is CARRIED verbatim to the
//! generated entry and a platform-gated pair bakes as two mutually exclusive
//! rows rather than one wrong row.
//!
//! ## Module-inherited cfgs
//!
//! The scanner walks FILES, but a file's gate usually lives on the `mod name;`
//! declaration in its parent (`#[cfg(feature = "asio")] pub mod asio_audio;` in
//! `rts-std/src/lib.rs`) — nothing inside the file mentions it. So a pre-pass
//! ([`module_gates`]) records the cfgs of every external `mod` declaration and
//! maps them onto the file that declaration pulls in (`<dir>/<name>.rs` and
//! `<dir>/<name>/mod.rs`). Without it the scan cannot see the gate at all.

mod class;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use quote::ToTokens;
use rts_abi::{Naming, Scope, symbol_for};

/// One discovered symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The exact linker name.
    pub symbol: String,
    /// `#[cfg(...)]` predicates on the declaring function, verbatim, to be
    /// replayed on the generated entry.
    pub cfgs: Vec<String>,
    /// The ABI SIGNATURE, derived from the Rust one via [`rts_abi::tymap`] — the
    /// same mapping `#[rtse::abi]` uses, so the baked table and the
    /// macro-emitted `SymbolDesc` cannot disagree.
    ///
    /// `Some((params, ret))` only for a `#[rtse::abi]` fn whose every type has an
    /// ABI spelling. `None` for a hand-written `#[no_mangle]` fn (its types are
    /// not declared in ABI terms) or an `#[rtse::abi]` fn using a type the
    /// mapping does not cover — a missing signature is an ABSENT row, never a
    /// guessed one.
    pub sig: Option<(Vec<rts_abi::AbiType>, rts_abi::AbiType)>,
    /// Source file, workspace-relative, for the generated provenance comment
    /// and for duplicate diagnostics.
    pub origin: String,
}

/// Scan `crates` (names relative to `<root>/crates`) for declarations.
///
/// Files are visited in sorted path order and crates in list order, so the
/// result is deterministic before sorting even happens.
pub fn scan_workspace(root: &Path, crates: &[&str]) -> Result<Vec<Declaration>> {
    let mut out = Vec::new();
    for c in crates {
        let src = root.join("crates").join(c).join("src");
        if !src.is_dir() {
            continue;
        }
        // Pre-pass: a file's gate normally lives on the `mod name;` that pulls it
        // in, not inside the file. Collect those before visiting any file.
        let gates = module_gates(&src)?;
        for file in rust_files(&src)? {
            if gates.get(&file).is_some_and(|c| is_feature_gate(c) || mentions_test(c)) {
                continue;
            }
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("read {}", file.display()))?;
            let ast = syn::parse_file(&text)
                .with_context(|| format!("parse {}", file.display()))?;
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            visit_items(&ast.items, &rel, &mut out)?;
        }
    }
    Ok(out)
}

/// Map every FILE pulled in by a gated `mod name;` declaration to that
/// declaration's cfgs.
///
/// Rust puts the gate on the declaration, not in the declared file, so this is
/// the only way the scanner can see it. Both module file layouts are recorded —
/// `<dir>/<name>.rs` and `<dir>/<name>/mod.rs` — because the declaration does
/// not say which one is on disk.
///
/// Gates are NOT propagated transitively to a gated module's own children: a
/// child of a skipped file is never visited anyway (the parent's whole subtree
/// lives under `<dir>/<name>/`, and each of those files is reached only through
/// the gated `mod`, so skipping is handled by the same map when the child's own
/// declaration is read). Nested un-gated children of a gated module are the one
/// case this does not cover, and the link error stays loud if it ever arises.
fn module_gates(src: &Path) -> Result<std::collections::HashMap<PathBuf, Vec<String>>> {
    let mut map = std::collections::HashMap::new();
    for file in rust_files(src)? {
        let text = std::fs::read_to_string(&file)
            .with_context(|| format!("read {}", file.display()))?;
        let Ok(ast) = syn::parse_file(&text) else {
            continue;
        };
        // A `mod x;` in `foo/mod.rs` (or `lib.rs`) resolves relative to its dir.
        let dir = file.parent().unwrap_or(src).to_path_buf();
        for item in &ast.items {
            let syn::Item::Mod(m) = item else { continue };
            if m.content.is_some() {
                continue; // inline module: its items carry their own attrs
            }
            let cfgs = cfgs_of(&m.attrs);
            if cfgs.is_empty() {
                continue;
            }
            let name = m.ident.to_string();
            map.insert(dir.join(format!("{name}.rs")), cfgs.clone());
            map.insert(dir.join(&name).join("mod.rs"), cfgs);
        }
    }
    Ok(map)
}

/// Does this cfg set gate on a CARGO FEATURE? Such a symbol stays out of the
/// table — see the module doc.
fn is_feature_gate(cfgs: &[String]) -> bool {
    cfgs.iter().any(|c| c.contains("feature"))
}

/// Does this cfg set mention `test`?
fn mentions_test(cfgs: &[String]) -> bool {
    cfgs.iter().any(|c| c.contains("test"))
}

/// Every `.rs` under `dir`, sorted — the determinism floor.
fn rust_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&d)
            .with_context(|| format!("read dir {}", d.display()))?
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|e| e == "rs") {
                files.push(p);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn visit_items(items: &[syn::Item], origin: &str, out: &mut Vec<Declaration>) -> Result<()> {
    for item in items {
        // A `#[cfg(test)]` or `#[cfg(feature = "…")]` on the ITEM excludes it for
        // the same reason a gated module is excluded: the symbol is not in the
        // linked image, so a row for it is a link error, not a stale row. This
        // check is on EVERY item kind — a `#[cfg(test)] #[rtse::class]` struct+impl
        // pair (the `__RtseSymbolKeyDemo` macro test) is exactly the case that
        // slipped through when only `Item::Mod` was checked.
        if let Some(attrs) = item_attrs(item) {
            let cfgs = cfgs_of(attrs);
            if mentions_test(&cfgs) || is_feature_gate(&cfgs) {
                continue;
            }
        }
        match item {
            syn::Item::Mod(m) => {
                if let Some((_, inner)) = &m.content {
                    visit_items(inner, origin, out)?;
                }
            }
            syn::Item::Fn(f) => {
                if let Some(symbol) = symbol_of(f)? {
                    out.push(Declaration {
                        symbol,
                        cfgs: cfgs_of(&f.attrs),
                        sig: sig_of(f),
                        origin: origin.to_string(),
                    });
                }
            }
            syn::Item::Impl(imp) => {
                out.extend(class::declarations_from_impl(imp, origin)?);
            }
            syn::Item::Struct(s) => {
                out.extend(class::declarations_from_struct(s, origin)?);
            }
            syn::Item::Const(c) => {
                if let Some(d) = class::declaration_from_const(c, origin)? {
                    out.push(d);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// The attribute list of any item kind this scanner looks at.
fn item_attrs(item: &syn::Item) -> Option<&[syn::Attribute]> {
    Some(match item {
        syn::Item::Mod(m) => &m.attrs,
        syn::Item::Fn(f) => &f.attrs,
        syn::Item::Impl(i) => &i.attrs,
        syn::Item::Struct(s) => &s.attrs,
        syn::Item::Const(c) => &c.attrs,
        _ => return None,
    })
}

/// The linker symbol this function exports, if it exports one.
fn symbol_of(f: &syn::ItemFn) -> Result<Option<String>> {
    let name = f.sig.ident.to_string();
    // `#[rtse::abi]` and `#[rtse::function]` derive their symbol through the SAME
    // `rts_abi::scope` rule — `function.rs` reuses `abi::scope`'s `AbiArgs`
    // deliberately, "so a free function and a raw ABI symbol in the same module
    // cannot drift onto different naming". One branch, therefore, not two.
    if let Some(attr) = f.attrs.iter().find(|a| is_rtse_symbolic(a)) {
        let naming = parse_naming(attr)
            .with_context(|| format!("{} on `{name}`", attr_label(attr)))?;
        return Ok(Some(symbol_for(&naming, &name)));
    }
    if has_no_mangle(&f.attrs) && name.starts_with("__") {
        // `no_mangle`: the linker name is the identifier, unchanged.
        return Ok(Some(name));
    }
    Ok(None)
}

/// The written token of a type — the last path segment's ident (`&str` → `str`,
/// `rts_abi::Handle` → `Handle`). [`rts_abi::tymap`] maps that token; see its
/// docs for why the mapping is on the WRITTEN token and not a resolved type.
fn type_token(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(p) => Some(p.path.segments.last()?.ident.to_string()),
        syn::Type::Reference(r) => type_token(&r.elem),
        _ => None,
    }
}

/// The ABI signature of a symbol-declaring fn (`#[rtse::abi]` or
/// `#[rtse::function]`), or `None` if it is neither — or uses a type with no ABI
/// spelling, in which case the row carries NO signature rather than a wrong one.
///
/// A `#[default(...)]` on a parameter changes only whether an OMITTED argument is
/// legal and what it is padded with (`Sig::default_args`); the slot's type — and
/// therefore this signature — is unaffected, so the attribute is ignored here.
fn sig_of(f: &syn::ItemFn) -> Option<(Vec<rts_abi::AbiType>, rts_abi::AbiType)> {
    if !f.attrs.iter().any(is_rtse_symbolic) {
        return None;
    }
    let mut params = Vec::new();
    for arg in &f.sig.inputs {
        let syn::FnArg::Typed(pt) = arg else {
            return None; // a receiver: not a free ABI fn
        };
        let token = type_token(&pt.ty)?;
        if rts_abi::tymap::is_str_token(&token) {
            params.push(rts_abi::AbiType::StrPtr);
            continue;
        }
        params.push(rts_abi::tymap::single_slot(&token)?);
    }
    let ret = match &f.sig.output {
        syn::ReturnType::Default => rts_abi::AbiType::Void,
        syn::ReturnType::Type(_, ty) => {
            let token = type_token(ty)?;
            if rts_abi::tymap::is_str_token(&token) {
                // A `String` RETURN is allocated into the pool and handed back as a
                // handle, not as a (ptr, len) pair — `StrPtr` is a PARAM-only shape.
                rts_abi::AbiType::Handle
            } else {
                rts_abi::tymap::single_slot(&token)?
            }
        }
    };
    Some((params, ret))
}

fn is_rtse_abi(a: &syn::Attribute) -> bool {
    rtse_attr_kind(a) == Some("abi")
}

/// Does this attribute DECLARE a symbol whose name comes from `rts_abi::scope`?
///
/// Both `#[rtse::abi]` (a raw ABI symbol) and `#[rtse::function]` (a free
/// module/global function) do. Missing the second was a real hole: a converted
/// `node:*` module's symbols vanished from the baked table, still working only
/// because the Registry harvest installs any member carrying a `fn_ptr` — the
/// table silently stopped being the single source for them.
fn is_rtse_symbolic(a: &syn::Attribute) -> bool {
    matches!(rtse_attr_kind(a), Some("abi") | Some("function"))
}

/// `#[rtse::<kind>]` → `Some("<kind>")`, for any other attribute `None`.
fn rtse_attr_kind(a: &syn::Attribute) -> Option<&'static str> {
    let segs: Vec<String> = a
        .path()
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    let [x, y] = segs.as_slice() else { return None };
    if x != "rtse" {
        return None;
    }
    match y.as_str() {
        "abi" => Some("abi"),
        "function" => Some("function"),
        _ => None,
    }
}

/// The attribute's spelling, for an error message that names what the author wrote.
fn attr_label(a: &syn::Attribute) -> &'static str {
    match rtse_attr_kind(a) {
        Some("function") => "#[rtse::function]",
        _ => "#[rtse::abi]",
    }
}

/// `#[no_mangle]` in either spelling — bare, or wrapped as `#[unsafe(no_mangle)]`
/// (the edition-2024 form the codebase uses).
fn has_no_mangle(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("no_mangle")
            || (a.path().is_ident("unsafe")
                && a.to_token_stream().to_string().contains("no_mangle"))
    })
}

/// `#[cfg(...)]` attributes, rendered back to source text for replay.
fn cfgs_of(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("cfg"))
        .map(|a| a.to_token_stream().to_string())
        .collect()
}

/// Parse `#[rtse::abi(...)]` arguments into the shared [`Naming`].
///
/// The grammar mirrors `rts-macro`'s `abi::scope::AbiArgs`; only the SEMANTICS
/// are shared (the macro owns the `syn::parse::Parse` impl because a foreign
/// trait needs a local type). Any divergence here is a parse error, not a wrong
/// name — the derivation itself is called, never copied.
fn parse_naming(attr: &syn::Attribute) -> Result<Naming> {
    if matches!(attr.meta, syn::Meta::Path(_)) {
        return Ok(Naming::Verbatim);
    }
    let parsed = attr.parse_args_with(|input: syn::parse::ParseStream| {
        if input.peek(syn::LitStr) {
            let s: syn::LitStr = input.parse()?;
            return Ok(Naming::Explicit(s.value()));
        }
        let mut scope: Option<Scope> = None;
        let mut value: Option<String> = None;
        let mut overload: Option<String> = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            let arg = if input.peek(syn::Token![=]) {
                input.parse::<syn::Token![=]>()?;
                let v: syn::LitStr = input.parse()?;
                Some(v.value())
            } else {
                None
            };
            match (key.to_string().as_str(), arg) {
                ("module", Some(m)) => scope = Some(Scope::Module(m)),
                ("global", g) => scope = Some(Scope::Global(g)),
                ("native", None) => scope = Some(Scope::Native),
                ("value", Some(v)) => value = Some(v),
                ("overload", Some(o)) => overload = Some(o),
                // Args that do not affect the SYMBOL — they set fields on the
                // registered member (`MemberFlags::THROWS`, `pure`, the TS return
                // type), which this table does not carry. Accepted and ignored,
                // so a declaration using them still bakes instead of failing the
                // scan. (`ret_ts` DOES change the emitted `Member`, but not the
                // linker name or the ABI slots, which is all this table records.)
                ("throws", None) | ("pure", None) | ("constant", None) | ("ret_ts", Some(_)) => {}
                _ => {
                    return Err(syn::Error::new_spanned(&key, "unsupported #[rtse::abi] arg"));
                }
            }
            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            } else {
                break;
            }
        }
        match scope {
            Some(scope) => Ok(Naming::Scoped {
                scope,
                // Same composition the macro applies — `rts_abi::scope` owns it,
                // so the baked symbol and the emitted one cannot disagree.
                value: value.map(|v| rts_abi::scope::with_overload(v, overload.as_deref())),
            }),
            None => Err(input.error("#[rtse::abi]: missing scope")),
        }
    })?;
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn syms(src: &str) -> Vec<Declaration> {
        let ast = syn::parse_file(src).unwrap();
        let mut out = Vec::new();
        visit_items(&ast.items, "x.rs", &mut out).unwrap();
        out
    }

    #[test]
    fn picks_up_no_mangle_verbatim() {
        let d = syms(r#"#[unsafe(no_mangle)] pub extern "C" fn __rtsadp_add(a: u64) -> u64 { a }"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].symbol, "__rtsadp_add");
    }

    #[test]
    fn derives_from_rtse_abi_scope() {
        let d = syms(r#"#[rtse::abi(module = "node:fs", value = "readFileSync")] pub fn r() {}"#);
        assert_eq!(d[0].symbol, "__rtsm_node_fs_readFileSync");
    }

    #[test]
    fn bare_rtse_abi_is_verbatim() {
        let d = syms(r#"#[rtse::abi] pub fn rtsadp_obj_get() {}"#);
        assert_eq!(d[0].symbol, "__rtsadp_obj_get");
    }

    /// A `#[rtse::abi]` fn's ABI signature is DERIVED from its Rust one through
    /// `rts_abi::tymap` — the same mapping the macro uses, so the baked row and
    /// the macro-emitted `SymbolDesc` cannot disagree. Note `&str` is ONE
    /// `StrPtr` param (two slots), and the declared alias wins over the shared
    /// Rust repr (`Handle` stays `Handle`, not `U64`).
    #[test]
    fn derives_the_abi_signature_from_the_rust_one() {
        let d = syms(r#"#[rtse::abi] pub fn rtsadp_x(a: Handle, b: &str, c: f64) -> u64 { 0 }"#);
        assert_eq!(
            d[0].sig,
            Some((
                vec![
                    rts_abi::AbiType::Handle,
                    rts_abi::AbiType::StrPtr,
                    rts_abi::AbiType::F64
                ],
                rts_abi::AbiType::U64
            ))
        );
    }

    /// No return type is `Void`, not a guess.
    #[test]
    fn no_return_type_is_void() {
        let d = syms(r#"#[rtse::abi] pub fn rtsadp_y(w: u64) {}"#);
        assert_eq!(
            d[0].sig,
            Some((vec![rts_abi::AbiType::U64], rts_abi::AbiType::Void))
        );
    }

    /// A hand-written `#[no_mangle]` fn contributes a SYMBOL but no signature:
    /// its types are not declared in ABI terms, and an absent row is honest
    /// where a guessed one would be the drift this whole design deletes.
    #[test]
    fn no_mangle_fn_carries_no_signature() {
        let d = syms(r#"#[unsafe(no_mangle)] pub extern "C" fn __rtsadp_z(a: u64) -> u64 { a }"#);
        assert_eq!(d[0].symbol, "__rtsadp_z");
        assert_eq!(d[0].sig, None);
    }

    /// A type with no ABI spelling yields NO signature rather than a wrong one.
    #[test]
    fn unmappable_type_yields_no_signature() {
        let d = syms(r#"#[rtse::abi] pub fn rtsadp_w(p: *const u8) {}"#);
        assert_eq!(d[0].sig, None);
    }

    #[test]
    fn carries_cfg_and_skips_test_modules() {
        let d = syms(
            r#"
            #[cfg(unix)] #[unsafe(no_mangle)] pub extern "C" fn __rtsn_x() {}
            #[cfg(test)] mod t { #[unsafe(no_mangle)] pub extern "C" fn __rtsn_y() {} }
            "#,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].symbol, "__rtsn_x");
        assert_eq!(d[0].cfgs.len(), 1);
    }

    #[test]
    fn ignores_ordinary_functions() {
        assert!(syms("pub fn helper() {}").is_empty());
    }
}
