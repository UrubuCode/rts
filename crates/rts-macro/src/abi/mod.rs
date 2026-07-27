//! `#[rtse::abi]` expansion — a `SymbolDesc` const NEXT TO an `extern "C"`
//! symbol, with BOTH the linker name and the ABI shape DERIVED, never typed.
//!
//! This is phase F0/F0b of `docs/specs/rts-macro-single-source.md`. A runtime
//! symbol used to be spelled out four times — the `extern "C"` definition, the
//! JIT fn-ptr table, the Cranelift signature table (`value::abi_sig`), and a
//! string literal at every lowering call site — and nothing checked that the
//! signature table agreed with the definition. `abi_sig`'s own doc says it
//! exists because "mis-marshaling a single-slot string where the ABI expects two
//! → SIGILL", so the guard against a SIGILL was itself hand-synced.
//!
//! Deriving the descriptor from the real parameter list makes that drift
//! UNREPRESENTABLE rather than test-checked: edit the Rust signature and the
//! descriptor changes in the same edit, or the crate does not compile.
//!
//! The author writes PLAIN RUST. `extern "C"`, `#[unsafe(no_mangle)]` and the
//! symbol string are ABI concerns, and this attribute owns all of them — writing
//! them by hand is what made the symbol a hand-synced fact in the first place.
//!
//! # Naming
//!
//! The declaration states its SCOPE and the macro derives the symbol (see
//! `scope.rs` for the full rule):
//!
//! ```ignore
//! #[rtse::abi(module = "io", value = "print")]        // __rtsm_io_print
//! #[rtse::abi(module = "node:fs")]                    // __rtsm_node_fs_<fn name>
//! #[rtse::abi(global = "String", value = "toUpperCase")] // __rtsm_global_string_toUpperCase
//! #[rtse::abi(global, value = "parseInt")]            // __rtsm_global_parseInt
//! #[rtse::abi(native)]                                // __rtsn_<fn name>   (NATIVE, not node)
//! #[rtse::abi(abi, value = "obj_get")]                // __rtsa_obj_get
//! #[rtse::abi("__rtsa_legacy")]                       // explicit escape hatch
//! #[rtse::abi]                                        // transitional: `__` + fn name
//! ```
//!
//! `<value>` keeps the JS spelling verbatim (`readFileSync`); the scope segments
//! are lower-cased and `_`-joined. There are no uppercase block segments any
//! more — `FN`/`NS`/`GL`/`GC`/`CONST` are gone, the prefix carries what they
//! encoded. The bare no-argument form exists only to carry symbols whose
//! spelling predates the convention until their group is converted.
//!
//! The const's name is the symbol with its prefix stripped, upper-cased,
//! suffixed `_ABI` — `__rtsa_obj_get` → `OBJ_GET_ABI`.
//!
//! `abi` is its OWN category, deliberately separate from `#[rtse::class]`: a
//! class member is resolved by NAME through the Registry at runtime, while an
//! ABI symbol is emitted directly by the codegen as an immediate. They need
//! different things from the macro and conflating them is what would force an
//! intrinsic through Registry dispatch it never wanted.
//!
//! # Parameter kinds
//!
//! See `params.rs` for the full table. Most signatures are rewritten IN PLACE
//! (the plain Rust fn becomes the ABI symbol — no trampoline, which matters on
//! the hottest path in the engine). A `&str` parameter (one `AbiType::StrPtr`
//! occupying two Rust slots) or a Rust `bool` (which crosses as `i64`, never
//! `i8`) makes the extern parameter list diverge from the Rust one, so those
//! emit a thin `#[inline(always)]` inner fn plus the extern wrapper.

pub(crate) mod params;
pub(crate) mod scope;

use proc_macro::TokenStream;
use quote::{format_ident, quote};

use crate::naming::abi_const_name;
use scope::{AbiArgs, symbol_for};

