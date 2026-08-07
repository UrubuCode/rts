//! `node:punycode` — RFC 3492 Bootstring, over
//! `docs/reference/node/punycode.md`.
//!
//! # What reuse-check found
//!
//! Nothing: this is Unicode-codepoint arithmetic with no notion of a shape, a
//! tag, or a runtime value until the very last step, so neither
//! `rts-cranelift` nor `rts-core-rwk` has anything to call. The one place
//! reuse mattered — `ucs2.decode`/`ucs2.encode`, which are "UTF-16 code
//! units ⇄ codepoints" — needs NO new logic at all: a Rust `String`'s
//! `.chars()` already combines a surrogate pair into one codepoint (Rust
//! strings are codepoint sequences under UTF-8, not UTF-16 code units), so
//! [`ucs2_decode`] is that iterator, unchanged. The reverse direction is
//! exactly what pushing a `char` onto a `String` already does.
//!
//! # Why the Bootstring core is hand-rolled here rather than shared
//!
//! `rts-node-rwk` is independent of `rts-std`/`rts-core-rwk` by design (see
//! `lib.rs`'s module doc), and no crate in this workspace implements RFC 3492
//! — `node:url`'s IDNA surface, if it ever lands, is a stricter, separate
//! algorithm (full UTS-46) that must NOT delegate to this module either, per
//! the spec's own §7. So there is nothing here to reuse and nothing this
//! module should be reused BY beyond its own six exports.
//!
//! # Pinned against RFC 3492 / Node's own documented vectors
//!
//! Not against "what this implementation happens to produce" — the
//! `#[cfg(test)]` block below asserts the exact strings Node's own docs give
//! (`'mañana'` ⇄ `'maana-pta'`, `'☃-⌘'` ⇄ `'--dqo34k'`) plus a
//! surrogate-pair round trip, and every parameter (`base`, `tmin`, `tmax`,
//! `skew`, `damp`, `initial_bias`, `initial_n`, the `-` delimiter) is the
//! fixed RFC 3492 value, not a tuned one.
//!
//! # Not implemented, by name
//!
//! Full IDNA/UTS-46 validation — Bidi/ContextJ rules, NFC normalization,
//! disallowed-character mapping. This module is the Bootstring transcoder
//! only, exactly as Node's own is (spec §4's security note): a string that
//! passes through `toASCII` unchanged may still be an invalid domain under
//! the fuller spec. A deprecation warning on import/require — RTS has no
//! general mechanism for one yet (spec §7); `node:punycode` behaves as if
//! undeprecated. The bare `require("punycode")` specifier (no `node:`
//! prefix) — only `node:punycode` is registered; see [`super::install`].

use rts_core_rwk::entry::{self, Context, Provided};

const BASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;
const SKEW: u32 = 38;
const DAMP: u32 = 700;
const INITIAL_BIAS: u32 = 72;
const INITIAL_N: u32 = 128;
const DELIMITER: char = '-';

/// Why a Bootstring pass can fail — a malformed `decode` input, or delta
/// arithmetic that would overflow. Both are `RangeError` in Node; see the
/// module doc for why this crate answers with a fallback instead.
#[derive(Debug, PartialEq, Eq)]
enum PunyError {
    NonBasicCodePoint,
    InvalidDigit,
    Overflow,
}

fn adapt(delta: u32, num_points: u32, first_time: bool) -> u32 {
    let mut delta = if first_time { delta / DAMP } else { delta / 2 };
    delta += delta / num_points;
    let mut k = 0u32;
    while delta > ((BASE - TMIN) * TMAX) / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }
    k + (((BASE - TMIN + 1) * delta) / (delta + SKEW))
}

fn digit_to_basic(digit: u32) -> char {
    if digit < 26 { (b'a' + digit as u8) as char } else { (b'0' + (digit - 26) as u8) as char }
}

fn basic_to_digit(ch: char) -> Option<u32> {
    match ch {
        'a'..='z' => Some(ch as u32 - 'a' as u32),
        'A'..='Z' => Some(ch as u32 - 'A' as u32),
        '0'..='9' => Some(ch as u32 - '0' as u32 + 26),
        _ => None,
    }
}

