//! `Symbol`'s remaining instance members — `description` (getter) and
//! `toString` — `#[rtse::class("Symbol")]`, merged onto the same Registry
//! class `ctor.rs`'s ctor/`for`/`keyFor` and `mod.rs`'s well-known constants
//! register.
//!
//! `Entry::Symbol { description }` is a DEDICATED `Entry::` variant (see
//! `ctor.rs`'s doc) — never a boxed `Entry::Rtse<T>` — so the macro's ordinary
//! instance-method/getter path (which unboxes the receiver via
//! `with_rtse::<T>`) cannot reach it. These two members instead use the
//! RAW-HANDLE receiver mode: no `self` at all, and the first (and only) typed
//! param is `SelfHandle` (`rts-macro/src/class/member/body.rs`). The macro
//! hands the receiver's raw handle straight through as that param — exactly
//! the same handle `with_entry` reads below, just with no `Entry::Rtse` box
//! and no clone-out/write-back around the call.
//!
//! `SymbolInstance` is a marker `struct` unit — no Rust-side fields, only a
//! place to hang this `impl` on, mirroring `SymbolCtor` in `ctor.rs`.

use rts_engine::abi::ty::SelfHandle;
use rts_engine::heap::handles::{Entry, with_entry};

/// Marker type only — see module doc.
#[rtse::class("Symbol")]
#[derive(Clone)]
pub struct SymbolInstance;

#[rtse::class("Symbol")]
impl SymbolInstance {
    /// Returns the symbol's description string, or `null` if none.
    ///
    /// `-> Option<String>`, NOT `-> Handle`: the macro derives the ts type from
    /// the Rust return type, and `Handle` maps to ts `object` — which makes the
    /// engine rebox a perfectly good string handle as an object (it printed
    /// `[]`). `Option<String>` carries ts `string | null`, the same contract the
    /// hand-written `Member` declared, and the macro allocates the string-pool
    /// handle itself (`None` → `0`, byte-identical to the old return).
    #[rtse::getter]
    fn description(recv: SelfHandle) -> Option<String> {
        with_entry(recv, |e| match e {
            Some(Entry::Symbol { description }) => description.clone(),
            _ => None,
        })
    }

    /// Returns `'Symbol(description)'` (or `'Symbol()'` with no description).
    /// `-> String` (not `Handle`) for the same reason `description` is
    /// `Option<String>`: ts `string`, string-pool alloc done by the macro.
    #[rtse::method(name = "toString")]
    fn js_to_string(recv: SelfHandle) -> String {
        with_entry(recv, |e| match e {
            Some(Entry::Symbol {
                description: Some(d),
            }) => format!("Symbol({d})"),
            Some(Entry::Symbol { description: None }) => "Symbol()".to_string(),
            _ => "[invalid Symbol]".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::alloc_entry;

    #[test]
    fn description_returns_string() {
        let h = alloc_entry(Entry::Symbol {
            description: Some("hello".to_string()),
        });
        let result = __rtsm_global_Symbol_description(h);
        let s = with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "hello");
    }

    #[test]
    fn description_none_returns_zero() {
        let h = alloc_entry(Entry::Symbol { description: None });
        assert_eq!(__rtsm_global_Symbol_description(h), 0);
    }

    #[test]
    fn to_string_with_description() {
        let h = alloc_entry(Entry::Symbol {
            description: Some("foo".to_string()),
        });
        let result = __rtsm_global_Symbol_js_to_string(h);
        let s = with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "Symbol(foo)");
    }

    #[test]
    fn to_string_without_description() {
        let h = alloc_entry(Entry::Symbol { description: None });
        let result = __rtsm_global_Symbol_js_to_string(h);
        let s = with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "Symbol()");
    }
}
