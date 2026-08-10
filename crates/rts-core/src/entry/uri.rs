//! `encodeURIComponent`, `decodeURIComponent`, `encodeURI`, `decodeURI`.
//!
//! # Why one module for four functions
//!
//! There are two algorithms here, not four. Encoding walks a string and escapes
//! everything outside a set; decoding walks it and unescapes everything outside
//! a set. The four names are those two algorithms with two different sets, and
//! **the sets are the entire difference** — which is worth writing once, because
//! the failure mode of writing it twice is the pair drifting apart in exactly
//! the place a caller cannot see.
//!
//! # What the two sets are, and why the asymmetry matters
//!
//! `encodeURIComponent` preserves only what a URI calls *unreserved*:
//! `A-Za-z0-9-_.!~*'()`. Everything else becomes a percent-escape, `/` and `?`
//! and `&` included — which is the point, because a component is a value being
//! placed *inside* a URI and a raw `&` there would end it early.
//!
//! `encodeURI` additionally preserves the *reserved* set `;/?:@&=+$,#`, because
//! its argument is a whole URI whose delimiters are already doing their job. So
//! `encodeURI("http://a/b?c=d")` comes back unchanged and
//! `encodeURIComponent` of the same string escapes six characters.
//!
//! The decoding pair mirrors it, and this is the direction people get wrong:
//! **`decodeURI` does not decode `%2F`**. It leaves the three characters exactly
//! as it found them, because turning `%2F` into `/` inside a URI would invent a
//! path separator the author deliberately escaped. `decodeURIComponent` decodes
//! everything, because a component has no delimiters to protect. A round trip
//! through `encodeURI` then `decodeURI` is therefore identity, which is the
//! property the preservation exists to give.
//!
//! # Why the transformation is over UTF-8 bytes and the string is not
//!
//! The specification defines the escape as UTF-8 octets, and a JavaScript string
//! here is UTF-16 code units ([`crate::text`] says why). So encoding decodes the
//! units into scalars first — and a **lone surrogate has no UTF-8 form at all**,
//! which is the case the specification answers with a `URIError`.
//!
//! # Where this answers instead of throwing
//!
//! A lone surrogate, and a malformed escape on the way back, are both `URIError`
//! in the language and neither is thrown here. [`super::throw`] ends the program:
//! a throw needs a protected region in a caller to land in, and an `extern "C"`
//! frame cannot unwind to find one. Killing a program because one query
//! parameter was malformed is a worse answer than any value.
//!
//! So **all four answer `undefined`** for input they cannot transform — the same
//! choice [`super::json`] made for a parse error, and for the same reason: it is
//! visible to the program, it is testable with `===`, and it is not the empty
//! string, which would be a wrong answer that looks like a right one. When
//! protected regions exist these are four `throw_js_error` calls and nothing else
//! in the file changes.

use super::native::Native;
use super::objects::undefined_of;
use super::with_current;
use crate::text::Str;
use crate::value::Value;

/// The natives this module provides, by the name a program reads.
///
/// The same shape [`super::global_fns::provided`] has, kept separate rather than
/// merged into that list: these four are one subject with one page of reasoning
/// behind them, and a name in a list somewhere else is a name whose reasoning
/// has to be found.
pub(super) fn provided(name: &str) -> Option<Native> {
    Some(match name {
        "encodeURIComponent" => encode_uri_component,
        "decodeURIComponent" => decode_uri_component,
        "encodeURI" => encode_uri,
        "decodeURI" => decode_uri,
        _ => return None,
    })
}

/// What a URI calls unreserved — never escaped by either encoder.
const UNRESERVED: &str = "-_.!~*'()";

/// What a URI calls reserved, plus `#`.
///
/// `encodeURI` leaves these alone and `decodeURI` leaves their escapes alone.
/// One constant for both directions, because they are required to agree: a
/// character in one list and not the other is a round trip that loses.
const RESERVED: &str = ";/?:@&=+$,#";

/// `encodeURIComponent(s)`.
extern "C" fn encode_uri_component(_e: u64, _t: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    encoded(value, false)
}

/// `encodeURI(s)`.
extern "C" fn encode_uri(_e: u64, _t: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    encoded(value, true)
}

/// `decodeURIComponent(s)`.
extern "C" fn decode_uri_component(_e: u64, _t: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    decoded(value, false)
}

/// `decodeURI(s)`.
extern "C" fn decode_uri(_e: u64, _t: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    decoded(value, true)
}

