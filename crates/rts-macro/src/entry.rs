//! `#[rtse::entry]` — the same derivation, for the engine that replaced the ABI.
//!
//! # Two attributes, not one with a mode
//!
//! [`crate::abi`] emits an `rts_abi::SymbolDesc`. `rts-abi` is the interface
//! `rts_cranelift::abi` was built to replace — its own module says why:
//! *"entirely scalar: no aggregate, no structure, a return position holding zero
//! or one machine slot, and a string that cannot be returned at all… It is not a
//! foundation."*
//!
//! So the two emissions describe different things and will diverge further, not
//! less: this one can express an aggregate parameter and more than one return,
//! and that one cannot. A single attribute with a flag would have to keep both
//! shapes alive in one body forever. Two attributes means the old one is
//! **deleted whole** when `rts-abi` goes, and nothing has to be untangled first.
//!
//! What they share is the part that matters, and it is shared by being written
//! once here: the ABI shape is **derived from the Rust signature**, never typed.
//! Editing a parameter changes the descriptor in the same edit, or the crate
//! does not compile.
//!
//! # Why this is worth an attribute rather than a hand-written list
//!
//! A hand-written entry list can be right. It cannot be *kept* right: the list
//! says `two parameters, both tagged` in one file while the function says
//! `(u64, u64)` in another, and nothing connects them. That is the drift this
//! whole authoring surface exists to make unrepresentable, and re-introducing it
//! for four entries would re-introduce it for four hundred.
//!
//! # What it does not decide
//!
//! Which number an entry has. A declaration cannot see its neighbours — no
//! proc-macro can — so numbering belongs to whatever assembles the set. This
//! emits a descriptor; something else puts the descriptors in order.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, ReturnType, Type};

/// Expand `#[rtse::entry]`.
pub fn expand(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let function: ItemFn = syn::parse2(item)?;
    let symbol = symbol_name(&args, &function)?;

    let params = function
        .sig
        .inputs
        .iter()
        .map(param_type)
        .collect::<syn::Result<Vec<_>>>()?;

    let returns = match &function.sig.output {
        ReturnType::Default => Vec::new(),
        ReturnType::Type(_, ty) => vec![abi_type(ty)?],
    };

    let descriptor = format_ident!(
        "{}_ENTRY",
        symbol.trim_start_matches("__rts_").to_uppercase()
    );

    let name = &function.sig.ident;
    let attrs = &function.attrs;
    let vis = &function.vis;
    let signature = &function.sig;
    let body = &function.block;

    Ok(quote! {
        /// The ABI shape of the entry point below, derived from its Rust
        /// signature so the two cannot disagree.
        pub const #descriptor: ::rts_cranelift::abi::EntryDesc =
            ::rts_cranelift::abi::EntryDesc {
                symbol: #symbol,
                params: &[#(#params),*],
                returns: &[#(#returns),*],
                convention: ::rts_cranelift::abi::Convention::Foreign,
            };

        #(#attrs)*
        // `export_name` rather than `no_mangle` plus a renamed function: the
        // author writes an ordinary Rust name and the linker sees the derived
        // one, so the two spellings cannot disagree.
        #[unsafe(export_name = #symbol)]
        #vis extern "C" #signature #body

        // Referring to the definition from the descriptor's own module keeps
        // the two in one compilation unit, so removing the function without the
        // descriptor is a build failure rather than a dangling declaration.
        const _: () = {
            let _ = #name;
        };
    })
}

/// The linker name: the one written, or `__rts_` plus the function's own name.
fn symbol_name(args: &TokenStream, function: &ItemFn) -> syn::Result<String> {
    let written = args.to_string();
    let trimmed = written.trim().trim_matches('"').trim();
    if !trimmed.is_empty() {
        return Ok(trimmed.to_owned());
    }
    Ok(format!("__rts_{}", function.sig.ident))
}

fn param_type(arg: &FnArg) -> syn::Result<TokenStream> {
    match arg {
        FnArg::Typed(typed) => abi_type(&typed.ty),
        FnArg::Receiver(receiver) => Err(syn::Error::new_spanned(
            receiver,
            "an entry point is a free function: `self` cannot cross an ABI boundary",
        )),
    }
}

/// The machine's `AbiType` a Rust type is.
///
/// Deliberately a short list. A type that is not here is one whose crossing has
/// not been decided, and refusing it is the honest answer — guessing produces a
/// call that compiles and passes the wrong number of registers.
///
/// `u64` is a **tagged value**, not an integer: it is what a `Value` is, and the
/// machine's word for "nothing has been proved about this" is `Repr::Tagged`.
/// An entry point wanting a genuine integer takes `i64`.
fn abi_type(ty: &Type) -> syn::Result<TokenStream> {
    let spelled = quote!(#ty).to_string().replace(' ', "");
    let repr = match spelled.as_str() {
        "u64" => quote!(::rts_cranelift::repr::Repr::Tagged),
        "i64" => quote!(::rts_cranelift::repr::Repr::I64),
        "i32" => quote!(::rts_cranelift::repr::Repr::I32),
        "f64" => quote!(::rts_cranelift::repr::Repr::F64),
        "bool" => quote!(::rts_cranelift::repr::Repr::Bool),
        other => {
            return Err(syn::Error::new_spanned(
                ty,
                format!(
                    "`{other}` has no decided ABI crossing. The types that do are \
                     u64 (a tagged value), i64, i32, f64 and bool. Adding one is a \
                     decision about the boundary, not about this function."
                ),
            ));
        }
    };
    Ok(quote!(::rts_cranelift::abi::AbiType::Scalar(#repr)))
}
