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
///
/// # Remembered by shape, for the length of one walk
///
/// A hundred rows of one shape asked all of this a hundred times: a `Vec`, and
/// per key an attribute lookup, the key's text, a symbol test and an index
/// test. Everything but the attributes is a fact about the SHAPE — a shape
/// never changes, the tree only grows — so [`Plan`] keeps the last answer and
/// a cell of the same shape with no attributes recorded takes it whole. One
/// entry per DEPTH and not a map: the rows of a document are adjacent, and a
/// row's own children would otherwise overwrite the row's answer between one
/// row and the next.
///
/// The answer is LENT, not shared: the caller takes the depth's plan out,
/// walks with it, and puts it back. It was an `Rc` for one build, and the clock
/// said what that cost — an allocation per object, so five nested objects of
/// five shapes went from 1 755 ns to 2 138.
fn plain_properties(
    context: &mut Context,
    cell: u32,
    plan: &mut Option<Plan>,
) -> Option<Lent> {
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
    // Only a cell with nothing recorded may take or leave a remembered answer:
    // `enumerable` below is then the same for every cell of the shape.
    let ordinary = !context.records_attributes(cell);
    if ordinary
        && let Some((known, keys, labelled)) = plan.as_mut()
        && *known == shape
    {
        // The SECOND cell of a shape is what pays for the labels, and the
        // first never does. Built eagerly they were two allocations a key for
        // every lone object ever serialised, and the clock said so: eight
        // properties went from 1 168 ns to 1 695 the day they were.
        if !*labelled {
            *keys = shape_keys(context, cell, shape, true);
            *labelled = true;
        }
        return keys.is_some().then_some(Lent::Planned);
    }
    let keys = shape_keys(context, cell, shape, false);
    if !ordinary {
        return keys.map(Lent::Own);
    }
    let usable = keys.is_some();
    *plan = Some((shape, keys, false));
    usable.then_some(Lent::Planned)
}

/// What one walk hands the next: the plans by depth, and two buffers kept for
/// their capacity. See [`super::Scratch`].
pub(super) type Kept = (Vec<Option<Plan>>, Vec<u32>, Vec<u8>);

/// Where the members [`plain_properties`] answered are.
enum Lent {
    /// In the plan the caller handed in.
    Planned,
    /// Here: the cell records attributes of its own, so its answer is about
    /// the cell and must not be left for the next one of its shape.
    Own(Keys),
}

/// One member of a shape, with everything about it that is the SHAPE's.
///
/// The slot is what `own_property` would find by hashing the key, and the label
/// is what `quoted` would produce by scanning its text — both asked per member
/// per object, and both the same for every object of the shape.
pub(super) struct Member {
    key: rts_cranelift::shape::Key,
    /// Where the value sits, WHILE the cell still has this shape. A `toJSON`
    /// further up the walk may delete a property of the holder, so the reader
    /// checks the shape before it trusts this — see [`Writer::plain`].
    slot: u32,
    /// The key as a JSON string literal, quotes and escapes included. Empty
    /// until a shape repeats, and for a key with a unit above 255 — both are
    /// written the long way, from the interner's text.
    label: Vec<u8>,
}

/// The members a shape walk serialises, or `None` where it must not be one.
type Keys = Vec<Member>;

/// The last shape [`plain_properties`] answered for, and its answer — a refusal
/// included, which is as much a fact about the shape as a key list is. The flag
/// is whether the labels were built yet.
pub(super) type Plan = (rts_cranelift::shape::ShapeId, Option<Keys>, bool);

