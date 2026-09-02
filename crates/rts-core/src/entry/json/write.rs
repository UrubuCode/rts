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
use super::hooks::Replacer;
use crate::text::Str;
use crate::value::{Kind, Value};

/// What a value is, as far as JSON is concerned.
///
/// Six kinds and an absence. A symbol is the one the language has that this
/// does not name: it serialises as an absence like a function, which is what
/// [`Shape::Absent`] already answers for it, so a variant would carry no
/// decision. A `BigInt` earns one because its rule is a `TypeError` rather than
/// text, which is why this is an enum rather than a chain of tests at the call
/// site.
///
/// A wrapper object is NOT one of them, and deliberately: the specification
/// says `SerializeJSONProperty` replaces `new Number(5)` by its
/// `[[NumberData]]` before it classifies anything, so it arrives here already
/// as `Number(5.0)`. A variant would be a second place deciding what a wrapper
/// serialises as, and the first place is where `valueOf` reads it from.
pub(super) enum Shape {
    Null,
    Bool(bool),
    Number(f64),
    /// A string, BY THE CELL that holds it.
    ///
    /// It carried a `Str` — an owned copy of the whole buffer — and its only
    /// consumer took a reference to it. The copy existed because a `Shape` is
    /// carried out of the `with_current` closure that made it, and nothing had
    /// asked whether it needed to be.
    ///
    /// It does not: `quoted` touches no context, only `self.out`, so the write
    /// can happen inside the borrow — which is what `plain` already does for a
    /// member's KEY one screen below.
    Text(u32),
    /// An array, by the cell that identifies it.
    Array(u32),
    /// Anything else with properties.
    Object(u32),
    /// A bigint, which the language refuses to serialise rather than
    /// approximating. Its own variant because it is the one shape here that
    /// answers with a `TypeError` instead of with text.
    Big,
    /// `undefined`, a function, or anything with no JSON form.
    Absent,
}

/// What a value is, answered inside the caller's borrow and carried out of it.

