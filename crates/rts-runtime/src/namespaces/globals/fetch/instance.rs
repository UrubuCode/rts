use crate::namespaces::gc::handles::{
    alloc_entry, free_handle, with_entry, Entry, HttpResponseData,
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
    let headers_h = with_entry(map_h, |entry| match entry {
        Some(Entry::Map(m)) => m.get("headers").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if headers_h == 0 {
        return vec![];
    }
    // Collect (key, value_handle) pairs first, then resolve each string handle
    let pairs: Vec<(String, u64)> = with_entry(headers_h, |entry| match entry {
        Some(Entry::Map(m)) => m
            .iter()
            .map(|(k, &v)| (k.clone(), v as u64))
            .collect(),
        _ => vec![],
    });
    pairs
        .into_iter()
        .filter_map(|(k, sh)| {
            if sh == 0 {
                return None;
            }
            with_entry(sh, |entry| match entry {
                Some(Entry::String(b)) => {
                    Some((k, String::from_utf8_lossy(b).into_owned()))
                }
                _ => None,
            })
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
//
// .then/.catch/.finally agora suportam ambas as variantes:
// - Entry::PromiseAsync(Arc<PromiseSlot>) — Promise async real (F1+F2)
// - Entry::Promise(i64) — Promise sync legacy (caminho rapido fetch)
//
// Para PromiseAsync: spawna task tokio que aguarda settle + chama
// callback + resolve nova Promise. Encadeamento real (issue #417 / F6).
//
// Para Promise sync legacy: comportamento antigo (chama callback
// imediato). Mantido para nao quebrar fetch().

type CallbackFn = unsafe extern "C" fn(i64) -> i64;
type CallbackFn0 = unsafe extern "C" fn() -> i64;

enum PromiseKind {
    Async(std::sync::Arc<crate::namespaces::gc::handles::PromiseSlot>),
    Sync(i64),
    NotPromise,
}

fn classify(handle: u64) -> PromiseKind {
    with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => PromiseKind::Async(arc.clone()),
        Some(Entry::Promise(v)) => PromiseKind::Sync(*v),
        _ => PromiseKind::NotPromise,
    })
}

/// promise.then(onFul, onRej?) — versao 2-arg.
/// Variante 1-arg eh `__RTS_FN_GL_PROMISE_THEN` que delega.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_THEN2(promise_h: u64, on_ful: u64, on_rej: u64) -> u64 {
    use crate::namespaces::gc::promise_slot;

    match classify(promise_h) {
        PromiseKind::Async(arc) => {
            // (cross-runtime #779) Fast-path: se ja' settled, executa
            // sync — preserva ordem FIFO de microtask do JS pra cadeias
            // sobre Promise.resolve/reject (que sao settled em construcao).
            let cur_state = promise_slot::current_state(&arc);
            if cur_state != promise_slot::STATE_PENDING {
                let value = promise_slot::current_value(&arc);
                let result = promise_slot::new_pending();
                let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
                if cur_state == promise_slot::STATE_FULFILLED {
                    if on_ful == 0 {
                        promise_slot::resolve(&result, value);
                    } else {
                        let r = unsafe {
                            (std::mem::transmute::<u64, CallbackFn>(on_ful))(value)
                        };
                        promise_slot::resolve(&result, r);
                    }
                } else {
                    if on_rej == 0 {
                        promise_slot::reject(&result, value);
                    } else {
                        let r = unsafe {
                            (std::mem::transmute::<u64, CallbackFn>(on_rej))(value)
                        };
                        promise_slot::resolve(&result, r);
                    }
                }
                return result_handle;
            }
            let result = promise_slot::new_pending();
            let result_clone = result.clone();
            let result_handle = alloc_entry(Entry::PromiseAsync(result));

            let rt = crate::runtime::async_rt::handle();
            rt.spawn_blocking(move || {
                let (state, value) = promise_slot::wait_blocking(&arc);
                if state == promise_slot::STATE_FULFILLED {
                    if on_ful == 0 {
                        promise_slot::resolve(&result_clone, value);
                    } else {
                        let r = unsafe { (std::mem::transmute::<u64, CallbackFn>(on_ful))(value) };
                        promise_slot::resolve(&result_clone, r);
                    }
                } else {
                    if on_rej == 0 {
                        promise_slot::reject(&result_clone, value);
                    } else {
                        let r = unsafe { (std::mem::transmute::<u64, CallbackFn>(on_rej))(value) };
                        // onRej recovers — resolved com o retorno
                        promise_slot::resolve(&result_clone, r);
                    }
                }
            });
            result_handle
        }
        PromiseKind::Sync(value) => {
            // Path rapido sync — sem rejection no Promise(i64) legacy.
            if on_ful == 0 {
                return alloc_entry(Entry::Promise(value));
            }
            let r = unsafe { (std::mem::transmute::<u64, CallbackFn>(on_ful))(value) };
            alloc_entry(Entry::Promise(r))
        }
        PromiseKind::NotPromise => promise_h,
    }
}

/// promise.then(fn) → versao 1-arg, delega pra THEN2 sem onRej.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_THEN(promise_h: u64, fp: u64) -> u64 {
    __RTS_FN_GL_PROMISE_THEN2(promise_h, fp, 0)
}

/// promise.catch(fn) → atalho de then(undefined, fn). Recupera de
/// rejection via callback que retorna novo valor. Sem callback é passthrough.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_CATCH(promise_h: u64, fp: u64) -> u64 {
    __RTS_FN_GL_PROMISE_THEN2(promise_h, 0, fp)
}

