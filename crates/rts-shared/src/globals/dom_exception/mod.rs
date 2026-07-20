//! `DOMException` global class (#77).
//!
//! new DOMException(message, name) com fields message, name, code (legacy
//! numeric code mapeado pelo name padrao). Migrado do `#[rts_class]` (macro)
//! pro modelo builder hand-written do `rts-engine` (rumo à remoção da
//! `rts-macro`). Os externs `__RTS_FN_GL_DOM_EXCEPTION_*` +
//! `register_dom_exception_class_spec()` são escritos à mão. Símbolos/spec
//! usam o prefixo `DOM_EXCEPTION`.

use indexmap::IndexMap;

use rts_engine::abi::ty::{Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

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

/// new DOMException()
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NEW_EMPTY() -> Handle {
    build("", "")
}

/// new DOMException(message)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NEW_MSG(
    message_ptr: *const u8,
    message_len: i64,
) -> Handle {
    let message = unsafe { rts_engine::abi::str_abi::from_abi(message_ptr, message_len) };
    build(message.unwrap_or(""), "")
}

/// new DOMException(message, name)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NEW(
    message_ptr: *const u8,
    message_len: i64,
    name_ptr: *const u8,
    name_len: i64,
) -> Handle {
    let message = unsafe { rts_engine::abi::str_abi::from_abi(message_ptr, message_len) };
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    build(message.unwrap_or(""), name.unwrap_or(""))
}

/// exception.name
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_NAME(h: Handle) -> Handle {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("name").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// exception.message
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_MESSAGE(h: Handle) -> Handle {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("message").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// exception.code — legacy WebIDL numeric code.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DOM_EXCEPTION_CODE(h: Handle) -> I64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("code").copied().unwrap_or(0),
        _ => 0,
    })
}

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
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
        intrinsic: None,
    }
}

/// Registra a classe global `DOMException` no motor (hand-written, sem macro).
pub fn register_dom_exception_class_spec(e: &mut Engine) {
    e.class("DOMException")
        .doc("DOMException — WebIDL legacy exception class com name/message/code.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_DOM_EXCEPTION_NEW_EMPTY",
            "new DOMException()",
            "new DOMException()",
            __RTS_FN_GL_DOM_EXCEPTION_NEW_EMPTY as *const u8,
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_DOM_EXCEPTION_NEW_MSG",
            "new DOMException(message: string)",
            "new DOMException(message)",
            __RTS_FN_GL_DOM_EXCEPTION_NEW_MSG as *const u8,
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_DOM_EXCEPTION_NEW",
            "new DOMException(message: string, name: string)",
            "new DOMException(message, name)",
            __RTS_FN_GL_DOM_EXCEPTION_NEW as *const u8,
            true,
        ))
        .member(m(
            "name",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DOM_EXCEPTION_NAME",
            "readonly name: string",
            "exception.name",
            __RTS_FN_GL_DOM_EXCEPTION_NAME as *const u8,
            true,
        ))
        .member(m(
            "message",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DOM_EXCEPTION_MESSAGE",
            "readonly message: string",
            "exception.message",
            __RTS_FN_GL_DOM_EXCEPTION_MESSAGE as *const u8,
            true,
        ))
        .member(m(
            "code",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DOM_EXCEPTION_CODE",
            "readonly code: number",
            "exception.code — legacy WebIDL numeric code.",
            __RTS_FN_GL_DOM_EXCEPTION_CODE as *const u8,
            true,
        ))
        .done();
}
