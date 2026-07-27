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
//! Everything under a `#[cfg(test)]` module is skipped — test symbols are not in
//! the shipped image. A `#[cfg(...)]` on the function itself is CARRIED to the
//! generated entry, so a platform-gated pair (`posix`/`win32`) bakes as two
//! mutually exclusive rows rather than one wrong row.

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
        for file in rust_files(&src)? {
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
        match item {
            syn::Item::Mod(m) => {
                if is_test_gated(&m.attrs) {
                    continue;
                }
                if let Some((_, inner)) = &m.content {
                    visit_items(inner, origin, out)?;
                }
            }
            syn::Item::Fn(f) => {
                if let Some(symbol) = symbol_of(f)? {
                    out.push(Declaration {
                        symbol,
                        cfgs: cfgs_of(&f.attrs),
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

/// The linker symbol this function exports, if it exports one.
fn symbol_of(f: &syn::ItemFn) -> Result<Option<String>> {
    let name = f.sig.ident.to_string();
    if let Some(attr) = f.attrs.iter().find(|a| is_rtse_abi(a)) {
        let naming = parse_naming(attr)
            .with_context(|| format!("#[rtse::abi] on `{name}`"))?;
        return Ok(Some(symbol_for(&naming, &name)));
    }
    if has_no_mangle(&f.attrs) && name.starts_with("__") {
        // `no_mangle`: the linker name is the identifier, unchanged.
        return Ok(Some(name));
    }
    Ok(None)
}

fn is_rtse_abi(a: &syn::Attribute) -> bool {
    let segs: Vec<String> = a
        .path()
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    matches!(segs.as_slice(), [x, y] if x == "rtse" && y == "abi")
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

fn is_test_gated(attrs: &[syn::Attribute]) -> bool {
    cfgs_of(attrs).iter().any(|c| c.contains("test"))
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
                ("abi", None) => scope = Some(Scope::Abi),
                ("value", Some(v)) => value = Some(v),
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
            Some(scope) => Ok(Naming::Scoped { scope, value }),
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
