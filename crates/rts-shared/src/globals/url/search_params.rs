//! `URLSearchParams` — WHATWG multimap of query pairs.
//!
//! Migrated (DRAIN_MOTOR §8-9) from the hand-written `Entry::Vec<i64>` builder to
//! `#[rtse::class]` (macro G4 + F1 + F4): the instance is a normal Rust struct
//! (`Vec<(String, String)>`) stored via the generic `Entry::Rtse`. Everything the
//! macro can express (ctor with an optional init string, get/has/set/delete/
//! append/sort/toString/size — all scalar/`&str`/`String`/`bool` params+returns)
//! is authored here; `register` (macro-generated) wires it.
//!
//! **Left hand-written** (in this same file, appended by `url::register_urlsp_class_spec`
//! after the macro's `register`): `getAll`/`keys`/`values`/`entries` (return a
//! JS `string[]`/`[string,string][]` — array-classified returns are macro gap F8,
//! not yet supported) and `forEach` (takes a callback VALUE — the macro's param
//! surface is scalar/`&str`/`Handle`/`U64` only, no `PolyValue`/function-value
//! param). These stay ordinary `#[no_mangle] extern "C"` fns, but now read the
//! SAME `UrlSearchParams` struct via `with_rtse`, so both authoring styles agree
//! on one storage representation.

use rts_engine::heap::handles::{Entry, alloc_entry, with_rtse};

use super::parse::percent_encode_form;

/// (cross-runtime #746) Percent-decode a URL query component: `%XX` -> byte
/// 0xXX; `+` -> ' '; invalid bytes kept as-is. Lossy UTF-8.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            let h = bytes[i + 1];
            let l = bytes[i + 2];
            let hv = match h {
                b'0'..=b'9' => Some(h - b'0'),
                b'a'..=b'f' => Some(h - b'a' + 10),
                b'A'..=b'F' => Some(h - b'A' + 10),
                _ => None,
            };
            let lv = match l {
                b'0'..=b'9' => Some(l - b'0'),
                b'a'..=b'f' => Some(l - b'a' + 10),
                b'A'..=b'F' => Some(l - b'A' + 10),
                _ => None,
            };
            if let (Some(hv), Some(lv)) = (hv, lv) {
                out.push((hv << 4) | lv);
                i += 3;
                continue;
            }
        }
        if b == b'+' {
            out.push(b' ');
        } else {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses a raw query string into `(key, value)` pairs, preserving insertion
/// order + duplicates.
pub(super) fn parse_query_pairs(s: &str) -> Vec<(String, String)> {
    let s = s.strip_prefix('?').unwrap_or(s);
    if s.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(String, String)> = Vec::new();
    for pair in s.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            out.push((url_decode(k), url_decode(v)));
        } else if !pair.is_empty() {
            out.push((url_decode(pair), String::new()));
        }
    }
    out
}

fn intern_str(s: &str) -> u64 {
    alloc_entry(Entry::String(s.as_bytes().to_vec()))
}

/// `URLSearchParams` instance state: the ordered `(key, value)` multimap. Not
/// `#[rtse::variable]` — no field is a JS-visible scalar property (the JS
/// surface is entirely methods + the computed `size`).
#[rtse::class("URLSearchParams")]
#[derive(Clone)]
pub struct UrlSearchParams {
    pairs: Vec<(String, String)>,
}

impl UrlSearchParams {
    /// Build directly from parsed pairs (used by `url.searchParams`, which
    /// already parsed the URL's `search` component).
    pub(super) fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        UrlSearchParams { pairs }
    }

    pub(super) fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }
}

#[rtse::class("URLSearchParams")]
impl UrlSearchParams {
    /// `new URLSearchParams(init?: string)` — `init` is `"a=1&b=2"`.
    #[rtse::ctor(optional = 1)]
    fn new(init: &str) -> Self {
        UrlSearchParams {
            pairs: parse_query_pairs(init),
        }
    }

