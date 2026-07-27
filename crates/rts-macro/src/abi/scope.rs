//! `#[rtse::abi(...)]` argument PARSING. The naming RULE itself (the enums,
//! `symbol_for`, `segments`) lives in `rts_abi::scope` — see that module for the
//! full convention doc and the `__rtsm_`/`__rtsn_`/`__rtsa_` table. This file
//! only owns the `syn::Parse` impl, because a `Parse` impl needs a local type
//! and a proc-macro crate cannot be depended on as an ordinary library (so the
//! symbol baker, which needs the same rule, calls `rts_abi::scope` directly).

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};

pub(crate) use rts_abi::scope::Scope;
pub(crate) use rts_abi::scope::symbol_for;

/// Parsed `#[rtse::abi(...)]` arguments. A newtype over the shared
/// [`rts_abi::scope::Naming`]: the naming RULE lives in `rts-abi` (so the symbol
/// baker can call the same function), while the `syn` parsing stays here.
pub(crate) struct AbiArgs(pub(crate) rts_abi::scope::Naming);

impl Parse for AbiArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        use rts_abi::scope::Naming;

        if input.is_empty() {
            return Ok(AbiArgs(Naming::Verbatim));
        }
        if input.peek(syn::LitStr) {
            let s: syn::LitStr = input.parse()?;
            if !input.is_empty() {
                return Err(syn::Error::new_spanned(
                    &s,
                    "#[rtse::abi]: an explicit symbol takes no other arguments",
                ));
            }
            return Ok(AbiArgs(Naming::Explicit(s.value())));
        }

        let mut scope: Option<Scope> = None;
        let mut value: Option<String> = None;
        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            let arg: Option<String> = if input.peek(syn::Token![=]) {
                input.parse::<syn::Token![=]>()?;
                let v: syn::LitStr = input.parse()?;
                Some(v.value())
            } else {
                None
            };
            match (key.to_string().as_str(), arg) {
                ("module", Some(m)) => scope = Some(Scope::Module(m)),
                ("global", g) => scope = Some(Scope::Global(g)),
                ("native", None) => scope = Some(Scope::Native),
                ("abi", None) => scope = Some(Scope::Abi),
                ("value", Some(v)) => value = Some(v),
                ("module", None) => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        "#[rtse::abi]: `module` needs a specifier — `module = \"node:fs\"`",
                    ));
                }
                ("native", Some(_)) | ("abi", Some(_)) => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        "#[rtse::abi]: `native`/`abi` are bare flags — put the name in `value = \"…\"`",
                    ));
                }
                ("value", None) => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        "#[rtse::abi]: `value` needs a name — `value = \"readFileSync\"`",
                    ));
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        "#[rtse::abi]: unknown arg (expected `module`, `global`, `native`, \
                         `abi` or `value`)",
                    ));
                }
            }
            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            } else {
                break;
            }
        }

        match scope {
            Some(scope) => Ok(AbiArgs(Naming::Scoped { scope, value })),
            None => Err(syn::Error::new(
                Span::call_site(),
                "#[rtse::abi]: missing scope — one of `module = \"…\"`, `global`, `native`, `abi`",
            )),
        }
    }
}
