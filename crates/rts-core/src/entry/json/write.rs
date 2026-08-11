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
/// Five kinds and an absence, where the language has more: a symbol and a
/// `BigInt` each have their own rule and neither exists in this engine yet.
/// When one does it arrives here as a variant, which is why this is an enum
/// rather than a chain of tests at the call site.
///
/// A wrapper object is NOT one of them, and deliberately: the specification
/// says `SerializeJSONProperty` replaces `new Number(5)` by its
/// `[[NumberData]]` before it classifies anything, so it arrives here already
/// as `Number(5.0)`. A variant would be a second place deciding what a wrapper
/// serialises as, and the first place is where `valueOf` reads it from.
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
    // The wrapper's primitive, before anything else is asked. Without it a
    // `new Number(5)` reached `Shape::Object` and serialised as `{}` — the
    // object has no own properties, so the output was well-formed JSON that had
    // silently dropped the value. `Object(5)` is the same object by another
    // spelling and needs the same substitution, which is why this is here rather
    // than in the `Number` class.
    let value = Value(super::super::primitive_proto::unwrap(context, value));
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
    ///
    /// `key` is the property key `toJSON` is passed, per the specification —
    /// the empty string at the root, the element's index in an array, the
    /// member's name in an object. It is a value rather than a `&Str` because
    /// that is what a call's argument is, and the empty-string root case has
    /// no `Str` lying around to borrow.
    pub(super) fn write(&mut self, value: u64, key: HookKey, depth: usize) -> bool {
        let value = to_json_of(value, key);
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
            // The key `toJSON` sees for an array member is its index, ToString'd
            // — `[9].toJSON` is called with `"0"`, never with the number 9.
            let key = with_current(|context| {
                context
                    .intern_value(crate::coerce::number_to_string(at as f64))
                    .bits()
            });
            // A hole, an `undefined` and a function are each `null` here, where
            // in an object they are skipped. The asymmetry is the language's
            // and it has a reason: an array's members are addressed by
            // position, so dropping one renumbers every one after it.
            // The key `toJSON` sees for an array member is its index, ToString'd
            // -- `[9].toJSON` is called with `"0"`, never with the number 9. Built
            // only if a hook is actually reached: it is a `number_to_string` and
            // an ALLOCATION, and it was paid per element of every array ever
            // serialised, for a hook almost no value has.
            if !self.write(*element, HookKey::Index(at), depth + 1) {
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
            // `name` is already the string key, straight from `own_keys` — see
            // its comment above — so this is the same value `toJSON` must see,
            // with no second conversion to disagree with the first.
            self.write(held, HookKey::Given(name), depth + 1);
        }
        if written {
            self.newline(depth);
        }
        self.ascii("}");
        self.leave();
    }

    /// Whether this cell may be descended into.
    ///
    /// False for one already on the path, which is a cycle, and for one past the
    /// depth limit. Both answer `null` at the call site.
    ///
    /// The specification throws for the first, and the machinery to do it now
    /// exists — a throw leaves one frame. It is still not done here, and the
    /// reason has moved: a raise from inside a native is only safe once the
    /// natives that call user code check for one. They do not, so a throw raised
    /// here would be left in flight and re-raised at an unrelated call site
    /// later. `null` remains the least wrong answer until that discipline lands.
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

/// The value a `toJSON` hook answers, or the value itself.
///
/// # Why the walk can afford this
///
/// The module header used to say this was "a feature with a design" waiting for
/// a caller, and named its cost: every descent probes for the method, releases,
/// calls, and restarts classification on whatever came back. That cost is real
/// and it is paid here — but the caller arrived, and it is correctness rather
/// than a feature. `JSON.stringify(new Date())` and every object with a `toJSON`
/// serialised as `{}`, which is well-formed JSON that lost the value.
///
/// The infinite-walk worry it also named does not happen, and NOT for the reason
/// that first looks right. The hook runs BEFORE the cell is pushed onto the
/// cycle path, so a hook answering the object it hangs off is not seen as a
/// cycle — the walk simply continues into that object once, finds the hook is a
/// function and skips it, and writes `{}`. It terminates. It also does not match
/// the language, which recurses until the stack runs out. Measured by running
/// it, not reasoned about: the first version of this comment claimed the cycle
/// stack caught it, and it does not.
///
/// A primitive is answered before anything is read, so the common member — a
/// number, a string — costs one borrow and no lookup.
///
/// `key` is the property key `toJSON` is called with — see [`Writer::write`]
/// for where each of the three callers gets theirs. It used to be `undefined`
/// unconditionally, because `write` is reached from three places and only one
/// had a key in hand; now all three do, so the hook sees what the
/// specification says it sees rather than a value that happened to be at hand
/// at the one call site that had one.
fn to_json_of(value: u64, key: HookKey) -> u64 {
    let name = with_current(|context| {
        match super::super::primitive::is_object_in(context, value) {
            true => Some(context.well_known_text("toJSON")),
            false => None,
        }
    });
    let Some(name) = name else {
        return value;
    };
    // Through the ordinary read, so an inherited `toJSON` is found — which is
    // how `Date` provides one — and so an accessor spelling of it runs.
    let hook = super::super::computed::get_indexed(value, name);
    if !with_current(|context| super::super::modules::is_callable_in(context, hook)) {
        return value;
    }
    // Only HERE does the key become a value, which is the point of `HookKey`:
    // by this line the value is an object AND it has a callable `toJSON`, which
    // almost nothing does. Built eagerly it was a `number_to_string` and a cell
    // per element of every array ever serialised.
    let (key, absent) = with_current(|context| {
        let key = match key {
            HookKey::Given(value) => value,
            HookKey::Index(at) => context
                .intern_value(crate::coerce::number_to_string(at as f64))
                .bits(),
        };
        (key, super::super::objects::undefined_of(context))
    });
    super::super::functions::call(hook, value, key, absent, absent, absent)
}

/// What `toJSON` will be called with, before anything decides it will be
/// called at all.
///
/// # Why the index is not resolved at the call site
///
/// Because resolving it ALLOCATES — an array member's key is its index
/// ToString'd, which is a `number_to_string` and a string cell — and the site
/// that has the index cannot know whether the member is even an object, let
/// alone whether it has a hook. Every element of every array serialised paid
/// for a value that was then discarded.
#[derive(Clone, Copy)]
pub(super) enum HookKey {
    /// A key the caller already holds as a value: a property name, or the
    /// empty string the root is serialised under.
    Given(u64),
    /// An array member's position, ToString'd only if a hook is reached.
    Index(usize),
}
