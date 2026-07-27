//! node:v8 — the `extern "C"` entry points: `serialize` (value → Buffer) and
//! `deserialize` (Buffer → value), over the engine's native pickle primitive
//! (`rts_engine::heap::pickle`, RTSP wire format — cycles + shared identity
//! via memo back-references).

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::pickle;
use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_PAYLOAD_MASK};

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// Read the bytes of a Buffer/Uint8Array/string argument.
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

/// A byte slice → `Uint8Array`-shaped `Entry::Vec` (each byte an inline-f64 word).
fn byte_array(bytes: &[u8]) -> u64 {
    let words: Vec<i64> = bytes.iter().map(|&b| f64::from(b).to_bits() as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

fn throw(msg: &str) {
    unsafe { __rtsadp_throw_js_error(b"Error".as_ptr(), 5, msg.as_ptr(), msg.len() as i64) };
}

/// `v8.serialize(value)` → Buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_V8_SERIALIZE(value: u64) -> u64 {
    match pickle::serialize_value(value) {
        Ok(out) => byte_array(&out),
        Err(e) => {
            throw(&e);
            byte_array(&[])
        }
    }
}

/// `v8.deserialize(buffer)` → value.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_V8_DESERIALIZE(buffer: u64) -> u64 {
    match pickle::deserialize_value(&read_bytes(buffer)) {
        Ok(word) => word,
        Err(e) => {
            throw(&e);
            rts_engine::heap::poly::POLY_UNDEFINED
        }
    }
}
