//! Marker-attribute parsing — turning `#[rtse::ctor]` / `#[rtse::method(...)]` /
//! `#[rtse::getter]` / … on an `impl` fn (and `#[rtse::variable]` on a struct
//! field) into the structured [`Kind`] `class::member::gen_member` dispatches
//! on. Kept apart from `member.rs`: this module only reads attribute tokens off
//! the `syn` item and classifies — it has no opinion on how a `Kind` gets
//! expanded into an extern-C wrapper + `Member`.

/// A parsed `#[rtse::ctor]`/`#[rtse::method]`/`#[rtse::statical]`/
/// `#[rtse::getter]`/`#[rtse::setter]`/`#[rtse::private]`/`#[rtse::asynch]`
/// marker, with its per-kind args already extracted.
pub(crate) enum Kind {
    Ctor {
        optional: usize,
        throws: bool,
    },
    Method {
        name: Option<String>,
        readonly: bool,
        private: bool,
        is_async: bool,
        optional: usize,
        throws: bool,
        returns: Option<String>,
    },
    Static {
        name: Option<String>,
        optional: usize,
        throws: bool,
        returns: Option<String>,
    },
    /// `#[rtse::getter]` on `fn prop(&self) -> T` → an `InstanceGetter` member
    /// (property read, no parens). Unlike `#[rtse::variable]` (scalar struct
    /// fields only), this backs a COMPUTED property by any type incl. `String`.
    Getter {
        name: Option<String>,
        returns: Option<String>,
    },
    /// `#[rtse::setter]` on `fn set_prop(&mut self, v: T)` → an `InstanceSetter`
    /// member. The engine's assignment path requires `MemberKind::InstanceSetter`
    /// (no method fallback), so a `String`-typed property setter needs this.
    Setter {
        name: Option<String>,
    },
    /// `#[rtse::functioncall]` — the class's call-WITHOUT-`new` behaviour
    /// (`Boolean(x)` vs `new Boolean(x)`). No receiver, same shape as
    /// `#[rtse::statical]`; kept as its own `Kind` (not folded into `Static`)
    /// because it is a semantically distinct protocol member the engine looks
    /// up by KIND, not by name — a class may declare at most one.
    FunctionCall {
        optional: usize,
        throws: bool,
        returns: Option<String>,
    },
}

