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
        // (cross-runtime #287) IPv6 literals: `[addr]` ou `[addr]:port`.
        // Hostname mantem os brackets. Rejeita IPv6 invalido (>2 `::` ou
        // chars nao-hex/colon dentro dos brackets).
        let (hostname, port) = if let Some(rest_after_lb) = host_part.strip_prefix('[') {
            let (ipv6, after_rb) = match rest_after_lb.split_once(']') {
                Some(parts) => parts,
                None => return None, // bracket nao fechado
            };
            // Valida conteudo IPv6: apenas hex digits e ':' permitidos;
            // no maximo 1 ocorrencia de `::` (zero-compression).
            if ipv6.is_empty() {
                return None;
            }
            if !ipv6.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
                return None;
            }
            if ipv6.matches("::").count() > 1 {
                return None;
            }
            // Adicionalmente rejeita 3+ colons consecutivos (eg `:::1`).
            if ipv6.contains(":::") {
                return None;
            }
            let host_with_brackets = format!("[{ipv6}]");
            let port = if let Some(p) = after_rb.strip_prefix(':') {
                if !p.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }
                p.to_owned()
            } else if after_rb.is_empty() {
                String::new()
            } else {
                return None;
            };
            (host_with_brackets, port)
        } else {
            match host_part.rsplit_once(':') {
                Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
                    (h.to_owned(), p.to_owned())
                }
                _ => (host_part.to_owned(), String::new()),
            }
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

/// (cross-runtime #746) URL.canParse(href) — true se o URL parsa.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_CAN_PARSE(ptr: i64, len: i64) -> i64 {
    let raw = str_from_parts(ptr, len);
    if ParsedUrl::parse(raw).is_some() { 1 } else { 0 }
}

/// URL.canParse(href, base) — true se href resolve relativo a base.
/// Implementacao minima: tenta parse(href); se falhar, tenta concatenar
/// com base + "/" + href; retorna true se algum parsa.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_CAN_PARSE_BASE(
    href_ptr: i64, href_len: i64,
    base_ptr: i64, base_len: i64,
) -> i64 {
    let href = str_from_parts(href_ptr, href_len);
    let base = str_from_parts(base_ptr, base_len);
    // Tenta parse direto primeiro (href ja absoluto).
    if ParsedUrl::parse(href).is_some() { return 1; }
    // base precisa parsar.
    let Some(base_url) = ParsedUrl::parse(base) else { return 0 };
    // Resolve href relativo a base. Para "/x" + "https://example.com",
    // retorna "https://example.com/x". Para path complexo, usa join naive.
    let combined = if href.starts_with('/') {
        format!("{}://{}{}", base_url.protocol.trim_end_matches(':'), base_url.host(), href)
    } else {
        format!("{}://{}/{}", base_url.protocol.trim_end_matches(':'), base_url.host(), href)
    };
    if ParsedUrl::parse(&combined).is_some() { 1 } else { 0 }
}

/// (#67) `new URL(relative, base)` — resolve URL relativa contra base.
/// Implementacao minima: detecta protocol absoluto na relativa; senao
/// merge basico path resolution.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_NEW_WITH_BASE(
    rel_ptr: i64,
    rel_len: i64,
    base_ptr: i64,
    base_len: i64,
) -> u64 {
    let rel = str_from_parts(rel_ptr, rel_len);
    let base = str_from_parts(base_ptr, base_len);
    // Se relative tem scheme proprio (http://, etc), usa direto.
    if rel.contains("://") {
        return __RTS_FN_GL_URL_NEW(rel_ptr, rel_len);
    }
    // Parse base.
    let base_parsed = match ParsedUrl::parse(base) {
        Some(p) => p,
        None => return 0,
    };
    // Resolve path relativo contra base.pathname.
    let resolved_path = if rel.starts_with('/') {
        // Absoluto na host.
        rel.to_string()
    } else {
        // Relativo: merge com base path (strip ultimo segmento da base).
        let base_dir = match base_parsed.pathname.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => "/".to_string(),
        };
        format!("{base_dir}{rel}")
    };
    // Normaliza segmentos (resolve ../ e ./).
    let combined = format!(
        "{}//{}{}",
        base_parsed.protocol,
        base_parsed.host(),
        resolved_path
    );
    // Reusa parse pra normalizar path + extract search/hash.
    let new_raw = combined;
    let bytes = new_raw.as_bytes();
    __RTS_FN_GL_URL_NEW(bytes.as_ptr() as i64, bytes.len() as i64)
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
    // O search inclui o '?' inicial; parse_query_pairs strip-prefix
    alloc_search_params_vec(parse_query_pairs(&search))
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

// (#373/#80) URLSearchParams — backing Entry::Vec<i64> onde pares
// consecutivos (slot 2i, slot 2i+1) sao (key_handle, value_handle).
// Multimap real para suportar append + sort + getAll com duplicatas.

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

