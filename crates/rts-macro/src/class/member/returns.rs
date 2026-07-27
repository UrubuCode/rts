//! Return-value marshalling for `class::member::gen_member` — the big match on
//! `ret_ty(sig)` that decides the ABI return type, the extern-C Rust return
//! type, the ts return type, and the wrapping expression (raw Rust return value
//! → ABI word). Plus the `#[rtse::asynch]` return re-wrap (settled Promise).
//! Split out because this single match is the widest part of `gen_member` (one
//! arm per return shape: void, Handle passthrough, Poly, `Self`,
//! `Option<Self>`, `Option<String>`, `String`/`&str`, `Vec<T>`, plain scalar) —
//! keeping it in its own file lets that shape list grow without pushing the
//! rest of member expansion around it.

use quote::quote;
use syn::Type;

use crate::types::{
    is_handle_ty, is_option_self_ret, is_option_string_ret, is_poly_ty, is_self_ret, is_string_ret,
    ret_ty, scalar, str_tuple_arity, vec_inner,
};

/// `(ret_abi, ret_ext_ty, ret_ts, wrap)` — `wrap` turns the raw Rust return
/// expression into the ABI-return-typed expression (alloc into the string
/// pool, box into an `Entry::Rtse`, pass a handle through untouched, …).
pub(crate) type RetInfo = (
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    String,
    Box<dyn Fn(proc_macro2::TokenStream) -> proc_macro2::TokenStream>,
);