/// Remove and classify the `#[rtse::ctor]`/`#[rtse::method(...)]`/`#[rtse::private]`
/// marker. `None` = a plain Rust helper (left untouched).
pub(crate) fn take_kind(attrs: &mut Vec<syn::Attribute>) -> Option<Kind> {
    let mut kind = None;
    attrs.retain(|a| {
        let segs = &a.path().segments;
        if segs.len() == 2 && segs[0].ident == "rtse" {
            match segs[1].ident.to_string().as_str() {
                "ctor" => {
                    let m = method_args(a);
                    kind = Some(Kind::Ctor {
                        optional: m.optional,
                        throws: m.throws,
                    });
                    return false;
                }
                "method" => {
                    let m = method_args(a);
                    kind = Some(Kind::Method {
                        name: m.name,
                        readonly: m.readonly,
                        private: false,
                        is_async: false,
                        optional: m.optional,
                        throws: m.throws,
                        returns: m.returns,
                    });
                    return false;
                }
                // `asynch` (not `async` — a Rust keyword) marks an async method: the
                // body runs (interim-synchronous, matching the engine's current
                // async model) and the return is wrapped in a settled Promise.
                "asynch" => {
                    let m = method_args(a);
                    kind = Some(Kind::Method {
                        name: m.name,
                        readonly: false,
                        private: false,
                        is_async: true,
                        optional: m.optional,
                        throws: m.throws,
                        returns: m.returns,
                    });
                    return false;
                }
                "private" => {
                    kind = Some(Kind::Method {
                        name: None,
                        readonly: false,
                        private: true,
                        is_async: false,
                        optional: 0,
                        throws: false,
                        returns: None,
                    });
                    return false;
                }
                // `statical` (not `static` — a Rust keyword) marks a static method.
                "statical" => {
                    let m = method_args(a);
                    kind = Some(Kind::Static {
                        name: m.name,
                        optional: m.optional,
                        throws: m.throws,
                        returns: m.returns,
                    });
                    return false;
                }
                "getter" => {
                    let m = method_args(a);
                    kind = Some(Kind::Getter {
                        name: m.name,
                        returns: m.returns,
                    });
                    return false;
                }
                "setter" => {
                    let m = method_args(a);
                    kind = Some(Kind::Setter { name: m.name });
                    return false;
                }
                "functioncall" => {
                    let m = method_args(a);
                    kind = Some(Kind::FunctionCall {
                        optional: m.optional,
                        throws: m.throws,
                        returns: m.returns,
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

/// Parsed `#[rtse::method(...)]`-style attribute args.
#[derive(Default)]
pub(crate) struct MethodArgs {
    pub(crate) name: Option<String>,
    pub(crate) readonly: bool,
    pub(crate) optional: usize,
    pub(crate) throws: bool,
    pub(crate) returns: Option<String>,
}

/// Parse `#[rtse::method(name = "x", readonly, optional = N)]` →
/// (Some("x"), readonly, N). A bare `#[rtse::method]` yields (None, false, 0).
/// `optional = N` (F4): the LAST N params are optional — each gets
/// `DefaultArg::Undefined`, so the engine admits calls that omit them (arity
/// window `[total-N, total]`) and injects the `undefined` sentinel word, which
/// the extern reads as its own "absent" value (`""` for `&str`, NaN for `f64`,
/// 0/undefined for a handle) — the same convention the hand-written externs use.
///
/// **Superseded by `Option<T>` in parameter position** (see
/// `types::option_inner` + `member::params::build_params`): a trailing
/// `Option<T>` param declares the SAME arity window as a COUNT, but says WHICH
/// param is optional (adding a param no longer silently shifts what the count
/// refers to) and the generated wrapper hands the body a real `Option<T>`
/// instead of a raw sentinel word to decode by hand. `optional = N` keeps
/// working (existing call sites; not migrated in this change) — prefer
/// `Option<T>` for new members.
fn method_args(a: &syn::Attribute) -> MethodArgs {
    let mut out = MethodArgs::default();
    let _ = a.parse_nested_meta(|m| {
        if m.path.is_ident("name") {
            let v = m.value()?;
            let s: syn::LitStr = v.parse()?;
            out.name = Some(s.value());
        } else if m.path.is_ident("readonly") {
            out.readonly = true;
        } else if m.path.is_ident("optional") {
            let v = m.value()?;
            let n: syn::LitInt = v.parse()?;
            out.optional = n.base10_parse()?;
        } else if m.path.is_ident("throws") {
            // The member may leave a pending JS error; the engine routes the
            // post-call check to try/catch (MemberFlags::THROWS).
            out.throws = true;
        } else if m.path.is_ident("returns") {
            // A `Handle` return that is a specific registered class: name it in the
            // ts return (`signal(): AbortSignal`) so the engine's return-class
            // tracking classifies the result → chained `.prop`/`.method()` on it
            // resolve. Without this a class handle rides as ts `object` and loses
            // its identity (`c.signal.aborted` → undefined).
            let v = m.value()?;
            let s: syn::LitStr = v.parse()?;
            out.returns = Some(s.value());
        }
        Ok(())
    });
    out
}

/// A parsed `#[rtse::symbol(...)]` marker — stacks alongside `#[rtse::method]`/
/// `#[rtse::getter]`/etc (it does not select the member KIND, only its KEY).
///
/// - `Registry(name)` — `#[rtse::symbol("foo")]`. Keyed like `Symbol.for("foo")`:
///   a string-keyed registry symbol, not a unique identity.
/// - `WellKnown(path)` — `#[rtse::symbol(Symbol::iterator)]`. `path` is emitted
///   VERBATIM into the generated extern body (`#path.handle()`), so a typo is a
///   rustc error at the site that declares the member, not a silent runtime
///   miss (see `rts_primitives::symbol::wellknown::Symbol`).
pub(crate) enum SymbolKey {
    Registry(String),
    WellKnown(syn::Path),
}

enum SymbolArg {
    Str(String),
    Path(syn::Path),
}

impl syn::parse::Parse for SymbolArg {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.peek(syn::LitStr) {
            let s: syn::LitStr = input.parse()?;
            Ok(SymbolArg::Str(s.value()))
        } else {
            let p: syn::Path = input.parse()?;
            Ok(SymbolArg::Path(p))
        }
    }
}

/// Detect + strip a `#[rtse::symbol("key")]` / `#[rtse::symbol(Path::to::const)]`
/// on an impl fn. `None` = not symbol-keyed (an ordinary named member).
pub(crate) fn take_symbol_key(attrs: &mut Vec<syn::Attribute>) -> syn::Result<Option<SymbolKey>> {
    let mut found = None;
    let mut err = None;
    attrs.retain(|a| {
        let segs = &a.path().segments;
        if segs.len() == 2 && segs[0].ident == "rtse" && segs[1].ident == "symbol" {
            match a.parse_args::<SymbolArg>() {
                Ok(SymbolArg::Str(s)) => found = Some(SymbolKey::Registry(s)),
                Ok(SymbolArg::Path(p)) => found = Some(SymbolKey::WellKnown(p)),
                Err(e) => err = Some(e),
            }
            return false;
        }
        true
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(found)
}

/// Detect + strip a `#[rtse::variable]` / `#[rtse::variable(readonly)]` on a field.
/// `Some(readonly)` when present; `None` = a plain field (not exposed).
pub(crate) fn take_variable(attrs: &mut Vec<syn::Attribute>) -> Option<bool> {
    let mut found = None;
    attrs.retain(|a| {
        let segs = &a.path().segments;
        if segs.len() == 2 && segs[0].ident == "rtse" && segs[1].ident == "variable" {
            let mut readonly = false;
            let _ = a.parse_nested_meta(|m| {
                if m.path.is_ident("readonly") {
                    readonly = true;
                }
                Ok(())
            });
            found = Some(readonly);
            return false;
        }
        true
    });
    found
}