/// The argument's code units, after `ToString`.
///
/// `ToString` first because the language says so: `encodeURIComponent(1)` is
/// `"1"` rather than a refusal, and a version that took only strings would
/// refuse calls the specification defines.
///
/// The whole borrow is this function. Nothing below it touches the heap until
/// the answer is interned, which is what keeps the module trivially free of the
/// nested-borrow abort the entry layer is arranged around.
fn units_of(value: u64) -> Option<Vec<u16>> {
    with_current(|context| {
        super::text::to_text(context, Value(value)).map(|text| text.units().collect())
    })
}

/// A finished answer, or `undefined` where the specification throws.
fn answer(units: Option<Vec<u16>>) -> u64 {
    with_current(|context| match units {
        Some(units) => context.intern_value(Str::from_utf16(&units)).bits(),
        None => undefined_of(context),
    })
}

fn encoded(value: u64, whole_uri: bool) -> u64 {
    answer(units_of(value).and_then(|units| encode(&units, whole_uri)))
}

fn decoded(value: u64, whole_uri: bool) -> u64 {
    answer(units_of(value).and_then(|units| decode(&units, whole_uri)))
}

/// Percent-escapes everything outside the preserved set.
///
/// `None` for a lone surrogate — see the module documentation. It is detected
/// rather than replaced with `U+FFFD`, which is what a naive conversion would
/// do: a replacement character escapes to `%EF%BF%BD`, so the caller would get
/// a plausible answer describing text it never had.
fn encode(units: &[u16], whole_uri: bool) -> Option<Vec<u16>> {
    let mut out = Vec::with_capacity(units.len());
    for scalar in char::decode_utf16(units.iter().copied()) {
        let scalar = scalar.ok()?;
        if kept(scalar, whole_uri) {
            // Every preserved character is ASCII, so its scalar value and its
            // code unit are the same number.
            out.push(scalar as u16);
            continue;
        }
        let mut buffer = [0u8; 4];
        for byte in scalar.encode_utf8(&mut buffer).as_bytes() {
            out.push(u16::from(b'%'));
            out.push(u16::from(HEX[usize::from(*byte >> 4)]));
            out.push(u16::from(HEX[usize::from(*byte & 0xf)]));
        }
    }
    Some(out)
}

/// Whether a character survives encoding unescaped.
fn kept(scalar: char, whole_uri: bool) -> bool {
    if scalar.is_ascii_alphanumeric() || UNRESERVED.contains(scalar) {
        return true;
    }
    whole_uri && RESERVED.contains(scalar)
}

/// Unescapes, leaving reserved escapes alone for `decodeURI`.
///
/// `None` for any malformed escape: a `%` without two hex digits, a continuation
/// byte where a lead byte belongs, a truncated sequence, an overlong form, or a
/// code point UTF-8 may not carry. Every one of those is a `URIError` in the
/// specification, and accepting any of them would let percent-encoding smuggle a
/// character past a check written against the encoded text — which is the class
/// of bug overlong forms exist in the literature for.
fn decode(units: &[u16], whole_uri: bool) -> Option<Vec<u16>> {
    let mut out = Vec::with_capacity(units.len());
    let mut at = 0;
    while at < units.len() {
        let unit = units[at];
        if unit != u16::from(b'%') {
            // Copied as a code unit, not as a character. A lone surrogate that
            // was never escaped is a legal string and passes through here
            // untouched — the specification only refuses one that would have to
            // become UTF-8, which is the encoding direction.
            out.push(unit);
            at += 1;
            continue;
        }
        let byte = escape_at(units, at)?;
        if byte < 0x80 {
            match whole_uri && RESERVED.contains(char::from(byte)) {
                // The three original units rather than a re-spelling of them.
                // The specification preserves the source text, so a lowercase
                // `%2f` stays lowercase — re-emitting `%2F` would be a
                // normalisation nothing asked for and a round trip that changes
                // its input.
                true => out.extend_from_slice(&units[at..at + 3]),
                false => out.push(u16::from(byte)),
            }
            at += 3;
            continue;
        }
        // A lead byte says how many continuations follow. `0x80..=0xBF` is a
        // continuation with nothing to continue and `0xC0`, `0xC1` and
        // `0xF5..` can only ever begin an overlong or an out-of-range sequence,
        // so none of them is admitted at all.
        let extra = match byte {
            0xc2..=0xdf => 1,
            0xe0..=0xef => 2,
            0xf0..=0xf4 => 3,
            _ => return None,
        };
        let mut point = u32::from(byte & (0x3f >> extra));
        for step in 1..=extra {
            let next = escape_at(units, at + step * 3)?;
            if next & 0xc0 != 0x80 {
                return None;
            }
            point = (point << 6) | u32::from(next & 0x3f);
        }
        // The shortest form is the only legal one, and the lead-byte filter
        // above catches only the two-byte case. Without this, `%E0%80%A2` would
        // decode to `.` — a character that was not in the encoded text.
        if point < LOWEST[extra - 1] {
            return None;
        }
        // Rejects the surrogate range and anything above `U+10FFFF`, neither of
        // which UTF-8 encodes and both of which a hand-built sequence can spell.
        let scalar = char::from_u32(point)?;
        let mut buffer = [0u16; 2];
        out.extend_from_slice(scalar.encode_utf16(&mut buffer));
        at += (extra + 1) * 3;
    }
    Some(out)
}

