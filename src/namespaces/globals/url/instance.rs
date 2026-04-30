use crate::namespaces::gc::handles::{alloc_entry, free_handle, shard_for_handle, Entry};

struct ParsedUrl {
    href: String,
    protocol: String,
    hostname: String,
    port: String,
    pathname: String,
    search: String,
    hash: String,
}

impl ParsedUrl {
    fn parse(raw: &str) -> Option<Self> {
        // Minimal URL parser without external deps.
        let (raw, hash) = match raw.split_once('#') {
            Some((before, after)) => (before, format!("#{after}")),
            None => (raw, String::new()),
        };
        let (raw, search) = match raw.split_once('?') {
            Some((before, after)) => (before, format!("?{after}")),
            None => (raw, String::new()),
        };
        let (scheme, rest) = raw.split_once("://")?;
        let protocol = format!("{scheme}:");
        let (authority, pathname) = match rest.split_once('/') {
            Some((auth, path)) => (auth, format!("/{path}")),
            None => (rest, "/".to_owned()),
        };
        let (hostname, port) = match authority.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
                (h.to_owned(), p.to_owned())
            }
            _ => (authority.to_owned(), String::new()),
        };
        let host = if port.is_empty() {
            hostname.clone()
        } else {
            format!("{hostname}:{port}")
        };
        let href = format!("{protocol}//{host}{pathname}{search}{hash}");
        Some(ParsedUrl {
            href,
            protocol,
            hostname,
            port,
            pathname,
            search,
            hash,
        })
    }

    fn host(&self) -> String {
        if self.port.is_empty() {
            self.hostname.clone()
        } else {
            format!("{}:{}", self.hostname, self.port)
        }
    }

    fn origin(&self) -> String {
        format!("{}//{}", self.protocol, self.host())
    }
}

// Store as Entry::Env with string handles for each component:
// [href_h, protocol_h, hostname_h, port_h, pathname_h, search_h, hash_h, origin_h]

fn intern_str(s: &str) -> u64 {
    alloc_entry(Entry::String(s.as_bytes().to_vec()))
}

fn str_from_parts(ptr: i64, len: i64) -> &'static str {
    if ptr == 0 || len == 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_NEW(ptr: i64, len: i64) -> u64 {
    let raw = str_from_parts(ptr, len);
    match ParsedUrl::parse(raw) {
        None => 0,
        Some(u) => {
            let href_h = intern_str(&u.href);
            let proto_h = intern_str(&u.protocol);
            let host_h = intern_str(&u.host());
            let hostname_h = intern_str(&u.hostname);
            let port_h = intern_str(&u.port);
            let pathname_h = intern_str(&u.pathname);
            let search_h = intern_str(&u.search);
            let hash_h = intern_str(&u.hash);
            let origin_h = intern_str(&u.origin());
            alloc_entry(Entry::Env(vec![
                href_h as i64,
                proto_h as i64,
                host_h as i64,
                hostname_h as i64,
                port_h as i64,
                pathname_h as i64,
                search_h as i64,
                hash_h as i64,
                origin_h as i64,
            ]))
        }
    }
}

fn url_field(handle: u64, idx: usize) -> u64 {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::Env(v)) if v.len() > idx => v[idx] as u64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HREF(h: u64) -> u64     { url_field(h, 0) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PROTOCOL(h: u64) -> u64 { url_field(h, 1) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HOST(h: u64) -> u64     { url_field(h, 2) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HOSTNAME(h: u64) -> u64 { url_field(h, 3) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PORT(h: u64) -> u64     { url_field(h, 4) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PATHNAME(h: u64) -> u64 { url_field(h, 5) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SEARCH(h: u64) -> u64   { url_field(h, 6) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HASH(h: u64) -> u64     { url_field(h, 7) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_ORIGIN(h: u64) -> u64   { url_field(h, 8) }

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_FREE(handle: u64) {
    // Free inner string handles
    let fields: Vec<u64> = {
        let guard = shard_for_handle(handle).lock().unwrap();
        match guard.get(handle) {
            Some(Entry::Env(v)) => v.iter().map(|&x| x as u64).collect(),
            _ => vec![],
        }
    };
    for fh in fields {
        if fh != 0 {
            free_handle(fh);
        }
    }
    free_handle(handle);
}
