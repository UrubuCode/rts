//! `FormData` global class (#72).
//!
//! DRAIN_MOTOR §11 (owner 2026-07-24): reimplemented as `#[rtse::class]`,
//! ported from the deleted hand-written builder (`git show HEAD~1:crates/
//! rts-std/src/globals/form_data/mod.rs` before this session's sweep) at
//! parity with the LIVE `.ts` surface (`crates/rts-shared/src/stdlib/
//! webapi.ts`'s `class FormData`), which always stringifies both `name` and
//! `value` at `append`/`set` time (`"" + name`/`"" + value`) — same semantics
//! as this `(String, String)` multimap storage.
//!
//! Multimap case-SENSITIVE (unlike `Headers`) preserving insertion order +
//! duplicates. Storage: `Vec<(String, String)>` via the generic `Entry::Rtse`
//! (same pattern as `UrlSearchParams`/`Headers`). `append`/`set`/`delete`/`get`/
//! `has`/`getAll`/`keys`/`values` are `#[rtse::class]`-authored (`getAll`/
//! `keys`/`values` use F8's flat `Vec<String>` return, same as `Headers`'s
//! `getSetCookie`/`keys`/`values`). `entries()` (nested `[string,string][]`,
//! F8 doesn't cover nested arrays) and `forEach` (function-VALUE callback, no
//! `PolyValue` param support) stay hand-written residuals reading the SAME
//! struct via `with_rtse` — one storage representation, two authoring styles
//! (identical pattern to `headers::register_headers_class_spec`).
//!
//! **Known gap vs the `.ts` / the spec:** `value` is typed `&str` here (like
//! the deleted hand-written Rust), so only string-coercible-at-the-callsite
//! arguments work; the `.ts` accepts `any` and stringifies it (`"" + value`),
//! which also covers e.g. a `Blob`/`File` value stringified to
//! `"[object Blob]"`-ish text — same string-only behavior in practice, but a
//! non-string arg that the engine can't coerce to `&str` bails instead of
//! stringifying. `get()`/multi-value lookups return `""` for an absent key
//! (WHATWG says `null`) — same documented deviation as `UrlSearchParams::get`
//! (a nullable-`String` return isn't expressible yet). No `Symbol.iterator`
//! (the `.ts`'s `*[Symbol.iterator]` generator) — `for...of fd` won't iterate;
//! `entries()`/`forEach` cover the common cases.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::with_rtse;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Ordered `(name, value)` pairs — insertion order + duplicates preserved,
/// case-sensitive names.
#[rtse::class("FormData")]
#[derive(Clone, Default)]
pub struct FormData {
    pairs: Vec<(String, String)>,
}

#[rtse::class("FormData")]
impl FormData {
    /// `new FormData()`.
    #[rtse::ctor]
    fn new() -> Self {
        FormData::default()
    }

    /// `fd.append(name, value)` — always appends, preserving duplicates.
    #[rtse::method]
    fn append(self: &mut FormData, name: &str, value: &str) {
        self.pairs.push((name.to_string(), value.to_string()));
    }

    /// `fd.set(name, value)` — replaces the FIRST existing occurrence with the
    /// new value, drops any other duplicates; appends if absent (WHATWG spec —
    /// same semantics as `UrlSearchParams::set`).
    #[rtse::method]
    fn set(self: &mut FormData, name: &str, value: &str) {
        let mut replaced = false;
        let mut new_pairs: Vec<(String, String)> = Vec::with_capacity(self.pairs.len());
        for (k, v) in self.pairs.drain(..) {
            if k == name {
                if !replaced {
                    new_pairs.push((k, value.to_string()));
                    replaced = true;
                }
            } else {
                new_pairs.push((k, v));
            }
        }
        if !replaced {
            new_pairs.push((name.to_string(), value.to_string()));
        }
        self.pairs = new_pairs;
    }

    /// `fd.delete(name)` — removes ALL occurrences.
    #[rtse::method]
    fn delete(self: &mut FormData, name: &str) {
        self.pairs.retain(|(k, _)| k != name);
    }

    /// `fd.get(name)` — first value, or `""` if absent (see module doc: the
    /// WHATWG `null` isn't expressible from a `String`-typed return yet).
    #[rtse::method]
    fn get(self: &FormData, name: &str) -> String {
        self.pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    /// `fd.has(name)`.
    #[rtse::method]
    fn has(self: &FormData, name: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == name)
    }

