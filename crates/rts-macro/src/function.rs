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
use crate::types::option_inner;

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

/// Wrap a marshalled-value expression (`raw`, already the bare-`T` call
/// expression `param_of` built) into `Option<T>`, `None` when `raw` equals the
/// "absent" sentinel `DefaultArg::Undefined` injects at the call site for that
/// `AbiType` — the same per-type sentinel `class::member::params` uses (NaN for
/// `f64`, `0` for `Handle`/`U64`, `""` for `&str`). `None` return = this type has
/// no established sentinel (i64/i32/bool/Poly); the caller reports it.
fn option_wrap_call(ty: &syn::Type, raw: &proc_macro2::TokenStream) -> Option<proc_macro2::TokenStream> {
    if crate::types::is_str_param(ty) {
        return Some(quote!({
            let __s: &str = #raw;
            if __s.is_empty() { ::core::option::Option::None } else { ::core::option::Option::Some(__s) }
        }));
    }
    if crate::types::is_handle_ty(ty).is_some() {
        return Some(quote!({
            let __h = #raw;
            if __h == 0 { ::core::option::Option::None } else { ::core::option::Option::Some(__h) }
        }));
    }
    if matches!(ty, syn::Type::Path(p) if p.path.is_ident("f64") || p.path.is_ident("F64")) {
        return Some(quote!({
            let __f = #raw;
            if __f.is_nan() { ::core::option::Option::None } else { ::core::option::Option::Some(__f) }
        }));
    }
    None
}

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
    // Parallel to `abis`: `None` until the first optional slot is seen (either a
    // `#[default(...)]` or an `Option<T>` param), then every SUBSEQUENT slot
    // must also be optional — enforced right after the loop.
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
        let explicit_default = match take_default(pt) {
            Ok(d) => d,
            Err(e) => return e.to_compile_error().into(),
        };
        // `Option<T>` (type) and `#[default(v)]` (attribute) COMPOSE: `Option<T>`
        // says "may be absent" and picks the arity window + the `Option<T>`
        // Rust-side value; `#[default(v)]` says "and pad omitted calls with `v`"
        // instead of the JS `undefined` sentinel — so `#[default("utf8")]
        // enc: Option<Poly>` reads `Some("utf8")` when the caller omits `enc`,
        // `None` only if the caller passes an explicit `undefined`. Without an
        // explicit default, an `Option<T>` param still gets `DefaultArg::Undefined`
        // automatically — no attribute needed to make it optional.
        let is_optional = option_inner(&pt.ty).is_some();
        let marshal_ty: std::borrow::Cow<syn::Type> = match option_inner(&pt.ty) {
            Some(inner) => std::borrow::Cow::Owned(inner.clone()),
            None => std::borrow::Cow::Borrowed(&*pt.ty),
        };
        let default = explicit_default
            .clone()
            .or_else(|| is_optional.then(|| quote!(::rts_engine::DefaultArg::Undefined)));
        defaults.push(default);
        let Some(p) = param_of(&marshal_ty, idx) else {
            return syn::Error::new_spanned(
                &pt.ty,
                "#[rtse::function]: no ABI spelling for this parameter type — one of `Handle`, \
                 `Poly`, `U64`, `I64`, `I32`, `F64`, `Bool`, a Rust scalar, or `&str`.",
            )
            .to_compile_error()
            .into();
        };
        let call = if is_optional {
            let Some(wrapped) = option_wrap_call(&marshal_ty, &p.call) else {
                return syn::Error::new_spanned(
                    &pt.ty,
                    "#[rtse::function]: `Option<T>` is only supported for f64/&str/Handle/U64 \
                     params (no absent-sentinel convention for i64/i32/bool/Poly) — use \
                     `#[default(...)]` on the bare type instead",
                )
                .to_compile_error()
                .into();
            };
            wrap = true;
            wrapped
        } else {
            p.call
        };
        let pname = match &*pt.pat {
            syn::Pat::Ident(i) => i.ident.to_string(),
            _ => format!("arg{idx}"),
        };
        let ts_ty = ts_of(&marshal_ty);
        ts_params.push(if is_optional {
            format!("{pname}?: {ts_ty}")
        } else {
            format!("{pname}: {ts_ty}")
        });
        abis.push(p.abi);
        ext_params.extend(p.ext);
        call_args.push(call);
        wrap |= p.needs_wrapper;
    }

    // An optional param (explicit default OR `Option<T>`) must not be followed
    // by a plain required one — JS optional-tail semantics, same rule the
    // engine's arity window assumes (`required_args` reads the leading run of
    // `DefaultArg::Required`).
    let first_default = defaults.iter().position(Option::is_some);
    if let Some(start) = first_default {
        if let Some(gap) = defaults[start..].iter().position(Option::is_none) {
            let bad = &func.sig.inputs[start + gap];
            return syn::Error::new_spanned(
                bad,
                "#[rtse::function]: a required parameter cannot follow an optional one \
                 (`#[default(...)]` or `Option<T>`)",
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

    // `-> SomeClass` / `-> Option<SomeClass>`: the function returns a fresh
    // instance of a `#[rtse::class("Name")]`-declared class BY VALUE (e.g.
    // `fn create_read_stream(path: &str) -> StreamReader`). Boxed into a classed
    // `Entry::Rtse` (same allocation `-> Self` uses on a class ctor), and the
    // class name is read from `SomeClass::RTSE_CLASS` — an associated const
    // `#[rtse::class]` emits on the type — so a typo'd type here is a `rustc`
    // unresolved-path error, not a silent runtime miss. `RTSE_CLASS` also fills
    // `Member.ret_class` as DATA, not a `ts_signature` string to re-parse.
    let class_ret = crate::types::ret_ty(&func.sig).and_then(|t| {
        if let Some(ct) = crate::types::is_other_class_ret(t) {
            Some((ct.clone(), false))
        } else {
            crate::types::is_option_other_class_ret(t).map(|ct| (ct.clone(), true))
        }
    });

    let (ret_abi, ret_ext_ty, ret_ts, ret_convert, ret_class_tok) = if let Some((cls_ty, nullable)) =
        &class_ret
    {
        wrap = true;
        let conv = if *nullable {
            quote!(match __r {
                ::core::option::Option::Some(__v) =>
                    ::rts_engine::heap::handles::alloc_rtse(<#cls_ty>::RTSE_CLASS, __v),
                ::core::option::Option::None => 0u64,
            })
        } else {
            quote!(::rts_engine::heap::handles::alloc_rtse(<#cls_ty>::RTSE_CLASS, __r))
        };
        (
            quote!(::rts_engine::abi::AbiType::Handle),
            quote!(u64),
            "object".to_string(),
            Some(conv),
            quote!(::core::option::Option::Some(<#cls_ty>::RTSE_CLASS.to_string())),
        )
    } else {
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
        let ts = match &func.sig.output {
            syn::ReturnType::Default => "void".to_string(),
            syn::ReturnType::Type(_, t) => ts_of(t),
        };
        (
            ret.abi,
            ret.ext_ty,
            ts,
            ret.convert,
            quote!(::core::option::Option::None),
        )
    };
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
        let body = match ret_convert {
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
                ret_class: #ret_class_tok,
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
