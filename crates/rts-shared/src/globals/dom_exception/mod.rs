//! `DOMException` — the web platform's exception class (#77). NOT a
//! primordial (no native syntax); a rts-shared universal utility class, same
//! bucket as `Point`/`URL`.
//!
//! DRAIN_MOTOR §11 (owner 2026-07-24 correction): reimplemented as
//! `#[rtse::class]`, replacing the ambient `.ts` `DOMEXCEPTION_TS` prelude
//! (`rts-shared/src/stdlib/domexception.ts`, now removed) AND the earlier
//! hand-written builder that was superseded by it (recovered from git history
//! via `git show HEAD:crates/rts-shared/src/globals/dom_exception/mod.rs` for
//! the exact legacy code table). One representation now: `Entry::Rtse`
//! (same pattern as `Point`/`Headers`/`TextDecoder`), no more `Entry::Map`
//! tagged with `__rts_class`.
//!
//! `new DOMException(message?, name?)` — both args optional (`ctor(optional =
//! 2)`, arity window `[0,2]`); an omitted `&str` arrives as the empty-string
//! sentinel, which the ctor body treats as "absent" exactly like the deleted
//! hand-written builder did (`name.is_empty()` → default `"Error"`) — the same
//! conflation between "omitted" and "explicit empty string" the prior Rust
//! and `.ts` versions both had (documented gap, not a regression).

use rts_engine::abi::ty::Handle;

/// `message`/`name` are plain Rust `String` fields — `#[rtse::variable]` only
/// covers scalar (f64/i64/i32/bool) fields, so the string-valued getters below
/// are hand-authored `#[rtse::getter]`s (same technique as `Point::tag`).
#[rtse::class("DOMException")]
#[derive(Clone)]
pub struct DomException {
    message: String,
    name: String,
}

#[rtse::class("DOMException")]
impl DomException {
    /// `new DOMException(message = "", name = "Error")`.
    #[rtse::ctor(optional = 2)]
    fn new(message: &str, name: &str) -> Self {
        let name = if name.is_empty() { "Error" } else { name };
        DomException {
            message: message.to_string(),
            name: name.to_string(),
        }
    }

    #[rtse::getter]
    fn message(self: &DomException) -> String {
        self.message.clone()
    }

    #[rtse::getter]
    fn name(self: &DomException) -> String {
        self.name.clone()
    }

    /// The LEGACY numeric code for the spec-listed names; `0` for everything
    /// else (`AbortError` included — it has no legacy code). Ported verbatim
    /// from the deleted hand-written `code_for_name` (git history) / the `.ts`
    /// `get code()` — same table, same order.
    #[rtse::getter]
    fn code(self: &DomException) -> i64 {
        match self.name.as_str() {
            "IndexSizeError" => 1,
            "HierarchyRequestError" => 3,
            "WrongDocumentError" => 4,
            "InvalidCharacterError" => 5,
            "NoModificationAllowedError" => 7,
            "NotFoundError" => 8,
            "NotSupportedError" => 9,
            "InUseAttributeError" => 10,
            "InvalidStateError" => 11,
            "SyntaxError" => 12,
            "InvalidModificationError" => 13,
            "NamespaceError" => 14,
            "InvalidAccessError" => 15,
            "TypeMismatchError" => 17,
            "SecurityError" => 18,
            "NetworkError" => 19,
            "AbortError" => 20,
            "URLMismatchError" => 21,
            "QuotaExceededError" => 22,
            "TimeoutError" => 23,
            "InvalidNodeTypeError" => 24,
            "DataCloneError" => 25,
            _ => 0,
        }
    }

    #[rtse::method(name = "toString")]
    fn to_string(self: &DomException) -> String {
        format!("{}: {}", self.name, self.message)
    }
}

/// Constructs a real `DOMException(message, name)` instance and returns its
/// `Handle` — a stable extern any other crate that links against
/// `rts-shared` (e.g. `rts-std`'s `abort` module) can call directly, since
/// this class is a normal compiled Rust symbol (`__RTS_FN_GL_DOMEXCEPTION_NEW`),
/// not an ambient per-program `.ts` class. See `rts-std/src/globals/abort/mod.rs`'s
/// `default_abort_reason` for the caller.
pub fn new_dom_exception(message: &str, name: &str) -> Handle {
    __RTS_FN_GL_DOMEXCEPTION_NEW(
        message.as_ptr() as i64,
        message.len() as i64,
        name.as_ptr() as i64,
        name.len() as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_engine::heap::handles::alloc_rtse;

    fn h(message: &str, name: &str) -> u64 {
        alloc_rtse(
            "DOMException",
            DomException {
                message: message.to_string(),
                name: name.to_string(),
            },
        )
    }

    fn string_of(handle: u64) -> String {
        rts_engine::heap::handles::with_entry(handle, |e| match e {
            Some(rts_engine::heap::handles::Entry::String(b)) => {
                String::from_utf8_lossy(b).into_owned()
            }
            _ => String::new(),
        })
    }

    #[test]
    fn code_maps_known_names() {
        let e = h("boom", "AbortError");
        assert_eq!(__RTS_FN_GL_DOMEXCEPTION_CODE(e), 20);
        let e = h("boom", "SyntaxError");
        assert_eq!(__RTS_FN_GL_DOMEXCEPTION_CODE(e), 12);
    }

    #[test]
    fn code_defaults_to_zero() {
        let e = h("boom", "SomethingElse");
        assert_eq!(__RTS_FN_GL_DOMEXCEPTION_CODE(e), 0);
    }

    #[test]
    fn ctor_defaults_name_to_error_and_to_string() {
        let handle = new_dom_exception("oops", "");
        assert_eq!(string_of(__RTS_FN_GL_DOMEXCEPTION_NAME(handle)), "Error");
        assert_eq!(string_of(__RTS_FN_GL_DOMEXCEPTION_MESSAGE(handle)), "oops");
        assert_eq!(
            string_of(__RTS_FN_GL_DOMEXCEPTION_TO_STRING(handle)),
            "Error: oops"
        );
    }
}
