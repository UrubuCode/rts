//! `#[rtse::class(...)]` expansion — the entry point for both the struct form
//! (field accessors via `#[rtse::variable]`) and the impl form (ctor/methods
//! via `#[rtse::method]`/`#[rtse::ctor]`/…). This file owns argument parsing
//! and the two top-level generators (`gen_impl`/`gen_struct`); the per-member
//! expansion (the actually large part) lives in `member/` and the per-field
//! expansion in `field.rs` — kept apart because a struct with N fields and M
//! methods only ever exercises ONE of those two passes per macro invocation,
//! so there is no shared control flow worth co-locating them for.

mod field;
mod kind;
mod member;

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ImplItem, ItemImpl, Type};

use crate::abi::scope::{Scope, symbol_for};
use kind::{take_kind, take_variable};

/// The linker symbol of one member of a global class, DERIVED by the same rule
/// `#[rtse::abi]` uses (`rts_abi::scope::symbol_for`) — a class member is exactly
/// the `global = "<Class>"` scope: `__rtsm_global_<class>_<value>`.
///
/// `value` is the Rust member/field spelling VERBATIM (case-preserved), so two
/// members differing only by case stay distinct symbols. Only the class segment
/// is lower-cased and `_`-joined (`Intl.NumberFormat` → `intl_numberformat`).
///
/// Restating the rule here instead of calling `symbol_for` is the drift this
/// campaign exists to delete — there is one derivation (in `rts-abi`), and this
/// is a call into it.
pub(crate) fn class_symbol(class: &str, value: &str) -> String {
    symbol_for(
        &rts_abi::scope::Naming::Scoped {
            scope: Scope::Global(Some(class.to_string())),
            value: Some(value.to_string()),
        },
        value,
    )
}

/// `#[rtse::class("Name")]` — on the STRUCT (fields via `#[rtse::variable]`) OR on
/// the `impl` block (ctor/methods). Both are required for a class with fields: the
/// struct macro emits `__rtse_fields_<Class>(cb)` and the impl's `register` calls
/// it (option A coordination — the two items can't see each other otherwise).
pub(crate) fn expand(args: TokenStream, item: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<ClassArgs>(args) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };
    if let Ok(s) = syn::parse::<syn::ItemStruct>(item.clone()) {
        return gen_struct(parsed.name, s);
    }
    match syn::parse::<ItemImpl>(item) {
        Ok(imp) => gen_impl(parsed.name, parsed.extends, parsed.value, imp),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Parsed `#[rtse::class("Name")]` / `#[rtse::class("Name", extends = "Parent")]`
/// / `#[rtse::class("Name", value)]`.
struct ClassArgs {
    name: String,
    extends: Option<String>,
    /// A VALUE (primitive) class: instances are not `Entry::Rtse` structs but raw
    /// primitive words (a `Number` is an inline `f64`, a `Boolean` a `bool`, a
    /// `String` a string handle). A `#[rtse::method]` on a value class takes the
    /// receiver as its FIRST typed param (no `self`); the macro marshals the
    /// receiver word to that type and calls the fn directly (no `with_rtse`).
    value: bool,
}

impl syn::parse::Parse for ClassArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: syn::LitStr = input.parse()?;
        let mut extends = None;
        let mut value = false;
        while input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: syn::Ident = input.parse()?;
            // Bare flag `value` (no `= "..."`).
            if key == "value" && !input.peek(syn::Token![=]) {
                value = true;
                continue;
            }
            input.parse::<syn::Token![=]>()?;
            let val: syn::LitStr = input.parse()?;
            if key == "extends" {
                extends = Some(val.value());
            } else {
                return Err(syn::Error::new_spanned(
                    key,
                    "rtse::class: unknown arg (expected `extends` or `value`)",
                ));
            }
        }
        Ok(ClassArgs {
            name: name.value(),
            extends,
            value,
        })
    }
}

/// A `#[used]` static forcing `keep`'s extern addresses into the AOT archive.
///
/// `class_upper` here names a RUST ITEM (this static, and `__rtse_fields_<Class>`),
/// not a linker symbol — it never reaches a consumer by name and is not part of the
/// `__rtsm_`/`__rtsn_`/`__rtsa_` convention. Upper-case is kept because these are
/// Rust `static`/`fn` idents whose casing follows Rust, not JS.
fn keep_static(class_upper: &str, suffix: &str, keep: &[proc_macro2::Ident]) -> proc_macro2::TokenStream {
    if keep.is_empty() {
        return quote!();
    }
    let ident = format_ident!("__RTSE_KEEP_{}{}", class_upper, suffix);
    let n = keep.len();
    quote! {
        #[used]
        static #ident: [::rts_engine::FnPtr; #n] = [#(::rts_engine::FnPtr(#keep as *const u8)),*];
    }
}

