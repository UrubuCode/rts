//! `Blob` e `File` global classes (#74/#75).
//!
//! Blob concatena partes num buffer imutavel; File estende Blob com
//! `name`/`lastModified`. Migrado do `#[rts_class]` (macro) pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_BLOB_*`/`__RTS_FN_GL_FILE_*` + `register_blob_class_spec()` /
//! `register_file_class_spec()` são escritos à mão. `File.size`/`File.text`
//! reusam os externs do Blob (membros `external` apontando p/
//! `__RTS_FN_GL_BLOB_SIZE`/`_TEXT`).

use indexmap::IndexMap;

use rts_engine::abi::ty::{Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};

/// Concatena os bytes de cada parte (Vec de handles de elemento).
fn concat_parts(parts_h: u64) -> Vec<u8> {
    if parts_h == 0 {
        return Vec::new();
    }
    let elems: Vec<i64> = with_entry(parts_h, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let mut out = Vec::new();
    for el in elems {
        let h = el as u64;
        let bytes = with_entry(h, |e| match e {
            Some(Entry::String(b)) => b.clone(),
            Some(Entry::Buffer(b)) => b.clone(),
            Some(Entry::Vec(v)) => v.iter().map(|&x| x as u8).collect(),
            Some(Entry::Map(m)) => {
                let bytes_h = m.get("bytes").copied().unwrap_or(0) as u64;
                with_entry(bytes_h, |be| match be {
                    Some(Entry::Buffer(b)) => b.clone(),
                    _ => Vec::new(),
                })
            }
            _ => Vec::new(),
        });
        out.extend(bytes);
    }
    out
}

fn make_blob(bytes: Vec<u8>, name: Option<String>, last_modified: i64, class: &str) -> u64 {
    let size = bytes.len() as i64;
    let bytes_h = alloc_entry(Entry::Buffer(bytes)) as i64;
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("bytes".to_string(), bytes_h);
    m.insert("size".to_string(), size);
    if let Some(n) = name {
        let name_h = alloc_entry(Entry::String(n.into_bytes())) as i64;
        m.insert("name".to_string(), name_h);
        m.insert("lastModified".to_string(), last_modified);
    }
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(class.as_bytes().to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

// ── Externs de membros `Blob` (`extern "C"` hand-written, antes via macro) ──────

/// new Blob()
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_NEW_EMPTY() -> Handle {
    make_blob(Vec::new(), None, 0, "Blob")
}

/// new Blob(parts)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_NEW(parts: Handle) -> Handle {
    make_blob(concat_parts(parts), None, 0, "Blob")
}

/// blob.size
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_SIZE(h: Handle) -> I64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("size").copied().unwrap_or(0),
        _ => 0,
    })
}

/// blob.text() — string UTF-8 num Promise resolvido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_TEXT(h: Handle) -> Handle {
    let bytes = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => {
            let bytes_h = m.get("bytes").copied().unwrap_or(0) as u64;
            with_entry(bytes_h, |be| match be {
                Some(Entry::Buffer(b)) => b.clone(),
                _ => Vec::new(),
            })
        }
        _ => Vec::new(),
    });
    let s = String::from_utf8_lossy(&bytes).into_owned();
    let str_h = alloc_entry(Entry::String(s.into_bytes()));
    let slot = crate::promise_slot::new_fulfilled(str_h as i64);
    alloc_entry(Entry::PromiseAsync(slot))
}

