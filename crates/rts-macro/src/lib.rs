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
//! Status — base: ctor + instance `&self`/`&mut self` methods + `#[rtse::statical]`
//! statics (`statical`, not the `static` keyword) + `#[rtse::variable]` scalar fields
//! (getter/setter) + `#[rtse::private]` + AOT force-keep (`#[used]` FnPtr array).
//! Params: `f64`/`i64`/`i32`/`bool`/`&str`. Returns: `String`/`&str`/`()`. Plus:
//!  - **F1** `Handle`/`U64` (u64 passthrough) params + returns.
//!  - **F2** overload by arity (N members same JS `name=`, distinct argc — free;
//!    the engine dispatches by argc).
//!  - **F3** `#[rtse::asynch]` on a real `async fn` — driven by `rts_engine::block_on`,
//!    return wrapped in a settled Promise (rejected on a pending error slot).
//!  - **F4** `optional=N` — last N params default `undefined` (Sig::with_defaults).
//!  - `#[rtse::getter]`/`#[rtse::setter]` — real InstanceGetter/InstanceSetter,
//!    String-capable computed properties.
//!  - `throws` — sets `MemberFlags::THROWS` (composes with readonly/optional).
//!  - **F8** `Vec<String>`/`Vec<Handle>` return → `Entry::Vec` + ts `T[]` (real array).
//!  - Instance methods CLONE the receiver out of the HandleTable before the body
//!    (drops the shard lock; write-back for `&mut self`) so a body touching a 2nd
//!    handle can't self-deadlock — the struct must be `Clone`.
//!  - Dotted class names (`Intl.NumberFormat`) sanitize `.`→`_` in symbols.
//! Open gaps: nested-array return (`[string,string][]`), PolyValue (`any`) params/
//! returns, `#[rtse::symbol]` (well-known), constants, `global(descriptor)`.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ItemImpl, ReturnType, Type};

/// `#[rtse::class("Name")]` — on the STRUCT (fields via `#[rtse::variable]`) OR on
/// the `impl` block (ctor/methods). Both are required for a class with fields: the
/// struct macro emits `__rtse_fields_<Class>(cb)` and the impl's `register` calls
/// it (option A coordination — the two items can't see each other otherwise).
#[proc_macro_attribute]
pub fn class(args: TokenStream, item: TokenStream) -> TokenStream {
    let parsed = match syn::parse::<ClassArgs>(args) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };
    if let Ok(s) = syn::parse::<syn::ItemStruct>(item.clone()) {
        return gen_struct(parsed.name, s);
    }
    match syn::parse::<ItemImpl>(item) {
        Ok(imp) => gen_impl(parsed.name, parsed.extends, imp),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Parsed `#[rtse::class("Name")]` / `#[rtse::class("Name", extends = "Parent")]`.
struct ClassArgs {
    name: String,
    extends: Option<String>,
}

impl syn::parse::Parse for ClassArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let name: syn::LitStr = input.parse()?;
        let mut extends = None;
        while input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
            if input.is_empty() {
                break;
            }
            let key: syn::Ident = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            let val: syn::LitStr = input.parse()?;
            if key == "extends" {
                extends = Some(val.value());
            } else {
                return Err(syn::Error::new_spanned(key, "rtse::class: unknown arg (expected `extends`)"));
            }
        }
        Ok(ClassArgs {
            name: name.value(),
            extends,
        })
    }
}

/// A `#[used]` static forcing `keep`'s extern addresses into the AOT archive.
fn keep_static(class_upper: &str, suffix: &str, keep: &[proc_macro2::Ident]) -> proc_macro2::TokenStream {
    if keep.is_empty() {
        return quote!();
    }
    let ident = format_ident!("__RTSE_KEEP_{}{}", class_upper, suffix);
    let n = keep.len();
    quote! {
        #[used]
        static #ident: [::rts_engine::FnPtr; #n] = [#(::rts_engine::FnPtr(#keep as *const u8)),*];
    }
}

