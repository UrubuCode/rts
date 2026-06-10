//! `DOMException` global class (#77).
//!
//! new DOMException(message, name) com fields message, name, code (legacy
//! numeric code mapeado pelo name padrao). Migrado ao modelo `#[rts_class]`
//! (stage 5); símbolos/spec usam o prefixo `DOM_EXCEPTION`.

use indexmap::IndexMap;

use rts_engine::abi::ty::{Handle, I64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

/// Legacy numeric code para nomes padrao do WebIDL.
fn code_for_name(name: &str) -> i64 {
    match name {
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

/// Constroi o objeto DOMException (Map com message/name/code/__rts_class).
fn build(msg: &str, name: &str) -> u64 {
    let name_final = if name.is_empty() { "Error" } else { name };
    let msg_h = alloc_entry(Entry::String(msg.as_bytes().to_vec())) as i64;
    let name_h = alloc_entry(Entry::String(name_final.as_bytes().to_vec())) as i64;
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("message".to_string(), msg_h);
    m.insert("name".to_string(), name_h);
    m.insert("code".to_string(), code_for_name(name_final));
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"DOMException".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

/// DOMException — WebIDL legacy exception class com name/message/code.
#[rts_class(
    DOMException,
    prefix = "DOM_EXCEPTION",
    spec = "DOM_EXCEPTION_CLASS_SPEC"
)]
impl DomExceptionClass {
    /// new DOMException()
    #[rts_ctor(ts = "new DOMException()", pure)]
    pub fn new_empty() -> Handle {
        build("", "")
    }

    /// new DOMException(message)
    #[rts_ctor(ts = "new DOMException(message: string)", opt_str, pure)]
    pub fn new_msg(message: Str) -> Handle {
        build(message.unwrap_or(""), "")
    }

    /// new DOMException(message, name)
    #[rts_ctor(ts = "new DOMException(message: string, name: string)", opt_str, pure)]
    pub fn new(message: Str, name: Str) -> Handle {
        build(message.unwrap_or(""), name.unwrap_or(""))
    }

    /// exception.name
    #[rts_getter(ts = "readonly name: string", pure)]
    pub fn name(h: Handle) -> Handle {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("name").copied().unwrap_or(0) as u64,
            _ => 0,
        })
    }

    /// exception.message
    #[rts_getter(ts = "readonly message: string", pure)]
    pub fn message(h: Handle) -> Handle {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("message").copied().unwrap_or(0) as u64,
            _ => 0,
        })
    }

    /// exception.code — legacy WebIDL numeric code.
    #[rts_getter(ts = "readonly code: number", pure)]
    pub fn code(h: Handle) -> I64 {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("code").copied().unwrap_or(0),
            _ => 0,
        })
    }
}
