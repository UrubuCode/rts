//! Parameter marshalling for `class::member::gen_member` — the loop that turns
//! a method's typed params (skipping ctor/static's absent receiver, and a
//! value-class receiver already consumed as the first typed param) into the
//! extern-C param list, the call-site args, the ABI arg vector, and the ts
//! param strings. Split out because it is one coherent pass over `sig.inputs`
//! with its own bit of state (`idx`, `recv_skipped`) — folding it back into
//! `gen_member` would bury that state machine inside the setup/return/body
//! bookkeeping that surrounds it.

use quote::{format_ident, quote};
use syn::{FnArg, Type};

use crate::types::{
    is_class_ref_param, is_class_value_param, is_handle_ty, is_poly_ty, is_self_handle_param,
    is_str_param, option_inner, scalar,
};

/// Everything the param loop produces: the extern-C param list, the call-site
/// arguments passed to the wrapped Rust fn, the `AbiType` vector for the `Sig`,
/// the ts param strings, and — for a value-class instance method — the
/// receiver's own ABI type + call expression (consumed separately from
/// `arg_abis`/`call_args`, since the receiver sits in ABI slot 0, not among the
/// JS-visible args).
pub(crate) struct ParamsInfo {
    pub(crate) ext_params: Vec<proc_macro2::TokenStream>,
    pub(crate) call_args: Vec<proc_macro2::TokenStream>,
    pub(crate) arg_abis: Vec<proc_macro2::TokenStream>,
    pub(crate) ts_params: Vec<String>,
    pub(crate) value_recv_abi: Option<proc_macro2::TokenStream>,
    pub(crate) value_recv_call: Option<proc_macro2::TokenStream>,
    /// Statements to run BEFORE the call, one per class-typed non-receiver
    /// parameter — clones the `T` out of its `Entry::Rtse` handle (same
    /// clone-out `with_rtse` uses for the receiver, see `body.rs`) into a local
    /// `__c{idx}` binding, `return`-ing the extern fn's default on a dead/
    /// wrong-class handle. `call_args` for that param then just references
    /// `__c{idx}` (by value, `&__c{idx}`, or `&mut __c{idx}`).
    pub(crate) setup: Vec<proc_macro2::TokenStream>,
    /// Statements to run AFTER the call, one per `&mut T` class-typed
    /// non-receiver param — writes the (possibly mutated) clone back into its
    /// handle via `with_rtse_mut`, mirroring the receiver's own mut write-back
    /// in `body.rs`. `&T`/by-value class params have no teardown.
    pub(crate) teardown: Vec<proc_macro2::TokenStream>,
    /// Count of trailing params declared `Option<T>` (the type-driven spelling
    /// of optionality — see `types::option_inner`). `gen_member` folds this into
    /// the same arity window `#[rtse::method(optional = N)]` populates via
    /// `optional.max(option_count)`, so either spelling (or neither) produces
    /// the identical `Sig`/`DefaultArg` shape.
    pub(crate) option_count: usize,
}

