//! What the attribute was given, and the two flavours a class comes in.

use proc_macro2::TokenStream;
use syn::spanned::Spanned;
use syn::{Expr, Path};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Flavour {
    /// A constructor with a prototype: `Error`, `Number`, `Boolean`.
    Class,
    /// A plain object with members on it and nothing to construct: `Math`.
    ///
    /// Not a degenerate class. `Math` has no `[[Construct]]`, no `prototype`,
    /// and `new Math()` is a `TypeError` — emitting a constructor for it would
    /// make the engine answer a question the language refuses.
    Namespace,
}

/// What the attribute was given.
pub(super) struct Options {
    /// The name JavaScript knows it by.
    pub(super) name: String,
    pub(super) flavour: Flavour,
    /// The function producing the constructor this one inherits from.
    ///
    /// A path rather than a class name, because resolving a name would mean
    /// looking one class up from inside another — and the registry that could
    /// answer that is populated in an order this expansion cannot see. A path
    /// is checked by the compiler in the same edit.
    pub(super) extends: Option<Path>,
    /// Whether `@@toStringTag` is installed, and so what
    /// `Object.prototype.toString` answers for an instance.
    ///
    /// Opt-in, and the language is why: the legacy built-ins — `Array`,
    /// `Function`, `Error`, `Boolean`, `Number`, `String`, `Date`, `RegExp` —
    /// are tagged by a table inside `Object.prototype.toString` and carry NO
    /// such property, so `Date.prototype[Symbol.toStringTag]` is `undefined`.
    /// Everything added since ES6 carries one. Defaulting to installed would
    /// put a property on eight prototypes that a real engine does not have.
    pub(super) tag: bool,
    /// Whether prototype members get their own constructible-function prototype.
    pub(super) method_prototypes: bool,
}

/// Reads `("Name")`, `("Name", namespace)`, `("Name", extends = path)`,
/// `("Name", tag)` and `("Name", method_prototypes)`.
pub(super) fn parse_options(args: TokenStream) -> syn::Result<Options> {
    let span = args.span();
    // A `macro_rules!` metavariable arrives wrapped in an invisible group, and
    // `syn` parses that group as a whole rather than as the literal inside it.
    // So a class declared through a repeating macro — which is how eight typed
    // arrays differing only in element type are written — was refused with the
    // message meant for a caller who forgot the name.
    //
    // Flattened rather than special-cased at the first token, because the same
    // wrapping reaches `extends` and `namespace`.
    let args = flattened(args);
    let parsed = syn::parse::Parser::parse2(
        syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated,
        args,
    )?;
    let mut parts = parsed.iter();

    let Some(Expr::Lit(first)) = parts.next() else {
        return Err(syn::Error::new(
            span,
            "a class declares the name JavaScript knows it by: \
             `#[rtse::class(\"Error\")]`",
        ));
    };
    let syn::Lit::Str(name) = &first.lit else {
        return Err(syn::Error::new(first.span(), "the name is a string literal"));
    };

    let mut options = Options {
        name: name.value(),
        flavour: Flavour::Class,
        extends: None,
        tag: false,
        method_prototypes: false,
    };

    for part in parts {
        match part {
            Expr::Path(path) if path.path.is_ident("namespace") => {
                options.flavour = Flavour::Namespace;
            }
            Expr::Path(path) if path.path.is_ident("tag") => {
                options.tag = true;
            }
            Expr::Path(path) if path.path.is_ident("method_prototypes") => {
                options.method_prototypes = true;
            }
            Expr::Assign(assign) => {
                let Expr::Path(left) = assign.left.as_ref() else {
                    return Err(syn::Error::new(assign.span(), "unknown option"));
                };
                if !left.path.is_ident("extends") {
                    return Err(syn::Error::new(assign.span(), "unknown option"));
                }
                let Expr::Path(right) = assign.right.as_ref() else {
                    return Err(syn::Error::new(
                        assign.right.span(),
                        "`extends` names the function producing the parent \
                         constructor, so the compiler checks it in this edit",
                    ));
                };
                options.extends = Some(right.path.clone());
            }
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "the options are `namespace`, `tag`, `method_prototypes` and `extends = path`",
                ));
            }
        }
    }

    Ok(options)
}

/// The same tokens with every invisible group opened out.
///
/// `proc_macro2::Delimiter::None` is what a `macro_rules!` substitution leaves
/// around a captured fragment. It exists so that `$a * $b` keeps its grouping,
/// and it means an attribute reading its own arguments sees one opaque token
/// where the author wrote a string literal.
fn flattened(tokens: TokenStream) -> TokenStream {
    tokens
        .into_iter()
        .flat_map(|tree| match tree {
            proc_macro2::TokenTree::Group(group)
                if group.delimiter() == proc_macro2::Delimiter::None =>
            {
                flattened(group.stream())
            }
            other => TokenStream::from(other),
        })
        .collect()
}
