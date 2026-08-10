//! Turning a plain Rust function into a runtime entry point.
//!
//! # The shape is derived, never typed
//!
//! The ABI shape comes from the Rust signature. Editing a parameter changes the
//! descriptor in the same edit, or the crate does not compile.
//!
//! That is the whole reason this exists rather than a hand-written list. A
//! hand-written list can be right; it cannot be *kept* right, because it says
//! `two tagged parameters` in one file while the function says `(u64, u64)` in
//! another and nothing connects them. This was not hypothetical — the first
//! version of the new engine's entry table was written that way, and the two
//! spellings were already sitting in different files by the time it was
//! reviewed.
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

    // A slice parameter cannot cross `extern "C"` as itself: it is a pointer and
    // a length, and Rust will not pass one as a single argument. So a function
    // taking one keeps its ordinary Rust signature and gains a trampoline; a
    // function taking none is rewritten in place and pays nothing.
    //
    // Which is why the two cases are separate rather than a trampoline for all:
    // the common case is scalars, and a trampoline there would add a call for
    // no reason.
    if function.sig.inputs.iter().any(|arg| slice_kind(arg).is_some()) {
        return Ok(trampoline(&function, &symbol, &descriptor, &params, &returns));
    }

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
        // A pointer and a length, as ONE logical argument — which is what the
        // machine's `Slice` is, and an improvement on the interface it replaced,
        // where a string was two loose slots a caller had to remember to pass
        // together.
        //
        // Three spellings, and the difference is not cosmetic. `&[u16]` is what
        // a JavaScript string IS — a sequence of UTF-16 code units, lone
        // surrogates included — so it crosses as itself, byte for byte, with
        // nothing to convert and nothing to validate. `&str` is UTF-8, which no
        // string in this engine is stored as, so a caller holding one of ours
        // re-encodes to reach it.
        //
        // Both exist because both have real callers: `&[u16]` is the shape for
        // anything the runtime hands over, and `&str` is what the rts-std and
        // rts-node surface is already written in — 119 parameters of it in
        // rts-node alone. Naming the element is what lets a reader see which of
        // the two a given entry point costs.
        "&str" | "&[u8]" => return Ok(slice_of(quote!(::rts_cranelift::repr::Repr::I8))),
        "&[u16]" => return Ok(slice_of(quote!(::rts_cranelift::repr::Repr::I16))),
        other => {
            return Err(syn::Error::new_spanned(
                ty,
                format!(
                    "`{other}` has no decided ABI crossing. The types that do are \
                     u64 (a tagged value), i64, i32, f64, bool, &str, &[u8] and \
                     &[u16]. Adding one is a decision about the boundary, not \
                     about this function."
                ),
            ));
        }
    };
    Ok(quote!(::rts_cranelift::abi::AbiType::Scalar(#repr)))
}

/// A slice of the given element.
fn slice_of(element: TokenStream) -> TokenStream {
    quote!(::rts_cranelift::abi::AbiType::Slice(#element))
}

/// The three slice spellings, which differ only in what the trampoline rebuilds.
#[derive(Clone, Copy)]
enum SliceKind {
    /// `&str` — UTF-8, and therefore checked.
    Text,
    /// `&[u8]` — bytes, with nothing to check.
    Bytes,
    /// `&[u16]` — UTF-16 code units, which is what a JavaScript string is.
    ///
    /// Nothing to check here either, and for a reason worth stating rather than
    /// inferring: a lone surrogate is a legal JavaScript string, so a crossing
    /// that rejected one would be unable to carry the result of an operation
    /// the language performs. That is the whole reason this spelling exists
    /// beside `&str` instead of everything going through UTF-8.
    Units,
}

impl SliceKind {
    /// The element the trampoline points at.
    fn element(self) -> TokenStream {
        match self {
            SliceKind::Text | SliceKind::Bytes => quote!(u8),
            SliceKind::Units => quote!(u16),
        }
    }
}

/// Which slice a parameter is, if it is one.
fn slice_kind(arg: &FnArg) -> Option<SliceKind> {
    let FnArg::Typed(typed) = arg else {
        return None;
    };
    let ty = &typed.ty;
    match quote!(#ty).to_string().replace(' ', "").as_str() {
        "&str" => Some(SliceKind::Text),
        "&[u8]" => Some(SliceKind::Bytes),
        "&[u16]" => Some(SliceKind::Units),
        _ => None,
    }
}

/// The expansion for a function taking a slice.
///
/// The author's function keeps its ordinary Rust signature — `&str`, `&[u16]`
/// and all — and an `extern "C"` trampoline beside it takes the pointer and the
/// length, rebuilds the slice, and calls it.
///
/// # What each of the three costs
///
/// `&[u16]` is free. A JavaScript string is a sequence of UTF-16 code units, so
/// the caller hands over the buffer it already has and the trampoline hands it
/// straight on. No copy, no check, no failure mode — including for a lone
/// surrogate, which is legal and which any UTF-8 path would have to reject.
///
/// `&[u8]` is free too, and means bytes rather than text.
///
/// `&str` is the one that costs. It is UTF-8, and no string in this engine is
/// stored that way: the narrow form is latin-1 and the wide form is UTF-16.
/// Reaching a `&str` parameter from a runtime string is therefore a re-encoding
/// on **every call**, and the machine's own ABI documentation names that class
/// of cost as the reason the interface it replaced "is not a foundation":
/// *"a client with value types pays an allocation at every boundary crossing —
/// which measurement identifies as the largest single cost in the system."*
///
/// It is accepted anyway, because the surface asking to use this attribute is
/// already written in it. What changed is that the descriptor now says which of
/// the two an entry point is, so the cost is visible where the entry is declared
/// rather than discovered in a profile.
///
/// # What the `&str` trampoline refuses
///
/// Bytes that are not UTF-8. The caller is compiled code handing over what the
/// runtime holds, so this cannot be a `Result` — there is nobody to return one
/// to. It aborts, for the same reason a missing context does: it means the two
/// sides disagree about what a string is, which is a broken build rather than a
/// condition a program can reach.
///
/// The `&[u16]` path has no equivalent, and that is the point of it existing.
fn trampoline(
    function: &ItemFn,
    symbol: &str,
    descriptor: &syn::Ident,
    params: &[TokenStream],
    returns: &[TokenStream],
) -> TokenStream {
    let name = &function.sig.ident;
    let output = &function.sig.output;
    // A distinct Rust name: the const already holds the descriptor spelling, and
    // the exported symbol is set by `export_name` rather than by this ident.
    let crossing = format_ident!("__crossing_{}", name);

    let mut forwarded = Vec::new();
    let mut declared = Vec::new();

    for (position, arg) in function.sig.inputs.iter().enumerate() {
        let ptr = format_ident!("__ptr{position}");
        let len = format_ident!("__len{position}");
        let value = format_ident!("__arg{position}");

        if let Some(kind) = slice_kind(arg) {
            let element = kind.element();
            declared.push(quote!(#ptr: *const #element, #len: usize));

            // SAFETY, shared by all three: the caller passed a pointer and a
            // length describing one allocation it keeps alive across this call,
            // which is what a slice argument means on both sides of the
            // boundary. The length is in ELEMENTS, not bytes — `from_raw_parts`
            // and the machine's `Slice` agree on that, and a `&[u16]` whose
            // length were bytes would read twice what it was given.
            let rebuilt = quote! {
                unsafe { ::core::slice::from_raw_parts(#ptr, #len) }
            };

            forwarded.push(match kind {
                // Code units and bytes cross as themselves. Nothing to check:
                // for `&[u16]` specifically, a lone surrogate is a legal
                // JavaScript string, so there is no validity to assert.
                SliceKind::Units | SliceKind::Bytes => rebuilt,
                SliceKind::Text => quote! {{
                    match ::core::str::from_utf8(#rebuilt) {
                        Ok(text) => text,
                        Err(_) => {
                            eprintln!(
                                "rts: {} received bytes that are not UTF-8",
                                #symbol
                            );
                            ::std::process::abort();
                        }
                    }
                }},
            });
        } else {
            let FnArg::Typed(typed) = arg else { continue };
            let ty = &typed.ty;
            declared.push(quote!(#value: #ty));
            forwarded.push(quote!(#value));
        }
    }

    quote! {
        /// The ABI shape of the entry point below, derived from its Rust
        /// signature so the two cannot disagree.
        pub const #descriptor: ::rts_cranelift::abi::EntryDesc =
            ::rts_cranelift::abi::EntryDesc {
                symbol: #symbol,
                params: &[#(#params),*],
                returns: &[#(#returns),*],
                convention: ::rts_cranelift::abi::Convention::Foreign,
            };

        #function

        #[unsafe(export_name = #symbol)]
        extern "C" fn #crossing(#(#declared),*) #output {
            #name(#(#forwarded),*)
        }
    }
}