fn shape_keys(
    context: &mut Context,
    cell: u32,
    shape: rts_cranelift::shape::ShapeId,
    labelled: bool,
) -> Option<Keys> {
    let mut keys = Vec::new();
    for (key, _) in context.shapes.properties(shape) {
        if !super::super::integrity::enumerable(context, cell, key) {
            continue;
        }
        // By reference, and the borrow ends before `enumerable` needs the
        // context again — a clone here would be one per key per call, which is
        // the allocation this path exists to remove.
        let (symbol, indexed, label) = match context.interner.text(key) {
            Some(text) => (
                super::super::symbol::is_symbol_key(text),
                crate::object::as_array_index(text).is_some(),
                text.narrow().filter(|_| labelled).map_or_else(Vec::new, |bytes| {
                    let mut label = super::out::Out::new();
                    label.bytes(b"\"");
                    label.escaped(bytes);
                    label.bytes(b"\"");
                    label.narrow().to_vec()
                }),
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
        let slot = context.shapes.slot_of(shape, key)?;
        keys.push(Member { key, slot, label });
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

    /// `[…]`.
    ///
    /// # Why each element is a fresh `[[Get]]` and not a clone of the store
    ///
    /// `SerializeJSONArray` reads `len` ONCE and then performs an ordinary
    /// property read per index — `array[i]`, the same operation `array[1]`
    /// compiles to — so an index that is an ACCESSOR runs its getter, and a
    /// getter or a `toJSON` that shrinks the array is observable on every
    /// index still to come: `arr = [0,1,2,3]` with a getter at index 1 that
    /// sets `arr.length = 2` serialises as `[0,"one",null,null]` — `len` was
    /// still 4, but reading indices 2 and 3 after the shrink answers
    /// `undefined`, which serialises as `null` here exactly as a hole does.
    ///
    /// A clone of the element store, taken once, cannot show any of that: it
    /// answers what the array held before the walk started, so a shrink mid
    /// walk left the old values printed — a well-formed but wrong array.
    fn array(&mut self, cell: u32, depth: usize) {
        if !self.enter(cell, depth) {
            return self.ascii("null");
        }
        // A root for the whole walk: every read below is a call back into the
        // runtime, and the array itself is otherwise named by nothing a
        // conservative stack scan can see once `length` has been read out of
        // it.
        let anchor = super::super::external::hold_current(Value::from_slot(cell).bits());
        let object = Value::from_slot(cell).bits();
        // `LengthOfArrayLike`, read ONCE — see this function's own
        // documentation for why every element after it is still a live read.
        let length = with_current(|context| {
            let key = super::super::computed::length_key(context);
            super::super::objects::own_property(context, cell, key)
                .and_then(|value| value.numeric())
                .map_or(0usize, |number| number.max(0.0) as usize)
        });
        self.ascii("[");
        let mut at = 0;
        while at < length {
            if super::super::throw::in_flight() {
                break;
            }
            // A RUN of primitive elements in one borrow — see
            // [`Self::run_of_elements`]. Still a live read of the store at the
            // moment each index is reached: a run ends at the first element
            // that could run anything, so a shrink by an element's `toJSON` is
            // seen by the run after it exactly as the ordinary read sees it.
            if self.unobserved() {
                at = with_current(|context| self.run_of_elements(context, cell, at, length, depth));
                if at >= length {
                    break;
                }
            }
            if at > 0 {
                self.ascii(",");
            }
            self.newline(depth + 1);
            // The ordinary indexed read: a hole and an `undefined` element
            // both answer `undefined` here exactly as [`super::super::array::visible`]
            // says a compiled `a[k]` does, which is what keeps this agreeing
            // with the language about what an element IS at the moment it is
            // actually read, rather than at the moment the walk began.
            let element = super::super::computed::get_indexed(
                object,
                Value::from_f64(at as f64).bits(),
            );
            let held = self.hooked(object, element, HookKey::Index(at));
            if !self.write(held, depth + 1) {
                self.ascii("null");
            }
            at += 1;
        }
        if length > 0 {
            self.newline(depth);
        }
        // The array is read for the last time above.
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
        if !matches!(self.replacer, Replacer::List(_)) {
            if self.plans.len() <= depth {
                self.plans.resize_with(depth + 1, || None);
            }
            // TAKEN OUT for the walk and put back after it: `plain` descends
            // into `self`, so the members cannot stay borrowed from it.
            let mut plan = self.plans[depth].take();
            let lent = with_current(|context| plain_properties(context, cell, &mut plan));
            let walked = match (&lent, &plan) {
                (Some(Lent::Own(keys)), _) => {
                    self.plain(value, keys, depth);
                    true
                }
                (Some(Lent::Planned), Some((_, Some(keys), _))) => {
                    self.plain(value, keys, depth);
                    true
                }
                _ => false,
            };
            self.plans[depth] = plan;
            if walked {
                self.leave();
                return;
            }
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
    fn plain(&mut self, value: u64, keys: &[Member], depth: usize) {
        self.ascii("{");
        let mut written = false;
        let Some(cell) = Value(value).as_slot() else {
            return self.ascii("}");
        };
        let shape = with_current(|context| context.shape_of(context.region.type_of(cell)?));
        let mut at = 0;
        while at < keys.len() {
            if super::super::throw::in_flight() {
                break;
            }
            // A RUN of primitive members in one borrow — see
            // [`Self::run_of_members`]. It stops AT the first member that is
            // not one, which is the member the rest of this pass is about.
            if self.unobserved() {
                at = with_current(|context| {
                    self.run_of_members(context, cell, shape, keys, at, depth, &mut written)
                });
                if at >= keys.len() {
                    break;
                }
            }
            let member = &keys[at];
            at += 1;
            // The member is an own data property of an object
            // `plain_properties` proved has no accessors and no proxy, so
            // reading it runs nothing and can allocate nothing.
            let Some(held) = with_current(|context| member_value(context, cell, shape, member)) else {
                continue;
            };
            let held = self.hooked(value, held, HookKey::Named(member.key));
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
            let classified = with_current(|context| shape_of(context, held));
            if matches!(classified, Shape::Absent) {
                continue;
            }
            if written {
                self.ascii(",");
            }
            written = true;
            self.newline(depth + 1);
            with_current(|context| self.label(context, member));
            self.ascii(if self.indent.is_empty() { ":" } else { ": " });
            self.write_shape(classified, held, depth + 1);
        }
        if written {
            self.newline(depth);
        }
        self.ascii("}");
    }

    /// Writes members from `at` for as long as they are primitives, and answers
    /// the index it stopped at.
    ///
    /// # Why a run and not a member
    ///
    /// Writing a primitive runs no user code and allocates no cell, so nothing
    /// can change between one member and the next: the borrow that read the
    /// first is as good for the second. It was a borrow per member — a
    /// thread-local read and a `RefCell` flag each — and before that five.
    /// Measured 2026-09-19, `target/release/rts.exe`: eight numeric members
    /// cost 110 ns each where an array element cost 45.
    #[allow(clippy::too_many_arguments)]
    fn run_of_members(
        &mut self,
        context: &mut Context,
        cell: u32,
        shape: Option<rts_cranelift::shape::ShapeId>,
        keys: &[Member],
        mut at: usize,
        depth: usize,
        written: &mut bool,
    ) -> usize {
        let indented = !self.indent.is_empty();
        while let Some(member) = keys.get(at) {
            let Some(found) = member_value(context, cell, shape, member) else {
                // Gone since the plan was made: skipped, as the ordinary read
                // skips it.
                at += 1;
                continue;
            };
            if !is_primitive(context, found) {
                break;
            }
            if *written {
                self.ascii(",");
            }
            *written = true;
            self.newline(depth + 1);
            self.label(context, member);
            self.ascii(if indented { ": " } else { ":" });
            self.direct(context, found);
            at += 1;
        }
        at
    }

    /// Writes elements from `at` for as long as they are primitives, and
    /// answers the index it stopped at. [`Self::run_of_members`] has the
    /// argument; what is particular to an array is what ends a run.
    ///
    /// A HOLE ends it, because a hole reads through the prototype chain and
    /// that is the ordinary read's to answer. So does an accessor anywhere on
    /// the cell, asked once per run: nothing in a run can define one.
    fn run_of_elements(&mut self, context: &Context, cell: u32, mut at: usize, length: usize, depth: usize) -> usize {
        if !context.ranked_accessors(cell).is_empty() {
            return at;
        }
        let Some(held) = context.elements_at(cell) else {
            return at;
        };
        while at < length {
            let Some(found) = held.get(at).copied() else {
                break;
            };
            if super::super::array::is_hole(context, found) || !is_primitive(context, found) {
                break;
            }
            if at > 0 {
                self.ascii(",");
            }
            self.newline(depth + 1);
            self.direct(context, found);
            at += 1;
        }
        at
    }

    /// A member's key, from the plan where it is narrow and from the interner
    /// where it is not.
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

/// A plain member's value: by SLOT while the cell is still the shape the plan
/// was made for, and by key the moment it is not — a hook earlier in the walk
/// may have deleted or added a property of the holder.
fn member_value(
    context: &mut Context,
    cell: u32,
    shape: Option<rts_cranelift::shape::ShapeId>,
    member: &Member,
) -> Option<u64> {
    if context.shape_of(context.region.type_of(cell)?) == shape {
        return super::super::objects::slot_value(context, cell, member.slot);
    }
    super::super::objects::own_property(context, cell, crate::object::Key::Name(member.key))
        .map(|found| found.bits())
}

/// Whether [`Writer::direct`] will write this value: text, a number, a boolean
/// or `null`. Not `undefined`, a symbol or a bigint — each has a rule of its
/// own (a skipped member, a `TypeError`) that the ordinary path states once.
fn is_primitive(context: &Context, value: u64) -> bool {
    if Value(value).numeric().is_some() {
        return true;
    }
    match Value(value).as_slot() {
        Some(cell) => context.text_at(cell).is_some(),
        None => matches!(shape_of(context, value), Shape::Null | Shape::Bool(_) | Shape::Number(_)),
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

