//! DOMException — class minima com name, message, code.

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};
use indexmap::IndexMap;

fn str_from_parts<'a>(ptr: i64, len: i64) -> &'a str {
    if ptr == 0 || len <= 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

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

/// new DOMException(message, name)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NEW(
    msg_ptr: i64,
    msg_len: i64,
    name_ptr: i64,
    name_len: i64,
) -> u64 {
    let msg = str_from_parts(msg_ptr, msg_len);
    let name = str_from_parts(name_ptr, name_len);
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

/// new DOMException() — sem args.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NEW_EMPTY() -> u64 {
    __RTS_FN_GL_DOM_EXCEPTION_NEW(0, 0, 0, 0)
}

/// new DOMException(message) — sem name.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NEW_MSG(msg_ptr: i64, msg_len: i64) -> u64 {
    __RTS_FN_GL_DOM_EXCEPTION_NEW(msg_ptr, msg_len, 0, 0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NAME(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("name").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_MESSAGE(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("message").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_CODE(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("code").copied().unwrap_or(0),
        _ => 0,
    })
}
