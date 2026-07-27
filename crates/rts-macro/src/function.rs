//! `#[rtse::function]` — a FREE function member: no receiver, no class. The
//! missing member kind `#[rtse::class]`'s `gen_impl` could not express (it
//! always emits `e.class(#name)` — every generated symbol had to be a class
//! member until now).
//!
//! ```ignore
//! #[rtse::function(module = "node:fs", value = "readFile")]
//! fn read_file(path: &str, encoding: Poly) -> Handle { … }
//! ```
//!
//! Reuses TWO existing single-source pieces rather than inventing new ones:
//!  - `crate::abi::scope` (`AbiArgs`/`symbol_for`) for scope parsing + symbol
//!    derivation — the SAME rule `#[rtse::abi]` uses, so a free function and a
//!    raw ABI symbol in the same module cannot drift onto different naming.
//!  - `crate::abi::params` (`param_of`/`ret_of`) for the extern-C marshalling —
//!    a free function has the identical shape to an `#[rtse::abi]` symbol (no
//!    receiver), so the parameter table is not re-derived.
//!
//! # Why `pub fn …_entry() -> Member`, not a `const`
//!
//! The design doc's sketch is `.registry(CONST)` with CONST a real `const`. In
//! practice a `Member` cannot be a compile-time `const` value: building one
//! needs `.into()`/`.to_string()` on its `String` fields, and those are not
//! `const fn`. `#[rtse::constant]` hit the exact same wall and settled on
//! emitting a `pub fn <name>_member() -> Member`; this macro stays consistent
//! with that established precedent rather than inventing a second convention.
//! `.registry(read_file_entry())` is called at the call site, same as
//! `.member(pi_member())` is today.

use proc_macro::TokenStream;
use quote::{format_ident, quote};

use crate::abi::params::{param_of, ret_of};
use crate::abi::scope::{AbiArgs, symbol_for};
use crate::naming::{abi_const_name, to_camel};

