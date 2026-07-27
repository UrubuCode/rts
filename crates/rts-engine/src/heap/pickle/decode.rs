//! `pickle` decode — cursor over the `RTSP` stream, mirroring the encoder's
//! memo discipline: a container allocates its PLACEHOLDER (and registers it in
//! the memo) BEFORE decoding children, so an `OP_REF` back-edge resolves even
//! mid-construction — that is what rebuilds cycles. Every allocated handle is
//! PINNED for the duration of the decode (a GC tick between allocations would
//! otherwise sweep entries only reachable from this Rust-heap state, which the
//! conservative stack scanner cannot see) and unpinned before returning.

use super::super::handles::{
    alloc_entry, pin_handle, unpin_handle, with_entry_mut, ArrayBufferBacking, ArrayBufferData,
    BigIntData, Entry, FunctionData,
};
use super::super::poly::{
    POLY_BOX_BASE, POLY_SING_HOLE, POLY_TAG_INT32, POLY_TAG_MASK, POLY_TAG_OBJECT,
    POLY_TAG_SHIFT, POLY_TAG_SINGLETON, POLY_TAG_STR, POLY_UNDEFINED,
};
use super::super::shapes::{
    alloc_shaped_object_owned, bool_word, class_shape_of, global_shape_keys, handle_word_auto,
    null_word, shape_id_word, string_word,
};
use super::*;

/// Deserialize a whole `RTSP` v1 byte stream into one PolyValue word.
pub fn deserialize_value(bytes: &[u8]) -> Result<u64, String> {
    let mut d = Dec {
        c: Cursor { b: bytes, i: 0 },
        memo: Vec::new(),
        pinned: Vec::new(),
    };
    let result = d.header().and_then(|()| d.value(0));
    for h in d.pinned.drain(..) {
        unpin_handle(h);
    }
    result
}

struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn u8(&mut self) -> Result<u8, String> {
        let v = *self.b.get(self.i).ok_or("pickle: truncated stream")?;
        self.i += 1;
        Ok(v)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.i.checked_add(n).ok_or("pickle: length overflow")?;
        let s = self.b.get(self.i..end).ok_or("pickle: truncated stream")?;
        self.i = end;
        Ok(s)
    }
    fn varint(&mut self) -> Result<u64, String> {
        let mut v: u64 = 0;
        for shift in 0..10 {
            let byte = self.u8()?;
            v |= u64::from(byte & 0x7F) << (shift * 7);
            if byte & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err("pickle: varint too long".into())
    }
    fn len(&mut self) -> Result<usize, String> {
        let n = self.varint()? as usize;
        // A declared length can never exceed the remaining stream.
        if n > self.b.len().saturating_sub(self.i) {
            return Err("pickle: truncated stream".into());
        }
        Ok(n)
    }
    fn str_block(&mut self) -> Result<&'a [u8], String> {
        let n = self.len()?;
        self.bytes(n)
    }
    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
}

struct Dec<'a> {
    c: Cursor<'a>,
    /// memo id → materialized PolyValue word, mirroring the encoder's ids.
    memo: Vec<u64>,
    /// Every handle allocated during this decode — GC-pinned until the end.
    pinned: Vec<u64>,
}

impl<'a> Dec<'a> {
    fn header(&mut self) -> Result<(), String> {
        if self.c.bytes(4)? != MAGIC {
            return Err("pickle: not an RTSP stream (bad magic)".into());
        }
        let version = self.c.u8()?;
        if version > VERSION {
            return Err(format!("pickle: format version {version} is newer than supported ({VERSION})"));
        }
        Ok(())
    }

    /// Allocate + pin, returning the raw handle.
    fn alloc(&mut self, e: Entry) -> u64 {
        let h = alloc_entry(e);
        pin_handle(h);
        self.pinned.push(h);
        h
    }

