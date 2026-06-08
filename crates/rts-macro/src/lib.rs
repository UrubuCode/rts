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

/// Parsed `#[rts_fn(...)]` / `#[rts_const(...)]` options.
struct FnOpts {
    ts: Option<String>,
    pure: bool,
    /// `true` when declared with `#[rts_const]` — a zero-arg member exposed as
    /// a `MemberKind::Constant` (accessed without parens, no TS arg list).
    is_const: bool,
    /// Custom early-return expression for the `Str` reconstruction guard when a
    /// string arg is null / invalid UTF-8. Overrides the typed-zero default —
    /// e.g. `on_null = i64::MIN`, `on_null = f64::NAN`, `on_null = -1`.
    on_null: Option<proc_macro2::TokenStream>,
}

fn parse_member(attrs: &[syn::Attribute]) -> Option<FnOpts> {
    let (attr, is_const) = attrs
        .iter()
        .find(|a| a.path().is_ident("rts_fn"))
        .map(|a| (a, false))
        .or_else(|| {
            attrs
                .iter()
                .find(|a| a.path().is_ident("rts_const"))
                .map(|a| (a, true))
        })?;
    let mut opts = FnOpts {
        ts: None,
        pure: false,
        is_const,
        on_null: None,
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
        } else if m.path.is_ident("on_null") {
            let v = m.value()?;
            let e: syn::Expr = v.parse()?;
            opts.on_null = Some(quote! { #e });
        }
        Ok(())
    });
    Some(opts)
}

fn err(span: proc_macro2::Span, msg: &str) -> TokenStream {
    syn::Error::new(span, msg).to_compile_error().into()
}

/// Early-return expression used when a `Str` arg fails to reconstruct (null /
/// invalid UTF-8). Mirrors the hand-written `return 0` convention; the zero is
/// typed to the function's return.
fn default_return(ret_variant: &str) -> proc_macro2::TokenStream {
    match ret_variant {
        "Void" => quote! { return },
        "F64" => quote! { return 0.0 },
        // Handle / U64 / I64 / I32 / Bool all zero-init to 0.
        _ => quote! { return 0 },
    }
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
        let Some(opts) = parse_member(&f.attrs) else {
            continue; // not an #[rts_fn]/#[rts_const] — skip (helpers allowed)
        };

        let span = f.sig.ident.span();
        let name = f.sig.ident.to_string();
        let name_upper = name.to_uppercase();
        let symbol = format!("__RTS_FN_NS_{ns_upper}_{name_upper}");
        let sym_ident = Ident::new(&symbol, span);
        let doc = doc_of(&f.attrs);

        // Return type first — its zero default seeds the Str-guard early return.
        let (ret_variant, ret_ts) = match &f.sig.output {
            ReturnType::Default => ("Void", "void"),
            ReturnType::Type(_, ty) => match type_token(ty) {
                Some(("StrPtr", _)) => {
                    return err(
                        span,
                        "Str is not a valid return type — return Handle (a GC string handle)",
                    );
                }
                Some((abi, tsty)) => (abi, tsty),
                None => {
                    return err(
                        span,
                        "unsupported return type — use a token from rts_abi::ty",
                    );
                }
            },
        };
        let ret_ident = Ident::new(ret_variant, span);
        let default_ret = match &opts.on_null {
            Some(expr) => quote! { return #expr },
            None => default_return(ret_variant),
        };

        // Derive args: AbiType + TS param + extern slots (Str → ptr+len) + the
        // `&str` reconstruction prelude injected before the user body.
        let mut arg_variants = Vec::new();
        let mut ts_params = Vec::new();
        let mut extern_inputs = Vec::new();
        let mut str_prelude = Vec::new();
        for input in &f.sig.inputs {
            let FnArg::Typed(pt) = input else {
                return err(
                    span,
                    "rts_fn does not take `self` — use #[rts_class] #[method] for instance methods",
                );
            };
            let Some((abi, tsty)) = type_token(&pt.ty) else {
                return err(
                    span,
                    "unsupported parameter type — use a token from rts_abi::ty (Handle/U64/I64/I32/F64/Bool/Str)",
                );
            };
            let Pat::Ident(pi) = &*pt.pat else {
                return err(span, "rts_fn parameters must be simple identifiers");
            };
            let pname_ident = pi.ident.clone();
            let pname = pname_ident.to_string();
            // A leading `_` marks the param unused in the body but is not part
            // of the public name — strip it for the TS signature.
            ts_params.push(format!("{}: {tsty}", pname.trim_start_matches('_')));
            let v = Ident::new(abi, span);
            arg_variants.push(quote! { ::rts_abi::AbiType::#v });

            if abi == "StrPtr" {
                let p_ptr = Ident::new(&format!("{pname}_ptr"), pname_ident.span());
                let p_len = Ident::new(&format!("{pname}_len"), pname_ident.span());
                extern_inputs.push(quote! { #p_ptr: *const u8, #p_len: i64 });
                str_prelude.push(quote! {
                    let #pname_ident = match unsafe { ::rts_abi::str_abi::from_abi(#p_ptr, #p_len) } {
                        ::core::option::Option::Some(s) => s,
                        ::core::option::Option::None => #default_ret,
                    };
                });
            } else {
                extern_inputs.push(quote! { #pt });
            }
        }

        // Constant members are zero-arg and rendered without parens in TS.
        if opts.is_const && !extern_inputs.is_empty() {
            return err(span, "#[rts_const] members must take no arguments");
        }
        let ts_sig = opts.ts.unwrap_or_else(|| {
            if opts.is_const {
                format!("{name}: {ret_ts}")
            } else {
                format!("{name}({}): {ret_ts}", ts_params.join(", "))
            }
        });
        let kind_ident = if opts.is_const {
            Ident::new("Constant", span)
        } else {
            Ident::new("Function", span)
        };
        let pure = opts.pure;

        // Emit the extern "C" symbol — Str prelude, then the user body verbatim.
        let output = &f.sig.output;
        let block = &f.block;
        externs.push(quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #sym_ident(#(#extern_inputs),*) #output {
                #(#str_prelude)*
                #block
            }
        });

        members.push(quote! {
            ::rts_abi::NamespaceMember {
                name: #name,
                kind: ::rts_abi::MemberKind::#kind_ident,
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
