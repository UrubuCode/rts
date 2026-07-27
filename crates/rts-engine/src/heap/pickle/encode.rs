//! `pickle` encode — the recursive PolyValue-graph walk with a memo table.
//!
//! Discipline (same as `structured_clone.ts` and Python's pickle): a heap value
//! gets its memo id ON FIRST VISIT, BEFORE its children are walked — so a cycle
//! back to it emits `OP_REF` instead of recursing forever, and shared children
//! serialize once. Entry data is SNAPSHOTTED (cloned) under the shard lock and
//! the recursion happens outside it — a nested `with_entry` on a same-shard
//! child would deadlock.

use std::collections::HashMap;

use super::super::handles::{with_entry, ArrayBufferBacking, Entry};
use super::super::poly::{
    poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_SING_FALSE, POLY_SING_HOLE,
    POLY_SING_NULL, POLY_SING_TRUE, POLY_TAG_FUNCTION, POLY_TAG_INT32, POLY_TAG_MASK,
    POLY_TAG_OBJECT, POLY_TAG_SHIFT, POLY_TAG_SINGLETON, POLY_TAG_STR,
};
use super::super::shapes::{class_name_of_shape, global_shape_keys, handle_word_auto, legacy_i64_to_word};
use super::*;

/// Serialize one PolyValue word into a fresh `RTSP` v1 byte stream.
pub fn serialize_value(word: u64) -> Result<Vec<u8>, String> {
    let mut enc = Enc {
        out: Vec::with_capacity(64),
        memo: HashMap::new(),
    };
    enc.out.extend_from_slice(&MAGIC);
    enc.out.push(VERSION);
    enc.value(word, 0)?;
    Ok(enc.out)
}

struct Enc {
    out: Vec<u8>,
    /// Raw handle → memo id, assigned on first visit (before children).
    memo: HashMap<u64, u32>,
}