/// `punycode.encode(string)` — a codepoint sequence to Bootstring ASCII, no
/// `xn--` prefix. RFC 3492 §6.3's reference algorithm, arithmetic guarded
/// with `checked_*` throughout so a pathological input degrades to
/// [`PunyError::Overflow`] rather than a panicking wraparound.
fn encode(input: &str) -> Result<String, PunyError> {
    let codepoints: Vec<u32> = input.chars().map(|ch| ch as u32).collect();
    let mut output: String = codepoints.iter().copied().filter(|&cp| cp < 0x80).map(|cp| cp as u8 as char).collect();
    let basic_count = output.chars().count() as u32;
    let mut handled = basic_count;
    if basic_count > 0 {
        output.push(DELIMITER);
    }
    let mut n = INITIAL_N;
    let mut delta: u32 = 0;
    let mut bias = INITIAL_BIAS;
    let total = codepoints.len() as u32;
    while handled < total {
        let next_min = codepoints.iter().copied().filter(|&cp| cp >= n).min().ok_or(PunyError::Overflow)?;
        let advance = (next_min - n).checked_mul(handled + 1).ok_or(PunyError::Overflow)?;
        delta = delta.checked_add(advance).ok_or(PunyError::Overflow)?;
        n = next_min;
        for &cp in &codepoints {
            if cp < n {
                delta = delta.checked_add(1).ok_or(PunyError::Overflow)?;
            }
            if cp == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = threshold(k, bias);
                    if q < t {
                        break;
                    }
                    output.push(digit_to_basic(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                output.push(digit_to_basic(q));
                bias = adapt(delta, handled + 1, handled == basic_count);
                delta = 0;
                handled += 1;
            }
        }
        delta = delta.checked_add(1).ok_or(PunyError::Overflow)?;
        n = n.checked_add(1).ok_or(PunyError::Overflow)?;
    }
    Ok(output)
}

/// `punycode.decode(string)` — Bootstring ASCII to the codepoint sequence it
/// encodes. RFC 3492 §6.2's reference algorithm.
fn decode(input: &str) -> Result<Vec<u32>, PunyError> {
    let chars: Vec<char> = input.chars().collect();
    let split = chars.iter().rposition(|&ch| ch == DELIMITER);
    let (basic, extended) = match split {
        Some(at) => (&chars[..at], &chars[at + 1..]),
        None => (&chars[..0], &chars[..]),
    };
    let mut output: Vec<u32> = Vec::with_capacity(basic.len());
    for &ch in basic {
        if ch as u32 >= 0x80 {
            return Err(PunyError::NonBasicCodePoint);
        }
        output.push(ch as u32);
    }
    let mut n = INITIAL_N;
    let mut i: u32 = 0;
    let mut bias = INITIAL_BIAS;
    let mut cursor = 0usize;
    while cursor < extended.len() {
        let old_i = i;
        let mut weight: u32 = 1;
        let mut k = BASE;
        loop {
            let ch = *extended.get(cursor).ok_or(PunyError::InvalidDigit)?;
            cursor += 1;
            let digit = basic_to_digit(ch).ok_or(PunyError::InvalidDigit)?;
            let term = digit.checked_mul(weight).ok_or(PunyError::Overflow)?;
            i = i.checked_add(term).ok_or(PunyError::Overflow)?;
            let t = threshold(k, bias);
            if digit < t {
                break;
            }
            weight = weight.checked_mul(BASE - t).ok_or(PunyError::Overflow)?;
            k += BASE;
        }
        let out_len = output.len() as u32 + 1;
        bias = adapt(i - old_i, out_len, old_i == 0);
        n = n.checked_add(i / out_len).ok_or(PunyError::Overflow)?;
        i %= out_len;
        output.insert(i as usize, n);
        i += 1;
    }
    Ok(output)
}

fn threshold(k: u32, bias: u32) -> u32 {
    if k <= bias { TMIN } else if k >= bias + TMAX { TMAX } else { k - bias }
}

fn decode_to_string(input: &str) -> Result<String, PunyError> {
    decode(input).map(|codepoints| codepoints.into_iter().filter_map(char::from_u32).collect())
}

/// The alternate ideographic/fullwidth label separators domain-splitting
/// treats as `.` — WHATWG domain-to-ASCII label handling, spec §4.
fn normalize_separators(text: &str) -> String {
    text.chars().map(|ch| if matches!(ch, '\u{3002}' | '\u{FF0E}' | '\u{FF61}') { '.' } else { ch }).collect()
}

/// The `user@`/`@` split `toASCII`/`toUnicode` both apply before touching
/// labels — everything up to and including the last `@` passes through
/// unchanged.
fn split_userinfo(domain: &str) -> (&str, &str) {
    match domain.rfind('@') {
        Some(at) => domain.split_at(at + 1),
        None => ("", domain),
    }
}

/// `punycode.toASCII(domain)`.
fn to_ascii(domain: &str) -> String {
    let (prefix, rest) = split_userinfo(domain);
    let normalized = normalize_separators(rest);
    let labels: Vec<String> = normalized
        .split('.')
        .map(|label| match label.chars().any(|ch| ch as u32 > 0x7F) {
            true => match encode(label) {
                Ok(encoded) => format!("xn--{encoded}"),
                Err(_) => label.to_owned(),
            },
            false => label.to_owned(),
        })
        .collect();
    format!("{prefix}{}", labels.join("."))
}

/// `punycode.toUnicode(domain)`.
fn to_unicode(domain: &str) -> String {
    let (prefix, rest) = split_userinfo(domain);
    let normalized = normalize_separators(rest);
    let labels: Vec<String> = normalized
        .split('.')
        .map(|label| {
            let lower = label.to_ascii_lowercase();
            match lower.strip_prefix("xn--") {
                Some(body) => decode_to_string(body).unwrap_or_else(|_| label.to_owned()),
                None => label.to_owned(),
            }
        })
        .collect();
    format!("{prefix}{}", labels.join("."))
}

/// The bundled algorithm's version — a build-time constant, not something
/// Node's own runtime computes either (spec §5.2).
const VERSION: &str = "2.3.1";

/// The namespace `node:punycode` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("decode", decode_fn),
        ("encode", encode_fn),
        ("toASCII", to_ascii_fn),
        ("toUnicode", to_unicode_fn),
    ];
    let namespace = entry::make_namespace(context, members);
    let ucs2_members: &[(&str, Provided)] = &[("decode", ucs2_decode_fn), ("encode", ucs2_encode_fn)];
    let ucs2 = entry::make_namespace(context, ucs2_members);
    entry::put_member(context, namespace, "ucs2", ucs2);
    let version = entry::make_string(context, VERSION);
    entry::put_member(context, namespace, "version", version);
    namespace
}

