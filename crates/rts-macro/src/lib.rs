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
use syn::parse::{Parse, ParseStream};
use syn::{FnArg, Ident, ImplItem, ItemImpl, Pat, ReturnType, Token, Type};

/// `#[rts_namespace(<name>)]` or `#[rts_namespace(<name>, part)]`.
///
/// `part` marks an impl block that contributes members to a namespace split
/// across several files (e.g. `collections` = `map` + `vec`): the block emits
/// its externs + a `pub const MEMBERS`, but NOT the `SPEC` — a single owning
/// module aggregates the parts via `rts_abi::concat_members`.
struct NsAttr {
    name: Ident,
    part: bool,
}

impl Parse for NsAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let mut part = false;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            let kw: Ident = input.parse()?;
            if kw == "part" {
                part = true;
            } else {
                return Err(syn::Error::new(kw.span(), "expected `part`"));
            }
        }
        Ok(NsAttr { name, part })
    }
}

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

/// Parsed `#[rts_fn(...)]` / `#[rts_const(...)]` / `#[rts_alias(...)]` options.
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
    /// `intrinsic = <Variant>` → `intrinsic: Some(Intrinsic::<Variant>)`.
    intrinsic: Option<Ident>,
    /// `#[rts_alias(of = <name>)]` — emits a member pointing at `<name>`'s
    /// symbol WITHOUT emitting an extern (the canonical fn already owns it).
    alias_of: Option<String>,
}

