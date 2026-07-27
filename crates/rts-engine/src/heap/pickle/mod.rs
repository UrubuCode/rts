//! `pickle` — deep structured serialization of runtime values to bytes (the
//! RTS analogue of Python's pickle / V8's structured serialize). ONE native
//! primitive over the PolyValue → HandleTable graph, consumed by the `serde`
//! namespace surface (rts-shared), `node:v8` serialize/deserialize (rts-node)
//! and — eventually — the ambient `structuredClone`.
//!
//! Wire format `RTSP` v1: `magic "RTSP" | version u8 | value-stream`. The value
//! stream is an opcode stream with a MEMO TABLE of back-references (`OP_REF`),
//! which is what gives cyclic graphs and shared identity (two fields pointing
//! at the same object stay ONE object after a round-trip) — the property JSON
//! structurally cannot express. Varints are protobuf-style LEB128 (unsigned)
//! with zigzag for signed.
//!
//! Non-primordial classes (Date, RegExp, …) plug in through [`ExtCodec`] — the
//! engine never names them (PRIMORDIAL-vs-REGISTRY doctrine); the crate that
//! owns the class registers its codec at namespace-register time.
//!
//! Layout: `encode` (graph walk + memo), `decode` (cursor + memo + GC pinning),
//! `mod` (format constants, varint helpers, ext-codec registry).

mod decode;
mod encode;

pub use decode::deserialize_value;
pub use encode::serialize_value;

use std::sync::{OnceLock, RwLock};

use super::handles::Entry;

/// File magic — "RTSP" (RTS Pickle).
pub(crate) const MAGIC: [u8; 4] = *b"RTSP";
/// Current wire-format version. A decoder accepts only versions ≤ its own.
pub(crate) const VERSION: u8 = 1;

// ── Opcodes ──────────────────────────────────────────────────────────────────
pub(crate) const OP_UNDEF: u8 = 0;
pub(crate) const OP_NULL: u8 = 1;
pub(crate) const OP_FALSE: u8 = 2;
pub(crate) const OP_TRUE: u8 = 3;
/// Array hole (sparse-array elision) — a singleton, preserved through a trip.
pub(crate) const OP_HOLE: u8 = 4;
/// Inline double: 8 bytes LE (NaN canonicalized on decode).
pub(crate) const OP_F64: u8 = 5;
/// Boxed INT32: zigzag varint.
pub(crate) const OP_I32: u8 = 6;
/// String: varint byte-len + UTF-8.
pub(crate) const OP_STR: u8 = 7;
/// Back-reference: varint memo id — cycles + shared identity.
pub(crate) const OP_REF: u8 = 8;
/// Array: varint len + element values.
pub(crate) const OP_ARRAY: u8 = 9;
/// Shaped object: varint key-count + keys block (varint len + UTF-8)* + values.
/// Keys come BEFORE values so the decoder can intern the shape and allocate the
/// placeholder object (memo registration) before descending into children.
pub(crate) const OP_OBJECT: u8 = 10;
/// Raw byte buffer (`Entry::Buffer`): varint len + bytes.
pub(crate) const OP_BUFFER: u8 = 11;
/// ArrayBuffer with OWNED backing: varint len + bytes.
pub(crate) const OP_ARRAYBUF: u8 = 12;
/// BigInt: sign u8 + varint word-count + words (8 bytes LE each).
pub(crate) const OP_BIGINT: u8 = 13;
/// Error: name str + message str + has-cause u8 + [cause value].
pub(crate) const OP_ERROR: u8 = 14;
/// `new Boolean(x)` box: u8.
pub(crate) const OP_BOOLBOX: u8 = 15;
/// `new Number(x)` box: 8 bytes LE.
pub(crate) const OP_NUMBOX: u8 = 16;
/// `new String(x)` box: inner string value follows.
pub(crate) const OP_STRBOX: u8 = 17;
/// Primitive float stored boxed in a heterogeneous container: 8 bytes LE.
/// Decodes to a plain inline-double word (the container slot holds words).
pub(crate) const OP_FLOATPRIM: u8 = 18;
/// Extension (Date, RegExp, …): tag str + varint payload-len + payload bytes.
pub(crate) const OP_EXT: u8 = 19;
/// `Entry::Json` boxed value: varint len + serde_json bytes.
pub(crate) const OP_JSON: u8 = 20;
/// User-class instance: class-name str + varint field-count + keys block +
/// values. Revives with the DESTINATION program's shape id for that class name
/// (so `instanceof` — a baked shape-id compare — holds), fields matched BY KEY
/// (extra stream fields dropped, missing ones stay undefined), then the
/// class-revive hook attaches the class proto. The class must be declared in
/// the destination program — otherwise a TypeError, like Python's pickle with
/// an unimportable class.
pub(crate) const OP_CLASS: u8 = 21;
/// Function BY REFERENCE (Python's `module.qualname` analogue): fn-name str.
/// Only a TOP-LEVEL NAMED function resolves — the decoder looks the name up in
/// the destination program's fn registry ([`register_program_fn`]) and rebuilds
/// a first-class fn value over its uniform-ABI thunk. Arrows, closures, bound
/// functions and class methods do NOT serialize (a TypeError — exactly the line
/// Python draws for lambdas/locals).
pub(crate) const OP_FN_REF: u8 = 22;

