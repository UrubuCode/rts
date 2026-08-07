//! `fileURLToPath`/`fileURLToPathBuffer`/`pathToFileURL`/`urlToHttpOptions`,
//! and `domainToASCII`/`domainToUnicode` over the `idna` crate.

use rts_core_rwk::entry;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// `#`/`%` plus the C0 control set — enough for the doc's own examples
/// (`'/foo#1'` → `'file:///foo%231'`, `'/some/path%.c'` →
/// `'file:///some/path%25.c'`); `new URL(...)` supplies the rest of the
/// WHATWG path percent-encoding once this string is handed to it.
const FILE_PATH_ENCODE: &AsciiSet = &CONTROLS.add(b'#').add(b'%');

/// The `url::Url` a `fileURLToPath`-family argument names — either a `URL`
/// instance (read from [`super::class::stored`]) or a plain string.
fn url_of(value: u64) -> Option<url::Url> {
    if let Some(parsed) = super::class::stored(value) {
        return Some(parsed);
    }
    url::Url::parse(&super::text(value)?).ok()
}

fn windows_option(options: u64) -> bool {
    let absent = entry::undefined_value();
    if options == absent {
        return cfg!(windows);
    }
    let value = entry::with_runtime(|context| entry::get_member(context, options, "windows"));
    if value == absent { cfg!(windows) } else { entry::to_boolean(value) }
}

/// The raw, percent-decoded path bytes a `file:` URL names, honoring
/// `windows`. `None` for a non-`file:` scheme, or a POSIX host that is
/// neither empty nor `localhost` — the two documented throw conditions,
/// answered as `None` (→ `undefined`) rather than a `TypeError`; see the
/// module doc.
fn decode_file_path(parsed: &url::Url, windows: bool) -> Option<Vec<u8>> {
    if parsed.scheme() != "file" {
        return None;
    }
    let decoded_path: Vec<u8> = percent_encoding::percent_decode_str(parsed.path()).collect();
    let path_text = String::from_utf8_lossy(&decoded_path).into_owned();
    match windows {
        false => match parsed.host_str() {
            None | Some("") | Some("localhost") => Some(decoded_path),
            Some(_) => None,
        },
        true => {
            let body = path_text.trim_start_matches('/').replace('/', "\\");
            let full = match parsed.host_str() {
                // UNC: `file://server/share/...` → `\\server\share\...`.
                Some(host) if !host.is_empty() && host != "localhost" => format!("\\\\{host}\\{body}"),
                _ => body,
            };
            Some(full.into_bytes())
        }
    }
}

/// `url.fileURLToPath(url, options?)` — `undefined` on any of the documented
/// throw conditions rather than a `TypeError`; see the module doc.
pub(super) extern "C" fn file_url_to_path(_e: u64, _this: u64, url: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let Some(parsed) = url_of(url) else {
        return entry::undefined_value();
    };
    let windows = windows_option(options);
    match decode_file_path(&parsed, windows) {
        Some(bytes) => super::string(&String::from_utf8_lossy(&bytes)),
        None => entry::undefined_value(),
    }
}

/// `url.fileURLToPathBuffer(url, options?)` — the raw bytes, UTF-8 validity
/// not required.
pub(super) extern "C" fn file_url_to_path_buffer(_e: u64, _this: u64, url: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let Some(parsed) = url_of(url) else {
        return entry::undefined_value();
    };
    let windows = windows_option(options);
    match decode_file_path(&parsed, windows) {
        Some(bytes) => entry::with_runtime(|context| entry::make_bytes(context, &bytes)),
        None => entry::undefined_value(),
    }
}

