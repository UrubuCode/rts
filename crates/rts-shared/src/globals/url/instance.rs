use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry};

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

/// (#67) Percent-encode chars unsafe em URL search/hash component.
/// Preserva chars ASCII safe (alfanum + - _ . ~ ! $ & ' ( ) * + , ; = : @ / ?
/// e # no inicio se ja' for prefixo). Encoda espaco, < > " { } | \ ^ ` etc.
fn percent_encode_search(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let safe = matches!(b,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~'
            | b'!' | b'$' | b'&' | b'\'' | b'(' | b')'
            | b'*' | b'+' | b',' | b';' | b'='
            | b':' | b'@' | b'/' | b'?' | b'#'
            | b'%'
        );
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

impl ParsedUrl {
    fn parse(raw: &str) -> Option<Self> {
        // Minimal URL parser without external deps.
        let (raw, hash_raw) = match raw.split_once('#') {
            Some((before, after)) => (before, format!("#{after}")),
            None => (raw, String::new()),
        };
        let (raw, search_raw) = match raw.split_once('?') {
            Some((before, after)) => (before, format!("?{after}")),
            None => (raw, String::new()),
        };
        // (#67) Percent-encode chars unsafe em search/hash. JS spec
        // codifica espaco como %20 (nao + em URL.search; URLSearchParams
        // usa + mas URL.search nao).
        let search = percent_encode_search(&search_raw);
        let hash = percent_encode_search(&hash_raw);
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
    if ParsedUrl::parse(raw).is_some() {
        1
    } else {
        0
    }
}

/// URL.canParse(href, base) — true se href resolve relativo a base.
/// Implementacao minima: tenta parse(href); se falhar, tenta concatenar
/// com base + "/" + href; retorna true se algum parsa.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_CAN_PARSE_BASE(
    href_ptr: i64,
    href_len: i64,
    base_ptr: i64,
    base_len: i64,
) -> i64 {
    let href = str_from_parts(href_ptr, href_len);
    let base = str_from_parts(base_ptr, base_len);
    // Tenta parse direto primeiro (href ja absoluto).
    if ParsedUrl::parse(href).is_some() {
        return 1;
    }
    // base precisa parsar.
    let Some(base_url) = ParsedUrl::parse(base) else {
        return 0;
    };
    // Resolve href relativo a base. Para "/x" + "https://example.com",
    // retorna "https://example.com/x". Para path complexo, usa join naive.
    let combined = if href.starts_with('/') {
        format!(
            "{}://{}{}",
            base_url.protocol.trim_end_matches(':'),
            base_url.host(),
            href
        )
    } else {
        format!(
            "{}://{}/{}",
            base_url.protocol.trim_end_matches(':'),
            base_url.host(),
            href
        )
    };
    if ParsedUrl::parse(&combined).is_some() {
        1
    } else {
        0
    }
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

/// The CURRENT `search` string (`"?a=1&b=2"` or `""`) of a URL, reconstructed
/// from the cached `URLSearchParams` when it has been mutated (`url.searchParams.
/// append/set/...`), else the parsed `search` slot. Shared by `url.search` and
/// `url.href`/`toString` so all three reflect searchParams mutations.
fn current_search(handle: u64) -> String {
    let cached_sp = url_sp_cache()
        .lock()
        .ok()
        .and_then(|c| c.get(&handle).copied());
    if let Some(sp) = cached_sp {
        let pairs: Vec<(String, String)> = rts_engine::heap::handles::with_rtse::<
            super::search_params::UrlSearchParams,
            _,
        >(sp, |s| s.map(|s| s.pairs().to_vec()).unwrap_or_default());
        if pairs.is_empty() {
            return String::new();
        }
        let mut s = String::from("?");
        for (i, (k, v)) in pairs.iter().enumerate() {
            if i > 0 {
                s.push('&');
            }
            s.push_str(&percent_encode_form(k));
            s.push('=');
            s.push_str(&percent_encode_form(v));
        }
        return s;
    }
    url_field_str(handle, 6)
}

/// The CURRENT full href, reconstructed from the component slots + the live
/// searchParams (mirrors `toString`). Shared by `url.href` and `toString`.
fn current_href(handle: u64) -> String {
    let protocol = url_field_str(handle, 1);
    let hostname = url_field_str(handle, 3);
    let port = url_field_str(handle, 4);
    let pathname = url_field_str(handle, 5);
    let username = url_field_str(handle, 9);
    let password = url_field_str(handle, 10);
    let search = current_search(handle);
    let hash = url_field_str(handle, 7);
    let host = if port.is_empty() {
        hostname
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
    format!("{protocol}//{userinfo_str}{host}{pathname}{search}{hash}")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HREF(h: u64) -> u64 {
    // Reconstruct so a searchParams/setter mutation is reflected (Node semantics).
    intern_str(&current_href(h))
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PROTOCOL(h: u64) -> u64 {
    url_field(h, 1)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HOST(h: u64) -> u64 {
    url_field(h, 2)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HOSTNAME(h: u64) -> u64 {
    url_field(h, 3)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PORT(h: u64) -> u64 {
    url_field(h, 4)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PATHNAME(h: u64) -> u64 {
    url_field(h, 5)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SEARCH(h: u64) -> u64 {
    // Reflect searchParams mutations (Node semantics), not just the parsed slot.
    intern_str(&current_search(h))
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_HASH(h: u64) -> u64 {
    url_field(h, 7)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_ORIGIN(h: u64) -> u64 {
    url_field(h, 8)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_USERNAME(h: u64) -> u64 {
    url_field(h, 9)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_PASSWORD(h: u64) -> u64 {
    url_field(h, 10)
}

/// (#373) `url.searchParams` — constroi URLSearchParams a partir do
/// `search` da URL. (cross-runtime #55) Cacheado por URL handle para que
/// `url.searchParams.set(...)` em uma chamada seja visivel na proxima.
static URL_SP_CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, u64>>> =
    std::sync::OnceLock::new();

fn url_sp_cache() -> &'static std::sync::Mutex<std::collections::HashMap<u64, u64>> {
    URL_SP_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SEARCH_PARAMS(handle: u64) -> u64 {
    use rts_engine::heap::handles::{alloc_rtse, with_rtse};
    use super::search_params::UrlSearchParams;
    if let Ok(cache) = url_sp_cache().lock() {
        if let Some(&cached) = cache.get(&handle) {
            // Valida que o handle cacheado ainda eh um UrlSearchParams vivo.
            let still_valid = with_rtse::<UrlSearchParams, _>(cached, |s| s.is_some());
            if still_valid {
                return cached;
            }
        }
    }
    let search_h = url_field(handle, 6);
    let search: String = with_entry(search_h, |e| match e {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    });
    // O search inclui o '?' inicial; parse_query_pairs strip-prefix
    let sp = alloc_rtse("URLSearchParams", UrlSearchParams::from_pairs(super::search_params::parse_query_pairs(
        &search,
    )));
    if let Ok(mut cache) = url_sp_cache().lock() {
        cache.insert(handle, sp);
    }
    sp
}

/// (#67) Helper: le campo string de uma URL como String owned.
fn url_field_str(handle: u64, idx: usize) -> String {
    let h = url_field(handle, idx);
    with_entry(h, |e| match e {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    })
}

/// (#67) Atualiza um slot de URL com nova string. Libera o handle antigo.
fn url_set_field(handle: u64, idx: usize, new_str: &str) {
    use rts_engine::heap::handles::with_entry_mut;
    let new_h = intern_str(new_str) as i64;
    with_entry_mut(handle, |entry| {
        if let Some(Entry::Env(v)) = entry {
            if v.len() > idx {
                let old = v[idx] as u64;
                v[idx] = new_h;
                if old != 0 {
                    free_handle(old);
                }
            }
        }
    });
}

/// (#67) Encode pathname segment (RFC 3986 path char set). Espaco -> %20,
/// nao toca em / e chars unreserved.
fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let safe = matches!(b,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~'
            | b'!' | b'$' | b'&' | b'\'' | b'(' | b')'
            | b'*' | b'+' | b',' | b';' | b'='
            | b':' | b'@' | b'/'
            | b'%'
        );
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// (#67) `url.pathname = "..."` setter. Percent-encode automatico
/// para chars unsafe (ex: espaco -> %20).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_PATHNAME(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len);
    let normalized = if raw.starts_with('/') {
        raw.to_string()
    } else {
        format!("/{raw}")
    };
    let encoded = percent_encode_path(&normalized);
    url_set_field(handle, 5, &encoded);
}

/// `url.protocol = value` — normaliza para terminar em ':' (WHATWG). Ignora
/// se vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_PROTOCOL(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len).trim_end_matches(':');
    if raw.is_empty() {
        return;
    }
    url_set_field(handle, 1, &format!("{raw}:"));
}

/// `url.hostname = value`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_HOSTNAME(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len);
    url_set_field(handle, 3, raw);
    // slot 2 (host) = hostname[:port]
    let port = url_field_str(handle, 4);
    let host = if port.is_empty() {
        raw.to_string()
    } else {
        format!("{raw}:{port}")
    };
    url_set_field(handle, 2, &host);
}

/// `url.port = value` — apenas dígitos; string vazia limpa a porta.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_PORT(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len);
    if !raw.is_empty() && !raw.chars().all(|c| c.is_ascii_digit()) {
        return; // porta inválida → ignora (WHATWG)
    }
    url_set_field(handle, 4, raw);
    let hostname = url_field_str(handle, 3);
    let host = if raw.is_empty() {
        hostname
    } else {
        format!("{hostname}:{raw}")
    };
    url_set_field(handle, 2, &host);
}

/// `url.hash = value` — normaliza leading '#'. String vazia limpa o hash.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_HASH(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len);
    let normalized = if raw.is_empty() {
        String::new()
    } else if raw.starts_with('#') {
        percent_encode_search(raw)
    } else {
        percent_encode_search(&format!("#{raw}"))
    };
    url_set_field(handle, 7, &normalized);
}

/// `url.search = value` — normaliza leading '?' + invalida o cache de
/// searchParams (a próxima leitura de `url.searchParams` re-parseia).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_SEARCH(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len);
    let normalized = if raw.is_empty() {
        String::new()
    } else if raw.starts_with('?') {
        percent_encode_search(raw)
    } else {
        percent_encode_search(&format!("?{raw}"))
    };
    url_set_field(handle, 6, &normalized);
    if let Ok(mut cache) = url_sp_cache().lock() {
        cache.remove(&handle);
    }
}

/// `url.username = value`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_USERNAME(handle: u64, ptr: i64, len: i64) {
    url_set_field(handle, 9, str_from_parts(ptr, len));
}

/// `url.password = value`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_PASSWORD(handle: u64, ptr: i64, len: i64) {
    url_set_field(handle, 10, str_from_parts(ptr, len));
}

/// `url.host = value` — split em `hostname[:port]`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_HOST(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len);
    let (hostname, port) = match raw.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) => (h, p),
        _ => (raw, ""),
    };
    url_set_field(handle, 3, hostname);
    url_set_field(handle, 4, port);
    url_set_field(handle, 2, raw);
}

/// `url.href = value` — re-parseia a URL inteira, reescrevendo todos os slots.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_SET_HREF(handle: u64, ptr: i64, len: i64) {
    let raw = str_from_parts(ptr, len);
    let Some(u) = ParsedUrl::parse(raw) else {
        return; // href inválido → ignora (Node lança TypeError; aqui no-op honesto)
    };
    url_set_field(handle, 0, &u.href);
    url_set_field(handle, 1, &u.protocol);
    url_set_field(handle, 2, &u.host());
    url_set_field(handle, 3, &u.hostname);
    url_set_field(handle, 4, &u.port);
    url_set_field(handle, 5, &u.pathname);
    url_set_field(handle, 6, &u.search);
    url_set_field(handle, 7, &u.hash);
    url_set_field(handle, 8, &u.origin());
    url_set_field(handle, 9, &u.username);
    url_set_field(handle, 10, &u.password);
    if let Ok(mut cache) = url_sp_cache().lock() {
        cache.remove(&handle);
    }
}

/// (#67) Reconstroi `url.href` atualizado, considerando setters E
/// searchParams cacheado (cujo conteudo pode ter mudado via .set/.append).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URL_TO_STRING(handle: u64) -> u64 {
    let href = current_href(handle);
    // Keep the cached href slot in sync (subsequent reads match toString).
    url_set_field(handle, 0, &href);
    intern_str(&href)
}

/// (#67) URLSearchParams form encoding: espaco -> '+', chars unsafe -> %XX.
/// Diferente do path encoding (que usa %20 para espaco). `pub(super)` — shared
/// with `search_params.rs`'s `UrlSearchParams::to_string`.
pub(super) fn percent_encode_form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
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

