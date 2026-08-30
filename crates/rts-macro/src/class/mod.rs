//! Declaring a built-in class, and deriving the wrappers it would be written as.
//!
//! # What this removes, and why that is the interesting part
//!
//! Every method in `entry/string/basic.rs` and `entry/array_proto/` is written
//! twice: once as the operation, and once as an `extern "C" fn(env, this, a0,
//! a1, a2, a3) -> u64` that unpacks the receiver, coerces the arguments and
//! packs the answer. The second half is identical every time, which is the
//! definition of what an attribute should be writing.
//!
//! It is also where the one mistake that is not a wrong answer lives.
//! `with_current` holds a `RefCell` borrow for the length of its body, and a
//! member that calls user code from inside one deadlocks — reproducibly only
//! when the callee happens to touch the runtime. The generated wrapper is
//! therefore written as **coerce, drop the borrow, call**: every argument is
//! converted through its own short borrow, and the author's body runs with none
//! held. The trap is removed by the shape of the expansion rather than by a
//! comment asking for care.
//!
//! # What a member looks like
//!
//! ```ignore
//! #[rtse::class("Math", namespace)]
//! impl Math {
//!     const PI: f64 = std::f64::consts::PI;
//!
//!     /// `Math.floor(x)`.
//!     fn floor(x: f64) -> f64 { x.floor() }
//! }
//! ```
//!
//! A first parameter named `this` receives the receiver, untouched. Every other
//! parameter is an argument, coerced from its Rust type: `u64` is the value as
//! it arrived, `f64` is `ToNumber` of it. The return is packed the same way.
//!
//! # What it does not do
//!
//! Decide where the class is installed, or number anything. A proc macro sees
//! one item and cannot see its neighbours — which is why `register` is a
//! function something else calls rather than a registration that happens by
//! itself. A distributed slice would collect at link time in an order the linker
//! picks, which is neither deterministic across platforms nor visible in a diff.

mod member;
mod options;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{ImplItem, ItemImpl, Pat, Path, Type};

use member::{Member, Role, constant_row, constant_type_row, doc_of};
use options::{Flavour, parse_options};