/// Build the non-async return marshalling for one member. `class` is the JS
/// class name (needed by the `Self`/`Option<Self>`/ctor arms, which allocate a
/// classed `Entry::Rtse` carrying it for `instanceof`/return-class tracking).
pub(crate) fn build_return(sig: &syn::Signature, class: &str, is_ctor: bool) -> syn::Result<RetInfo> {
    if is_ctor {
        // The ctor allocates the struct as a classed `Entry::Rtse` (the class name
        // travels so `instanceof` can consult the hierarchy). A FALLIBLE ctor
        // (`-> Option<Self>`) allocs the `Some` and returns null (`0`) for `None`.
        let cls = class.to_string();
        let fallible = ret_ty(sig).is_some_and(is_option_self_ret);
        let wrap: Box<dyn Fn(proc_macro2::TokenStream) -> proc_macro2::TokenStream> = if fallible {
            Box::new(move |b| {
                quote!(match #b {
                    ::core::option::Option::Some(__v) =>
                        ::rts_engine::heap::handles::alloc_rtse(#cls, __v),
                    ::core::option::Option::None => 0u64,
                })
            })
        } else {
            Box::new(move |b| quote!(::rts_engine::heap::handles::alloc_rtse(#cls, #b)))
        };
        return Ok((
            quote!(::rts_engine::AbiType::Handle),
            quote!(u64),
            class.to_string(),
            wrap,
        ));
    }
    Ok(match ret_ty(sig) {
        None => (
            quote!(::rts_engine::AbiType::Void),
            quote!(()),
            "void".into(),
            Box::new(|b| quote!({ #b; })),
        ),
        Some(t) if is_handle_ty(t).is_some() => (
            // F1: return a raw u64 handle untouched.
            quote!(::rts_engine::AbiType::Handle),
            quote!(u64),
            is_handle_ty(t).unwrap().into(),
            Box::new(|b| quote!(#b)),
        ),
        Some(t) if is_poly_ty(t) => (
            // `Poly` (`any`): return the raw NaN-boxed word untouched.
            quote!(::rts_engine::AbiType::PolyValue),
            quote!(u64),
            "any".into(),
            Box::new(|b| quote!(#b)),
        ),
        Some(t) if is_self_ret(t) => {
            // `-> Self`: alloc the fresh instance as a classed `Entry::Rtse`,
            // exactly like the ctor. ts = the class name (return-class tracked).
            let cls = class.to_string();
            (
                quote!(::rts_engine::AbiType::Handle),
                quote!(u64),
                class.to_string(),
                Box::new(move |b| quote!(::rts_engine::heap::handles::alloc_rtse(#cls, #b))),
            )
        }
        Some(t) if is_option_self_ret(t) => {
            // `-> Option<Self>`: a fallible factory — alloc the `Some`, return
            // null (`0`) for `None`. Same class-tracked handle as `-> Self`.
            let cls = class.to_string();
            (
                quote!(::rts_engine::AbiType::Handle),
                quote!(u64),
                class.to_string(),
                Box::new(move |b| {
                    quote!(match #b {
                        ::core::option::Option::Some(__v) =>
                            ::rts_engine::heap::handles::alloc_rtse(#cls, __v),
                        ::core::option::Option::None => 0u64,
                    })
                }),
            )
        }
        Some(t) if is_option_string_ret(t) => (
            // `-> Option<String>`: a nullable string — `Some(s)` allocs a string
            // handle, `None` returns `0` (JS `null`). ts `string | null` so the
            // engine's nullable-string rebox maps the `0` handle to `null`.
            quote!(::rts_engine::AbiType::Handle),
            quote!(u64),
            "string | null".into(),
            Box::new(|b| {
                quote!(match #b {
                    ::core::option::Option::Some(__s) =>
                        ::rts_engine::heap::handles::alloc_entry(
                            ::rts_engine::heap::handles::Entry::String(__s.into_bytes())
                        ),
                    ::core::option::Option::None => 0u64,
                })
            }),
        ),
        Some(t) if is_string_ret(t) => (
            quote!(::rts_engine::AbiType::Handle),
            quote!(u64),
            "string".into(),
            // A String / &str return → allocate into the string pool, return
            // the handle. `.to_string()` covers both `String` and `&str`.
            Box::new(|b| {
                quote!(::rts_engine::heap::handles::alloc_entry(
                    ::rts_engine::heap::handles::Entry::String((#b).to_string().into_bytes())
                ))
            }),
        ),
        // F8: `Vec<String>` / `Vec<Handle>` → an `Entry::Vec` of element
        // handles, ts `string[]` / `object[]`. The engine sees the `[]` and
        // reboxes via `__rtsadp_box_handle_auto` (normalizes the raw element
        // handles to words).
        Some(t) if vec_inner(t).is_some() => {
            let inner = vec_inner(t).unwrap();
            // Per-element boxing: `String` → a string handle; `Handle` → the raw
            // handle; a String-tuple `(String, String)` → an inner `Entry::Vec`
            // of string handles (nested `[string,string][]`, e.g. `entries()`).
            let (elem_ts, push): (&str, proc_macro2::TokenStream) = if is_string_ret(inner) {
                (
                    "string",
                    quote!(::rts_engine::heap::handles::alloc_entry(
                        ::rts_engine::heap::handles::Entry::String(__e.into_bytes())
                    ) as i64),
                )
            } else if is_handle_ty(inner).is_some() {
                ("object", quote!(__e as i64))
            } else if let Some(n) = str_tuple_arity(inner) {
                let binds: Vec<_> = (0..n).map(|i| quote::format_ident!("__e{}", i)).collect();
                (
                    "string[]",
                    quote!({
                        let ( #(#binds),* ) = __e;
                        let __inner: ::std::vec::Vec<i64> = ::std::vec![
                            #( ::rts_engine::heap::handles::alloc_entry(
                                ::rts_engine::heap::handles::Entry::String(#binds.into_bytes())
                            ) as i64 ),*
                        ];
                        ::rts_engine::heap::handles::alloc_entry(
                            ::rts_engine::heap::handles::Entry::Vec(::std::boxed::Box::new(__inner))
                        ) as i64
                    }),
                )
            } else {
                return Err(syn::Error::new_spanned(
                    inner,
                    "rtse: Vec<T> element must be String, Handle, or a tuple of String",
                ));
            };
            (
                quote!(::rts_engine::AbiType::Handle),
                quote!(u64),
                format!("{elem_ts}[]"),
                Box::new(move |b| {
                    quote!({
                        let __v: ::std::vec::Vec<i64> =
                            (#b).into_iter().map(|__e| #push).collect();
                        ::rts_engine::heap::handles::alloc_entry(
                            ::rts_engine::heap::handles::Entry::Vec(::std::boxed::Box::new(__v))
                        )
                    })
                }),
            )
        }
        Some(t) => {
            let Some((abi, ext_ty, ts)) = scalar(t) else {
                return Err(syn::Error::new_spanned(
                    t,
                    "rtse G1: return must be f64/i64/i32/bool/String/&str or ()",
                ));
            };
            let is_bool = matches!(t, Type::Path(p) if p.path.is_ident("bool"));
            (
                abi,
                ext_ty,
                ts.into(),
                if is_bool {
                    Box::new(|b| quote!((#b) as i64))
                } else {
                    Box::new(|b| quote!(#b))
                },
            )
        }
    })
}

/// F3 `#[rtse::asynch]`: the body runs interim-synchronously (the engine's
/// current async model), then its return is wrapped in a settled Promise —
/// fulfilled with the value, or rejected if the body left a pending JS error
/// in the thread-local error slot. The base return must already be a heap
/// handle (String/Handle) so `__rtsadp_box_handle_auto` can box it to a word.
/// Settlers/box/errslot are adapter-layer symbols ABOVE this crate — reached
/// via a local `extern "C"` forward-decl (layering-safe: one linked binary).
pub(crate) fn wrap_async(sig: &syn::Signature, ret: RetInfo) -> syn::Result<RetInfo> {
    // Only the ts return + wrap of the non-async return carry forward (as the
    // Promise's inner shape); the non-async ABI/ext-ty are superseded below by
    // the Promise's own Handle/u64 shape.
    let (_ret_abi, _ret_ext_ty, ret_ts, wrap) = ret;
    let ok = match ret_ty(sig) {
        Some(t) => is_string_ret(t) || is_handle_ty(t).is_some(),
        None => false,
    };
    if !ok {
        return Err(syn::Error::new_spanned(
            &sig.output,
            "rtse asynch: return must be String or Handle (wrapped in a Promise)",
        ));
    }
    let base = wrap;
    let inner_ts = ret_ts;
    let awrap: Box<dyn Fn(proc_macro2::TokenStream) -> proc_macro2::TokenStream> = Box::new(move |call| {
        let inner = base(call);
        quote!({
            unsafe extern "C" {
                fn __rtsadp_box_handle_auto(h: u64) -> u64;
                fn __rtsadp_promise_resolve_w(w: u64) -> u64;
                fn __rtsadp_err_pending() -> i64;
                fn __rtsadp_err_take() -> u64;
                fn __RTS_FN_NS_PROMISE_NEW_REJECTED(e: i64) -> u64;
            }
            let __h: u64 = #inner;
            unsafe {
                if __rtsadp_err_pending() != 0 {
                    let __e = __rtsadp_err_take();
                    __RTS_FN_NS_PROMISE_NEW_REJECTED(__e as i64)
                } else {
                    __rtsadp_promise_resolve_w(__rtsadp_box_handle_auto(__h))
                }
            }
        })
    });
    Ok((
        quote!(::rts_engine::AbiType::Handle),
        quote!(u64),
        format!("Promise<{inner_ts}>"),
        awrap,
    ))
}