/// Parses raw query como Vec<(key, value)> preservando ordem + duplicatas.
fn parse_query_pairs(s: &str) -> Vec<(String, String)> {
    let s = s.strip_prefix('?').unwrap_or(s);
    if s.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<(String, String)> = Vec::new();
    for pair in s.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            out.push((url_decode(k), url_decode(v)));
        } else if !pair.is_empty() {
            out.push((url_decode(pair), String::new()));
        }
    }
    out
}

/// Aloca handle Entry::Vec contendo pares (key_h, value_h) intercalados.
fn alloc_search_params_vec(pairs: Vec<(String, String)>) -> u64 {
    let mut slots: Vec<i64> = Vec::with_capacity(pairs.len() * 2);
    for (k, v) in pairs {
        let kh = alloc_entry(Entry::String(k.into_bytes())) as i64;
        let vh = alloc_entry(Entry::String(v.into_bytes())) as i64;
        slots.push(kh);
        slots.push(vh);
    }
    alloc_entry(Entry::Vec(Box::new(slots)))
}

fn with_usp_pairs<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Vec<i64>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => f(v.as_ref()),
        _ => default,
    })
}

fn with_usp_pairs_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut Vec<i64>) -> R,
{
    crate::namespaces::gc::handles::with_entry_mut(handle, |entry| match entry {
        Some(Entry::Vec(v)) => f(v.as_mut()),
        _ => default,
    })
}

/// Resolve key handle para string (lookup no HandleTable). Util para
/// comparar/match contra chave de usuario passada em ABI.
fn key_str_of_handle(h: i64) -> Option<String> {
    if h <= 0 {
        return None;
    }
    with_entry(h as u64, |e| match e {
        Some(Entry::String(b)) => std::str::from_utf8(b).ok().map(|s| s.to_string()),
        _ => None,
    })
}

/// (cross-runtime #80) URL-encode form-data: ' '→'+', alfanumeric e
/// `-._~*` literais, demais bytes virram `%XX`. Aplicado em values
/// (toString JS spec usa application/x-www-form-urlencoded).
fn url_encode_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b' ' => out.push('+'),
            b if b.is_ascii_alphanumeric() => out.push(b as char),
            b'-' | b'.' | b'_' | b'*' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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
    alloc_search_params_vec(parse_query_pairs(s))
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
    // Primeira ocorrencia (JS spec).
    with_usp_pairs(self_h, 0u64, |slots| {
        let mut i = 0;
        while i + 1 < slots.len() {
            if let Some(k) = key_str_of_handle(slots[i]) {
                if k == key {
                    return slots[i + 1] as u64;
                }
            }
            i += 2;
        }
        0
    })
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
    with_usp_pairs(self_h, 0, |slots| {
        let mut i = 0;
        while i + 1 < slots.len() {
            if let Some(k) = key_str_of_handle(slots[i]) {
                if k == key { return 1; }
            }
            i += 2;
        }
        0
    })
}

/// (#373) `usp.set(key, value)` — JS spec: substitui TODAS as ocorrencias
/// existentes da key pelo primeiro slot com value novo; demais removidas.
/// Se key nao existe, appenda no fim.
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
    let val_h = alloc_entry(Entry::String(val_bytes)) as i64;
    with_usp_pairs_mut(self_h, (), |slots| {
        // Encontra primeira ocorrencia, substitui value; remove demais.
        let mut replaced = false;
        let mut i = 0;
        let mut new_slots: Vec<i64> = Vec::with_capacity(slots.len());
        while i + 1 < slots.len() {
            let k_match = key_str_of_handle(slots[i])
                .map(|s| s == key)
                .unwrap_or(false);
            if k_match {
                if !replaced {
                    new_slots.push(slots[i]);
                    new_slots.push(val_h);
                    replaced = true;
                }
                // else: skip (remove duplicata)
            } else {
                new_slots.push(slots[i]);
                new_slots.push(slots[i + 1]);
            }
            i += 2;
        }
        if !replaced {
            let kh = alloc_entry(Entry::String(key.into_bytes())) as i64;
            new_slots.push(kh);
            new_slots.push(val_h);
        }
        *slots = new_slots;
    });
    self_h
}

/// (#373) `usp.delete(key)` — remove TODAS as ocorrencias da key.
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
        std::str::from_utf8(s).unwrap_or("").to_string()
    };
    with_usp_pairs_mut(self_h, (), |slots| {
        let mut new_slots: Vec<i64> = Vec::with_capacity(slots.len());
        let mut i = 0;
        while i + 1 < slots.len() {
            let k_match = key_str_of_handle(slots[i])
                .map(|s| s == key)
                .unwrap_or(false);
            if !k_match {
                new_slots.push(slots[i]);
                new_slots.push(slots[i + 1]);
            }
            i += 2;
        }
        *slots = new_slots;
    });
    self_h
}

