//! The order a module DECLARES its exports in.
//!
//! # Why this is read from the bytes rather than asked of the engine
//!
//! `wasmi` holds a module's exports in a sorted map, so `Module::exports()`
//! answers them alphabetically — `add, memory, mul` for a module that declares
//! `add, mul, memory`. The JS-API's order is declaration order, and it is
//! observable twice: in the array `WebAssembly.Module.exports` answers, and in
//! `Object.keys(instance.exports)`.
//!
//! Measured under node 22: the array comes back `add, mul, memory`. Sorting our
//! answer alphabetically and calling it the spec's would be a wrong answer a
//! fixture cannot see unless it asks about a module whose declaration order is
//! not alphabetical — which is exactly why the fixture's does not sort.
//!
//! So this reads the export section itself. It is a scanner over section
//! headers and one length-prefixed name per entry, not a validator: anything it
//! cannot make sense of answers an empty order, and the caller then keeps
//! `wasmi`'s. A module that got this far already compiled, so a disagreement
//! here is a bug in this file rather than a malformed input — and the safe
//! direction is the engine's order, not a panic.

/// The id of the export section in the binary format.
const EXPORT_SECTION: u8 = 7;

/// The export names, in the order the module declares them.
///
/// Empty when the bytes cannot be walked, which the caller reads as "no opinion".
pub(super) fn exported_names(bytes: &[u8]) -> Vec<String> {
    let mut at = match bytes.len() >= 8 && bytes.starts_with(&[0x00, 0x61, 0x73, 0x6d]) {
        true => 8,
        false => return Vec::new(),
    };
    while at < bytes.len() {
        let id = bytes[at];
        at += 1;
        let Some((size, next)) = leb(bytes, at) else { return Vec::new() };
        at = next;
        let end = at + size;
        if end > bytes.len() {
            return Vec::new();
        }
        if id == EXPORT_SECTION {
            return names_in(&bytes[at..end]);
        }
        at = end;
    }
    Vec::new()
}

/// The names inside an export section's payload.
fn names_in(payload: &[u8]) -> Vec<String> {
    let Some((count, mut at)) = leb(payload, 0) else { return Vec::new() };
    let mut found = Vec::with_capacity(count);
    for _ in 0..count {
        let Some((length, next)) = leb(payload, at) else { return found };
        at = next;
        let end = at + length;
        if end > payload.len() {
            return found;
        }
        // A name is UTF-8 by the format. One that is not belongs to a module
        // that would not have compiled, so it is skipped rather than guessed at.
        if let Ok(name) = std::str::from_utf8(&payload[at..end]) {
            found.push(name.to_owned());
        }
        at = end;
        // The kind byte, then the index of what is exported — both skipped,
        // because `wasmi` already answers what each export IS and this file
        // answers only the order.
        if at >= payload.len() {
            return found;
        }
        at += 1;
        let Some((_, next)) = leb(payload, at) else { return found };
        at = next;
    }
    found
}

/// One unsigned LEB128 at `at`, and where it ends.
fn leb(bytes: &[u8], mut at: usize) -> Option<(usize, usize)> {
    let mut value: usize = 0;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(at)?;
        at += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, at));
        }
        shift += 7;
        // Five bytes is the most a 32-bit LEB128 can take, which is what the
        // format allows; more than that is a malformed input, answered as one.
        if shift > 28 {
            return None;
        }
    }
}

/// Reorders `rows` to follow `order`, keeping anything `order` does not name at
/// the end in the order it already had.
pub(super) fn applied<T>(order: &[String], rows: Vec<T>, name_of: impl Fn(&T) -> &str) -> Vec<T> {
    if order.is_empty() {
        return rows;
    }
    let mut sorted = rows;
    sorted.sort_by_key(|row| order.iter().position(|name| name == name_of(row)).unwrap_or(usize::MAX));
    sorted
}

#[cfg(test)]
mod tests {
    /// Declaration order is not alphabetical here, which is the whole point:
    /// sorted output would pass a test written over a module whose exports
    /// happen to be in order.
    #[test]
    fn reads_declaration_order_rather_than_sorted() {
        let bytes = [
            0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
            0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01, 0x7f,
            0x03, 0x03, 0x02, 0x00, 0x00,
            0x05, 0x03, 0x01, 0x00, 0x01,
            0x07, 0x16, 0x03,
            0x03, b'a', b'd', b'd', 0x00, 0x00,
            0x03, b'm', b'u', b'l', 0x00, 0x01,
            0x06, b'm', b'e', b'm', b'o', b'r', b'y', 0x02, 0x00,
            0x0a, 0x11, 0x02,
            0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6a, 0x0b,
            0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x6c, 0x0b,
        ];
        assert_eq!(super::exported_names(&bytes), ["add", "mul", "memory"]);
    }

    #[test]
    fn bytes_that_are_not_a_module_have_no_opinion() {
        assert!(super::exported_names(&[1, 2, 3, 4]).is_empty());
        assert!(super::exported_names(&[]).is_empty());
    }
}
