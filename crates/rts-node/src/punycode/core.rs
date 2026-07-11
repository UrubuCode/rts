//! node:punycode — the label/domain layer over the Bootstring core: single-
//! label `encode`/`decode` (code-point bridging) and the `toASCII`/`toUnicode`
//! domain wrappers (`@`-split, alternate-separator normalization, per-label
//! `xn--` handling). Pure string computation.

use super::bootstring::{decode, encode, PunyError};

/// `punycode.encode(label)` — code points → Punycode (single label).
pub fn encode_label(s: &str) -> Result<String, PunyError> {
    let cps: Vec<u32> = s.chars().map(|c| c as u32).collect();
    encode(&cps)
}

/// `punycode.decode(label)` — Punycode → Unicode string (single label).
pub fn decode_label(s: &str) -> Result<String, PunyError> {
    let cps = decode(s)?;
    let mut out = String::new();
    for cp in cps {
        match char::from_u32(cp) {
            Some(c) => out.push(c),
            None => return Err(PunyError("Invalid input")),
        }
    }
    Ok(out)
}

/// `punycode.toASCII(domain)`.
pub fn to_ascii(domain: &str) -> Result<String, PunyError> {
    let (prefix, dom) = split_userinfo(domain);
    let normalized = normalize_separators(dom);
    let mut labels = Vec::new();
    for label in normalized.split('.') {
        if label.chars().any(|c| c as u32 >= 0x80) {
            labels.push(format!("xn--{}", encode_label(label)?));
        } else {
            labels.push(label.to_string());
        }
    }
    Ok(format!("{prefix}{}", labels.join(".")))
}

/// `punycode.toUnicode(domain)`.
pub fn to_unicode(domain: &str) -> Result<String, PunyError> {
    let (prefix, dom) = split_userinfo(domain);
    let normalized = normalize_separators(dom);
    let mut labels = Vec::new();
    for label in normalized.split('.') {
        if label.to_lowercase().starts_with("xn--") {
            labels.push(decode_label(&label[4..])?);
        } else {
            labels.push(label.to_string());
        }
    }
    Ok(format!("{prefix}{}", labels.join(".")))
}

/// Split off a leading `user@` (userinfo) part: everything up to and including
/// the last `@` is the passthrough prefix; the rest is the domain.
fn split_userinfo(domain: &str) -> (&str, &str) {
    match domain.rfind('@') {
        Some(i) => domain.split_at(i + 1),
        None => ("", domain),
    }
}

/// Normalize the three alternate ideographic label separators to ASCII `.`.
fn normalize_separators(s: &str) -> String {
    s.replace(['\u{3002}', '\u{FF0E}', '\u{FF61}'], ".")
}