/// The properties of an object a shape walk alone can serialise, in order.
///
/// # Why this exists beside the general path
///
/// Because the general path reaches an object through the doors a JavaScript
/// PROGRAM uses, and for a plain object every one of them is a detour.
/// `own_keys` allocates a JavaScript array on the heap and a string cell per
/// key, the loop clones that array's elements into a Rust `Vec`, and each
/// member is then read by `get_indexed`, which walks the prototype chain by a
/// key it re-derives from the text. To serialise `{a:1,…,h:8}` — forty
/// characters — that is one heap array, eight key lookups by text and eight
/// chain walks.
///
/// Measured 2026-08-25, `target/release/rts.exe`: `Object.keys` of an
/// eight-property object costs 2 023 ns and `JSON.stringify` of the same object
/// 4 046 — so producing the key list is **half of stringify**, before a single
/// character is written.
///
/// # The four refusals, and none of them is caution
///
/// Each is a case where the general path does something this cannot see:
///
/// - a **proxy** answers `ownKeys` by running a handler, so it has no shape to
///   walk;
/// - an **accessor** must run its getter, which is observable, and its position
///   in the enumeration is ranked separately (`ranked_accessors`) rather than
///   living in the layout;
/// - **elements** come first in enumeration order and are not shape properties
///   at all, so a shape walk would silently drop them;
/// - a **non-enumerable** property is skipped by `Object.keys` and by this, and
///   answering that question per key is what the general path calls
///   `integrity::enumerable` for — asked here too, so the two agree.
///
/// # What it deliberately does NOT return
///
/// The values. Only keys, which are numbers, because a JavaScript reference
/// held in a Rust `Vec` is invisible to the collector — the hazard the general
/// path's `external::hold_current` exists for, and which cost 31 wrong results
/// per 300 000 calls before it did. Each member is read inside its own borrow,
/// one at a time, exactly as the general path reads it.
fn plain_properties(
    context: &mut Context,
    cell: u32,
) -> Option<Vec<rts_cranelift::shape::Key>> {
    if context.proxy_at(cell).is_some() {
        return None;
    }
    if !context.ranked_accessors(cell).is_empty() {
        return None;
    }
    if context.elements_at(cell).is_some() {
        return None;
    }
    let ty = context.region.type_of(cell)?;
    let shape = context.shape_of(ty)?;
    let mut keys = Vec::new();
    for (key, _) in context.shapes.properties(shape) {
        if !super::super::integrity::enumerable(context, cell, key) {
            continue;
        }
        // By reference, and the borrow ends before `enumerable` needs the
        // context again — a clone here would be one per key per call, which is
        // the allocation this path exists to remove.
        let (symbol, indexed) = match context.interner.text(key) {
            Some(text) => (
                super::super::symbol::is_symbol_key(text),
                crate::object::as_array_index(text).is_some(),
            ),
            None => return None,
        };
        // A symbol-keyed property is not enumerated, and its key lives in a
        // RESERVED NAME SPACE rather than in a variant of its own. Asked through
        // the same predicate `key_texts` asks, which is the one place that
        // encoding is known — a second spelling of it here is how the two would
        // come to disagree about what a symbol looks like.
        //
        // Written after a first version tested `text().is_none()`, which is
        // wrong in the direction that ships: a symbol key HAS text, so the check
        // passed and `{ a: 1, [Symbol("s")]: 2 }` serialised as
        // `{"a":1,"@@sym:14":2}` — the engine's internal spelling, in valid
        // JSON, against node and bun answering `{"a":1}`.
        if symbol {
            continue;
        }
        // An ARRAY-INDEX key is refused rather than handled, because
        // enumeration puts those first and in ascending numeric order while a
        // shape holds them in insertion order. `array::ordered` is that rule and
        // this does not restate it: an object with one such key takes the
        // general path, which already applies it.
        if indexed {
            return None;
        }
        keys.push(key);
    }
    Some(keys)
}