impl Enc {
    fn value(&mut self, word: u64, depth: u32) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("pickle: nesting too deep".into());
        }
        // A genuine inline double (not in the boxed NaN quadrant) — unless it
        // is a HANDLE-AS-NUMBER: several front paths (e.g. `new Error(...)`,
        // whose ABI return is `Handle`) store a raw handle into a container as
        // a plain f64 integer. Same convention `element_to_handle` decodes: a
        // finite whole f64 in the handle range (≥ 2^48, gen ≥ 1) whose slot is
        // LIVE is a heap reference, not user data (a real user double of that
        // exact live bit-meaning is not expressible — gen 0 is never live).
        if (word & POLY_BOX_BASE) != POLY_BOX_BASE {
            if let Some(h) = f64_encoded_handle(word) {
                return self.heap_handle(h, depth);
            }
            self.out.push(OP_F64);
            self.out.extend_from_slice(&word.to_le_bytes());
            return Ok(());
        }
        let tag = (word >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
        match tag {
            POLY_TAG_INT32 => {
                let v = (word & POLY_PAYLOAD_MASK) as u32 as i32;
                self.out.push(OP_I32);
                put_varint(&mut self.out, zigzag(v as i64));
            }
            POLY_TAG_SINGLETON => {
                self.out.push(match word & POLY_PAYLOAD_MASK {
                    POLY_SING_NULL => OP_NULL,
                    POLY_SING_FALSE => OP_FALSE,
                    POLY_SING_TRUE => OP_TRUE,
                    POLY_SING_HOLE => OP_HOLE,
                    // undefined / empty (and anything future) → undefined.
                    _ => OP_UNDEF,
                });
            }
            POLY_TAG_STR | POLY_TAG_OBJECT | POLY_TAG_FUNCTION => self.heap(word, depth)?,
            other => return Err(format!("pickle: unserializable value tag {other}")),
        }
        Ok(())
    }

    fn heap(&mut self, word: u64, depth: u32) -> Result<(), String> {
        let h = poly_handle_normalize(word).ok_or("pickle: unboxable heap word")?;
        if h == 0 {
            // The boxed NULL-handle sentinel (payload all-ones — what
            // `POLY_FROM_HANDLE(0)` boxes, e.g. an unset `stack` field):
            // reads back as absent → undefined.
            self.out.push(OP_UNDEF);
            return Ok(());
        }
        self.heap_handle(h, depth)
    }

    fn heap_handle(&mut self, h: u64, depth: u32) -> Result<(), String> {
        if let Some(&id) = self.memo.get(&h) {
            self.out.push(OP_REF);
            put_varint(&mut self.out, id as u64);
            return Ok(());
        }
        let id = self.memo.len() as u32;
        self.memo.insert(h, id);
        match snapshot(h).map_err(|e| format!("{e} (h={h:#x})"))? {
            Snap::Str(bytes) => {
                self.out.push(OP_STR);
                put_str(&mut self.out, &bytes);
            }
            Snap::Array(slots) => {
                self.out.push(OP_ARRAY);
                put_varint(&mut self.out, slots.len() as u64);
                for w in slots {
                    self.value(w as u64, depth + 1)?;
                }
            }
            Snap::Object { keys, values, normalize } => {
                self.out.push(OP_OBJECT);
                put_varint(&mut self.out, keys.len() as u64);
                for k in &keys {
                    put_str(&mut self.out, k.as_bytes());
                }
                for v in values {
                    // `Entry::Map` slots are the documented raw-mixed i64
                    // surface (word OR live raw handle OR sentinel) —
                    // normalize outside the shard lock. Shaped-object slots
                    // are already words.
                    let w = if normalize { legacy_i64_to_word(v) } else { v as u64 };
                    self.value(w, depth + 1)?;
                }
            }
            Snap::ClassInst { class, keys, values } => {
                self.out.push(OP_CLASS);
                put_str(&mut self.out, class.as_bytes());
                put_varint(&mut self.out, keys.len() as u64);
                for k in &keys {
                    put_str(&mut self.out, k.as_bytes());
                }
                for v in values {
                    self.value(v as u64, depth + 1)?;
                }
            }
            Snap::Buffer(bytes) => {
                self.out.push(OP_BUFFER);
                put_str(&mut self.out, &bytes);
            }
            Snap::ArrayBuf(bytes) => {
                self.out.push(OP_ARRAYBUF);
                put_str(&mut self.out, &bytes);
            }
            Snap::BigInt { negative, words } => {
                self.out.push(OP_BIGINT);
                self.out.push(negative as u8);
                put_varint(&mut self.out, words.len() as u64);
                for w in words {
                    self.out.extend_from_slice(&w.to_le_bytes());
                }
            }
            Snap::Error { name, message, cause } => {
                self.out.push(OP_ERROR);
                put_str(&mut self.out, name.as_bytes());
                put_str(&mut self.out, message.as_bytes());
                if cause == 0 {
                    self.out.push(0);
                } else {
                    self.out.push(1);
                    self.value(handle_word_auto(cause), depth + 1)?;
                }
            }
            Snap::BoolBox(b) => {
                self.out.push(OP_BOOLBOX);
                self.out.push(b as u8);
            }
            Snap::NumBox(f) => {
                self.out.push(OP_NUMBOX);
                self.out.extend_from_slice(&f.to_le_bytes());
            }
            Snap::StrBox(inner) => {
                self.out.push(OP_STRBOX);
                self.value(handle_word_auto(inner), depth + 1)?;
            }
            Snap::FloatPrim(f) => {
                self.out.push(OP_FLOATPRIM);
                self.out.extend_from_slice(&f.to_le_bytes());
            }
            Snap::Json(bytes) => {
                self.out.push(OP_JSON);
                put_str(&mut self.out, &bytes);
            }
            Snap::FnRef(name) => {
                self.out.push(OP_FN_REF);
                put_str(&mut self.out, name.as_bytes());
            }
            Snap::Ext(tag, payload) => {
                self.out.push(OP_EXT);
                put_str(&mut self.out, tag.as_bytes());
                put_str(&mut self.out, &payload);
            }
        }
        Ok(())
    }
}

