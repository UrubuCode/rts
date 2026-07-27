//! `gen_member` — expand one `#[rtse::ctor]`/`#[rtse::method]`/`#[rtse::statical]`/
//! `#[rtse::getter]`/`#[rtse::setter]` impl fn into an extern-C wrapper + a
//! Registry `Member`. This module is only the orchestrator: it derives the
//! member's identity (JS name, symbol, flags, arity checks) and does the final
//! assembly (the `Sig`/`ts_signature`/`Member` quote); the three real passes —
//! param marshalling, return marshalling, and body construction — live in
//! `params.rs`/`returns.rs`/`body.rs` because each is a self-contained sweep
//! over the signature with its own local state, and `gen_member` itself is
//! already the widest function in the crate once those are inlined.

mod body;
mod params;
mod returns;

use quote::{format_ident, quote};
use syn::{FnArg, Type};

use crate::class::kind::{Kind, SymbolKey};
use crate::naming::{member_sym_const_name, to_camel};
use crate::types::{is_handle_ty, ret_ty};

pub(crate) fn gen_member(
    class: &str,
    self_ty: &Type,
    sig: &syn::Signature,
    kind: Kind,
    symbol_key: Option<SymbolKey>,
    value_class: bool,
    doc: String,
) -> syn::Result<(
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    proc_macro2::Ident,
)> {
    let rust_name = sig.ident.clone();
    let is_ctor = matches!(kind, Kind::Ctor { .. });
    let is_static = matches!(kind, Kind::Static { .. });
    let is_functioncall = matches!(kind, Kind::FunctionCall { .. });
    let is_async = matches!(kind, Kind::Method { is_async: true, .. });
    let is_getter = matches!(kind, Kind::Getter { .. });
    let is_setter = matches!(kind, Kind::Setter { .. });
    // A VALUE-class instance method: the receiver is a primitive word (the first
    // typed param), NOT an `Entry::Rtse` struct — marshalled + called directly, no
    // `with_rtse`. Ctors/statics on a value class stay ordinary (no receiver).
    let is_value_method = value_class && matches!(kind, Kind::Method { .. });
    // `#[rtse::functioncall]` has no receiver — everywhere the param/body/
    // sig-args builders branch on "no implicit `this`" it is treated exactly
    // like `static`.
    let is_no_recv = is_static || is_functioncall;
    let optional = match &kind {
        Kind::Ctor { optional, .. }
        | Kind::Method { optional, .. }
        | Kind::Static { optional, .. }
        | Kind::FunctionCall { optional, .. } => *optional,
        Kind::Getter { .. } | Kind::Setter { .. } => 0,
    };
    let throws = matches!(
        &kind,
        Kind::Ctor { throws: true, .. }
            | Kind::Method { throws: true, .. }
            | Kind::Static { throws: true, .. }
            | Kind::FunctionCall { throws: true, .. }
    );
    // A `returns = "Class"` names the class a `Handle` return carries, so the ts
    // return says `: Class` and the engine's return-class tracking classifies the
    // result (chained `.prop`/`.method()` resolve).
    let returns_class: Option<String> = match &kind {
        Kind::Method { returns, .. }
        | Kind::Static { returns, .. }
        | Kind::Getter { returns, .. }
        | Kind::FunctionCall { returns, .. } => returns.clone(),
        _ => None,
    };
    let (js_name, readonly, private) = match &kind {
        Kind::Ctor { .. } => ("new".to_string(), false, false),
        // No textual JS name — reached only via the engine's call-without-`new`
        // protocol query (`registry::class_functioncall`), never `obj.call(...)`,
        // so it is kept out of `rts.d.ts` like a symbol-keyed member.
        Kind::FunctionCall { .. } => ("call".to_string(), false, true),
        Kind::Method {
            name,
            readonly,
            private,
            ..
        } => (
            name.clone().unwrap_or_else(|| to_camel(&rust_name.to_string())),
            *readonly,
            *private,
        ),
        Kind::Static { name, .. } | Kind::Getter { name, .. } => (
            name.clone().unwrap_or_else(|| to_camel(&rust_name.to_string())),
            false,
            false,
        ),
        // A setter's default JS name strips a leading `set_` (`set_href` → `href`).
        Kind::Setter { name } => (
            name.clone().unwrap_or_else(|| {
                let r = rust_name.to_string();
                to_camel(r.strip_prefix("set_").unwrap_or(&r))
            }),
            false,
            false,
        ),
    };
    // `#[rtse::symbol(...)]` overrides the member's KEY, not its kind: the JS
    // name a caller writes (`method`/`getter`/…) still picks the extern shape,
    // but the Registry name this member resolves under stops being a plain
    // string. Both forms are spelled `@@...` — the SAME internal convention the
    // engine's computed-key desugar already uses for a `.ts` class's literal
    // `[Symbol.iterator]()` (`front/run/desugar/objmethod/collect.rs`), so a
    // Rust-declared member and a `.ts`-declared member resolve through one path,
    // not two. `Symbol.for("foo")` (registry symbol, string-keyed) and
    // `Symbol.iterator` (well-known, unique identity) are DIFFERENT JS values,
    // so they get different key prefixes: `@@sym:` vs `@@`.
    // `name_tok` is the expression the `Member.name` field is built from. For
    // a plain/registry-keyed member it's a compile-time string literal; for a
    // well-known symbol it's `#path.member_key()` — a RUNTIME expression that
    // reads `WellKnown.key` off the const, so the JS key comes from exactly one
    // place (`Symbol::matcher`'s `key: "match"`) even though the Rust ident
    // (`matcher`) differs from the JS name (`match`, a reserved word).
    let (js_name, name_tok, well_known_check) = match &symbol_key {
        None => {
            let tok = quote!(#js_name.into());
            (js_name, tok, None)
        }
        Some(SymbolKey::Registry(name)) => {
            let key = format!("@@sym:{name}");
            let tok = quote!(#key.into());
            (key, tok, None)
        }
        Some(SymbolKey::WellKnown(path)) => {
            // `#path.handle()` / `#path.member_key()` are emitted VERBATIM: if
            // `path` does not resolve to a real `WellKnown` const (a typo'd
            // `Symbol::iteratorr`), this line fails to compile — the check a
            // stringly lookup could not give.
            let check = quote!(let _: u64 = #path.handle(););
            let tok = quote!(#path.member_key());
            // Placeholder for the (unused-when-private) ts_sig formatting below;
            // the real key is `name_tok`, derived from `WellKnown.key` above.
            (String::new(), tok, Some(check))
        }
    };
    // A symbol-keyed member has no textual JS name to publish — it is reached by
    // `obj[Symbol.iterator]`, never `obj.iterator`, so it is kept out of `rts.d.ts`
    // the same way a `#[rtse::private]` member is.
    let private = private || symbol_key.is_some();
    // getter/setter arity guards (the engine expects getter `(recv)->T`, setter
    // `(recv, v)->void`).
    let n_typed = sig
        .inputs
        .iter()
        .filter(|a| matches!(a, FnArg::Typed(_)))
        .count();
    if is_getter && n_typed != 0 {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            "rtse getter: takes only `&self` (a property read has no params)",
        ));
    }
    if is_setter {
        if n_typed != 1 {
            return Err(syn::Error::new_spanned(
                &sig.ident,
                "rtse setter: takes `&mut self` and exactly one value param",
            ));
        }
        if ret_ty(sig).is_some() {
            return Err(syn::Error::new_spanned(
                &sig.ident,
                "rtse setter: must return `()` (the assigned value is discarded)",
            ));
        }
    }

    // `__rtsm_global_<class>_<member>` — derived by the ONE rule in `abi::scope`
    // (a class member IS the `global = "<Class>"` scope). The Rust fn name is used
    // VERBATIM (case-preserved) as `<member>`, so two members differing only by
    // case (`fn Foo` vs `fn foo`) stay distinct symbols instead of colliding.
    let member = rust_name.to_string();
    let symbol = crate::class::class_symbol(class, &member);
    let extern_ident = format_ident!("{}", symbol);

    let p = params::build_params(sig, is_ctor, is_no_recv, is_value_method)?;
    // `optional = N` (attribute) and `Option<T>` (type, see `types::option_inner`)
    // are two spellings of the SAME arity window — fold whichever is larger so a
    // member using either (or, in principle, both) gets one consistent window.
    let optional = optional.max(p.option_count);

    // Return marshalling (+ the F3 async re-wrap into a settled Promise).
    let ret = returns::build_return(sig, class, is_ctor)?;
    let (ret_abi, ret_ext_ty, ret_ts, wrap, ret_class_tok) = if is_async {
        returns::wrap_async(sig, ret)?
    } else {
        ret
    };

    let is_mut_recv = body::check_async_signature(sig, is_async)?;
    let body = body::build_body(
        self_ty,
        &rust_name,
        &p.call_args,
        is_value_method,
        is_ctor,
        is_no_recv,
        is_async,
        p.value_recv_call.as_ref(),
        wrap.as_ref(),
        is_mut_recv,
        &ret_ext_ty,
        &p.setup,
        &p.teardown,
    );

    let ext_params = &p.ext_params;
    let extern_fn = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn #extern_ident(#(#ext_params),*) -> #ret_ext_ty {
            #well_known_check
            #body
        }
    };

    let kind_tok = if is_ctor {
        quote!(::rts_engine::MemberKind::Constructor)
    } else if is_functioncall {
        quote!(::rts_engine::MemberKind::CallWithoutNew)
    } else if is_static {
        quote!(::rts_engine::MemberKind::StaticMethod)
    } else if is_getter {
        quote!(::rts_engine::MemberKind::InstanceGetter)
    } else if is_setter {
        quote!(::rts_engine::MemberKind::InstanceSetter)
    } else {
        quote!(::rts_engine::MemberKind::InstanceMethod)
    };
    // Instance methods carry the receiver in arg slot 0 (a `Handle` for an
    // `Entry::Rtse` class; the primitive's own ABI for a value class); ctor/static
    // carry no receiver.
    let arg_abis = &p.arg_abis;
    let sig_args = if is_ctor || is_no_recv {
        quote!(::std::vec![#(#arg_abis),*])
    } else if is_value_method {
        let recv_abi = p.value_recv_abi.as_ref().expect("value method has a receiver abi");
        quote!(::std::vec![#recv_abi #(, #arg_abis)*])
    } else {
        quote!(::std::vec![::rts_engine::AbiType::Handle #(, #arg_abis)*])
    };
    // F4: the last `optional` explicit params default to `undefined`. Build the
    // `DefaultArg` vec (same length as the Sig args, receiver included → Required)
    // and use `Sig::with_defaults`; else plain `Sig::new`.
    let nparams = p.arg_abis.len();
    if optional > nparams {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            format!("rtse optional={optional}: only {nparams} params to make optional"),
        ));
    }
    let sig_build = if optional == 0 {
        quote!(::rts_engine::Sig::new(#sig_args, #ret_abi))
    } else {
        let required_params = nparams - optional;
        let mut defs: Vec<proc_macro2::TokenStream> = Vec::new();
        if !is_ctor && !is_no_recv {
            defs.push(quote!(::rts_engine::DefaultArg::Required)); // receiver
        }
        for _ in 0..required_params {
            defs.push(quote!(::rts_engine::DefaultArg::Required));
        }
        for _ in 0..optional {
            defs.push(quote!(::rts_engine::DefaultArg::Undefined));
        }
        quote!(::rts_engine::Sig::with_defaults(#sig_args, #ret_abi, ::std::vec![#(#defs),*]))
    };
    // The per-member `SymbolDesc`, derived from the SAME `arg_abis`/`ret_abi`
    // that build `sig_build` above (just `&[..]` instead of `::std::vec![..]`,
    // since `SymbolDesc::params` is `&'static [AbiType]`) — so the const and the
    // `Member.sig` this member registers CANNOT disagree; they are two
    // renderings of one set of tokens, not two derivations. Consumed via
    // `rtse::sym!(Type::member)` at a codegen call site instead of a hand-typed
    // `SymSig` row (`docs/specs/rts-macro-single-source.md`).
    let sym_desc_params = if is_ctor || is_no_recv {
        quote!(&[#(#arg_abis),*])
    } else if is_value_method {
        let recv_abi = p.value_recv_abi.as_ref().expect("value method has a receiver abi");
        quote!(&[#recv_abi #(, #arg_abis)*])
    } else {
        quote!(&[::rts_engine::AbiType::Handle #(, #arg_abis)*])
    };
    let sym_const_ident =
        proc_macro2::Ident::new(&member_sym_const_name(&member), rust_name.span());
    let sym_doc = format!("ABI descriptor for `{symbol}` (generated by `#[rtse::class]`).");
    let sym_desc = quote! {
        impl #self_ty {
            #[doc = #sym_doc]
            pub const #sym_const_ident: ::rts_engine::abi::SymbolDesc =
                ::rts_engine::abi::SymbolDesc {
                    name: #symbol,
                    params: #sym_desc_params,
                    ret: #ret_abi,
                };
        }
    };
    let extern_fn = quote! {
        #extern_fn
        #sym_desc
    };
    // Mark the last `optional` params `?:` in the ts signature.
    let ts_params: Vec<String> = p
        .ts_params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i >= nparams - optional {
                p.replacen(": ", "?: ", 1)
            } else {
                p.clone()
            }
        })
        .collect();
    let ps = ts_params.join(", ");
    // `returns = "Class"` names a Handle return's class in the ts (`: Class`) so
    // the engine classifies the result for chained dispatch. Only applies to a
    // `Handle`-typed return (a String/scalar keeps its own ts).
    let ret_is_handle = ret_ty(sig).is_some_and(|t| is_handle_ty(t).is_some());
    let ret_ts = match &returns_class {
        Some(c) if ret_is_handle => c.clone(),
        _ => ret_ts,
    };
    // `#[rtse::method(returns = "Class")]` is a compile-time-known class name
    // too (just spelled as a string literal, not a `RTSE_CLASS` path) — promote
    // it into `Member.ret_class` the same as a type-inferred class return, so it
    // stops depending on `ts_signature` re-parsing at the consumer.
    let ret_class_tok = match &returns_class {
        Some(c) if ret_is_handle => quote!(::core::option::Option::Some(#c.to_string())),
        _ => ret_class_tok,
    };
    // A private member has NO ts_signature (kept out of `rts.d.ts`).
    let ts_sig = if private {
        String::new()
    } else if is_ctor {
        format!("new {class}({ps}): {class}")
    } else if is_getter {
        // Property read: `name: type`, no parens. Paired setter carries no ts.
        format!("{js_name}: {ret_ts}")
    } else if is_setter {
        String::new()
    } else {
        format!("{js_name}({ps}): {ret_ts}")
    };
    let flags = match (readonly, throws) {
        (false, false) => quote!(::rts_engine::MemberFlags::NONE),
        (true, false) => quote!(::rts_engine::MemberFlags::READONLY),
        (false, true) => quote!(::rts_engine::MemberFlags::THROWS),
        (true, true) => {
            quote!(::rts_engine::MemberFlags::READONLY.or(::rts_engine::MemberFlags::THROWS))
        }
    };

    let member = quote! {
        .member(::rts_engine::Member {
            name: #name_tok,
            kind: #kind_tok,
            sig: #sig_build,
            symbol: #symbol.into(),
            fn_ptr: ::rts_engine::FnPtr(#extern_ident as *const u8),
            flags: #flags,
            aliases: ::std::vec::Vec::new(),
            variadic: false,
            ts_signature: #ts_sig.into(),
            doc: #doc.into(),
            pure: false,
            emit: ::core::option::Option::None,
            ret_class: #ret_class_tok,
        })
    };

    Ok((extern_fn, member, extern_ident))
}
