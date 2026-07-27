//! Discovery of symbols that exist only after `#[rtse::class]`/`#[rtse::constant]`
//! macro expansion: class members, `#[rtse::variable]` field accessors, and
//! `#[rtse::constant]` getters. These idents are not present in the source text
//! the way `#[rtse::abi]`/`no_mangle` functions are — they are synthesized by
//! `rts-macro` — so this module re-applies the SAME classification rules
//! `crates/rts-macro/src/class/` uses, and derives the symbol through the same
//! single authority ([`rts_abi::scope::symbol_for`]), never a second copy of the
//! naming rule itself.
//!
//! ## Which members produce a symbol
//!
//! Every `impl` fn carrying one of the seven marker attributes
//! (`#[rtse::ctor]`, `#[rtse::method]`, `#[rtse::asynch]`, `#[rtse::private]`,
//! `#[rtse::statical]`, `#[rtse::getter]`, `#[rtse::setter]`) produces EXACTLY
//! ONE `#[unsafe(no_mangle)] extern "C"` wrapper, unconditionally — see
//! `rts-macro/src/class/member/mod.rs::gen_member`: the `extern_fn` is built
//! once per call, and none of `private`/`readonly`/`throws`/`optional`/
//! `is_async` gates that emission (they only change the `Member` metadata and,
//! for `private`, suppress the `ts_signature`). So recognizing the marker is
//! sufficient; no further attribute inspection is needed to know a symbol
//! exists.
//!
//! `#[rtse::instanceof]` and `#[rtse::symbol]` are currently INERT passthrough
//! markers in `rts-macro` (`lib.rs` returns the item unchanged; `class/kind.rs`
//! does not recognize them in `take_kind`, so a fn carrying only one of these
//! is left as a plain Rust fn with no `Kind` and `gen_impl` skips it — no
//! extern, no `Member`). They are therefore deliberately NOT treated as
//! symbol-producing here. If `rts-macro` ever implements them, this module must
//! be updated in the same change.
//!
//! A struct field is symbol-producing iff it carries `#[rtse::variable]`
//! (`class/field.rs::gen_field`): always a getter (`<field>__get`), plus a
//! setter (`<field>__set`) unless `#[rtse::variable(readonly)]`.
//!
//! ## What must NOT be picked up
//!
//! `#[rtse::class]` also emits two Rust items that are NOT linker symbols:
//! `__RTSE_KEEP_<CLASS>[_FIELDS]` (a `#[used]` static, not an extern fn) and
//! `__rtse_fields_<Class>` (an ordinary `pub fn`, not `#[no_mangle] extern
//! "C"`). Neither is reached by the ordinary `syn::Item::Fn` scan in
//! `scan/mod.rs` (one is a `static`, the other lacks `no_mangle`/`rtse::abi`),
//! so no explicit exclusion is needed here — but it is the reason this module
//! walks `ImplItem::Fn` / struct fields directly instead of re-running the
//! generic function scan over the (post-expansion-shaped) source.

use anyhow::{Context, Result};
use rts_abi::scope::{Naming, Scope, symbol_for};

use super::Declaration;

/// `__rtsm_global_<class>_<value>` — the same derivation `rts-macro`'s
/// `class::class_symbol` calls (`rts_abi::scope::symbol_for` with a
/// `Global(Some(class))` scope). `value` is used VERBATIM.
fn class_symbol(class: &str, value: &str) -> String {
    symbol_for(
        &Naming::Scoped {
            scope: Scope::Global(Some(class.to_string())),
            value: Some(value.to_string()),
        },
        value,
    )
}

/// Marker attributes that, on an `impl` fn inside `#[rtse::class]`, each
/// produce exactly one linker symbol. Mirrors `class::kind::take_kind`'s
/// recognized set.
const MEMBER_MARKERS: &[&str] = &[
    "ctor", "method", "asynch", "private", "statical", "getter", "setter",
];

fn member_marker(a: &syn::Attribute) -> bool {
    let segs = &a.path().segments;
    segs.len() == 2 && segs[0].ident == "rtse" && MEMBER_MARKERS.contains(&segs[1].ident.to_string().as_str())
}

