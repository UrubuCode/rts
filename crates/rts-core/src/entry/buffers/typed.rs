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
            false => words_of(context, source, kind),
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
            for (index, word) in values.iter().enumerate() {
                super::element::write_word(bytes, index * size, kind, *word, true);
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
        let kind = view.kind;
        let Some(bytes) = super::window(context, &view) else {
            return absent;
        };
        // The same split `indexed_get` makes, and it has to be made twice
        // because a program reaches an element both ways: a bigint element is
        // the word, where a numeric one is the double the codec speaks in.
        if kind.is_bigint() {
            return match super::element::word_at(bytes, at * kind.size(), kind, true) {
                Some(word) => super::bigint_value(context, word, kind),
                None => absent,
            };
        }
        match super::element::read(bytes, at * kind.size(), kind, true) {
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
/// The value is taken as it arrived rather than as a number, because a bigint
/// element takes a bigint: coercing at the boundary would have made the two new
/// classes unwritable through this spelling while `t[i] = v` worked.
pub(in crate::entry) fn store_at(this: u64, index: f64, value: u64) -> u64 {
    with_current(|context| {
        if let Some(view) = super::view_of(context, this)
            && let Some(at) = resolve(&view, index, false)
        {
            let kind = view.kind;
            // `element_word` rather than `word_of`: a refused value leaves this
            // element alone, where the bulk paths write a zero. The difference
            // is the one `word_of` documents, and this is the side of it that
            // has something to leave.
            if let Some(word) = super::element_word(context, value, kind)
                && let Some(bytes) = super::window_mut(context, &view)
            {
                super::element::write_word(bytes, at * kind.size(), kind, word, true);
            }
        }
        value
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
        let values = match super::view_of(context, this) {
            Some(view) => words_of(context, source, view.kind),
            None => Vec::new(),
        };
        if let Some(view) = super::view_of(context, this) {
            let kind = view.kind;
            let size = kind.size();
            if let Some(bytes) = super::window_mut(context, &view) {
                for (index, word) in values.iter().enumerate() {
                    super::element::write_word(bytes, (start + index) * size, kind, *word, true);
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

/// `t.fill(value, begin, end)` — the array itself, so that calls chain.
pub(in crate::entry) fn fill(this: u64, value: u64, begin: u64, end: u64) -> u64 {
    let begin = super::optional_number(begin);
    let end = super::optional_number(end);
    with_current(|context| {
        if let Some(view) = super::view_of(context, this) {
            let (first, last) = super::range(view.count(), begin, end);
            let kind = view.kind;
            let size = kind.size();
            // Converted once, before the loop: the value does not change per
            // element, and a bigint's conversion reads the digit slab.
            let word = word_of(context, value, kind);
            if let Some(bytes) = super::window_mut(context, &view) {
                for at in first..last {
                    super::element::write_word(bytes, at * size, kind, word, true);
                }
            }
        }
        this
    })
}

/// Every element of a view, as the JS values they read as — the same answer
/// [`element_at`] gives one at a time, gathered for whoever walks the whole
/// view: `for`-`of`, `.values()`/`.keys()`/`.entries()`, `.includes()`.
///
/// `None` only for "not a view". An empty view answers `Some(vec![])`, which
/// keeps a caller from having to tell "no view" and "zero elements" apart.
pub(in crate::entry) fn elements(context: &mut Context, view: &View) -> Vec<u64> {
    let kind = view.kind;
    let size = kind.size();
    let Some(bytes) = super::window(context, view) else {
        return Vec::new();
    };
    // Read the words out first: a bigint element's conversion allocates,
    // which needs the borrow this slice is holding to end first.
    let words: Vec<Option<u64>> = (0..view.count())
        .map(|at| match kind.is_bigint() {
            true => super::element::word_at(bytes, at * size, kind, true),
            false => super::element::read(bytes, at * size, kind, true)
                .map(|number| Value::from_f64(number).bits()),
        })
        .collect();
    words
        .into_iter()
        .map(|word| match (kind.is_bigint(), word) {
            (true, Some(word)) => super::bigint_value(context, word, kind),
            (_, Some(value)) => value,
            (_, None) => Value::from_f64(0.0).bits(),
        })
        .collect()
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

/// The **words** a value yields as source data, already in the destination's
/// element form: an array's elements, or another view's.
///
/// # Why words and not numbers
///
/// It was `Vec<f64>`, and that was right while every element fitted in a double.
/// `BigInt64Array` is the width where it stops: 2^63 − 1 is not a double, so a
/// copy through one would round, silently, in exactly the range the class exists
/// for.
///
/// A word is what an element IS at every width, so converting once here rather
/// than at each write also puts the destination's conversion rule in one place —
/// which is what lets a source of one kind and a destination of another be one
/// question rather than a table.
///
/// An empty vector for anything else, including a number, which is what makes
/// `new T(8)` fall through to the length form rather than being asked about
/// first.
fn words_of(context: &Context, source: u64, kind: Kind) -> Vec<u64> {
    let Some(cell) = Value(source).as_slot() else {
        return Vec::new();
    };
    if let Some(view) = context.view_at(cell)
        && let Some(bytes) = super::window(context, &view)
    {
        let size = view.kind.size();
        // A view of matching bigint-ness copies the WORD, and one of the other
        // kind copies nothing: the language refuses a numeric element written
        // into a bigint one in both directions, and there is no double that
        // could carry the conversion honestly anyway. Between two bigint kinds
        // the bits are the value, so signed-to-unsigned is the reinterpretation
        // the spec's wrap already produces.
        if view.kind.is_bigint() != kind.is_bigint() {
            return Vec::new();
        }
        return (0..view.count())
            .filter_map(|at| match kind.is_bigint() {
                true => super::element::word_at(bytes, at * size, view.kind, true),
                false => super::element::read(bytes, at * size, view.kind, true)
                    .map(|number| super::element::to_bits(number, kind)),
            })
            .collect();
    }
    match context.elements_at(cell) {
        Some(elements) => elements
            .iter()
            .map(|value| word_of(context, *value, kind))
            .collect(),
        None => Vec::new(),
    }
}

/// One value as the word a destination element holds, or zero if it refuses it.
///
/// The refusal is [`super::element_word`]'s and is not restated here. What this
/// adds is what a **bulk** copy does with one: zero, where an element-at-a-time
/// write leaves the old value. They differ because a bulk copy is building bytes
/// that did not exist, so there is nothing to leave — and a fresh
/// `BigInt64Array` full of zeros is what a program sees either way.
///
/// Reachable from the sibling modules because a sort writing an ordered run
/// back is the same bulk write: the values came out of this very view, so the
/// refusal cannot fire, and a second `unwrap_or` beside it would be the place
/// the two answers came to differ.
pub(super) fn word_of(context: &Context, value: u64, kind: Kind) -> u64 {
    super::element_word(context, value, kind).unwrap_or(0)
}

/// `t.includes(search, from)` — strict equality, like `Array.prototype`'s,
/// except `NaN` matches itself here as it does there for `includes` (unlike
/// `indexOf`).
pub(in crate::entry) fn includes(this: u64, search: u64, from: u64) -> bool {
    let from = super::optional_number(from);
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return false;
        };
        let values = elements(context, &view);
        let start = match from {
            Some(asked) if asked < 0.0 => {
                (values.len() as f64 + asked).max(0.0) as usize
            }
            Some(asked) => asked as usize,
            None => 0,
        };
        values
            .get(start..)
            .unwrap_or_default()
            .iter()
            .any(|held| crate::value::same_value_zero(Value(*held), Value(search), |a, b| context.same_text(a, b)))
    })
}

/// `t.indexOf(search, from)` — the first index holding it, or `-1`.
///
/// Strict equality rather than [`includes`]'s SameValueZero, and the difference
/// is the whole reason both exist: `new Float64Array([NaN]).includes(NaN)` is
/// `true` and `.indexOf(NaN)` is `-1`. A single implementation serving both
/// would have to pick one, and whichever it picked would be wrong for the other
/// on exactly that input.
///
/// A typed array has no holes, so there is no skipping rule to mirror from
/// `Array.prototype` — every index in range is a property.
pub(in crate::entry) fn index_of(this: u64, search: u64, from: u64) -> f64 {
    let from = super::optional_number(from);
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return -1.0;
        };
        let values = elements(context, &view);
        let start = relative_start(from, values.len());
        for (at, held) in values.iter().enumerate().skip(start) {
            if crate::value::strict_equals(Value(*held), Value(search), |a, b| context.same_text(a, b)) {
                return at as f64;
            }
        }
        -1.0
    })
}

/// `t.lastIndexOf(search, from)` — the last index holding it, or `-1`.
///
/// The backwards walk clamps its `from` differently from [`index_of`], which is
/// the asymmetry `Array.prototype` has and the reason this is not the forward
/// scan reversed: a negative `from` past the start makes `lastIndexOf` find
/// nothing at all, where `indexOf` clamps to zero and searches everything.
pub(in crate::entry) fn last_index_of(this: u64, search: u64, from: u64) -> f64 {
    let from = super::optional_number(from);
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return -1.0;
        };
        let values = elements(context, &view);
        let count = values.len() as f64;
        let last = match from {
            Some(asked) if asked.is_nan() => return -1.0,
            Some(asked) if asked < 0.0 => {
                let shifted = count + asked;
                if shifted < 0.0 {
                    return -1.0;
                }
                shifted as usize
            }
            Some(asked) => (asked as usize).min(values.len().saturating_sub(1)),
            None => values.len().saturating_sub(1),
        };
        for at in (0..=last).rev() {
            let Some(held) = values.get(at) else { continue };
            if crate::value::strict_equals(Value(*held), Value(search), |a, b| context.same_text(a, b)) {
                return at as f64;
            }
        }
        -1.0
    })
}