    /// `fd.getAll(name)` — every value for `name`, in append order (F8 flat
    /// `Vec<String>` return).
    #[rtse::method(name = "getAll")]
    fn get_all(self: &FormData, name: &str) -> Vec<String> {
        self.pairs
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// `fd.keys()` — names, insertion order + duplicates (F8).
    #[rtse::method]
    fn keys(self: &FormData) -> Vec<String> {
        self.pairs.iter().map(|(k, _)| k.clone()).collect()
    }

    /// `fd.values()` — values, insertion order + duplicates (F8).
    #[rtse::method]
    fn values(self: &FormData) -> Vec<String> {
        self.pairs.iter().map(|(_, v)| v.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// Hand-written residual (macro gaps): nested-array return (`entries()` →
// `[string,string][]`, F8 doesn't cover nested arrays) and a function-VALUE
// callback param (`forEach`, no `PolyValue` param support yet). Both read the
// SAME `FormData` struct via `with_rtse` — one storage representation, two
// authoring styles (identical to `headers.rs`/`search_params.rs`).
// ---------------------------------------------------------------------------

/// Snapshot of the pairs (mirrors `search_params::pairs_snapshot` — a
/// mutating callback during `forEach` iterates the list as it was at call
/// time).
fn pairs_snapshot(h: Handle) -> Vec<(String, String)> {
    with_rtse::<FormData, _>(h, |s| s.map(|s| s.pairs.clone()).unwrap_or_default())
}

/// `fd.entries()` — an array of `[name, value]` string pairs (insertion
/// order, with duplicates). Each pair is a boxed 2-element array word.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_ENTRIES(h: Handle) -> Handle {
    use rts_engine::heap::handles::{Entry, alloc_entry};
    use rts_engine::heap::shapes::{handle_word_auto, string_word};
    let mut out: Vec<i64> = Vec::new();
    for (k, v) in pairs_snapshot(h) {
        let pair = alloc_entry(Entry::Vec(Box::new(vec![
            string_word(k.as_bytes()) as i64,
            string_word(v.as_bytes()) as i64,
        ])));
        out.push(handle_word_auto(pair) as i64);
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// `fd.forEach(callback)` — invokes `callback(value, name, fd)` per pair, in
/// insertion order (over a snapshot). Returns `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_FOR_EACH(h: Handle, cb: Handle) -> Handle {
    use rts_engine::heap::poly::POLY_UNDEFINED;
    use rts_engine::heap::shapes::{handle_word_auto, string_word};
    let self_word = handle_word_auto(h);
    for (k, v) in pairs_snapshot(h) {
        let vw = string_word(v.as_bytes());
        let kw = string_word(k.as_bytes());
        crate::gc_surface::__rtsadp_fn_invoke(cb, vw, kw, self_word, POLY_UNDEFINED, POLY_UNDEFINED);
    }
    POLY_UNDEFINED
}

#[allow(clippy::too_many_arguments)]
fn member(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        emit: None,
    }
}

/// Reopens the macro-generated `FormData` class (ctor/append/set/delete/get/
/// has/getAll/keys/values) and appends the hand-written residual (`entries`/
/// `forEach`) — same pattern as `headers::register_headers_class_spec`.
pub fn register_form_data_class_spec(e: &mut Engine) {
    register(e);
    let macro_members: Vec<Member> = e
        .registry()
        .class("FormData")
        .map(|c| c.members.clone())
        .unwrap_or_default();
    let mut cb = e
        .class("FormData")
        .doc("FormData — case-sensitive multimap of form fields (order + duplicates).");
    for mem in macro_members {
        cb = cb.member(mem);
    }
    cb.member(member(
        "entries",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_FORM_DATA_ENTRIES",
        "entries(): IterableIterator<[string, string]>",
        "fd.entries() — Vec of [name, value] string pairs (order + duplicates).",
        __RTS_FN_GL_FORM_DATA_ENTRIES as *const u8,
        true,
    ))
    .member(member(
        "forEach",
        MemberKind::InstanceMethod,
        Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
        "__RTS_FN_GL_FORM_DATA_FOR_EACH",
        "forEach(callback: (value: string, key: string, fd: FormData) => void): void",
        "fd.forEach(callback) — invokes callback(value, name, self) per pair (snapshot).",
        __RTS_FN_GL_FORM_DATA_FOR_EACH as *const u8,
        false,
    ))
    .done();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::{Entry, alloc_rtse, with_entry};

    fn h(pairs: &[(&str, &str)]) -> u64 {
        alloc_rtse(
            "FormData",
            FormData {
                pairs: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            },
        )
    }

    #[test]
    fn append_preserves_dupes() {
        let handle = h(&[]);
        __rtsm_global_formdata_append(handle, "a".as_ptr() as i64, 1, "1".as_ptr() as i64, 1);
        __rtsm_global_formdata_append(handle, "a".as_ptr() as i64, 1, "2".as_ptr() as i64, 1);
        with_rtse::<FormData, _>(handle, |s| {
            assert_eq!(
                s.unwrap().pairs,
                vec![("a".into(), "1".into()), ("a".into(), "2".into())]
            );
        });
    }

    #[test]
    fn set_replaces_first_drops_dupes() {
        let handle = h(&[("a", "1"), ("a", "x"), ("b", "2")]);
        __rtsm_global_formdata_set(handle, "a".as_ptr() as i64, 1, "9".as_ptr() as i64, 1);
        with_rtse::<FormData, _>(handle, |s| {
            let s = s.unwrap();
            assert_eq!(s.pairs, vec![("a".into(), "9".into()), ("b".into(), "2".into())]);
        });
    }

    #[test]
    fn delete_removes_all() {
        let handle = h(&[("a", "1"), ("a", "2"), ("b", "3")]);
        __rtsm_global_formdata_delete(handle, "a".as_ptr() as i64, 1);
        with_rtse::<FormData, _>(handle, |s| {
            assert_eq!(s.unwrap().pairs, vec![("b".into(), "3".into())]);
        });
    }


    #[test]
    fn entries_returns_pairs() {
        let handle = h(&[("a", "1"), ("b", "2")]);
        let out = __RTS_FN_GL_FORM_DATA_ENTRIES(handle);
        let count = with_entry(out, |e| match e {
            Some(Entry::Vec(v)) => v.len(),
            _ => 0,
        });
        assert_eq!(count, 2);
    }
}
