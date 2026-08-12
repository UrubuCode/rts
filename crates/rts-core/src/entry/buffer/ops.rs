//! What every `Buffer` member does, over [`super::codec`] and
//! [`super::super::buffers`] — the same split [`super::super::buffers::typed`]
//! is written over, and for the same reason: eight (here, one) declarations in
//! [`super::mod`] stay one line each, and the borrow discipline lives in
//! exactly one place.
//!
//! # Where the borrow is taken
//!
//! Every member takes exactly one, at the top, and drops it before answering.
//! An argument needing "was this omitted" — a begin/end/offset pair — is
//! converted through [`super::super::buffers::optional_number`] BEFORE the
//! borrow opens, because that helper (and [`super::super::class_support::to_number`]
//! beneath it) takes one of its own: calling it from inside an open borrow is
//! the second-borrow abort every native here is written to avoid.

use super::super::buffers::element::Kind;
use super::super::buffers::{View, as_count, optional_number, range, view_of, window, window_mut};
use super::super::modules::undefined_value;
use super::super::objects::undefined_of;
use super::super::{Context, native, with_current};
use super::codec;
use crate::value::Value;

/// A new `Buffer` instance, owning a copy of these bytes.
pub(in crate::entry) fn made(context: &mut Context, bytes: &[u8]) -> u64 {
    let Some(buffer) = super::super::buffers::new_buffer(context, bytes.len()) else {
        return undefined_of(context);
    };
    let view = View {
        buffer,
        offset: 0,
        length: bytes.len(),
        kind: Kind::Uint8,
    };
    if let Some(destination) = window_mut(context, &view) {
        destination.copy_from_slice(bytes);
    }
    instance_over(context, view)
}

/// A `Buffer` instance over an EXISTING view — what [`slice`]/[`subarray`]
/// answer, sharing bytes rather than copying them.
fn instance_over(context: &mut Context, view: View) -> u64 {
    let Some(cell) = native::plain(context) else {
        return undefined_of(context);
    };
    super::register_buffer(context);
    if let Some(prototype) = super::super::class_support::prototype(context, "Buffer") {
        context.set_prototype(cell, prototype);
    }
    super::super::buffers::attach(context, cell, view);
    Value::from_slot(cell).bits()
}

/// The bytes a value carries as source data: a `Uint8Array`/`Buffer`'s window,
/// a string encoded per `encoding`, or a JS array of numbers.
fn source_bytes(context: &Context, value: u64, encoding: &str) -> Option<Vec<u8>> {
    if let Some(view) = view_of(context, value) {
        return window(context, &view).map(<[u8]>::to_vec);
    }
    if let Some(cell) = Value(value).as_slot()
        && let Some(text) = context.text_at(cell)
    {
        return codec::encode(&text.to_rust()?, encoding);
    }
    if let Some(cell) = Value(value).as_slot()
        && let Some(elements) = context.elements_at(cell)
    {
        return Some(
            elements
                .iter()
                .map(|element| super::super::operators::as_number(context, Value(*element)).unwrap_or(0.0) as u8)
                .collect(),
        );
    }
    None
}

/// An `encoding` argument, defaulting to `"utf8"` for `undefined`.
fn encoding_arg(context: &Context, value: u64) -> String {
    if value == undefined_of(context) {
        return "utf8".to_owned();
    }
    super::super::text::to_text(context, Value(value))
        .and_then(|text| text.to_rust())
        .unwrap_or_else(|| "utf8".to_owned())
}

/// A byte, a string's encoded bytes, or a view's bytes — what
/// [`fill`]/[`index_of`]/[`includes`] search for or write.
fn pattern_of(context: &Context, value: u64, encoding: &str) -> Vec<u8> {
    match Value(value).as_slot() {
        Some(cell) => {
            if let Some(text) = context.text_at(cell) {
                return codec::encode(&text.to_rust().unwrap_or_default(), encoding).unwrap_or_default();
            }
            if let Some(view) = view_of(context, value) {
                return window(context, &view).map(<[u8]>::to_vec).unwrap_or_default();
            }
            Vec::new()
        }
        None => vec![super::super::operators::as_number(context, Value(value)).unwrap_or(0.0) as u8],
    }
}

