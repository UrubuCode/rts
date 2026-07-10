//! node:url — base extern "C" symbol implementations (the sync surface).
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (`None` on null / invalid UTF-8,
//! mapped to the ABI's error sentinel — `0`/empty — never a panic); string
//! results are interned to GC string handles. Symbols follow the rts-node
//! convention `__RTS_FN_NODE_URL_*`.
//!
//! This slice covers the four flat `string -> string` conversions
//! `url.fileURLToPath`/`url.pathToFileURL`/`url.domainToASCII`/
//! `url.domainToUnicode`. Fully self-contained: the RFC 3492 Bootstring
//! (punycode) codec and the percent-encode/decode helpers are reimplemented
//! here rather than reused from `node:punycode` — no cross-module
//! dependency, per the module's isolation convention.
//!
//! The legacy `url.parse(urlString)` object-returning function lives in the
//! sibling `legacy_parse` submodule (kept separate for the 500-line-per-file
//! layout rule) and is re-exported below.
//!
//! **Deferred** (need object/class machinery this flat-function slice
//! doesn't have): the `URL`/`URLSearchParams` classes (covered as *global*
//! classes elsewhere, `rts-shared` `globals/url`, not duplicated here);
//! `url.format(urlObject)` (reads an object handle) / `url.resolve(from,
//! to)`; full IDNA (UTS #46) — `domainToASCII`/`domainToUnicode` here do
//! real per-label Bootstring conversion (the part that changes the wire
//! bytes) plus ASCII lowercasing, not the full Unicode normalization table.

use rts_engine::abi::str_abi::from_abi;

mod legacy_parse;
pub use legacy_parse::__RTS_FN_NODE_URL_PARSE;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Interns a Rust string as a GC string handle (the ABI `Handle` return).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ---------------------------------------------------------------------------
// RFC 3492 Bootstring (punycode) — self-contained copy (see module doc: no
// dependency on `node:punycode`).
// ---------------------------------------------------------------------------

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

/// Punycode-encode a single label (no `xn--` prefix). `None` on overflow.
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

/// Punycode-decode a single label (no `xn--` prefix). `None` on error.
fn punycode_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output: Vec<u32> = Vec::new();

    // Everything before the last '-' is basic (copied verbatim); decoding of
    // the non-basic insertions begins after it. No '-' ⇒ empty basic, decode
    // from 0.
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

/// `url.domainToASCII(domain)` — per-label IDNA ASCII: ASCII labels are kept
/// (lowercased), non-ASCII labels become `xn--<punycode>`. A single trailing
/// empty label (an FQDN's trailing dot, e.g. `"example.com."`) is preserved;
/// any other empty label, or a label that fails to encode, makes the whole
/// domain invalid — Node returns `""` in that case.
fn domain_to_ascii(domain: &str) -> String {
    if domain.is_empty() {
        return String::new();
    }
    let labels: Vec<&str> = domain.split('.').collect();
    let mut out = Vec::with_capacity(labels.len());
    for (i, label) in labels.iter().enumerate() {
        if label.is_empty() {
            if i == labels.len() - 1 && i != 0 {
                out.push(String::new());
                continue;
            }
            return String::new();
        }
        if label.is_ascii() {
            out.push(label.to_ascii_lowercase());
        } else {
            match punycode_encode(label) {
                Some(enc) => out.push(format!("xn--{enc}")),
                None => return String::new(),
            }
        }
    }
    out.join(".")
}

/// `url.domainToUnicode(domain)` — reverse of [`domain_to_ascii`]: an
/// `xn--`-prefixed label (case-insensitive) is punycode-decoded back to
/// Unicode; any other label is kept as-is. A label that fails to decode
/// makes the whole domain invalid — Node returns `""`.
fn domain_to_unicode(domain: &str) -> String {
    if domain.is_empty() {
        return String::new();
    }
    let labels: Vec<&str> = domain.split('.').collect();
    let mut out = Vec::with_capacity(labels.len());
    for (i, label) in labels.iter().enumerate() {
        if label.is_empty() {
            if i == labels.len() - 1 && i != 0 {
                out.push(String::new());
                continue;
            }
            return String::new();
        }
        let lower = label.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("xn--") {
            match punycode_decode(rest) {
                Some(dec) => out.push(dec),
                None => return String::new(),
            }
        } else {
            out.push((*label).to_string());
        }
    }
    out.join(".")
}

// ---------------------------------------------------------------------------
// Percent-encode / decode (path-safe set) — used by fileURLToPath /
// pathToFileURL.
// ---------------------------------------------------------------------------

fn hex_upper(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'A' + (nibble - 10),
    }
}

/// Bytes left unescaped when percent-encoding a path into a `file:` URL —
/// unreserved + sub-delims + `:` `@` `/` (the WHATWG URL path-safe set). A
/// literal `%` is NOT in this set, so a raw `%` in the input path re-encodes
/// to `%25`, matching Node's own `pathToFileURL`.
fn is_url_path_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'.'
                | b'_'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'/'
        )
}

fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if is_url_path_safe(b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex_upper(b >> 4) as char);
            out.push(hex_upper(b & 0x0F) as char);
        }
    }
    out
}

