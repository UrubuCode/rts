//! `ArrayBuffer`, `DataView` and the typed arrays — memory a program addresses
//! by byte rather than by name.
//!
//! # Where the bytes live, and why not in the region
//!
//! A region cell is sixty-four bytes, fixed, with word accessors only. A buffer
//! is any length and is addressed by byte, so it cannot be a cell — and carving
//! byte accessors into the region to serve it would put a second addressing model
//! into the one structure the compiler and the runtime agree about.
//!
//! So the bytes go in a `Slab<Vec<u8>>` and the cell is the identity, which is
//! **exactly** what [`super::array`] already does with `Slab<Vec<u64>>`: elements
//! apart from properties, the cell naming which. The rejected alternative was a
//! reserved layout for a buffer, and it is rejected for the reason arrays learned
//! the hard way — a cell with a reserved layout cannot hold a property, so
//! `b.tag = 9` becomes a silent no-op and `b.tag` reads `undefined`. A wrong
//! program that runs is worse than a refusal.
//!
//! # Why a view holds a reference and never a copy
//!
//! This is the property the whole module is shaped around: a `DataView` and a
//! `Uint8Array` over the SAME `ArrayBuffer` must see each other's writes. So
//! [`View`] records the buffer's **cell**, a byte offset, a byte length and an
//! element kind, and every read and write goes to the buffer's bytes through it.
//! Nothing is ever copied except where the language says a copy is made, which is
//! `slice` and only `slice` — `subarray` answers another view over the same
//! bytes, and that difference between the two methods is their entire reason for
//! both existing.
//!
//! The view does NOT cache the buffer's [`Slot`]. It could — one indirection
//! saved per access — and then there would be two answers to "where are this
//! view's bytes", which is the shape of every aliasing bug this module exists to
//! avoid. The cell is the single source and the slot is derived.
//!
//! # The collector does not know about these tables
//!
//! Nothing calls `crate::collect::mark` today, so nothing collects, and an
//! [`Aside`] is invisible to a tracing collector in any case. Holding bytes in
//! `Context::buffers` and views in `Context::views` is therefore safe **today**
//! and is exactly the bet `Context::arrays` already makes for array elements.
//!
//! The day there is a collector it has to learn about **both** of these tables,
//! and about one thing arrays do not have: a live typed array keeps its
//! `ArrayBuffer` alive through `View::buffer`, which is a reference the collector
//! can only see if it is told to trace this table. Untraced, a program holding
//! only the view would have its bytes swept while the view still points at them.
//! That is the note, not a mechanism — inventing a tracing hook with no collector
//! to call it would be a second design for the collector to disagree with.
//!
//! # Eleven classes, and the two that are not like the other nine
//!
//! Nine of them differ in a width and a conversion rule, which is why they are
//! one line each over one implementation. `Uint8ClampedArray` is the ninth and
//! stretches that: its write saturates and rounds half-to-even where every other
//! one wraps and truncates — a different rule at the same width.
//!
//! `BigInt64Array` and `BigUint64Array` are the two that genuinely differ,
//! because their elements are **bigints and not numbers**. Sixty-four bits is
//! exactly the width where a double stops being able to carry an integer
//! element: every other kind fits inside 2^53, so the codec could speak in
//! doubles throughout, and these two are the reason it now speaks in **words**
//! with the double as one face over them.
//!
//! What that costs is one question at each element access — [`element::Kind::is_bigint`]
//! — and what it buys is that the byte gathering and the byte order are still
//! written once. Two codecs would have been two places for the endianness
//! decision, and a typed array and a `DataView` disagreeing about byte three is
//! invisible until it is not.
//!
//! The language refuses coercion between the two families in **both**
//! directions: a number written into a bigint element is a `TypeError`, and so is
//! a bigint written into a numeric one. This engine cannot raise it, so such a
//! write is **dropped** — the element keeps what it held. Coercing instead was
//! the rejected alternative, and it is the dangerous one: it would make a program
//! no other engine accepts run and answer something.
//!
//! A `DataView` reads and writes them too, through four members of its own —
//! `getBigInt64`, `getBigUint64`, `setBigInt64`, `setBigUint64`. They are four
//! rather than a flag on the existing sixteen because the split is in the TYPE
//! of the value and not in its width, and they gather bytes through the same
//! [`element::word_at`] the typed arrays do, so the two cannot come to disagree
//! about byte three. This paragraph said they were absent and that "nothing has
//! asked"; something did.

