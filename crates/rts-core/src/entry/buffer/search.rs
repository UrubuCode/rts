//! Search bounds and byte-search primitives shared by Buffer index methods.
//!
//! The forward and reverse methods have different default bounds, but the
//! needle encoding overload and the actual byte search are the same questions.
//! Keeping them here prevents the instance-method module from crossing the
//! 500-line boundary while preserving the SIMD-backed `memchr` implementation.

use super::super::buffers::optional_number;
use crate::entry;
use crate::value::Value;

fn is_text_value(value: u64) -> bool {
    entry::with_runtime(|context| {
        Value(value)
            .as_slot()
            .and_then(|cell| context.text_at(cell))
            .is_some()
    })
}

/// The first index at or after `from` where `needle` occurs in `haystack`, or
/// `None`. An empty needle matches at `from` itself, the way `String.indexOf`
/// treats one.
pub(in crate::entry) fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
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

/// The last index at or before `from` where `needle` occurs in `haystack`.
pub(in crate::entry) fn find_last(
    haystack: &[u8],
    needle: &[u8],
    from: usize,
) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(haystack.len()));
    }
    if needle.len() > haystack.len() {
        return None;
    }
    let start = from.min(haystack.len() - needle.len());
    let end = start + needle.len();
    memchr::memmem::rfind(&haystack[..end], needle)
}

/// Detects Node's overload where a textual second argument is the encoding.
pub(in crate::entry) fn search_arguments(
    byte_offset: u64,
    encoding: u64,
) -> (Option<f64>, bool) {
    // The native ABI fills omitted trailing slots with either singleton; an
    // explicit text in the offset slot is still the unambiguous encoding form.
    let encoding_in_offset = (encoding == entry::undefined_value()
        || encoding == entry::null_value())
        && entry::number_of(byte_offset).is_none()
        && is_text_value(byte_offset);
    let offset = if encoding_in_offset {
        None
    } else {
        optional_number(byte_offset)
    };
    (offset, encoding_in_offset)
}

/// Forward search on two-byte code-unit boundaries.
pub(in crate::entry) fn find_utf16(
    haystack: &[u8],
    needle: &[u8],
    from: usize,
) -> Option<usize> {
    if needle.is_empty() {
        return find(haystack, needle, from);
    }
    let mut from = from.saturating_add(from % 2);
    while let Some(found) = find(haystack, needle, from) {
        if found % 2 == 0 {
            return Some(found);
        }
        from = found + 1;
    }
    None
}

/// Reverse search on two-byte code-unit boundaries.
pub(in crate::entry) fn find_last_utf16(
    haystack: &[u8],
    needle: &[u8],
    from: usize,
) -> Option<usize> {
    if needle.is_empty() {
        return find_last(haystack, needle, from);
    }
    let mut from = from - from % 2;
    loop {
        let found = find_last(haystack, needle, from)?;
        if found % 2 == 0 {
            return Some(found);
        }
        if found == 0 {
            return None;
        }
        from = found - 1;
    }
}

/// Converts the optional last-search bound, retaining negative infinity as no
/// possible index rather than clamping it to the first byte.
pub(in crate::entry) fn last_from(count: usize, offset: Option<f64>) -> Option<usize> {
    let Some(offset) = offset else { return Some(count) };
    if offset.is_nan() {
        return Some(count);
    }
    if offset == f64::NEG_INFINITY {
        return None;
    }
    if offset == f64::INFINITY {
        return Some(count);
    }
    let offset = offset.trunc();
    if offset < 0.0 {
        let from_end = count as f64 + offset;
        (from_end >= 0.0).then_some(from_end as usize)
    } else {
        Some((offset as usize).min(count))
    }
}