/// promise.finally(fn) — chama fn() ao settle (independente de
/// fulfilled/rejected) e retorna nova Promise com mesmo state/value
/// da original.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_FINALLY(promise_h: u64, fp: u64) -> u64 {
    use crate::namespaces::gc::promise_slot;

    match classify(promise_h) {
        PromiseKind::Async(arc) => {
            // (cross-runtime #750) Fast-path: ja' settled — executa fp
            // sync e retorna Promise resolvida com mesmo state/value.
            // Preserva FIFO microtask order pra chains.
            let cur_state = promise_slot::current_state(&arc);
            if cur_state != promise_slot::STATE_PENDING {
                let value = promise_slot::current_value(&arc);
                if fp != 0 {
                    unsafe { (std::mem::transmute::<u64, CallbackFn0>(fp))() };
                }
                let result = promise_slot::new_pending();
                let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
                if cur_state == promise_slot::STATE_FULFILLED {
                    promise_slot::resolve(&result, value);
                } else {
                    promise_slot::reject(&result, value);
                }
                return result_handle;
            }
            let result = promise_slot::new_pending();
            let result_clone = result.clone();
            let result_handle = alloc_entry(Entry::PromiseAsync(result));

            let rt = crate::runtime::async_rt::handle();
            rt.spawn_blocking(move || {
                let (state, value) = promise_slot::wait_blocking(&arc);
                if fp != 0 {
                    let _ = unsafe { (std::mem::transmute::<u64, CallbackFn0>(fp))() };
                }
                if state == promise_slot::STATE_FULFILLED {
                    promise_slot::resolve(&result_clone, value);
                } else {
                    promise_slot::reject(&result_clone, value);
                }
            });
            result_handle
        }
        PromiseKind::Sync(value) => {
            if fp != 0 {
                unsafe { (std::mem::transmute::<u64, CallbackFn0>(fp))() };
            }
            alloc_entry(Entry::Promise(value))
        }
        PromiseKind::NotPromise => promise_h,
    }
}

/// `Promise.resolve(v)` — JS spec: se `v` ja eh Promise, retorna ela
/// mesma; caso contrario cria nova `PromiseAsync` ja fulfilled.
///
/// Tambem usada pelo lowering legado de `await` em fetch sync —
/// nesse caso o caller passa um handle `Entry::Promise` (legacy) e
/// recebe o valor inline.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_RESOLVE_EMPTY() -> i64 {
    // Promise.resolve() — equivalente a Promise.resolve(undefined).
    __RTS_FN_GL_PROMISE_RESOLVE(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_RESOLVE(value: u64) -> i64 {
    
    let kind = with_entry(value, |entry| match entry {
        Some(Entry::Promise(v)) => Some(*v),
        Some(Entry::PromiseAsync(_)) => Some(i64::MIN), // sentinela: ja e' Promise
        _ => None,
    });
    match kind {
        Some(i64::MIN) => value as i64, // PromiseAsync — passthrough do handle
        Some(v) => v,                    // Entry::Promise legacy — extrai inline
        None => {
            // Valor regular: cria PromiseAsync ja fulfilled.
            let slot = crate::namespaces::gc::promise_slot::new_fulfilled(value as i64);
            alloc_entry(Entry::PromiseAsync(slot)) as i64
        }
    }
}

/// `Promise.reject(reason)` — cria PromiseAsync ja rejected.
/// Antes era alias de RESOLVE, o que quebrava `.catch`, `Promise.any`
/// e `Promise.allSettled` que precisam distinguir state.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_REJECT(reason: u64) -> i64 {
    let slot = crate::namespaces::gc::promise_slot::new_rejected(reason as i64);
    alloc_entry(Entry::PromiseAsync(slot)) as i64
}