// ---------------------------------------------------------------------------
// Statics
// ---------------------------------------------------------------------------

/// `Buffer.alloc(size, fill?, encoding?)`.
///
/// A negative `size` throws `RangeError` — Node's own answer — rather than
/// going through [`as_count`]'s zero-clamp. `as_count` clamping is right for an
/// offset or a length that can legitimately end up past the end of something and
/// mean "nothing left"; a negative *allocation size* is not that, it is a
/// program telling itself it wants -1 bytes, and answering an empty buffer
/// let it believe that worked. `.length` silently reading 0 instead of the
/// error a caller may be checking for (`node_buffer_allocneg.test.ts` does) is
/// the wrong-answer-over-throw case rule 8's neighbourhood exists to avoid.
pub(in crate::entry) fn alloc(size: f64, fill: u64, encoding: u64) -> u64 {
    if size.is_finite() && size < 0.0 {
        super::super::throw::range_error("The value is out of range. It must be >= 0.");
        return undefined_value();
    }
    with_current(|context| {
        let length = as_count(size);
        let absent = undefined_of(context);
        let pattern = if fill == absent {
            Vec::new()
        } else {
            let enc = encoding_arg(context, encoding);
            pattern_of(context, fill, &enc)
        };
        let bytes = repeated(&pattern, length);
        made(context, &bytes)
    })
}

/// `Buffer.allocUnsafe(size)` — zero-filled here (see the module doc).
///
/// Same negative-size refusal as [`alloc`], for the same reason.
pub(in crate::entry) fn alloc_unsafe(size: f64) -> u64 {
    if size.is_finite() && size < 0.0 {
        super::super::throw::range_error("The value is out of range. It must be >= 0.");
        return undefined_value();
    }
    with_current(|context| made(context, &vec![0u8; as_count(size)]))
}

/// `Buffer.from(source, encodingOrOffset?)`.
pub(in crate::entry) fn from(source: u64, encoding_or_offset: u64) -> u64 {
    with_current(|context| {
        let enc = encoding_arg(context, encoding_or_offset);
        let bytes = source_bytes(context, source, &enc).unwrap_or_default();
        made(context, &bytes)
    })
}

/// `Buffer.concat(list, totalLength?)`.
pub(in crate::entry) fn concat(list: u64, total_length: u64) -> u64 {
    let total_length = optional_number(total_length);
    with_current(|context| {
        let mut joined = Vec::new();
        if let Some(cell) = Value(list).as_slot()
            && let Some(elements) = context.elements_at(cell).cloned()
        {
            for element in elements {
                if let Some(bytes) = source_bytes(context, element, "utf8") {
                    joined.extend_from_slice(&bytes);
                }
            }
        }
        if let Some(wanted) = total_length {
            joined.resize(as_count(wanted), 0);
        }
        made(context, &joined)
    })
}

/// `Buffer.byteLength(source, encoding?)`.
pub(in crate::entry) fn byte_length(source: u64, encoding: u64) -> f64 {
    with_current(|context| {
        let enc = encoding_arg(context, encoding);
        source_bytes(context, source, &enc).map(|bytes| bytes.len() as f64).unwrap_or(0.0)
    })
}

/// `Buffer.isBuffer(value)` — whether it is a view at all (see the module
/// doc: nothing this runtime hands back that is not one).
pub(in crate::entry) fn is_buffer(value: u64) -> bool {
    with_current(|context| view_of(context, value).is_some())
}

/// `Buffer.isEncoding(name)`.
pub(in crate::entry) fn is_encoding(encoding: u64) -> bool {
    with_current(|context| {
        super::super::text::to_text(context, Value(encoding))
            .and_then(|text| text.to_rust())
            .is_some_and(|text| codec::canonical_encoding(&text).is_some())
    })
}

/// Bytewise comparison, shared by the static and the instance `compare`.
pub(in crate::entry) fn compare_values(a: u64, b: u64) -> f64 {
    with_current(|context| {
        let a = view_of(context, a).and_then(|view| window(context, &view)).map(<[u8]>::to_vec).unwrap_or_default();
        let b = view_of(context, b).and_then(|view| window(context, &view)).map(<[u8]>::to_vec).unwrap_or_default();
        match a.cmp(&b) {
            std::cmp::Ordering::Less => -1.0,
            std::cmp::Ordering::Equal => 0.0,
            std::cmp::Ordering::Greater => 1.0,
        }
    })
}

