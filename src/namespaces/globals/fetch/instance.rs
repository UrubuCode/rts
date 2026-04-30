use crate::namespaces::gc::handles::{
    alloc_entry, free_handle, with_entry, with_entry_mut, Entry, HttpResponseData,
};

// ── helpers ──────────────────────────────────────────────────────────────────

fn str_from_parts<'a>(ptr: i64, len: i64) -> &'a str {
    if ptr == 0 || len <= 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

/// Read a string value from a Map handle by key. Returns None if missing.
fn map_get_str(map_h: u64, key: &str) -> Option<Vec<u8>> {
    if map_h == 0 {
        return None;
    }
    // Get the value handle from the map (lock released after closure).
    let val_h: u64 = with_entry(map_h, |entry| match entry {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if val_h == 0 {
        return None;
    }
    // Now resolve the string handle (separate lock acquisition).
    with_entry(val_h, |entry| match entry {
        Some(Entry::String(v)) => Some(v.clone()),
        _ => None,
    })
}

/// Read all headers from a nested Map handle stored under key "headers".
fn map_get_headers(map_h: u64) -> Vec<(String, String)> {
    if map_h == 0 {
        return vec![];
    }
    let headers_h = {
        let guard = shard_for_handle(map_h).lock().unwrap();
        match guard.get(map_h) {
            Some(Entry::Map(m)) => m.get("headers").copied().unwrap_or(0) as u64,
            _ => 0,
        }
    };
    if headers_h == 0 {
        return vec![];
    }
    // Collect (key, value_handle) pairs first, then resolve each string handle
    let pairs: Vec<(String, u64)> = {
        let guard = shard_for_handle(headers_h).lock().unwrap();
        match guard.get(headers_h) {
            Some(Entry::Map(m)) => m
                .iter()
                .map(|(k, &v)| (k.clone(), v as u64))
                .collect(),
            _ => vec![],
        }
    };
    pairs
        .into_iter()
        .filter_map(|(k, sh)| {
            if sh == 0 {
                return None;
            }
            let guard = shard_for_handle(sh).lock().unwrap();
            match guard.get(sh) {
                Some(Entry::String(b)) => {
                    Some((k, String::from_utf8_lossy(b).into_owned()))
                }
                _ => None,
            }
        })
        .collect()
}

fn do_fetch(url: &str, opts_h: u64) -> u64 {
    // Read options from Map handle (opts_h == 0 means no options → GET)
    let method = if opts_h != 0 {
        map_get_str(opts_h, "method")
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_else(|| "GET".into())
    } else {
        "GET".into()
    };

    let body_bytes: Option<Vec<u8>> = if opts_h != 0 {
        map_get_str(opts_h, "body")
    } else {
        None
    };

    let mut req = ureq::request(&method, url);

    // headers from opts.headers map
    if opts_h != 0 {
        for (k, v) in map_get_headers(opts_h) {
            req = req.set(&k, &v);
        }
    }

    let result = match body_bytes {
        Some(ref b) => req.send_bytes(b),
        None => req.call(),
    };

    let (status, final_url, body) = match result {
        Ok(resp) => {
            let status = resp.status();
            let url = resp.get_url().to_owned();
            let body = resp.into_string().unwrap_or_default().into_bytes();
            (status, url, body)
        }
        Err(ureq::Error::Status(status, resp)) => {
            let url = resp.get_url().to_owned();
            let body = resp.into_string().unwrap_or_default().into_bytes();
            (status, url, body)
        }
        Err(e) => {
            // Network error — status 0, body = error message
            (0u16, url.to_owned(), e.to_string().into_bytes())
        }
    };

    // Return Response handle directly — RTS é síncrono, await é no-op.
    // .then(fn) está implementado como instance method em Response.
    alloc_entry(Entry::HttpResponse(Box::new(HttpResponseData {
        status,
        url: final_url,
        body,
    })))
}

// ── fetch() ──────────────────────────────────────────────────────────────────

/// fetch(url: string, opts?: RequestInit) -> Promise<Response>
/// opts is a Map handle (object literal from codegen). 0 = no opts.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH(url_ptr: i64, url_len: i64, opts_h: u64) -> u64 {
    let url = str_from_parts(url_ptr, url_len);
    do_fetch(url, opts_h)
}

// ── Promise ──────────────────────────────────────────────────────────────────

type CallbackFn = unsafe extern "C" fn(i64) -> i64;

/// promise.then(fn) → calls fn(resolved_value), wraps result in new Promise
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_THEN(promise_h: u64, fp: u64) -> u64 {
    let value = {
        let guard = shard_for_handle(promise_h).lock().unwrap();
        match guard.get(promise_h) {
            Some(Entry::Promise(v)) => *v,
            _ => promise_h as i64, // bare value, not wrapped
        }
    };
    if fp == 0 {
        return promise_h;
    }
    let result = unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(value) };
    // Wrap result in a new Promise
    alloc_entry(Entry::Promise(result))
}

/// promise.catch(fn) → passthrough (sync, never rejects unless status 0)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_CATCH(promise_h: u64, _fp: u64) -> u64 {
    promise_h
}

