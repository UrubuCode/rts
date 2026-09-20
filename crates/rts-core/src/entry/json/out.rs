//! The text `stringify` is writing, in whichever layout it still fits.
//!
//! # Why not the `Vec<u16>` this replaced
//!
//! The header of `write.rs` argues for code units over UTF-8, and that argument
//! stands: a lone surrogate must survive. What it does not argue for is sixteen
//! bits per unit from the first character. A `Str` has two layouts, nearly every
//! document fits the narrow one, and the wide buffer paid for that twice — a
//! `u16` pushed per input byte on the way in, then `Str::from_utf16` walking
//! every unit again to discover they were all below 256 and copying them into
//! the bytes they started as. Measured 2026-09-19, `target/release/rts.exe`: a
//! 1000-character string cost 2 561 ns to quote, about 2.5 ns a character, for
//! what is a `memcpy` when nothing needs an escape.
//!
//! So this starts narrow and widens ONCE, at the first unit that does not fit —
//! the same late decision `Str` itself makes, moved to where the units arrive.

use crate::text::Str;

/// Narrow until a unit says otherwise.
pub(super) enum Out {
    /// Every unit so far is below 256.
    Narrow(Vec<u8>),
    /// At least one was not, and everything before it was widened once.
    Wide(Vec<u16>),
}

impl Out {
    pub(super) fn new() -> Self {
        Out::Narrow(Vec::new())
    }

    /// The same, over a buffer kept from the walk before — see
    /// [`super::Scratch`]. Cleared, and its capacity is the point.
    pub(super) fn over(mut held: Vec<u8>) -> Self {
        held.clear();
        Out::Narrow(held)
    }

    /// The finished text AND the buffer it was written in, for the next walk.
    ///
    /// One exact-size copy instead of handing the buffer to the string: a
    /// buffer that grew by doubling is up to twice the text, and a string keeps
    /// what it is given for as long as the program keeps the string. A walk
    /// that widened has no narrow buffer to give back.
    pub(super) fn finish_keeping(self) -> (Str, Vec<u8>) {
        match self {
            Out::Narrow(held) => (Str::from_latin1(&held), held),
            Out::Wide(held) => (Str::owning_utf16(held), Vec::new()),
        }
    }

    /// Bytes that are each one code unit — ASCII this module wrote itself, or a
    /// run of a narrow string that needed no escape.
    pub(super) fn bytes(&mut self, bytes: &[u8]) {
        match self {
            Out::Narrow(held) => held.extend_from_slice(bytes),
            Out::Wide(held) => held.extend(bytes.iter().copied().map(u16::from)),
        }
    }

    /// One code unit, of any width.
    pub(super) fn unit(&mut self, unit: u16) {
        match self {
            Out::Narrow(held) => match u8::try_from(unit) {
                Ok(byte) => held.push(byte),
                Err(_) => {
                    let mut wide: Vec<u16> = held.iter().copied().map(u16::from).collect();
                    wide.push(unit);
                    *self = Out::Wide(wide);
                }
            },
            Out::Wide(held) => held.push(unit),
        }
    }

    /// A narrow string's body with JSON's escapes applied, copied in RUNS.
    ///
    /// Almost every byte of almost every string needs no escape, so the unit of
    /// work is the run between two that do — one `extend_from_slice` — rather
    /// than the byte. It was a `match` and a push per byte.
    ///
    /// Narrow input only, which is why no surrogate rule appears here: a lone
    /// surrogate is a unit above 255 and cannot be in these bytes.
    pub(super) fn escaped(&mut self, bytes: &[u8]) {
        let mut from = 0;
        for (at, &unit) in bytes.iter().enumerate() {
            if unit >= 0x20 && unit != b'"' && unit != b'\\' {
                continue;
            }
            self.bytes(&bytes[from..at]);
            from = at + 1;
            match unit {
                b'"' => self.bytes(b"\\\""),
                b'\\' => self.bytes(b"\\\\"),
                0x08 => self.bytes(b"\\b"),
                0x0c => self.bytes(b"\\f"),
                b'\n' => self.bytes(b"\\n"),
                b'\r' => self.bytes(b"\\r"),
                b'\t' => self.bytes(b"\\t"),
                // Every other control character has no short form.
                _ => {
                    let digits = b"0123456789abcdef";
                    self.bytes(b"\\u00");
                    self.bytes(&[digits[(unit >> 4) as usize], digits[(unit & 0xf) as usize]]);
                }
            }
        }
        self.bytes(&bytes[from..]);
    }

    /// The bytes of a buffer that never widened — a key's label, built once.
    pub(super) fn narrow(&self) -> &[u8] {
        match self {
            Out::Narrow(held) => held,
            Out::Wide(_) => &[],
        }
    }

}