/// `url.pathToFileURL(path, options?)`.
pub(super) extern "C" fn path_to_file_url(_e: u64, _this: u64, path: u64, options: u64, _c: u64, _d: u64) -> u64 {
    let Some(path) = super::text(path) else {
        return entry::undefined_value();
    };
    let windows = windows_option(options);
    let raw = match windows {
        true => {
            let forward = path.replace('\\', "/");
            match forward.strip_prefix("//") {
                // UNC: `\\server\share\...` → `file://server/share/...`.
                Some(rest) => format!("file://{rest}"),
                None if forward.starts_with('/') => format!("file://{forward}"),
                None => format!("file:///{forward}"),
            }
        }
        false => match path.starts_with('/') {
            true => format!("file://{path}"),
            false => format!("file:///{path}"),
        },
    };
    let encoded = encode_file_url(&raw);
    let Ok(parsed) = url::Url::parse(&encoded) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| super::class::from_parsed(context, parsed))
}

/// Percent-encodes `#`/`%` in the PATH portion only — everything up to and
/// including the scheme/authority (`file://` or `file:///`) is left alone
/// so it still parses as the URL structure it is.
fn encode_file_url(raw: &str) -> String {
    let Some(after_scheme) = raw.strip_prefix("file://") else {
        return raw.to_owned();
    };
    format!("file://{}", utf8_percent_encode(after_scheme, FILE_PATH_ENCODE))
}

/// `url.domainToASCII(domain)` — `""` for input `idna` rejects, matching
/// Node's documented no-throw contract.
pub(super) extern "C" fn domain_to_ascii(_e: u64, _this: u64, domain: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let domain = super::text(domain).unwrap_or_default();
    let ascii = idna::domain_to_ascii(&domain).unwrap_or_default();
    super::string(&ascii)
}

/// `url.domainToUnicode(domain)`.
pub(super) extern "C" fn domain_to_unicode(_e: u64, _this: u64, domain: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let domain = super::text(domain).unwrap_or_default();
    let (unicode, result) = idna::domain_to_unicode(&domain);
    let answer = if result.is_ok() { unicode } else { String::new() };
    super::string(&answer)
}

/// `url.urlToHttpOptions(url)` — reads the argument's own properties by
/// name, so a `URL` instance (whose properties are the plain data snapshot
/// [`super::class::refresh`] writes) and a duck-typed plain object both
/// work the same way real Node's own reflection-based reader does.
pub(super) extern "C" fn url_to_http_options(_e: u64, _this: u64, url: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let protocol = entry::get_member(context, url, "protocol");
        let hostname = entry::get_member(context, url, "hostname");
        let hash = entry::get_member(context, url, "hash");
        let search = entry::get_member(context, url, "search");
        let pathname = entry::get_member(context, url, "pathname");
        let href = entry::get_member(context, url, "href");
        let port_raw = entry::get_member(context, url, "port");
        let port_text = entry::text_in(context, port_raw);
        let username_raw = entry::get_member(context, url, "username");
        let username = entry::text_in(context, username_raw).unwrap_or_default();
        let password_raw = entry::get_member(context, url, "password");
        let password = entry::text_in(context, password_raw).unwrap_or_default();

        let object = entry::make_object(context);
        entry::put_member(context, object, "protocol", protocol);
        entry::put_member(context, object, "hostname", hostname);
        entry::put_member(context, object, "hash", hash);
        entry::put_member(context, object, "search", search);
        entry::put_member(context, object, "pathname", pathname);
        let pathname_text = entry::text_in(context, pathname).unwrap_or_default();
        let search_text = entry::text_in(context, search).unwrap_or_default();
        let path = entry::make_string(context, &format!("{pathname_text}{search_text}"));
        entry::put_member(context, object, "path", path);
        entry::put_member(context, object, "href", href);
        let port = match port_text.filter(|text| !text.is_empty()).and_then(|text| text.parse::<f64>().ok()) {
            Some(port) => entry::make_number(port),
            None => entry::undefined_in(context),
        };
        entry::put_member(context, object, "port", port);
        let auth = match (username.is_empty(), password.is_empty()) {
            (true, true) => entry::undefined_in(context),
            _ => entry::make_string(context, &format!("{username}:{password}")),
        };
        entry::put_member(context, object, "auth", auth);
        object
    })
}
