//! `Symbol` well-known-symbol constants — the single source `#[rtse::symbol(...)]`
//! points at. Declaring the well-known set as associated items (instead of a
//! bare `&str` list) means a typo in `Symbol::iteratorr` is a Rust compile error
//! at the USE site (`crates/rts-macro` emits a reference to the path verbatim),
//! not a silent `well_known_handle(name) -> 0` at runtime.
//!
//! `Symbol.for("iterator")` (a REGISTRY symbol, keyed by string) and
//! `Symbol.iterator` (a WELL-KNOWN symbol, unique identity) are different JS
//! values. `Symbol` here only names the well-known set; the registry form is
//! `__RTS_FN_GL_SYMBOL_FOR` in `mod.rs`.

#![allow(non_upper_case_globals)] // JS-cased names (`Symbol.iterator`), not Rust consts.

use super::well_known_handle;

/// One well-known symbol slot. `key` is the exact text `well_known_handle`
/// matches on — the associated consts below are the only place that string is
/// written.
pub struct WellKnown {
    key: &'static str,
}

impl WellKnown {
    /// Resolves (and lazily allocates, first call) the cached handle for this
    /// well-known symbol. Same key always returns the same handle.
    pub fn handle(&self) -> u64 {
        well_known_handle(self.key)
    }

    /// The Registry member key this well-known symbol is stored under:
    /// `@@<key>`. The SINGLE SOURCE for the `@@` wire spelling — the macro
    /// reads this at registration time instead of re-deriving the key from the
    /// Rust ident, so a Rust spelling that must differ from the JS name (e.g.
    /// `matcher` for `Symbol.match`, a reserved word) cannot desync the two.
    pub fn member_key(&self) -> String {
        format!("@@{}", self.key)
    }
}

/// Namespace for the well-known symbols (ECMA-262 §Well-Known Symbols, the
/// subset RTS implements). Not instantiated — a marker type whose associated
/// consts are the source of truth `#[rtse::symbol(Symbol::iterator)]` checks
/// against at compile time.
pub struct Symbol;

impl Symbol {
    pub const iterator: WellKnown = WellKnown { key: "iterator" };
    pub const asyncIterator: WellKnown = WellKnown { key: "asyncIterator" };
    pub const hasInstance: WellKnown = WellKnown { key: "hasInstance" };
    pub const toPrimitive: WellKnown = WellKnown { key: "toPrimitive" };
    pub const toStringTag: WellKnown = WellKnown { key: "toStringTag" };
    pub const isConcatSpreadable: WellKnown = WellKnown {
        key: "isConcatSpreadable",
    };
    // `match` is a Rust keyword, so the Rust spelling is `matcher` instead of a
    // raw identifier — easier to use at every call site. The JS name
    // (`Symbol.match`) is unaffected; `key` (not the const's ident) is what the
    // macro reads via `member_key()`, so this Rust/JS spelling mismatch cannot
    // desync the Registry key from the JS name.
    pub const matcher: WellKnown = WellKnown { key: "match" };
    pub const replace: WellKnown = WellKnown { key: "replace" };
    pub const search: WellKnown = WellKnown { key: "search" };
    pub const split: WellKnown = WellKnown { key: "split" };
    pub const species: WellKnown = WellKnown { key: "species" };
    pub const unscopables: WellKnown = WellKnown { key: "unscopables" };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_stable_across_calls() {
        assert_eq!(Symbol::iterator.handle(), Symbol::iterator.handle());
    }

    #[test]
    fn distinct_well_knowns_distinct_handles() {
        assert_ne!(Symbol::iterator.handle(), Symbol::asyncIterator.handle());
    }

    #[test]
    fn match_keyword_name_resolves() {
        assert_ne!(Symbol::matcher.handle(), 0);
    }

    /// Regression guard: `Symbol::matcher`'s Rust ident ("matcher") differs from
    /// its JS key ("match"). `member_key()` must derive from `key`, not from the
    /// const's Rust spelling — else this would wrongly produce `@@matcher`.
    #[test]
    fn member_key_derives_from_js_key_not_rust_ident() {
        assert_eq!(Symbol::matcher.member_key(), "@@match");
        assert_eq!(Symbol::iterator.member_key(), "@@iterator");
    }
}
