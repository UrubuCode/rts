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

use crate::class::kind::Kind;
use crate::naming::to_camel;
use crate::types::{is_handle_ty, ret_ty};

pub(crate) fn gen_member(
    class: &str,
    self_ty: &Type,
    sig: &syn::Signature,
    kind: Kind,
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
    let is_async = matches!(kind, Kind::Method { is_async: true, .. });
    let is_getter = matches!(kind, Kind::Getter { .. });
    let is_setter = matches!(kind, Kind::Setter { .. });
    // A VALUE-class instance method: the receiver is a primitive word (the first
    // typed param), NOT an `Entry::Rtse` struct — marshalled + called directly, no
    // `with_rtse`. Ctors/statics on a value class stay ordinary (no receiver).
    let is_value_method = value_class && matches!(kind, Kind::Method { .. });
    let optional = match &kind {
        Kind::Ctor { optional, .. }
        | Kind::Method { optional, .. }
        | Kind::Static { optional, .. } => *optional,
        Kind::Getter { .. } | Kind::Setter { .. } => 0,
    };
    let throws = matches!(
        &kind,
        Kind::Ctor { throws: true, .. }
            | Kind::Method { throws: true, .. }
            | Kind::Static { throws: true, .. }
    );
    // A `returns = "Class"` names the class a `Handle` return carries, so the ts
    // return says `: Class` and the engine's return-class tracking classifies the
    // result (chained `.prop`/`.method()` resolve).
    let returns_class: Option<String> = match &kind {
        Kind::Method { returns, .. }
        | Kind::Static { returns, .. }
        | Kind::Getter { returns, .. } => returns.clone(),
        _ => None,
    };
    let (js_name, readonly, private) = match &kind {
        Kind::Ctor { .. } => ("new".to_string(), false, false),
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

    let p = params::build_params(sig, is_ctor, is_static, is_value_method)?;

    // Return marshalling (+ the F3 async re-wrap into a settled Promise).
    let ret = returns::build_return(sig, class, is_ctor)?;
    let (ret_abi, ret_ext_ty, ret_ts, wrap) = if is_async {
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
        is_static,
        is_async,
        p.value_recv_call.as_ref(),
        wrap.as_ref(),
        is_mut_recv,
        &ret_ext_ty,
    );

    let ext_params = &p.ext_params;
    let extern_fn = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn #extern_ident(#(#ext_params),*) -> #ret_ext_ty {
            #body
        }
    };

    let kind_tok = if is_ctor {
        quote!(::rts_engine::MemberKind::Constructor)
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
    let sig_args = if is_ctor || is_static {
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
        if !is_ctor && !is_static {
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
            name: #js_name.into(),
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
        })
    };

    Ok((extern_fn, member, extern_ident))
}