    /// `get(key)` — first occurrence's value, or `""` if absent.
    ///
    /// Returns `String` (ts `): string`) so the engine boxes the result as a string
    /// word (`ret_is_string_handle`). WHATWG returns `null` for an absent key —
    /// the `-> Option<String>` return maps `None` → JS `null` (ts `string | null`),
    /// `Some` → the value string.
    #[rtse::method]
    fn get(self: &UrlSearchParams, key: &str) -> Option<String> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    /// `has(key)`.
    #[rtse::method]
    fn has(self: &UrlSearchParams, key: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == key)
    }

    /// `set(key, value)` — JS spec: replace the FIRST existing occurrence with
    /// the new value; drop any other duplicates. Appends if absent.
    ///
    /// (Behavior note, DRAIN_MOTOR migration: the hand-written extern used to
    /// return the receiver's own Handle — an undocumented internal quirk, not
    /// the JS spec, which says `set` returns `undefined`. Returning a self
    /// Handle isn't expressible from an idiomatic `&mut self` method [the
    /// struct doesn't know its own handle], so this now returns `void`,
    /// matching spec. Explicit, justified regression per the REGRESS-WHEN-
    /// NECESSARY rule — no test exercised the old chaining return.)
    #[rtse::method]
    fn set(self: &mut UrlSearchParams, key: &str, value: &str) {
        let mut replaced = false;
        let mut new_pairs: Vec<(String, String)> = Vec::with_capacity(self.pairs.len());
        for (k, v) in self.pairs.drain(..) {
            if k == key {
                if !replaced {
                    new_pairs.push((k, value.to_string()));
                    replaced = true;
                }
                // else: drop duplicate.
            } else {
                new_pairs.push((k, v));
            }
        }
        if !replaced {
            new_pairs.push((key.to_string(), value.to_string()));
        }
        self.pairs = new_pairs;
    }

    /// `delete(key)` — removes ALL occurrences. (See `set`'s doc: returns
    /// `void`, not the old self-Handle quirk.)
    #[rtse::method]
    fn delete(self: &mut UrlSearchParams, key: &str) {
        self.pairs.retain(|(k, _)| k != key);
    }

    /// `append(key, value)` — always appends, preserving duplicates.
    #[rtse::method]
    fn append(self: &mut UrlSearchParams, key: &str, value: &str) {
        self.pairs.push((key.to_string(), value.to_string()));
    }

    /// `sort()` — stable sort by key (JS spec: UTF-16 code unit order;
    /// approximated by byte order, same as the prior implementation).
    #[rtse::method]
    fn sort(self: &mut UrlSearchParams) {
        self.pairs.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// `toString()` — `application/x-www-form-urlencoded` serialization.
    #[rtse::method]
    fn to_string(self: &UrlSearchParams) -> String {
        self.pairs
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode_form(k), percent_encode_form(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// `size` — number of pairs (WHATWG getter, v19+). Read as a property
    /// (`usp.size`, no parens) via the engine's generic 0-arg-instance-method
    /// property-read fallback (`try_global_class_member`) — no distinct
    /// `InstanceGetter` kind needed.
    #[rtse::method]
    fn size(self: &UrlSearchParams) -> i64 {
        self.pairs.len() as i64
    }
}

// ---------------------------------------------------------------------------
// Hand-written residual members (macro gaps): array-return (`string[]`/
// `[string,string][]`, F8) and a function-VALUE callback param (`forEach`,
// no `PolyValue` param support yet). All read the SAME `UrlSearchParams`
// struct via `with_rtse` — one storage representation, two authoring styles.
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn __rtsadp_fn_invoke(f: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64;
}

/// Snapshot of the pairs — WHATWG semantics: `forEach`'s callback (which may
/// mutate the params) iterates the list as it was at call time.
fn pairs_snapshot(self_h: u64) -> Vec<(String, String)> {
    with_rtse::<UrlSearchParams, _>(self_h, |s| {
        s.map(|s| s.pairs.clone()).unwrap_or_default()
    })
}

/// `usp.getAll(key)` — all values for `key`, in append order.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_GET_ALL(self_h: u64, key_ptr: *const u8, key_len: i64) -> u64 {
    if key_ptr.is_null() || key_len < 0 {
        return alloc_entry(Entry::Vec(Box::new(Vec::new())));
    }
    let key = unsafe {
        let s = std::slice::from_raw_parts(key_ptr, key_len as usize);
        std::str::from_utf8(s).unwrap_or("")
    };
    let vals: Vec<i64> = with_rtse::<UrlSearchParams, _>(self_h, |s| {
        s.map(|s| {
            s.pairs
                .iter()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| intern_str(v) as i64)
                .collect()
        })
        .unwrap_or_default()
    });
    alloc_entry(Entry::Vec(Box::new(vals)))
}

/// `usp.keys()` — a `Vec` of string handles (order + duplicates preserved).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_KEYS(self_h: u64) -> u64 {
    let out: Vec<i64> = with_rtse::<UrlSearchParams, _>(self_h, |s| {
        s.map(|s| s.pairs.iter().map(|(k, _)| intern_str(k) as i64).collect())
            .unwrap_or_default()
    });
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// `usp.values()` — a `Vec` of string handles (order + duplicates preserved).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_VALUES(self_h: u64) -> u64 {
    let out: Vec<i64> = with_rtse::<UrlSearchParams, _>(self_h, |s| {
        s.map(|s| s.pairs.iter().map(|(_, v)| intern_str(v) as i64).collect())
            .unwrap_or_default()
    });
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// `usp.entries()` — an array of `[key, value]` string pairs (insertion order,
/// with duplicates). Each pair is a boxed 2-element array word.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_ENTRIES(self_h: u64) -> u64 {
    use rts_engine::heap::shapes::{handle_word_auto, string_word};
    let mut outer: Vec<i64> = Vec::new();
    for (k, v) in pairs_snapshot(self_h) {
        let inner = alloc_entry(Entry::Vec(Box::new(vec![
            string_word(k.as_bytes()) as i64,
            string_word(v.as_bytes()) as i64,
        ])));
        outer.push(handle_word_auto(inner) as i64);
    }
    alloc_entry(Entry::Vec(Box::new(outer)))
}

