//! `url` — WHATWG URL + URLSearchParams.
//!
//! `URL` (`instance.rs`): migrated (DRAIN_MOTOR) to `#[rtse::class]` — the class
//! is a normal Rust struct (`Url`, owned `String` components) stored via the
//! generic `Entry::Rtse`. Getters (`href`/`protocol`/`host`/…), the fully-mutable
//! WHATWG setters (`url.protocol = …`, `url.href = …`, …), `toString`/`toJSON`,
//! the live `searchParams`, and the `canParse` statics are all macro-authored.
//! Only the two `new URL(...)` CONSTRUCTORS stay hand-written (a fallible ctor
//! returning null on an invalid input — the macro's `-> Self` ctor always
//! allocates), appended here via the class-reopen pattern.
//!
//! `URLSearchParams` (`search_params.rs`): migrated. Ctor + get/has/set/delete/
//! append/sort/toString/size are `#[rtse::class]`-authored; `getAll`/`keys`/
//! `values`/`entries` (`string[]`/`[string,string][]` array returns) and
//! `forEach` (a function-VALUE callback param) stay hand-written in that file and
//! are appended here (macro gaps F8 / PolyValue callback param).

pub mod instance;
pub mod parse;
pub mod search_params;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Build a hand-written `external` [`Member`] carrying the real `fn_ptr` directly
/// (the extern lives in `instance.rs`/`search_params.rs`, this crate). The engine
/// installs the JIT symbol via the Registry harvest (`all_jit_symbols`).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    fn_ptr: *const u8,
    ts: &str,
    doc: &str,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fn_ptr),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        emit: None,
    }
}

/// Register the global class `URL`. Getters/setters/methods/statics come from the
/// `#[rtse::class]` in `instance.rs` (`instance::register`). The two fallible
/// `new URL(...)` constructors are hand-written (macro gap: a ctor returning
/// null on an unparseable input) and appended here — `ClassBuilder::done()`
/// REPLACES (does not merge) the Registry entry, so we read the macro members
/// back and re-emit them together with the ctors in a single `.done()`.
pub fn register_url_class_spec(e: &mut Engine) {
    // 1) Macro-authored getters/setters/methods/statics.
    instance::register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("URL")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    // 2) Re-open the class, replay the macro members, then append the two ctors.
    let mut cb = e
        .class("URL")
        .doc("WHATWG URL API — new URL(href[, base]) + href/protocol/host/hostname/port/pathname/search/hash/origin/username/password (getters+setters) + searchParams + toString/toJSON + static canParse.");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(m(
        "new",
        MemberKind::Constructor,
        Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
        "__RTS_FN_GL_URL_NEW",
        instance::__RTS_FN_GL_URL_NEW as *const u8,
        "new URL(url: string): URL",
        "new URL(url) — parse a URL. Returns a URL handle or 0 (null) if invalid.",
        true,
    ))
    .member(m(
        "new",
        MemberKind::Constructor,
        Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Handle),
        "__RTS_FN_GL_URL_NEW_WITH_BASE",
        instance::__RTS_FN_GL_URL_NEW_WITH_BASE as *const u8,
        "new URL(url: string, base: string): URL",
        "new URL(relative, base) — resolve a relative URL against a base URL.",
        true,
    ))
    .done();
}

/// Register the global class `URLSearchParams`. Ctor + get/has/set/delete/
/// append/sort/toString/size come from the `#[rtse::class]` in `search_params.rs`
/// (`search_params::register`). `getAll`/`keys`/`values`/`entries` (array returns
/// = F8) and `forEach` (a function-VALUE callback param) are macro gaps today —
/// they stay hand-written (same `Entry::Rtse<UrlSearchParams>`) and are APPENDED
/// here because `ClassBuilder::done()` REPLACES (does not merge) the Registry
/// entry: we read the macro members back and re-emit them with the residuals in
/// a single `.done()`.
pub fn register_urlsp_class_spec(e: &mut Engine) {
    // 1) Macro-authored ctor/instance methods (search_params.rs::register).
    search_params::register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("URLSearchParams")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    // 2) Re-open the class, replay the macro members, then append the residual
    // hand-written ones.
    let mut cb = e
        .class("URLSearchParams")
        .doc("WHATWG URLSearchParams API — get/has/set/delete/append/sort/toString/size (rtse-authored) + getAll/keys/values/entries/forEach (hand-written: array-return/callback-param macro gaps).");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(m(
        "getAll",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
        "__RTS_FN_GL_USP_GET_ALL",
        search_params::__RTS_FN_GL_USP_GET_ALL as *const u8,
        "getAll(key: string): string[]",
        "All values for a key, in append order.",
        true,
    ))
    .member(m(
        "keys",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_USP_KEYS",
        search_params::__RTS_FN_GL_USP_KEYS as *const u8,
        "keys(): string[]",
        "Vec of string handles with the keys.",
        true,
    ))
    .member(m(
        "values",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_USP_VALUES",
        search_params::__RTS_FN_GL_USP_VALUES as *const u8,
        "values(): string[]",
        "Vec of string handles with the values.",
        true,
    ))
    .member(m(
        "entries",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_USP_ENTRIES",
        search_params::__RTS_FN_GL_USP_ENTRIES as *const u8,
        "entries(): [string, string][]",
        "Array of [key, value] pairs in order (with duplicates).",
        true,
    ))
    .member(m(
        "forEach",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle, AbiType::PolyValue], AbiType::Handle),
        "__RTS_FN_GL_USP_FOR_EACH",
        search_params::__RTS_FN_GL_USP_FOR_EACH as *const u8,
        "forEach(callback: (value: string, key: string, searchParams: URLSearchParams) => void): void",
        "Invoke callback(value, key, self) per pair (snapshot).",
        false,
    ))
    .done();
}
