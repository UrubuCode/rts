//! node:string_decoder — the per-encoding incremental decoder state machine, a
//! faithful port of Node's `lib/string_decoder.js`. Holds up to 4 pending bytes
//! of a multi-byte/multi-unit character that straddled a chunk boundary; the
//! next `write`/`end` completes it. Pure byte→`String` computation.

/// The encodings `StringDecoder` accepts (normalized).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Enc {
    Utf8,
    Utf16le,
    Base64,
    Base64url,
    Latin1,
    Ascii,
    Hex,
}

impl Enc {
    /// Normalize a Node encoding name, or `None` if unrecognized.
    pub fn parse(name: &str) -> Option<Enc> {
        match name.to_lowercase().as_str() {
            "utf8" | "utf-8" => Some(Enc::Utf8),
            "utf16le" | "utf-16le" | "ucs2" | "ucs-2" => Some(Enc::Utf16le),
            "base64" => Some(Enc::Base64),
            "base64url" => Some(Enc::Base64url),
            "latin1" | "binary" => Some(Enc::Latin1),
            "ascii" => Some(Enc::Ascii),
            "hex" => Some(Enc::Hex),
            _ => None,
        }
    }

    /// The canonical name (what `decoder.encoding` reports).
    pub fn name(self) -> &'static str {
        match self {
            Enc::Utf8 => "utf8",
            Enc::Utf16le => "utf16le",
            Enc::Base64 => "base64",
            Enc::Base64url => "base64url",
            Enc::Latin1 => "latin1",
            Enc::Ascii => "ascii",
            Enc::Hex => "hex",
        }
    }
}

/// Incremental decoder state (`lastChar`/`lastNeed`/`lastTotal` in Node).
pub struct Decoder {
    pub enc: Enc,
    pub last_char: [u8; 4],
    pub last_need: usize,
    pub last_total: usize,
}

impl Decoder {
    pub fn new(enc: Enc) -> Decoder {
        Decoder { enc, last_char: [0; 4], last_need: 0, last_total: 0 }
    }

    /// `decoder.write(buf)`.
    pub fn write(&mut self, buf: &[u8]) -> String {
        if buf.is_empty() {
            return String::new();
        }
        let mut r = String::new();
        let mut i = 0usize;
        if self.last_need != 0 {
            match self.fill_last(buf) {
                Some(s) => {
                    r = s;
                    i = self.last_need;
                    self.last_need = 0;
                }
                None => return String::new(),
            }
        }
        if i < buf.len() {
            r.push_str(&self.text(buf, i));
        }
        r
    }

    /// `decoder.end(buf?)` — flush; incomplete pending → substitution char(s).
    pub fn end(&mut self, buf: Option<&[u8]>) -> String {
        let mut r = match buf {
            Some(b) if !b.is_empty() => self.write(b),
            _ => String::new(),
        };
        if self.last_need != 0 {
            match self.enc {
                Enc::Utf8 => r.push('\u{FFFD}'),
                Enc::Utf16le => {
                    let end = self.last_total - self.last_need;
                    r.push_str(&to_string(Enc::Utf16le, &self.last_char[..end]));
                }
                Enc::Base64 | Enc::Base64url => {
                    r.push_str(&to_string(self.enc, &self.last_char[..3 - self.last_need]));
                }
                _ => {}
            }
        }
        self.last_need = 0;
        self.last_total = 0;
        r
    }

    /// The base `fillLast` (utf16le/base64) with the utf8 override folded in:
    /// complete the pending char from the head of `buf`. `Some(s)` when the
    /// char completed; `None` when `buf` was consumed without completing.
    fn fill_last(&mut self, buf: &[u8]) -> Option<String> {
        if self.enc == Enc::Utf8 {
            // utf8 validates the continuation bytes (invalid → single U+FFFD).
            let p = self.last_total - self.last_need;
            if let Some(bad) = self.utf8_check_extra(buf, p) {
                return Some(bad);
            }
        }
        let offset = self.last_total - self.last_need;
        if self.last_need <= buf.len() {
            self.last_char[offset..offset + self.last_need]
                .copy_from_slice(&buf[..self.last_need]);
            return Some(to_string(self.enc, &self.last_char[..self.last_total]));
        }
        self.last_char[offset..offset + buf.len()].copy_from_slice(buf);
        self.last_need -= buf.len();
        None
    }

    /// utf8 continuation-byte validation (Node `utf8CheckExtraBytes`). Returns
    /// `Some("\u{FFFD}")` and adjusts `last_need` when a byte is not `10xxxxxx`.
    fn utf8_check_extra(&mut self, buf: &[u8], _p: usize) -> Option<String> {
        if buf.is_empty() {
            return None;
        }
        if buf[0] & 0xC0 != 0x80 {
            self.last_need = 0;
            return Some("\u{FFFD}".to_string());
        }
        if self.last_need > 1 && buf.len() > 1 {
            if buf[1] & 0xC0 != 0x80 {
                self.last_need = 1;
                return Some("\u{FFFD}".to_string());
            }
            if self.last_need > 2 && buf.len() > 2 && buf[2] & 0xC0 != 0x80 {
                self.last_need = 2;
                return Some("\u{FFFD}".to_string());
            }
        }
        None
    }