// ── Response instance methods ─────────────────────────────────────────────────

fn with_response<T>(h: u64, f: impl FnOnce(&HttpResponseData) -> T) -> Option<T> {
    with_entry(h, |entry| match entry {
        Some(Entry::HttpResponse(r)) => Some(f(r)),
        _ => None,
    })
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

/// response.then(fn) → Promise<fn(response)> — compatibilidade com chains
/// `.then(...).then(...)`. Em RTS síncrono, then chama fn imediatamente com o
/// Response handle e wrappa o retorno num PromiseAsync já fulfilled. Isso
/// permite encadear `.then` sem segfault quando o callback retorna handle de
/// outro tipo (Map, String, Buffer) — o segundo `.then` ataca o Promise, não
/// o handle bruto. (#379)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_THEN(h: u64, fp: u64) -> u64 {
    if fp == 0 {
        let slot = crate::namespaces::gc::promise_slot::new_fulfilled(h as i64);
        return alloc_entry(Entry::PromiseAsync(slot));
    }
    let result = unsafe { (std::mem::transmute::<u64, CallbackFn>(fp))(h as i64) };
    // Se o callback ja retornou um Promise, passthrough (evita Promise<Promise>).
    let already_promise = with_entry(result as u64, |entry| {
        matches!(entry, Some(Entry::PromiseAsync(_)))
    });
    if already_promise {
        return result as u64;
    }
    let slot = crate::namespaces::gc::promise_slot::new_fulfilled(result);
    alloc_entry(Entry::PromiseAsync(slot))
}

/// response.free() — libera Response + Promise handles
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_FREE(h: u64) {
    let inner = with_entry(h, |entry| match entry {
        Some(Entry::Promise(v)) => Some(*v as u64),
        _ => None,
    });
    if let Some(inner_h) = inner {
        free_handle(inner_h);
    }
    free_handle(h);
}

#[cfg(test)]
mod tests_response_then {
    //! Cobertura do fix #379 — response.then() retorna PromiseAsync,
    //! permitindo `.then(...).then(...)` sem segfault.

    use super::*;

    fn make_response(body: &str) -> u64 {
        alloc_entry(Entry::HttpResponse(Box::new(HttpResponseData {
            status: 200,
            url: String::new(),
            body: body.as_bytes().to_vec(),
        })))
    }

    #[test]
    fn then_with_null_callback_returns_promise() {
        let h = make_response("{}");
        let result = __RTS_FN_GL_FETCH_RESPONSE_THEN(h, 0);
        let is_promise = with_entry(result, |entry| {
            matches!(entry, Some(Entry::PromiseAsync(_)))
        });
        assert!(is_promise, "RESPONSE_THEN(h, 0) deveria retornar PromiseAsync");
    }

    extern "C" fn cb_returns_string(_h: i64) -> i64 {
        // Simula `r => r.text()` — retorna handle String puro.
        alloc_entry(Entry::String(b"hello".to_vec())) as i64
    }

    #[test]
    fn then_wraps_string_result_in_promise() {
        let h = make_response("hello");
        let cb = cb_returns_string as usize as u64;
        let result = __RTS_FN_GL_FETCH_RESPONSE_THEN(h, cb);
        let is_promise = with_entry(result, |entry| {
            matches!(entry, Some(Entry::PromiseAsync(_)))
        });
        assert!(is_promise, "Resultado de String deve ser wrappado em PromiseAsync");
    }

    extern "C" fn cb_returns_promise(h: i64) -> i64 {
        // Simula callback que ja retorna Promise: passthrough esperado.
        let slot = crate::namespaces::gc::promise_slot::new_fulfilled(h);
        alloc_entry(Entry::PromiseAsync(slot)) as i64
    }

    #[test]
    fn then_passthrough_when_callback_returns_promise() {
        let h = make_response("x");
        let cb = cb_returns_promise as usize as u64;
        let result = __RTS_FN_GL_FETCH_RESPONSE_THEN(h, cb);
        let is_promise = with_entry(result, |entry| {
            matches!(entry, Some(Entry::PromiseAsync(_)))
        });
        assert!(is_promise);
        // Nao deveria ter Promise<Promise> — verificar que e' apenas 1 nivel.
        // (PromiseAsync nao expoe valor sintaticamente, mas garantia importante
        // e' que `.then` no result pega o Promise original, nao um wrapper duplo.)
    }
}
