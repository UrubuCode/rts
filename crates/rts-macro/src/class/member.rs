//! One member of a declared class: which list it goes in, and the wrapper that
//! gives its body the shape a compiled function has.
//!
//! Split from the expansion because this is where the two decisions that are not
//! volume live — where the receiver comes from, and where each argument's borrow
//! is taken and given back.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{FnArg, ImplItemConst, ImplItemFn, Pat, ReturnType};

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
    /// What a program calling it writes, derived from the Rust signature.
    pub(super) ts: String,
    /// The `///` comments above it, as written.
    pub(super) doc: String,
    /// What `fn.length` answers — `SetFunctionLength`'s arity.
    ///
    /// Derived from the Rust signature, because that is the one place the two
    /// cannot drift: a member taking `(x: f64)` has arity 1 and nothing has to
    /// remember to say so. `#[arity(n)]` overrides it, and exists for exactly
    /// one shape — a VARIADIC, which reads four `u64` slots regardless of the
    /// arity the language pins. `Math.max` takes four slots here and answers
    /// `2`, which is the specification's number and not a count of anything in
    /// the Rust signature.
    ///
    /// `this` is excluded for the reason [`ts_signature`] excludes it: a
    /// receiver is not an argument.
    pub(super) arity: u32,
}

impl Member {
    /// Reads the attributes and the name off a declared function.
    pub(super) fn read(function: &ImplItemFn, prefix: &str) -> syn::Result<Self> {
        let mut role = Role::Member;
        let mut js = camel_of(&function.sig.ident.to_string());
        let mut arity = None;

        for attribute in &function.attrs {
            if attribute.path().is_ident("stat") {
                role = Role::Static;
            } else if attribute.path().is_ident("construct") {
                role = Role::Construct;
            } else if attribute.path().is_ident("js") {
                js = attribute.parse_args::<syn::LitStr>()?.value();
            } else if attribute.path().is_ident("arity") {
                arity = Some(attribute.parse_args::<syn::LitInt>()?.base10_parse::<u32>()?);
            }
        }

        let ts = ts_signature(&js, function);
        let arity = arity.unwrap_or_else(|| declared_arity(function));
        Ok(Member {
            js,
            wrapper: format_ident!("__{prefix}_{}", function.sig.ident),
            role,
            ts,
            doc: doc_of(&function.attrs),
            arity,
        })
    }

    /// The row the declared-type list holds: what to write, and what it means.
    pub(super) fn type_row(&self) -> TokenStream {
        let ts = &self.ts;
        let doc = &self.doc;
        let role = match self.role {
            Role::Member => quote!(crate::entry::declared::Role::Prototype),
            Role::Static => quote!(crate::entry::declared::Role::Static),
            Role::Construct => quote!(crate::entry::declared::Role::Construct),
        };
        quote!(crate::entry::declared::Member {
            signature: #ts,
            doc: #doc,
            role: #role,
        })
    }