mod array_buffer;
pub(in crate::entry) mod atomics;
mod bounds;
pub(in crate::entry) mod detach;
mod data_view;
pub(in crate::entry) mod element;
mod shared_array_buffer;
pub(in crate::entry) mod typed;
mod typed_classes;
mod typed_order;
mod typed_species;
mod typed_visit;

// The declared-type consts travel with the registrations, for the same reason:
// `entry::declared` names one class per line and cannot reach into a private
// submodule to find it.
pub(in crate::entry) use array_buffer::{ARRAY_BUFFER_TYPES, register_array_buffer};
pub(in crate::entry) use atomics::ATOMICS_TYPES;
pub(in crate::entry) use data_view::DATA_VIEW_TYPES;
pub(in crate::entry) use shared_array_buffer::SHARED_ARRAY_BUFFER_TYPES;
pub(in crate::entry) use atomics::register_atomics;
pub(in crate::entry) use data_view::register_data_view;
pub(in crate::entry) use shared_array_buffer::register_shared_array_buffer;
// The argument rules, re-exported rather than moved at the call sites: every
// member reads them as `super::range` and `super::optional_number`, and where
// they are written is not a fact any of them should have to know.
pub(in crate::entry) use bounds::{as_count, optional_number, range, undefined};
// The eleven wrappers rather than the eleven names `#[rtse::class]` derives:
// each also installs `BYTES_PER_ELEMENT` on the constructor, which is a
// property the language puts on both halves and the attribute can only put on
// one.
pub(in crate::entry) use typed_classes::{
    big_int64_array, big_uint64_array, float32_array, float64_array, int8_array, int16_array,
    int32_array, uint8_array, uint8_clamped_array, uint16_array,
    uint32_array,
};

use element::Kind;

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::heap::Slot;
use crate::value::Value;

/// What a view sees: whose bytes, from where, how many, read as what.
///
/// `length` is in **bytes** for every kind, including the typed arrays, and the
/// element count is derived. One number with one meaning: the alternative —
/// elements for a typed array and bytes for a `DataView` — is a field whose unit
/// depends on a sibling field, which is the kind of invariant that holds until
/// one function forgets and then reads four times too far.
#[derive(Clone, Copy)]
pub(in crate::entry) struct View {
    /// The `ArrayBuffer` cell these bytes belong to, never a copy of them.
    pub(in crate::entry) buffer: u32,
    /// Where in that buffer this view starts.
    pub(in crate::entry) offset: usize,
    /// How many bytes of it the view covers.
    pub(in crate::entry) length: usize,
    /// How those bytes are read.
    pub(in crate::entry) kind: Kind,
}

impl View {
    /// How many elements the view holds.
    pub(in crate::entry) fn count(&self) -> usize {
        self.length / self.kind.size()
    }
}

impl Context {
    /// The bytes a cell owns, if it is an `ArrayBuffer`.
    pub(in crate::entry) fn bytes_at(&self, cell: u32) -> Option<&Vec<u8>> {
        self.buffers.at(self.buffer_of.copied(cell)?).ok()
    }

    /// The same, to write through.
    pub(in crate::entry) fn bytes_at_mut(&mut self, cell: u32) -> Option<&mut Vec<u8>> {
        let store = self.buffer_of.copied(cell)?;
        self.buffers.at_mut(store).ok()
    }

    /// What a cell views, if it is a view.
    pub(in crate::entry) fn view_at(&self, cell: u32) -> Option<View> {
        self.views.copied(cell)
    }

    /// Records that a cell is a buffer over these bytes.
    fn mark_buffer(&mut self, cell: u32, store: Slot) {
        self.buffer_of.set(cell, store);
    }
}

/// The view a value names, if it names one.
pub(in crate::entry) fn view_of(context: &Context, value: u64) -> Option<View> {
    context.view_at(Value(value).as_slot()?)
}