/// Expand `#[rtse::class]`.
pub fn expand(args: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let block: ItemImpl = syn::parse2(item)?;
    let options = parse_options(args)?;

    // From the type the block is written over, not from the JavaScript name.
    //
    // That is the whole job the type ident has — nothing named `Math` is
    // emitted — and it exists because the two are not the same string:
    // `URIError` snake-cases to `u_r_i_error`, while `impl UriError` gives the
    // `register_uri_error` a caller wants to write.
    let prefix = snake_of(&type_ident(&block)?.to_string());
    let mut members = Vec::new();
    let mut statics = Vec::new();
    let mut constants = Vec::new();
    let mut static_constants = Vec::new();
    let mut construct = None;
    let mut emitted = Vec::new();
    let mut type_rows = Vec::new();

    for item in &block.items {
        match item {
            ImplItem::Fn(function) => {
                let member = Member::read(function, &prefix)?;
                emitted.push(member.expand(function));
                type_rows.push(member.type_row());
                let row = member.row();
                match (member.role, options.flavour) {
                    (Role::Construct, Flavour::Namespace) => {
                        return Err(syn::Error::new(
                            function.span(),
                            "a namespace has nothing to construct: `Math` has no \
                             [[Construct]], and `new Math()` is a TypeError",
                        ));
                    }
                    (Role::Construct, _) => construct = Some((member.wrapper.clone(), member.arity)),
                    (Role::Static, _) => statics.push(row),
                    (Role::Member, _) => members.push(row),
                }
            }
            ImplItem::Const(constant) => {
                type_rows.push(constant_type_row(constant)?);
                match constant_row(constant)? {
                    (row, true) => static_constants.push(row),
                    (row, false) => constants.push(row),
                }
            }
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "a class holds functions and `f64` constants; anything else \
                     has no installed form",
                ));
            }
        }
    }

    let natives = format_ident!("{}_NATIVES", prefix.to_uppercase());
    let statics_name = format_ident!("{}_STATICS", prefix.to_uppercase());
    let constants_name = format_ident!("{}_CONSTANTS", prefix.to_uppercase());
    let static_constants_name = format_ident!("{}_STATIC_CONSTANTS", prefix.to_uppercase());
    let register = format_ident!("register_{prefix}");

    let name = &options.name;
    // A namespace installs its members on the object itself, so a `#[stat]`
    // there would be a second list nothing reads.
    if options.flavour == Flavour::Namespace && !(statics.is_empty() && static_constants.is_empty())
    {
        return Err(syn::Error::new(
            block.span(),
            "a namespace has one set of members, installed on the object: \
             `#[stat]` marks the constructor half, and there is no constructor",
        ));
    }

    let body = match options.flavour {
        Flavour::Namespace => namespace_body(name, &natives, &constants_name, options.tag),
        Flavour::Class => class_body(
            name,
            &natives,
            &statics_name,
            &constants_name,
            &static_constants_name,
            construct.as_ref().map(|(wrapper, arity)| (wrapper, *arity)),
            options.extends.as_ref(),
            &prefix,
            options.tag,
            options.method_prototypes,
        ),
    };

    let statics_decl = match options.flavour {
        Flavour::Namespace => quote!(),
        Flavour::Class => quote! {
            /// What the constructor holds.
            const #statics_name: &[(&str, ::rts_core::entry::class_abi::Native, u32)] =
                &[#(#statics),*];

            /// The constructor's own constants.
            const #static_constants_name:
                &[(&str, ::rts_core::entry::class_abi::Constant)] =
                &[#(#static_constants),*];
        },
    };

    let doc = format!("Installs `{name}`, once, and answers the value the name reads.");
    let types_name = format_ident!("{}_TYPES", prefix.to_uppercase());
    let types_doc = format!("What a program calling `{name}` writes. See `entry::declared`.");
    let class_doc = doc_of(&block.attrs);
    let namespace = options.flavour == Flavour::Namespace;
    // The parent as a JavaScript NAME, derived from the path `extends` names.
    // The option carries a function because a name could not be checked in the
    // same edit (see `options.rs`), and the two spellings meet here: a register
    // function is `register_<prefix>` and a prefix is the snake case of the Rust
    // type, so undoing both recovers the type ident — `register_type_error` →
    // `TypeError`, `uint8_array` → `Uint8Array`.
    //
    // That is a derivation and derivations are wrong eventually, so it is not
    // trusted: `entry::declared` prints an `extends` only when the derived name
    // is itself a class this engine declares, and drops it otherwise. A missing
    // `extends` in a `.d.ts` under-describes; a wrong one names a type that does
    // not exist and refuses to compile.
    let extends = match options.extends.as_ref().and_then(|path| path.segments.last()) {
        Some(segment) => {
            let parent = pascal_of(
                segment
                    .ident
                    .to_string()
                    .strip_prefix("register_")
                    .unwrap_or(&segment.ident.to_string()),
            );
            quote!(Some(#parent))
        }
        None => quote!(None),
    };

    Ok(quote! {
        #(#emitted)*

        #[doc = #types_doc]
        pub(crate) const #types_name: ::rts_core::entry::declared::Class =
            ::rts_core::entry::declared::Class {
                name: #name,
                doc: #class_doc,
                namespace: #namespace,
                extends: #extends,
                members: &[#(#type_rows),*],
            };

        /// What the prototype holds — or, for a namespace, the object itself.
        const #natives: &[(&str, ::rts_core::entry::class_abi::Native, u32)] = &[#(#members),*];

        #statics_decl

        /// The constants, installed as ordinary properties.
        const #constants_name: &[(&str, ::rts_core::entry::class_abi::Constant)] =
            &[#(#constants),*];

        #[doc = #doc]
        pub(crate) fn #register(
            context: &mut ::rts_core::entry::Context,
        ) -> u64 {
            #body
        }
    })
}

/// The registration a namespace gets: one object, its members, its constants.
fn namespace_body(
    name: &str,
    natives: &syn::Ident,
    constants: &syn::Ident,
    tag: bool,
) -> TokenStream {
    let tagged = tagging(name, quote!(cell), tag);
    quote! {
        if let Some(made) = ::rts_core::entry::class_abi::made(context, #name) {
            return made;
        }
        let Some(cell) = ::rts_core::entry::class_abi::plain(context) else {
            return ::rts_core::entry::class_abi::undefined_of(context);
        };
        let object = ::rts_core::value::Value::from_slot(cell).bits();
        // Recorded BEFORE anything is installed. Installing interns names, and
        // interning allocates — the reason `string::prototype_of` records its
        // cell first, and the reason that version recursed until the region ran
        // out when it did not.
        ::rts_core::entry::class_abi::record(context, #name, object, object, Some("rts-core::class"));
        ::rts_core::entry::class_abi::install_with_arity(context, cell, #natives);
        ::rts_core::entry::class_abi::constants(context, cell, #constants);
        #tagged
        object
    }
}

/// The registration a class gets: a constructor, a prototype, and the link.
fn class_body(
    name: &str,
    natives: &syn::Ident,
    statics: &syn::Ident,
    constants: &syn::Ident,
    static_constants: &syn::Ident,
    construct: Option<(&syn::Ident, u32)>,
    extends: Option<&Path>,
    prefix: &str,
    tag: bool,
    method_prototypes: bool,
) -> TokenStream {
    let tagged = tagging(name, quote!(prototype_cell), tag);
    let install_members = if method_prototypes {
        quote! {
            ::rts_core::entry::class_abi::install_with_arity_and_prototypes(
                context, prototype_cell, #natives,
            );
        }
    } else {
        quote! {
            ::rts_core::entry::class_abi::install_with_arity(context, prototype_cell, #natives);
        }
    };
    // A class with no constructor of its own still has to be callable, because
    // `new C()` runs one. The default keeps the object `construct` made, which
    // is what a JavaScript constructor with an empty body does.
    let code = match construct {
        Some((wrapper, _)) => quote!(#wrapper),
        None => {
            let default = format_ident!("__{prefix}_construct_default");
            quote!(#default)
        }
    };
    let construct_arity = construct.map_or(0u32, |(_, arity)| arity);
    let default = match construct {
        Some(_) => quote!(),
        None => {
            let ident = format_ident!("__{prefix}_construct_default");
            quote! {
                /// `new C()` for a class that declared no constructor: the
                /// object `construct` already made is the answer.
                extern "C" fn #ident(
                    _e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64,
                ) -> u64 { this }
            }
        }
    };

    let inherit = match extends {
        None => quote!(),
        Some(path) => quote! {
            // The parent's prototype, reached through the parent's own
            // registration rather than through a name: `TypeError.prototype`
            // inherits from `Error.prototype`, which is what makes
            // `new TypeError("x").message` find the accessor Error installed.
            let parent = #path(context);
            if let Some(parent_cell) = ::rts_core::value::Value(parent).as_slot() {
                let key = context.well_known("prototype");
                if let Some(found) =
                    ::rts_core::entry::class_abi::read_property(context, parent_cell, key)
                {
                    context.set_prototype(prototype_cell, found.bits());
                }
            }
            // And the constructor inherits the parent's statics, the same way
            // `class B extends A {}` gives `B` everything `A` had.
            if let Some(cell) = ::rts_core::value::Value(callable).as_slot() {
                context.set_prototype(cell, parent);
            }
        },
    };

    quote! {
        #default

        if let Some(made) = ::rts_core::entry::class_abi::made(context, #name) {
            return made;
        }
        let Some(prototype_cell) = ::rts_core::entry::class_abi::plain(context) else {
            return ::rts_core::entry::class_abi::undefined_of(context);
        };
        let prototype = ::rts_core::value::Value::from_slot(prototype_cell).bits();
        let callable = ::rts_core::entry::class_abi::callable(context, #code);
        // `C.name` e uma propriedade real, como em qualquer funcao: sem ela
        // `new Error("x").constructor.name` respondia vazio, e o inspetor do
        // console nao tinha como rotular uma instancia.
        ::rts_core::entry::class_abi::name_of(context, callable, #name);
        // `C.length` is `SetFunctionLength` over the constructor, exactly as it
        // is over a method: what `new C(…)` declares. Without it a program
        // reading `Map.length` saw `undefined` where every runtime answers `0`,
        // and `Boolean.length` where every runtime answers `1`.
        ::rts_core::entry::class_abi::length_of(context, callable, #construct_arity);
        // Before installing anything, for the reason the namespace body states:
        // installing interns, interning allocates, and an allocation can reach
        // back here.
        ::rts_core::entry::class_abi::record(context, #name, callable, prototype, Some("rts-core::class"));

        if let Some(cell) = ::rts_core::value::Value(callable).as_slot() {
            ::rts_core::entry::class_abi::install_with_arity(context, cell, #statics);
            ::rts_core::entry::class_abi::constants(context, cell, #static_constants);
            let key = context.well_known("prototype");
            ::rts_core::entry::class_abi::put(context, cell, key, prototype);
            // `{ writable: false, enumerable: false, configurable: false }` — a
            // built-in constructor's `prototype` is the one property the
            // specification nails down completely. Unmarked it was enumerable,
            // so `Object.keys(Boolean)` answered `["prototype"]` where every
            // runtime answers `[]`, and `for (const k in Map)` walked it.
            ::rts_core::entry::class_abi::pinned(context, cell, key);
        }
        #install_members
        ::rts_core::entry::class_abi::constants(context, prototype_cell, #constants);
        // `p.constructor` is an ordinary property, and a program reads it.
        let key = context.well_known("constructor");
        ::rts_core::entry::class_abi::put(context, prototype_cell, key, callable);
        ::rts_core::entry::class_abi::hidden(context, prototype_cell, key);

        #tagged
        #inherit

        callable
    }
}

/// `X.prototype[Symbol.toStringTag] = "X"`, for a class that declares `tag`.
///
/// # Why the tag is a property and not a list inside `toString`
///
/// It was a list — twenty class names inside `Object.prototype.toString`, walked
/// until one of their prototypes matched. That answered the same string for the
/// built-ins and was wrong about the mechanism in three ways a program can see:
/// `Map.prototype[Symbol.toStringTag]` read `undefined` where the language says
/// `"Map"`, `Object.getOwnPropertySymbols(Map.prototype)` was empty, and every
/// class added afterwards would have had to be remembered in a second place.
///
/// A property also gets inheritance for free, which the list only imitated: a
/// subclass of `Map` answers `[object Map]` because it INHERITS the tag, not
/// because a walk in the runtime recognised its parent.
fn tagging(name: &str, cell: TokenStream, tag: bool) -> TokenStream {
    if !tag {
        return quote!();
    }
    quote! {
        let key = context.well_known(concat!("@@toStringTag"));
        let tag = context.intern_value(::rts_core::text::Str::from_str(#name)).bits();
        ::rts_core::entry::class_abi::put(context, #cell, key, tag);
        // `{ writable: false, enumerable: false, configurable: true }`, which is
        // what the specification gives every `@@toStringTag`. Unmarked it was
        // writable AND enumerable, so `Object.keys(Math)` listed a symbol-keyed
        // property and `Math[Symbol.toStringTag] = "x"` silently retagged the
        // namespace.
        ::rts_core::entry::class_abi::tagged(context, #cell, key);
    }
}

/// The name the block is written over, which is what the emitted names use.
fn type_ident(block: &ItemImpl) -> syn::Result<syn::Ident> {
    let Type::Path(path) = block.self_ty.as_ref() else {
        return Err(syn::Error::new(
            block.self_ty.span(),
            "a class block is written over a plain name: `impl Math { … }`",
        ));
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.clone())
        .ok_or_else(|| syn::Error::new(block.self_ty.span(), "an empty path names nothing"))
}

/// Whether a pattern is exactly this identifier.
fn is_named(pattern: &Pat, name: &str) -> bool {
    matches!(pattern, Pat::Ident(ident) if ident.ident == name)
}

/// A type as written, with the spaces the tokeniser inserted removed.
fn spelled(ty: &Type) -> String {
    quote!(#ty).to_string().replace(' ', "")
}

/// `TypeError` → `type_error`, which is what the generated names are built from.
fn snake_of(name: &str) -> String {
    let mut out = String::new();
    for (position, character) in name.chars().enumerate() {
        if character.is_uppercase() && position != 0 {
            out.push('_');
        }
        out.extend(character.to_lowercase());
    }
    out
}

/// `type_error` → `TypeError`: the Rust type ident a prefix was made from.
///
/// The inverse of [`snake_of`] on the cases it is used for, and only those.
fn pascal_of(name: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for character in name.chars() {
        if character == '_' {
            upper = true;
            continue;
        }
        match upper {
            true => out.extend(character.to_uppercase()),
            false => out.push(character),
        }
        upper = false;
    }
    out
}

/// `to_string` → `toString`, which is what JavaScript calls it.
///
/// Overridable with `#[js("…")]`, because the mapping is not total: `charCodeAt`
/// comes back as written, and a name with a digit in it does not survive.
fn camel_of(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for character in name.chars() {
        if character == '_' {
            upper = true;
            continue;
        }
        match upper {
            true => out.extend(character.to_uppercase()),
            false => out.push(character),
        }
        upper = false;
    }
    out
}
