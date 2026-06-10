//! `Blob` e `File` global classes (#74/#75).
//!
//! Blob concatena partes num buffer imutavel; File estende Blob com
//! `name`/`lastModified`. Migrado ao modelo `#[rts_class]` (stage 5) — duas
//! classes no mesmo arquivo. `File.size`/`File.text` reusam os externs do Blob
//! (membros `external` apontando p/ `__RTS_FN_GL_BLOB_SIZE`/`_TEXT`).

use indexmap::IndexMap;

use rts_engine::abi::ty::{Handle, I64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

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

/// Blob — concatena partes num buffer imutavel.
#[rts_class(Blob)]
impl BlobClass {
    /// new Blob()
    #[rts_ctor(symbol = "__RTS_FN_GL_BLOB_NEW_EMPTY", ts = "new Blob()", pure)]
    pub fn new_empty() -> Handle {
        make_blob(Vec::new(), None, 0, "Blob")
    }

    /// new Blob(parts)
    #[rts_ctor(ts = "new Blob(parts: BlobPart[])", pure)]
    pub fn new(parts: Handle) -> Handle {
        make_blob(concat_parts(parts), None, 0, "Blob")
    }

    /// blob.size
    #[rts_getter(ts = "readonly size: number", pure)]
    pub fn size(h: Handle) -> I64 {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("size").copied().unwrap_or(0),
            _ => 0,
        })
    }

    /// blob.text() — string UTF-8 num Promise resolvido.
    #[rts_method(ts = "text(): Promise<string>", pure)]
    pub fn text(h: Handle) -> Handle {
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
        let slot = crate::namespaces::gc::promise_slot::new_fulfilled(str_h as i64);
        alloc_entry(Entry::PromiseAsync(slot))
    }

    /// blob.stream() — ReadableStream com UM chunk ja' enfileirado e fechado.
    #[rts_method(ts = "stream(): ReadableStream", pure)]
    pub fn stream(h: Handle) -> Handle {
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
}

/// File — Blob com `name` + `lastModified`.
#[rts_class(File)]
impl FileClass {
    /// new File(parts, name, options?)
    #[rts_ctor(
        ts = "new File(parts: BlobPart[], name: string, options?: FilePropertyBag)",
        opt_str,
        pure
    )]
    pub fn new(parts: Handle, name: Str, opts: Handle) -> Handle {
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
    #[rts_getter(ts = "readonly name: string", pure)]
    pub fn name(h: Handle) -> Handle {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("name").copied().unwrap_or(0) as u64,
            _ => 0,
        })
    }

    /// file.lastModified
    #[rts_getter(name = "lastModified", ts = "readonly lastModified: number", pure)]
    pub fn last_modified(h: Handle) -> I64 {
        with_entry(h, |e| match e {
            Some(Entry::Map(m)) => m.get("lastModified").copied().unwrap_or(0),
            _ => 0,
        })
    }

    /// file.size — reusa o extern do Blob.
    #[rts_getter(
        external,
        symbol = "__RTS_FN_GL_BLOB_SIZE",
        ts = "readonly size: number",
        pure
    )]
    pub fn size(_h: Handle) -> I64 {
        unreachable!()
    }

    /// file.text() — reusa o extern do Blob.
    #[rts_method(
        external,
        symbol = "__RTS_FN_GL_BLOB_TEXT",
        ts = "text(): Promise<string>",
        pure
    )]
    pub fn text(_h: Handle) -> Handle {
        unreachable!()
    }
}
