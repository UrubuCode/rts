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

/// Parsed `#[rtse::abi(...)]` / `#[rtse::function(...)]` arguments.
///
/// The naming RULE lives in `rts-abi` (so the symbol baker can call the same
/// function); only the `syn` parsing stays here. `throws` rides along in the
/// same parser because both attributes put the same `MemberFlags` on what they
/// declare — a second parser would let the two spellings drift.
pub(crate) struct AbiArgs {
    pub(crate) naming: rts_abi::scope::Naming,
    /// `throws` — the declaration sets the pending-error slot, so the engine
    /// must emit its post-call error check (`MemberFlags::THROWS`). Losing this
    /// bit turns a real `throw` into a silently-ignored failure, which is why
    /// a member that needs it cannot simply be converted without it.
    pub(crate) throws: bool,
    /// `pure` — the call has no observable effect, so the engine may treat it as
    /// such. Like `throws`, it was previously unreachable from a declaration:
    /// the generated member pinned it to `false`, which meant a genuinely pure
    /// member (`tls.getCiphers`) could not be declared here without silently
    /// losing the property.
    pub(crate) pure: bool,
    /// The JS member name WITHOUT the overload suffix. `naming` carries the
    /// suffixed spelling because that is what the LINKER needs; JS must still
    /// see the shared name, since sharing it is the whole point of an overload.
    pub(crate) js_value: Option<String>,
    /// `ret_ts = "…"` — the TS RETURN type, for when the Rust one cannot express
    /// it. The engine decides how a `Handle` result is reboxed FROM the ts
    /// signature — a heap string, a `T[]` array, a plain shaped object, or an
    /// instance of a named registered class — and every one of those is spelled
    /// `Handle` in Rust. So this is load-bearing, not documentation: without it
    /// `dgram.createSocket(): DgramSocket` reboxes as a bare object and the
    /// socket's methods stop resolving.
    pub(crate) ret_ts: Option<String>,
}

impl Parse for AbiArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        use rts_abi::scope::Naming;

        if input.is_empty() {
            return Ok(AbiArgs { naming: Naming::Verbatim, throws: false, pure: false, js_value: None, ret_ts: None });
        }
        if input.peek(syn::LitStr) {
            let s: syn::LitStr = input.parse()?;
            if !input.is_empty() {
                return Err(syn::Error::new_spanned(
                    &s,
                    "#[rtse::abi]: an explicit symbol takes no other arguments",
                ));
            }
            return Ok(AbiArgs { naming: Naming::Explicit(s.value()), throws: false, pure: false, js_value: None, ret_ts: None });
        }

        let mut scope: Option<Scope> = None;
        let mut value: Option<String> = None;
        let mut throws = false;
        let mut pure = false;
        // Arity-overload disambiguator: several members share one JS name, so the
        // SYMBOL needs a suffix the linker can tell apart. Composed by
        // `rts_abi::scope::with_overload`, which the baker calls too.
        let mut overload: Option<String> = None;
        // Explicit RETURN TS type. The engine derives how a `Handle` result is
        // reboxed FROM the ts signature — string vs array vs plain object vs a
        // named class — so a `Handle` that is really a `DgramSocket` or a
        // `string[]` must be able to say so. Derivation from the Rust type alone
        // cannot know (every one of them is `Handle`).
        let mut ret_ts: Option<String> = None;
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
                ("throws", None) => throws = true,
                ("pure", None) => pure = true,
                ("overload", Some(o)) => overload = Some(o),
                ("ret_ts", Some(t)) => ret_ts = Some(t),
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
                         `abi`, `value`, `throws` or `pure`)",
                    ));
                }
            }
            if input.peek(syn::Token![,]) {
                input.parse::<syn::Token![,]>()?;
            } else {
                break;
            }
        }

        let value_js = value;
        match scope {
            Some(scope) => Ok(AbiArgs {
                naming: Naming::Scoped {
                    scope,
                    value: value_js
                        .clone()
                        .map(|v| rts_abi::scope::with_overload(v, overload.as_deref())),
                },
                js_value: value_js,
                ret_ts,
                throws,
                pure,
            }),
            None => Err(syn::Error::new(
                Span::call_site(),
                "#[rtse::abi]: missing scope — one of `module = \"…\"`, `global`, `native`, `abi`",
            )),
        }
    }
}
