use crate::namespaces::gc::handles::{alloc_entry, free_handle, with_entry, Entry};

struct ParsedUrl {
    href: String,
    protocol: String,
    username: String,
    password: String,
    hostname: String,
    port: String,
    pathname: String,
    search: String,
    hash: String,
}

fn normalize_path(p: &str) -> String {
    // (cross-runtime #746) Resolve "." e ".." em path segments,
    // batendo WHATWG URL spec.
    if p.is_empty() {
        return "/".to_string();
    }
    let trailing_slash = p.ends_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    let mut result = String::from("/");
    result.push_str(&out.join("/"));
    if trailing_slash && !result.ends_with('/') {
        result.push('/');
    }
    result
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
        let (authority, pathname_raw) = match rest.split_once('/') {
            Some((auth, path)) => (auth, format!("/{path}")),
            None => (rest, "/".to_owned()),
        };
        // (cross-runtime #746) Extrai userinfo (`user:pass@`) do authority.
        let (userinfo, host_part) = match authority.rsplit_once('@') {
            Some((ui, host)) => (Some(ui), host),
            None => (None, authority),
        };
        let (username, password) = match userinfo {
            Some(ui) => match ui.split_once(':') {
                Some((u, p)) => (u.to_owned(), p.to_owned()),
                None => (ui.to_owned(), String::new()),
            },
            None => (String::new(), String::new()),
        };
        let (hostname, port) = match host_part.rsplit_once(':') {
            Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
                (h.to_owned(), p.to_owned())
            }
            _ => (host_part.to_owned(), String::new()),
        };
        let pathname = normalize_path(&pathname_raw);
        let host = if port.is_empty() {
            hostname.clone()
        } else {
            format!("{hostname}:{port}")
        };
        let userinfo_str = if username.is_empty() && password.is_empty() {
            String::new()
        } else if password.is_empty() {
            format!("{username}@")
        } else {
            format!("{username}:{password}@")
        };
        let href = format!("{protocol}//{userinfo_str}{host}{pathname}{search}{hash}");
        Some(ParsedUrl {
            href,
            protocol,
            username,
            password,
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
// [href, protocol, host, hostname, port, pathname, search, hash, origin, username, password]

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
            let username_h = intern_str(&u.username);
            let password_h = intern_str(&u.password);
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
                username_h as i64,
                password_h as i64,
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
pub extern "C" fn __RTS_FN_GL_URL_USERNAME(h: u64) -> u64 { url_field(h, 9) }
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PASSWORD(h: u64) -> u64 { url_field(h, 10) }

/// (#373) `url.searchParams` — constroi URLSearchParams a partir do
/// `search` da URL. Cada chamada cria novo handle (sem cache vinculado).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SEARCH_PARAMS(handle: u64) -> u64 {
    let search_h = url_field(handle, 6);
    let search: String = with_entry(search_h, |e| match e {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    });
    // O search inclui o '?' inicial; parse_query strip-prefix
    alloc_search_params(parse_query(&search))
}

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

/// (cross-runtime #746) Percent-decode URL query component conforme
/// WHATWG URL spec: `%XX` -> byte 0xXX; `+` -> ' '; bytes invalidos sao
/// mantidos como-esta. Retorna lossy UTF-8.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            let h = bytes[i + 1];
            let l = bytes[i + 2];
            let hv = match h { b'0'..=b'9' => Some(h - b'0'), b'a'..=b'f' => Some(h - b'a' + 10), b'A'..=b'F' => Some(h - b'A' + 10), _ => None };
            let lv = match l { b'0'..=b'9' => Some(l - b'0'), b'a'..=b'f' => Some(l - b'a' + 10), b'A'..=b'F' => Some(l - b'A' + 10), _ => None };
            if let (Some(hv), Some(lv)) = (hv, lv) {
                out.push((hv << 4) | lv);
                i += 3;
                continue;
            }
        }
        if b == b'+' {
            out.push(b' ');
        } else {
            out.push(b);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(s: &str) -> indexmap::IndexMap<String, String> {
    let mut map: indexmap::IndexMap<String, String> = indexmap::IndexMap::new();
    let s = s.strip_prefix('?').unwrap_or(s);
    if s.is_empty() {
        return map;
    }
    for pair in s.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(url_decode(k), url_decode(v));
        } else if !pair.is_empty() {
            map.insert(url_decode(pair), String::new());
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

/// (#373) `usp.append(key, value)` — v0 limitacao: backing IndexMap
/// nao permite multiple values por key, entao append faz overwrite
/// (mesma semantica de set). JS spec real apendaria nova entry.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_APPEND(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
    value_ptr: i64,
    value_len: i64,
) -> u64 {
    __RTS_FN_GL_USP_SET(self_h, key_ptr, key_len, value_ptr, value_len)
}

/// (#373) `usp.getAll(key)` — v0 retorna Vec com 0 ou 1 elemento.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_GET_ALL(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> u64 {
    let v_h = __RTS_FN_GL_USP_GET(self_h, key_ptr, key_len);
    let mut out: Vec<i64> = Vec::new();
    if v_h != 0 {
        out.push(v_h as i64);
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (#373) `usp.keys()` — Vec de string handles com keys.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_KEYS(self_h: u64) -> u64 {
    let keys: Vec<String> = with_search_map(self_h, Vec::new(), |m| {
        m.keys().cloned().collect()
    });
    let mut out: Vec<i64> = Vec::with_capacity(keys.len());
    for k in keys {
        let h = alloc_entry(Entry::String(k.into_bytes()));
        out.push(h as i64);
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (#373) `usp.values()` — Vec de string handles com values.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_VALUES(self_h: u64) -> u64 {
    let vals: Vec<i64> = with_search_map(self_h, Vec::new(), |m| {
        m.values().copied().collect()
    });
    alloc_entry(Entry::Vec(Box::new(vals)))
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
