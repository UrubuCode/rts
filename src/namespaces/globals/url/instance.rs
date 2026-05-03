use crate::namespaces::gc::handles::{alloc_entry, free_handle, with_entry, Entry};

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
    with_entry(handle, |entry| match entry {
        Some(Entry::Env(v)) if v.len() > idx => v[idx] as u64,
        _ => 0,
    })
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
    // Collect inner string handles outside any with_entry closure to avoid
    // nested lock acquisition (deadlock risk if same shard).
    let fields: Vec<u64> = with_entry(handle, |entry| match entry {
        Some(Entry::Env(v)) => v.iter().map(|&x| x as u64).collect(),
        _ => vec![],
    });
    for fh in fields {
        if fh != 0 {
            free_handle(fh);
        }
    }
    free_handle(handle);
}

// (#373) URLSearchParams — backing IndexMap<String, i64> onde i64 e' handle
// de string com value. Implementacao minimal: get/has/set/delete/toString.

fn parse_query(s: &str) -> indexmap::IndexMap<String, String> {
    let mut map: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
    let s = s.strip_prefix('?').unwrap_or(s);
    if s.is_empty() {
        return map;
    }
    for pair in s.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        } else if !pair.is_empty() {
            map.insert(pair.to_string(), String::new());
        }
    }
    map
}

fn alloc_search_params(map: indexmap::IndexMap<String, String>) -> u64 {
    let mut store: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
    for (k, v) in map {
        let h = alloc_entry(Entry::String(v.into_bytes()));
        store.insert(k, h as i64);
    }
    alloc_entry(Entry::Map(Box::new(store)))
}

fn with_search_map<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&indexmap::IndexMap<String, i64>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_ref()),
        _ => default,
    })
}

fn with_search_map_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut indexmap::IndexMap<String, i64>) -> R,
{
    crate::namespaces::gc::handles::with_entry_mut(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_mut()),
        _ => default,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_NEW(init_ptr: i64, init_len: i64) -> u64 {
    let s = if init_ptr == 0 || init_len <= 0 {
        ""
    } else {
        unsafe {
            let slice = std::slice::from_raw_parts(init_ptr as *const u8, init_len as usize);
            std::str::from_utf8(slice).unwrap_or("")
        }
    };
    alloc_search_params(parse_query(s))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_GET(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> u64 {
    if key_ptr.is_null() || key_len < 0 {
        return 0;
    }
    let key = unsafe {
        let s = std::slice::from_raw_parts(key_ptr, key_len as usize);
        std::str::from_utf8(s).unwrap_or("")
    };
    let h: i64 = with_search_map(self_h, 0, |m| m.get(key).copied().unwrap_or(0));
    h as u64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_HAS(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    if key_ptr.is_null() || key_len < 0 {
        return 0;
    }
    let key = unsafe {
        let s = std::slice::from_raw_parts(key_ptr, key_len as usize);
        std::str::from_utf8(s).unwrap_or("")
    };
    with_search_map(self_h, 0, |m| if m.contains_key(key) { 1 } else { 0 })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_SET(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
    value_ptr: i64,
    value_len: i64,
) -> u64 {
    if key_ptr.is_null() || key_len < 0 {
        return self_h;
    }
    let key = unsafe {
        let s = std::slice::from_raw_parts(key_ptr, key_len as usize);
        std::str::from_utf8(s).unwrap_or("").to_string()
    };
    let val_bytes: Vec<u8> = if value_ptr == 0 || value_len <= 0 {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(value_ptr as *const u8, value_len as usize).to_vec()
        }
    };
    let val_h = alloc_entry(Entry::String(val_bytes));
    with_search_map_mut(self_h, (), |m| {
        m.insert(key, val_h as i64);
    });
    self_h
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_DELETE(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> u64 {
    if key_ptr.is_null() || key_len < 0 {
        return self_h;
    }
    let key = unsafe {
        let s = std::slice::from_raw_parts(key_ptr, key_len as usize);
        std::str::from_utf8(s).unwrap_or("")
    };
    with_search_map_mut(self_h, (), |m| {
        m.shift_remove(key);
    });
    self_h
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_TO_STRING(self_h: u64) -> u64 {
    let pairs: Vec<(String, String)> = with_search_map(self_h, Vec::new(), |m| {
        m.iter()
            .map(|(k, v_h)| {
                let val: String = with_entry(*v_h as u64, |e| match e {
                    Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                    _ => String::new(),
                });
                (k.clone(), val)
            })
            .collect()
    });
    let parts: Vec<String> = pairs.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    let s = parts.join("&");
    alloc_entry(Entry::String(s.into_bytes()))
}