/// Build the extern-C params + call args + ABI/ts vectors for one member.
///
/// `is_value_method`: a VALUE-class instance method receives the primitive word
/// as its FIRST typed param (no `self`). It is marshalled to `__recv` of the
/// primitive repr here (not via `with_rtse`), and the arg loop then SKIPS that
/// param when walking the rest.
pub(crate) fn build_params(
    sig: &syn::Signature,
    is_ctor: bool,
    is_static: bool,
    is_value_method: bool,
) -> syn::Result<ParamsInfo> {
    let mut ext_params = Vec::new();
    let mut call_args = Vec::new();
    let mut arg_abis = Vec::new();
    let mut ts_params = Vec::new();
    let mut value_recv_abi: Option<proc_macro2::TokenStream> = None;
    let mut value_recv_call: Option<proc_macro2::TokenStream> = None;
    let mut setup: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut teardown: Vec<proc_macro2::TokenStream> = Vec::new();
    if !is_ctor && !is_static {
        if is_value_method {
            let recv_ty = sig
                .inputs
                .iter()
                .find_map(|a| if let FnArg::Typed(pt) = a { Some(&*pt.ty) } else { None })
                .ok_or_else(|| {
                    syn::Error::new_spanned(
                        &sig.ident,
                        "rtse value method: needs a receiver (the first param is the primitive value)",
                    )
                })?;
            let (abi, ext_ty, call) = if is_poly_ty(recv_ty) {
                // `Poly` receiver: the raw NaN-boxed PolyValue word, verbatim. The
                // body unboxes it itself (autoboxed primitive OR wrapper object) —
                // the UNIFORM primitive receiver (`String`/`Number`/`Boolean`:
                // `xval(word)` handles both the inline primitive and the
                // `Entry::Rtse` wrapper).
                (quote!(::rts_engine::AbiType::PolyValue), quote!(u64), quote!(__recv))
            } else if let Some((abi, ext_ty, _)) = scalar(recv_ty) {
                let is_bool = matches!(recv_ty, Type::Path(p) if p.path.is_ident("bool"));
                let call = if is_bool { quote!((__recv != 0)) } else { quote!(__recv) };
                (abi, ext_ty, call)
            } else if is_handle_ty(recv_ty).is_some() {
                (quote!(::rts_engine::AbiType::Handle), quote!(u64), quote!(__recv))
            } else {
                return Err(syn::Error::new_spanned(
                    recv_ty,
                    "rtse value receiver: must be `Poly` (raw word), f64/i64/i32/bool, or a Handle",
                ));
            };
            ext_params.push(quote!(__recv: #ext_ty));
            value_recv_abi = Some(abi);
            value_recv_call = Some(call);
        } else {
            ext_params.push(quote!(__recv: u64));
        }
    }
    let mut idx = 0usize;
    let mut recv_skipped = false;
    // Set once the first `Option<T>` param is seen; every SUBSEQUENT JS-visible
    // param must also be `Option<T>` — JS optional-tail semantics (mirrors
    // `function.rs::take_default`'s identical trailing check for `#[default(...)]`).
    let mut seen_optional = false;
    let mut option_count = 0usize;
    for a in &sig.inputs {
        let FnArg::Typed(pt) = a else { continue };
        // For a value method the first typed param IS the receiver (already
        // marshalled above) — not a JS arg.
        if is_value_method && !recv_skipped {
            recv_skipped = true;
            continue;
        }
        if is_self_handle_param(&pt.ty) {
            // The receiver's own handle — passed to the body as `__recv`, NOT a JS
            // arg (no ext param / ts / abi, idx unchanged, no optionality either).
            call_args.push(quote!(__recv));
            continue;
        }
        let (ty, is_optional): (&Type, bool) = match option_inner(&pt.ty) {
            Some(inner) => (inner, true),
            None => (&pt.ty, false),
        };
        if is_optional {
            seen_optional = true;
            option_count += 1;
        } else if seen_optional {
            return Err(syn::Error::new_spanned(
                &pt.ty,
                "rtse: a required parameter cannot follow an `Option<T>` one — JS optional \
                 params are trailing-only (make every param after the first `Option<T>` \
                 optional too)",
            ));
        }
        if is_str_param(ty) {
            // `&str` crosses as StrPtr = two slots (ptr:i64, len:i64); rebuild the
            // &str from the string-pool pointer the codegen passes.
            let pp = format_ident!("__a{}_ptr", idx);
            let pl = format_ident!("__a{}_len", idx);
            ext_params.push(quote!(#pp: i64));
            ext_params.push(quote!(#pl: i64));
            let raw = quote!({
                let __b = unsafe { ::core::slice::from_raw_parts(#pp as *const u8, #pl as usize) };
                ::core::str::from_utf8(__b).unwrap_or("")
            });
            // Same "absent = empty string" convention `DefaultArg::Undefined`
            // already injects at the StrPtr call site (registry_call.rs) — an
            // omitted `Option<&str>` reads as `None`, a caller-passed `""` reads
            // as `Some("")` too (documented limitation, not a new one).
            call_args.push(if is_optional {
                quote!({ let __s: &str = #raw; if __s.is_empty() { ::core::option::Option::None } else { ::core::option::Option::Some(__s) } })
            } else {
                raw
            });
            arg_abis.push(quote!(::rts_engine::AbiType::StrPtr));
            ts_params.push(format!("a{idx}: string"));
            idx += 1;
            continue;
        }
        if let Some(ts) = is_handle_ty(ty) {
            // F1: raw u64 handle passthrough — no marshalling, hand it straight to
            // the Rust body (whose param type is the `Handle`/`U64` alias = u64).
            let pid = format_ident!("__a{}", idx);
            ext_params.push(quote!(#pid: u64));
            // Absent = handle `0` (the same sentinel `DefaultArg::Undefined`
            // injects for a Handle slot — see registry_call.rs).
            call_args.push(if is_optional {
                quote!(if #pid == 0 { ::core::option::Option::None } else { ::core::option::Option::Some(#pid) })
            } else {
                quote!(#pid)
            });
            arg_abis.push(quote!(::rts_engine::AbiType::Handle));
            ts_params.push(format!("a{idx}: {ts}"));
            idx += 1;
            continue;
        }
        if is_poly_ty(ty) {
            if is_optional {
                // `Poly` already carries JS `undefined` as an ordinary tagged
                // word — the body reads it directly (no Rust-side sentinel to
                // decode), so `Option<Poly>` has no extra meaning to express.
                return Err(syn::Error::new_spanned(
                    &pt.ty,
                    "rtse: `Option<Poly>` is redundant — `Poly` already represents JS \
                     `undefined` natively; use `optional = N` (or a bare `Poly` param) instead",
                ));
            }
            // `Poly` (`any`): the raw NaN-boxed PolyValue word, ABI-unchanged.
            let pid = format_ident!("__a{}", idx);
            ext_params.push(quote!(#pid: u64));
            call_args.push(quote!(#pid));
            arg_abis.push(quote!(::rts_engine::AbiType::PolyValue));
            ts_params.push(format!("a{idx}: any"));
            idx += 1;
            continue;
        }
        // A class-typed param (`&T` / `&mut T` / `T`, `T` another `#[rtse::class]`
        // struct): crosses as a `Handle`, clone-out via `with_rtse` before the
        // call (see `setup` doc). `&mut T` gets its OWN clone — never the same
        // memory as the receiver's or another `&mut T` param's clone, even when
        // both handles are identical, so there is no Rust-level aliasing. The
        // tradeoff (documented, not silent): if the SAME handle is passed twice
        // as `&mut` (or once as `&mut` and once as the receiver), each clone is
        // independent and only the mutation on the param that is written back
        // LAST survives — this macro does not (and cannot, without re-locking
        // the shard) reconcile concurrent mutable views of one handle.
        if let Some((cls_ty, is_mut)) = is_class_ref_param(ty) {
            let pid = format_ident!("__a{}", idx);
            let cvar = format_ident!("__c{}", idx);
            ext_params.push(quote!(#pid: u64));
            let mutkw = if is_mut { quote!(mut) } else { quote!() };
            if is_optional {
                setup.push(quote!(
                    let #mutkw #cvar: ::core::option::Option<#cls_ty> = if #pid == 0 {
                        ::core::option::Option::None
                    } else {
                        match ::rts_engine::heap::handles::with_rtse::<#cls_ty, _>(#pid, |__s| __s.cloned()) {
                            ::core::option::Option::Some(__v) => ::core::option::Option::Some(__v),
                            ::core::option::Option::None => return ::core::default::Default::default(),
                        }
                    };
                ));
                call_args.push(if is_mut { quote!(#cvar.as_mut()) } else { quote!(#cvar.as_ref()) });
                if is_mut {
                    teardown.push(quote!(
                        if let ::core::option::Option::Some(__v) = #cvar {
                            ::rts_engine::heap::handles::with_rtse_mut::<#cls_ty, _>(#pid, |__slot| {
                                if let ::core::option::Option::Some(__slot) = __slot { *__slot = __v; }
                            });
                        }
                    ));
                }
            } else {
                setup.push(quote!(
                    let #mutkw #cvar: #cls_ty =
                        match ::rts_engine::heap::handles::with_rtse::<#cls_ty, _>(#pid, |__s| __s.cloned()) {
                            ::core::option::Option::Some(__v) => __v,
                            ::core::option::Option::None => return ::core::default::Default::default(),
                        };
                ));
                call_args.push(if is_mut { quote!(&mut #cvar) } else { quote!(&#cvar) });
                if is_mut {
                    teardown.push(quote!(
                        ::rts_engine::heap::handles::with_rtse_mut::<#cls_ty, _>(#pid, |__slot| {
                            if let ::core::option::Option::Some(__slot) = __slot { *__slot = #cvar; }
                        });
                    ));
                }
            }
            arg_abis.push(quote!(::rts_engine::AbiType::Handle));
            ts_params.push(format!("a{idx}: object"));
            idx += 1;
            continue;
        }
        if !is_optional {
            if let Some(cls_ty) = is_class_value_param(ty) {
                let pid = format_ident!("__a{}", idx);
                let cvar = format_ident!("__c{}", idx);
                ext_params.push(quote!(#pid: u64));
                setup.push(quote!(
                    let #cvar: #cls_ty =
                        match ::rts_engine::heap::handles::with_rtse::<#cls_ty, _>(#pid, |__s| __s.cloned()) {
                            ::core::option::Option::Some(__v) => __v,
                            ::core::option::Option::None => return ::core::default::Default::default(),
                        };
                ));
                call_args.push(quote!(#cvar));
                arg_abis.push(quote!(::rts_engine::AbiType::Handle));
                ts_params.push(format!("a{idx}: object"));
                idx += 1;
                continue;
            }
        } else if is_class_value_param(ty).is_some() {
            return Err(syn::Error::new_spanned(
                &pt.ty,
                "rtse: `Option<T>` by-value class param is not supported — use `Option<&T>`",
            ));
        }
        let Some((abi, ext_ty, ts)) = scalar(ty) else {
            return Err(syn::Error::new_spanned(
                ty,
                "rtse: param must be f64/i64/i32/bool/&str/Handle/U64/&T (class)",
            ));
        };
        let is_f64 = matches!(ty, Type::Path(p) if p.path.is_ident("f64"));
        if is_optional && !is_f64 {
            // No established "absent" sentinel for i64/i32/bool (unlike f64's
            // NaN) — `optional = N` still covers these; the body reads the
            // raw padded word itself, same as today.
            return Err(syn::Error::new_spanned(
                &pt.ty,
                "rtse: `Option<T>` is only supported for f64/&str/Handle/U64 params (no \
                 absent-sentinel convention for i64/i32/bool) — use `optional = N` instead",
            ));
        }
        let pid = format_ident!("__a{}", idx);
        ext_params.push(quote!(#pid: #ext_ty));
        let is_bool = matches!(ty, Type::Path(p) if p.path.is_ident("bool"));
        let raw = if is_bool { quote!((#pid != 0)) } else { quote!(#pid) };
        call_args.push(if is_optional {
            // Absent = NaN (the sentinel `DefaultArg::Undefined` injects for an
            // F64 slot via `ToNumber(undefined)`).
            quote!(if #pid.is_nan() { ::core::option::Option::None } else { ::core::option::Option::Some(#pid) })
        } else {
            raw
        });
        arg_abis.push(abi);
        ts_params.push(format!("a{idx}: {ts}"));
        idx += 1;
    }

    Ok(ParamsInfo {
        ext_params,
        call_args,
        arg_abis,
        ts_params,
        value_recv_abi,
        value_recv_call,
        setup,
        teardown,
        option_count,
    })
}