fn is_rtse_class(a: &syn::Attribute) -> bool {
    let segs = &a.path().segments;
    segs.len() == 2 && segs[0].ident == "rtse" && segs[1].ident == "class"
}

fn is_rtse_constant(a: &syn::Attribute) -> bool {
    let segs = &a.path().segments;
    segs.len() == 2 && segs[0].ident == "rtse" && segs[1].ident == "constant"
}

fn is_rtse_variable(a: &syn::Attribute) -> Option<bool> {
    let segs = &a.path().segments;
    if !(segs.len() == 2 && segs[0].ident == "rtse" && segs[1].ident == "variable") {
        return None;
    }
    let mut readonly = false;
    let _ = a.parse_nested_meta(|m| {
        if m.path.is_ident("readonly") {
            readonly = true;
        }
        Ok(())
    });
    Some(readonly)
}

/// The class name from `#[rtse::class("Name", ...)]` — only the leading string
/// literal is needed; `extends`/`value` do not affect symbol derivation.
fn class_name_of(attrs: &[syn::Attribute]) -> Result<Option<String>> {
    let Some(attr) = attrs.iter().find(|a| is_rtse_class(a)) else {
        return Ok(None);
    };
    let name = attr
        .parse_args_with(|input: syn::parse::ParseStream| {
            let s: syn::LitStr = input.parse()?;
            // Ignore the rest (`, extends = "..."` / `, value`); only the class
            // name feeds symbol derivation. Mirrors `ClassArgs::parse` in
            // `rts-macro/src/class/mod.rs` minus building the struct.
            while input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
                if input.is_empty() {
                    break;
                }
                let _key: syn::Ident = input.parse()?;
                if input.peek(syn::Token![=]) {
                    input.parse::<syn::Token![=]>()?;
                    let _v: syn::LitStr = input.parse()?;
                }
            }
            Ok(s.value())
        })
        .with_context(|| "#[rtse::class(...)]: expected a string literal class name")?;
    Ok(Some(name))
}

/// `#[rtse::class("Name")] impl Type { ... }` — one [`Declaration`] per member
/// marker found on an `impl` fn.
pub(super) fn declarations_from_impl(imp: &syn::ItemImpl, origin: &str) -> Result<Vec<Declaration>> {
    let Some(class) = class_name_of(&imp.attrs)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for it in &imp.items {
        let syn::ImplItem::Fn(f) = it else { continue };
        if !f.attrs.iter().any(member_marker) {
            continue;
        }
        out.push(Declaration {
            symbol: class_symbol(&class, &f.sig.ident.to_string()),
            cfgs: super::cfgs_of(&f.attrs),
            origin: origin.to_string(),
        });
    }
    Ok(out)
}

/// `#[rtse::class("Name")] struct Type { ... }` — a getter (+ setter unless
/// `readonly`) per `#[rtse::variable]` field.
pub(super) fn declarations_from_struct(s: &syn::ItemStruct, origin: &str) -> Result<Vec<Declaration>> {
    let Some(class) = class_name_of(&s.attrs)? else {
        return Ok(Vec::new());
    };
    let syn::Fields::Named(named) = &s.fields else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for f in &named.named {
        let Some(readonly) = f.attrs.iter().find_map(is_rtse_variable) else {
            continue;
        };
        let Some(fname) = &f.ident else { continue };
        let fname = fname.to_string();
        out.push(Declaration {
            symbol: class_symbol(&class, &format!("{fname}__get")),
            cfgs: super::cfgs_of(&f.attrs),
            origin: origin.to_string(),
        });
        if !readonly {
            out.push(Declaration {
                symbol: class_symbol(&class, &format!("{fname}__set")),
                cfgs: super::cfgs_of(&f.attrs),
                origin: origin.to_string(),
            });
        }
    }
    Ok(out)
}

/// The JS name `#[rtse::constant]` derives from a Rust const ident when no
/// `value = "…"` is given — DUPLICATED from `rts-macro/src/constant.rs::js_name_of`
/// because that crate is a proc-macro and cannot be linked as an ordinary
/// library. This is a JS-spelling heuristic feeding the shared `symbol_for`,
/// not a second copy of the naming rule itself; if it drifts from the macro's
/// copy, `--check` will not catch it (a real seam — flagged, not silently
/// papered over).
fn js_name_of(ident: &str) -> String {
    let screaming = ident
        .chars()
        .all(|c| !c.is_ascii_alphabetic() || c.is_ascii_uppercase());
    if screaming {
        to_camel(&ident.to_ascii_lowercase())
    } else {
        to_camel(ident)
    }
}