    /// The `(name, wrapper, arity)` row the install list holds.
    ///
    /// The arity rides along so that every declared built-in gets a `.length`.
    /// It was absent, and `undefined` is not a smaller answer than a number:
    /// `Math.max.length` participates in arithmetic, and a currying or
    /// forwarding helper that reads it got `NaN`. Six fixtures failed on exactly
    /// this line and nothing else.
    pub(super) fn row(&self) -> TokenStream {
        let js = &self.js;
        let wrapper = &self.wrapper;
        let arity = self.arity;
        quote!((#js, #wrapper, #arity))
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
                    || attribute.path().is_ident("js")
                    || attribute.path().is_ident("arity"))
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

/// What a program writes to call this member, derived from the Rust one.
///
/// # Why it is derived rather than declared
///
/// The alternative is an attribute carrying the TypeScript text — and that is a
/// second statement of the same function, sitting one line above the first, with
/// nothing keeping them in agreement. The repository's own rule is that a
/// declaration is written once and the views are generated from it; an
/// annotation that says `push(x: number): number` over a body taking `u64`
/// would be the first thing to go stale, and stale is worse than imprecise
/// because a reader cannot tell.
///
/// # What is lost, and why that is the honest answer
///
/// `u64` is the value exactly as it arrived, so it becomes `any`. It is the
/// only spelling available: the coercion the wrapper performs is where a
/// narrower type would come from, and for `u64` there is none — the member
/// itself decides what it accepts. Saying `string` there would be a claim this
/// layer cannot check, and a `.d.ts` that promises a type the runtime does not
/// enforce is worse than one that admits `any`.
///
/// `f64` becomes `number` and `bool` becomes `boolean`, both exactly true: the
/// wrapper runs `ToNumber`/`ToBoolean` before the body sees the argument, so
/// every value IS one by the time it arrives.
fn ts_signature(js: &str, function: &ImplItemFn) -> String {
    let mut params = Vec::new();
    for (position, argument) in function.sig.inputs.iter().enumerate() {
        let FnArg::Typed(typed) = argument else {
            continue;
        };
        // The receiver is not an argument. A `.d.ts` writes `this` only where it
        // constrains the receiver, and nothing here does.
        if position == 0 && is_named(&typed.pat, "this") {
            continue;
        }
        let name = match &*typed.pat {
            Pat::Ident(ident) => camel_of(&ident.ident.to_string()),
            // A pattern rather than a name binds nothing a caller could be told
            // about, so the position is what identifies it.
            _ => format!("arg{position}"),
        };
        // Every parameter is OPTIONAL, and that is what the call actually
        // permits rather than a convenience. A compiled call carries four
        // argument slots; an omitted one arrives as `undefined` and the member
        // decides what that means — `Buffer.alloc(8)` runs, and so does
        // `big.toString()`. Declaring them required refuses both at the type
        // checker while the runtime accepts them, which is the one kind of
        // wrongness a `.d.ts` cannot be forgiven for: it stops correct programs.
        params.push(format!("{name}?: {}", ts_type(&typed.ty)));
    }
    let returns = match &function.sig.output {
        ReturnType::Default => "void".to_owned(),
        ReturnType::Type(_, ty) => ts_type(ty),
    };
    format!("{js}({}): {returns}", params.join(", "))
}

/// The three types a member can spell, as TypeScript spells them.
///
/// Anything else is not reachable: `Member::expand` refuses it with a message
/// naming the three, so this cannot be asked about a type that compiled.
fn ts_type(ty: &syn::Type) -> String {
    match spelled(ty).as_str() {
        "f64" => "number".to_owned(),
        "bool" => "boolean".to_owned(),
        _ => "any".to_owned(),
    }
}

/// The `///` comments above an item, joined, with the leading space dropped.
///
/// Kept as the author wrote it, including the backticked JavaScript spelling
/// most of them open with — that line is frequently more precise than the
/// derived signature (it names what a `u64` actually accepts), and the two
/// beside each other is the point.
pub(super) fn doc_of(attrs: &[syn::Attribute]) -> String {
    let mut lines = Vec::new();
    for attribute in attrs {
        if !attribute.path().is_ident("doc") {
            continue;
        }
        let syn::Meta::NameValue(pair) = &attribute.meta else {
            continue;
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(text),
            ..
        }) = &pair.value
        else {
            continue;
        };
        lines.push(text.value().trim_start().to_owned());
    }
    lines.join("\n")
}

/// How many arguments the Rust signature declares, not counting the receiver.
///
/// This is `fn.length` for every member that is not variadic, and it is derived
/// rather than written for the same reason the TypeScript signature is: a number
/// stated twice is a number that will be stated differently. A variadic says so
/// with `#[arity(n)]`, because its four `u64` slots are a calling convention and
/// not a count of what the language pins.
fn declared_arity(function: &ImplItemFn) -> u32 {
    let mut count = 0;
    for (position, argument) in function.sig.inputs.iter().enumerate() {
        let FnArg::Typed(typed) = argument else {
            continue;
        };
        if position == 0 && is_named(&typed.pat, "this") {
            continue;
        }
        count += 1;
    }
    count
}

/// The same constant as the row the declared-type list holds.
///
/// A property rather than a call, so it is spelled without parentheses. Its type
/// follows the same rule the install does: an `f64` is a `number` and a
/// `&'static str` is a `string`, and there is no third case.
pub(super) fn constant_type_row(constant: &ImplItemConst) -> syn::Result<TokenStream> {
    let name = constant.ident.to_string();
    let ts = match spelled(&constant.ty).as_str() {
        "f64" => format!("{name}: number"),
        _ => format!("{name}: string"),
    };
    let doc = doc_of(&constant.attrs);
    let role = match constant
        .attrs
        .iter()
        .any(|attribute| attribute.path().is_ident("stat"))
    {
        true => quote!(crate::entry::declared::Role::StaticConstant),
        false => quote!(crate::entry::declared::Role::Constant),
    };
    Ok(quote!(crate::entry::declared::Member {
        signature: #ts,
        doc: #doc,
        role: #role,
    }))
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