/// Percent-decodes a string. `None` on a truncated/malformed `%XX` escape or
/// a resulting byte sequence that isn't valid UTF-8 (both treated as an
/// invalid file URL by the caller).
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = (bytes[i + 1] as char).to_digit(16)?;
            let lo = (bytes[i + 2] as char).to_digit(16)?;
            out.push(((hi << 4) | lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

// ---------------------------------------------------------------------------
// fileURLToPath / pathToFileURL
// ---------------------------------------------------------------------------

/// Strips the `file:` scheme + `//` authority marker from a URL string.
/// `None` when the scheme isn't `file` (case-insensitive, matching the
/// WHATWG URL parser Node uses) or the `//` marker is missing — the
/// "non-file URL" case the caller turns into `0`.
fn strip_file_scheme(url: &str) -> Option<&str> {
    let colon = url.find(':')?;
    if !url[..colon].eq_ignore_ascii_case("file") {
        return None;
    }
    url[colon + 1..].strip_prefix("//")
}

/// Splits what's left after [`strip_file_scheme`] into `(authority, path)`:
/// authority is everything before the first `/`, path is from that `/` on
/// (inclusive). No `/` at all ⇒ the whole remainder is the authority, empty
/// path.
fn split_authority_path(rest: &str) -> (&str, &str) {
    match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    }
}

/// Drops the leading `/` before a Windows drive letter (`/C:/x` -> `C:/x`);
/// a driveless path is returned unchanged.
#[cfg(windows)]
fn strip_windows_drive_slash(path: &str) -> String {
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        path[1..].to_string()
    } else {
        path.to_string()
    }
}

/// `url.fileURLToPath(url)` on Windows: `file:///C:/x` -> `C:\x` (drive
/// path); `file://host/share/x` -> `\\host\share\x` (UNC, real Node
/// behavior for a non-`localhost` authority).
#[cfg(windows)]
fn file_url_to_path(url: &str) -> Option<String> {
    let rest = strip_file_scheme(url)?;
    let (authority, raw_path) = split_authority_path(rest);
    let decoded_path = percent_decode(raw_path)?;
    if authority.is_empty() || authority.eq_ignore_ascii_case("localhost") {
        let drive_path = strip_windows_drive_slash(&decoded_path);
        Some(drive_path.replace('/', "\\"))
    } else {
        let decoded_host = percent_decode(authority)?;
        let unc_tail = decoded_path.replace('/', "\\");
        Some(format!("\\\\{decoded_host}{unc_tail}"))
    }
}

/// `url.fileURLToPath(url)` on Unix: `file:///home/x` -> `/home/x`. A
/// non-empty, non-`localhost` host has no meaning for a POSIX path (Node
/// throws `ERR_INVALID_FILE_URL_HOST` here) — treated as failure (`None`).
#[cfg(not(windows))]
fn file_url_to_path(url: &str) -> Option<String> {
    let rest = strip_file_scheme(url)?;
    let (authority, raw_path) = split_authority_path(rest);
    if !(authority.is_empty() || authority.eq_ignore_ascii_case("localhost")) {
        return None;
    }
    percent_decode(raw_path)
}

/// `url.pathToFileURL(path)` on Windows: `C:\x` -> `file:///C:/x`;
/// `\\host\share\x` (UNC) -> `file://host/share/x`.
#[cfg(windows)]
fn path_to_file_url(path: &str) -> String {
    if let Some(tail) = path.strip_prefix("\\\\").or_else(|| path.strip_prefix("//")) {
        let (host, rest) = match tail.find(['\\', '/']) {
            Some(idx) => (&tail[..idx], &tail[idx..]),
            None => (tail, ""),
        };
        let rest_fwd = rest.replace('\\', "/");
        format!(
            "file://{}{}",
            percent_encode_path(host),
            percent_encode_path(&rest_fwd)
        )
    } else {
        let fwd = path.replace('\\', "/");
        let with_slash = if fwd.starts_with('/') {
            fwd
        } else {
            format!("/{fwd}")
        };
        format!("file://{}", percent_encode_path(&with_slash))
    }
}

/// `url.pathToFileURL(path)` on Unix: `/home/x` -> `file:///home/x`.
#[cfg(not(windows))]
fn path_to_file_url(path: &str) -> String {
    let with_slash = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("file://{}", percent_encode_path(&with_slash))
}

// ---------------------------------------------------------------------------
// extern "C" surface
// ---------------------------------------------------------------------------

/// `url.fileURLToPath(url)`. Returns `0` for a non-`file:` URL or one that
/// fails to decode (malformed percent-encoding, invalid host on POSIX).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_FILE_URL_TO_PATH(ptr: *const u8, len: i64) -> u64 {
    let Some(url) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    match file_url_to_path(url) {
        Some(path) => intern(&path),
        None => 0,
    }
}

/// `url.pathToFileURL(path)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_PATH_TO_FILE_URL(ptr: *const u8, len: i64) -> u64 {
    let Some(path) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&path_to_file_url(path))
}

/// `url.domainToASCII(domain)`. Empty string on an invalid domain (matches
/// Node).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_DOMAIN_TO_ASCII(ptr: *const u8, len: i64) -> u64 {
    let Some(domain) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&domain_to_ascii(domain))
}

/// `url.domainToUnicode(domain)`. Empty string on an invalid domain (matches
/// Node).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_DOMAIN_TO_UNICODE(ptr: *const u8, len: i64) -> u64 {
    let Some(domain) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    intern(&domain_to_unicode(domain))
}