/// The raw handle a plain-f64 word encodes as an integer, if its slot is LIVE.
/// Handle range starts at 2^48 (generation ≥ 1); whole f64s below 2^53 encode
/// handles exactly. Anything else is genuine user data.
fn f64_encoded_handle(word: u64) -> Option<u64> {
    let f = f64::from_bits(word);
    if !f.is_finite() || f.fract() != 0.0 || f < (1u64 << 48) as f64 || f >= (1u64 << 53) as f64 {
        return None;
    }
    let h = f as u64;
    with_entry(h, |e| e.is_some()).then_some(h)
}

/// Owned snapshot of one entry's serializable data, cloned under the shard
/// lock so the recursion runs outside it.
enum Snap {
    Str(Vec<u8>),
    Array(Vec<i64>),
    Object { keys: Vec<String>, values: Vec<i64>, normalize: bool },
    ClassInst { class: String, keys: Vec<String>, values: Vec<i64> },
    Buffer(Vec<u8>),
    ArrayBuf(Vec<u8>),
    BigInt { negative: bool, words: Vec<u64> },
    Error { name: String, message: String, cause: u64 },
    BoolBox(bool),
    NumBox(f64),
    StrBox(u64),
    FloatPrim(f64),
    Json(Vec<u8>),
    FnRef(String),
    Ext(&'static str, Vec<u8>),
}

fn snapshot(h: u64) -> Result<Snap, String> {
    with_entry(h, |e| {
        let entry = e.ok_or("pickle: freed handle entry")?;
        match entry {
            Entry::String(s) => Ok(Snap::Str(s.clone())),
            Entry::Vec(slots) => snap_vec(slots),
            Entry::Map(m) => {
                if m.contains_key("__rts_class") {
                    return Err("pickle: cannot serialize a class instance (unsupported in v1)".into());
                }
                Ok(Snap::Object {
                    keys: m.keys().cloned().collect(),
                    values: m.values().copied().collect(),
                    normalize: true,
                })
            }
            Entry::Buffer(b) => Ok(Snap::Buffer(b.clone())),
            Entry::ArrayBuffer(ab) => match (&ab.backing, ab.detached) {
                (ArrayBufferBacking::Owned(b), false) => Ok(Snap::ArrayBuf(b.to_vec())),
                _ => Err("pickle: cannot serialize a detached or externally-backed ArrayBuffer".into()),
            },
            Entry::BigInt(b) => Ok(Snap::BigInt {
                negative: b.negative,
                words: b.words.clone(),
            }),
            Entry::ErrorObj { message, name, cause } => Ok(Snap::Error {
                name: name.clone(),
                message: message.clone(),
                cause: *cause,
            }),
            Entry::BooleanBox(b) => Ok(Snap::BoolBox(*b)),
            Entry::NumberBox(f) => Ok(Snap::NumBox(*f)),
            Entry::StringBox(inner) => Ok(Snap::StrBox(*inner)),
            Entry::FloatPrim(f) => Ok(Snap::FloatPrim(*f)),
            Entry::Json(v) => {
                serde_json::to_vec(v.as_ref()).map(Snap::Json).map_err(|e| format!("pickle: JSON snapshot failed: {e}"))
            }
            // Function BY REFERENCE — Python's rule: only a top-level named fn
            // (resolvable in the program fn registry) serializes; a closure /
            // arrow / bound fn is state the reference cannot carry.
            Entry::Function(f) => {
                if f.has_bound_this || !f.bound_args.is_empty() {
                    return Err("pickle: cannot serialize a bound function or closure".into());
                }
                fn_name_of_ptr(f.fn_ptr).map(Snap::FnRef).ok_or_else(|| {
                    format!(
                        "pickle: cannot serialize function '{}' (only top-level named functions serialize, by reference)",
                        f.name
                    )
                })
            }
            // Everything else: an extension codec (Date, RegExp, … registered
            // by the class's owner crate) or an honest, named error.
            other => ext_encode(other)
                .map(|(tag, payload)| Snap::Ext(tag, payload))
                .ok_or_else(|| format!("pickle: cannot serialize a {}", kind_name(other))),
        }
    })
}

/// Shaped object (slot 0 = boxed REGISTERED shape id) vs plain array. Same
/// discrimination `node:v8` used, done ONCE at encode and written explicitly to
/// the wire — the decoder never re-guesses.
fn snap_vec(slots: &[i64]) -> Result<Snap, String> {
    if let Some(&w0) = slots.first() {
        let w0 = w0 as u64;
        if (w0 & POLY_BOX_BASE) == POLY_BOX_BASE
            && (w0 >> POLY_TAG_SHIFT) & POLY_TAG_MASK == POLY_TAG_INT32
        {
            let shape_id = (w0 & POLY_PAYLOAD_MASK) as u32;
            if let Some(keys) = global_shape_keys(shape_id) {
                if keys.len() + 1 == slots.len() {
                    // A shape owned by a `class` declaration → CLASS_INST with
                    // the WHOLE field state, `#`-private fields included (they
                    // are the instance's state — Python pickles __dict__ the
                    // same way). Methods/accessors live on the shared proto,
                    // not in slots, and are re-attached at revive.
                    if let Some(class) = class_name_of_shape(shape_id) {
                        return Ok(Snap::ClassInst {
                            class,
                            keys,
                            values: slots[1..].to_vec(),
                        });
                    }
                    // Not a class shape: an accessor / symbol key marks an
                    // exotic literal (defineProperty getter etc.) — explicit
                    // error, not silently lossy.
                    if let Some(k) = keys.iter().find(|k| {
                        k.starts_with('#')
                            || k.starts_with("__get_")
                            || k.starts_with("__set_")
                            || k.starts_with("@@sym:")
                    }) {
                        return Err(format!(
                            "pickle: cannot serialize an object with key '{k}' (accessor / symbol / private key on a non-class shape)"
                        ));
                    }
                    return Ok(Snap::Object {
                        keys,
                        values: slots[1..].to_vec(),
                        normalize: false,
                    });
                }
            }
        }
    }
    Ok(Snap::Array(slots.to_vec()))
}

/// Human name of an entry family for error messages. EXHAUSTIVE on purpose —
/// a new `Entry` variant fails compilation here instead of silently falling
/// into a catch-all, forcing an explicit serialize-or-reject decision.
fn kind_name(e: &Entry) -> &'static str {
    match e {
        Entry::String(_) => "string",
        Entry::BigFixed(_) => "BigFixed decimal",
        Entry::Buffer(_) => "buffer",
        Entry::ProcessChild(_) => "child process",
        Entry::Map(_) => "object",
        Entry::Vec(_) => "array",
        Entry::Regex(_) => "RegExp",
        Entry::CString(_) => "CString",
        Entry::OsString(_) => "OsString",
        Entry::AtomicI64(_) | Entry::AtomicBool(_) | Entry::AtomicF64(_) => "atomic",
        Entry::SyncMutex(_) | Entry::SyncRwLock(_) | Entry::SyncOnce(_) => "sync primitive",
        Entry::TcpListener(_) | Entry::TcpStream(_) | Entry::UdpSocket(_) | Entry::TlsClient(_) => {
            "socket"
        }
        Entry::JoinHandle(_) => "thread handle",
        Entry::Env(_) => "closure environment",
        Entry::Json(_) => "JSON value",
        Entry::ErrorObj { .. } => "Error",
        Entry::Rtse { .. } => "class instance",
        Entry::RtsEventsEmitter(_) => "EventEmitter",
        Entry::PromiseAsync(_) => "Promise",
        Entry::HttpResponse(_) => "HTTP response",
        Entry::Function(_) => "function",
        Entry::Symbol { .. } => "symbol",
        Entry::Proxy { .. } => "Proxy",
        Entry::Hasher(_) => "streaming hasher",
        Entry::BooleanBox(_) => "Boolean box",
        Entry::StringBox(_) => "String box",
        Entry::NumberBox(_) => "Number box",
        Entry::FloatPrim(_) => "number",
        Entry::Headers(_) => "Headers",
        Entry::GenState(_) => "generator",
        Entry::NapiExternal(_) => "N-API external",
        Entry::BigInt(_) => "BigInt",
        Entry::ArrayBuffer(_) => "ArrayBuffer",
        Entry::Backend(_) => "backend object",
        Entry::Free => "freed handle",
    }
}