/// `usp.forEach(callback)` — invoke `callback(value, key, usp)` for each pair,
/// in insertion order (over a snapshot). Returns `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_FOR_EACH(self_h: u64, cb: u64) -> u64 {
    use rts_engine::heap::poly::POLY_UNDEFINED;
    use rts_engine::heap::shapes::{handle_word_auto, string_word};
    let self_word = handle_word_auto(self_h);
    for (k, v) in pairs_snapshot(self_h) {
        let vw = string_word(v.as_bytes());
        let kw = string_word(k.as_bytes());
        unsafe {
            __rtsadp_fn_invoke(cb, vw, kw, self_word, POLY_UNDEFINED, POLY_UNDEFINED);
        }
    }
    POLY_UNDEFINED
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::{alloc_rtse, with_entry};

    fn h(pairs: &[(&str, &str)]) -> u64 {
        alloc_rtse("URLSearchParams", UrlSearchParams::from_pairs(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ))
    }

    #[test]
    fn get_has_size() {
        let handle = h(&[("a", "1"), ("b", "2")]);
        assert_eq!(__rtsm_global_urlsearchparams_size(handle), 2);
        assert!(__rtsm_global_urlsearchparams_has(
            handle,
            "a".as_ptr() as i64,
            1
        ) != 0);
        let vh = __rtsm_global_urlsearchparams_get(handle, "a".as_ptr() as i64, 1);
        assert!(vh != 0);
    }

    #[test]
    fn set_replaces_first_drops_dupes() {
        let handle = h(&[("a", "1"), ("a", "x"), ("b", "2")]);
        __rtsm_global_urlsearchparams_set(handle, "a".as_ptr() as i64, 1, "9".as_ptr() as i64, 1);
        with_rtse::<UrlSearchParams, _>(handle, |s| {
            let s = s.unwrap();
            assert_eq!(s.pairs, vec![("a".into(), "9".into()), ("b".into(), "2".into())]);
        });
    }

    #[test]
    fn delete_removes_all() {
        let handle = h(&[("a", "1"), ("a", "2"), ("b", "3")]);
        __rtsm_global_urlsearchparams_delete(handle, "a".as_ptr() as i64, 1);
        with_rtse::<UrlSearchParams, _>(handle, |s| {
            assert_eq!(s.unwrap().pairs, vec![("b".into(), "3".into())]);
        });
    }

    #[test]
    fn append_preserves_dupes() {
        let handle = h(&[]);
        __rtsm_global_urlsearchparams_append(handle, "a".as_ptr() as i64, 1, "1".as_ptr() as i64, 1);
        __rtsm_global_urlsearchparams_append(handle, "a".as_ptr() as i64, 1, "2".as_ptr() as i64, 1);
        with_rtse::<UrlSearchParams, _>(handle, |s| {
            assert_eq!(
                s.unwrap().pairs,
                vec![("a".into(), "1".into()), ("a".into(), "2".into())]
            );
        });
    }

    #[test]
    fn sort_orders_by_key() {
        let handle = h(&[("b", "2"), ("a", "1")]);
        __rtsm_global_urlsearchparams_sort(handle);
        with_rtse::<UrlSearchParams, _>(handle, |s| {
            assert_eq!(s.unwrap().pairs, vec![("a".into(), "1".into()), ("b".into(), "2".into())]);
        });
    }

    #[test]
    fn to_string_form_encodes() {
        let handle = h(&[("a b", "c+d")]);
        let sh = __rtsm_global_urlsearchparams_to_string(handle);
        let s = with_entry(sh, |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        });
        assert_eq!(s, "a+b=c%2Bd");
    }

    #[test]
    fn get_all_keys_values_entries() {
        let handle = h(&[("a", "1"), ("a", "2")]);
        let all = __RTS_FN_GL_USP_GET_ALL(handle, "a".as_ptr(), 1);
        let count = with_entry(all, |e| match e {
            Some(Entry::Vec(v)) => v.len(),
            _ => 0,
        });
        assert_eq!(count, 2);
    }
}
