//! node:v8 — the structured-clone serialize/deserialize over RTS's OWN wire
//! format (v8.md §24: "RTS's own wire format … not V8's ValueSerializer byte
//! format"). A real recursive walk of the PolyValue graph — numbers, strings,
//! booleans, null/undefined, arrays, and plain (shaped/Map) objects round-trip
//! in-runtime. Functions/symbols are unserializable (an error, like V8).

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{
    poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_SHIFT,
    POLY_UNDEFINED,
};
use rts_engine::heap::shapes::{alloc_shaped_object, bool_word, global_shape_keys, null_word, string_word};

// Wire tags (RTS-own).
const T_UNDEF: u8 = 0;
const T_NULL: u8 = 1;
const T_FALSE: u8 = 2;
const T_TRUE: u8 = 3;
const T_DOUBLE: u8 = 5;
const T_STRING: u8 = 6;
const T_ARRAY: u8 = 7;
const T_OBJECT: u8 = 8;

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// Serialize one PolyValue word into `out`. Errors on a function/symbol.
pub fn encode(word: u64, out: &mut Vec<u8>) -> Result<(), String> {
    // A genuine inline double (not in the boxed NaN quadrant).
    if (word & POLY_BOX_BASE) != POLY_BOX_BASE {
        out.push(T_DOUBLE);
        out.extend_from_slice(&f64::from_bits(word).to_le_bytes());
        return Ok(());
    }
    let tag = (word >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
    match tag {
        1 => {
            // boxed INT32 → the same value as a double (exact for i32).
            let v = (word & POLY_PAYLOAD_MASK) as u32 as i32;
            out.push(T_DOUBLE);
            out.extend_from_slice(&(v as f64).to_le_bytes());
        }
        2 => {
            let p = word & POLY_PAYLOAD_MASK;
            out.push(match p {
                1 => T_NULL,
                2 => T_FALSE,
                3 => T_TRUE,
                _ => T_UNDEF,
            });
        }
        3 => {
            let h = poly_handle_normalize(word).ok_or("serialize: bad string handle")?;
            let bytes = with_entry(h, |e| match e {
                Some(Entry::String(s)) => s.clone(),
                _ => Vec::new(),
            });
            out.push(T_STRING);
            put_u32(out, bytes.len() as u32);
            out.extend_from_slice(&bytes);
        }
        4 => {
            let h = poly_handle_normalize(word).ok_or("serialize: bad object handle")?;
            encode_heap(h, out)?;
        }
        _ => return Err("serialize: a function or symbol cannot be cloned".into()),
    }
    Ok(())
}

/// An array's element words, or an object's `(key, value_word)` pairs.
enum Heap {
    Array(Vec<i64>),
    Object(Vec<(String, i64)>),
}

fn heap_of(h: u64) -> Option<Heap> {
    with_entry(h, |e| match e {
        Some(Entry::Vec(slots)) => {
            // Shaped object? slot 0 is a boxed-INT32 REGISTERED shape id.
            if let Some(&w0) = slots.first() {
                let w0 = w0 as u64;
                if (w0 & POLY_BOX_BASE) == POLY_BOX_BASE {
                    let shape_id = (w0 & POLY_PAYLOAD_MASK) as u32;
                    if let Some(keys) = global_shape_keys(shape_id) {
                        if keys.len() + 1 == slots.len() {
                            return Some(Heap::Object(
                                keys.into_iter().zip(slots[1..].iter().copied()).collect(),
                            ));
                        }
                    }
                }
            }
            Some(Heap::Array(slots.as_ref().clone()))
        }
        Some(Entry::Map(m)) => Some(Heap::Object(m.iter().map(|(k, v)| (k.clone(), *v)).collect())),
        _ => None,
    })
}

fn encode_heap(h: u64, out: &mut Vec<u8>) -> Result<(), String> {
    match heap_of(h).ok_or("serialize: unserializable heap value")? {
        Heap::Array(slots) => {
            out.push(T_ARRAY);
            put_u32(out, slots.len() as u32);
            for w in slots {
                encode(w as u64, out)?;
            }
        }
        Heap::Object(pairs) => {
            out.push(T_OBJECT);
            put_u32(out, pairs.len() as u32);
            for (k, v) in pairs {
                put_u32(out, k.len() as u32);
                out.extend_from_slice(k.as_bytes());
                encode(v as u64, out)?;
            }
        }
    }
    Ok(())
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.i).ok_or("deserialize: truncated")?;
        self.i += 1;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, String> {
        let end = self.i + 4;
        let slice = self.b.get(self.i..end).ok_or("deserialize: truncated u32")?;
        self.i = end;
        Ok(u32::from_le_bytes(slice.try_into().unwrap()))
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.i + n;
        let slice = self.b.get(self.i..end).ok_or("deserialize: truncated bytes")?;
        self.i = end;
        Ok(slice)
    }
}

/// Deserialize the whole buffer into one PolyValue word.
pub fn decode(bytes: &[u8]) -> Result<u64, String> {
    let mut c = Cursor { b: bytes, i: 0 };
    decode_one(&mut c)
}

fn double_word(f: f64) -> u64 {
    // Canonicalize NaN to positive qNaN so it never collides with the boxed
    // (negative-qNaN) space.
    if f.is_nan() {
        0x7FF8_0000_0000_0000
    } else {
        f.to_bits()
    }
}

fn decode_one(c: &mut Cursor) -> Result<u64, String> {
    match c.u8()? {
        T_UNDEF => Ok(POLY_UNDEFINED),
        T_NULL => Ok(null_word()),
        T_FALSE => Ok(bool_word(false)),
        T_TRUE => Ok(bool_word(true)),
        T_DOUBLE => {
            let raw = c.bytes(8)?;
            Ok(double_word(f64::from_le_bytes(raw.try_into().unwrap())))
        }
        T_STRING => {
            let n = c.u32()? as usize;
            let s = c.bytes(n)?;
            Ok(string_word(s) as u64)
        }
        T_ARRAY => {
            let n = c.u32()? as usize;
            let mut slots = Vec::with_capacity(n);
            for _ in 0..n {
                slots.push(decode_one(c)? as i64);
            }
            Ok(handle_object(alloc_entry(Entry::Vec(Box::new(slots)))))
        }
        T_OBJECT => {
            let n = c.u32()? as usize;
            let mut keys: Vec<String> = Vec::with_capacity(n);
            let mut values: Vec<i64> = Vec::with_capacity(n);
            for _ in 0..n {
                let klen = c.u32()? as usize;
                keys.push(String::from_utf8_lossy(c.bytes(klen)?).into_owned());
                values.push(decode_one(c)? as i64);
            }
            let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
            Ok(handle_object(alloc_shaped_object(&key_refs, &values)))
        }
        other => Err(format!("deserialize: unknown tag {other}")),
    }
}

/// Box a freshly-allocated heap handle as an OBJECT word.
fn handle_object(h: u64) -> u64 {
    rts_engine::heap::shapes::handle_word_auto(h)
}
