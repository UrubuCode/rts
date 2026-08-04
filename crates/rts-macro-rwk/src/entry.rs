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

    // A string parameter cannot cross `extern "C"` as a `&str`: it is a pointer
    // and a length, and Rust will not pass one as a single argument. So a
    // function taking one keeps its ordinary Rust signature and gains a
    // trampoline; a function taking none is rewritten in place and pays nothing.
    //
    // Which is why the two cases are separate rather than a trampoline for all:
    // the common case is scalars, and a trampoline there would add a call for
    // no reason.
    if function.sig.inputs.iter().any(is_string_param) {
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
        // machine's `Slice` is, and an improvement on the interface it
        // replaced, where a string was two loose slots a caller had to
        // remember to pass together.
        "&str" => return Ok(quote!(::rts_cranelift::abi::AbiType::Slice)),
        other => {
            return Err(syn::Error::new_spanned(
                ty,
                format!(
                    "`{other}` has no decided ABI crossing. The types that do are \
                     u64 (a tagged value), i64, i32, f64, bool and &str. Adding one is a \
                     decision about the boundary, not about this function."
                ),
            ));
        }
    };
    Ok(quote!(::rts_cranelift::abi::AbiType::Scalar(#repr)))
}

/// Whether a parameter is a string, which crosses as a pointer and a length.
fn is_string_param(arg: &FnArg) -> bool {
    let FnArg::Typed(typed) = arg else {
        return false;
    };
    let ty = &typed.ty;
    quote!(#ty).to_string().replace(' ', "") == "&str"
}

/// The expansion for a function taking a string.
///
/// The author's function keeps its ordinary Rust signature — `&str` and all —
/// and an `extern "C"` trampoline beside it takes the pointer and the length,
/// rebuilds the slice, and calls it.
///
/// # What the trampoline refuses
///
/// Bytes that are not UTF-8. The caller is compiled code handing over what the
/// runtime holds, so this cannot be a `Result` — there is nobody to return one
/// to. It aborts, for the same reason a missing context does: it means the two
/// sides disagree about what a string is, which is a broken build rather than a
/// condition a program can reach.
///
/// # The conversion this makes visible
///
/// A string in `rts-core-rwk` is **UTF-16 code units**. A `&str` is UTF-8. So
/// every call through this trampoline from the new runtime is a re-encoding,
/// and the machine's own ABI documentation names that class of cost as the
/// reason the interface it replaced "is not a foundation": *"a client with value
/// types pays an allocation at every boundary crossing — which measurement
/// identifies as the largest single cost in the system."*
///
/// That is not an argument against this existing. It is an argument for knowing
/// which surface uses it: a handle or a scalar crosses free, and a string does
/// not.
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

        if is_string_param(arg) {
            declared.push(quote!(#ptr: *const u8, #len: usize));
            forwarded.push(quote! {{
                // SAFETY: the caller passed a pointer and a length describing
                // one allocation it keeps alive across this call, which is what
                // a slice argument means on both sides of the boundary.
                let bytes = unsafe { ::core::slice::from_raw_parts(#ptr, #len) };
                match ::core::str::from_utf8(bytes) {
                    Ok(text) => text,
                    Err(_) => {
                        eprintln!(
                            "rts: {} received bytes that are not UTF-8",
                            #symbol
                        );
                        ::std::process::abort();
                    }
                }
            }});
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
