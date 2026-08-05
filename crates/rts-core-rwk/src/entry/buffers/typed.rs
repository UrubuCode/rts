//! What all eight typed arrays do, written once.
//!
//! # Why one implementation and not eight
//!
//! They differ in the element type and in nothing else. Eight copies is the
//! duplication `#[rtse::class]` exists to remove one level up, and it is worse
//! than volume: eight copies is where one of them clamps differently, or forgets
//! that `subarray` shares bytes while `slice` copies them. Every member here
//! takes a [`Kind`] and the eight class declarations in
//! [`super::typed_classes`] are one line each over it.
//!
//! # Where the borrow is taken
//!
//! Each member takes exactly one, at the top, and drops it before answering.
//! Nothing here calls an entry point or user code — the element conversions go
//! through [`crate::entry::operators::as_number`], which takes a borrow it is
//! *given* rather than one it takes. That is what keeps a second `with_current`
//! out of these bodies, and a second one panics inside an `extern "C"` frame that
//! cannot unwind, so the process aborts rather than failing a test.

use super::element::Kind;
use super::{Context, View, with_current};
use crate::entry::objects::undefined_of;
use crate::value::Value;

/// `new T(lengthOrArrayOrBuffer, byteOffset?, length?)`.
///
/// The three forms are told apart by what the first argument *is*, which is what
/// the language does: a number is a length, a buffer is memory to view, and
/// anything with elements is data to copy. A fourth case — another typed array —
/// copies its elements, converting through the destination's kind, so
/// `new Int8Array(new Uint8Array([200]))` holds `-56` rather than `200`.
pub(in crate::entry) fn construct(
    this: u64,
    kind: Kind,
    source: u64,
    offset: u64,
    length: u64,
) -> u64 {
    // Before any borrow: each of these takes one of its own.
    let offset = super::optional_number(offset);
    let length = super::optional_number(length);
    let absent = super::undefined();
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return undefined_of(context);
        };
        let size = kind.size();

        // Over an existing buffer: the one form that does not allocate memory,
        // and the only way two views come to share bytes.
        if let Some(over) = Value(source).as_slot()
            && let Some(bytes) = context.bytes_at(over).map(Vec::len)
        {
            let start = super::range(bytes, offset, None).0;
            let available = bytes - start;
            let count = match length {
                Some(asked) => (super::as_count(asked) * size).min(available),
                // The remainder of the buffer, rounded down to whole elements:
                // a five-byte buffer viewed as `Int32Array` holds one element,
                // not one and a quarter.
                None => (available / size) * size,
            };
            super::attach(
                context,
                cell,
                View {
                    buffer: over,
                    offset: start,
                    length: count,
                    kind,
                },
            );
            return Value::from_slot(cell).bits();
        }

        // From data, or from a length.
        let values = match source == absent {
            true => Vec::new(),
            false => numbers_of(context, source),
        };
        let count = match values.is_empty() {
            true => super::as_count(Value(source).numeric().unwrap_or(0.0)),
            false => values.len(),
        };
        let Some(buffer) = super::new_buffer(context, count * size) else {
            return undefined_of(context);
        };
        let view = View {
            buffer,
            offset: 0,
            length: count * size,
            kind,
        };
        if let Some(bytes) = super::window_mut(context, &view) {
            for (index, number) in values.iter().enumerate() {
                super::element::write(bytes, index * size, kind, *number, true);
            }
        }
        super::attach(context, cell, view);
        Value::from_slot(cell).bits()
    })
}

/// `t.at(i)` / `t.get(i)` — the element, or `undefined`.
///
/// A negative index counts from the end, which is `at`'s whole reason for
/// existing beside indexing. `get` is not a method the language has: it is here
/// because `t[0]` cannot reach the elements until
/// [`crate::entry::computed::get_indexed`] learns about views, and a class nothing
/// can read is a class nothing can test.
pub(in crate::entry) fn element_at(this: u64, index: f64, negatives: bool) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = super::view_of(context, this) else {
            return absent;
        };
        let Some(at) = resolve(&view, index, negatives) else {
            return absent;
        };
        let Some(bytes) = super::window(context, &view) else {
            return absent;
        };
        match super::element::read(bytes, at * view.kind.size(), view.kind, true) {
            Some(number) => Value::from_f64(number).bits(),
            None => absent,
        }
    })
}

/// `t.setAt(i, v)` — the value, because an assignment is an expression.
///
/// Out of range writes nothing and is not an error, which is what indexing a
/// typed array does: `a[99] = 1` on a three-element one neither grows it nor
/// creates a property.
pub(in crate::entry) fn store_at(this: u64, index: f64, value: f64) -> u64 {
    with_current(|context| {
        if let Some(view) = super::view_of(context, this)
            && let Some(at) = resolve(&view, index, false)
        {
            let kind = view.kind;
            if let Some(bytes) = super::window_mut(context, &view) {
                super::element::write(bytes, at * kind.size(), kind, value, true);
            }
        }
        Value::from_f64(value).bits()
    })
}