/// Recursion ceiling for encode AND decode — a nesting deeper than this errors
/// instead of overflowing the Rust stack (Python's pickle errors likewise).
pub(crate) const MAX_DEPTH: u32 = 2000;

// ── Varint helpers (LEB128, protobuf-style) ─────────────────────────────────

pub(crate) fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

pub(crate) fn put_str(out: &mut Vec<u8>, s: &[u8]) {
    put_varint(out, s.len() as u64);
    out.extend_from_slice(s);
}

/// ZigZag-encode a signed value so small negatives stay small on the wire.
pub(crate) fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

pub(crate) fn unzigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}

// ── Extension codecs (non-primordial classes, registered by their owner) ────

/// A pluggable (de)serializer for one heap-entry family the engine core does
/// not know by name (Date, RegExp, …). `encode` inspects an [`Entry`] and
/// returns the payload bytes when it recognizes it (it MUST NOT touch other
/// handles — it runs under the entry's shard lock); `decode` rebuilds the value
/// from the payload and returns the fresh RAW handle.
pub struct ExtCodec {
    /// Wire tag — the JS class name ("Date", "RegExp", …). Stable format surface.
    pub tag: &'static str,
    pub encode: fn(&Entry) -> Option<Vec<u8>>,
    pub decode: fn(&[u8]) -> Option<u64>,
}

static EXT_CODECS: OnceLock<RwLock<Vec<ExtCodec>>> = OnceLock::new();

fn ext_codecs() -> &'static RwLock<Vec<ExtCodec>> {
    EXT_CODECS.get_or_init(|| RwLock::new(Vec::new()))
}

/// Register (or idempotently replace, keyed by `tag`) an extension codec.
/// Called by the crate that OWNS the class at its namespace-register time.
pub fn register_ext_codec(codec: ExtCodec) {
    let mut g = ext_codecs().write().unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = g.iter_mut().find(|c| c.tag == codec.tag) {
        *slot = codec;
    } else {
        g.push(codec);
    }
}

/// First registered codec that recognizes `entry` → `(tag, payload)`.
pub(crate) fn ext_encode(entry: &Entry) -> Option<(&'static str, Vec<u8>)> {
    let g = ext_codecs().read().unwrap_or_else(|e| e.into_inner());
    g.iter().find_map(|c| (c.encode)(entry).map(|p| (c.tag, p)))
}

