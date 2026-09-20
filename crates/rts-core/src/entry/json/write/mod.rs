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

mod plan;
mod shape;
mod to_json;
mod walk;

pub(super) use plan::Kept;
pub(super) use shape::indent_of;
pub(super) use to_json::HookKey;

use plan::{Member, Plan};
pub(super) use shape::{Shape, shape_of};
use to_json::to_json_of;

use super::super::{Context, with_current};
use super::hooks::Replacer;
use crate::text::Str;
use crate::value::Value;

/// What a value is, answered inside the caller's borrow and carried out of it.

/// The buffer, the indentation, and the set of cells currently being written.
pub(super) struct Writer {
    /// See [`super::out::Out`] for why this is not a `Vec<u16>`.
    out: super::out::Out,
    /// The cells on the path from the root to here.
    ///
    /// A vector and a linear scan rather than a set: a JSON document's depth is
    /// small, and a hash of a `u32` costs more than comparing the handful this
    /// ever holds.
    open: Vec<u32>,
    indent: Vec<u16>,
    /// What the second argument to `stringify` was, classified once before the
    /// walk started. See [`super::hooks::Replacer`].
    replacer: Replacer,
    /// See [`plain_properties`]. Indexed by depth.
    plans: Vec<Option<Plan>>,
}

impl Writer {
    pub(super) fn new(indent: Vec<u16>, replacer: Replacer, kept: Kept) -> Self {
        let (plans, mut open, out) = kept;
        // A walk that raised left its path behind; this one starts at the root.
        open.clear();
        Writer {
            out: super::out::Out::over(out),
            open,
            indent,
            replacer,
            plans,
        }
    }

    /// The text written so far.
    pub(super) fn finish(self) -> (Str, Kept) {
        let (text, out) = self.out.finish_keeping();
        (text, (self.plans, self.open, out))
    }

    /// What a walk that produced no text still has worth keeping.
    pub(super) fn abandon(self) -> Kept {
        self.finish().1
    }

    /// Writes one value, and answers whether it had a JSON form at all.
    ///
    /// The boolean is the whole `undefined`-versus-`"undefined"` distinction:
    /// the caller decides what an absence means, and it means different things
    /// in the three places one can occur — `null` in an array, a skipped member
    /// in an object, and `undefined` from `stringify` itself.
    ///
    /// `key` is the property key `toJSON` is passed, per the specification —
    /// the empty string at the root, the element's index in an array, the
    /// member's name in an object. It is a value rather than a `&Str` because
    /// that is what a call's argument is, and the empty-string root case has
    /// no `Str` lying around to borrow.
    ///
    /// What a member serialises as, once both hooks have had it.
    ///
    /// Separate from [`Writer::write`], and that separation is a correctness
    /// fix rather than tidiness. The object walk has to know whether a member
    /// has a JSON form *before* it writes the key, and it used to ask that of
    /// the raw property — so a `toJSON` or a replacer answering `undefined`
    /// produced `{"drop":}`, which is not JSON at all. Now one call answers
    /// what will be written, and both questions are asked of the same value.
    ///
    /// `holder` is the object the member was read from, which is what a
    /// function replacer is called with as its receiver — the synthetic
    /// `{"": value}` at the root, the array or the object below it.
    pub(super) fn hooked(&self, holder: u64, value: u64, key: HookKey) -> u64 {
        // `toJSON` first and the replacer second, which is the order
        // `SerializeJSONProperty` states: a replacer sees what the hook
        // answered, not what the property held.
        let value = to_json_of(value, key);
        match self.replacer {
            Replacer::Function(hook) => {
                let key = with_current(|context| key.value(context));
                super::hooks::replaced(hook, holder, key, value)
            }
            _ => value,
        }
    }

    /// Writes one value — already hooked — and answers whether it had a JSON
    /// form at all.
    pub(super) fn write(&mut self, value: u64, depth: usize) -> bool {
        // Rule 8: a hook may have raised, and a walk that carries on writes
        // members computed from an answer that never happened.
        if super::super::throw::in_flight() {
            return false;
        }
        let shape = with_current(|context| shape_of(context, value));
        self.write_shape(shape, value, depth)
    }

    /// The same, for a caller that has already classified.
    ///
    /// `plain` had to classify to answer rule 8's question — may this member be
    /// written at all — and then `write` classified again to decide how. One
    /// decision, carried.
    fn write_shape(&mut self, shape: Shape, value: u64, depth: usize) -> bool {
        match shape {
            Shape::Absent => return false,
            Shape::Big => {
                super::super::throw::type_error("Do not know how to serialize a BigInt");
                return false;
            }
            Shape::Null => self.ascii("null"),
            Shape::Bool(true) => self.ascii("true"),
            Shape::Bool(false) => self.ascii("false"),
            // `Infinity` and `NaN` have no JSON spelling, and the language
            // chose `null` over an error for them. The shortest round-tripping
            // decimal comes from the runtime's own conversion, so a number
            // printed here and one printed by `String(n)` cannot disagree.
            Shape::Number(number) => match number.is_finite() {
                // Straight off the stack: a number is text nobody keeps, so it
                // is never made into a string on the way to the buffer.
                true => self.out.bytes(crate::coerce::decimal_of(number).bytes()),
                false => self.ascii("null"),
            },
            Shape::Text(cell) => with_current(|context| {
                if let Some(text) = context.text_at(cell) {
                    self.quoted(text);
                }
            }),
            Shape::Array(cell) => self.array(cell, depth),
            Shape::Object(cell) => self.object(value, cell, depth),
        }
        true
    }

