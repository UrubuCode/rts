//! `#[rtse::type]` — a plain Rust struct that marshals to a PLAIN JS OBJECT (a
//! record) when returned to JS, as opposed to `#[rtse::class]` (which gives JS
//! class identity: `new X()`, `instanceof`, methods, `Entry::Rtse`). No
//! Registry class is registered and no ctor exists; `typeof` the result is
//! `"object"` and its fields are OWN enumerable properties — the runtime layout
//! is IDENTICAL to an object literal the lowering itself builds
//! (`rts-codegen-new/src/front/run/obj.rs`): slot 0 = the interned shape id
//! (boxed INT32), then one PolyValue-word slot per field IN SHAPE-KEY ORDER.
//!
//! ```ignore
//! #[rtse::type]
//! struct Stats { size: f64, mtime: f64, is_file: bool }
//! ```
//!
//! The shape is interned ONCE per process (a `OnceLock` local to the generated
//! `__rtse_into_handle`), not per call — `intern_global_shape` is idempotent
//! but still hashes the key list under a lock, which a hot return path (e.g.
//! `fs.stat` in a loop) should not pay on every call.
//!
//! Field names map through `to_camel` (`is_file` → `isFile`); a field written
//! `#[rtse::variable(name = "...")]` (the same attribute + arg spelling
//! `#[rtse::class]`'s method `name = "..."` override already uses) overrides it.
//!
//! Supported field types, in priority order: `f64`, `i64`, `bool`,
//! `String`/`&str`, `Handle`/`U64`, `Poly`, `Option<T>` (`None` → JS `null` —
//! the same convention `-> Option<String>` already uses), `Vec<T>` (→ a JS
//! array), and a nested `#[rtse::type]`/`#[rtse::class]` field (→ a nested
//! object/instance, via the same `RtseReturn` trait).
//!
//! Implements the SAME `RtseReturn` trait `#[rtse::class]` implements (see
//! `class::mod`'s `gen_impl`) so the return-path codegen in
//! `class::member::returns` / `function` needs no idea which kind of type it is
//! marshalling. `RTSE_CLASS` is ALSO emitted, but EMPTY (`""`) — the same
//! "class-less" sentinel `alloc_rtse`'s own `class` param already uses — so the
//! shared `ret_class` computation at each return call site resolves to `None`
//! (a record has no class identity to track for chained `.method()` calls).

use proc_macro::TokenStream;
use quote::quote;
use syn::{Fields, ItemStruct, Type};

use crate::naming::to_camel;
use crate::types::{is_handle_ty, is_other_class_ret, is_poly_ty, is_string_ret, option_inner, vec_inner};

pub(crate) fn expand(_args: TokenStream, item: TokenStream) -> TokenStream {
    let mut s = match syn::parse::<ItemStruct>(item) {
        Ok(s) => s,
        Err(e) => return e.to_compile_error().into(),
    };
    let self_ty = s.ident.clone();
    let Fields::Named(named) = &mut s.fields else {
        return syn::Error::new_spanned(&s, "rtse::type: struct must have named fields")
            .to_compile_error()
            .into();
    };

    let mut keys: Vec<String> = Vec::new();
    let mut pushes: Vec<proc_macro2::TokenStream> = Vec::new();
    for f in named.named.iter_mut() {
        let js_name = take_name_override(&mut f.attrs)
            .unwrap_or_else(|| to_camel(&f.ident.as_ref().unwrap().to_string()));
        let fname = f.ident.clone().unwrap();
        match field_word(&f.ty, quote!(self.#fname)) {
            Ok(tok) => {
                keys.push(js_name);
                pushes.push(tok);
            }
            Err(e) => return e.to_compile_error().into(),
        }
    }
    let n = pushes.len();

    quote! {
        #s
        impl #self_ty {
            /// A `#[rtse::type]` record carries NO class identity (see the
            /// `RtseReturn`/`ret_class` doc in `class::member::returns`) — this is
            /// deliberately EMPTY, the shared sentinel every return call site checks.
            pub const RTSE_CLASS: &'static str = "";
        }
        impl ::rts_engine::heap::handles::RtseReturn for #self_ty {
            fn __rtse_into_handle(self) -> u64 {
                static __RTSE_SHAPE: ::std::sync::OnceLock<::rts_engine::heap::shapes::GlobalShapeId> =
                    ::std::sync::OnceLock::new();
                let __shape = *__RTSE_SHAPE.get_or_init(|| {
                    ::rts_engine::heap::shapes::intern_global_shape(&[
                        #(#keys.to_string()),*
                    ])
                });
                let __values: [i64; #n] = [ #(#pushes),* ];
                ::rts_engine::heap::shapes::alloc_shaped_object_with_id(__shape, &__values)
            }
        }
    }
    .into()
}