/// `#[rtse::class]` on the `impl` — ctor/methods + `register` (which calls the
/// struct's `__rtse_fields_<Class>` to add the field accessors first).
fn gen_impl(class_name: String, extends: Option<String>, mut imp: ItemImpl) -> TokenStream {
    let class_upper = class_name.to_uppercase().replace(['.'], "_");
    let self_ty = (*imp.self_ty).clone();
    let mut externs = Vec::new();
    let mut members = Vec::new();
    let mut keep = Vec::new();
    for it in imp.items.iter_mut() {
        let ImplItem::Fn(f) = it else { continue };
        let Some(kind) = take_kind(&mut f.attrs) else {
            continue;
        };
        let doc = extract_doc(&f.attrs);
        match gen_member(&class_name, &class_upper, &self_ty, &f.sig, kind, doc) {
            Ok((ext, mem, id)) => {
                externs.push(ext);
                members.push(mem);
                keep.push(id);
            }
            Err(e) => return e.to_compile_error().into(),
        }
    }
    let keep_tok = keep_static(&class_upper, "", &keep);
    let fields_fn = format_ident!("__rtse_fields_{}", class_upper);
    // `extends = "Parent"` → `.extends("Parent")` on the ClassBuilder (Registry
    // parent link for `x instanceof Parent`; methods are inherited by composition
    // + forwarding in the author's struct, not by a dispatch chain-walk).
    let extends_tok = match &extends {
        Some(p) => quote!(.extends(#p)),
        None => quote!(),
    };
    quote! {
        #imp
        #(#externs)*
        #keep_tok
        /// Register this class + members into the engine Registry. Add to `REGISTER`.
        pub fn register(e: &mut ::rts_engine::Engine) {
            #fields_fn(e.class(#class_name))
                #extends_tok
                #(#members)*
                .done();
        }
    }
    .into()
}

/// `#[rtse::class]` on the STRUCT — generate a getter (and setter unless
/// `readonly`) per `#[rtse::variable]` field, plus
/// `pub fn __rtse_fields_<Class>(cb) -> cb` that the impl's `register` chains.
fn gen_struct(class_name: String, mut s: syn::ItemStruct) -> TokenStream {
    let class_upper = class_name.to_uppercase().replace(['.'], "_");
    let self_ty: Type = syn::parse_str(&s.ident.to_string()).unwrap();
    let mut externs = Vec::new();
    let mut members = Vec::new();
    let mut keep = Vec::new();
    if let syn::Fields::Named(named) = &mut s.fields {
        for f in named.named.iter_mut() {
            let Some(readonly) = take_variable(&mut f.attrs) else {
                continue;
            };
            let fname = f.ident.clone().unwrap();
            let fdoc = extract_doc(&f.attrs);
            match gen_field(&class_name, &class_upper, &self_ty, &fname, &f.ty, readonly, fdoc) {
                Ok((exts, mems, ids)) => {
                    externs.extend(exts);
                    members.extend(mems);
                    keep.extend(ids);
                }
                Err(e) => return e.to_compile_error().into(),
            }
        }
    }
    let keep_tok = keep_static(&class_upper, "_FIELDS", &keep);
    let fields_fn = format_ident!("__rtse_fields_{}", class_upper);
    quote! {
        #s
        #(#externs)*
        #keep_tok
        /// Add this class's `#[rtse::variable]` field accessors to `cb`. Called by
        /// the impl's generated `register`.
        pub fn #fields_fn(cb: ::rts_engine::ClassBuilder) -> ::rts_engine::ClassBuilder {
            cb #(#members)*
        }
    }
    .into()
}

enum Kind {
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
                _ => {}
            }
        }
        true
    });
    kind
}

/// Parse `#[rtse::method(name = "x", readonly, optional = N)]` →
/// (Some("x"), readonly, N). A bare `#[rtse::method]` yields (None, false, 0).
/// `optional = N` (F4): the LAST N params are optional — each gets
/// `DefaultArg::Undefined`, so the engine admits calls that omit them (arity
/// window `[total-N, total]`) and injects the `undefined` sentinel word, which
/// the extern reads as its own "absent" value (`""` for `&str`, NaN for `f64`,
/// 0/undefined for a handle) — the same convention the hand-written externs use.
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

/// Parsed `#[rtse::method(...)]`-style attribute args.
#[derive(Default)]
struct MethodArgs {
    name: Option<String>,
    readonly: bool,
    optional: usize,
    throws: bool,
    returns: Option<String>,
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

/// Is this a raw-handle passthrough type (`Handle`/`U64` alias = u64)? Returns the
/// TS type-name (`object` for a runtime handle, `number` for a raw U64). This is F1
/// — a u64 crosses the ABI untouched (no marshalling), letting a class store/return
/// other heap values by handle (weak refs, collections, rich objects).
fn is_handle_ty(ty: &Type) -> Option<&'static str> {
    let Type::Path(p) = ty else { return None };
    match p.path.segments.last()?.ident.to_string().as_str() {
        "Handle" => Some("object"),
        "U64" => Some("number"),
        _ => None,
    }
}

/// F8: if `ty` is `Vec<T>`, return its element type `T`. A `Vec<String>` /
/// `Vec<Handle>` return marshals to an `Entry::Vec` of element handles + a ts
/// `T[]` return, which the engine boxes via `__rtsadp_box_handle_auto` (the
/// `ret_is_array_handle` path — tag-dispatched, null/string/object correct).
fn vec_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return None;
    };
    match a.args.first()? {
        syn::GenericArgument::Type(t) => Some(t),
        _ => None,
    }
}

/// F8-nested: `Some(n)` if `ty` is a tuple of `n` `String` fields (e.g.
/// `(String, String)` for `entries()` → `[string,string][]`). Each element
/// marshals to an inner `Entry::Vec` of `n` string handles.
fn str_tuple_arity(ty: &Type) -> Option<usize> {
    let Type::Tuple(t) = ty else { return None };
    if t.elems.is_empty() || !t.elems.iter().all(is_string_ret) {
        return None;
    }
    Some(t.elems.len())
}

