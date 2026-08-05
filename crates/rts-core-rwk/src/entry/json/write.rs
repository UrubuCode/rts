//! A value as JSON text.
//!
//! # Why the output is code units and not a Rust `String`
//!
//! A JavaScript string holds anything a `u16` sequence can hold, lone
//! surrogates included, and `JSON.stringify` is required to copy one through
//! unchanged. Building the answer as UTF-8 would mean deciding what to do with
//! a half of a pair on the way in and again on the way out — two lossy steps
//! around text that was already in the right shape. So the buffer is `Vec<u16>`
//! from the first character to the last, and `Str::from_utf16` narrows it once
//! at the end.
//!
//! # Why the walk classifies before it descends
//!
//! Every question about a value — is it text, is it an array, is it callable —
//! is a heap question, and the answers must be taken in one borrow and carried
//! out of it. [`Shape`] is that carrying: after it, the writer holds no borrow
//! and is free to call `own_keys` and `get_indexed`, which take their own.

use super::super::{Context, with_current};
use crate::text::Str;
use crate::value::{Kind, Value};

/// What a value is, as far as JSON is concerned.
///
/// Five kinds and an absence, where the language has more: a symbol, a
/// `BigInt` and a wrapper object each have their own rule and none of them
/// exists in this engine yet. When one does it arrives here as a variant, which
/// is why this is an enum rather than a chain of tests at the call site.
enum Shape {
    Null,
    Bool(bool),
    Number(f64),
    Text(Str),
    /// An array, by the cell that identifies it.
    Array(u32),
    /// Anything else with properties.
    Object(u32),
    /// `undefined`, a function, or anything with no JSON form.
    Absent,
}

/// What a value is, answered inside the caller's borrow and carried out of it.
fn shape_of(context: &Context, value: u64) -> Shape {
    let value = Value(value);
    if let Some(number) = value.numeric() {
        return Shape::Number(number);
    }
    if let Some(flag) = value.as_bool() {
        return Shape::Bool(flag);
    }
    if let Some(cell) = value.as_slot() {
        if let Some(text) = context.text_at(cell) {
            return Shape::Text(text.clone());
        }
        // Asked before "does it have elements", because a callable is an object
        // too and the language says a function has no JSON form wherever it
        // appears. Getting the order wrong writes a function's properties.
        if context.callable_at(cell).is_some() {
            return Shape::Absent;
        }
        if context.elements_at(cell).is_some() {
            return Shape::Array(cell);
        }
        return Shape::Object(cell);
    }
    match value.kind() {
        Kind::Singleton(number) if number == context.singletons.null => Shape::Null,
        // `undefined` and any singleton this crate does not name. Answering
        // `null` for an unknown one would invent data; absence is recoverable.
        _ => Shape::Absent,
    }
}

/// One level of indentation, from the third argument to `stringify`.
///
/// A number of spaces or a string, both capped at ten, which is the
/// specification's own cap — and worth keeping rather than simplifying away,
/// because it is what stops `JSON.stringify(o, null, 1e9)` from asking for a
/// gigabyte of spaces per line.
pub(super) fn indent_of(space: u64) -> Vec<u16> {
    with_current(|context| match shape_of(context, space) {
        Shape::Number(count) => {
            let count = count.floor().clamp(0.0, 10.0) as usize;
            vec![b' ' as u16; count]
        }
        Shape::Text(text) => text.units().take(10).collect(),
        _ => Vec::new(),
    })
}

/// The buffer, the indentation, and the set of cells currently being written.
pub(super) struct Writer {
    out: Vec<u16>,
    /// The cells on the path from the root to here.
    ///
    /// A vector and a linear scan rather than a set: a JSON document's depth is
    /// small, and a hash of a `u32` costs more than comparing the handful this
    /// ever holds.
    open: Vec<u32>,
    indent: Vec<u16>,
}

impl Writer {
    pub(super) fn new(indent: Vec<u16>) -> Self {
        Writer {
            out: Vec::new(),
            open: Vec::new(),
            indent,
        }
    }

    /// The text written so far.
    pub(super) fn finish(self) -> Vec<u16> {
        self.out
    }

    /// Writes one value, and answers whether it had a JSON form at all.
    ///
    /// The boolean is the whole `undefined`-versus-`"undefined"` distinction:
    /// the caller decides what an absence means, and it means different things
    /// in the three places one can occur — `null` in an array, a skipped member
    /// in an object, and `undefined` from `stringify` itself.
    pub(super) fn write(&mut self, value: u64, depth: usize) -> bool {
        match with_current(|context| shape_of(context, value)) {
            Shape::Absent => return false,
            Shape::Null => self.ascii("null"),
            Shape::Bool(true) => self.ascii("true"),
            Shape::Bool(false) => self.ascii("false"),
            // `Infinity` and `NaN` have no JSON spelling, and the language
            // chose `null` over an error for them. The shortest round-tripping
            // decimal comes from the runtime's own conversion, so a number
            // printed here and one printed by `String(n)` cannot disagree.
            Shape::Number(number) => match number.is_finite() {
                true => self.text(&crate::coerce::number_to_string(number)),
                false => self.ascii("null"),
            },
            Shape::Text(text) => self.quoted(&text),
            Shape::Array(cell) => self.array(cell, depth),
            Shape::Object(cell) => self.object(value, cell, depth),
        }
        true
    }

