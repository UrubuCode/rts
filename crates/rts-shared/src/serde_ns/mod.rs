//! `serde` namespace — the user surface over the engine's native pickle
//! primitive (`rts_engine::heap::pickle`): deep binary serialization of runtime
//! values (RTSP wire format), with a memo table for cyclic graphs and shared
//! identity — what `JSON.stringify` structurally cannot express.
//!
//! ```ts
//! import { serialize, deserialize } from "rts:serde";
//! const bytes = serialize(anyValue);   // number[] of bytes
//! const back = deserialize(bytes);     // deep copy, cycles intact
//! ```
//!
//! Members are authored with `#[rtse::function]` (F7 of
//! `docs/specs/rts-macro-single-source.md`): symbol, signature, `ts_signature`
//! and fn-ptr all derive from the Rust declaration. The macro fixes
//! `MemberFlags::NONE`, so `register` patches `THROWS` on (both members throw a
//! JS `TypeError` on an unserializable value / corrupt stream).
//!
//! Functions, symbols, sockets, threads and other live resources are
//! unserializable (a thrown TypeError, exactly like Python's pickle with a
//! socket). Class instances are phase 2. Date and RegExp round-trip via the
//! extension codecs registered here (`codecs`) — the engine core never names
//! them (PRIMORDIAL-vs-REGISTRY doctrine).
//!
//! Module dir is `serde_ns` (not `serde`) only to avoid shadowing the `serde`
//! crate rts-shared already depends on; the JS-visible namespace is `serde`.

mod codecs;

use rts_engine::abi::ty::{Handle, Poly};
use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::pickle;
use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_UNDEFINED};
use rts_engine::{Engine, Member, MemberFlags};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

fn throw_type_error(msg: &str) {
    unsafe { __rtsadp_throw_js_error(b"TypeError".as_ptr(), 9, msg.as_ptr(), msg.len() as i64) };
}

/// A byte slice → `number[]`-shaped `Entry::Vec` (each byte an inline-f64 word).
fn byte_array(bytes: &[u8]) -> u64 {
    let words: Vec<i64> = bytes.iter().map(|&b| f64::from(b).to_bits() as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// Read the bytes of a Buffer / byte-array / string argument.
fn read_bytes(handle: u64) -> Vec<u8> {
    with_entry(handle, |e| match e {
        Some(Entry::Buffer(b)) => b.clone(),
        Some(Entry::String(s)) => s.clone(),
        Some(Entry::Vec(v)) => v
            .iter()
            .map(|&w| {
                let u = w as u64;
                if (u & POLY_BOX_BASE) != POLY_BOX_BASE {
                    f64::from_bits(u) as u8
                } else {
                    (u & POLY_PAYLOAD_MASK) as u32 as u8
                }
            })
            .collect(),
        _ => Vec::new(),
    })
}

/// Deep-serialize any value graph to RTSP bytes — cycles and shared references
/// round-trip via memo back-references. Throws TypeError on functions, symbols,
/// class instances and live resources (sockets, threads, pending Promises).
#[rtse::function(module = "serde")]
pub fn serialize(value: Poly) -> Handle {
    match pickle::serialize_value(value) {
        Ok(bytes) => byte_array(&bytes),
        Err(e) => {
            throw_type_error(&e);
            byte_array(&[])
        }
    }
}

/// Rebuild the value graph from RTSP bytes (a deep copy; cycles and shared
/// identity intact). Throws TypeError on a corrupt or unsupported stream.
#[rtse::function(module = "serde")]
pub fn deserialize(bytes: Handle) -> Poly {
    match pickle::deserialize_value(&read_bytes(bytes)) {
        Ok(word) => word,
        Err(e) => {
            throw_type_error(&e);
            POLY_UNDEFINED
        }
    }
}

/// The macro pins `MemberFlags::NONE`; both members throw.
fn throws(mut m: Member) -> Member {
    m.flags = MemberFlags::THROWS;
    m
}

/// Registers the `serde` namespace + the Date/RegExp pickle codecs.
pub fn register(e: &mut Engine) {
    codecs::install();
    e.module("serde", |m| {
        m.doc("Deep binary serialization (pickle): serialize/deserialize any value graph — cycles, shared references, Date, RegExp, BigInt, Error, buffers.");
        m.registry(throws(serialize_entry()));
        m.registry(throws(deserialize_entry()));
    });
}