/// `snake_case` / `SCREAMING_SNAKE` → `camelCase`. Mirrors `rts-macro::naming::to_camel`.
fn to_camel(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper_next = false;
    for c in s.chars() {
        if c == '_' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `#[rtse::constant(...)]` on a `const` — one getter symbol.
pub(super) fn declaration_from_const(item: &syn::ItemConst, origin: &str) -> Result<Option<Declaration>> {
    let Some(attr) = item.attrs.iter().find(|a| is_rtse_constant(a)) else {
        return Ok(None);
    };
    let naming = super::parse_naming(attr)
        .with_context(|| format!("#[rtse::constant] on `{}`", item.ident))?;
    let js_name = match &naming {
        Naming::Scoped {
            value: Some(v), ..
        } => v.clone(),
        _ => js_name_of(&item.ident.to_string()),
    };
    let symbol = symbol_for(&naming, &js_name);
    Ok(Some(Declaration {
        symbol,
        cfgs: super::cfgs_of(&item.attrs),
        origin: origin.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decls_impl(src: &str) -> Vec<Declaration> {
        let ast = syn::parse_file(src).unwrap();
        let mut out = Vec::new();
        super::super::visit_items(&ast.items, "x.rs", &mut out).unwrap();
        out
    }

    #[test]
    fn class_method_symbol() {
        let d = decls_impl(
            r#"
            #[rtse::class("Point")]
            impl Point {
                #[rtse::method] fn sum(&self) -> f64 { 0.0 }
            }
            "#,
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].symbol, "__rtsm_global_point_sum");
    }

    #[test]
    fn class_ctor_and_static_symbols() {
        let d = decls_impl(
            r#"
            #[rtse::class("Point")]
            impl Point {
                #[rtse::ctor] fn new() -> Self { Point {} }
                #[rtse::statical] fn origin() -> Self { Point {} }
            }
            "#,
        );
        let syms: Vec<_> = d.iter().map(|x| x.symbol.as_str()).collect();
        assert!(syms.contains(&"__rtsm_global_point_new"));
        assert!(syms.contains(&"__rtsm_global_point_origin"));
    }

    #[test]
    fn field_getter_and_setter_symbols() {
        let ast = syn::parse_file(
            r#"
            #[rtse::class("Url")]
            struct Url {
                #[rtse::variable] href: f64,
                #[rtse::variable(readonly)] length: i64,
            }
            "#,
        )
        .unwrap();
        let mut out = Vec::new();
        super::super::visit_items(&ast.items, "x.rs", &mut out).unwrap();
        let syms: Vec<_> = out.iter().map(|x| x.symbol.as_str()).collect();
        assert!(syms.contains(&"__rtsm_global_url_href__get"));
        assert!(syms.contains(&"__rtsm_global_url_href__set"));
        assert!(syms.contains(&"__rtsm_global_url_length__get"));
        assert!(!syms.contains(&"__rtsm_global_url_length__set"));
    }

    #[test]
    fn inert_markers_produce_no_symbol() {
        let d = decls_impl(
            r#"
            #[rtse::class("Point")]
            impl Point {
                #[rtse::instanceof] fn check(v: u64) -> bool { true }
                #[rtse::symbol] fn well_known() -> u64 { 0 }
            }
            "#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn constant_symbol_bare_and_scoped() {
        let d1 = decls_impl(r#"#[rtse::constant(module = "math")] pub const PI: f64 = 3.0;"#);
        assert_eq!(d1[0].symbol, "__rtsm_math_pi");

        let d2 = decls_impl(
            r#"#[rtse::constant(global = "Number", value = "MAX_SAFE_INTEGER")]
               pub const MAX_SAFE: f64 = 1.0;"#,
        );
        assert_eq!(d2[0].symbol, "__rtsm_global_number_MAX_SAFE_INTEGER");
    }
}