/// Harvest the Rust `///` doc comments of an item (syn lowers each to a
/// `#[doc = "..."]` attribute) into one string — flows into `Member.doc`, which
/// the `.d.ts` generator emits. Leading space (from `/// text`) is trimmed.
fn extract_doc(attrs: &[syn::Attribute]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &a.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                let v = s.value();
                lines.push(v.strip_prefix(' ').unwrap_or(&v).to_string());
            }
        }
    }
    lines.join("\n")
}

/// Is this a `SelfHandle` param? The macro fills it with the receiver's own
/// handle (`__recv`) and drops it from the JS signature — for a body that needs
/// its own handle (return a fresh instance sharing state, dispatch stamping
/// `ev.target`, hand a child its parent handle).
fn is_self_handle_param(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "SelfHandle"))
}

/// Is the return type `Self` (a method returning a fresh instance of its OWN
/// class, e.g. `Blob.slice() -> Blob`)? The macro allocs it as a classed
/// `Entry::Rtse` (like a ctor) + ts = the class name (return-class tracked).
fn is_self_ret(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.is_ident("Self"))
}

/// Is this return type `Option<Self>` — a FALLIBLE ctor/factory that yields a
/// NULL handle (`0`) on `None` (WHATWG `new URL(bad)` → null; the shape any
/// parse-or-null constructor needs). The macro allocs the `Some` payload via
/// `alloc_rtse` exactly like `-> Self`, and returns `0u64` for `None`.
fn is_option_self_ret(ty: &Type) -> bool {
    let Type::Path(p) = ty else { return false };
    let Some(seg) = p.path.segments.last() else {
        return false;
    };
    if seg.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return false;
    };
    matches!(a.args.first(), Some(syn::GenericArgument::Type(inner)) if is_self_ret(inner))
}

/// Is this return type `Option<String>` — a NULLABLE string getter/method that
/// yields JS `null` (a `0` handle) on `None` and a string handle on `Some`
/// (`URLSearchParams.get(missing)` → null, not `""`). ts return is
/// `string | null` so the engine's nullable-string rebox maps `0` → `null`.
fn is_option_string_ret(ty: &Type) -> bool {
    let Type::Path(p) = ty else { return false };
    let Some(seg) = p.path.segments.last() else {
        return false;
    };
    if seg.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(a) = &seg.arguments else {
        return false;
    };
    matches!(a.args.first(), Some(syn::GenericArgument::Type(inner)) if is_string_ret(inner))
}

/// Is this a `Poly` (`any`) type? A NaN-boxed PolyValue word crosses the ABI
/// UNCHANGED (`AbiType::PolyValue`) — an arbitrary JS value (weakmap value, etc.).
fn is_poly_ty(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Poly"))
}

