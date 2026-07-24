//! `#[rtse::*]` — authoring macros for the engine ABI (RTS_ENGINE_ABI_CODEGEN).
//!
//! Annotate a normal Rust struct + `impl`; `#[rtse::class(...)]` on the impl block
//! generates the extern-C ABI wrappers + Registry metadata for the JS engine,
//! WITHOUT changing the struct's normal Rust usability (like PyO3/napi-rs).
//!
//! ```ignore
//! struct Point { x: f64, y: f64 }
//! #[rtse::class("Point")]
//! impl Point {
//!     #[rtse::ctor]   fn new(x: f64, y: f64) -> Self { Point { x, y } }
//!     #[rtse::method] fn sum(&self) -> f64 { self.x + self.y }
//!     #[rtse::method] fn label(&self) -> String { format!("({},{})", self.x, self.y) }
//! }
//! ```
//!
//! Emits: the impl unchanged (methods stay normal Rust), one `extern "C"` wrapper
//! per ctor/method (marshalling handle↔struct and Rust↔ABI — a `String`/`&str`
//! return is allocated into the string pool and returned as a handle), and
//! `pub fn register(e: &mut Engine)`. The author adds `register` to `REGISTER`.
//!
//! Status: G1 — ctor + instance methods, scalar (`f64`/`i64`/`i32`/`bool`) +
//! `String`/`&str` RETURN, `#[rtse::method(name=…, readonly)]`, `#[rtse::private]`.
//! `#[rtse::variable]` fields, `&str` PARAMS, statics, `primitive=`,
//! `#[rtse::symbol]` land incrementally.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ItemImpl, ReturnType, Type, parse_macro_input};

/// `#[rtse::class("Name")]` on an `impl` block — generate the engine ABI glue.
#[proc_macro_attribute]
pub fn class(args: TokenStream, item: TokenStream) -> TokenStream {
    let class_name = match syn::parse::<syn::LitStr>(args) {
        Ok(l) => l.value(),
        Err(e) => return e.to_compile_error().into(),
    };
    let class_upper = class_name.to_uppercase();
    let mut imp = parse_macro_input!(item as ItemImpl);
    let self_ty = (*imp.self_ty).clone();

    let mut externs = Vec::new();
    let mut members = Vec::new();

    for it in imp.items.iter_mut() {
        let ImplItem::Fn(f) = it else { continue };
        let Some(kind) = take_kind(&mut f.attrs) else {
            continue;
        };
        match gen_member(&class_name, &class_upper, &self_ty, &f.sig, kind) {
            Ok((ext, mem)) => {
                externs.push(ext);
                members.push(mem);
            }
            Err(e) => return e.to_compile_error().into(),
        }
    }

    quote! {
        #imp
        #(#externs)*
        /// Register this class + members into the engine Registry. Add to `REGISTER`.
        pub fn register(e: &mut ::rts_engine::Engine) {
            e.class(#class_name)
                #(#members)*
                .done();
        }
    }
    .into()
}

enum Kind {
    Ctor,
    Method {
        name: Option<String>,
        readonly: bool,
        private: bool,
    },
}

/// Remove and classify the `#[rtse::ctor]`/`#[rtse::method(...)]`/`#[rtse::private]`
/// marker. `None` = a plain Rust helper (left untouched).
fn take_kind(attrs: &mut Vec<syn::Attribute>) -> Option<Kind> {
    let mut kind = None;
    attrs.retain(|a| {
        let segs = &a.path().segments;
        if segs.len() == 2 && segs[0].ident == "rtse" {
            match segs[1].ident.to_string().as_str() {
                "ctor" => {
                    kind = Some(Kind::Ctor);
                    return false;
                }
                "method" => {
                    let (name, readonly) = method_args(a);
                    kind = Some(Kind::Method {
                        name,
                        readonly,
                        private: false,
                    });
                    return false;
                }
                "private" => {
                    kind = Some(Kind::Method {
                        name: None,
                        readonly: false,
                        private: true,
                    });
                    return false;
                }
                _ => {}
            }
        }
        true
    });
    kind
}

/// Parse `#[rtse::method(name = "x", readonly)]` → (Some("x"), readonly). A bare
/// `#[rtse::method]` yields (None, false).
fn method_args(a: &syn::Attribute) -> (Option<String>, bool) {
    let mut name = None;
    let mut readonly = false;
    let _ = a.parse_nested_meta(|m| {
        if m.path.is_ident("name") {
            let v = m.value()?;
            let s: syn::LitStr = v.parse()?;
            name = Some(s.value());
        } else if m.path.is_ident("readonly") {
            readonly = true;
        }
        Ok(())
    });
    (name, readonly)
}

/// AbiType token + extern Rust type + ts type-name for a supported scalar.
fn scalar(ty: &Type) -> Option<(proc_macro2::TokenStream, proc_macro2::TokenStream, &'static str)> {
    let Type::Path(p) = ty else { return None };
    let id = p.path.segments.last()?.ident.to_string();
    Some(match id.as_str() {
        "f64" => (quote!(::rts_engine::AbiType::F64), quote!(f64), "number"),
        "i64" => (quote!(::rts_engine::AbiType::I64), quote!(i64), "number"),
        "i32" => (quote!(::rts_engine::AbiType::I32), quote!(i32), "number"),
        "bool" => (quote!(::rts_engine::AbiType::Bool), quote!(i64), "boolean"),
        _ => return None,
    })
}

/// Is this return type a `String` or `&str`? (both marshal to a string handle).
fn is_string_ret(ty: &Type) -> bool {
    match ty {
        Type::Path(p) => p.path.is_ident("String"),
        Type::Reference(r) => matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")),
        _ => false,
    }
}

fn ret_ty(sig: &syn::Signature) -> Option<&Type> {
    match &sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, t) => Some(t),
    }
}

