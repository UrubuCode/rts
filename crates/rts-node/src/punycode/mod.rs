//! `node:punycode` — Punycode (RFC 3492) + IDNA ToASCII/ToUnicode.
//!
//! Native rts-node implementation (no rts-std mirror). Pure `string`→`string`;
//! symbols use the `__RTS_FN_NODE_PUNYCODE_*` convention. The module is
//! deprecated in Node but still shipped, so it is part of the parity surface.
//!
//! `encode`/`decode` operate on a whole string (Bootstring); `toASCII`/
//! `toUnicode` operate per dot-separated label, adding/stripping the `xn--`
//! ACE prefix for labels that contain non-ASCII.

use rts_engine::abi::str_abi::from_abi;
use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// RFC 3492 Bootstring parameters for Punycode.
const BASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;
const SKEW: u32 = 38;
const DAMP: u32 = 700;
const INITIAL_BIAS: u32 = 72;
const INITIAL_N: u32 = 128;

fn adapt(mut delta: u32, numpoints: u32, firsttime: bool) -> u32 {
    delta = if firsttime { delta / DAMP } else { delta / 2 };
    delta += delta / numpoints;
    let mut k = 0u32;
    while delta > ((BASE - TMIN) * TMAX) / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }
    k + (BASE - TMIN + 1) * delta / (delta + SKEW)
}

/// digit (0..35) → basic code point byte: 0..25 → `a`..`z`, 26..35 → `0`..`9`.
fn encode_digit(d: u32) -> u8 {
    if d < 26 {
        b'a' + d as u8
    } else {
        b'0' + (d - 26) as u8
    }
}

/// basic code point → digit (0..35), case-insensitive; `None` if not a digit.
fn decode_digit(cp: u8) -> Option<u32> {
    match cp {
        b'0'..=b'9' => Some((cp - b'0') as u32 + 26),
        b'a'..=b'z' => Some((cp - b'a') as u32),
        b'A'..=b'Z' => Some((cp - b'A') as u32),
        _ => None,
    }
}

