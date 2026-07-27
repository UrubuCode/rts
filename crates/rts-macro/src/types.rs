//! Rust-type → ABI-type mapping.
//!
//! Every predicate/mapping here answers ONE question: "given this `syn::Type`
//! from a `#[rtse::*]`-annotated signature, what `AbiType` word crosses the
//! extern-C boundary, and what does it look like on both sides (Rust extern
//! param type, ts type-name)?" Kept separate from `class::member` (which only
//! orchestrates: loop over params, call these, assemble the `Sig`) so the type
//! vocabulary can grow (new marshalled shapes) without touching the expansion
//! logic, and so `#[rtse::abi]` (in `abi/`) can reuse the shared predicates
//! (`is_str_param`, …) without depending on the class-expansion module.
//! `#[rtse::abi]`'s own single-slot table lives in `abi/params.rs`: an ABI
//! symbol is a raw intrinsic-shaped fn with no receiver and no ts surface, so
//! it needs the ABI word only — not the extern-repr + ts triple `scalar()`
//! returns for a class member.

use syn::{ReturnType, Type};

/// AbiType token + extern Rust type + ts type-name for a supported scalar.
pub(crate) fn scalar(ty: &Type) -> Option<(proc_macro2::TokenStream, proc_macro2::TokenStream, &'static str)> {
    let Type::Path(p) = ty else { return None };
    let id = p.path.segments.last()?.ident.to_string();
    Some(match id.as_str() {
        "f64" => (quote::quote!(::rts_engine::AbiType::F64), quote::quote!(f64), "number"),
        "i64" => (quote::quote!(::rts_engine::AbiType::I64), quote::quote!(i64), "number"),
        "i32" => (quote::quote!(::rts_engine::AbiType::I32), quote::quote!(i32), "number"),
        "bool" => (quote::quote!(::rts_engine::AbiType::Bool), quote::quote!(i64), "boolean"),
        _ => return None,
    })
}

/// Is this a raw-handle passthrough type (`Handle`/`U64` alias = u64)? Returns the
/// TS type-name (`object` for a runtime handle, `number` for a raw U64). This is F1
/// — a u64 crosses the ABI untouched (no marshalling), letting a class store/return
/// other heap values by handle (weak refs, collections, rich objects).
pub(crate) fn is_handle_ty(ty: &Type) -> Option<&'static str> {
    let Type::Path(p) = ty else { return None };
    match p.path.segments.last()?.ident.to_string().as_str() {
        "Handle" => Some("object"),
        "U64" => Some("number"),
        _ => None,
    }
}

/// F8: if `ty` is `Vec<T>`, return its element type `T`. A `Vec<String>` /
/// `Vec<Handle>` return marshals to an `Entry::Vec` of element handles + a ts
/// `T[]` return, which the engine boxes via `__rtsadp_box_handle_auto` (the
/// `ret_is_array_handle` path — tag-dispatched, null/string/object correct).
pub(crate) fn vec_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return None;
    };
    match a.args.first()? {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    }
}

/// F8-nested: `Some(n)` if `ty` is a tuple of `n` `String` fields (e.g.
/// `(String, String)` for `entries()` → `[string,string][]`). Each element
/// marshals to an inner `Entry::Vec` of `n` string handles.
pub(crate) fn str_tuple_arity(ty: &Type) -> Option<usize> {
    let Type::Tuple(t) = ty else { return None };
    if t.elems.is_empty() || !t.elems.iter().all(is_string_ret) {
        return None;
    }
    Some(t.elems.len())
}

/// Is this a `SelfHandle` param? The macro fills it with the receiver's own
/// handle (`__recv`) and drops it from the JS signature — for a body that needs
/// its own handle (return a fresh instance sharing state, dispatch stamping
/// `ev.target`, hand a child its parent handle).
pub(crate) fn is_self_handle_param(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "SelfHandle"))
}

/// Is the return type `Self` (a method returning a fresh instance of its OWN
/// class, e.g. `Blob.slice() -> Blob`)? The macro allocs it as a classed
/// `Entry::Rtse` (like a ctor) + ts = the class name (return-class tracked).
pub(crate) fn is_self_ret(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.is_ident("Self"))
}

/// Is this return type `Option<Self>` — a FALLIBLE ctor/factory that yields a
/// NULL handle (`0`) on `None` (WHATWG `new URL(bad)` → null; the shape any
/// parse-or-null constructor needs). The macro allocs the `Some` payload via
/// `alloc_rtse` exactly like `-> Self`, and returns `0u64` for `None`.
pub(crate) fn is_option_self_ret(ty: &Type) -> bool {
    let Type::Path(p) = ty else { return false };
    let Some(seg) = p.path.segments.last() else {
        return false;
    };
    if seg.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return false;
    };
    matches!(a.args.first(), Some(syn::GenericArgument::Type(inner)) if is_self_ret(inner))
}