fn gen_member(
    class: &str,
    class_upper: &str,
    self_ty: &Type,
    sig: &syn::Signature,
    kind: Kind,
) -> syn::Result<(proc_macro2::TokenStream, proc_macro2::TokenStream)> {
    let rust_name = sig.ident.clone();
    let is_ctor = matches!(kind, Kind::Ctor);
    let (js_name, readonly, private) = match &kind {
        Kind::Ctor => ("new".to_string(), false, false),
        Kind::Method {
            name,
            readonly,
            private,
        } => (
            name.clone().unwrap_or_else(|| to_camel(&rust_name.to_string())),
            *readonly,
            *private,
        ),
    };
    let member_upper = rust_name.to_string().to_uppercase();
    let symbol = format!("__RTS_FN_GL_{class_upper}_{member_upper}");
    let extern_ident = format_ident!("{}", symbol);

    // Params (skip the receiver).
    let mut ext_params = Vec::new();
    let mut call_args = Vec::new();
    let mut arg_abis = Vec::new();
    let mut ts_params = Vec::new();
    if !is_ctor {
        ext_params.push(quote!(__recv: u64));
    }
    let mut idx = 0usize;
    for a in &sig.inputs {
        let FnArg::Typed(pt) = a else { continue };
        let Some((abi, ext_ty, ts)) = scalar(&pt.ty) else {
            return Err(syn::Error::new_spanned(
                &pt.ty,
                "rtse G1: param must be f64/i64/i32/bool (String/&str params: G2)",
            ));
        };
        let pid = format_ident!("__a{}", idx);
        ext_params.push(quote!(#pid: #ext_ty));
        let is_bool = matches!(&*pt.ty, Type::Path(p) if p.path.is_ident("bool"));
        call_args.push(if is_bool {
            quote!((#pid != 0))
        } else {
            quote!(#pid)
        });
        arg_abis.push(abi);
        ts_params.push(format!("a{idx}: {ts}"));
        idx += 1;
    }

    // Return marshalling.
    let (ret_abi, ret_ext_ty, ret_ts, wrap): (
        _,
        _,
        String,
        Box<dyn Fn(proc_macro2::TokenStream) -> proc_macro2::TokenStream>,
    ) = if is_ctor {
        (
            quote!(::rts_engine::AbiType::Handle),
            quote!(u64),
            class.to_string(),
            Box::new(|b| quote!(::rts_engine::heap::handles::alloc_rtse(#b))),
        )
    } else {
        match ret_ty(sig) {
            None => (
                quote!(::rts_engine::AbiType::Void),
                quote!(()),
                "void".into(),
                Box::new(|b| quote!({ #b; })),
            ),
            Some(t) if is_string_ret(t) => (
                quote!(::rts_engine::AbiType::Handle),
                quote!(u64),
                "string".into(),
                // A String / &str return → allocate into the string pool, return
                // the handle. `.to_string()` covers both `String` and `&str`.
                Box::new(|b| {
                    quote!(::rts_engine::heap::handles::alloc_entry(
                        ::rts_engine::heap::handles::Entry::String((#b).to_string().into_bytes())
                    ))
                }),
            ),
            Some(t) => {
                let Some((abi, ext_ty, ts)) = scalar(t) else {
                    return Err(syn::Error::new_spanned(
                        t,
                        "rtse G1: return must be f64/i64/i32/bool/String/&str or ()",
                    ));
                };
                let is_bool = matches!(t, Type::Path(p) if p.path.is_ident("bool"));
                (
                    abi,
                    ext_ty,
                    ts.into(),
                    if is_bool {
                        Box::new(|b| quote!((#b) as i64))
                    } else {
                        Box::new(|b| quote!(#b))
                    },
                )
            }
        }
    };

    let call = if is_ctor {
        quote!(<#self_ty>::#rust_name(#(#call_args),*))
    } else {
        quote!(
            ::rts_engine::heap::handles::with_rtse::<#self_ty, _>(__recv, |__s| match __s {
                ::core::option::Option::Some(__s) => __s.#rust_name(#(#call_args),*),
                ::core::option::Option::None => ::core::default::Default::default(),
            })
        )
    };
    let body = wrap(call);

    let extern_fn = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn #extern_ident(#(#ext_params),*) -> #ret_ext_ty {
            #body
        }
    };

    let kind_tok = if is_ctor {
        quote!(::rts_engine::MemberKind::Constructor)
    } else {
        quote!(::rts_engine::MemberKind::InstanceMethod)
    };
    let sig_args = if is_ctor {
        quote!(::std::vec![#(#arg_abis),*])
    } else {
        quote!(::std::vec![::rts_engine::AbiType::Handle #(, #arg_abis)*])
    };
    let ps = ts_params.join(", ");
    // A private member has NO ts_signature (kept out of `rts.d.ts`).
    let ts_sig = if private {
        String::new()
    } else if is_ctor {
        format!("new {class}({ps}): {class}")
    } else {
        format!("{js_name}({ps}): {ret_ts}")
    };
    let flags = if readonly {
        quote!(::rts_engine::MemberFlags::READONLY)
    } else {
        quote!(::rts_engine::MemberFlags::NONE)
    };

    let member = quote! {
        .member(::rts_engine::Member {
            name: #js_name.into(),
            kind: #kind_tok,
            sig: ::rts_engine::Sig::new(#sig_args, #ret_abi),
            symbol: #symbol.into(),
            fn_ptr: ::rts_engine::FnPtr(#extern_ident as *const u8),
            flags: #flags,
            aliases: ::std::vec::Vec::new(),
            variadic: false,
            ts_signature: #ts_sig.into(),
            doc: ::std::string::String::new(),
            pure: false,
            emit: ::core::option::Option::None,
        })
    };

    Ok((extern_fn, member))
}

fn to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for c in s.chars() {
        if c == '_' {
            up = true;
        } else if up {
            out.push(c.to_ascii_uppercase());
            up = false;
        } else {
            out.push(c);
        }
    }
    out
}

// Standalone marker attrs (inert — `#[rtse::class]` on the impl strips them).
#[proc_macro_attribute]
pub fn method(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn ctor(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn variable(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn private(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn instanceof(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn symbol(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