/// An argument's text, `None` for `undefined` — the same convention
/// `path.rs`'s `text` helper uses.
fn text_arg(value: u64) -> Option<String> {
    let absent = entry::undefined_value();
    match value == absent {
        true => None,
        false => entry::text_of(value),
    }
}

fn string_value(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// `punycode.decode(string)`. Malformed input answers `""` rather than
/// throwing — the no-throw stand-in the module doc names.
extern "C" fn decode_fn(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text = text_arg(value).unwrap_or_default();
    string_value(&decode_to_string(&text).unwrap_or_default())
}

/// `punycode.encode(string)`. Same no-throw stand-in as [`decode_fn`].
extern "C" fn encode_fn(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text = text_arg(value).unwrap_or_default();
    string_value(&encode(&text).unwrap_or_default())
}

/// `punycode.toASCII(domain)`.
extern "C" fn to_ascii_fn(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text = text_arg(value).unwrap_or_default();
    string_value(&to_ascii(&text))
}

/// `punycode.toUnicode(domain)`.
extern "C" fn to_unicode_fn(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text = text_arg(value).unwrap_or_default();
    string_value(&to_unicode(&text))
}

/// `punycode.ucs2.decode(string)` — a Rust `String`'s `.chars()` already
/// combines a surrogate pair into one codepoint; see the module doc.
extern "C" fn ucs2_decode_fn(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text = text_arg(value).unwrap_or_default();
    let codepoints: Vec<u64> = text.chars().map(|ch| entry::make_number(ch as u32 as f64)).collect();
    entry::make_array(codepoints)
}

/// `punycode.ucs2.encode(codePoints)` — the inverse. An out-of-range or
/// non-integer entry becomes U+FFFD rather than the `RangeError` Node
/// throws; see the module doc's no-throw stand-in.
extern "C" fn ucs2_encode_fn(_e: u64, _this: u64, array: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let text: String = collect_array(array)
        .into_iter()
        .map(|value| entry::number_of(value).and_then(|n| char::from_u32(n as u32)).unwrap_or('\u{FFFD}'))
        .collect();
    string_value(&text)
}

/// A JS array's elements, through `.length` and indexed reads — the same
/// small helper `events.rs`'s `collect_array` is, needed independently here
/// since [`entry::modules`] exposes indexing, not array iteration.
fn collect_array(array: u64) -> Vec<u64> {
    let absent = entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_key = string_value("length");
    let length_value = entry::get_indexed(array, length_key);
    let length = entry::number_of(length_value).map(|value| value as usize).unwrap_or(0);
    (0..length).map(|index| entry::get_indexed(array, entry::make_number(index as f64))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Node's own documented examples (spec §2, `punycode.decode`/`encode`).
    #[test]
    fn decodes_nodes_documented_examples() {
        assert_eq!(decode_to_string("maana-pta").unwrap(), "mañana");
        assert_eq!(decode_to_string("--dqo34k").unwrap(), "☃-⌘");
    }

    #[test]
    fn encodes_nodes_documented_examples() {
        assert_eq!(encode("mañana").unwrap(), "maana-pta");
        assert_eq!(encode("☃-⌘").unwrap(), "--dqo34k");
    }

    #[test]
    fn round_trips_ascii_accented_and_astral_input() {
        for sample in ["example", "mañana", "☃-⌘", "😀party"] {
            let encoded = encode(sample).unwrap();
            assert_eq!(decode_to_string(&encoded).unwrap(), sample);
        }
    }

    /// Spec §6's documented `toASCII`/`toUnicode` vectors.
    #[test]
    fn transcodes_domains() {
        assert_eq!(to_ascii("mañana.com"), "xn--maana-pta.com");
        assert_eq!(to_ascii("☃-⌘.com"), "xn----dqo34k.com");
        assert_eq!(to_ascii("example.com"), "example.com");
        assert_eq!(to_unicode("xn--maana-pta.com"), "mañana.com");
        assert_eq!(to_unicode("xn----dqo34k.com"), "☃-⌘.com");
        assert_eq!(to_unicode("example.com"), "example.com");
        assert_eq!(to_unicode("XN--maana-pta.com"), "mañana.com");
    }

    #[test]
    fn handles_the_user_at_domain_form() {
        assert_eq!(to_ascii("user@mañana.com"), "user@xn--maana-pta.com");
    }

    #[test]
    fn normalizes_alternate_separators() {
        assert_eq!(to_ascii("mañana\u{3002}com"), "xn--maana-pta.com");
    }

    /// Spec §6's ucs2 vectors — a surrogate pair is ONE entry, not two.
    #[test]
    fn ucs2_combines_surrogate_pairs() {
        assert_eq!("abc".chars().map(|ch| ch as u32).collect::<Vec<_>>(), vec![0x61, 0x62, 0x63]);
        assert_eq!("𝌆".chars().map(|ch| ch as u32).collect::<Vec<_>>(), vec![0x1D306]);
    }

    #[test]
    fn decode_rejects_a_non_basic_code_point() {
        // A non-ASCII char BEFORE the last delimiter is a basic code point
        // that should never appear there.
        assert_eq!(decode("m\u{00E9}-"), Err(PunyError::NonBasicCodePoint));
    }
}