/// Is this param type `&str`?
fn is_str_param(ty: &Type) -> bool {
    matches!(ty, Type::Reference(r) if matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")))
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
    doc: String,
) -> syn::Result<(
    proc_macro2::TokenStream,
    proc_macro2::TokenStream,
    proc_macro2::Ident,
)> {
    let rust_name = sig.ident.clone();
    let is_ctor = matches!(kind, Kind::Ctor { .. });
    let is_static = matches!(kind, Kind::Static { .. });
    let is_async = matches!(kind, Kind::Method { is_async: true, .. });
    let is_getter = matches!(kind, Kind::Getter { .. });
    let is_setter = matches!(kind, Kind::Setter { .. });
    let optional = match &kind {
        Kind::Ctor { optional, .. }
        | Kind::Method { optional, .. }
        | Kind::Static { optional, .. } => *optional,
        Kind::Getter { .. } | Kind::Setter { .. } => 0,
    };
    let throws = matches!(
        &kind,
        Kind::Ctor { throws: true, .. }
            | Kind::Method { throws: true, .. }
            | Kind::Static { throws: true, .. }
    );
    // A `returns = "Class"` names the class a `Handle` return carries, so the ts
    // return says `: Class` and the engine's return-class tracking classifies the
    // result (chained `.prop`/`.method()` resolve).
    let returns_class: Option<String> = match &kind {
        Kind::Method { returns, .. }
        | Kind::Static { returns, .. }
        | Kind::Getter { returns, .. } => returns.clone(),
        _ => None,
    };
    let (js_name, readonly, private) = match &kind {
        Kind::Ctor { .. } => ("new".to_string(), false, false),
        Kind::Method {
            name,
            readonly,
            private,
            ..
        } => (
            name.clone().unwrap_or_else(|| to_camel(&rust_name.to_string())),
            *readonly,
            *private,
        ),
        Kind::Static { name, .. } | Kind::Getter { name, .. } => (
            name.clone().unwrap_or_else(|| to_camel(&rust_name.to_string())),
            false,
            false,
        ),
        // A setter's default JS name strips a leading `set_` (`set_href` → `href`).
        Kind::Setter { name } => (
            name.clone().unwrap_or_else(|| {
                let r = rust_name.to_string();
                to_camel(r.strip_prefix("set_").unwrap_or(&r))
            }),
            false,
            false,
        ),
    };
    // getter/setter arity guards (the engine expects getter `(recv)->T`, setter
    // `(recv, v)->void`).
    let n_typed = sig
        .inputs
        .iter()
        .filter(|a| matches!(a, FnArg::Typed(_)))
        .count();
    if is_getter && n_typed != 0 {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            "rtse getter: takes only `&self` (a property read has no params)",
        ));
    }
    if is_setter {
        if n_typed != 1 {
            return Err(syn::Error::new_spanned(
                &sig.ident,
                "rtse setter: takes `&mut self` and exactly one value param",
            ));
        }
        if ret_ty(sig).is_some() {
            return Err(syn::Error::new_spanned(
                &sig.ident,
                "rtse setter: must return `()` (the assigned value is discarded)",
            ));
        }
    }

    // The Rust fn name is used VERBATIM (case-preserved) in the symbol — NOT
    // uppercased — so two members differing only by case (`fn Foo` vs `fn foo`)
    // stay distinct symbols instead of colliding. Consumers link by the exact
    // name; the engine reads `Member.symbol`, so it is case-agnostic regardless.
    let member = rust_name.to_string();
    let symbol = format!("__RTS_FN_GL_{class_upper}_{member}");
    let extern_ident = format_ident!("{}", symbol);

    // Params (skip the receiver).
    let mut ext_params = Vec::new();
    let mut call_args = Vec::new();
    let mut arg_abis = Vec::new();
    let mut ts_params = Vec::new();
    if !is_ctor && !is_static {
        ext_params.push(quote!(__recv: u64));
    }
    let mut idx = 0usize;
    for a in &sig.inputs {
        let FnArg::Typed(pt) = a else { continue };
        if is_self_handle_param(&pt.ty) {
            // The receiver's own handle — passed to the body as `__recv`, NOT a JS
            // arg (no ext param / ts / abi, idx unchanged).
            call_args.push(quote!(__recv));
            continue;
        }
        if is_str_param(&pt.ty) {
            // `&str` crosses as StrPtr = two slots (ptr:i64, len:i64); rebuild the
            // &str from the string-pool pointer the codegen passes.
            let pp = format_ident!("__a{}_ptr", idx);
            let pl = format_ident!("__a{}_len", idx);
            ext_params.push(quote!(#pp: i64));
            ext_params.push(quote!(#pl: i64));
            call_args.push(quote!({
                let __b = unsafe { ::core::slice::from_raw_parts(#pp as *const u8, #pl as usize) };
                ::core::str::from_utf8(__b).unwrap_or("")
            }));
            arg_abis.push(quote!(::rts_engine::AbiType::StrPtr));
            ts_params.push(format!("a{idx}: string"));
            idx += 1;
            continue;
        }
        if let Some(ts) = is_handle_ty(&pt.ty) {
            // F1: raw u64 handle passthrough — no marshalling, hand it straight to
            // the Rust body (whose param type is the `Handle`/`U64` alias = u64).
            let pid = format_ident!("__a{}", idx);
            ext_params.push(quote!(#pid: u64));
            call_args.push(quote!(#pid));
            arg_abis.push(quote!(::rts_engine::AbiType::Handle));
            ts_params.push(format!("a{idx}: {ts}"));
            idx += 1;
            continue;
        }
        if is_poly_ty(&pt.ty) {
            // `Poly` (`any`): the raw NaN-boxed PolyValue word, ABI-unchanged.
            let pid = format_ident!("__a{}", idx);
            ext_params.push(quote!(#pid: u64));
            call_args.push(quote!(#pid));
            arg_abis.push(quote!(::rts_engine::AbiType::PolyValue));
            ts_params.push(format!("a{idx}: any"));
            idx += 1;
            continue;
        }
        let Some((abi, ext_ty, ts)) = scalar(&pt.ty) else {
            return Err(syn::Error::new_spanned(
                &pt.ty,
                "rtse: param must be f64/i64/i32/bool/&str/Handle/U64",
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
        // The ctor allocates the struct as a classed `Entry::Rtse` (the class name
        // travels so `instanceof` can consult the hierarchy). A FALLIBLE ctor
        // (`-> Option<Self>`) allocs the `Some` and returns null (`0`) for `None`.
        let cls = class.to_string();
        let fallible = ret_ty(sig).is_some_and(is_option_self_ret);
        let wrap: Box<dyn Fn(proc_macro2::TokenStream) -> proc_macro2::TokenStream> = if fallible {
            Box::new(move |b| {
                quote!(match #b {
                    ::core::option::Option::Some(__v) =>
                        ::rts_engine::heap::handles::alloc_rtse(#cls, __v),
                    ::core::option::Option::None => 0u64,
                })
            })
        } else {
            Box::new(move |b| quote!(::rts_engine::heap::handles::alloc_rtse(#cls, #b)))
        };
        (
            quote!(::rts_engine::AbiType::Handle),
            quote!(u64),
            class.to_string(),
            wrap,
        )
    } else {
        match ret_ty(sig) {
            None => (
                quote!(::rts_engine::AbiType::Void),
                quote!(()),
                "void".into(),
                Box::new(|b| quote!({ #b; })),
            ),
            Some(t) if is_handle_ty(t).is_some() => (
                // F1: return a raw u64 handle untouched.
                quote!(::rts_engine::AbiType::Handle),
                quote!(u64),
                is_handle_ty(t).unwrap().into(),
                Box::new(|b| quote!(#b)),
            ),
            Some(t) if is_poly_ty(t) => (
                // `Poly` (`any`): return the raw NaN-boxed word untouched.
                quote!(::rts_engine::AbiType::PolyValue),
                quote!(u64),
                "any".into(),
                Box::new(|b| quote!(#b)),
            ),
            Some(t) if is_self_ret(t) => {
                // `-> Self`: alloc the fresh instance as a classed `Entry::Rtse`,
                // exactly like the ctor. ts = the class name (return-class tracked).
                let cls = class.to_string();
                (
                    quote!(::rts_engine::AbiType::Handle),
                    quote!(u64),
                    class.to_string(),
                    Box::new(move |b| quote!(::rts_engine::heap::handles::alloc_rtse(#cls, #b))),
                )
            }
            Some(t) if is_option_self_ret(t) => {
                // `-> Option<Self>`: a fallible factory — alloc the `Some`, return
                // null (`0`) for `None`. Same class-tracked handle as `-> Self`.
                let cls = class.to_string();
                (
                    quote!(::rts_engine::AbiType::Handle),
                    quote!(u64),
                    class.to_string(),
                    Box::new(move |b| {
                        quote!(match #b {
                            ::core::option::Option::Some(__v) =>
                                ::rts_engine::heap::handles::alloc_rtse(#cls, __v),
                            ::core::option::Option::None => 0u64,
                        })
                    }),
                )
            }
            Some(t) if is_option_string_ret(t) => (
                // `-> Option<String>`: a nullable string — `Some(s)` allocs a string
                // handle, `None` returns `0` (JS `null`). ts `string | null` so the
                // engine's nullable-string rebox maps the `0` handle to `null`.
                quote!(::rts_engine::AbiType::Handle),
                quote!(u64),
                "string | null".into(),
                Box::new(|b| {
                    quote!(match #b {
                        ::core::option::Option::Some(__s) =>
                            ::rts_engine::heap::handles::alloc_entry(
                                ::rts_engine::heap::handles::Entry::String(__s.into_bytes())
                            ),
                        ::core::option::Option::None => 0u64,
                    })
                }),
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
            // F8: `Vec<String>` / `Vec<Handle>` → an `Entry::Vec` of element
            // handles, ts `string[]` / `object[]`. The engine sees the `[]` and
            // reboxes via `__rtsadp_box_handle_auto` (normalizes the raw element
            // handles to words).
            Some(t) if vec_inner(t).is_some() => {
                let inner = vec_inner(t).unwrap();
                // Per-element boxing: `String` → a string handle; `Handle` → the raw
                // handle; a String-tuple `(String, String)` → an inner `Entry::Vec`
                // of string handles (nested `[string,string][]`, e.g. `entries()`).
                let (elem_ts, push): (&str, proc_macro2::TokenStream) = if is_string_ret(inner) {
                    (
                        "string",
                        quote!(::rts_engine::heap::handles::alloc_entry(
                            ::rts_engine::heap::handles::Entry::String(__e.into_bytes())
                        ) as i64),
                    )
                } else if is_handle_ty(inner).is_some() {
                    ("object", quote!(__e as i64))
                } else if let Some(n) = str_tuple_arity(inner) {
                    let binds: Vec<_> = (0..n).map(|i| format_ident!("__e{}", i)).collect();
                    (
                        "string[]",
                        quote!({
                            let ( #(#binds),* ) = __e;
                            let __inner: ::std::vec::Vec<i64> = ::std::vec![
                                #( ::rts_engine::heap::handles::alloc_entry(
                                    ::rts_engine::heap::handles::Entry::String(#binds.into_bytes())
                                ) as i64 ),*
                            ];
                            ::rts_engine::heap::handles::alloc_entry(
                                ::rts_engine::heap::handles::Entry::Vec(::std::boxed::Box::new(__inner))
                            ) as i64
                        }),
                    )
                } else {
                    return Err(syn::Error::new_spanned(
                        inner,
                        "rtse: Vec<T> element must be String, Handle, or a tuple of String",
                    ));
                };
                (
                    quote!(::rts_engine::AbiType::Handle),
                    quote!(u64),
                    format!("{elem_ts}[]"),
                    Box::new(move |b| {
                        quote!({
                            let __v: ::std::vec::Vec<i64> =
                                (#b).into_iter().map(|__e| #push).collect();
                            ::rts_engine::heap::handles::alloc_entry(
                                ::rts_engine::heap::handles::Entry::Vec(::std::boxed::Box::new(__v))
                            )
                        })
                    }),
                )
            }
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

    // F3 `#[rtse::asynch]`: the body runs interim-synchronously (the engine's
    // current async model), then its return is wrapped in a settled Promise —
    // fulfilled with the value, or rejected if the body left a pending JS error
    // in the thread-local error slot. The base return must already be a heap
    // handle (String/Handle) so `__rtsadp_box_handle_auto` can box it to a word.
    // Settlers/box/errslot are adapter-layer symbols ABOVE this crate — reached
    // via a local `extern "C"` forward-decl (layering-safe: one linked binary).
    let (ret_abi, ret_ext_ty, ret_ts, wrap) = if is_async {
        let ok = match ret_ty(sig) {
            Some(t) => is_string_ret(t) || is_handle_ty(t).is_some(),
            None => false,
        };
        if !ok {
            return Err(syn::Error::new_spanned(
                &sig.output,
                "rtse asynch: return must be String or Handle (wrapped in a Promise)",
            ));
        }
        let base = wrap;
        let inner_ts = ret_ts;
        let awrap: Box<dyn Fn(proc_macro2::TokenStream) -> proc_macro2::TokenStream> =
            Box::new(move |call| {
                let inner = base(call);
                quote!({
                    unsafe extern "C" {
                        fn __rtsadp_box_handle_auto(h: u64) -> u64;
                        fn __rtsadp_promise_resolve_w(w: u64) -> u64;
                        fn __rtsadp_err_pending() -> i64;
                        fn __rtsadp_err_take() -> u64;
                        fn __RTS_FN_NS_PROMISE_NEW_REJECTED(e: i64) -> u64;
                    }
                    let __h: u64 = #inner;
                    unsafe {
                        if __rtsadp_err_pending() != 0 {
                            let __e = __rtsadp_err_take();
                            __RTS_FN_NS_PROMISE_NEW_REJECTED(__e as i64)
                        } else {
                            __rtsadp_promise_resolve_w(__rtsadp_box_handle_auto(__h))
                        }
                    }
                })
            });
        (
            quote!(::rts_engine::AbiType::Handle),
            quote!(u64),
            format!("Promise<{inner_ts}>"),
            awrap,
        )
    } else {
        (ret_abi, ret_ext_ty, ret_ts, wrap)
    };

    // A `&mut self` (or `self: &mut X`) method needs mutable access to the boxed
    // struct → `with_rtse_mut`; an `&self` method → `with_rtse`.
    let is_mut_recv = sig.inputs.iter().any(|a| match a {
        FnArg::Receiver(r) => {
            r.mutability.is_some()
                || matches!(&*r.ty, Type::Reference(rf) if rf.mutability.is_some())
        }
        _ => false,
    });
    // A real Rust `async fn` body yields a `Future`; the `#[rtse::asynch]` bridge
    // drives it to a value with `rts_engine::block_on` (interim engine async is
    // synchronous). This keeps the Rust side a genuine `async fn` — `.await` works
    // in the body and the method is usable as `.await` from other Rust — while the
    // JS side still gets a settled Promise (the F3 return wrap above).
    if is_async && sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            "rtse asynch: mark a real `async fn` (write `async fn ...`); the body's \
             Future is driven by `rts_engine::block_on` and wrapped in a Promise",
        ));
    }
    if sig.asyncness.is_some() && !is_async {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            "rtse: an `async fn` member must be marked `#[rtse::asynch]`",
        ));
    }
    let invoke = |recv: proc_macro2::TokenStream| {
        if is_async {
            quote!(::rts_engine::block_on(#recv.#rust_name(#(#call_args),*)))
        } else {
            quote!(#recv.#rust_name(#(#call_args),*))
        }
    };
    // Instance dispatch CLONES the receiver OUT of the HandleTable (which drops the
    // per-shard `Mutex` guard) BEFORE running the body, then — for `&mut self` —
    // writes the mutated clone back. This is required for correctness: `with_rtse`
    // holds a NON-reentrant per-shard lock, so a body that touches ANOTHER handle
    // (`with_entry`/`alloc_entry`, e.g. `WeakRef::deref`, `TextDecoder::decode`,
    // allocating a String return) would SELF-DEADLOCK whenever that handle hashes
    // to the receiver's shard (1-in-32). Cloning out first removes the held lock
    // across the body. The struct must therefore be `Clone` (a clear compile error
    // otherwise). Cost: one clone per call — acceptable, this is the (non-hot)
    // library surface, never the numeric fast path.
    let inner = invoke(quote!(__s));
    // `wrap` (raw Rust return → ABI value) is applied INSIDE the Some-arm, so the
    // None-arm (dead/invalid handle) returns `Default::default()` of the EXTERN
    // return type (u64/f64/…, always `Default`) — NOT `Default` of the raw method
    // return (which for `-> Self`/`-> String` need not be `Default`).
    let body = if is_ctor || is_static {
        let ty = &self_ty;
        let raw = if is_async {
            quote!(::rts_engine::block_on(<#ty>::#rust_name(#(#call_args),*)))
        } else {
            quote!(<#ty>::#rust_name(#(#call_args),*))
        };
        wrap(raw)
    } else {
        let some = wrap(inner);
        if is_mut_recv {
            quote!({
                let mut __c: ::core::option::Option<#self_ty> =
                    ::rts_engine::heap::handles::with_rtse::<#self_ty, _>(__recv, |__s| __s.cloned());
                let __r: #ret_ext_ty = match __c.as_mut() {
                    ::core::option::Option::Some(__s) => #some,
                    ::core::option::Option::None => ::core::default::Default::default(),
                };
                if let ::core::option::Option::Some(__c) = __c {
                    ::rts_engine::heap::handles::with_rtse_mut::<#self_ty, _>(__recv, |__slot| {
                        if let ::core::option::Option::Some(__slot) = __slot {
                            *__slot = __c;
                        }
                    });
                }
                __r
            })
        } else {
            quote!({
                let __c: ::core::option::Option<#self_ty> =
                    ::rts_engine::heap::handles::with_rtse::<#self_ty, _>(__recv, |__s| __s.cloned());
                let __r: #ret_ext_ty = match __c.as_ref() {
                    ::core::option::Option::Some(__s) => #some,
                    ::core::option::Option::None => ::core::default::Default::default(),
                };
                __r
            })
        }
    };

    let extern_fn = quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn #extern_ident(#(#ext_params),*) -> #ret_ext_ty {
            #body
        }
    };

    let kind_tok = if is_ctor {
        quote!(::rts_engine::MemberKind::Constructor)
    } else if is_static {
        quote!(::rts_engine::MemberKind::StaticMethod)
    } else if is_getter {
        quote!(::rts_engine::MemberKind::InstanceGetter)
    } else if is_setter {
        quote!(::rts_engine::MemberKind::InstanceSetter)
    } else {
        quote!(::rts_engine::MemberKind::InstanceMethod)
    };
    // Instance methods carry the receiver Handle in arg slot 0; ctor/static do not.
    let sig_args = if is_ctor || is_static {
        quote!(::std::vec![#(#arg_abis),*])
    } else {
        quote!(::std::vec![::rts_engine::AbiType::Handle #(, #arg_abis)*])
    };
    // F4: the last `optional` explicit params default to `undefined`. Build the
    // `DefaultArg` vec (same length as the Sig args, receiver included → Required)
    // and use `Sig::with_defaults`; else plain `Sig::new`.
    let nparams = arg_abis.len();
    if optional > nparams {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            format!("rtse optional={optional}: only {nparams} params to make optional"),
        ));
    }
    let sig_build = if optional == 0 {
        quote!(::rts_engine::Sig::new(#sig_args, #ret_abi))
    } else {
        let required_params = nparams - optional;
        let mut defs: Vec<proc_macro2::TokenStream> = Vec::new();
        if !is_ctor && !is_static {
            defs.push(quote!(::rts_engine::DefaultArg::Required)); // receiver
        }
        for _ in 0..required_params {
            defs.push(quote!(::rts_engine::DefaultArg::Required));
        }
        for _ in 0..optional {
            defs.push(quote!(::rts_engine::DefaultArg::Undefined));
        }
        quote!(::rts_engine::Sig::with_defaults(#sig_args, #ret_abi, ::std::vec![#(#defs),*]))
    };
    // Mark the last `optional` params `?:` in the ts signature.
    let ts_params: Vec<String> = ts_params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i >= nparams - optional {
                p.replacen(": ", "?: ", 1)
            } else {
                p.clone()
            }
        })
        .collect();
    let ps = ts_params.join(", ");
    // `returns = "Class"` names a Handle return's class in the ts (`: Class`) so
    // the engine classifies the result for chained dispatch. Only applies to a
    // `Handle`-typed return (a String/scalar keeps its own ts).
    let ret_is_handle = ret_ty(sig).is_some_and(|t| is_handle_ty(t).is_some());
    let ret_ts = match &returns_class {
        Some(c) if ret_is_handle => c.clone(),
        _ => ret_ts,
    };
    // A private member has NO ts_signature (kept out of `rts.d.ts`).
    let ts_sig = if private {
        String::new()
    } else if is_ctor {
        format!("new {class}({ps}): {class}")
    } else if is_getter {
        // Property read: `name: type`, no parens. Paired setter carries no ts.
        format!("{js_name}: {ret_ts}")
    } else if is_setter {
        String::new()
    } else {
        format!("{js_name}({ps}): {ret_ts}")
    };
    let flags = match (readonly, throws) {
        (false, false) => quote!(::rts_engine::MemberFlags::NONE),
        (true, false) => quote!(::rts_engine::MemberFlags::READONLY),
        (false, true) => quote!(::rts_engine::MemberFlags::THROWS),
        (true, true) => {
            quote!(::rts_engine::MemberFlags::READONLY.or(::rts_engine::MemberFlags::THROWS))
        }
    };

    let member = quote! {
        .member(::rts_engine::Member {
            name: #js_name.into(),
            kind: #kind_tok,
            sig: #sig_build,
            symbol: #symbol.into(),
            fn_ptr: ::rts_engine::FnPtr(#extern_ident as *const u8),
            flags: #flags,
            aliases: ::std::vec::Vec::new(),
            variadic: false,
            ts_signature: #ts_sig.into(),
            doc: #doc.into(),
            pure: false,
            emit: ::core::option::Option::None,
        })
    };

    Ok((extern_fn, member, extern_ident))
}

/// Detect + strip a `#[rtse::variable]` / `#[rtse::variable(readonly)]` on a field.
/// `Some(readonly)` when present; `None` = a plain field (not exposed).
fn take_variable(attrs: &mut Vec<syn::Attribute>) -> Option<bool> {
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

type FieldGen = (
    Vec<proc_macro2::TokenStream>,
    Vec<proc_macro2::TokenStream>,
    Vec<proc_macro2::Ident>,
);

/// Generate the getter (and setter unless `readonly`) extern + Member for one
/// `#[rtse::variable]` scalar field.
fn gen_field(
    _class: &str,
    class_upper: &str,
    self_ty: &Type,
    fname: &proc_macro2::Ident,
    fty: &Type,
    readonly: bool,
    doc: String,
) -> syn::Result<FieldGen> {
    let Some((abi, ext_ty, ts)) = scalar(fty) else {
        return Err(syn::Error::new_spanned(
            fty,
            "rtse variable: field must be f64/i64/i32/bool",
        ));
    };
    let is_bool = matches!(fty, Type::Path(p) if p.path.is_ident("bool"));
    let js_name = to_camel(&fname.to_string());
    // Field name VERBATIM (case-preserved) in the symbol — see `member` above.
    let field_upper = fname.to_string();
    let mut externs = Vec::new();
    let mut members = Vec::new();
    let mut keep = Vec::new();

    // Getter.
    let get_sym = format!("__RTS_FN_GL_{class_upper}_GET_{field_upper}");
    let get_id = format_ident!("{}", get_sym);
    let read = quote!(::rts_engine::heap::handles::with_rtse::<#self_ty, _>(__recv, |__s| match __s {
        ::core::option::Option::Some(__s) => __s.#fname,
        ::core::option::Option::None => ::core::default::Default::default(),
    }));
    let get_ret = if is_bool { quote!((#read) as i64) } else { read };
    externs.push(quote! {
        #[unsafe(no_mangle)]
        pub extern "C" fn #get_id(__recv: u64) -> #ext_ty { #get_ret }
    });
    let get_ts = format!("{js_name}: {ts}");
    members.push(quote! {
        .member(::rts_engine::Member {
            name: #js_name.into(),
            kind: ::rts_engine::MemberKind::InstanceGetter,
            sig: ::rts_engine::Sig::new(::std::vec![::rts_engine::AbiType::Handle], #abi),
            symbol: #get_sym.into(),
            fn_ptr: ::rts_engine::FnPtr(#get_id as *const u8),
            flags: ::rts_engine::MemberFlags::NONE,
            aliases: ::std::vec::Vec::new(),
            variadic: false,
            ts_signature: #get_ts.into(),
            doc: #doc.into(),
            pure: true,
            emit: ::core::option::Option::None,
        })
    });
    keep.push(get_id);

    // Setter (unless readonly).
    if !readonly {
        let set_sym = format!("__RTS_FN_GL_{class_upper}_SET_{field_upper}");
        let set_id = format_ident!("{}", set_sym);
        let val = if is_bool { quote!((__v != 0)) } else { quote!(__v) };
        externs.push(quote! {
            #[unsafe(no_mangle)]
            pub extern "C" fn #set_id(__recv: u64, __v: #ext_ty) {
                ::rts_engine::heap::handles::with_rtse_mut::<#self_ty, _>(__recv, |__s| {
                    if let ::core::option::Option::Some(__s) = __s { __s.#fname = #val; }
                });
            }
        });
        members.push(quote! {
            .member(::rts_engine::Member {
                name: #js_name.into(),
                kind: ::rts_engine::MemberKind::InstanceSetter,
                sig: ::rts_engine::Sig::new(
                    ::std::vec![::rts_engine::AbiType::Handle, #abi],
                    ::rts_engine::AbiType::Void,
                ),
                symbol: #set_sym.into(),
                fn_ptr: ::rts_engine::FnPtr(#set_id as *const u8),
                flags: ::rts_engine::MemberFlags::NONE,
                aliases: ::std::vec::Vec::new(),
                variadic: false,
                ts_signature: ::std::string::String::new(),
                doc: ::std::string::String::new(),
                pure: false,
                emit: ::core::option::Option::None,
            })
        });
        keep.push(set_id);
    }

    Ok((externs, members, keep))
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
pub fn statical(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn asynch(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn getter(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn setter(_a: TokenStream, item: TokenStream) -> TokenStream {
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