    fn value(&mut self, depth: u32) -> Result<u64, String> {
        if depth > MAX_DEPTH {
            return Err("pickle: nesting too deep".into());
        }
        match self.c.u8()? {
            OP_UNDEF => Ok(POLY_UNDEFINED),
            OP_NULL => Ok(null_word()),
            OP_FALSE => Ok(bool_word(false)),
            OP_TRUE => Ok(bool_word(true)),
            OP_HOLE => {
                Ok(POLY_BOX_BASE | (POLY_TAG_SINGLETON << POLY_TAG_SHIFT) | POLY_SING_HOLE)
            }
            OP_F64 => Ok(double_word(self.c.f64()?)),
            OP_I32 => {
                let v = unzigzag(self.c.varint()?) as i32;
                Ok(POLY_BOX_BASE | (POLY_TAG_INT32 << POLY_TAG_SHIFT) | (v as u32 as u64))
            }
            OP_STR => {
                let s = self.c.str_block()?;
                let word = string_word(s);
                self.pin_word(word);
                self.memo.push(word);
                Ok(word)
            }
            OP_REF => {
                let id = self.c.varint()? as usize;
                self.memo.get(id).copied().ok_or_else(|| "pickle: bad back-reference".into())
            }
            OP_ARRAY => {
                let n = self.c.len()?;
                let h = self.alloc(Entry::Vec(Box::new(Vec::new())));
                let word = handle_word_auto(h);
                self.memo.push(word);
                let mut slots = Vec::with_capacity(n);
                for _ in 0..n {
                    slots.push(self.value(depth + 1)? as i64);
                }
                with_entry_mut(h, |e| {
                    if let Some(Entry::Vec(v)) = e {
                        **v = slots;
                    }
                });
                Ok(word)
            }
            OP_OBJECT => {
                let n = self.c.len()?;
                let mut keys = Vec::with_capacity(n);
                for _ in 0..n {
                    keys.push(String::from_utf8_lossy(self.c.str_block()?).into_owned());
                }
                // Placeholder with undefined slots FIRST, so a cycle back to
                // this object resolves; values are patched in below.
                let undef = vec![POLY_UNDEFINED as i64; n];
                let h = alloc_shaped_object_owned(keys, &undef);
                pin_handle(h);
                self.pinned.push(h);
                let word = handle_word_auto(h);
                self.memo.push(word);
                let mut values = Vec::with_capacity(n);
                for _ in 0..n {
                    values.push(self.value(depth + 1)? as i64);
                }
                with_entry_mut(h, |e| {
                    if let Some(Entry::Vec(slots)) = e {
                        slots[1..].copy_from_slice(&values);
                    }
                });
                Ok(word)
            }
            OP_CLASS => {
                let class = String::from_utf8_lossy(self.c.str_block()?).into_owned();
                let n = self.c.len()?;
                let mut keys = Vec::with_capacity(n);
                for _ in 0..n {
                    keys.push(String::from_utf8_lossy(self.c.str_block()?).into_owned());
                }
                // The DESTINATION program's shape for this class name — that is
                // what the baked `instanceof` shape-id compares match against.
                let dest_shape = class_shape_of(&class).ok_or_else(|| {
                    format!("pickle: class '{class}' is not declared in this program")
                })?;
                let dest_keys = global_shape_keys(dest_shape)
                    .ok_or_else(|| format!("pickle: class '{class}' has no shape layout"))?;
                let mut slots = vec![POLY_UNDEFINED as i64; dest_keys.len() + 1];
                slots[0] = shape_id_word(dest_shape) as i64;
                let h = self.alloc(Entry::Vec(Box::new(slots)));
                let word = handle_word_auto(h);
                self.memo.push(word);
                // Fields matched BY KEY against the destination layout: extra
                // stream fields drop, missing ones stay undefined (schema
                // evolution tolerated — stricter than Python only in shape).
                for key in &keys {
                    let v = self.value(depth + 1)? as i64;
                    if let Some(pos) = dest_keys.iter().position(|k| k == key) {
                        with_entry_mut(h, |e| {
                            if let Some(Entry::Vec(s)) = e {
                                s[1 + pos] = v;
                            }
                        });
                    }
                }
                class_revive(word, &class);
                Ok(word)
            }
            OP_BUFFER => {
                let bytes = self.c.str_block()?.to_vec();
                let h = self.alloc(Entry::Buffer(bytes));
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            OP_ARRAYBUF => {
                let bytes = self.c.str_block()?.to_vec();
                let h = self.alloc(Entry::ArrayBuffer(Box::new(ArrayBufferData {
                    backing: ArrayBufferBacking::Owned(bytes.into_boxed_slice()),
                    detached: false,
                })));
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            OP_BIGINT => {
                let negative = self.c.u8()? != 0;
                let count = self.c.varint()? as usize;
                if count > self.c.b.len().saturating_sub(self.c.i) / 8 {
                    return Err("pickle: truncated stream".into());
                }
                let mut words = Vec::with_capacity(count);
                for _ in 0..count {
                    words.push(u64::from_le_bytes(self.c.bytes(8)?.try_into().unwrap()));
                }
                let h = self.alloc(Entry::BigInt(Box::new(BigIntData { negative, words })));
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            OP_ERROR => {
                let name = String::from_utf8_lossy(self.c.str_block()?).into_owned();
                let message = String::from_utf8_lossy(self.c.str_block()?).into_owned();
                let h = self.alloc(Entry::ErrorObj { message, name, cause: 0 });
                let word = handle_word_auto(h);
                self.memo.push(word);
                if self.c.u8()? == 1 {
                    let cause_word = self.value(depth + 1)?;
                    let cause = word_handle(cause_word);
                    with_entry_mut(h, |e| {
                        if let Some(Entry::ErrorObj { cause: c, .. }) = e {
                            *c = cause;
                        }
                    });
                }
                Ok(word)
            }
            OP_BOOLBOX => {
                let b = self.c.u8()? != 0;
                let h = self.alloc(Entry::BooleanBox(b));
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            OP_NUMBOX => {
                let f = self.c.f64()?;
                let h = self.alloc(Entry::NumberBox(f));
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            OP_STRBOX => {
                // Reserve the memo slot before the inner string (the encoder
                // assigned the box's id first); patch it once allocated.
                let idx = self.memo.len();
                self.memo.push(POLY_UNDEFINED);
                let inner_word = self.value(depth + 1)?;
                let inner = word_handle(inner_word);
                if inner == 0 {
                    return Err("pickle: corrupt String box".into());
                }
                let h = self.alloc(Entry::StringBox(inner));
                let word = handle_word_auto(h);
                self.memo[idx] = word;
                Ok(word)
            }
            OP_FLOATPRIM => Ok(double_word(self.c.f64()?)),
            OP_JSON => {
                let bytes = self.c.str_block()?;
                let v: serde_json::Value = serde_json::from_slice(bytes)
                    .map_err(|e| format!("pickle: corrupt JSON payload: {e}"))?;
                let h = self.alloc(Entry::Json(Box::new(v)));
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            OP_FN_REF => {
                let name = String::from_utf8_lossy(self.c.str_block()?).into_owned();
                let info = fn_info_of(&name).ok_or_else(|| {
                    format!("pickle: function '{name}' is not declared in this program")
                })?;
                // A first-class fn value over the DESTINATION program's uniform
                // thunk — same shape `new Function` produces (dynfn contract).
                let h = self.alloc(Entry::Function(Box::new(FunctionData {
                    fn_ptr: info.ptr,
                    arity: info.arity,
                    name: name.clone().into_boxed_str(),
                    bound_this: 0,
                    has_bound_this: false,
                    bound_args: Vec::new(),
                    is_arrow: false,
                    has_this_param: false,
                    param_kinds: Vec::new(),
                    return_kind: 0,
                    packed_shim: 0,
                    source: None,
                    keep_alive: None,
                    prototype_handle: 0,
                    rest_param_idx: -1,
                    uniform_thunk: true,
                })));
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            OP_EXT => {
                let tag = String::from_utf8_lossy(self.c.str_block()?).into_owned();
                let payload = self.c.str_block()?;
                let h = ext_decode(&tag, payload)
                    .ok_or_else(|| format!("pickle: no decoder registered for class '{tag}'"))?;
                pin_handle(h);
                self.pinned.push(h);
                let word = handle_word_auto(h);
                self.memo.push(word);
                Ok(word)
            }
            other => Err(format!("pickle: unknown opcode {other}")),
        }
    }

    /// Pin the handle behind an already-boxed word (fresh string words).
    fn pin_word(&mut self, word: u64) {
        let h = word_handle(word);
        if h != 0 {
            pin_handle(h);
            self.pinned.push(h);
        }
    }
}

/// NaN-canonicalize so a wire double never lands in the boxed quadrant.
fn double_word(f: f64) -> u64 {
    if f.is_nan() {
        0x7FF8_0000_0000_0000
    } else {
        f.to_bits()
    }
}

/// Raw handle behind a boxed STR/OBJECT word, or 0.
fn word_handle(word: u64) -> u64 {
    if (word & POLY_BOX_BASE) != POLY_BOX_BASE {
        return 0;
    }
    let tag = (word >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
    if tag == POLY_TAG_STR || tag == POLY_TAG_OBJECT {
        super::super::poly::poly_handle_normalize(word).unwrap_or(0)
    } else {
        0
    }
}