/// `t.set(source, offset?)` — copies elements in, converting each.
pub(in crate::entry) fn copy_from(this: u64, source: u64, offset: u64) -> u64 {
    let offset = super::optional_number(offset);
    with_current(|context| {
        let start = super::as_count(offset.unwrap_or(0.0));
        // Read out before anything is written: the source may be a view over the
        // very bytes about to be overwritten, and a copy is what makes an
        // overlapping `set` answer what a non-overlapping one would.
        let values = numbers_of(context, source);
        if let Some(view) = super::view_of(context, this) {
            let kind = view.kind;
            let size = kind.size();
            if let Some(bytes) = super::window_mut(context, &view) {
                for (index, number) in values.iter().enumerate() {
                    super::element::write(bytes, (start + index) * size, kind, *number, true);
                }
            }
        }
        undefined_of(context)
    })
}

/// `t.subarray(begin, end)` — another view over the SAME bytes.
///
/// The counterpart to [`slice`], which copies. Both exist because a caller means
/// one or the other, and an implementation that made them agree would be wrong
/// for whichever caller chose deliberately.
pub(in crate::entry) fn subarray(this: u64, begin: u64, end: u64) -> u64 {
    let begin = super::optional_number(begin);
    let end = super::optional_number(end);
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return undefined_of(context);
        };
        let (first, last) = super::range(view.count(), begin, end);
        let size = view.kind.size();
        made(
            context,
            View {
                buffer: view.buffer,
                offset: view.offset + first * size,
                length: (last - first) * size,
                kind: view.kind,
            },
        )
    })
}

/// `t.slice(begin, end)` — a copy, in a buffer of its own.
pub(in crate::entry) fn slice(this: u64, begin: u64, end: u64) -> u64 {
    let begin = super::optional_number(begin);
    let end = super::optional_number(end);
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = super::view_of(context, this) else {
            return absent;
        };
        let (first, last) = super::range(view.count(), begin, end);
        let size = view.kind.size();
        let Some(bytes) = super::window(context, &view) else {
            return absent;
        };
        // Copied out before the buffer is made, because allocating one takes the
        // byte store mutably and this slice borrows it.
        let taken = bytes[first * size..last * size].to_vec();
        let Some(buffer) = super::new_buffer(context, taken.len()) else {
            return absent;
        };
        let fresh = View {
            buffer,
            offset: 0,
            length: taken.len(),
            kind: view.kind,
        };
        if let Some(destination) = super::window_mut(context, &fresh) {
            destination.copy_from_slice(&taken);
        }
        made(context, fresh)
    })
}

/// `t.fill(value, begin, end)` — the array itself, so that calls chain.
pub(in crate::entry) fn fill(this: u64, value: f64, begin: u64, end: u64) -> u64 {
    let begin = super::optional_number(begin);
    let end = super::optional_number(end);
    with_current(|context| {
        if let Some(view) = super::view_of(context, this) {
            let (first, last) = super::range(view.count(), begin, end);
            let kind = view.kind;
            let size = kind.size();
            if let Some(bytes) = super::window_mut(context, &view) {
                for at in first..last {
                    super::element::write(bytes, at * size, kind, value, true);
                }
            }
        }
        this
    })
}

/// An index within a view, or `None` for one outside it.
fn resolve(view: &View, index: f64, negatives: bool) -> Option<usize> {
    if !index.is_finite() {
        return None;
    }
    let index = index.trunc();
    let count = view.count() as f64;
    let at = match negatives && index < 0.0 {
        true => index + count,
        false => index,
    };
    match at >= 0.0 && at < count {
        true => Some(at as usize),
        false => None,
    }
}

/// The numbers a value yields as source data: an array's elements, or another
/// view's.
///
/// An empty vector for anything else, including a number — which is what makes
/// `new T(8)` fall through to the length form rather than needing to be asked
/// about first.
fn numbers_of(context: &Context, source: u64) -> Vec<f64> {
    let Some(cell) = Value(source).as_slot() else {
        return Vec::new();
    };
    if let Some(view) = context.view_at(cell)
        && let Some(bytes) = super::window(context, &view)
    {
        let size = view.kind.size();
        return (0..view.count())
            .filter_map(|at| super::element::read(bytes, at * size, view.kind, true))
            .collect();
    }
    match context.elements_at(cell) {
        // `as_number` rather than the entry point: it is handed the borrow this
        // function already holds, where `class_support::to_number` would take a
        // second one and abort the process.
        Some(elements) => elements
            .iter()
            .map(|value| {
                crate::entry::operators::as_number(context, Value(*value)).unwrap_or(f64::NAN)
            })
            .collect(),
        None => Vec::new(),
    }
}

/// A new instance of the view's own class, over these bytes.
fn made(context: &mut Context, view: View) -> u64 {
    let Some(cell) = crate::entry::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = super::typed_classes::ensure(context, view.kind) {
        context.set_prototype(cell, prototype);
    }
    super::attach(context, cell, view);
    Value::from_slot(cell).bits()
}
