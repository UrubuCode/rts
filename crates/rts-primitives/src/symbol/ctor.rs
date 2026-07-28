//! `Symbol`'s ctor + `for`/`keyFor` statics — `#[rtse::class("Symbol")]`,
//! merged onto the same Registry class the hand-written instance members
//! (`description`/`toString`/well-known constants, in `mod.rs`) register.
//!
//! `Symbol` allocates its OWN dedicated `Entry::Symbol { description }` (never
//! a boxed `Entry::Rtse<T>`), so the ctor uses the SAME `-> Handle` escape
//! hatch `Proxy` pioneered (`rts-primitives/src/proxy/mod.rs`,
//! `rts-macro/tests/ctor_handle_escape.rs`): a `#[rtse::ctor]` returning
//! `Handle` verbatim skips `alloc_rtse` entirely. `SymbolCtor` is a marker
//! `struct` unit — no Rust-side fields, only a place to hang the ctor/statics
//! `impl` on.
//!
//! `for`/`keyFor` are `#[rtse::statical]` (no receiver, ordinary `Handle`
//! in/out) — that shape has no boxed-`self` requirement, so it converts
//! cleanly. `description`/`toString`, by contrast, are INSTANCE members that
//! read a live `Entry::Symbol`'s field; the macro's non-value instance-method/
//! getter path always unboxes the receiver via `with_rtse::<T>` (looking for
//! an `Entry::Rtse<T>` box), which a `Symbol` handle never is — so those stay
//! hand-written builder `Member`s in `mod.rs` (see that file's module doc).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{Entry, alloc_entry};

use super::registry;

/// Marker type only — see module doc. `Entry::Symbol` is allocated directly by
/// the ctor body, never via `alloc_rtse`.
#[rtse::class("Symbol")]
#[derive(Clone)]
pub struct SymbolCtor;

#[rtse::class("Symbol")]
impl SymbolCtor {
    /// `new Symbol(description?)` / `Symbol(description?)` — always allocates a
    /// FRESH, unique `Entry::Symbol`. An omitted description (or an explicit
    /// `undefined`, indistinguishable at this ABI level — the same documented
    /// limitation `String`/`Boolean`/`Number`'s ctors already carry) reads as
    /// `None`. Matches JS: `Symbol().description === Symbol(undefined)
    /// .description === undefined` (unlike `Number()` vs `Number(undefined)`,
    /// Symbol's spec does NOT distinguish the two cases).
    #[rtse::ctor(optional = 1)]
    fn new(description: Option<&str>) -> Handle {
        alloc_entry(Entry::Symbol { description: description.map(|s| s.to_string()) })
    }

    /// `Symbol.for(key)` — registry lookup/create; the same key always returns
    /// the same handle.
    #[rtse::statical(name = "for")]
    fn for_key(key: &str) -> Handle {
        let key_owned = key.to_string();
        let reg = registry().lock().unwrap();
        if let Some(&h) = reg.get(&key_owned) {
            return h;
        }
        drop(reg);
        let h = alloc_entry(Entry::Symbol { description: Some(key_owned.clone()) });
        let mut reg = registry().lock().unwrap();
        reg.insert(key_owned, h);
        h
    }

    /// `Symbol.keyFor(sym)` — the registered key, or the literal `"undefined"`
    /// string sentinel when `sym` isn't registered. That sentinel (rather than
    /// real JS `undefined`) is the PRE-EXISTING behavior this conversion keeps
    /// byte-for-byte (a historical bridge convention, same one `strops::at`'s
    /// out-of-range case documents) — not something this conversion introduces.
    #[rtse::statical(name = "keyFor")]
    fn key_for(sym: Handle) -> String {
        let reg = registry().lock().unwrap();
        for (k, &h) in reg.iter() {
            if h == sym {
                return k.clone();
            }
        }
        "undefined".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_symbols_different_handles() {
        // Absent description: empty StrPtr slice (ptr/len both zero-ish), the
        // same "absent = empty string" convention every optional-`&str` ctor
        // in this codebase uses at the raw-extern call boundary.
        let a = __rtsm_global_symbol_new(0, 0);
        let b = __rtsm_global_symbol_new(0, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn for_same_key_returns_same_handle() {
        let key = b"my_key_ctor_test";
        let a = __rtsm_global_symbol_for_key(key.as_ptr() as *const u8 as i64, key.len() as i64);
        let b = __rtsm_global_symbol_for_key(key.as_ptr() as *const u8 as i64, key.len() as i64);
        assert_eq!(a, b);
    }

    #[test]
    fn key_for_returns_registered_key() {
        let key = b"another_key_ctor_test";
        let h = __rtsm_global_symbol_for_key(key.as_ptr() as i64, key.len() as i64);
        let result = __rtsm_global_symbol_key_for(h);
        let s = rts_engine::heap::handles::with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "another_key_ctor_test");
    }

    #[test]
    fn key_for_unregistered_returns_undefined_sentinel() {
        let h = __rtsm_global_symbol_new(0, 0);
        let result = __rtsm_global_symbol_key_for(h);
        let s = rts_engine::heap::handles::with_entry(result, |e| match e {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        });
        assert_eq!(s.unwrap(), "undefined");
    }
}
