//! node:zlib — value helpers: read the input bytes of a `*Sync(buffer)` arg
//! (Buffer / Uint8Array / string), return a `Buffer`, and the error throw.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK};
use rts_engine::heap::shapes::global_shape_keys;

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// Read `options.level` (0..=9) off an options object, defaulting to 6 (the
/// flate2/zlib default). Accepts a raw `Handle` or a boxed word; a shaped object
/// or an `Entry::Map`.
pub fn opt_level(options: u64) -> u32 {
    let handle = poly_handle_normalize(options).unwrap_or(options);
    let word = with_entry(handle, |e| match e {
        Some(Entry::Vec(slots)) if !slots.is_empty() => {
            let w0 = slots[0] as u64;
            if (w0 & POLY_BOX_BASE) != POLY_BOX_BASE {
                return None;
            }
            let keys = global_shape_keys((w0 & POLY_PAYLOAD_MASK) as u32)?;
            if keys.len() + 1 != slots.len() {
                return None;
            }
            keys.iter().position(|k| k == "level").map(|i| slots[i + 1])
        }
        Some(Entry::Map(m)) => m.get("level").copied(),
        _ => None,
    });
    match word {
        Some(w) => {
            let u = w as u64;
            let v = if (u & POLY_BOX_BASE) != POLY_BOX_BASE {
                f64::from_bits(u) as i64
            } else {
                (u & POLY_PAYLOAD_MASK) as u32 as i64
            };
            v.clamp(0, 9) as u32
        }
        None => 6,
    }
}

/// Read the raw bytes of a compress/decompress argument: `Entry::Buffer`
/// (ArrayBuffer view), `Entry::Vec` (`new Uint8Array([...])`, boxed number
/// words), or `Entry::String` (its UTF-8 bytes).
pub fn read_bytes(handle: u64) -> Vec<u8> {
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

/// Allocate a `Buffer` (`Entry::Buffer`) from `bytes` → handle.
pub fn buffer(bytes: Vec<u8>) -> u64 {
    alloc_entry(Entry::Buffer(bytes))
}

/// Throw an `Error` with `message`.
pub fn throw(message: &str) {
    let kind = "Error";
    unsafe {
        __rtsadp_throw_js_error(kind.as_ptr(), kind.len() as i64, message.as_ptr(), message.len() as i64);
    }
}
