//! `rts-macro` — single-declaration namespace/class derivation for RTS.
//!
//! Replaces the quadruple-bookkeeping (abi.rs `MEMBERS` table, ops.rs extern
//! fn, the `SPECS` aggregation array, and the jit `add_fn!` list) with one
//! annotated `impl` block. From each `#[rts_fn]` the macro derives:
//!
//! - the `#[no_mangle] extern "C"` symbol (body kept verbatim),
//! - a `rts_abi::NamespaceMember` entry (args/returns from the type tokens,
//!   doc from the `///` comment, `ts_signature` derived or overridden),
//! - a `pub const SPEC: NamespaceSpec` aggregating all members.
//!
//! Stage 1 (this file): derivation + extern emission, using the existing
//! `__RTS_FN_NS_<NS>_<NAME>` symbol convention so it is a drop-in for the
//! hand-written tables. Opaque symbol hashing and the `linkme` registry land
//! in later stages (see `docs/specs/rts-core-engine.md`).
//!
//! See the spec for the full design (classes, prototype/object model, dynamic
//! tier). This stage covers plain namespace functions with scalar ABI types.

use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, Ident, ImplItem, ItemImpl, Pat, ReturnType, Type};

/// Maps the last path segment of a Rust type to `(AbiType variant, TS type)`.
/// Returns `None` for unrecognised tokens.
fn type_token(ty: &Type) -> Option<(&'static str, &'static str)> {
    let seg = match ty {
        Type::Path(p) => p.path.segments.last()?.ident.to_string(),
        _ => return None,
    };
    Some(match seg.as_str() {
        "Handle" => ("Handle", "number"),
        "U64" => ("U64", "number"),
        "I64" | "i64" => ("I64", "number"),
        "I32" | "i32" => ("I32", "number"),
        "F64" | "f64" => ("F64", "number"),
        "Bool" | "bool" => ("Bool", "boolean"),
        "Str" => ("StrPtr", "string"),
        _ => return None,
    })
}

/// Collects the text of `#[doc = "..."]` attributes, trimmed and newline-joined.
fn doc_of(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &a.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                lines.push(s.value().trim().to_string());
            }
        }
    }
    lines.join("\n")
}

/// Parses `#[rts_fn(...)]` options: `ts = "..."` (signature override), `pure`.
struct FnOpts {
    ts: Option<String>,
    pure: bool,
}

fn parse_rts_fn(attrs: &[syn::Attribute]) -> Option<FnOpts> {
    let attr = attrs.iter().find(|a| a.path().is_ident("rts_fn"))?;
    let mut opts = FnOpts {
        ts: None,
        pure: false,
    };
    // `#[rts_fn]` (no args) → Path meta; `#[rts_fn(...)]` → List.
    if matches!(attr.meta, syn::Meta::Path(_)) {
        return Some(opts);
    }
    let _ = attr.parse_nested_meta(|m| {
        if m.path.is_ident("pure") {
            opts.pure = true;
        } else if m.path.is_ident("ts") {
            let v = m.value()?;
            let s: syn::LitStr = v.parse()?;
            opts.ts = Some(s.value());
        }
        Ok(())
    });
    Some(opts)
}

fn err(span: proc_macro2::Span, msg: &str) -> TokenStream {
    syn::Error::new(span, msg).to_compile_error().into()
}

/// `#[rts_namespace(<name>)]` on an `impl` block. See module docs.
#[proc_macro_attribute]
pub fn rts_namespace(attr: TokenStream, item: TokenStream) -> TokenStream {
    let ns = syn::parse_macro_input!(attr as Ident);
    let imp = syn::parse_macro_input!(item as ItemImpl);

    let ns_str = ns.to_string();
    let ns_upper = ns_str.to_uppercase();
    let spec_doc = doc_of(&imp.attrs);

    let mut externs = Vec::new();
    let mut members = Vec::new();

    for it in &imp.items {
        let ImplItem::Fn(f) = it else { continue };
        let Some(opts) = parse_rts_fn(&f.attrs) else {
            continue; // not an #[rts_fn] — skip (helpers allowed in the impl)
        };

        let name = f.sig.ident.to_string();
        let name_upper = name.to_uppercase();
        let symbol = format!("__RTS_FN_NS_{ns_upper}_{name_upper}");
        let sym_ident = Ident::new(&symbol, f.sig.ident.span());
        let doc = doc_of(&f.attrs);

        // Derive arg AbiTypes + TS param list.
        let mut arg_variants = Vec::new();
        let mut ts_params = Vec::new();
        for input in &f.sig.inputs {
            let FnArg::Typed(pt) = input else {
                return err(
                    f.sig.ident.span(),
                    "rts_fn does not take `self` — use #[rts_class] #[method] for instance methods",
                );
            };
            let Some((abi, tsty)) = type_token(&pt.ty) else {
                return err(f.sig.ident.span(), "unsupported parameter type — use a token from rts_abi::ty (Handle/U64/I64/I32/F64/Bool/Str)");
            };
            if abi == "StrPtr" {
                return err(f.sig.ident.span(), "Str parameters are not yet supported by the stage-1 macro (ptr+len expansion pending)");
            }
            let pname = match &*pt.pat {
                Pat::Ident(pi) => pi.ident.to_string(),
                _ => "arg".to_string(),
            };
            ts_params.push(format!("{pname}: {tsty}"));
            let v = Ident::new(abi, proc_macro2::Span::call_site());
            arg_variants.push(quote! { ::rts_abi::AbiType::#v });
        }

        // Derive return AbiType + TS return.
        let (ret_variant, ret_ts) = match &f.sig.output {
            ReturnType::Default => ("Void", "void"),
            ReturnType::Type(_, ty) => match type_token(ty) {
                Some((abi, tsty)) => (abi, tsty),
                None => {
                    return err(
                        f.sig.ident.span(),
                        "unsupported return type — use a token from rts_abi::ty",
                    )
                }
            },
        };
        let ret_ident = Ident::new(ret_variant, proc_macro2::Span::call_site());

        let ts_sig = opts
            .ts
            .unwrap_or_else(|| format!("{name}({}): {ret_ts}", ts_params.join(", ")));
        let pure = opts.pure;

        // Emit the extern "C" symbol — body verbatim.
        let inputs = &f.sig.inputs;
        let output = &f.sig.output;
        let block = &f.block;
        externs.push(quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #sym_ident(#inputs) #output #block
        });

        members.push(quote! {
            ::rts_abi::NamespaceMember {
                name: #name,
                kind: ::rts_abi::MemberKind::Function,
                symbol: #symbol,
                args: &[ #(#arg_variants),* ],
                returns: ::rts_abi::AbiType::#ret_ident,
                doc: #doc,
                ts_signature: #ts_sig,
                intrinsic: None,
                pure: #pure,
            }
        });
    }

    let out = quote! {
        #(#externs)*

        /// Derived namespace members (`#[rts_namespace]`). Source of truth.
        pub const MEMBERS: &[::rts_abi::NamespaceMember] = &[ #(#members),* ];

        /// Derived namespace spec — replaces the hand-written `SPEC` const.
        pub const SPEC: ::rts_abi::NamespaceSpec = ::rts_abi::NamespaceSpec {
            name: #ns_str,
            doc: #spec_doc,
            members: MEMBERS,
        };
    };
    out.into()
}