/// The bytes a view covers, already narrowed to its own window.
///
/// Narrowed here rather than at each caller because every accessor would
/// otherwise repeat `offset + index`, and the one that forgot would read another
/// view's elements — which is not a crash but a wrong answer, and one that only
/// appears once two views share a buffer at different offsets.
pub(in crate::entry) fn window<'a>(context: &'a Context, view: &View) -> Option<&'a [u8]> {
    let bytes = context.bytes_at(view.buffer)?;
    bytes.get(view.offset..view.offset + view.length)
}

/// The same, to write through.
pub(in crate::entry) fn window_mut<'a>(
    context: &'a mut Context,
    view: &View,
) -> Option<&'a mut [u8]> {
    let range = view.offset..view.offset + view.length;
    context.bytes_at_mut(view.buffer)?.get_mut(range)
}

/// A new `ArrayBuffer` of `length` zero bytes, as a cell.
///
/// The prototype comes from the class's own registration, so a buffer made by
/// `t.slice()` answers to the same `ArrayBuffer.prototype` a `new ArrayBuffer`
/// does — and registering here rather than assuming registration is what makes
/// `new Uint8Array(4).buffer.byteLength` work in a program that never wrote the
/// name `ArrayBuffer`.
pub(in crate::entry) fn new_buffer(context: &mut Context, length: usize) -> Option<u32> {
    let cell = super::native::plain(context)?;
    // ASKED first, registered only if the answer is no.
    //
    // `class_support`'s table is a `Vec` scanned by comparing class NAMES, and
    // this paid two of those scans per buffer — one inside the registration's
    // own idempotence check, one to read the prototype back — for a class that
    // is already there on every allocation but the first.
    //
    // Registering is still what happens when it is genuinely absent, and that
    // is not a nicety: `new Uint8Array(8)` in a program that never wrote the
    // word `ArrayBuffer` has to work.
    let prototype = match super::class_support::prototype(context, "ArrayBuffer") {
        Some(found) => Some(found),
        None => {
            register_array_buffer(context);
            super::class_support::prototype(context, "ArrayBuffer")
        }
    };
    if let Some(prototype) = prototype {
        context.set_prototype(cell, prototype);
    }
    install_bytes(context, cell, length);
    Some(cell)
}

/// Gives a cell that already exists its bytes.
///
/// Apart from [`new_buffer`] because `new ArrayBuffer(8)` is handed the object
/// `new` already made, prototype and all, and allocating a second one would
/// answer something `instanceof` a subclass is not.
pub(in crate::entry) fn install_bytes(context: &mut Context, cell: u32, length: usize) {
    let store = context.buffers.insert(vec![0u8; length]).slot();
    context.mark_buffer(cell, store);
    stamp(context, cell, "byteLength", length as f64);
    // Written at birth rather than at the detach, so that the property EXISTS
    // on every buffer: a program asking `b.detached` before anything detached it
    // has to read `false` and not `undefined`, and `undefined` is falsy — so the
    // bug would be invisible in the `if` every caller writes and visible only in
    // the `===` a test writes.
    flag(context, cell, "detached", false);
}

/// One boolean property, by name.
///
/// Beside [`stamp`] rather than folded into it: a property whose value is a
/// number and one whose value is a boolean are different enough that a single
/// `f64` entry point would have callers passing `1.0` for true, which is the
/// value `b.detached === true` does not equal.
pub(in crate::entry) fn flag(context: &mut Context, cell: u32, name: &str, value: bool) {
    let key = context.well_known(name);
    let value = Value::from_bool(value).bits();
    super::objects::put(context, cell, key, value);
}