/// The byte one `%XX` spells, or nothing if there is not one here.
fn escape_at(units: &[u16], at: usize) -> Option<u8> {
    if units.get(at).copied()? != u16::from(b'%') {
        return None;
    }
    let high = hex(units.get(at + 1).copied()?)?;
    let low = hex(units.get(at + 2).copied()?)?;
    Some((high << 4) | low)
}

/// The value of one hexadecimal digit, in either case.
fn hex(unit: u16) -> Option<u8> {
    match unit {
        0x30..=0x39 => Some((unit - 0x30) as u8),
        0x41..=0x46 => Some((unit - 0x41 + 10) as u8),
        0x61..=0x66 => Some((unit - 0x61 + 10) as u8),
        _ => None,
    }
}

/// Uppercase, because the specification's encoder emits uppercase. Lowercase
/// escapes are equally valid to *read*, which is why [`hex`] takes both.
const HEX: [u8; 16] = *b"0123456789ABCDEF";

/// The smallest code point each continuation count is allowed to carry.
const LOWEST: [u32; 3] = [0x80, 0x800, 0x1_0000];

#[cfg(test)]
mod tests {
    use super::*;

    /// The one behaviour that separates the two decoders, and the reason both
    /// exist. If this ever passes for both, `decodeURI` has stopped being a URI
    /// operation and become a second `decodeURIComponent`.
    #[test]
    fn decode_uri_preserves_a_reserved_escape_and_the_component_form_does_not() {
        let input: Vec<u16> = "a%2Fb".encode_utf16().collect();
        assert_eq!(decode(&input, true), Some(input.clone()));
        assert_eq!(
            decode(&input, false),
            Some("a/b".encode_utf16().collect::<Vec<u16>>())
        );
    }

    #[test]
    fn encode_uri_leaves_a_whole_url_alone_where_the_component_form_escapes_it() {
        let input: Vec<u16> = "http://a/b?c=d".encode_utf16().collect();
        assert_eq!(encode(&input, true), Some(input.clone()));
        let component = encode(&input, false).and_then(|units| Str::from_utf16(&units).to_rust());
        assert_eq!(component.as_deref(), Some("http%3A%2F%2Fa%2Fb%3Fc%3Dd"));
    }

    #[test]
    fn a_non_ascii_character_round_trips_through_its_utf8_bytes() {
        let input: Vec<u16> = "é😀".encode_utf16().collect();
        let encoded = encode(&input, false).unwrap();
        assert_eq!(
            Str::from_utf16(&encoded).to_rust().as_deref(),
            Some("%C3%A9%F0%9F%98%80"),
            "two octets and four, escaped one octet at a time"
        );
        assert_eq!(decode(&encoded, false), Some(input));
    }

    #[test]
    fn a_lone_surrogate_has_no_utf8_form_and_is_refused() {
        assert_eq!(
            encode(&[0xd800], false),
            None,
            "a URIError in the specification; undefined here, and never U+FFFD"
        );
        assert_eq!(
            decode(&[0xd800], false),
            Some(vec![0xd800]),
            "decoding never has to produce octets, so it copies it through"
        );
    }

    #[test]
    fn a_malformed_escape_is_refused_rather_than_guessed_at() {
        for bad in ["%", "%2", "%2G", "%80", "%C3", "%C3%28", "%E0%80%A2", "%F5%80%80%80"] {
            let units: Vec<u16> = bad.encode_utf16().collect();
            assert_eq!(decode(&units, false), None, "{bad}");
        }
    }
}
