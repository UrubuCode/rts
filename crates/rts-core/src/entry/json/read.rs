//! JSON text as a tree, with no heap anywhere in it.
//!
//! # Why a tree and not values as it goes
//!
//! Allocating while parsing is what every small JSON reader does, and it is
//! exactly wrong here: each allocation takes the context's borrow, and the
//! parser is recursive, so a borrow taken at one depth would be live across the
//! call that takes the next one. That is the panic this crate's whole entry
//! layer is arranged to make unreachable — and it would be reproducible only for
//! nested input, which is the worst kind of intermittent.
//!
//! So this file names no runtime type at all. It answers [`Node`], and the one
//! function that turns a `Node` into values lives in the parent, where it can be
//! read in a single screen and checked for exactly this.
//!
//! # Why the input is code units
//!
//! `😀` is a surrogate pair, and the natural way to build it is to
//! push each escape's four hex digits as the `u16` they spell and let the pair
//! be a pair by construction. Decoding to `char` would mean pairing them here —
//! a second implementation of something UTF-16 already does — and would have to
//! decide what a *lone* `\uD800` becomes, which the language says is a perfectly
//! good string.

/// A parsed document.
///
/// The variants are the grammar's, not the language's: there is no `undefined`
/// in JSON and no way to write one, which is why the enum has six arms where
/// the writer's [`super::write`] classification has seven.
pub(super) enum Node {
    Null,
    Bool(bool),
    Number(f64),
    /// A string token, ALREADY BUILT.
    ///
    /// It carried the raw `Vec<u16>` and `materialise` called
    /// `Str::from_utf16` on it — a second allocation and a walk of every unit
    /// to decide whether the narrow layout fits. Both were paid over text the
    /// input already holds contiguously, and the reader is looking straight at
    /// it.
    ///
    /// Built here instead, inside the borrow of the input, because the borrow
    /// is alive exactly here and gone by the time `materialise` runs.
    Text(Str),
    Array(Vec<Node>),
    /// Members in source order, which is what the object is built in — and so
    /// what enumeration answers afterwards.
    Object(Vec<(Str, Node)>),
}

use crate::text::Str;

/// The whole document, or `None` for anything that is not one.
///
/// One answer for every failure, deliberately: a `SyntaxError` carries a
/// position and a reason, and there is nothing here that can throw one — see
/// the module documentation in the parent. Inventing a richer failure type that
/// only ever collapses to `undefined` would be detail nothing reads.
/// Parses directly over a runtime string without copying the complete input.
pub(super) fn parse_text(text: &Str) -> Option<Node> {
    let mut reader = Reader { text, at: 0 };
    reader.spaces();
    let node = reader.value(0)?;
    reader.spaces();
    // Trailing content is a failure rather than a stop: `JSON.parse("1 2")`
    // must not answer 1, because a caller that got 1 has no way to learn the
    // rest of its input was discarded.
    match reader.at == reader.len() {
        true => Some(node),
        false => None,
    }
}

/// A cursor over borrowed runtime text.
struct Reader<'a> {
    text: &'a Str,
    at: usize,
}