/// `t.join(separator)` — the elements as text, separated.
///
/// An absent separator is a comma, and — unlike `Array.prototype.join` — there
/// is no `null`/`undefined`-becomes-empty rule to apply, because a typed array's
/// elements are numbers and every one of them has a text form.
pub(in crate::entry) fn join(this: u64, separator: u64) -> u64 {
    let apart = with_current(|context| match super::super::string::absent(context, separator) {
        true => ",".to_owned(),
        false => super::super::string::text_of(context, separator)
            .and_then(|text| text.to_rust())
            .unwrap_or_else(|| ",".to_owned()),
    });
    with_current(|context| {
        let Some(view) = super::view_of(context, this) else {
            return undefined_of(context);
        };
        let joined = elements(context, &view)
            .into_iter()
            .map(|held| {
                Value(held)
                    .numeric()
                    .map(|number| {
                        crate::coerce::number_to_string(number)
                            .to_rust()
                            .unwrap_or_default()
                    })
                    .unwrap_or_default()
            })
            .collect::<Vec<String>>()
            .join(&apart);
        context
            .intern_value(crate::text::Str::from_str(&joined))
            .bits()
    })
}

/// The index a forward scan starts at, given the argument as written.
///
/// Shared by [`index_of`] so the clamping is stated once; `last_index_of` walks
/// the other way and clamps differently, which is why it does not call this.
fn relative_start(from: Option<f64>, count: usize) -> usize {
    match from {
        Some(asked) if asked.is_nan() => 0,
        Some(asked) if asked < 0.0 => (count as f64 + asked).max(0.0) as usize,
        Some(asked) => asked as usize,
        None => 0,
    }
}