pub(crate) fn expand(a: TokenStream, item: TokenStream) -> TokenStream {
    let mut func = syn::parse_macro_input!(item as syn::ItemFn);
    let args = match syn::parse::<AbiArgs>(a) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };
    let sym = symbol_for(&args.0, &func.sig.ident.to_string());

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
    if func
        .attrs
        .iter()
        .any(|at| at.path().is_ident("no_mangle") || quote!(#at).to_string().contains("no_mangle"))
    {
        return syn::Error::new_spanned(
            &func.sig,
            "#[rtse::abi]: drop the `#[unsafe(no_mangle)]` — the attribute applies it",
        )
        .to_compile_error()
        .into();
    }

    let mut abis: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut ext_params: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut call_args: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut wrap = false;
    for (idx, arg) in func.sig.inputs.iter().enumerate() {
        let syn::FnArg::Typed(pt) = arg else {
            return syn::Error::new_spanned(arg, "#[rtse::abi]: `self` is not an ABI parameter")
                .to_compile_error()
                .into();
        };
        let Some(p) = params::param_of(&pt.ty, idx) else {
            return syn::Error::new_spanned(
                &pt.ty,
                "#[rtse::abi]: no ABI spelling for this parameter type — write one of the \
                 declared ABI tokens (`Handle`, `Poly`, `U64`, `I64`, `I32`, `F64`, `Bool`), a \
                 Rust scalar, or `&str` for a string. A raw pointer has no single-slot ABI \
                 form; pass the string as `&str` (it expands to the `StrPtr` ptr+len pair) or \
                 the address as `U64`.",
            )
            .to_compile_error()
            .into();
        };
        abis.push(p.abi);
        ext_params.extend(p.ext);
        call_args.push(p.call);
        wrap |= p.needs_wrapper;
    }

    let Some(ret) = params::ret_of(&func.sig.output) else {
        return syn::Error::new_spanned(
            &func.sig.output,
            "#[rtse::abi]: no ABI spelling for this return type — a string result is returned \
             as a `Handle`, not as `&str` (`StrPtr` is not returnable)",
        )
        .to_compile_error()
        .into();
    };
    wrap |= ret.needs_wrapper;

    let ret_abi = ret.abi;
    let const_ident = proc_macro2::Ident::new(&abi_const_name(&sym), func.sig.ident.span());
    let doc = format!("ABI descriptor for `{sym}` (generated by `#[rtse::abi]`).");
    let extern_ident = proc_macro2::Ident::new(&sym, func.sig.ident.span());

    let emitted = if wrap {
        // The extern parameter list diverges from the Rust one (a `StrPtr` pair,
        // or a `bool` crossing as `i64`), so the symbol must be a distinct
        // function. `#[inline(always)]` on the inner fn collapses the call back
        // to nothing.
        let inner_ident = format_ident!("__rtsabi_inner_{}", sym);
        func.sig.ident = inner_ident.clone();
        func.vis = syn::Visibility::Inherited;
        func.attrs.push(syn::parse_quote!(#[inline(always)]));
        let ret_ext_ty = ret.ext_ty;
        let call = quote!(#inner_ident(#(#call_args),*));
        let body = match ret.convert {
            Some(conv) => quote!({ let __r = #call; #conv }),
            None => quote!({ #call }),
        };
        quote! {
            #func
            #[unsafe(no_mangle)]
            pub extern "C" fn #extern_ident(#(#ext_params),*) -> #ret_ext_ty #body
        }
    } else {
        // Rewrite the plain Rust fn INTO the ABI symbol: rename to the linker
        // spelling, make it `extern "C"`, mark it `no_mangle`. One function, not
        // a wrapper around another — a trampoline here would be a real call per
        // invocation on the hottest path in the engine.
        func.sig.ident = extern_ident;
        func.sig.abi = Some(syn::parse_quote!(extern "C"));
        func.attrs.push(syn::parse_quote!(#[unsafe(no_mangle)]));
        quote!(#func)
    };

    quote! {
        #emitted
        #[doc = #doc]
        pub const #const_ident: ::rts_engine::abi::SymbolDesc =
            ::rts_engine::abi::SymbolDesc {
                name: #sym,
                params: &[#(#abis),*],
                ret: #ret_abi,
            };
    }
    .into()
}