/// (#373/#80) `usp.append(key, value)` — appenda nova entry sempre
/// preservando duplicatas.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_APPEND(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
    value_ptr: i64,
    value_len: i64,
) -> u64 {
    if key_ptr.is_null() || key_len < 0 {
        return self_h;
    }
    let key_bytes: Vec<u8> = unsafe {
        std::slice::from_raw_parts(key_ptr, key_len as usize).to_vec()
    };
    let val_bytes: Vec<u8> = if value_ptr == 0 || value_len <= 0 {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(value_ptr as *const u8, value_len as usize).to_vec()
        }
    };
    let kh = alloc_entry(Entry::String(key_bytes)) as i64;
    let vh = alloc_entry(Entry::String(val_bytes)) as i64;
    with_usp_pairs_mut(self_h, (), |slots| {
        slots.push(kh);
        slots.push(vh);
    });
    self_h
}

/// (#373/#80) `usp.getAll(key)` — retorna todos os values da key, na
/// ordem em que foram appended.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_GET_ALL(
    self_h: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> u64 {
    if key_ptr.is_null() || key_len < 0 {
        return alloc_entry(Entry::Vec(Box::new(Vec::new())));
    }
    let key = unsafe {
        let s = std::slice::from_raw_parts(key_ptr, key_len as usize);
        std::str::from_utf8(s).unwrap_or("").to_string()
    };
    let vals: Vec<i64> = with_usp_pairs(self_h, Vec::new(), |slots| {
        let mut out: Vec<i64> = Vec::new();
        let mut i = 0;
        while i + 1 < slots.len() {
            if let Some(k) = key_str_of_handle(slots[i]) {
                if k == key {
                    out.push(slots[i + 1]);
                }
            }
            i += 2;
        }
        out
    });
    alloc_entry(Entry::Vec(Box::new(vals)))
}

/// (#373) `usp.keys()` — Vec de string handles com keys (preservando
/// ordem + duplicatas).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_KEYS(self_h: u64) -> u64 {
    let out: Vec<i64> = with_usp_pairs(self_h, Vec::new(), |slots| {
        let mut out: Vec<i64> = Vec::with_capacity(slots.len() / 2);
        let mut i = 0;
        while i + 1 < slots.len() {
            out.push(slots[i]);
            i += 2;
        }
        out
    });
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (#373) `usp.values()` — Vec de string handles com values (preservando
/// ordem + duplicatas).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_VALUES(self_h: u64) -> u64 {
    let out: Vec<i64> = with_usp_pairs(self_h, Vec::new(), |slots| {
        let mut out: Vec<i64> = Vec::with_capacity(slots.len() / 2);
        let mut i = 0;
        while i + 1 < slots.len() {
            out.push(slots[i + 1]);
            i += 2;
        }
        out
    });
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// (cross-runtime #80) `usp.sort()` — sort estavel por nome de key
/// (JS spec: ordenacao por UTF-16 code units; aproximamos por bytes UTF-8).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_SORT(self_h: u64) -> u64 {
    with_usp_pairs_mut(self_h, (), |slots| {
        // Coleta pares (key_str, key_h, val_h) e sort por key_str.
        let mut pairs: Vec<(String, i64, i64)> = Vec::with_capacity(slots.len() / 2);
        let mut i = 0;
        while i + 1 < slots.len() {
            let k = key_str_of_handle(slots[i]).unwrap_or_default();
            pairs.push((k, slots[i], slots[i + 1]));
            i += 2;
        }
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let mut new_slots: Vec<i64> = Vec::with_capacity(slots.len());
        for (_, kh, vh) in pairs {
            new_slots.push(kh);
            new_slots.push(vh);
        }
        *slots = new_slots;
    });
    self_h
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_USP_TO_STRING(self_h: u64) -> u64 {
    let pairs: Vec<(String, String)> = with_usp_pairs(self_h, Vec::new(), |slots| {
        let mut out: Vec<(String, String)> = Vec::with_capacity(slots.len() / 2);
        let mut i = 0;
        while i + 1 < slots.len() {
            let k = key_str_of_handle(slots[i]).unwrap_or_default();
            let v = with_entry(slots[i + 1] as u64, |e| match e {
                Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                _ => String::new(),
            });
            out.push((k, v));
            i += 2;
        }
        out
    });
    let parts: Vec<String> = pairs
        .into_iter()
        .map(|(k, v)| format!("{}={}", url_encode_form(&k), url_encode_form(&v)))
        .collect();
    let s = parts.join("&");
    alloc_entry(Entry::String(s.into_bytes()))
}