/// Punycode-encode a full Unicode string (no `xn--` prefix). `None` on overflow.
fn punycode_encode(input: &str) -> Option<String> {
    let codepoints: Vec<u32> = input.chars().map(|c| c as u32).collect();
    let input_len = codepoints.len() as u32;
    let mut output: Vec<u8> = Vec::new();

    for &cp in &codepoints {
        if cp < 0x80 {
            output.push(cp as u8);
        }
    }
    let b = output.len() as u32;
    let mut h = b;
    if b > 0 {
        output.push(b'-');
    }

    let mut n = INITIAL_N;
    let mut delta: u32 = 0;
    let mut bias = INITIAL_BIAS;

    while h < input_len {
        let m = codepoints.iter().copied().filter(|&cp| cp >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(h + 1)?)?;
        n = m;
        for &cp in &codepoints {
            if cp < n {
                delta = delta.checked_add(1)?;
            }
            if cp == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = if k <= bias {
                        TMIN
                    } else if k >= bias + TMAX {
                        TMAX
                    } else {
                        k - bias
                    };
                    if q < t {
                        break;
                    }
                    output.push(encode_digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                output.push(encode_digit(q));
                bias = adapt(delta, h + 1, h == b);
                delta = 0;
                h += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    String::from_utf8(output).ok()
}

/// Punycode-decode a string (no `xn--` prefix) back to Unicode. `None` on error.
fn punycode_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output: Vec<u32> = Vec::new();

    // Everything before the last '-' is basic (copied verbatim); decoding of the
    // non-basic insertions begins after it. No '-' ⇒ empty basic, decode from 0.
    let (basic, mut idx) = match input.rfind('-') {
        Some(pos) => (&bytes[..pos], pos + 1),
        None => (&bytes[0..0], 0usize),
    };
    for &c in basic {
        if c >= 0x80 {
            return None;
        }
        output.push(c as u32);
    }

    let mut n = INITIAL_N;
    let mut i: u32 = 0;
    let mut bias = INITIAL_BIAS;

    while idx < bytes.len() {
        let oldi = i;
        let mut w: u32 = 1;
        let mut k = BASE;
        loop {
            if idx >= bytes.len() {
                return None;
            }
            let digit = decode_digit(bytes[idx])?;
            idx += 1;
            i = i.checked_add(digit.checked_mul(w)?)?;
            let t = if k <= bias {
                TMIN
            } else if k >= bias + TMAX {
                TMAX
            } else {
                k - bias
            };
            if digit < t {
                break;
            }
            w = w.checked_mul(BASE - t)?;
            k += BASE;
        }
        let out_len = output.len() as u32 + 1;
        bias = adapt(i - oldi, out_len, oldi == 0);
        n = n.checked_add(i / out_len)?;
        i %= out_len;
        char::from_u32(n)?; // validate the code point before inserting
        output.insert(i as usize, n);
        i += 1;
    }
    output
        .iter()
        .map(|&cp| char::from_u32(cp))
        .collect::<Option<String>>()
}

fn to_ascii(domain: &str) -> String {
    domain
        .split('.')
        .map(|label| {
            if label.is_ascii() {
                label.to_string()
            } else {
                match punycode_encode(label) {
                    Some(enc) => format!("xn--{enc}"),
                    None => label.to_string(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn to_unicode(domain: &str) -> String {
    domain
        .split('.')
        .map(|label| {
            let stripped = label
                .strip_prefix("xn--")
                .or_else(|| label.strip_prefix("XN--"));
            match stripped {
                Some(rest) => punycode_decode(rest).unwrap_or_else(|| label.to_string()),
                None => label.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// `punycode.encode(str)` — Bootstring-encode a Unicode string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_ENCODE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&punycode_encode(s).unwrap_or_default())
}

/// `punycode.decode(str)` — Bootstring-decode back to Unicode.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_DECODE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&punycode_decode(s).unwrap_or_default())
}

/// `punycode.toASCII(domain)` — per-label ACE (`xn--`) encoding.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_TO_ASCII(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&to_ascii(s))
}

/// `punycode.toUnicode(domain)` — per-label ACE decoding.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_PUNYCODE_TO_UNICODE(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&to_unicode(s))
}

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:punycode` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.ns("punycode")
        .doc("Punycode (RFC 3492) + IDNA ToASCII/ToUnicode (node:punycode).")
        .member(pure_func(
            "encode",
            "__RTS_FN_NODE_PUNYCODE_ENCODE",
            sig!(StrPtr => Handle),
            "encode(input: string): string",
            "Bootstring-encodes a Unicode string to Punycode.",
            __RTS_FN_NODE_PUNYCODE_ENCODE as *const u8,
        ))
        .member(pure_func(
            "decode",
            "__RTS_FN_NODE_PUNYCODE_DECODE",
            sig!(StrPtr => Handle),
            "decode(input: string): string",
            "Bootstring-decodes Punycode back to a Unicode string.",
            __RTS_FN_NODE_PUNYCODE_DECODE as *const u8,
        ))
        .member(pure_func(
            "toASCII",
            "__RTS_FN_NODE_PUNYCODE_TO_ASCII",
            sig!(StrPtr => Handle),
            "toASCII(domain: string): string",
            "Converts a domain to ASCII, ACE-encoding non-ASCII labels (xn--).",
            __RTS_FN_NODE_PUNYCODE_TO_ASCII as *const u8,
        ))
        .member(pure_func(
            "toUnicode",
            "__RTS_FN_NODE_PUNYCODE_TO_UNICODE",
            sig!(StrPtr => Handle),
            "toUnicode(domain: string): string",
            "Converts an ACE (xn--) domain back to Unicode.",
            __RTS_FN_NODE_PUNYCODE_TO_UNICODE as *const u8,
        ))
        .done();
}