fn parse_member(attrs: &[syn::Attribute]) -> Option<FnOpts> {
    let (attr, is_const, is_alias) = attrs
        .iter()
        .find(|a| a.path().is_ident("rts_fn"))
        .map(|a| (a, false, false))
        .or_else(|| {
            attrs
                .iter()
                .find(|a| a.path().is_ident("rts_const"))
                .map(|a| (a, true, false))
        })
        .or_else(|| {
            attrs
                .iter()
                .find(|a| a.path().is_ident("rts_alias"))
                .map(|a| (a, false, true))
        })?;
    let mut opts = FnOpts {
        ts: None,
        pure: false,
        is_const,
        on_null: None,
        intrinsic: None,
        alias_of: if is_alias { Some(String::new()) } else { None },
    };
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
        } else if m.path.is_ident("intrinsic") {
            let v = m.value()?;
            let id: Ident = v.parse()?;
            opts.intrinsic = Some(id);
        } else if m.path.is_ident("of") {
            // `#[rts_alias(of = ln)]` — the canonical fn name.
            let v = m.value()?;
            let id: Ident = v.parse()?;
            opts.alias_of = Some(id.to_string());
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
    let NsAttr { name: ns, part } = syn::parse_macro_input!(attr as NsAttr);
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
        // An alias points at the canonical fn's symbol (`of = <name>`); a normal
        // member owns `__RTS_FN_NS_<NS>_<NAME>`.
        let symbol = match &opts.alias_of {
            Some(target) => format!("__RTS_FN_NS_{ns_upper}_{}", target.to_uppercase()),
            None => format!("__RTS_FN_NS_{ns_upper}_{name_upper}"),
        };
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

        // An alias emits NO extern — it reuses the canonical fn's symbol.
        let is_alias = opts.alias_of.is_some();
        if !is_alias {
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
        }

        let intrinsic_tok = match &opts.intrinsic {
            Some(variant) => quote! { Some(::rts_abi::Intrinsic::#variant) },
            None => quote! { None },
        };

        members.push(quote! {
            ::rts_abi::NamespaceMember {
                name: #name,
                kind: ::rts_abi::MemberKind::#kind_ident,
                symbol: #symbol,
                args: &[ #(#arg_variants),* ],
                returns: ::rts_abi::AbiType::#ret_ident,
                doc: #doc,
                ts_signature: #ts_sig,
                intrinsic: #intrinsic_tok,
                pure: #pure,
            }
        });
    }

    // A `part` impl emits only externs + MEMBERS; the owning module aggregates
    // the parts into one SPEC via `rts_abi::concat_members`. A non-part impl
    // emits the full triple (externs + MEMBERS + SPEC) as before.
    let spec = if part {
        quote! {}
    } else {
        quote! {
            /// Derived namespace spec — replaces the hand-written `SPEC` const.
            pub const SPEC: ::rts_abi::NamespaceSpec = ::rts_abi::NamespaceSpec {
                name: #ns_str,
                doc: #spec_doc,
                members: MEMBERS,
            };
        }
    };

    let out = quote! {
        #(#externs)*

        /// Derived namespace members (`#[rts_namespace]`). Source of truth.
        pub const MEMBERS: &[::rts_abi::NamespaceMember] = &[ #(#members),* ];

        #spec
    };
    out.into()
}

/// Parsed options for a `#[rts_class]` member. The kind is fixed by which
/// attribute tagged the method; `name`/`ts` override the JS-visible name and
/// signature (required for camelCase names, `any`/class-typed params, and the
/// receiver-dropping convention of instance methods).
struct ClassFnOpts {
    /// `MemberKind` variant name.
    kind: &'static str,
    /// `true` for `#[rts_ctor]` — member name forced to `"new"`.
    is_ctor: bool,
    name: Option<String>,
    ts: Option<String>,
    /// Full symbol override — for members whose Rust fn ident can't match the
    /// canonical symbol (e.g. JS `for`, a Rust keyword, → `__RTS_FN_GL_SYMBOL_FOR`).
    symbol: Option<String>,
    /// When set, `Str` params bind to `Option<&str>` (null/invalid → `None`,
    /// no early return) instead of `&str`. For optional-string args like
    /// `new Symbol(description?)` where null means "absent", not "fail".
    opt_str: bool,
    pure: bool,
    intrinsic: Option<Ident>,
}

fn parse_class_member(attrs: &[syn::Attribute]) -> Option<ClassFnOpts> {
    // Map the tagging attribute to a MemberKind. `rts_fn` = static `Function`,
    // `rts_const` = `Constant` (same spelling as the namespace macro, by design).
    let kinds: &[(&str, &str, bool)] = &[
        ("rts_ctor", "Constructor", true),
        ("rts_fn", "Function", false),
        ("rts_smethod", "StaticMethod", false),
        ("rts_method", "InstanceMethod", false),
        ("rts_getter", "InstanceGetter", false),
        ("rts_const", "Constant", false),
    ];
    let (attr, kind, is_ctor) = kinds.iter().find_map(|(id, kind, is_ctor)| {
        attrs
            .iter()
            .find(|a| a.path().is_ident(id))
            .map(|a| (a, *kind, *is_ctor))
    })?;
    let mut opts = ClassFnOpts {
        kind,
        is_ctor,
        name: None,
        ts: None,
        symbol: None,
        opt_str: false,
        pure: false,
        intrinsic: None,
    };
    if matches!(attr.meta, syn::Meta::Path(_)) {
        return Some(opts);
    }
    let _ = attr.parse_nested_meta(|m| {
        if m.path.is_ident("pure") {
            opts.pure = true;
        } else if m.path.is_ident("opt_str") {
            opts.opt_str = true;
        } else if m.path.is_ident("name") {
            let v = m.value()?;
            let s: syn::LitStr = v.parse()?;
            opts.name = Some(s.value());
        } else if m.path.is_ident("ts") {
            let v = m.value()?;
            let s: syn::LitStr = v.parse()?;
            opts.ts = Some(s.value());
        } else if m.path.is_ident("symbol") {
            let v = m.value()?;
            let s: syn::LitStr = v.parse()?;
            opts.symbol = Some(s.value());
        } else if m.path.is_ident("intrinsic") {
            let v = m.value()?;
            let id: Ident = v.parse()?;
            opts.intrinsic = Some(id);
        }
        Ok(())
    });
    Some(opts)
}

/// `#[rts_class(<ClassName>)]` on an `impl` block — derives a `GlobalClassSpec`
/// for a built-in JS class (`Boolean`, `Date`, …). Each method is tagged with
/// `#[rts_ctor]` / `#[rts_fn]` (static) / `#[rts_smethod]` / `#[rts_method]`
/// (instance) / `#[rts_getter]` / `#[rts_const]`. Emits, per method, the
/// `#[no_mangle] extern "C"` symbol `__RTS_FN_GL_<CLASS>_<FN_IDENT>` plus a
/// `NamespaceMember`; aggregates them into `pub const <CLASS>_CLASS_SPEC`.
///
/// Instance methods/getters take the receiver as their first parameter (an
/// `I64`/`U64` handle): it stays in `args` but the `ts` (always explicit for
/// class members) lists only the explicit params.
#[proc_macro_attribute]
pub fn rts_class(attr: TokenStream, item: TokenStream) -> TokenStream {
    let class = syn::parse_macro_input!(attr as Ident);
    let imp = syn::parse_macro_input!(item as ItemImpl);

    let class_str = class.to_string();
    let class_upper = class_str.to_uppercase();
    let spec_doc = doc_of(&imp.attrs);
    let spec_ident = Ident::new(&format!("{class_upper}_CLASS_SPEC"), class.span());

    let mut externs = Vec::new();
    let mut members = Vec::new();

    for it in &imp.items {
        let ImplItem::Fn(f) = it else { continue };
        let Some(opts) = parse_class_member(&f.attrs) else {
            continue; // helper fn — leave untouched (not re-emitted; see note)
        };

        let span = f.sig.ident.span();
        let fn_name = f.sig.ident.to_string();
        let symbol = opts
            .symbol
            .clone()
            .unwrap_or_else(|| format!("__RTS_FN_GL_{class_upper}_{}", fn_name.to_uppercase()));
        let sym_ident = Ident::new(&symbol, span);
        let doc = doc_of(&f.attrs);

        let (ret_variant, _ret_ts) = match &f.sig.output {
            ReturnType::Default => ("Void", "void"),
            ReturnType::Type(_, ty) => match type_token(ty) {
                Some(("StrPtr", _)) => {
                    return err(span, "Str is not a valid return type — return Handle");
                }
                Some((abi, tsty)) => (abi, tsty),
                None => {
                    return err(
                        span,
                        "unsupported return type — use a token from rts_abi::ty",
                    )
                }
            },
        };
        let ret_ident = Ident::new(ret_variant, span);
        let default_ret = default_return(ret_variant);

        // Derive args (Str → ptr+len, with reconstruction prelude) exactly like
        // the namespace macro. The receiver (first param of instance methods) is
        // just another typed arg.
        let mut arg_variants = Vec::new();
        let mut extern_inputs = Vec::new();
        let mut str_prelude = Vec::new();
        for input in &f.sig.inputs {
            let FnArg::Typed(pt) = input else {
                return err(span, "rts_class methods do not take `self` — pass the receiver as a typed handle param");
            };
            let Some((abi, _tsty)) = type_token(&pt.ty) else {
                return err(
                    span,
                    "unsupported parameter type — use a token from rts_abi::ty",
                );
            };
            let Pat::Ident(pi) = &*pt.pat else {
                return err(span, "rts_class parameters must be simple identifiers");
            };
            let pname_ident = pi.ident.clone();
            let v = Ident::new(abi, span);
            arg_variants.push(quote! { ::rts_abi::AbiType::#v });
            if abi == "StrPtr" {
                let pname = pname_ident.to_string();
                let p_ptr = Ident::new(&format!("{pname}_ptr"), pname_ident.span());
                let p_len = Ident::new(&format!("{pname}_len"), pname_ident.span());
                extern_inputs.push(quote! { #p_ptr: *const u8, #p_len: i64 });
                if opts.opt_str {
                    // Optional string: bind the raw `Option<&str>` (null/invalid
                    // → None), let the body decide.
                    str_prelude.push(quote! {
                        let #pname_ident = unsafe { ::rts_abi::str_abi::from_abi(#p_ptr, #p_len) };
                    });
                } else {
                    str_prelude.push(quote! {
                        let #pname_ident = match unsafe { ::rts_abi::str_abi::from_abi(#p_ptr, #p_len) } {
                            ::core::option::Option::Some(s) => s,
                            ::core::option::Option::None => #default_ret,
                        };
                    });
                }
            } else {
                extern_inputs.push(quote! { #pt });
            }
        }

        // Class members require an explicit `ts` (receiver-drop / `any` / class
        // return types make derivation unreliable). Ctor name is forced to "new".
        let Some(ts_sig) = opts.ts.clone() else {
            return err(span, "rts_class members require an explicit ts = \"...\"");
        };
        let name = if opts.is_ctor {
            "new".to_string()
        } else {
            opts.name.clone().unwrap_or_else(|| fn_name.clone())
        };
        let kind_ident = Ident::new(opts.kind, span);
        let pure = opts.pure;
        let intrinsic_tok = match &opts.intrinsic {
            Some(variant) => quote! { Some(::rts_abi::Intrinsic::#variant) },
            None => quote! { None },
        };

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
                intrinsic: #intrinsic_tok,
                pure: #pure,
            }
        });
    }

    let out = quote! {
        #(#externs)*

        /// Derived class members (`#[rts_class]`). Source of truth.
        pub const MEMBERS: &[::rts_abi::NamespaceMember] = &[ #(#members),* ];

        /// Derived global-class spec — replaces the hand-written `*_CLASS_SPEC`.
        pub const #spec_ident: ::rts_abi::GlobalClassSpec = ::rts_abi::GlobalClassSpec {
            name: #class_str,
            doc: #spec_doc,
            members: MEMBERS,
        };
    };
    out.into()
}