/// Parse and STRIP a `#[default(...)]` attribute from one parameter's attrs,
/// returning the [`DefaultArg`](rts_engine::DefaultArg)-shaped token stream to
/// embed in the generated `Sig`. Written once, next to the parameter it
/// defaults — the single-source spot an author reads to know a call omitting
/// this argument is legal and what it is padded with.
///
/// ```ignore
/// #[rtse::function(module = "node:fs")]
/// fn read_file(path: &str, #[default("utf8")] encoding: Poly) -> Poly { … }
/// ```
///
/// Accepted forms (parsed from the token inside the parens):
///  - `#[default(undefined)]` / `#[default(null)]` — the JS singletons.
///  - `#[default(nan)]` / `#[default(infinity)]` — the two `f64` sentinels
///    that read awkwardly as a bare float literal.
///  - `#[default(true)]` / `#[default(false)]` — a boolean literal.
///  - `#[default(10)]` — an integer literal → `DefaultArg::Int`.
///  - `#[default(1.5)]` — a float literal → `DefaultArg::Float`.
///  - `#[default("utf8")]` — a string literal → `DefaultArg::Str`, materialized
///    at the call site through the ordinary string-construction path (never a
///    hand-baked handle).
///
/// The attribute is purely a macro-time marker: it is REMOVED from `pt.attrs`
/// before the parameter is re-emitted, so it never reaches rustc as a real
/// attribute on the generated `extern "C"` (or wrapped) function.
fn take_default(pt: &mut syn::PatType) -> syn::Result<Option<proc_macro2::TokenStream>> {
    let Some(idx) = pt.attrs.iter().position(|a| a.path().is_ident("default")) else {
        return Ok(None);
    };
    let attr = pt.attrs.remove(idx);
    let syn::Meta::List(list) = &attr.meta else {
        return Err(syn::Error::new_spanned(
            &attr,
            "#[default(...)]: expected a parenthesized value, e.g. `#[default(\"utf8\")]`",
        ));
    };
    let toks = list.tokens.clone();
    if let Ok(ident) = syn::parse2::<syn::Ident>(toks.clone()) {
        let name = ident.to_string();
        return Ok(Some(match name.as_str() {
            "undefined" => quote!(::rts_engine::DefaultArg::Undefined),
            "null" => quote!(::rts_engine::DefaultArg::Null),
            "nan" => quote!(::rts_engine::DefaultArg::Float(f64::NAN)),
            "infinity" => quote!(::rts_engine::DefaultArg::Float(f64::INFINITY)),
            "true" => quote!(::rts_engine::DefaultArg::Bool(true)),
            "false" => quote!(::rts_engine::DefaultArg::Bool(false)),
            _ => {
                return Err(syn::Error::new_spanned(
                    &attr,
                    "#[default(...)]: unknown keyword — expected `undefined`, `null`, `nan`, \
                     `infinity`, `true`, or `false`, or a literal",
                ));
            }
        }));
    }
    let lit: syn::Lit = syn::parse2(toks).map_err(|_| {
        syn::Error::new_spanned(
            &attr,
            "#[default(...)]: expected a keyword or a literal (int/float/string/bool)",
        )
    })?;
    Ok(Some(match lit {
        syn::Lit::Str(s) => {
            let v = s.value();
            quote!(::rts_engine::DefaultArg::Str(#v))
        }
        syn::Lit::Int(n) => {
            let v: i64 = n.base10_parse()?;
            quote!(::rts_engine::DefaultArg::Int(#v))
        }
        syn::Lit::Float(f) => {
            let v: f64 = f.base10_parse()?;
            quote!(::rts_engine::DefaultArg::Float(#v))
        }
        syn::Lit::Bool(b) => {
            let v = b.value;
            quote!(::rts_engine::DefaultArg::Bool(#v))
        }
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "#[default(...)]: unsupported literal kind",
            ));
        }
    }))
}

/// Parse and STRIP a `#[ts("…")]` attribute overriding the DERIVED TS return
/// type.
///
/// The derivation ([`ts_of`]) maps the ABI type, which is all it can see — and
/// for `Handle` that is genuinely ambiguous. The engine reboxes a `Handle`
/// return by what the `ts_signature` DECLARES (`registry.rs`:
/// `ts_returns_string` / `ts_returns_array` / `ts_returns_object`), so the same
/// `-> Handle` is a string, an array, an object, or an OPAQUE key depending on
/// what the member means. `rts:gpu`'s `shader()` returns a pipeline handle the
/// script treats as a plain `number`; derived as `object` it reboxes into `[]`
/// and every later call gets an invalid handle.
///
/// So the ambiguity is resolved where the answer is known — on the declaration:
///
/// ```ignore
/// #[rtse::function(module = "rts:gpu", value = "shader")]
/// #[ts("number")]
/// fn shader(wgsl: &str) -> Handle { … }
/// ```
///
/// Only the RETURN type is overridable: parameter TS names are cosmetic (they
/// appear in the generated `.d.ts`), while the return type drives real reboxing
/// behavior. Purely a macro-time marker, removed before the function is
/// re-emitted — and invisible to `rts-symbol-baker`, which keys on the naming
/// attribute's path alone.
fn take_ts_override(attrs: &mut Vec<syn::Attribute>) -> syn::Result<Option<String>> {
    let Some(idx) = attrs.iter().position(|a| a.path().is_ident("ts")) else {
        return Ok(None);
    };
    let attr = attrs.remove(idx);
    let lit: syn::LitStr = attr.parse_args().map_err(|_| {
        syn::Error::new_spanned(
            &attr,
            "#[ts(...)]: expected a TS type as a string, e.g. `#[ts(\"number\")]`",
        )
    })?;
    Ok(Some(lit.value()))
}

pub(crate) fn expand(a: TokenStream, item: TokenStream) -> TokenStream {
    let mut func = syn::parse_macro_input!(item as syn::ItemFn);
    let args = match syn::parse::<AbiArgs>(a) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };
    let ts_override = match take_ts_override(&mut func.attrs) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };
    let rust_name = func.sig.ident.clone();
    let sym = symbol_for(&args.0, &rust_name.to_string());

    // `value = "…"` names the JS member directly; otherwise camelCase the Rust
    // fn name, same rule `#[rtse::class]`'s members use.
    let js_name = match &args.0 {
        rts_abi::scope::Naming::Scoped {
            value: Some(v), ..
        } => v.clone(),
        _ => to_camel(&rust_name.to_string()),
    };

    if func.sig.abi.is_some() {
        return syn::Error::new_spanned(
            &func.sig,
            "#[rtse::function]: drop the `extern \"C\"` — the attribute applies it",
        )
        .to_compile_error()
        .into();
    }

    let mut abis: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut ext_params: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut call_args: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut ts_params: Vec<String> = Vec::new();
    // Parallel to `abis`: `None` until the first `#[default(...)]` is seen, then
    // every SUBSEQUENT slot must also carry one (a required param cannot follow
    // an optional one) — enforced right after the loop.
    let mut defaults: Vec<Option<proc_macro2::TokenStream>> = Vec::new();
    let mut wrap = false;
    for (idx, arg) in func.sig.inputs.iter_mut().enumerate() {
        let syn::FnArg::Typed(pt) = arg else {
            return syn::Error::new_spanned(
                arg,
                "#[rtse::function]: a free function has no receiver — `self` is not valid here",
            )
            .to_compile_error()
            .into();
        };
        let default = match take_default(pt) {
            Ok(d) => d,
            Err(e) => return e.to_compile_error().into(),
        };
        defaults.push(default);
        let Some(p) = param_of(&pt.ty, idx) else {
            return syn::Error::new_spanned(
                &pt.ty,
                "#[rtse::function]: no ABI spelling for this parameter type — one of `Handle`, \
                 `Poly`, `U64`, `I64`, `I32`, `F64`, `Bool`, a Rust scalar, or `&str`.",
            )
            .to_compile_error()
            .into();
        };
        let pname = match &*pt.pat {
            syn::Pat::Ident(i) => i.ident.to_string(),
            _ => format!("arg{idx}"),
        };
        ts_params.push(format!("{pname}: {}", ts_of(&pt.ty)));
        abis.push(p.abi);
        ext_params.extend(p.ext);
        call_args.push(p.call);
        wrap |= p.needs_wrapper;
    }

    // A `#[default(...)]` param must not be followed by a plain required one —
    // JS optional-tail semantics, same rule the engine's arity window assumes
    // (`required_args` reads the leading run of `DefaultArg::Required`).
    let first_default = defaults.iter().position(Option::is_some);
    if let Some(start) = first_default {
        if let Some(gap) = defaults[start..].iter().position(Option::is_none) {
            let bad = &func.sig.inputs[start + gap];
            return syn::Error::new_spanned(
                bad,
                "#[rtse::function]: a required parameter cannot follow a `#[default(...)]` one",
            )
            .to_compile_error()
            .into();
        }
    }
    let default_args: Vec<proc_macro2::TokenStream> = if first_default.is_some() {
        defaults
            .iter()
            .map(|d| match d {
                Some(t) => t.clone(),
                None => quote!(::rts_engine::DefaultArg::Required),
            })
            .collect()
    } else {
        Vec::new()
    };

    let Some(ret) = ret_of(&func.sig.output) else {
        return syn::Error::new_spanned(
            &func.sig.output,
            "#[rtse::function]: no ABI spelling for this return type — a string result is \
             returned as a `Handle`, not `&str`.",
        )
        .to_compile_error()
        .into();
    };
    wrap |= ret.needs_wrapper;
    let ret_ts = match (ts_override, &func.sig.output) {
        (Some(t), _) => t,
        (None, syn::ReturnType::Default) => "void".to_string(),
        (None, syn::ReturnType::Type(_, t)) => ts_of(t),
    };

    let ret_abi = ret.abi;
    let ret_ext_ty = ret.ext_ty.clone();
    let const_ident = proc_macro2::Ident::new(&abi_const_name(&sym), rust_name.span());
    let extern_ident = proc_macro2::Ident::new(&sym, rust_name.span());
    let entry_fn = format_ident!("{}_entry", rust_name.to_string().to_lowercase());
    let doc = extract_doc(&func.attrs);
    let ts_sig = format!("{js_name}({}): {ret_ts}", ts_params.join(", "));
    let desc_doc = format!("ABI descriptor for `{sym}` (generated by `#[rtse::function]`).");
    let entry_doc = format!("Registry entry for the free function `{js_name}` — pass it to `.registry(...)`.");

    let emitted = if wrap {
        let inner_ident = format_ident!("__rtsfn_inner_{}", sym);
        func.sig.ident = inner_ident.clone();
        func.vis = syn::Visibility::Inherited;
        func.attrs.push(syn::parse_quote!(#[inline(always)]));
        let call = quote!(#inner_ident(#(#call_args),*));
        let body = match ret.convert {
            Some(conv) => quote!({ let __r = #call; #conv }),
            None => quote!({ #call }),
        };
        quote! {
            #func
            #[unsafe(no_mangle)]
            pub extern "C" fn #extern_ident(#(#ext_params),*) -> #ret_ext_ty #body
        }
    } else {
        func.sig.ident = extern_ident.clone();
        func.sig.abi = Some(syn::parse_quote!(extern "C"));
        func.attrs.push(syn::parse_quote!(#[unsafe(no_mangle)]));
        quote!(#func)
    };

    quote! {
        #emitted

        #[doc = #desc_doc]
        pub const #const_ident: ::rts_engine::abi::SymbolDesc =
            ::rts_engine::abi::SymbolDesc {
                name: #sym,
                params: &[#(#abis),*],
                ret: #ret_abi,
            };

        #[doc = #entry_doc]
        pub fn #entry_fn() -> ::rts_engine::Member {
            ::rts_engine::Member {
                name: #js_name.into(),
                kind: ::rts_engine::MemberKind::Function,
                sig: ::rts_engine::Sig::with_defaults(
                    ::std::vec![#(#abis),*],
                    #ret_abi,
                    ::std::vec![#(#default_args),*],
                ),
                symbol: #sym.into(),
                fn_ptr: ::rts_engine::FnPtr(#extern_ident as *const u8),
                flags: ::rts_engine::MemberFlags::NONE,
                aliases: ::std::vec::Vec::new(),
                variadic: false,
                ts_signature: #ts_sig.into(),
                doc: #doc.into(),
                pure: false,
                emit: ::core::option::Option::None,
            }
        }
    }
    .into()
}

/// The ts type-name of a supported `#[rtse::function]` param/return type.
/// A local mapping (not `types::scalar`, which returns the extern-repr triple
/// a class member needs): a free function only needs the TS spelling.
fn ts_of(ty: &syn::Type) -> String {
    if crate::types::is_str_param(ty) || crate::types::is_string_ret(ty) {
        return "string".to_string();
    }
    if crate::types::is_poly_ty(ty) {
        return "any".to_string();
    }
    if crate::types::is_handle_ty(ty).is_some() {
        return "object".to_string();
    }
    let syn::Type::Path(p) = ty else { return "number".to_string() };
    match p.path.segments.last().map(|s| s.ident.to_string()).as_deref() {
        Some("bool" | "Bool") => "boolean".to_string(),
        _ => "number".to_string(),
    }
}

/// Harvest `///` docs into `Member.doc`.
fn extract_doc(attrs: &[syn::Attribute]) -> String {
    let mut lines: Vec<String> = Vec::new();
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
                let v = s.value();
                lines.push(v.strip_prefix(' ').unwrap_or(&v).to_string());
            }
        }
    }
    lines.join("\n")
}