/// Is this return type `Option<String>` — a NULLABLE string getter/method that
/// yields JS `null` (a `0` handle) on `None` and a string handle on `Some`
/// (`URLSearchParams.get(missing)` → null, not `""`). ts return is
/// `string | null` so the engine's nullable-string rebox maps `0` → `null`.
pub(crate) fn is_option_string_ret(ty: &Type) -> bool {
    let Type::Path(p) = ty else { return false };
    let Some(seg) = p.path.segments.last() else {
        return false;
    };
    if seg.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return false;
    };
    matches!(a.args.first(), Some(syn::GenericArgument::Type(inner)) if is_string_ret(inner))
}

/// Is this a `Poly` (`any`) type? A NaN-boxed PolyValue word crosses the ABI
/// UNCHANGED (`AbiType::PolyValue`) — an arbitrary JS value (weakmap value, etc.).
pub(crate) fn is_poly_ty(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Poly"))
}

/// Is this param type `&str`?
pub(crate) fn is_str_param(ty: &Type) -> bool {
    matches!(ty, Type::Reference(r) if matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")))
}

/// Is this return type a `String` or `&str`? (both marshal to a string handle).
pub(crate) fn is_string_ret(ty: &Type) -> bool {
    match ty {
        Type::Path(p) => p.path.is_ident("String"),
        Type::Reference(r) => matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")),
        _ => false,
    }
}

/// Is this a plain single-segment `Type::Path` whose ident LOOKS like a class
/// name (starts uppercase, alphanumeric/`_`) and is none of the other
/// recognized special spellings (`Self`/`Handle`/`U64`/`Poly`/`String`/`Vec`/
/// `Option`)? Used to detect `-> SomeClass` / `Option<SomeClass>` returns where
/// `SomeClass` is expected to carry a `#[rtse::class("Name")]`-declared
/// `SomeClass::RTSE_CLASS` associated const (checked by `#path::RTSE_CLASS`,
/// verbatim, in the generated code — an unresolved path is a `rustc` error at
/// this site, not a silent runtime miss). Callers must place this check AFTER
/// the more specific predicates (`is_self_ret`, `scalar`, …) in their match, so
/// those take priority.
fn is_class_like_ident(ty: &Type) -> Option<&syn::Ident> {
    let Type::Path(p) = ty else { return None };
    if p.path.segments.len() != 1 {
        return None;
    }
    let seg = p.path.segments.last()?;
    let name = seg.ident.to_string();
    let first = name.chars().next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    if matches!(
        name.as_str(),
        "Self" | "Handle" | "U64" | "Poly" | "String" | "Vec" | "Option" | "Bool"
    ) {
        return None;
    }
    Some(&seg.ident)
}

/// `-> SomeClass` where `SomeClass` is not `Self` — a member returning an
/// INSTANCE OF ANOTHER registered class (`node:fs::createReadStream() ->
/// StreamReader`, `url.searchParams -> URLSearchParams`). Returns the type's
/// path so the caller can emit `#ty::RTSE_CLASS` verbatim.
pub(crate) fn is_other_class_ret(ty: &Type) -> Option<&Type> {
    is_class_like_ident(ty)?;
    Some(ty)
}

/// `-> Option<SomeClass>` (not `Option<Self>`) — a nullable other-class return.
/// Returns the OUTER `Option<...>` type's inner type argument (the class type)
/// so the caller can emit `#inner::RTSE_CLASS`.
pub(crate) fn is_option_other_class_ret(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return None;
    };
    let syn::GenericArgument::Type(inner) = a.args.first()? else {
        return None;
    };
    is_other_class_ret(inner)
}

pub(crate) fn ret_ty(sig: &syn::Signature) -> Option<&Type> {
    match &sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, t) => Some(t),
    }
}

#[cfg(test)]
mod class_ret_tests {
    use super::{is_option_other_class_ret, is_other_class_ret};
    use syn::parse_quote;

    #[test]
    fn plain_class_type_detected() {
        let ty: syn::Type = parse_quote!(StreamReader);
        assert!(is_other_class_ret(&ty).is_some());
    }

    #[test]
    fn option_class_type_detected_and_unwraps_inner() {
        let ty: syn::Type = parse_quote!(Option<StreamReader>);
        let inner = is_option_other_class_ret(&ty).expect("Option<Class> should match");
        let want: syn::Type = parse_quote!(StreamReader);
        assert_eq!(quote::quote!(#inner).to_string(), quote::quote!(#want).to_string());
    }

    #[test]
    fn special_spellings_are_not_class_like() {
        let tys: [syn::Type; 6] = [
            parse_quote!(Handle),
            parse_quote!(U64),
            parse_quote!(Poly),
            parse_quote!(String),
            parse_quote!(Bool),
            parse_quote!(Self),
        ];
        for ty in &tys {
            assert!(is_other_class_ret(ty).is_none(), "should not be class-like");
        }
    }

    #[test]
    fn lowercase_scalars_are_not_class_like() {
        let ty: syn::Type = parse_quote!(f64);
        assert!(is_other_class_ret(&ty).is_none());
    }

    #[test]
    fn vec_is_not_class_like() {
        let ty: syn::Type = parse_quote!(Vec<StreamReader>);
        assert!(is_other_class_ret(&ty).is_none());
    }
}