/// `pattern` repeated/truncated to exactly `length` bytes.
fn repeated(pattern: &[u8], length: usize) -> Vec<u8> {
    if pattern.is_empty() {
        return vec![0u8; length];
    }
    (0..length).map(|index| pattern[index % pattern.len()]).collect()
}

// ---------------------------------------------------------------------------
// Instance methods
// ---------------------------------------------------------------------------

/// `buf.toString(encoding?, start?, end?)`.
pub(in crate::entry) fn to_string(this: u64, encoding: u64, start: u64, end: u64) -> u64 {
    let start = optional_number(start);
    let end = optional_number(end);
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = view_of(context, this) else { return absent };
        let Some(bytes) = window(context, &view) else { return absent };
        let (first, last) = range(bytes.len(), start, end);
        let enc = encoding_arg(context, encoding);
        let text = codec::decode(&bytes[first..last], &enc);
        context.intern_value(crate::text::Str::from_str(&text)).bits()
    })
}

/// `buf.write(string, offset?, length?, encoding?)`.
pub(in crate::entry) fn write(this: u64, string: u64, offset: u64, length: u64, encoding: u64) -> f64 {
    let offset = optional_number(offset);
    let length = optional_number(length);
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return 0.0 };
        let Some(cell) = Value(string).as_slot() else { return 0.0 };
        let Some(text) = context.text_at(cell).and_then(|text| text.to_rust()) else { return 0.0 };
        let enc = encoding_arg(context, encoding);
        let bytes = codec::encode(&text, &enc).unwrap_or_default();
        let start = as_count(offset.unwrap_or(0.0)).min(view.count());
        let cap = length.map(as_count).unwrap_or(view.count() - start);
        let count = bytes.len().min(cap).min(view.count() - start);
        if let Some(destination) = window_mut(context, &view) {
            destination[start..start + count].copy_from_slice(&bytes[..count]);
        }
        count as f64
    })
}

/// `buf.slice(begin?, end?)` / `buf.subarray(begin?, end?)` — a Buffer
/// SHARING the same bytes, which is Node's own divergence from
/// `TypedArray.prototype.slice`'s copy (see the module doc on `mod.rs`).
pub(in crate::entry) fn windowed(this: u64, begin: u64, end: u64) -> u64 {
    let begin = optional_number(begin);
    let end = optional_number(end);
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = view_of(context, this) else { return absent };
        let (first, last) = range(view.count(), begin, end);
        let size = view.kind.size();
        instance_over(
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

/// `buf.equals(other)`.
pub(in crate::entry) fn equals(this: u64, other: u64) -> bool {
    compare_values(this, other) == 0.0
}

/// `buf.copy(target, targetStart?, sourceStart?, sourceEnd?)`.
pub(in crate::entry) fn copy(this: u64, target: u64, target_start: u64, source_start: u64, source_end: u64) -> f64 {
    let target_start = optional_number(target_start);
    let source_start = optional_number(source_start);
    let source_end = optional_number(source_end);
    with_current(|context| {
        let Some(source) = view_of(context, this) else { return 0.0 };
        let Some(destination) = view_of(context, target) else { return 0.0 };
        let (first, last) = range(source.count(), source_start, source_end);
        let Some(bytes) = window(context, &source) else { return 0.0 };
        let taken: Vec<u8> = bytes[first..last].to_vec();
        let start = as_count(target_start.unwrap_or(0.0)).min(destination.count());
        let count = taken.len().min(destination.count() - start);
        if let Some(into) = window_mut(context, &destination) {
            into[start..start + count].copy_from_slice(&taken[..count]);
        }
        count as f64
    })
}

/// `buf.fill(value, begin?, end?, encoding?)`.
pub(in crate::entry) fn fill(this: u64, value: u64, begin: u64, end: u64, encoding: u64) -> u64 {
    let begin = optional_number(begin);
    let end = optional_number(end);
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return this };
        let (first, last) = range(view.count(), begin, end);
        let enc = encoding_arg(context, encoding);
        let pattern = pattern_of(context, value, &enc);
        if !pattern.is_empty()
            && let Some(bytes) = window_mut(context, &view)
        {
            for (offset, at) in (first..last).enumerate() {
                bytes[at] = pattern[offset % pattern.len()];
            }
        }
        this
    })
}

