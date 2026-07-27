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

pub(crate) fn expand(a: TokenStream, item: TokenStream) -> TokenStream {
    let mut func = syn::parse_macro_input!(item as syn::ItemFn);
    let args = match syn::parse::<AbiArgs>(a) {
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
    let mut wrap = false;
    for (idx, arg) in func.sig.inputs.iter().enumerate() {
        let syn::FnArg::Typed(pt) = arg else {
            return syn::Error::new_spanned(
                arg,
                "#[rtse::function]: a free function has no receiver — `self` is not valid here",
            )
            .to_compile_error()
            .into();
        };
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
    let ret_ts = match &func.sig.output {
        syn::ReturnType::Default => "void".to_string(),
        syn::ReturnType::Type(_, t) => ts_of(t),
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
                sig: ::rts_engine::Sig::new(::std::vec![#(#abis),*], #ret_abi),
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