/// blob.stream() — ReadableStream com UM chunk ja' enfileirado e fechado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_STREAM(h: Handle) -> Handle {
    let bytes = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => {
            let bytes_h = m.get("bytes").copied().unwrap_or(0) as u64;
            with_entry(bytes_h, |be| match be {
                Some(Entry::Buffer(b)) => b.clone(),
                _ => Vec::new(),
            })
        }
        _ => Vec::new(),
    });
    let chunk: Vec<i64> = bytes.iter().map(|&b| b as i64).collect();
    let chunk_h = alloc_entry(Entry::Vec(Box::new(chunk))) as i64;
    let buf_h = alloc_entry(Entry::Vec(Box::new(vec![chunk_h]))) as i64;
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("__buf".to_string(), buf_h);
    m.insert("__closed".to_string(), 1);
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(b"ReadableStream".to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

// ── Externs de membros `File` (`extern "C"` hand-written, antes via macro) ──────

/// new File(parts, name, options?)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_NEW(
    parts: Handle,
    name_ptr: *const u8,
    name_len: i64,
    opts: Handle,
) -> Handle {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let name = name.unwrap_or("").to_string();
    let last_modified = if opts == 0 {
        0
    } else {
        with_entry(opts, |e| match e {
            Some(Entry::Map(m)) => m.get("lastModified").copied().unwrap_or(0),
            _ => 0,
        })
    };
    make_blob(concat_parts(parts), Some(name), last_modified, "File")
}

/// file.name
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_NAME(h: Handle) -> Handle {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("name").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// file.lastModified
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_LAST_MODIFIED(h: Handle) -> I64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("lastModified").copied().unwrap_or(0),
        _ => 0,
    })
}

// ── Builders hand-written das classes globais `Blob` e `File` ──────────────────

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
        emit: None,
    }
}

/// Registra a classe global `Blob` no motor (hand-written, sem macro).
pub fn register_blob_class_spec(e: &mut Engine) {
    e.class("Blob")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_BLOB_NEW_EMPTY",
            "new Blob()",
            "new Blob()",
            __RTS_FN_GL_BLOB_NEW_EMPTY as *const u8,
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_BLOB_NEW",
            "new Blob(parts: BlobPart[])",
            "new Blob(parts)",
            __RTS_FN_GL_BLOB_NEW as *const u8,
            true,
        ))
        .member(m(
            "size",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_BLOB_SIZE",
            "readonly size: number",
            "blob.size",
            __RTS_FN_GL_BLOB_SIZE as *const u8,
            true,
        ))
        .member(m(
            "text",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_BLOB_TEXT",
            "text(): Promise<string>",
            "blob.text() — string UTF-8 num Promise resolvido.",
            __RTS_FN_GL_BLOB_TEXT as *const u8,
            true,
        ))
        .member(m(
            "stream",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_BLOB_STREAM",
            "stream(): ReadableStream",
            "blob.stream() — ReadableStream com UM chunk ja' enfileirado e fechado.",
            __RTS_FN_GL_BLOB_STREAM as *const u8,
            true,
        ))
        .done();
}

/// Registra a classe global `File` no motor (hand-written, sem macro).
pub fn register_file_class_spec(e: &mut Engine) {
    e.class("File")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::Handle],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_FILE_NEW",
            "new File(parts: BlobPart[], name: string, options?: FilePropertyBag)",
            "new File(parts, name, options?)",
            __RTS_FN_GL_FILE_NEW as *const u8,
            true,
        ))
        .member(m(
            "name",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FILE_NAME",
            "readonly name: string",
            "file.name",
            __RTS_FN_GL_FILE_NAME as *const u8,
            true,
        ))
        .member(m(
            "lastModified",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_FILE_LAST_MODIFIED",
            "readonly lastModified: number",
            "file.lastModified",
            __RTS_FN_GL_FILE_LAST_MODIFIED as *const u8,
            true,
        ))
        .member(m(
            "size",
            MemberKind::InstanceGetter,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_BLOB_SIZE",
            "readonly size: number",
            "file.size — reusa o extern do Blob.",
            __RTS_FN_GL_BLOB_SIZE as *const u8,
            true,
        ))
        .member(m(
            "text",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_BLOB_TEXT",
            "text(): Promise<string>",
            "file.text() — reusa o extern do Blob.",
            __RTS_FN_GL_BLOB_TEXT as *const u8,
            true,
        ))
        .done();
}