/// Attaches a view to a cell and writes the four facts a program reads off it.
///
/// # Why `length`, `byteLength`, `byteOffset` and `buffer` are real properties
///
/// The reason [`super::array::set_length`] records and [`super::collections`]
/// records again: compiled code reading a property that is in the layout never
/// asks the runtime at all — it emits `cached_get` and finds the stored slot — so
/// a value only the slow path knows about is one the fast path disagrees with the
/// moment it starts working. The language makes these accessors on the prototype;
/// this makes them own data properties, and the divergence is that `a.length = 5`
/// stores a number instead of being refused. The same trade an array's `length`
/// already makes.
pub(in crate::entry) fn attach(context: &mut Context, cell: u32, view: View) {
    context.views.set(cell, view);

    // ONE shape, not four. These properties are always the same names in the
    // same order for a given kind, so every typed array ever made walks the
    // same four transitions to the same layout — and each `put` was a shape
    // transition, a slot lookup, a type mint and a header write, three of
    // which are thrown away by the next one.
    //
    // The transitions themselves are memoised on `(parent, key, repr)`, so
    // arriving at the shape is cheap. What this removes is the three
    // intermediate TYPES and the work `objects::put` does around a store that
    // a freshly allocated cell cannot need: an integrity check on an object
    // nothing has frozen, an accessor walk on one that has none, and the
    // array-length reconciliation for a `length` that is not an array's.
    let buffer = Value::from_slot(view.buffer).bits();
    let named: [(crate::object::Key, u64); 4] = [
        (context.well_known("byteLength"), Value::from_f64(view.length as f64).bits()),
        (context.well_known("byteOffset"), Value::from_f64(view.offset as f64).bits()),
        (context.well_known("length"), Value::from_f64(view.count() as f64).bits()),
        (context.well_known("buffer"), buffer),
    ];
    // `Raw` is an ArrayBuffer view with no element count, so it takes three of
    // the four — the same set the loop below would have stamped.
    let wanted = match view.kind {
        Kind::Raw => &named[..2],
        _ => &named[..],
    };

    let mut shape = context.shapes.root();
    let mut slots = Vec::with_capacity(wanted.len());
    for (key, _) in wanted {
        let crate::object::Key::Name(machine) = key else {
            continue;
        };
        let Ok(grown) = context.shapes.transition(shape, *machine, rts_cranelift::repr::Repr::Tagged) else {
            // Something refused the layout. Fall back to the general path
            // rather than leaving the cell half-shaped.
            for (key, value) in wanted {
                super::objects::put(context, cell, *key, *value);
            }
            return;
        };
        let Some(at) = context.shapes.slot_of(grown, *machine) else {
            for (key, value) in wanted {
                super::objects::put(context, cell, *key, *value);
            }
            return;
        };
        slots.push(at);
        shape = grown;
    }

    // The type once, against the link this cell already has — `made` sets the
    // prototype before calling here, and losing that discrimination would put
    // every typed array kind back on one layout.
    let link = context.prototype_at(cell);
    let ty = context.typed_as(shape, link).index() as u32;
    context.region.set_type(cell, ty);
    for (at, (_, value)) in slots.iter().zip(wanted) {
        super::objects::set_slot_value(context, cell, *at, *value);
    }
}

/// One numeric property, by name.
pub(in crate::entry) fn stamp(context: &mut Context, cell: u32, name: &str, value: f64) {
    let key = context.well_known(name);
    let value = Value::from_f64(value).bits();
    super::objects::put(context, cell, key, value);
}

/// `t[i]` — the element, or `undefined` for an index outside the view.
///
/// # Why absence is still an answer
///
/// `None` here means "this cell is not a view", and only that. A view asked for
/// an index it does not have answers `undefined` **rather than falling through to
/// the property lookup**, which is what the language says: an index-shaped key on
/// a typed array is never a property, so `a[99] = 1; a[99]` on a three-element
/// array is `undefined` and not `1`. Letting it fall through would make the read
/// find a property the write should never have created.
///
/// Both halves take a borrow they are handed. The caller is inside
/// [`super::computed::get_indexed`]'s, which already holds one.
pub(in crate::entry) fn indexed_get(context: &mut Context, cell: u32, key: Value) -> Option<u64> {
    let view = context.view_at(cell)?;
    let at = super::array::as_index(context, key)?;
    let absent = undefined_of(context);
    if at >= view.count() {
        return Some(absent);
    }
    let kind = view.kind;
    let bytes = window(context, &view)?;
    // A bigint element takes the word straight, where a numeric one goes
    // through the double the codec speaks in. The two paths meet at the same
    // gathering, so they cannot disagree about byte order.
    if kind.is_bigint() {
        let word = element::word_at(bytes, at * kind.size(), kind, true)?;
        // The read is finished with the bytes, which is what lets the borrow
        // become mutable — allocating the digits needs it.
        return Some(bigint_value(context, word, kind));
    }
    Some(match element::read(bytes, at * kind.size(), kind, true) {
        Some(number) => Value::from_f64(number).bits(),
        None => absent,
    })
}

