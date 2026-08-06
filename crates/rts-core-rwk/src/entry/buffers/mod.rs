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
//! A `DataView` has no `getBigInt64` here. It would need the same split at every
//! one of its sixteen methods and nothing has asked; the class-shaped spelling
//! reaches the same bytes.

mod array_buffer;
mod data_view;
pub(in crate::entry) mod element;
pub(in crate::entry) mod typed;
mod typed_classes;

pub(in crate::entry) use array_buffer::register_array_buffer;
pub(in crate::entry) use data_view::register_data_view;
// The eight wrappers rather than the eight names `#[rtse::class]` derives: each
// also installs `BYTES_PER_ELEMENT` on the constructor, which is a property the
// language puts on both halves and the attribute can only put on one.
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
    register_array_buffer(context);
    if let Some(prototype) = super::class_support::prototype(context, "ArrayBuffer") {
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
}

/// A length argument as a count of bytes.
///
/// Negative, fractional and non-finite all become zero rather than an error: the
/// language throws a `RangeError` for a negative one, which this engine cannot
/// raise where a handler could catch it — the same stated gap every refusal in
/// this layer settles on.
pub(in crate::entry) fn as_count(length: f64) -> usize {
    match length.is_finite() && length > 0.0 {
        true => length.trunc() as usize,
        false => 0,
    }
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
    stamp(context, cell, "byteLength", view.length as f64);
    stamp(context, cell, "byteOffset", view.offset as f64);
    if view.kind != Kind::Raw {
        stamp(context, cell, "length", view.count() as f64);
    }
    let key = context.well_known("buffer");
    let buffer = Value::from_slot(view.buffer).bits();
    super::objects::put(context, cell, key, buffer);
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

/// `undefined`, from outside a borrow.
pub(in crate::entry) fn undefined() -> u64 {
    with_current(|context| undefined_of(context))
}

/// An argument that may have been left off, as a number.
///
/// **Called at the top of a member body, never inside a borrow.** Both halves
/// take one of their own, and a second borrow of the `RefCell` panics inside an
/// `extern "C"` frame that cannot unwind — so the process aborts rather than
/// failing a test.
///
/// `None` for an absent argument rather than `NaN`, because the two mean opposite
/// things at a range boundary: `t.slice(0)` ends at the length and
/// `t.slice(0, NaN)` ends at zero. Coercing first and testing for `NaN` would
/// merge them.
pub(in crate::entry) fn optional_number(value: u64) -> Option<f64> {
    if value == undefined() {
        return None;
    }
    Some(super::class_support::to_number(value))
}

/// A `[begin, end)` pair over `count` items, with the language's clamping.
///
/// A negative index counts from the end, everything is clamped into range, and an
/// empty range comes back as `begin == end` rather than as `None` — the callers
/// all want to produce something empty rather than to refuse.
pub(in crate::entry) fn range(count: usize, begin: Option<f64>, end: Option<f64>) -> (usize, usize) {
    let first = relative(begin.unwrap_or(0.0), count);
    let last = match end {
        Some(number) => relative(number, count),
        None => count,
    };
    (first, last.max(first))
}

/// One relative index, resolved against a length.
fn relative(index: f64, count: usize) -> usize {
    if !index.is_finite() {
        // ToIntegerOrInfinity: -Infinity is before the start, +Infinity past the
        // end, and NaN is zero.
        return match (index.is_nan(), index > 0.0) {
            (true, _) => 0,
            (false, true) => count,
            (false, false) => 0,
        };
    }
    let index = index.trunc();
    match index < 0.0 {
        true => ((count as f64) + index).max(0.0) as usize,
        false => (index as usize).min(count),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_negative_bound_counts_from_the_end_and_never_past_the_start() {
        assert_eq!(range(8, Some(-3.0), None), (5, 8));
        assert_eq!(range(8, Some(-99.0), Some(2.0)), (0, 2));
        // An inverted range is empty, not reversed: `t.slice(5, 2)` answers
        // nothing rather than three elements backwards.
        assert_eq!(range(8, Some(5.0), Some(2.0)), (5, 5));
    }

    #[test]
    fn an_absent_end_is_the_length_and_an_explicit_nan_is_zero() {
        // The distinction `optional_number` exists to keep: these two are not
        // the same call, and a coercion that answered `NaN` for both would make
        // `t.slice(0)` empty.
        assert_eq!(range(4, Some(0.0), None), (0, 4));
        assert_eq!(range(4, Some(0.0), Some(f64::NAN)), (0, 0));
    }
}