/// The first index at or after `from` where `needle` occurs in `haystack`, or
/// `None`. An empty needle matches at `from` itself, the way `String.indexOf`
/// treats one.
fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return (from <= haystack.len()).then_some(from);
    }
    if from >= haystack.len() {
        return None;
    }
    // Two-way with a SIMD prefilter, where the window compare this replaces was
    // O(n*m). The same swap `entry::string::basic` took, and this one is the
    // case that wants it most: a `Buffer` is bytes by construction and is
    // routinely megabytes, where a string in this engine is usually short.
    memchr::memmem::find(&haystack[from..], needle).map(|position| position + from)
}

/// `buf.indexOf(value, byteOffset?, encoding?)`.
pub(in crate::entry) fn index_of(this: u64, value: u64, byte_offset: u64, encoding: u64) -> f64 {
    let byte_offset = optional_number(byte_offset);
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return -1.0 };
        let Some(bytes) = window(context, &view) else { return -1.0 };
        let enc = encoding_arg(context, encoding);
        let needle = pattern_of(context, value, &enc);
        let (from, _) = range(bytes.len(), byte_offset, None);
        find(bytes, &needle, from).map(|at| at as f64).unwrap_or(-1.0)
    })
}

/// `buf.includes(value, byteOffset?, encoding?)`.
pub(in crate::entry) fn includes(this: u64, value: u64, byte_offset: u64, encoding: u64) -> bool {
    index_of(this, value, byte_offset, encoding) >= 0.0
}

/// `buf.toJSON()` — `{ type: "Buffer", data: [...] }`.
pub(in crate::entry) fn to_json(this: u64) -> u64 {
    with_current(|context| {
        let absent = undefined_of(context);
        let Some(view) = view_of(context, this) else { return absent };
        let bytes = window(context, &view).map(<[u8]>::to_vec).unwrap_or_default();
        let Some(cell) = native::plain(context) else { return absent };
        let type_value = context.intern_value(crate::text::Str::from_str("Buffer")).bits();
        let type_key = context.well_known("type");
        super::super::objects::put(context, cell, type_key, type_value);
        let data = bytes.iter().map(|byte| Value::from_f64(f64::from(*byte)).bits()).collect();
        let data_value = super::super::array::built_in(context, data);
        let data_key = context.well_known("data");
        super::super::objects::put(context, cell, data_key, data_value);
        Value::from_slot(cell).bits()
    })
}

// ---------------------------------------------------------------------------
// The numeric family — every read and write goes through `buffers::element`,
// the same codec the typed arrays use. See that module's doc for why writing
// wraps and why the endianness gathering is written once.
// ---------------------------------------------------------------------------

/// One element, read at a byte offset.
pub(in crate::entry) fn read_num(this: u64, offset: f64, kind: Kind, little: bool) -> f64 {
    with_current(|context| {
        let Some(view) = view_of(context, this) else { return f64::NAN };
        let Some(bytes) = window(context, &view) else { return f64::NAN };
        super::super::buffers::element::read(bytes, as_count(offset), kind, little).unwrap_or(f64::NAN)
    })
}

/// One element, written at a byte offset. Answers `offset + the element's
/// width`, which is what Node's writes answer on success — and, here, always:
/// an out-of-range write is dropped rather than thrown, the stand-in this
/// layer states everywhere it cannot raise.
pub(in crate::entry) fn write_num(this: u64, value: f64, offset: f64, kind: Kind, little: bool) -> f64 {
    with_current(|context| {
        let at = as_count(offset);
        if let Some(view) = view_of(context, this)
            && let Some(bytes) = window_mut(context, &view)
        {
            super::super::buffers::element::write(bytes, at, kind, value, little);
        }
        offset + kind.size() as f64
    })
}
