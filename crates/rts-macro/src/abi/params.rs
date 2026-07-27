//! Rust signature → ABI slots for `#[rtse::abi]`.
//!
//! The mapping answers one question per parameter: which `AbiType` word does it
//! contribute, what does the `extern "C"` parameter list look like, and what
//! expression reconstructs the author's Rust value from those raw slots.
//!
//! The vocabulary is the one that already exists in `rts_engine::abi::ty` — the
//! author writes the alias whose NAME carries the intent (`Handle`, `Poly`,
//! `U64`, `Bool`) even though several share a Rust repr. This is the same
//! recognize-by-written-token rule `#[rtse::class]` uses, so the two macros read
//! one vocabulary instead of two.
//!
//! | written        | `AbiType`  | extern slots            | reconstruction        |
//! |----------------|------------|-------------------------|-----------------------|
//! | `u64` / `U64`  | `U64`      | `u64`                   | passthrough           |
//! | `Handle`       | `Handle`   | `u64`                   | passthrough           |
//! | `Poly`         | `PolyValue`| `u64`                   | passthrough           |
//! | `i64` / `I64`  | `I64`      | `i64`                   | passthrough           |
//! | `i32` / `I32`  | `I32`      | `i32`                   | passthrough           |
//! | `f64` / `F64`  | `F64`      | `f64`                   | passthrough           |
//! | `Bool`         | `Bool`     | `i64`                   | passthrough           |
//! | `bool`         | `Bool`     | `i64`                   | `!= 0` / `1i64`:`0i64`|
//! | `&str`         | `StrPtr`   | `i64` ptr + `i64` len   | `str_abi::from_abi`   |
//!
//! Two of those rows cannot be expressed by renaming the author's function in
//! place, because the extern parameter list stops matching the Rust one: `&str`
//! is ONE `AbiType` occupying TWO Rust parameters, and `bool` crosses as `i64`
//! (never `i8`). Those set [`AbiParam::needs_wrapper`], and `expand` emits a
//! thin `#[inline(always)]`-fed wrapper instead of the in-place rewrite. Every
//! other signature keeps the zero-trampoline path.

use quote::{format_ident, quote};
use syn::Type;

use crate::types::is_str_param;

/// One ABI parameter: the descriptor entry it contributes, the `extern "C"`
/// parameters it occupies, and the expression rebuilding the Rust value.
pub(crate) struct AbiParam {
    /// `AbiType` token — exactly ONE per ABI slot, `StrPtr` included.
    pub(crate) abi: proc_macro2::TokenStream,
    /// The `extern "C"` parameter declarations this contributes (one, or two
    /// for a `StrPtr` pair).
    pub(crate) ext: Vec<proc_macro2::TokenStream>,
    /// Expression handed to the author's function, in its declared Rust type.
    pub(crate) call: proc_macro2::TokenStream,
    /// True when the extern parameter list diverges from the Rust one.
    pub(crate) needs_wrapper: bool,
}

/// The ABI shape of a return type.
pub(crate) struct AbiRet {
    pub(crate) abi: proc_macro2::TokenStream,
    /// The `extern "C"` return type.
    pub(crate) ext_ty: proc_macro2::TokenStream,
    /// `Some(expr-template)` when the Rust value must be converted before it
    /// leaves the wrapper; the placeholder identifier is `__r`.
    pub(crate) convert: Option<proc_macro2::TokenStream>,
    pub(crate) needs_wrapper: bool,
}

fn abi_ident(v: &str) -> proc_macro2::TokenStream {
    let id = proc_macro2::Ident::new(v, proc_macro2::Span::call_site());
    quote!(::rts_engine::abi::AbiType::#id)
}

/// The `AbiType` variant name for a single-slot scalar/handle type, by written
/// token. `None` for anything with no single-slot spelling.
fn single_slot(ty: &Type) -> Option<&'static str> {
    let Type::Path(p) = ty else { return None };
    Some(match p.path.segments.last()?.ident.to_string().as_str() {
        "u64" | "U64" => "U64",
        "Handle" => "Handle",
        "Poly" => "PolyValue",
        "i64" | "I64" => "I64",
        "i32" | "I32" => "I32",
        "f64" | "F64" => "F64",
        "bool" | "Bool" => "Bool",
        _ => return None,
    })
}

/// Is the written token the Rust primitive `bool` (repr `i8`) rather than the
/// `Bool` ABI alias (repr `i64`)? Only the former needs a conversion.
fn is_rust_bool(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.is_ident("bool"))
}

/// Map one parameter. `idx` names the generated extern parameters.
pub(crate) fn param_of(ty: &Type, idx: usize) -> Option<AbiParam> {
    if is_str_param(ty) {
        // One `AbiType::StrPtr` occupying two Rust/Cranelift slots — the case
        // the old inference could not express at all.
        let pp = format_ident!("__a{}_ptr", idx);
        let pl = format_ident!("__a{}_len", idx);
        return Some(AbiParam {
            abi: abi_ident("StrPtr"),
            ext: vec![quote!(#pp: i64), quote!(#pl: i64)],
            call: quote!(
                unsafe { ::rts_engine::abi::str_abi::from_abi(#pp as *const u8, #pl) }
                    .unwrap_or("")
            ),
            needs_wrapper: true,
        });
    }
    let variant = single_slot(ty)?;
    let pid = format_ident!("__a{}", idx);
    if is_rust_bool(ty) {
        return Some(AbiParam {
            abi: abi_ident("Bool"),
            ext: vec![quote!(#pid: i64)],
            call: quote!((#pid != 0)),
            needs_wrapper: true,
        });
    }
    Some(AbiParam {
        abi: abi_ident(variant),
        ext: vec![quote!(#pid: #ty)],
        call: quote!(#pid),
        needs_wrapper: false,
    })
}

/// Map the return type.
pub(crate) fn ret_of(output: &syn::ReturnType) -> Option<AbiRet> {
    let syn::ReturnType::Type(_, ty) = output else {
        return Some(AbiRet {
            abi: abi_ident("Void"),
            ext_ty: quote!(()),
            convert: None,
            needs_wrapper: false,
        });
    };
    let variant = single_slot(ty)?;
    if is_rust_bool(ty) {
        return Some(AbiRet {
            abi: abi_ident("Bool"),
            ext_ty: quote!(i64),
            convert: Some(quote!(if __r { 1i64 } else { 0i64 })),
            needs_wrapper: true,
        });
    }
    Some(AbiRet {
        abi: abi_ident(variant),
        ext_ty: quote!(#ty),
        convert: None,
        needs_wrapper: false,
    })
}
