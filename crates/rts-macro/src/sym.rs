//! `rtse::sym!(Type::member)` — a function-like macro resolving to the
//! `SymbolDesc` const `#[rtse::class]` emits per member
//! (`class::member::mod.rs`, name rule in `naming::member_sym_const_name`).
//!
//! Expands PURELY SYNTACTICALLY: `Type::member` becomes `Type::MEMBER_SYM`, a
//! path reference to the associated const. There is no lookup table here — the
//! macro does not need one, because the only thing that makes
//! `Type::MEMBER_SYM` resolve is `rustc` finding a real associated const of
//! that name on `Type`, which only exists if `#[rtse::class]` actually
//! generated it for a member named `member`. A typo'd member
//! (`rtse::sym!(NumberWrapper::is_nann)`) or a renamed one is therefore a
//! `rustc` "no associated item" error at the call site — never a runtime
//! SIGILL from a symbol string that silently stopped matching anything.

use proc_macro::TokenStream;
use quote::quote;

use crate::naming::member_sym_const_name;

pub(crate) fn expand(input: TokenStream) -> TokenStream {
    let path = syn::parse_macro_input!(input as syn::Path);
    let mut segs = path.segments;
    let Some(last) = segs.pop() else {
        return syn::Error::new_spanned(&segs, "rtse::sym!: expected `Type::member`")
            .to_compile_error()
            .into();
    };
    let member_seg = last.into_value();
    let const_name = member_sym_const_name(&member_seg.ident.to_string());
    let const_ident = proc_macro2::Ident::new(&const_name, member_seg.ident.span());

    if segs.is_empty() {
        return syn::Error::new_spanned(
            &member_seg.ident,
            "rtse::sym!: expected `Type::member`, got a bare ident",
        )
        .to_compile_error()
        .into();
    }
    // `Punctuated::pop` removes the SEGMENT but leaves its trailing `::`, so the
    // path would render as `Type::` and splice into `Type::::MEMBER_SYM` — a
    // parse error at the call site rather than anything meaningful. Drop it.
    while segs.trailing_punct() {
        segs.pop_punct();
    }
    let ty_path = syn::Path {
        leading_colon: path.leading_colon,
        segments: segs,
    };
    quote!(#ty_path::#const_ident).into()
}
