//! `#[rtse::abi]` expansion — a `SymbolDesc` const NEXT TO an `extern "C"`
//! symbol, with the ABI shape DERIVED FROM THE RUST SIGNATURE.
//!
//! This is phase F0 of `docs/specs/rts-macro-single-source.md`. A runtime symbol
//! used to be spelled out four times — the `extern "C"` definition, the JIT
//! fn-ptr table, the Cranelift signature table (`value::abi_sig`), and a string
//! literal at every lowering call site — and nothing checked that the signature
//! table agreed with the definition. `abi_sig`'s own doc says it exists because
//! "mis-marshaling a single-slot string where the ABI expects two → SIGILL", so
//! the guard against a SIGILL was itself hand-synced.
//!
//! Deriving the descriptor from the real parameter list makes that drift
//! UNREPRESENTABLE rather than test-checked: edit the Rust signature and the
//! descriptor changes in the same edit, or the crate does not compile.
//!
//! The author writes PLAIN RUST. `extern "C"`, `#[unsafe(no_mangle)]` and the
//! leading `__` of the linker symbol are ABI concerns, and this attribute owns
//! all of them — writing them by hand is what made the symbol a hand-synced fact
//! in the first place.
//!
//! ```ignore
//! #[rtse::abi]
//! pub fn rtsadp_obj_get(obj: u64, key: u64) -> u64 { … }
//!
//! // expands to the real ABI symbol …
//! // #[unsafe(no_mangle)]
//! // pub extern "C" fn __rtsadp_obj_get(obj: u64, key: u64) -> u64 { … }
//! // … plus its descriptor:
//! // pub const OBJ_GET_ABI: SymbolDesc =
//! //     SymbolDesc { name: "__rtsadp_obj_get", params: &[U64, U64], ret: U64 };
//! ```
//!
//! **Naming.** The linker symbol is the Rust fn name prefixed with `__`
//! (`rtsadp_obj_get` → `__rtsadp_obj_get`, `RTS_FN_NS_IO_PRINT` →
//! `__RTS_FN_NS_IO_PRINT`), so Rust-internal callers keep using the `__` spelling
//! they already use and nothing at the call sites changes. Pass an explicit name
//! (`#[rtse::abi("__rtsadp_legacy")]`) only where a symbol must keep a spelling
//! the rule would not produce.
//!
//! The const's name is the symbol with its `__rtsadp_` / `__RTS_FN_` prefix
//! stripped, upper-cased, suffixed `_ABI`.
//!
//! `abi` is its OWN category, deliberately separate from `#[rtse::class]`: a
//! class member is resolved by NAME through the Registry at runtime, while an
//! ABI symbol is emitted directly by the codegen as an immediate. They need
//! different things from the macro and conflating them is what would force an
//! intrinsic through Registry dispatch it never wanted.
//!
//! Type mapping (Rust → `AbiType`): `u64`→`U64`, `i64`→`I64`, `i32`→`I32`,
//! `f64`→`F64`, `bool`→`Bool`, no return→`Void`. A `StrPtr` pair
//! (`*const u8, i64`) is NOT inferable from the Rust types alone (it is two
//! parameters that must collapse to one `AbiType`), so a pointer parameter is a
//! compile error here until F0b adds an explicit opt-in; those symbols keep
//! their hand-written `abi_sig` entry for now.

use proc_macro::TokenStream;
use quote::quote;

use crate::naming::abi_const_name;
use crate::types::abi_ty_of;

pub(crate) fn expand(a: TokenStream, item: TokenStream) -> TokenStream {
    let mut func = syn::parse_macro_input!(item as syn::ItemFn);

    // The linker symbol: an explicit `#[rtse::abi("…")]` override, else the fn
    // name prefixed with `__`.
    let sym = if a.is_empty() {
        format!("__{}", func.sig.ident)
    } else {
        match syn::parse::<syn::LitStr>(a) {
            Ok(s) => s.value(),
            Err(e) => return e.to_compile_error().into(),
        }
    };

    // The author wrote plain Rust; reject hand-written ABI ceremony rather than
    // silently double-applying it. This attribute is the one authority for it.
    if func.sig.abi.is_some() {
        return syn::Error::new_spanned(
            &func.sig,
            "#[rtse::abi]: drop the `extern \"C\"` — the attribute applies it",
        )
        .to_compile_error()
        .into();
    }
    if func.attrs.iter().any(|at| {
        at.path().is_ident("no_mangle")
            || quote!(#at).to_string().contains("no_mangle")
    }) {
        return syn::Error::new_spanned(
            &func.sig,
            "#[rtse::abi]: drop the `#[unsafe(no_mangle)]` — the attribute applies it",
        )
        .to_compile_error()
        .into();
    }

    // params
    let mut params: Vec<proc_macro2::TokenStream> = Vec::new();
    for arg in &func.sig.inputs {
        let syn::FnArg::Typed(pt) = arg else {
            return syn::Error::new_spanned(arg, "#[rtse::abi]: `self` is not an ABI parameter")
                .to_compile_error()
                .into();
        };
        match abi_ty_of(&pt.ty) {
            Some(t) => params.push(t),
            None => {
                return syn::Error::new_spanned(
                    &pt.ty,
                    "#[rtse::abi]: unsupported ABI parameter type (pointer/StrPtr pairs are not \
                     inferable yet — keep this symbol's hand-written abi_sig entry)",
                )
                .to_compile_error()
                .into();
            }
        }
    }

    // return
    let ret = match &func.sig.output {
        syn::ReturnType::Default => quote::quote!(::rts_engine::abi::AbiType::Void),
        syn::ReturnType::Type(_, ty) => match abi_ty_of(ty) {
            Some(t) => t,
            None => {
                return syn::Error::new_spanned(ty, "#[rtse::abi]: unsupported ABI return type")
                    .to_compile_error()
                    .into();
            }
        },
    };

    let const_ident = proc_macro2::Ident::new(&abi_const_name(&sym), func.sig.ident.span());
    let doc = format!("ABI descriptor for `{sym}` (generated by `#[rtse::abi]`).");

    // Rewrite the plain Rust fn INTO the ABI symbol: rename to the linker
    // spelling, make it `extern "C"`, mark it `no_mangle`. One function, not a
    // wrapper around another — a trampoline here would be a real call per
    // invocation on the hottest path in the engine.
    func.sig.ident = proc_macro2::Ident::new(&sym, func.sig.ident.span());
    func.sig.abi = Some(syn::parse_quote!(extern "C"));
    func.attrs.push(syn::parse_quote!(#[unsafe(no_mangle)]));

    quote::quote! {
        #func
        #[doc = #doc]
        pub const #const_ident: ::rts_engine::abi::SymbolDesc =
            ::rts_engine::abi::SymbolDesc {
                name: #sym,
                params: &[#(#params),*],
                ret: #ret,
            };
    }
    .into()
}