/// `#[rtse::class]` on the `impl` — ctor/methods + `register` (which calls the
/// struct's `__rtse_fields_<Class>` to add the field accessors first).
fn gen_impl(
    class_name: String,
    extends: Option<String>,
    value_class: bool,
    mut imp: ItemImpl,
) -> TokenStream {
    let class_upper = class_name.to_uppercase().replace(['.'], "_");
    let self_ty = (*imp.self_ty).clone();
    let mut externs = Vec::new();
    let mut members = Vec::new();
    let mut keep = Vec::new();
    for it in imp.items.iter_mut() {
        let ImplItem::Fn(f) = it else { continue };
        let Some(kind) = take_kind(&mut f.attrs) else {
            continue;
        };
        let doc = extract_doc(&f.attrs);
        match member::gen_member(&class_name, &self_ty, &f.sig, kind, value_class, doc) {
            Ok((ext, mem, id)) => {
                externs.push(ext);
                members.push(mem);
                keep.push(id);
            }
            Err(e) => return e.to_compile_error().into(),
        }
    }
    let keep_tok = keep_static(&class_upper, "", &keep);
    let fields_fn = format_ident!("__rtse_fields_{}", class_upper);
    // `extends = "Parent"` → `.extends("Parent")` on the ClassBuilder (Registry
    // parent link for `x instanceof Parent`; methods are inherited by composition
    // + forwarding in the author's struct, not by a dispatch chain-walk).
    let extends_tok = match &extends {
        Some(p) => quote!(.extends(#p)),
        None => quote!(),
    };
    quote! {
        #imp
        #(#externs)*
        #keep_tok
        /// Register this class + members into the engine Registry. Add to `REGISTER`.
        pub fn register(e: &mut ::rts_engine::Engine) {
            #fields_fn(e.class(#class_name))
                #extends_tok
                #(#members)*
                .done();
        }
    }
    .into()
}

/// `#[rtse::class]` on the STRUCT — generate a getter (and setter unless
/// `readonly`) per `#[rtse::variable]` field, plus
/// `pub fn __rtse_fields_<Class>(cb) -> cb` that the impl's `register` chains.
fn gen_struct(class_name: String, mut s: syn::ItemStruct) -> TokenStream {
    let class_upper = class_name.to_uppercase().replace(['.'], "_");
    let self_ty: Type = syn::parse_str(&s.ident.to_string()).unwrap();
    let mut externs = Vec::new();
    let mut members = Vec::new();
    let mut keep = Vec::new();
    if let syn::Fields::Named(named) = &mut s.fields {
        for f in named.named.iter_mut() {
            let Some(readonly) = take_variable(&mut f.attrs) else {
                continue;
            };
            let fname = f.ident.clone().unwrap();
            let fdoc = extract_doc(&f.attrs);
            match field::gen_field(&class_name, &self_ty, &fname, &f.ty, readonly, fdoc) {
                Ok((exts, mems, ids)) => {
                    externs.extend(exts);
                    members.extend(mems);
                    keep.extend(ids);
                }
                Err(e) => return e.to_compile_error().into(),
            }
        }
    }
    let keep_tok = keep_static(&class_upper, "_FIELDS", &keep);
    let fields_fn = format_ident!("__rtse_fields_{}", class_upper);
    quote! {
        #s
        #(#externs)*
        #keep_tok
        /// Add this class's `#[rtse::variable]` field accessors to `cb`. Called by
        /// the impl's generated `register`.
        pub fn #fields_fn(cb: ::rts_engine::ClassBuilder) -> ::rts_engine::ClassBuilder {
            cb #(#members)*
        }
    }
    .into()
}

/// Harvest the Rust `///` doc comments of an item (syn lowers each to a
/// `#[doc = "..."]` attribute) into one string — flows into `Member.doc`, which
/// the `.d.ts` generator emits. Leading space (from `/// text`) is trimmed.
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