    /// A member's key, from the plan where it is narrow and from the interner
    /// where it is not.
    #[inline]
    fn label(&mut self, context: &Context, member: &Member) {
        if !member.label.is_empty() {
            return self.out.bytes(&member.label);
        }
        if let Some(text) = context.interner.text(member.key) {
            self.quoted(text);
        }
    }

    /// Whether nothing but this walk sees a member before it is written.
    ///
    /// A function replacer is called for EVERY member, primitive or not, so it
    /// is the one thing that rules the one-borrow path out. `toJSON` does not:
    /// the specification reads it off an Object or a BigInt and off nothing
    /// else, and [`is_primitive`] admits neither.
    #[inline]
    fn unobserved(&self) -> bool {
        !matches!(self.replacer, Replacer::Function(_))
    }

    /// Writes a primitive inside the caller's borrow, and answers whether it
    /// was one.
    ///
    /// # What this removes
    ///
    /// A member used to cost a borrow to read it, one for `toJSON` to discover
    /// it was not an object, one to classify it, one to write its key and one
    /// to classify it AGAIN inside `write` — five `RefCell` borrows and two
    /// thread-local reads each, to copy a number. Measured 2026-09-19 on
    /// `target/release/rts.exe`: 95 ns an array element and 230 a member, of
    /// which the digits are a handful.
    #[inline]
    fn direct(&mut self, context: &Context, value: u64) -> bool {
        // A double first, and without `shape_of`: that asks about wrappers and
        // bigints before it asks about numbers, which is the right order for a
        // value nobody has looked at and two lookups too many for this one.
        if let Some(number) = Value(value).numeric() {
            return self.write_shape(Shape::Number(number), value, 0);
        }
        if let Some(cell) = Value(value).as_slot() {
            return match context.text_at(cell) {
                Some(text) => {
                    self.quoted(text);
                    true
                }
                None => false,
            };
        }
        match shape_of(context, value) {
            shape @ (Shape::Null | Shape::Bool(_) | Shape::Number(_)) => self.write_shape(shape, value, 0),
            _ => false,
        }
    }

    /// Whether this cell may be descended into.
    ///
    /// A cycle is a `TypeError`, which is what the language says and what this
    /// answered `null` for until the discipline arrived. The reason it could not
    /// before was rule 8 from the other side: a raise is only safe once the
    /// walk that calls user code CHECKS for one, or the throw is left in flight
    /// and re-raised at an unrelated call site later. `write` checks now, the
    /// two loops break, and `stringify` answers `undefined` — so the raise has
    /// somewhere to land.
    ///
    /// Past the depth limit is still `null`, and stays that way: it is this
    /// crate's own limit protecting the Rust stack, not a rule of the language,
    /// and inventing a `TypeError` for it would report our ceiling as the
    /// program's mistake.
    fn enter(&mut self, cell: u32, depth: usize) -> bool {
        if self.open.contains(&cell) {
            super::super::throw::type_error("Converting circular structure to JSON");
            return false;
        }
        if depth >= super::DEPTH {
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
    #[inline]
    fn newline(&mut self, depth: usize) {
        if self.indent.is_empty() {
            return;
        }
        self.out.bytes(b"\n");
        for _ in 0..depth {
            for unit in &self.indent {
                self.out.unit(*unit);
            }
        }
    }

    /// Text this module wrote itself, which is ASCII by construction.
    #[inline]
    fn ascii(&mut self, text: &str) {
        self.out.bytes(text.as_bytes());
    }

    /// Text from the heap, as a JSON string literal.
    ///
    /// Only what the grammar forbids is escaped, plus one thing the grammar
    /// allows and the language does not: a LONE surrogate. A non-ASCII
    /// character goes through as itself rather than as `\uXXXX` — both are
    /// legal JSON and the answer is a JavaScript string, not a byte stream, so
    /// escaping would lengthen it for a transport question this layer does not
    /// have.
    ///
    /// The surrogate rule is ES2019's well-formed `JSON.stringify`, and it is
    /// not cosmetic: a lone surrogate written raw makes text that no UTF-8
    /// transport can carry, so the specification escapes exactly those and
    /// leaves matched pairs alone. Units are indexed rather than iterated
    /// because deciding whether a high surrogate is lone means looking at the
    /// next one.
    fn quoted(&mut self, text: &Str) {
        if let Some(bytes) = text.narrow() {
            self.out.bytes(b"\"");
            self.out.escaped(bytes);
            self.out.bytes(b"\"");
            return;
        }
        let units: Vec<u16> = text.units().collect();
        self.out.bytes(b"\"");
        for (at, unit) in units.iter().copied().enumerate() {
            let lone = match unit {
                0xd800..=0xdbff => !matches!(units.get(at + 1), Some(0xdc00..=0xdfff)),
                0xdc00..=0xdfff => !matches!(at.checked_sub(1).and_then(|before| units.get(before)), Some(0xd800..=0xdbff)),
                _ => false,
            };
            if lone {
                self.ascii("\\u");
                let digits = b"0123456789abcdef";
                for shift in [12, 8, 4, 0] {
                    self.out.bytes(&[digits[((unit >> shift) & 0xf) as usize]]);
                }
                continue;
            }
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
                    self.out.bytes(&[digits[(unit >> 4) as usize], digits[(unit & 0xf) as usize]]);
                }
                _ => self.out.unit(unit),
            }
        }
        self.out.bytes(b"\"");
    }
}

