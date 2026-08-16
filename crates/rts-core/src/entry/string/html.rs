//! The thirteen Annex B methods that wrap a string in a tag.
//!
//! # Why an engine with no browser in it has these
//!
//! Because they are not a browser feature. `CreateHTML` builds a string and
//! nothing else — no document, no parser, no escaping beyond one character —
//! so the only thing "HTML" about them is the shape of the text they produce.
//! Annex B calls them normative-optional and every engine a program is likely
//! to meet implements them, which makes their ABSENCE the divergence: a program
//! that calls `"x".bold()` here got `TypeError: S.big is not a function` where
//! Node and Bun answer, and `string/claude2-annexb-html-methods` is the file
//! that says so.
//!
//! # The one rule worth stating, because it looks like a bug
//!
//! `CreateHTML` escapes `"` in the ATTRIBUTE VALUE — as `&quot;` — and escapes
//! nothing else, anywhere. Not `<`, not `&`, and not the quote in the BODY.
//! That is deliberate in the specification and it is why these are not a
//! sanitiser; an implementation that "fixed" it by escaping more would diverge
//! from every engine on exactly the inputs a test reaches for.
//!
//! # Why they are here rather than in `more.rs`
//!
//! Thirteen methods over one helper is a cohesive module, and `more.rs` was
//! already the folder's second-largest file. Nothing in this one is reachable
//! from anywhere else.

use super::super::native::Native;
use super::super::with_current;
use super::{answer, arg_units, units_of};

/// The nine that take no argument, and the four that take an attribute value.
///
/// With arity, and the numbers are observable rather than decoration: the
/// fixture reads `String.prototype.anchor.length` and every engine answers 1
/// for the four and 0 for the nine.
pub(super) const NATIVES: &[(&str, Native, u32)] = &[
    ("big", big, 0),
    ("blink", blink, 0),
    ("bold", bold, 0),
    ("fixed", fixed, 0),
    ("italics", italics, 0),
    ("small", small, 0),
    ("strike", strike, 0),
    ("sub", sub, 0),
    ("sup", sup, 0),
    ("anchor", anchor, 1),
    ("link", link, 1),
    ("fontcolor", fontcolor, 1),
    ("fontsize", fontsize, 1),
];

/// `CreateHTML(this, tag, attribute, value)`.
///
/// The receiver is coerced FIRST and the value second, which is observable
/// through a `toString` that throws — the fixture's `order` row asks for
/// exactly that, and doing the value first would answer `value` where every
/// engine answers `this`.
fn create(this: u64, tag: &str, attribute: Option<(&str, u64)>) -> u64 {
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // `None` for the tag-only methods; `Some(text)` once the value has been
    // through `ToString`, which is where a symbol becomes a `TypeError` and a
    // throwing `toString` ends the call.
    let attribute = match attribute {
        None => None,
        Some((name, value)) => {
            let Some(value) = super::text_arg(value) else {
                return super::refused();
            };
            Some((name, value))
        }
    };

    with_current(|context| {
        let Some(body) = units_of(context, this) else {
            return super::nothing(context);
        };
        let mut out: Vec<u16> = Vec::new();
        out.extend("<".encode_utf16());
        out.extend(tag.encode_utf16());
        if let Some((name, value)) = attribute {
            let Some(value) = arg_units(context, value) else {
                return super::nothing(context);
            };
            out.extend(" ".encode_utf16());
            out.extend(name.encode_utf16());
            out.extend("=\"".encode_utf16());
            // The one escape, and only here. `"` is 0x22; every other unit,
            // including `<` and `&`, is copied through.
            for unit in value {
                match unit {
                    0x22 => out.extend("&quot;".encode_utf16()),
                    other => out.push(other),
                }
            }
            out.extend("\"".encode_utf16());
        }
        out.extend(">".encode_utf16());
        out.extend(body);
        out.extend("</".encode_utf16());
        out.extend(tag.encode_utf16());
        out.extend(">".encode_utf16());
        answer(context, &out)
    })
}

/// One tag-only method, and one that takes an attribute.
///
/// A macro rather than thirteen bodies: each differs only in the two strings it
/// hands [`create`], so written out they would be thirteen places for one of
/// them to name the wrong tag — which is a divergence no local test would see.
macro_rules! tagged {
    ($name:ident, $tag:literal) => {
        extern "C" fn $name(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
            create(this, $tag, None)
        }
    };
    ($name:ident, $tag:literal, $attribute:literal) => {
        extern "C" fn $name(_e: u64, this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
            create(this, $tag, Some(($attribute, value)))
        }
    };
}

tagged!(big, "big");
tagged!(blink, "blink");
tagged!(bold, "b");
tagged!(fixed, "tt");
tagged!(italics, "i");
tagged!(small, "small");
tagged!(strike, "strike");
tagged!(sub, "sub");
tagged!(sup, "sup");
tagged!(anchor, "a", "name");
tagged!(link, "a", "href");
tagged!(fontcolor, "font", "color");
tagged!(fontsize, "font", "size");
