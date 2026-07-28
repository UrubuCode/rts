//! Minimal snapshotting of an `Entry` — copies just enough to format outside
//! the `HandleTable` lock, avoiding recursive deadlock. Shared by the
//! coercion/formatting ABI in `alloc.rs`/`coerce.rs`, and re-exported `pub`
//! for the `rts-std` siblings (`__RTS_FN_RT_SPREAD_INTO_VEC`,
//! `__RTS_FN_RT_OBJECT_TO_STRING`, `__RTS_FN_RT_INSPECT`) that still need it
//! but couldn't move down (they touch `rts-shared`/`rts-primitives`).

use crate::heap::handles::{Entry, with_entry};
use crate::numfmt::format_js_number;

/// Helper: textual name of an Entry's kind (for the `[object Kind]` fallback).
pub fn entry_kind_name(e: &Entry) -> &'static str {
    match e {
        Entry::String(_) => "String",
        Entry::Buffer(_) => "Buffer",
        Entry::Vec(_) => "Array",
        Entry::Map(_) => "Object",
        Entry::Json(_) => "Json",
        Entry::BigFixed(_) => "BigFixed",
        Entry::ProcessChild(_) => "ProcessChild",
        Entry::TcpListener(_) => "TcpListener",
        Entry::TcpStream(_) => "TcpStream",
        Entry::UdpSocket(_) => "UdpSocket",
        Entry::Function { .. } => "Function",
        _ => "Object",
    }
}

pub enum EntrySnap {
    Str(Vec<u8>),
    Vec(Vec<i64>),
    Map,
    Buffer,
    Json(String),
    // (narrow-storage) boxed primitive float -> coerce as a number.
    Float(f64),
    Other(&'static str),
    None,
}

pub fn snapshot_entry(h: u64) -> EntrySnap {
    with_entry(h, |e| match e {
        Some(Entry::String(s)) => EntrySnap::Str(s.clone()),
        Some(Entry::Vec(v)) => EntrySnap::Vec((**v).clone()),
        Some(Entry::Map(_)) => EntrySnap::Map,
        Some(Entry::Buffer(_)) => EntrySnap::Buffer,
        Some(Entry::Json(j)) => EntrySnap::Json(j.to_string()),
        Some(Entry::FloatPrim(f)) => EntrySnap::Float(*f),
        Some(other) => EntrySnap::Other(entry_kind_name(other)),
        None => EntrySnap::None,
    })
}

pub fn snapshot_to_bytes(s: &EntrySnap) -> Vec<u8> {
    match s {
        EntrySnap::Str(b) => b.clone(),
        EntrySnap::Vec(slots) => {
            let parts: Vec<String> = slots.iter().map(|x| element_to_string(*x)).collect();
            parts.join(",").into_bytes()
        }
        EntrySnap::Float(f) => format_js_number(*f).into_bytes(),
        EntrySnap::Map => b"[object Object]".to_vec(),
        EntrySnap::Buffer => b"[object Buffer]".to_vec(),
        EntrySnap::Json(j) => j.clone().into_bytes(),
        EntrySnap::Other(name) => format!("[object {}]", name).into_bytes(),
        EntrySnap::None => Vec::new(),
    }
}

/// Converts a raw i64 (a Vec slot) into a string. Tries a HandleTable lookup:
/// if it is a valid String/Vec/Map/etc. handle, renders the Entry
/// recursively. Otherwise formats it as an integer (the default semantics for
/// `Vec<i64>` in RTS — slots are raw i64, not f64 bits).
///
/// **Do not call from inside a `with_entry`/`with_two_entries` lock** — this
/// fn re-accesses the HandleTable and would deadlock on the same shard.
pub fn element_to_string(raw: i64) -> String {
    // Bool sentinel (same logic as inspect_slot).
    if raw == i64::MIN {
        return "false".to_string();
    }
    if raw == i64::MIN + 1 {
        return "true".to_string();
    }
    // (cross-runtime #51) undefined/null/sparse-hole in
    // Array.prototype.toString become "" (JS spec). Without this, the raw
    // i64::MIN+2/+3/+4 would fall into format_js_number and produce garbage
    // like "-9223372036854776000".
    if raw == i64::MIN + 2 || raw == i64::MIN + 3 || raw == i64::MIN + 4 {
        return String::new();
    }
    let h = raw as u64;
    // Heuristic: RTS handles start at gen >= 1, giving values >= 2^48.
    // Smaller values are almost certainly literal integers (TS [1,2,3]).
    // This optimization avoids an unnecessary HandleTable lookup for small
    // numeric arrays (the common case).
    if h < (1u64 << 48) {
        return format_js_number(raw as f64);
    }
    enum Resolved {
        StringBytes(Vec<u8>),
        VecSlots(Vec<i64>),
        Object,
        Json(String),
        Other(&'static str),
        NotHandle,
    }
    let resolved = with_entry(h, |e| match e {
        Some(Entry::String(s)) => Resolved::StringBytes(s.clone()),
        Some(Entry::Vec(v)) => Resolved::VecSlots((**v).clone()),
        Some(Entry::Map(_)) => Resolved::Object,
        Some(Entry::Json(j)) => Resolved::Json(j.to_string()),
        Some(other) => Resolved::Other(entry_kind_name(other)),
        None => Resolved::NotHandle,
    });
    match resolved {
        Resolved::StringBytes(b) => String::from_utf8_lossy(&b).into_owned(),
        Resolved::VecSlots(slots) => {
            let inner: Vec<String> = slots.iter().map(|x| element_to_string(*x)).collect();
            inner.join(",")
        }
        Resolved::Object => "[object Object]".to_string(),
        Resolved::Json(s) => s,
        Resolved::Other(name) => format!("[object {}]", name),
        // Unrecognized handle — format as an integer.
        Resolved::NotHandle => format_js_number(raw as f64),
    }
}
