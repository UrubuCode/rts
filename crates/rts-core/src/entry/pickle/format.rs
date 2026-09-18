//! The bytes: opcodes, varints, and text.
//!
//! `docs/engine/pickle.md` is the specification of the stream; this module is
//! the part of it that is about BYTES rather than about values, and it is the
//! only place either direction spells an opcode's number or a varint's shape.

use crate::text::Str;

/// File magic — "RTSP" (RTS Pickle).
pub(super) const MAGIC: [u8; 4] = *b"RTSP";
/// The version this writes. The reader accepts it and `1`.
pub(super) const VERSION: u8 = 2;

pub(super) const OP_UNDEF: u8 = 0;
pub(super) const OP_NULL: u8 = 1;
pub(super) const OP_FALSE: u8 = 2;
pub(super) const OP_TRUE: u8 = 3;
pub(super) const OP_HOLE: u8 = 4;
pub(super) const OP_F64: u8 = 5;
pub(super) const OP_I32: u8 = 6;
pub(super) const OP_STR: u8 = 7;
pub(super) const OP_REF: u8 = 8;
pub(super) const OP_ARRAY: u8 = 9;
pub(super) const OP_OBJECT: u8 = 10;
/// A Node `Buffer`.
pub(super) const OP_BUFFER: u8 = 11;
pub(super) const OP_ARRAYBUF: u8 = 12;
pub(super) const OP_BIGINT: u8 = 13;
pub(super) const OP_ERROR: u8 = 14;
pub(super) const OP_BOOLBOX: u8 = 15;
pub(super) const OP_NUMBOX: u8 = 16;
pub(super) const OP_STRBOX: u8 = 17;
/// v1 only: a double the old engine kept boxed. Read, never written.
pub(super) const OP_FLOATPRIM: u8 = 18;
pub(super) const OP_EXT: u8 = 19;
/// v1 only: a parsed-JSON value the old engine kept whole. Read, never written.
pub(super) const OP_JSON: u8 = 20;
pub(super) const OP_CLASS: u8 = 21;
pub(super) const OP_FN_REF: u8 = 22;
pub(super) const OP_MAP: u8 = 23;
pub(super) const OP_SET: u8 = 24;
pub(super) const OP_VIEW: u8 = 25;
/// `Object.create(null)`: OBJECT's payload, and no prototype on the way back.
pub(super) const OP_BARE: u8 = 26;
/// A symbol, as the strref of its stream spelling — `super::symbols`.
pub(super) const OP_SYMBOL: u8 = 27;

/// How deep either direction nests before it refuses.
///
/// Neither direction recurses — both keep their own stack — so this is not a
/// guard on Rust's stack, which is what v1's 2000 was. It is a bound on the
/// memory a hostile stream can make the reader commit to open containers with
/// two bytes each, and it is far above anything a program builds on purpose: a
/// linked list of this many nodes is this many levels, and pickles.
pub(super) const MAX_DEPTH: usize = 100_000;

/// What went wrong reading a stream, as the text the `TypeError` carries.
pub(super) type Broken = String;

/// Writes a varint: LEB128, protobuf's.
pub(super) fn varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// ZigZag, so a small negative stays small on the wire.
pub(super) fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