/// `t[i] = v` — whether the write was a view's and is finished.
///
/// `true` for any index-shaped key on a view, including one out of range: the
/// write is dropped and no property is created, which is the counterpart of the
/// read above and the reason both are needed or neither is.
pub(in crate::entry) fn indexed_set(
    context: &mut Context,
    cell: u32,
    key: Value,
    value: u64,
) -> bool {
    let Some(view) = context.view_at(cell) else {
        return false;
    };
    let Some(at) = super::array::as_index(context, key) else {
        return false;
    };
    let kind = view.kind;
    // Read before the mutable borrow, because both halves need the context: a
    // bigint's digits are in the slab and a number's conversion may read a
    // string's text.
    //
    // `None` is a value the element refuses, in either direction. Answering true
    // still — the key was a view's, so no property is created — and the element
    // keeps what it held.
    let Some(word) = element_word(context, value, kind) else {
        return true;
    };
    if let Some(bytes) = window_mut(context, &view) {
        element::write_word(bytes, at * kind.size(), kind, word, true);
    }
    true
}

/// The value a bigint element reads as, from the word it holds.
///
/// # Why the two kinds differ only here
///
/// The bytes are the same bytes: `BigInt64Array` and `BigUint64Array` over one
/// buffer see one bit pattern, and which of them is looked at decides only
/// whether the top bit means a sign. So the sign lives in this function and in
/// [`bigint_word`] beside it, and nowhere else — the codec gathers and orders
/// bytes without knowing either class exists.
pub(in crate::entry) fn bigint_value(context: &mut Context, word: u64, kind: Kind) -> u64 {
    let held = match kind.is_signed() {
        true => crate::bigint::BigInt::from_i64(word as i64),
        // Not `from_i64(word as i64)`, which answers a negative value for
        // everything at or above 2^63 — precisely the half of the range this
        // class exists to hold.
        false => crate::bigint::BigInt::from_u64(word),
    };
    context.bigint_value(held)
}

/// The word a value stores into a bigint element, if it is a bigint at all.
///
/// `None` for a number, which the language refuses in both directions: a bigint
/// element takes a bigint and nothing else. This engine cannot raise the
/// `TypeError` that refusal is, so the write is dropped — which leaves the
/// element holding what it held, where coercing would make a program no other
/// engine accepts run and answer something.
///
/// The value is wrapped to sixty-four bits first, because that is what a store
/// into a fixed width IS: `BigInt64Array` given `2n ** 64n` stores zero, exactly
/// as `Int8Array` given 256 stores zero.
pub(in crate::entry) fn bigint_word(context: &Context, value: u64, kind: Kind) -> Option<u64> {
    let held = super::bigints::digits_of(context, value)?;
    held.wrap_to_bits(64, kind.is_signed()).as_u64().or_else(|| {
        // Signed and negative: the wrap answered the value rather than its bit
        // pattern, so the two's complement is taken here. `as_i64` is exact for
        // everything `wrap_to_bits(64, true)` can produce.
        held.wrap_to_bits(64, true).as_i64().map(|signed| signed as u64)
    })
}

/// The word a value stores into an element of any kind.
///
/// `None` is a value the destination **refuses**, and the refusal runs in both
/// directions: a number written into a bigint element is a `TypeError` in the
/// language, and so is a bigint written into a numeric one. The second half is
/// the one an implementation forgets, because the numeric path has a conversion
/// that will happily answer for anything — `as_number` of a bigint is `NaN`, and
/// `NaN` stores as zero. So a `Uint8Array` given `9n` would have been quietly
/// zeroed rather than left alone.
///
/// One function for both families, because "does this value belong in this
/// element" is one question and answering it in two places is how one of them
/// comes to answer it only one way round.
pub(in crate::entry) fn element_word(context: &Context, value: u64, kind: Kind) -> Option<u64> {
    if kind.is_bigint() {
        return bigint_word(context, value, kind);
    }
    if super::bigints::digits_of(context, value).is_some() {
        return None;
    }
    // `as_number` rather than the entry point: it is handed the borrow the
    // caller already holds, where `class_support::to_number` would take a second
    // one and abort the process.
    let number = super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN);
    Some(element::to_bits(number, kind))
}