impl Reader<'_> {
    fn len(&self) -> usize {
        self.text.len()
    }

    fn unit_at(&self, index: usize) -> Option<u16> {
        self.text.unit_at(index)
    }

    fn peek(&self) -> Option<u16> {
        self.unit_at(self.at)
    }

    /// Borrows an ASCII number from a narrow runtime string when possible.
    fn ascii_slice(&self, start: usize, end: usize) -> Option<&str> {
        let bytes = self.text.narrow()?.get(start..end)?;
        std::str::from_utf8(bytes).ok()
    }

    /// Whether the next unit is this ASCII character, consuming it if so.
    fn eat(&mut self, character: u8) -> bool {
        if self.peek() == Some(u16::from(character)) {
            self.at += 1;
            return true;
        }
        false
    }

    /// The four characters JSON calls whitespace — and only those. A vertical
    /// tab or a form feed is legal JavaScript spacing and illegal JSON, and
    /// accepting them would make this parser answer for documents no other
    /// engine accepts.
    fn spaces(&mut self) {
        while matches!(self.peek(), Some(0x20 | 0x09 | 0x0a | 0x0d)) {
            self.at += 1;
        }
    }

    /// One value, with whatever follows it left for the caller.
    fn value(&mut self, depth: usize) -> Option<Node> {
        if depth >= super::DEPTH {
            return None;
        }
        match self.peek()? {
            0x7b => self.object(depth),
            0x5b => self.array(depth),
            0x22 => {
                self.at += 1;
                self.string().map(Node::Text)
            }
            0x74 => self.word("true", Node::Bool(true)),
            0x66 => self.word("false", Node::Bool(false)),
            0x6e => self.word("null", Node::Null),
            _ => self.number(),
        }
    }

    /// One of the three literals, matched whole.
    fn word(&mut self, text: &str, node: Node) -> Option<Node> {
        for byte in text.bytes() {
            if !self.eat(byte) {
                return None;
            }
        }
        Some(node)
    }

    /// `[…]`, the opening bracket not yet consumed.
    fn array(&mut self, depth: usize) -> Option<Node> {
        self.at += 1;
        let mut items = Vec::new();
        self.spaces();
        if self.eat(b']') {
            return Some(Node::Array(items));
        }
        loop {
            self.spaces();
            items.push(self.value(depth + 1)?);
            self.spaces();
            if self.eat(b',') {
                continue;
            }
            // A comma with nothing after it lands back at `value`, which
            // refuses `]` — so `[1,]` fails here rather than being quietly
            // accepted the way an array literal in the language would be.
            return self.eat(b']').then_some(Node::Array(items));
        }
    }

    /// `{…}`, the opening brace not yet consumed.
    fn object(&mut self, depth: usize) -> Option<Node> {
        self.at += 1;
        let mut members = Vec::new();
        self.spaces();
        if self.eat(b'}') {
            return Some(Node::Object(members));
        }
        loop {
            self.spaces();
            // A key is a string and nothing else. JSON has no bare identifiers,
            // and accepting one here is the single most common way a permissive
            // parser stops agreeing with the format it claims to read.
            if !self.eat(b'"') {
                return None;
            }
            let key = self.string()?;
            self.spaces();
            if !self.eat(b':') {
                return None;
            }
            self.spaces();
            members.push((key, self.value(depth + 1)?));
            self.spaces();
            if self.eat(b',') {
                continue;
            }
            return self.eat(b'}').then_some(Node::Object(members));
        }
    }

    /// The rest of a string, the opening quote already consumed.
    ///
    /// Two paths, and the first one allocates once instead of twice.
    ///
    /// A token with no escape is a contiguous run of the INPUT, and the reader
    /// is holding the input. Where that input is narrow — every byte below 256,
    /// which is what a Latin-1 representation means — the slice can be handed
    /// straight to `Str::from_latin1`, whose own documentation says it needs no
    /// scan because a slice of a narrow string is narrow by construction.
    ///
    /// It was: push one `u16` at a time into a growing `Vec`, then walk every
    /// unit again in `Str::from_utf16` to pick a layout, then allocate a second
    /// buffer. Two allocations and three passes over text that was already
    /// there. Measured, min of 9 over 200 K iterations: four two-character
    /// string values cost 505 ns more than four numbers in the same document.
    ///
    /// The escaped path keeps the `Vec`, because that is the one case where the
    /// units genuinely have to be assembled rather than pointed at.
    fn string(&mut self) -> Option<Str> {
        let from = self.at;
        if let Some(bytes) = self.text.narrow() {
            let mut at = self.at;
            loop {
                let unit = *bytes.get(at)?;
                match unit {
                    0x22 => {
                        self.at = at + 1;
                        return Some(Str::from_latin1(&bytes[from..at]));
                    }
                    // An escape ends the fast path and nothing else does. The
                    // scan restarts from the opening quote rather than trying to
                    // splice, because a token with one escape is rare and a
                    // second code path for it would be the third way this file
                    // builds a string.
                    0x5c => break,
                    0x00..=0x1f => return None,
                    _ => at += 1,
                }
            }
        }
        let mut units = Vec::new();
        self.at = from;
        loop {
            let unit = self.peek()?;
            self.at += 1;
            match unit {
                0x22 => return Some(Str::from_utf16(&units)),
                0x5c => units.push(self.escape()?),
                // A raw control character is not allowed inside a JSON string,
                // and this is the check that makes a truncated document fail
                // instead of absorbing the newline that ended it.
                0x00..=0x1f => return None,
                _ => units.push(unit),
            }
        }
    }

    /// What one `\` introduces — a code unit, never a character.
    fn escape(&mut self) -> Option<u16> {
        let unit = self.peek()?;
        self.at += 1;
        match unit {
            0x22 | 0x5c | 0x2f => Some(unit),
            0x62 => Some(0x08),
            0x66 => Some(0x0c),
            0x6e => Some(0x0a),
            0x72 => Some(0x0d),
            0x74 => Some(0x09),
            0x75 => {
                let mut value = 0u16;
                for _ in 0..4 {
                    let digit = hex(self.peek()?)?;
                    self.at += 1;
                    value = (value << 4) | u16::from(digit);
                }
                // Pushed as it is, high surrogate or not: two consecutive
                // escapes that form a pair are a pair in the buffer already, and
                // a lone one is a string the language permits.
                Some(value)
            }
            _ => None,
        }
    }

    /// A number, matched against JSON's grammar rather than Rust's.
    ///
    /// The difference matters in both directions. JSON rejects `+1`, `.5`, `1.`,
    /// `01`, `Infinity` and `NaN`, all of which Rust's `f64` parser accepts or
    /// nearly accepts; and the value of what it does accept is a double, which
    /// is what Rust's parser is for. So the scan decides *whether*, and `parse`
    /// decides *what*.
    fn number(&mut self) -> Option<Node> {
        let start = self.at;
        self.eat(b'-');
        match self.digit()? {
            // A leading zero stands alone. `01` is two tokens, and a parser
            // that read it as 1 would answer for a document every other
            // implementation rejects.
            b'0' => self.at += 1,
            _ => self.digits(),
        }
        if self.eat(b'.') {
            self.digit()?;
            self.digits();
        }
        if matches!(self.peek(), Some(0x65 | 0x45)) {
            self.at += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            self.digit()?;
            self.digits();
        }
        // Every unit in the range is an ASCII character the scan above admitted,
        // so this cannot lose anything. Narrow runtime strings can parse directly
        // from their borrowed byte slice; wide strings retain the allocation only
        // for the representation conversion required by `f64::from_str`.
        if let Some(text) = self.ascii_slice(start, self.at) {
            return parse_number_text(text).map(Node::Number);
        }
        let mut text = String::with_capacity(self.at - start);
        for index in start..self.at {
            text.push(self.unit_at(index)? as u8 as char);
        }
        parse_number_text(&text).map(Node::Number)
    }

    /// The next unit as an ASCII digit, without consuming it.
    fn digit(&self) -> Option<u8> {
        let unit = self.peek()?;
        (0x30..=0x39).contains(&unit).then_some(unit as u8)
    }

    /// Consumes the run of digits at the cursor, however long — including none.
    fn digits(&mut self) {
        while self.digit().is_some() {
            self.at += 1;
        }
    }
}

/// Parses an already grammar-checked JSON number.
fn parse_number_text(text: &str) -> Option<f64> {
    // Preserve the sign of JSON's `-0`; integer parsing alone would erase it.
    if text == "-0" {
        return Some(-0.0);
    }
    // An integer within either signed or unsigned 64-bit range can be converted
    // directly to the same IEEE-754 value as the standard parser, without its
    // decimal scanner. Larger integers and all fractions/exponents keep the
    // standards-compliant fallback.
    if !text.as_bytes().iter().any(|unit| matches!(unit, b'.' | b'e' | b'E')) {
        if let Ok(integer) = text.parse::<u64>() {
            return Some(integer as f64);
        }
        if let Ok(integer) = text.parse::<i64>() {
            return Some(integer as f64);
        }
    }
    text.parse().ok()
}

/// The value of one hexadecimal digit.
fn hex(unit: u16) -> Option<u8> {
    match unit {
        0x30..=0x39 => Some((unit - 0x30) as u8),
        0x41..=0x46 => Some((unit - 0x41 + 10) as u8),
        0x61..=0x66 => Some((unit - 0x61 + 10) as u8),
        _ => None,
    }
}
