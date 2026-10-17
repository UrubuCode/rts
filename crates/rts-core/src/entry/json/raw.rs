//! `JSON.rawJSON` and `JSON.isRawJSON` (ES2024 JSON.parse source text access).
//!
//! # The object
//!
//! A frozen, prototype-less object with one own property, `rawJSON`, holding
//! the text. The text is validated as ONE JSON primitive — a number, a string,
//! `true`, `false` or `null` — with no leading or trailing whitespace, because
//! `JSON.stringify` copies it into the output verbatim and anything else would
//! let a caller smuggle structure past the serialiser.
//!
//! # The brand
//!
//! `isRawJSON` must be true for what `rawJSON` made and false for a look-alike
//! (`Object.freeze(Object.assign(Object.create(null), {rawJSON: "1"}))`). The
//! brand is the cell's [`super::super::foreign`] word set to [`BRAND`]. That
//! table is "a word a client keeps beside an object": its other user, `napi_wrap`,
//! stores pointers, which are aligned and so never equal to the odd sentinel
//! used here, and the word is dropped with the cell, so the brand cannot
//! outlive the object. It is invisible to the program, as an internal slot is.

use super::super::objects::{put, undefined_of};
use super::super::{foreign, integrity, throw, with_current};
use super::read::{self, Node};
use crate::value::Value;

/// An odd word: a pointer from `napi_wrap` is never odd.
const BRAND: usize = 0x5241_574a_534f_4e01;

/// `JSON.rawJSON(text)`.
pub(super) fn raw_json(text: u64) -> u64 {
    // `ToString` first, outside any borrow: it can run user code.
    let Some(text) = super::super::text::to_string_value(text) else {
        return with_current(|context| undefined_of(context));
    };
    let verdict = with_current(|context| {
        let source = Value(text).as_slot().and_then(|cell| context.text_at(cell))?;
        let units: Vec<u16> = source.units().collect();
        Some(check(&units, source))
    });
    match verdict {
        Some(Ok(())) => {}
        Some(Err(message)) => {
            throw::syntax_error(&message);
            return with_current(|context| undefined_of(context));
        }
        None => return with_current(|context| undefined_of(context)),
    }
    let made = with_current(|context| {
        let Some(cell) = super::super::native::plain(context) else {
            return undefined_of(context);
        };
        let pin = super::super::buffers::Pin::new(cell);
        let null = Value::from_singleton(context.singletons.null).bits();
        context.set_prototype(cell, null);
        let key = context.well_known("rawJSON");
        put(context, cell, key, text);
        pin.release();
        foreign::attach(context, Value::from_slot(cell).bits(), BRAND);
        Value::from_slot(cell).bits()
    });
    integrity::restrict(made, integrity::Integrity::Frozen)
}

/// `JSON.isRawJSON(value)`.
pub(super) fn is_raw_json(value: u64) -> bool {
    with_current(|context| foreign::attached(context, value) == Some(BRAND))
}

/// Whether `value` is a cell [`raw_json`] made.
pub(super) fn branded(context: &super::super::Context, value: u64) -> bool {
    foreign::attached(context, value) == Some(BRAND)
}

/// The text a raw JSON cell holds, as the cell of the string.
pub(super) fn text_of(context: &mut super::super::Context, cell: u32) -> Option<u32> {
    let key = context.well_known("rawJSON");
    let held = super::super::objects::own_property(context, cell, key)?;
    let text = held.as_slot()?;
    context.text_at(text).map(|_| text)
}

/// Whether `units` is a single JSON primitive with no surrounding whitespace.
fn check(units: &[u16], source: &crate::text::Str) -> Result<(), String> {
    let invalid = || "Invalid value for JSON.rawJSON".to_string();
    let (Some(first), Some(last)) = (units.first().copied(), units.last().copied()) else {
        return Err(invalid());
    };
    let space = |unit: u16| matches!(unit, 0x09 | 0x0a | 0x0d | 0x20);
    if matches!(first, 0x7b | 0x5b) || space(first) {
        // V8 words these as the ordinary parse error for the leading token.
        let shown = String::from_utf16_lossy(units);
        let token = String::from_utf16_lossy(&[first]);
        return Err(format!("Unexpected token '{token}', \"{shown}\" is not valid JSON"));
    }
    if space(last) {
        return Err(invalid());
    }
    match read::parse_text(source) {
        Some(Node::Array(_)) | Some(Node::Object(_)) | None => {
            Err(format!("\"{}\" is not valid JSON", String::from_utf16_lossy(units)))
        }
        Some(_) => Ok(()),
    }
}
