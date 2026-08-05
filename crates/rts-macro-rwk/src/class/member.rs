//! One member of a declared class: which list it goes in, and the wrapper that
//! gives its body the shape a compiled function has.
//!
//! Split from the expansion because this is where the two decisions that are not
//! volume live — where the receiver comes from, and where each argument's borrow
//! is taken and given back.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{FnArg, ImplItemConst, ImplItemFn, ReturnType};

use super::{camel_of, is_named, spelled};

/// Which of the three lists a member goes in.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Role {
    /// On the prototype.
    Member,
    /// On the constructor.
    Static,
    /// The code `new C()` runs.
    Construct,
}

/// One member, and everything the expansion needs to know about it.
pub(super) struct Member {
    /// The name JavaScript knows it by.
    pub(super) js: String,
    /// The `extern "C"` wrapper's Rust name.
    pub(super) wrapper: syn::Ident,
    pub(super) role: Role,
}

impl Member {
    /// Reads the attributes and the name off a declared function.
    pub(super) fn read(function: &ImplItemFn, prefix: &str) -> syn::Result<Self> {
        let mut role = Role::Member;
        let mut js = camel_of(&function.sig.ident.to_string());

        for attribute in &function.attrs {
            if attribute.path().is_ident("stat") {
                role = Role::Static;
            } else if attribute.path().is_ident("construct") {
                role = Role::Construct;
            } else if attribute.path().is_ident("js") {
                js = attribute.parse_args::<syn::LitStr>()?.value();
            }
        }

        Ok(Member {
            js,
            wrapper: format_ident!("__{prefix}_{}", function.sig.ident),
            role,
        })
    }

    /// The `(name, wrapper)` pair the install list holds.
    pub(super) fn row(&self) -> TokenStream {
        let js = &self.js;
        let wrapper = &self.wrapper;
        quote!((#js, #wrapper))
    }

    /// The author's function, plus the wrapper that gives it the shape a
    /// compiled function has.
    pub(super) fn expand(&self, function: &ImplItemFn) -> TokenStream {
        let wrapper = &self.wrapper;
        let inner = format_ident!("{}_body", self.wrapper);

        let mut takes_this = false;
        let mut coercions = Vec::new();
        let mut forwarded = Vec::new();
        let slots = [
            format_ident!("a0"),
            format_ident!("a1"),
            format_ident!("a2"),
            format_ident!("a3"),
        ];

        for (position, argument) in function.sig.inputs.iter().enumerate() {
            let FnArg::Typed(typed) = argument else {
                continue;
            };
            if position == 0 && is_named(&typed.pat, "this") {
                takes_this = true;
                forwarded.push(quote!(this));
                continue;
            }
            let index = position - usize::from(takes_this);
            let Some(slot) = slots.get(index) else {
                return syn::Error::new(
                    typed.span(),
                    "a call carries four arguments and a receiver; a member \
                     wanting more reads the argument vector rather than \
                     declaring a fifth parameter",
                )
                .to_compile_error();
            };
            let local = format_ident!("__arg{index}");
            let ty = &typed.ty;
            // Each conversion takes its own borrow and gives it back. That is
            // what leaves the author's body running with none held, which is the
            // whole reason this wrapper is generated rather than written.
            let converted = match spelled(ty).as_str() {
                "u64" => quote!(#slot),
                "f64" => quote!(crate::entry::class_support::to_number(#slot)),
                "bool" => quote!(crate::entry::class_support::to_boolean(#slot)),
                other => {
                    return syn::Error::new(
                        ty.span(),
                        format!(
                            "`{other}` has no decided coercion from a JavaScript \
                             value. The types that do are u64 (the value as it \
                             arrived), f64 (ToNumber) and bool (ToBoolean)."
                        ),
                    )
                    .to_compile_error();
                }
            };
            coercions.push(quote!(let #local = #converted;));
            forwarded.push(quote!(#local));
        }

        let packed = match &function.sig.output {
            // Refused rather than filled in with `undefined`. Every JavaScript
            // function answers something, so a member that returned nothing
            // would be one whose answer the attribute chose — and `undefined`
            // is a value a program branches on, not an absence.
            ReturnType::Default => {
                return syn::Error::new(
                    function.sig.span(),
                    "a member answers a value: give it a return type, `u64` \
                     for the receiver or `undefined` when there is nothing else",
                )
                .to_compile_error();
            }
            ReturnType::Type(_, ty) => {
                let call = quote!(#inner(#(#forwarded),*));
                match spelled(ty).as_str() {
                    "u64" => call,
                    "f64" => quote!(crate::value::Value::from_f64(#call).bits()),
                    "bool" => quote!(crate::value::Value::from_bool(#call).bits()),
                    other => {
                        return syn::Error::new(
                            ty.span(),
                            format!(
                                "`{other}` has no decided packing into a \
                                 JavaScript value. The types that do are u64, \
                                 f64 and bool."
                            ),
                        )
                        .to_compile_error();
                    }
                }
            }
        };

        let attrs: Vec<_> = function
            .attrs
            .iter()
            .filter(|attribute| {
                !(attribute.path().is_ident("stat")
                    || attribute.path().is_ident("construct")
                    || attribute.path().is_ident("js"))
            })
            .collect();
        let signature = &function.sig;
        let block = &function.block;
        let renamed = {
            let mut signature = signature.clone();
            signature.ident = inner.clone();
            signature
        };
        let this = match takes_this {
            true => quote!(this),
            false => quote!(_this),
        };

        quote! {
            #(#attrs)*
            #renamed #block

            /// The shape a compiled function has, over the member above.
            extern "C" fn #wrapper(
                _e: u64,
                #this: u64,
                a0: u64,
                a1: u64,
                a2: u64,
                a3: u64,
            ) -> u64 {
                let _ = (a0, a1, a2, a3);
                #(#coercions)*
                #packed
            }
        }
    }
}

/// A `const NAME: … = …` as the pair the install list holds, and where it goes.
///
/// `#[stat]` puts it on the constructor, exactly as it does for a function.
/// Which half a constant belongs to is not a detail: `Number.MAX_SAFE_INTEGER`
/// is on the constructor and `Error.prototype.name` is on the prototype, and an
/// attribute that guessed would be wrong for one of them every time.
pub(super) fn constant_row(constant: &ImplItemConst) -> syn::Result<(TokenStream, bool)> {
    let value = &constant.expr;
    let held = match spelled(&constant.ty).as_str() {
        "f64" => quote!(crate::entry::class_support::Constant::Number(#value)),
        "&str" | "&'staticstr" => {
            quote!(crate::entry::class_support::Constant::Text(#value))
        }
        other => {
            return Err(syn::Error::new(
                constant.ty.span(),
                format!(
                    "`{other}` has no decided spelling as a property. A class \
                     constant is an `f64` or a `&'static str`."
                ),
            ));
        }
    };
    // The Rust name unchanged. `PI`, `E`, `LN2`, `MAX_SAFE_INTEGER` — every
    // constant JavaScript has is already spelled this way, so translating the
    // case would be inventing a difference to undo.
    let name = constant.ident.to_string();
    let statics = constant
        .attrs
        .iter()
        .any(|attribute| attribute.path().is_ident("stat"));
    Ok((quote!((#name, #held)), statics))
}