    /// `this.text(buf, i)` — decode the completable prefix, saving any trailing
    /// incomplete bytes into `last_char`.
    fn text(&mut self, buf: &[u8], i: usize) -> String {
        match self.enc {
            Enc::Utf8 => self.utf8_text(buf, i),
            Enc::Utf16le => self.utf16_text(buf, i),
            Enc::Base64 | Enc::Base64url => self.base64_text(buf, i),
            Enc::Latin1 | Enc::Ascii | Enc::Hex => to_string(self.enc, &buf[i..]),
        }
    }

    fn utf8_text(&mut self, buf: &[u8], i: usize) -> String {
        let total = self.utf8_check_incomplete(buf, i);
        if self.last_need == 0 {
            return to_string(Enc::Utf8, &buf[i..]);
        }
        self.last_total = total;
        let end = buf.len() - (total - self.last_need);
        self.last_char[..buf.len() - end].copy_from_slice(&buf[end..]);
        to_string(Enc::Utf8, &buf[i..end])
    }

    /// Node `utf8CheckIncomplete` — sets `last_need`, returns the total length
    /// of the trailing incomplete sequence (0 if the tail is complete).
    fn utf8_check_incomplete(&mut self, buf: &[u8], i: usize) -> usize {
        let mut j = buf.len() as isize - 1;
        if j < i as isize {
            return 0;
        }
        let mut nb = utf8_check_byte(buf[j as usize]);
        if nb >= 0 {
            if nb > 0 {
                self.last_need = (nb - 1) as usize;
            }
            return nb as usize;
        }
        j -= 1;
        if j < i as isize || nb == -2 {
            return 0;
        }
        nb = utf8_check_byte(buf[j as usize]);
        if nb >= 0 {
            if nb > 0 {
                self.last_need = (nb - 2) as usize;
            }
            return nb as usize;
        }
        j -= 1;
        if j < i as isize || nb == -2 {
            return 0;
        }
        nb = utf8_check_byte(buf[j as usize]);
        if nb >= 0 {
            if nb > 0 {
                if nb == 2 {
                    nb = 0;
                } else {
                    self.last_need = (nb - 3) as usize;
                }
            }
            return nb as usize;
        }
        0
    }

    fn utf16_text(&mut self, buf: &[u8], i: usize) -> String {
        let len = buf.len();
        if (len - i) % 2 == 0 {
            // Inspect the RAW trailing code unit (decoding first would let a lone
            // high surrogate collapse to U+FFFD before we can hold it back).
            let last_unit = u16::from_le_bytes([buf[len - 2], buf[len - 1]]);
            if (0xD800..=0xDBFF).contains(&last_unit) {
                self.last_need = 2;
                self.last_total = 4;
                self.last_char[0] = buf[len - 2];
                self.last_char[1] = buf[len - 1];
                return to_string(Enc::Utf16le, &buf[i..len - 2]);
            }
            return to_string(Enc::Utf16le, &buf[i..]);
        }
        self.last_need = 1;
        self.last_total = 2;
        self.last_char[0] = buf[len - 1];
        to_string(Enc::Utf16le, &buf[i..len - 1])
    }

    fn base64_text(&mut self, buf: &[u8], i: usize) -> String {
        let n = (buf.len() - i) % 3;
        if n == 0 {
            return to_string(self.enc, &buf[i..]);
        }
        self.last_need = 3 - n;
        self.last_total = 3;
        if n == 1 {
            self.last_char[0] = buf[buf.len() - 1];
        } else {
            self.last_char[0] = buf[buf.len() - 2];
            self.last_char[1] = buf[buf.len() - 1];
        }
        to_string(self.enc, &buf[i..buf.len() - n])
    }
}

/// Node `utf8CheckByte`: expected byte-length of a UTF-8 sequence starting with
/// `byte` (0 = ASCII), or `-1`/`-2` for a continuation/invalid lead byte.
fn utf8_check_byte(byte: u8) -> isize {
    if byte <= 0x7F {
        0
    } else if byte >> 5 == 0x06 {
        2
    } else if byte >> 4 == 0x0E {
        3
    } else if byte >> 3 == 0x1E {
        4
    } else if byte >> 6 == 0x02 {
        -1
    } else {
        -2
    }
}

/// `buf.toString(encoding)` for a fully-formed byte slice.
pub fn to_string(enc: Enc, bytes: &[u8]) -> String {
    match enc {
        Enc::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Enc::Latin1 => bytes.iter().map(|&b| b as char).collect(),
        Enc::Ascii => bytes.iter().map(|&b| (b & 0x7F) as char).collect(),
        Enc::Hex => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        Enc::Utf16le => {
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        }
        Enc::Base64 => base64_encode(bytes, false),
        Enc::Base64url => base64_encode(bytes, true),
    }
}

/// Standard / URL-safe base64 encode (no external crate).
fn base64_encode(data: &[u8], url_safe: bool) -> String {
    const STD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let alpha = if url_safe { URL } else { STD };
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(alpha[(n >> 18 & 0x3F) as usize] as char);
        out.push(alpha[(n >> 12 & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(alpha[(n >> 6 & 0x3F) as usize] as char);
        } else if !url_safe {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(alpha[(n & 0x3F) as usize] as char);
        } else if !url_safe {
            out.push('=');
        }
    }
    out
}