pub(super) fn shape_of(context: &Context, value: u64) -> Shape {
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
    // Before the slot test: a bigint is a client value, not an object, and
    // asking `as_slot` first would file it among the objects and serialise it
    // as `{}` — well-formed JSON that lost the number, which is exactly what
    // this answered before.
    if super::super::bigints::digits_of(context, value.bits()).is_some() {
        return Shape::Big;
    }
    if let Some(flag) = value.as_bool() {
        return Shape::Bool(flag);
    }
    if let Some(cell) = value.as_slot() {
        if let Some(text) = context.text_at(cell) {
            // The cell rather than the text: see `Shape::Text`.
            let _ = text;
            return Shape::Text(cell);
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
        Shape::Text(cell) => context
            .text_at(cell)
            .map_or_else(Vec::new, |text| text.units().take(10).collect()),
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
    /// What the second argument to `stringify` was, classified once before the
    /// walk started. See [`super::hooks::Replacer`].
    replacer: Replacer,
}

impl Writer {
    pub(super) fn new(indent: Vec<u16>, replacer: Replacer) -> Self {
        Writer {
            out: Vec::new(),
            open: Vec::new(),
            indent,
            replacer,
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
                true => self.text(&crate::coerce::number_to_string(number)),
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

    /// `[…]`.
    fn array(&mut self, cell: u32, depth: usize) {
        if !self.enter(cell, depth) {
            return self.ascii("null");
        }
        // Copied out of the borrow rather than iterated inside one, because
        // each element's own serialisation calls back into the runtime.
        //
        // HELD for the same reason the key array is: the copy lives in a Rust
        // `Vec`, whose buffer is on the Rust heap and is not scanned, and the
        // array it came from is dead to Rust the moment the clone returns. Every
        // element's serialisation allocates, so a collection in the middle of
        // this loop freed cells this loop still names.
        let anchor = super::super::external::hold_current(Value::from_slot(cell).bits());
        let elements = with_current(|context| context.elements_at(cell).cloned().unwrap_or_default());
        self.ascii("[");
        for (at, element) in elements.iter().enumerate() {
            if super::super::throw::in_flight() {
                break;
            }
            if at > 0 {
                self.ascii(",");
            }
            self.newline(depth + 1);

            // A hole, an `undefined` and a function are each `null` here, where
            // in an object they are skipped. The asymmetry is the language's
            // and it has a reason: an array's members are addressed by
            // position, so dropping one renumbers every one after it.
            // The key `toJSON` sees for an array member is its index, ToString'd
            // -- `[9].toJSON` is called with `"0"`, never with the number 9. Built
            // only if a hook is actually reached: it is a `number_to_string` and
            // an ALLOCATION, and it was paid per element of every array ever
            // serialised, for a hook almost no value has.
            let held = self.hooked(Value::from_slot(cell).bits(), *element, HookKey::Index(at));
            if !self.write(held, depth + 1) {
                self.ascii("null");
            }
        }
        if !elements.is_empty() {
            self.newline(depth);
        }
        // The elements are read for the last time above.
        super::super::external::release_current(anchor);
        self.ascii("]");
        self.leave();
    }

    /// `{…}`.
    fn object(&mut self, value: u64, cell: u32, depth: usize) {
        if !self.enter(cell, depth) {
            return self.ascii("null");
        }
        // A plain object serialised straight off its shape, with no key list on
        // the heap and no read by text. `plain_properties` says which objects
        // those are and why the four it refuses are refusals of substance.
        //
        // Not attempted at all when a list replacer is in force: that names the
        // members and their order itself, so the object's own enumeration is not
        // consulted — a fast path over the shape would answer the wrong members
        // rather than the same ones faster.
        if !matches!(self.replacer, Replacer::List(_))
            && let Some(keys) = with_current(|context| plain_properties(context, cell))
        {
            self.plain(value, keys, depth);
            self.leave();
            return;
        }
        // The runtime's own enumeration, which is what `Object.keys` and
        // `for-in` walk. A second walk of the layout here would be a second
        // answer to "what order", and the two would drift the first time one
        // was fixed.
        // HELD, and this is a correctness fix rather than a nicety.
        //
        // `own_keys` answers an ARRAY on the heap, and the loop below clones its
        // elements into a Rust `Vec` and then allocates — a string per key, a
        // value per member — while walking that clone. The array itself is dead
        // to Rust after the clone, so nothing keeps it in a register and the
        // conservative stack scan cannot see it; the cloned references live in a
        // `Vec`'s buffer, which is on the Rust heap and is not scanned at all.
        //
        // A collection triggered by one of those allocations therefore freed the
        // key strings this loop was about to read, and the cells came back out
        // of the free list as something else. Measured before this: 31 wrong
        // results per 300 000 `JSON.stringify` calls on a four-member object —
        // a key duplicated or dropped, silently, in valid-looking JSON.
        //
        // `external` is a root (`roots.rs`), so holding the array keeps it and
        // everything it reaches alive for exactly as long as this needs them.
        // Released at the end of the function rather than at the end of the
        // loop, because the last key is read after the last iteration.
        // A list replacer names the members and their order; the object's own
        // enumeration is not consulted at all, which is what makes
        // `stringify(o, ["c", "a"])` answer `{"c":…,"a":…}` for an object whose
        // own order is the other way round. Built as a heap array so the hold
        // below covers both cases with one rule rather than two.
        let names = match &self.replacer {
            Replacer::List(keys) => with_current(|context| {
                let interned: Vec<u64> = keys
                    .iter()
                    .map(|key| super::hooks::interned(context, key))
                    .collect();
                super::super::array::built_in(context, interned)
            }),
            _ => super::super::array::own_keys(value),
        };
        let anchor = super::super::external::hold_current(names);
        let names = with_current(|context| {
            Value(names)
                .as_slot()
                .and_then(|cell| context.elements_at(cell).cloned())
                .unwrap_or_default()
        });

        self.ascii("{");
        let mut written = false;
        for name in names {
            if super::super::throw::in_flight() {
                break;
            }
            // Through the ordinary read, so a member that is an accessor runs
            // its getter — which is what `stringify` observably does, and what
            // reading the slot directly would have skipped.
            // Through the ordinary read, so a member that is an accessor runs
            // its getter — which is what `stringify` observably does, and what
            // reading the slot directly would have skipped.
            //
            // Reading it by KEY instead, in one borrow, was written and
            // MEASURED and reverted: `get_indexed` already takes the fast route
            // for a name that is a string cell, so collapsing the borrows moved
            // `{a:1}` from 1942 ns to 2084 ns — inside the run-to-run spread on
            // this machine, which is to say it bought nothing and cost a second
            // path through this loop. Whatever the ~800 ns per member is, it is
            // not this.
            //
            // # FOUND, 2026-08-23, and it is not a JSON problem
            //
            // The per-member cost is ~480 ns, not 800 — the earlier figure came
            // from dividing a fixed cost by a member count. Measured by varying
            // the shape instead of the count:
            //
            //   JSON.stringify(42)          225 ns    the floor for any call
            //   JSON.stringify({})          695 ns    +470 just for being an object
            //   JSON.stringify({a:1})      1417 ns
            //   JSON.stringify({a..h})     4763 ns    ~480 per member
            //   JSON.stringify([1,2,3,4])   778 ns    ~74 per ELEMENT
            //
            // An array element and an object member write the same number, and
            // the member costs six to ten times the element. The whole
            // difference is the KEY, and the key's cost is not here either:
            //
            //   o.a  + o.b  + o.c  + o.d     (literal keys)     39 ns
            //   o[k] x4, k from Object.keys (string keys)     1086 ns
            //
            // Twenty-seven times, and it SCALES WITH THE LENGTH OF THE NAME —
            // 115 ns for a one-character key, 331 for 64 characters, 891 for
            // 256. That is `Context::key_of_text_cell`, which ends in
            // `interner.intern(text, …)`: a HASH OF THE TEXT on every access.
            //
            // So this loop is not slow; reading a property by a string is, and
            // this loop does it once per member. The fix belongs there and is
            // researched rather than guessed — V8 caches the hash in the
            // string's own header and internalizes key strings so lookup
            // compares pointers; SpiderMonkey canonicalizes to atoms and added a
            // cache of recently-atomized strings for exactly this.
            //
            // LANDED as `Str::key`, and the escalation with name length is gone:
            // a 256-character key went from 798 ns to 63, a one-character key
            // from 104 to 63, and a literal read stayed at 26.
            //
            // THIS LOOP moved much less — 4 160 ns to 3 891 — and the reason is
            // worth writing down here rather than being rediscovered: `own_keys`
            // hands back FRESH string cells, so the memo is cold on every call.
            // The key resolution is no longer the cost; building the key strings
            // is. That is the next question for this file, and it is a different
            // one.
            let held = super::super::computed::get_indexed(value, name);
            let key = with_current(|context| super::super::text::to_text(context, Value(name)));
            let Some(key) = key else {
                continue;
            };
            // The hooks run HERE, before the key is written, because they are
            // what decides whether there is a value at all: a `toJSON` or a
            // replacer answering `undefined` skips the member, and asking after
            // the key was emitted produced `{"drop":}`.
            //
            // Classified once here and again inside `write` — one extra borrow
            // per member, and it buys the separator staying correct: a member
            // skipped after its comma was emitted is a trailing comma, which is
            // not JSON either.
            //
            // `name` is already the string key, straight from `own_keys`, so
            // this is the same value `toJSON` must see with no second
            // conversion to disagree with the first.
            let held = self.hooked(value, held, HookKey::Given(name));
            // Classified once, the answer carried — see `plain`.
            if super::super::throw::in_flight() {
                return;
            }
            let shape = with_current(|context| shape_of(context, held));
            if matches!(shape, Shape::Absent) {
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
            self.write_shape(shape, held, depth + 1);
        }
        if written {
            self.newline(depth);
        }
        // The keys are read for the last time above, so the hold ends here.
        super::super::external::release_current(anchor);
        self.ascii("}");
        self.leave();
    }

    /// The members of a plain object, read one at a time off its layout.
    ///
    /// Mirrors the general loop in [`Self::object`] step for step — the throw
    /// check, the hooks before the key is written, the `Absent` skip that keeps
    /// a trailing comma from happening, the separator — and differs only in
    /// where the key and the value come from. Written as its own function so
    /// that the difference is the only thing a reader has to compare, rather
    /// than a second copy of the whole rule to keep in agreement with the first.
    ///
    /// `keys` holds numbers, never references, so nothing here is invisible to
    /// the collector while an allocation happens. That is why the general path's
    /// `external::hold_current` has no counterpart in this one: there is no
    /// heap array to keep alive, because none was made.
    fn plain(&mut self, value: u64, keys: Vec<rts_cranelift::shape::Key>, depth: usize) {
        self.ascii("{");
        let mut written = false;
        for key in keys {
            if super::super::throw::in_flight() {
                break;
            }
            // Both in ONE borrow: the member is an own data property of an
            // object `plain_properties` proved has no accessors and no proxy, so
            // reading it runs nothing and can allocate nothing — which is what
            // makes taking the text alongside it safe here and not in the
            // general loop.
            let Some(held) = with_current(|context| {
                let found = super::super::objects::own_property(
                    context,
                    Value(value).as_slot()?,
                    crate::object::Key::Name(key),
                )?;
                Some(found.bits())
            }) else {
                continue;
            };
            let held = self.hooked(value, held, HookKey::Named(key));
            // CLASSIFIED ONCE, and the answer carried to the write.
            //
            // The test exists to satisfy rule 8 — a `toJSON` or a replacer
            // answering `undefined` must not produce `{"drop":}` — and it was
            // asking `shape_of` solely to see `Absent`, after which `write`
            // asked the identical question again. Passing the decision along
            // removes the second borrow and the second classification without
            // removing the question.
            if super::super::throw::in_flight() {
                return;
            }
            let shape = with_current(|context| shape_of(context, held));
            if matches!(shape, Shape::Absent) {
                continue;
            }
            if written {
                self.ascii(",");
            }
            written = true;
            self.newline(depth + 1);
            with_current(|context| {
                if let Some(text) = context.interner.text(key) {
                    self.quoted(text);
                }
            });
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
            self.out.push(b'"' as u16);
            quoted_narrow(&mut self.out, bytes);
            self.out.push(b'"' as u16);
            return;
        }
        let units: Vec<u16> = text.units().collect();
        self.out.push(b'"' as u16);
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
                    self.out.push(u16::from(digits[((unit >> shift) & 0xf) as usize]));
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
    // The common shape, decided inside ONE borrow: an ordinary object, asked
    // for `toJSON` by KEY. The general route below converts a string cell to a
    // key and then walks the chain through `get_indexed`, which is a second
    // resolution of a name this crate already knows the number of — paid per
    // object value, and answering "absent" for almost all of them.
    //
    // A proxy and a getter are the two cases it hands back, because both call
    // user code and neither may happen while the context is borrowed.
    enum Ask {
        /// An ordinary object, and this is what `toJSON` read as.
        Read(u64),
        /// Not an object at all: nothing to ask.
        Skip,
        /// Ask the long way — a proxy, or an accessor spelling of `toJSON`.
        Slowly,
    }
    let asked = with_current(|context| {
        // A BIGINT is asked too, in as many words: `SerializeJSONProperty`
        // reads `toJSON` when the value is an Object **or a BigInt**. It is the
        // one primitive with that exemption, and it has to be — a bigint has no
        // JSON form, so a hook is the only way a program can give it one, and
        // not looking means the `TypeError` in `write` fires for a value that
        // had an answer. The SLOW route because a bigint is
        // `Value::from_client` and not a cell: there is no cell for
        // `accessor::resolve` to start a chain walk from, and `get_indexed`
        // already knows how one reaches `BigInt.prototype`.
        if super::super::bigints::digits_of(context, value).is_some() {
            return Ask::Slowly;
        }
        if !super::super::primitive::is_object_in(context, value) {
            return Ask::Skip;
        }
        let Some(cell) = Value(value).as_slot() else {
            return Ask::Slowly;
        };
        if context.proxy_at(cell).is_some() {
            return Ask::Slowly;
        }
        let key = context.well_known("toJSON");
        match super::super::accessor::resolve(context, cell, key) {
            super::super::accessor::Found::Value(found) => Ask::Read(found),
            super::super::accessor::Found::Absent => Ask::Skip,
            super::super::accessor::Found::Getter(_) => Ask::Slowly,
        }
    });
    let hook = match asked {
        Ask::Skip => return value,
        Ask::Read(hook) => hook,
        // Through the ordinary read, so an inherited `toJSON` is found — which
        // is how `Date` provides one — and so an accessor spelling of it runs.
        Ask::Slowly => {
            let name = with_current(|context| context.well_known_text("toJSON"));
            super::super::computed::get_indexed(value, name)
        }
    };
    if !with_current(|context| super::super::modules::is_callable_in(context, hook)) {
        return value;
    }
    // Only HERE does the key become a value, which is the point of `HookKey`:
    // by this line the value is an object AND it has a callable `toJSON`, which
    // almost nothing does. Built eagerly it was a `number_to_string` and a cell
    // per element of every array ever serialised.
    let (key, absent) = with_current(|context| {
        (key.value(context), super::super::objects::undefined_of(context))
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
    /// A property the shape walk named, resolved to its one cell only if a hook
    /// is actually reached.
    ///
    /// The plain-object path never materialises a key otherwise — not building
    /// them is the whole of what it saves — so this variant is what keeps a
    /// `toJSON` seeing exactly the value the general path would have shown it.
    Named(rts_cranelift::shape::Key),
}

impl HookKey {
    /// The key as a value, which is what a call's argument is.
    ///
    /// Reached from two places now — `toJSON` and the replacer — and one of
    /// them would otherwise convert an index a second time and disagree with
    /// the first about what `"0"` is.
    pub(super) fn value(self, context: &mut Context) -> u64 {
        match self {
            HookKey::Given(value) => value,
            HookKey::Index(at) => context
                .intern_value(crate::coerce::number_to_string(at as f64))
                .bits(),
            HookKey::Named(key) => context.key_value(key),
        }
    }
}


/// Writes a Latin-1 JSON string body directly into the UTF-16 output buffer.
fn quoted_narrow(out: &mut Vec<u16>, bytes: &[u8]) {
    for &unit in bytes {
        match unit {
            b'"' => out.extend([b'\\' as u16, b'"' as u16]),
            b'\\' => out.extend([b'\\' as u16, b'\\' as u16]),
            0x08 => out.extend([b'\\' as u16, b'b' as u16]),
            0x0c => out.extend([b'\\' as u16, b'f' as u16]),
            b'\n' => out.extend([b'\\' as u16, b'n' as u16]),
            b'\r' => out.extend([b'\\' as u16, b'r' as u16]),
            b'\t' => out.extend([b'\\' as u16, b't' as u16]),
            0x00..=0x1f => {
                out.extend([b'\\' as u16, b'u' as u16, b'0' as u16, b'0' as u16]);
                let digits = b"0123456789abcdef";
                out.push(u16::from(digits[(unit >> 4) as usize]));
                out.push(u16::from(digits[(unit & 0xf) as usize]));
            }
            _ => out.push(u16::from(unit)),
        }
    }
}