/// Decode an `OP_EXT` payload by tag → fresh raw handle.
pub(crate) fn ext_decode(tag: &str, payload: &[u8]) -> Option<u64> {
    let g = ext_codecs().read().unwrap_or_else(|e| e.into_inner());
    g.iter().find(|c| c.tag == tag).and_then(|c| (c.decode)(payload))
}

// ── Class-revive hook (proto attachment lives ABOVE the engine) ─────────────

/// Attach the class prototype to a freshly revived instance (`obj_word`,
/// `class_name`). The proto tables live in rts-adapters (`__rtsadp_class_proto`
/// / `__rtsadp_obj_set_proto`), a crate ABOVE this one — installed at engine
/// bootstrap, the same dep-direction pattern as `SHAPED_OBJ_HOOK` /
/// `install_gc_hook`. Absent hook (a bare-engine unit test) → the instance
/// still has the right shape id (`instanceof` works); only proto-resolved
/// method dispatch would miss.
pub type ClassReviveHook = fn(obj_word: u64, class_name: &str);

static CLASS_REVIVE_HOOK: OnceLock<ClassReviveHook> = OnceLock::new();

/// Install the class-revive hook (idempotent; first install wins).
pub fn set_class_revive_hook(f: ClassReviveHook) {
    let _ = CLASS_REVIVE_HOOK.set(f);
}

pub(crate) fn class_revive(obj_word: u64, class_name: &str) {
    if let Some(f) = CLASS_REVIVE_HOOK.get() {
        f(obj_word, class_name);
    }
}

// ── Program fn registry (functions BY REFERENCE, Python-style) ──────────────

/// One top-level function of the LIVE program, registered by the JIT after
/// finalize: the uniform-ABI thunk pointer (words in/out — the same convention
/// every engine fn-value uses) + arity.
#[derive(Clone, Copy)]
pub struct FnInfo {
    pub ptr: u64,
    pub arity: u8,
}

struct FnRegistry {
    by_name: std::collections::HashMap<String, FnInfo>,
    /// Any pointer identifying the fn (thunk AND raw body) → its name, so the
    /// encoder can match whatever pointer a reified fn value carries.
    by_ptr: std::collections::HashMap<u64, String>,
}

fn fn_registry() -> &'static RwLock<FnRegistry> {
    static T: OnceLock<RwLock<FnRegistry>> = OnceLock::new();
    T.get_or_init(|| {
        RwLock::new(FnRegistry {
            by_name: std::collections::HashMap::new(),
            by_ptr: std::collections::HashMap::new(),
        })
    })
}

/// Clear the fn registry at a program boundary (stale pointers from a dropped
/// JIT module must never resolve).
pub fn reset_program_fns() {
    let mut g = fn_registry().write().unwrap_or_else(|e| e.into_inner());
    g.by_name.clear();
    g.by_ptr.clear();
}

/// Register one top-level program function: `thunk_ptr` is what a revived fn
/// value invokes (uniform ABI); `alias_ptrs` are additional pointers (the raw
/// body) that identify the same fn at encode time.
pub fn register_program_fn(name: &str, thunk_ptr: u64, arity: u8, alias_ptrs: &[u64]) {
    let mut g = fn_registry().write().unwrap_or_else(|e| e.into_inner());
    g.by_name.insert(name.to_string(), FnInfo { ptr: thunk_ptr, arity });
    g.by_ptr.insert(thunk_ptr, name.to_string());
    for &p in alias_ptrs {
        if p != 0 {
            g.by_ptr.insert(p, name.to_string());
        }
    }
}

pub(crate) fn fn_name_of_ptr(ptr: u64) -> Option<String> {
    fn_registry().read().unwrap_or_else(|e| e.into_inner()).by_ptr.get(&ptr).cloned()
}

pub(crate) fn fn_info_of(name: &str) -> Option<FnInfo> {
    fn_registry().read().unwrap_or_else(|e| e.into_inner()).by_name.get(name).copied()
}