/// promise.finally(fn) → calls fn() then returns original promise
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_FINALLY(promise_h: u64, fp: u64) -> u64 {
    if fp != 0 {
        unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(0) };
    }
    promise_h
}

/// Resolve a Promise to its inner value (i64). Used by `await` lowering.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_RESOLVE(promise_h: u64) -> i64 {
    let guard = shard_for_handle(promise_h).lock().unwrap();
    match guard.get(promise_h) {
        Some(Entry::Promise(v)) => *v,
        _ => promise_h as i64,
    }
}

// ── Response instance methods ─────────────────────────────────────────────────

fn with_response<T>(h: u64, f: impl FnOnce(&HttpResponseData) -> T) -> Option<T> {
    let guard = shard_for_handle(h).lock().unwrap();
    match guard.get(h) {
        Some(Entry::HttpResponse(r)) => Some(f(r)),
        _ => None,
    }
}

/// response.status → number
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_STATUS(h: u64) -> i64 {
    with_response(h, |r| r.status as i64).unwrap_or(0)
}

/// response.ok → boolean (status 200-299)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_OK(h: u64) -> i8 {
    let status = with_response(h, |r| r.status).unwrap_or(0);
    if (200..300).contains(&status) { 1 } else { 0 }
}

/// response.statusText → string handle (e.g. "OK", "Not Found")
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_STATUS_TEXT(h: u64) -> u64 {
    let status = with_response(h, |r| r.status).unwrap_or(0);
    let text = match status {
        200 => "OK", 201 => "Created", 204 => "No Content",
        301 => "Moved Permanently", 302 => "Found", 304 => "Not Modified",
        400 => "Bad Request", 401 => "Unauthorized", 403 => "Forbidden",
        404 => "Not Found", 405 => "Method Not Allowed",
        429 => "Too Many Requests",
        500 => "Internal Server Error", 502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "",
    };
    alloc_entry(Entry::String(text.as_bytes().to_vec()))
}

/// response.url → string handle
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_URL(h: u64) -> u64 {
    let url = with_response(h, |r| r.url.as_bytes().to_vec())
        .unwrap_or_default();
    alloc_entry(Entry::String(url))
}

/// response.text() → string handle (RTS sync — sem Promise wrapper)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_TEXT(h: u64) -> u64 {
    let body = with_response(h, |r| r.body.clone()).unwrap_or_default();
    alloc_entry(Entry::String(body))
}

/// response.json() → JSON handle (RTS sync — sem Promise wrapper)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_JSON(h: u64) -> u64 {
    let body = with_response(h, |r| r.body.clone()).unwrap_or_default();
    let json_val = serde_json::from_slice::<serde_json::Value>(&body)
        .unwrap_or(serde_json::Value::Null);
    alloc_entry(Entry::Json(Box::new(json_val)))
}

/// response.arrayBuffer() / response.blob() → Buffer handle (RTS sync)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_ARRAY_BUFFER(h: u64) -> u64 {
    let body = with_response(h, |r| r.body.clone()).unwrap_or_default();
    alloc_entry(Entry::Buffer(body))
}

/// response.then(fn) → fn(response) — compatibilidade com .then() chains.
/// Em RTS síncrono, then chama fn imediatamente com o Response handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_THEN(h: u64, fp: u64) -> u64 {
    if fp == 0 {
        return h;
    }
    let result = unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(h as i64) };
    result as u64
}

/// response.free() — libera Response + Promise handles
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_FREE(h: u64) {
    let inner = {
        let guard = shard_for_handle(h).lock().unwrap();
        match guard.get(h) {
            Some(Entry::Promise(v)) => Some(*v as u64),
            _ => None,
        }
    };
    if let Some(inner_h) = inner {
        free_handle(inner_h);
    }
    free_handle(h);
}