pub(super) fn unzigzag(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

/// A string's code units as generalised UTF-8 — WTF-8.
///
/// # Why not UTF-8
///
/// A JavaScript string is UTF-16 code units and may hold a LONE surrogate,
/// which UTF-8 cannot spell. v1 wrote UTF-8 and so could not round-trip one;
/// WTF-8 encodes a lone surrogate as the three bytes its code point would take,
/// and is byte-identical to UTF-8 for every string that has none — so a v1
/// stream reads through the same decoder.
///
/// # Why straight from the representation
///
/// The narrow layout's ASCII case is the common one and is its own bytes, so it
/// is one copy with no intermediate `String`; the rest is encoded unit by unit
/// into the output, again with nothing in between.
pub(super) fn wtf8(out: &mut Vec<u8>, text: &Str) {
    if let Some(narrow) = text.narrow() {
        if narrow.is_ascii() {
            out.extend_from_slice(narrow);
            return;
        }
        for byte in narrow {
            push_scalar(out, u32::from(*byte));
        }
        return;
    }
    let mut units = text.units().peekable();
    while let Some(unit) = units.next() {
        let unit = u32::from(unit);
        if (0xD800..0xDC00).contains(&unit)
            && let Some(&low) = units.peek()
            && (0xDC00..0xE000).contains(&u32::from(low))
        {
            units.next();
            push_scalar(out, 0x10000 + ((unit - 0xD800) << 10) + (u32::from(low) - 0xDC00));
            continue;
        }
        push_scalar(out, unit);
    }
}

/// The WTF-8 byte length of a string, so its prefix can be written first.
pub(super) fn wtf8_len(text: &Str) -> usize {
    if let Some(narrow) = text.narrow() {
        return narrow.len() + narrow.iter().filter(|byte| **byte >= 0x80).count();
    }
    let mut length = 0;
    let mut units = text.units().peekable();
    while let Some(unit) = units.next() {
        length += match unit {
            0..=0x7F => 1,
            0x80..=0x7FF => 2,
            0xD800..=0xDBFF if units.peek().is_some_and(|low| (0xDC00..0xE000).contains(low)) => {
                units.next();
                4
            }
            _ => 3,
        };
    }
    length
}

fn push_scalar(out: &mut Vec<u8>, point: u32) {
    match point {
        0..=0x7F => out.push(point as u8),
        0x80..=0x7FF => {
            out.push(0xC0 | (point >> 6) as u8);
            out.push(0x80 | (point & 0x3F) as u8);
        }
        0x800..=0xFFFF => {
            out.push(0xE0 | (point >> 12) as u8);
            out.push(0x80 | ((point >> 6) & 0x3F) as u8);
            out.push(0x80 | (point & 0x3F) as u8);
        }
        _ => {
            out.push(0xF0 | (point >> 18) as u8);
            out.push(0x80 | ((point >> 12) & 0x3F) as u8);
            out.push(0x80 | ((point >> 6) & 0x3F) as u8);
            out.push(0x80 | (point & 0x3F) as u8);
        }
    }
}

/// Text back out of WTF-8, or `None` for bytes that are not.
pub(super) fn text(bytes: &[u8]) -> Option<Str> {
    if bytes.is_ascii() {
        return Some(Str::from_latin1(bytes));
    }
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let first = bytes[at];
        let (length, initial) = match first {
            0x00..=0x7F => (1, u32::from(first)),
            0xC2..=0xDF => (2, u32::from(first & 0x1F)),
            0xE0..=0xEF => (3, u32::from(first & 0x0F)),
            0xF0..=0xF4 => (4, u32::from(first & 0x07)),
            _ => return None,
        };
        let tail = bytes.get(at + 1..at + length)?;
        let mut point = initial;
        for byte in tail {
            if byte & 0xC0 != 0x80 {
                return None;
            }
            point = (point << 6) | u32::from(byte & 0x3F);
        }
        // An overlong spelling is not a spelling: refused, as UTF-8 refuses it.
        let shortest = match point {
            0..=0x7F => 1,
            0x80..=0x7FF => 2,
            0x800..=0xFFFF => 3,
            0x10000..=0x10FFFF => 4,
            _ => return None,
        };
        if shortest != length {
            return None;
        }
        match point {
            0x10000.. => {
                let offset = point - 0x10000;
                units.push(0xD800 + (offset >> 10) as u16);
                units.push(0xDC00 + (offset & 0x3FF) as u16);
            }
            _ => units.push(point as u16),
        }
        at += length;
    }
    Some(match units.iter().all(|unit| *unit < 0x100) {
        true => Str::owning_latin1(units.into_iter().map(|unit| unit as u8).collect()),
        false => Str::owning_utf16(units),
    })
}

/// A cursor over a stream, every read of which can fail rather than panic.
pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Cursor { bytes, at: 0 }
    }

    /// How many bytes are left — what bounds every count a stream declares.
    pub(super) fn left(&self) -> usize {
        self.bytes.len() - self.at
    }

    pub(super) fn byte(&mut self) -> Result<u8, Broken> {
        let found = *self.bytes.get(self.at).ok_or_else(truncated)?;
        self.at += 1;
        Ok(found)
    }

    pub(super) fn take(&mut self, count: usize) -> Result<&'a [u8], Broken> {
        let end = self.at.checked_add(count).ok_or_else(truncated)?;
        let found = self.bytes.get(self.at..end).ok_or_else(truncated)?;
        self.at = end;
        Ok(found)
    }

    pub(super) fn varint(&mut self) -> Result<u64, Broken> {
        let mut value: u64 = 0;
        for shift in 0..10 {
            let byte = self.byte()?;
            value |= u64::from(byte & 0x7F) << (shift * 7);
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("pickle: a varint longer than ten bytes".into())
    }

    /// A count of things that each take at least `each` bytes — refused when
    /// the stream is too short to hold them, which is what stops a hostile
    /// count from reserving memory the stream cannot fill.
    pub(super) fn count(&mut self, each: usize) -> Result<usize, Broken> {
        let declared = self.varint()?;
        let declared = usize::try_from(declared).map_err(|_| truncated())?;
        if declared.saturating_mul(each.max(1)) > self.left() {
            return Err(truncated());
        }
        Ok(declared)
    }

    /// A length-prefixed block of bytes.
    pub(super) fn block(&mut self) -> Result<&'a [u8], Broken> {
        let length = self.count(1)?;
        self.take(length)
    }

    pub(super) fn f64(&mut self) -> Result<f64, Broken> {
        let bytes = self.take(8)?;
        let mut word = [0u8; 8];
        word.copy_from_slice(bytes);
        Ok(f64::from_le_bytes(word))
    }

    pub(super) fn u64(&mut self) -> Result<u64, Broken> {
        let bytes = self.take(8)?;
        let mut word = [0u8; 8];
        word.copy_from_slice(bytes);
        Ok(u64::from_le_bytes(word))
    }
}

fn truncated() -> Broken {
    "pickle: the stream ends in the middle of a value".into()
}