/// Strip + read a field's `#[rtse::variable(name = "...")]` override (if any).
fn take_name_override(attrs: &mut Vec<syn::Attribute>) -> Option<String> {
    let mut found = None;
    attrs.retain(|a| {
        let segs = &a.path().segments;
        if segs.len() == 2 && segs[0].ident == "rtse" && segs[1].ident == "variable" {
            let _ = a.parse_nested_meta(|m| {
                if m.path.is_ident("name") {
                    let v = m.value()?;
                    let s: syn::LitStr = v.parse()?;
                    found = Some(s.value());
                }
                Ok(())
            });
            return false;
        }
        true
    });
    found
}

fn ident_is(ty: &Type, name: &str) -> bool {
    matches!(ty, Type::Path(p) if p.path.is_ident(name))
}

/// Build the i64 PolyValue-word expression for one field, given `access` (the
/// Rust expression reading the field/element by value: `self.foo`, `__v`, `__e`).
/// Recurses into `Option<T>`/`Vec<T>` to marshal their inner `T` the same way.
fn field_word(ty: &Type, access: proc_macro2::TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    if ident_is(ty, "f64") {
        return Ok(quote!(::rts_engine::heap::shapes::f64_word(#access) as i64));
    }
    if ident_is(ty, "i64") || ident_is(ty, "i32") {
        return Ok(quote!(::rts_engine::heap::shapes::int_word((#access) as i64) as i64));
    }
    if ident_is(ty, "bool") {
        return Ok(quote!(::rts_engine::heap::shapes::bool_word(#access) as i64));
    }
    if is_string_ret(ty) {
        return Ok(quote!(::rts_engine::heap::shapes::string_word((#access).as_bytes()) as i64));
    }
    if let Some(tag) = is_handle_ty(ty) {
        return Ok(if tag == "object" {
            // `Handle`: a runtime handle — box it as the OBJECT/STR/FUNCTION word
            // matching its live entry kind.
            quote!(::rts_engine::heap::shapes::handle_word_auto(#access) as i64)
        } else {
            // `U64`: a raw number, not a heap reference.
            quote!(::rts_engine::heap::shapes::int_word((#access) as i64) as i64)
        });
    }
    if is_poly_ty(ty) {
        // Already a raw PolyValue word — pass through untouched.
        return Ok(quote!((#access) as i64));
    }
    if let Some(inner) = option_inner(ty) {
        let inner_word = field_word(inner, quote!(__v))?;
        // `None` → JS `null` (matches `-> Option<String>`'s own convention).
        return Ok(quote!(match #access {
            ::core::option::Option::Some(__v) => #inner_word,
            ::core::option::Option::None => ::rts_engine::heap::poly::POLY_NULL as i64,
        }));
    }
    if let Some(inner) = vec_inner(ty) {
        let elem_word = field_word(inner, quote!(__e))?;
        return Ok(quote!({
            let __vals: ::std::vec::Vec<i64> = (#access).into_iter().map(|__e| #elem_word).collect();
            let __h = ::rts_engine::heap::handles::alloc_entry(
                ::rts_engine::heap::handles::Entry::Vec(::std::boxed::Box::new(__vals)),
            );
            ::rts_engine::heap::shapes::handle_word_auto(__h) as i64
        }));
    }
    if is_other_class_ret(ty).is_some() {
        // A nested `#[rtse::type]`/`#[rtse::class]` field: alloc via the same
        // uniform `RtseReturn` trait, then box the resulting handle as an object
        // word for this slot.
        return Ok(quote!({
            let __h = ::rts_engine::heap::handles::RtseReturn::__rtse_into_handle(#access);
            ::rts_engine::heap::shapes::handle_word_auto(__h) as i64
        }));
    }
    Err(syn::Error::new_spanned(
        ty,
        "rtse::type: unsupported field type (supported: f64/i64/i32/bool/String/&str/Handle/U64/Poly, \
         Option<T>, Vec<T>, or a nested #[rtse::type]/#[rtse::class] type)",
    ))
}