/// `t.values()` — an iterator over the elements.
///
/// # Why the borrow ends before `over`
///
/// `list_iterator::over` takes its own [`with_current`] borrow to build the
/// iterator object, and this function's own borrow — taken to read the
/// view's bytes — has to be gone before that call or the second `RefCell`
/// borrow aborts the process: a `with_current` inside a `with_current` is
/// exactly the shape rule 8's neighbouring rule (the crate doc's borrow
/// discipline) forbids, and there is no unwinding across this boundary to
/// turn the mistake into a panic that could be caught.
pub(in crate::entry) fn values(this: u64) -> u64 {
    let Some(elements) = with_current(|context| {
        super::view_of(context, this).map(|view| elements(context, &view))
    }) else {
        return with_current(|context| undefined_of(context));
    };
    let listed = with_current(|context| crate::entry::array::built_in(context, elements));
    crate::entry::list_iterator::over(listed, "Array Iterator")
}

/// `t.keys()` — an iterator over the indices.
pub(in crate::entry) fn keys(this: u64) -> u64 {
    let Some(count) = with_current(|context| super::view_of(context, this).map(|view| view.count())) else {
        return with_current(|context| undefined_of(context));
    };
    let indices = (0..count).map(|at| Value::from_f64(at as f64).bits()).collect();
    let listed = with_current(|context| crate::entry::array::built_in(context, indices));
    crate::entry::list_iterator::over(listed, "Array Iterator")
}

/// `t.entries()` — an iterator over `[index, element]` pairs.
pub(in crate::entry) fn entries(this: u64) -> u64 {
    let Some(values) = with_current(|context| {
        super::view_of(context, this).map(|view| elements(context, &view))
    }) else {
        return with_current(|context| undefined_of(context));
    };
    let pairs: Vec<u64> = values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            with_current(|context| {
                crate::entry::array::built_in(context, vec![Value::from_f64(index as f64).bits(), value])
            })
        })
        .collect();
    let listed = with_current(|context| crate::entry::array::built_in(context, pairs));
    crate::entry::list_iterator::over(listed, "Array Iterator")
}

/// A new instance of the view's own class, over these bytes.
pub(in crate::entry) fn made(context: &mut Context, view: View) -> u64 {
    let Some(cell) = crate::entry::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = super::typed_classes::ensure(context, view.kind) {
        context.set_prototype(cell, prototype);
    }
    super::attach(context, cell, view);
    Value::from_slot(cell).bits()
}