    /// `[…]`.
    fn array(&mut self, cell: u32, depth: usize) {
        if !self.enter(cell, depth) {
            return self.ascii("null");
        }
        // Copied out of the borrow rather than iterated inside one, because
        // each element's own serialisation calls back into the runtime.
        let elements = with_current(|context| context.elements_at(cell).cloned().unwrap_or_default());
        self.ascii("[");
        for (at, element) in elements.iter().enumerate() {
            if at > 0 {
                self.ascii(",");
            }
            self.newline(depth + 1);
            // A hole, an `undefined` and a function are each `null` here, where
            // in an object they are skipped. The asymmetry is the language's
            // and it has a reason: an array's members are addressed by
            // position, so dropping one renumbers every one after it.
            if !self.write(*element, depth + 1) {
                self.ascii("null");
            }
        }
        if !elements.is_empty() {
            self.newline(depth);
        }
        self.ascii("]");
        self.leave();
    }

    /// `{…}`.
    fn object(&mut self, value: u64, cell: u32, depth: usize) {
        if !self.enter(cell, depth) {
            return self.ascii("null");
        }
        // The runtime's own enumeration, which is what `Object.keys` and
        // `for-in` walk. A second walk of the layout here would be a second
        // answer to "what order", and the two would drift the first time one
        // was fixed.
        let names = super::super::array::own_keys(value);
        let names = with_current(|context| {
            Value(names)
                .as_slot()
                .and_then(|cell| context.elements_at(cell).cloned())
                .unwrap_or_default()
        });

        self.ascii("{");
        let mut written = false;
        for name in names {
            // Through the ordinary read, so a member that is an accessor runs
            // its getter — which is what `stringify` observably does, and what
            // reading the slot directly would have skipped.
            let held = super::super::computed::get_indexed(value, name);
            let key = with_current(|context| super::super::text::to_text(context, Value(name)));
            let Some(key) = key else {
                continue;
            };
            // Classified once here to decide whether the key is written at all,
            // and again inside `write`. That is one extra borrow per member and
            // it buys the separator staying correct: a member skipped after its
            // comma was emitted is a trailing comma, which is not JSON.
            if with_current(|context| matches!(shape_of(context, held), Shape::Absent)) {
                continue;
            }
            if written {
                self.ascii(",");
            }
            written = true;
            self.newline(depth + 1);
            self.quoted(&key);
            self.ascii(":");
            if !self.indent.is_empty() {
                self.ascii(" ");
            }
            self.write(held, depth + 1);
        }
        if written {
            self.newline(depth);
        }
        self.ascii("}");
        self.leave();
    }

    /// Whether this cell may be descended into.
    ///
    /// False for one already on the path, which is a cycle, and for one past
    /// the depth limit. Both answer `null` at the call site; the module
    /// documentation records that the specification throws for the first and
    /// why this cannot yet.
    fn enter(&mut self, cell: u32, depth: usize) -> bool {
        if depth >= super::DEPTH || self.open.contains(&cell) {
            return false;
        }
        self.open.push(cell);
        true
    }

    fn leave(&mut self) {
        self.open.pop();
    }

    /// A newline and the indentation for a depth — nothing at all when
    /// `stringify` was asked for the compact form, which is the common call.
    fn newline(&mut self, depth: usize) {
        if self.indent.is_empty() {
            return;
        }
        self.out.push(b'\n' as u16);
        for _ in 0..depth {
            self.out.extend_from_slice(&self.indent);
        }
    }

    /// Text this module wrote itself, which is ASCII by construction.
    fn ascii(&mut self, text: &str) {
        self.out.extend(text.bytes().map(u16::from));
    }

    /// Text from the heap, unquoted — a number's decimal, and nothing else.
    fn text(&mut self, text: &Str) {
        self.out.extend(text.units());
    }

    /// Text from the heap, as a JSON string literal.
    ///
    /// Only what the grammar forbids is escaped. A non-ASCII character goes
    /// through as itself rather than as `\uXXXX`: both are legal JSON and the
    /// answer is a JavaScript string, not a byte stream, so escaping would
    /// lengthen it for a transport question this layer does not have.
    fn quoted(&mut self, text: &Str) {
        self.out.push(b'"' as u16);
        for unit in text.units() {
            match unit {
                0x22 => self.ascii("\\\""),
                0x5c => self.ascii("\\\\"),
                0x08 => self.ascii("\\b"),
                0x0c => self.ascii("\\f"),
                0x0a => self.ascii("\\n"),
                0x0d => self.ascii("\\r"),
                0x09 => self.ascii("\\t"),
                // Every other control character has no short form and must not
                // appear raw inside a string.
                0x00..=0x1f => {
                    self.ascii("\\u00");
                    let digits = b"0123456789abcdef";
                    self.out.push(u16::from(digits[(unit >> 4) as usize]));
                    self.out.push(u16::from(digits[(unit & 0xf) as usize]));
                }
                _ => self.out.push(unit),
            }
        }
        self.out.push(b'"' as u16);
    }
}
