//! node:url — `fileURLToPath` / `fileURLToPathBuffer` / `pathToFileURL`, the
//! `file:` URL ⇄ platform-path conversion. Platform-aware (POSIX vs Windows
//! drive-letter/UNC), reusing the real WHATWG `URL` parser for component
//! extraction and construction.

use super::words::{self, arg_href, url_new};

/// `url.fileURLToPath(url)` → the raw path BYTES (for both the string return and
/// the `Buffer` return). `url` may be a `file:` URL string or a `URL` instance.
pub fn file_url_to_path_bytes(url_arg: u64) -> Vec<u8> {
    let href = arg_href(url_arg);
    let u = url_new(&href);
    let hostname = words::url_hostname(u);
    let pathname = words::url_pathname(u);

    #[cfg(windows)]
    {
        if !hostname.is_empty() {
            // UNC: file://host/share → \\host\share
            let rest = percent_decode(&pathname).replace('/', "\\");
            return format!("\\\\{hostname}{rest}").into_bytes();
        }
        // Drive path: /C:/x → C:\x
        let decoded = percent_decode(&pathname);
        let trimmed = decoded.strip_prefix('/').unwrap_or(&decoded);
        return trimmed.replace('/', "\\").into_bytes();
    }
    #[cfg(not(windows))]
    {
        let _ = hostname;
        percent_decode(&pathname).into_bytes()
    }
}

/// `url.pathToFileURL(path)` → a real `URL` handle for the `file:` URL.
pub fn path_to_file_url(path: &str) -> u64 {
    let absolute = to_absolute(path);
    #[cfg(windows)]
    let normalized = absolute.replace('\\', "/");
    #[cfg(not(windows))]
    let normalized = absolute;

    let encoded = encode_path(&normalized);
    // A drive/absolute path already starts with '/'; a Windows drive path
    // ("C:/x") needs a leading '/'.
    let with_root = if encoded.starts_with('/') {
        encoded
    } else {
        format!("/{encoded}")
    };
    url_new(&format!("file://{with_root}"))
}

/// Make `path` absolute (prefix the process CWD when relative).
fn to_absolute(path: &str) -> String {
    let is_abs = path.starts_with('/')
        || (cfg!(windows)
            && path.len() >= 2
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':');
    if is_abs {
        return path.to_string();
    }
    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sep = if cfg!(windows) { "\\" } else { "/" };
    format!("{cwd}{sep}{path}")
}

/// Percent-encode the path chars the WHATWG path set (and Node's `pathToFileURL`)
/// encode: `%`, `#`, `?`, and control characters. `/` stays a separator.
fn encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'%' => out.push_str("%25"),
            b'#' => out.push_str("%23"),
            b'?' => out.push_str("%3F"),
            b'\n' => out.push_str("%0A"),
            b'\r' => out.push_str("%0D"),
            b'\t' => out.push_str("%09"),
            _ => out.push(b as char),
        }
    }
    out
}

/// `decodeURIComponent`-style percent-decode (lenient: a malformed `%XX` is kept
/// literal — a URL pathname is always well-formed here anyway).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    i += 3;
                    continue;
                }
                _ => {}
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
