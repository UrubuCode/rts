//! Body construction for `class::member::gen_member` — the extern-C fn body
//! that actually calls the wrapped Rust method: async-signature validation,
//! `&mut self` vs `&self` dispatch (clone-out/write-back around `with_rtse`),
//! and the three receiver shapes (value-class primitive, ctor/static with no
//! receiver, ordinary `Entry::Rtse` instance). Kept apart from `returns.rs`
//! (which only decides how a raw Rust value becomes an ABI word) and from
//! `params.rs` (which only decides how ABI words become Rust call args) — this
//! is the third leg: how the call itself is wired to the HandleTable.

use quote::quote;
use syn::{FnArg, Type};

/// `sig.asyncness` vs the `#[rtse::asynch]` marker must agree — check + compute
/// `is_mut_recv` in the same order `gen_member` originally did (the mutability
/// scan runs before either error check, though neither depends on the other).
pub(crate) fn check_async_signature(sig: &syn::Signature, is_async: bool) -> syn::Result<bool> {
    // A `&mut self` (or `self: &mut X`) method needs mutable access to the boxed
    // struct → `with_rtse_mut`; an `&self` method → `with_rtse`.
    let is_mut_recv = sig.inputs.iter().any(|a| match a {
        FnArg::Receiver(r) => {
            r.mutability.is_some()
                || matches!(&*r.ty, Type::Reference(rf) if rf.mutability.is_some())
        }
        _ => false,
    });
    // A real Rust `async fn` body yields a `Future`; the `#[rtse::asynch]` bridge
    // drives it to a value with `rts_engine::block_on` (interim engine async is
    // synchronous). This keeps the Rust side a genuine `async fn` — `.await` works
    // in the body and the method is usable as `.await` from other Rust — while the
    // JS side still gets a settled Promise (the F3 return wrap above).
    if is_async && sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            "rtse asynch: mark a real `async fn` (write `async fn ...`); the body's \
             Future is driven by `rts_engine::block_on` and wrapped in a Promise",
        ));
    }
    if sig.asyncness.is_some() && !is_async {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            "rtse: an `async fn` member must be marked `#[rtse::asynch]`",
        ));
    }
    Ok(is_mut_recv)
}

/// Build the extern-C fn body for one member.
///
/// `wrap` turns the raw Rust return expression into the ABI-return-typed
/// expression (see `returns::build_return`); applied INSIDE the Some-arm for an
/// instance method, so the None-arm (dead/invalid handle) returns
/// `Default::default()` of the EXTERN return type — NOT `Default` of the raw
/// method return (which for `-> Self`/`-> String` need not be `Default`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_body(
    self_ty: &Type,
    rust_name: &syn::Ident,
    call_args: &[proc_macro2::TokenStream],
    is_value_method: bool,
    is_ctor: bool,
    is_static: bool,
    is_async: bool,
    value_recv_call: Option<&proc_macro2::TokenStream>,
    wrap: &dyn Fn(proc_macro2::TokenStream) -> proc_macro2::TokenStream,
    is_mut_recv: bool,
    ret_ext_ty: &proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
    let invoke = |recv: proc_macro2::TokenStream| {
        if is_async {
            quote!(::rts_engine::block_on(#recv.#rust_name(#(#call_args),*)))
        } else {
            quote!(#recv.#rust_name(#(#call_args),*))
        }
    };
    // Instance dispatch CLONES the receiver OUT of the HandleTable (which drops the
    // per-shard `Mutex` guard) BEFORE running the body, then — for `&mut self` —
    // writes the mutated clone back. This is required for correctness: `with_rtse`
    // holds a NON-reentrant per-shard lock, so a body that touches ANOTHER handle
    // (`with_entry`/`alloc_entry`, e.g. `WeakRef::deref`, `TextDecoder::decode`,
    // allocating a String return) would SELF-DEADLOCK whenever that handle hashes
    // to the receiver's shard (1-in-32). Cloning out first removes the held lock
    // across the body. The struct must therefore be `Clone` (a clear compile error
    // otherwise). Cost: one clone per call — acceptable, this is the (non-hot)
    // library surface, never the numeric fast path.
    let inner = invoke(quote!(__s));
    if is_value_method {
        // Value receiver: call the assoc fn directly with the marshalled primitive
        // receiver + args — no `Entry::Rtse` lookup, no `with_rtse` lock.
        let ty = self_ty;
        let recv = value_recv_call.expect("value method has a receiver");
        let raw = if is_async {
            quote!(::rts_engine::block_on(<#ty>::#rust_name(#recv #(, #call_args)*)))
        } else {
            quote!(<#ty>::#rust_name(#recv #(, #call_args)*))
        };
        wrap(raw)
    } else if is_ctor || is_static {
        let ty = self_ty;
        let raw = if is_async {
            quote!(::rts_engine::block_on(<#ty>::#rust_name(#(#call_args),*)))
        } else {
            quote!(<#ty>::#rust_name(#(#call_args),*))
        };
        wrap(raw)
    } else {
        let some = wrap(inner);
        if is_mut_recv {
            quote!({
                let mut __c: ::core::option::Option<#self_ty> =
                    ::rts_engine::heap::handles::with_rtse::<#self_ty, _>(__recv, |__s| __s.cloned());
                let __r: #ret_ext_ty = match __c.as_mut() {
                    ::core::option::Option::Some(__s) => #some,
                    ::core::option::Option::None => ::core::default::Default::default(),
                };
                if let ::core::option::Option::Some(__c) = __c {
                    ::rts_engine::heap::handles::with_rtse_mut::<#self_ty, _>(__recv, |__slot| {
                        if let ::core::option::Option::Some(__slot) = __slot {
                            *__slot = __c;
                        }
                    });
                }
                __r
            })
        } else {
            quote!({
                let __c: ::core::option::Option<#self_ty> =
                    ::rts_engine::heap::handles::with_rtse::<#self_ty, _>(__recv, |__s| __s.cloned());
                let __r: #ret_ext_ty = match __c.as_ref() {
                    ::core::option::Option::Some(__s) => #some,
                    ::core::option::Option::None => ::core::default::Default::default(),
                };
                __r
            })
        }
    }
}
