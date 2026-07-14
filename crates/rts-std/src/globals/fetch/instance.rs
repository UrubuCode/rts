use rts_engine::heap::handles::{
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
        Some(Entry::Map(m)) => m.iter().map(|(k, &v)| (k.clone(), v as u64)).collect(),
        _ => vec![],
    });
    pairs
        .into_iter()
        .filter_map(|(k, sh)| {
            if sh == 0 {
                return None;
            }
            with_entry(sh, |entry| match entry {
                Some(Entry::String(b)) => Some((k, String::from_utf8_lossy(b).into_owned())),
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

    // Browser-style default UA (`Rts v<version>` / `Rts development`); an
    // explicit `headers: { "User-Agent": … }` below overrides it.
    req = req.set("User-Agent", super::default_user_agent());

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

/// `fetchText(url)` → baixa a URL (GET) e devolve o CORPO como STRING (handle do
/// pool GC), numa só chamada síncrona. Conveniência para o mini-browser: junta o
/// `fetch(url)` + `.text()` sem exigir o `fetch()` global no front do engine nem a
/// máquina de Promise. Erro de rede → string vazia (o browser mostra "falhou").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FETCH_FETCH_TEXT(url_ptr: i64, url_len: i64) -> u64 {
    let url = str_from_parts(url_ptr, url_len);
    let resp_h = do_fetch(url, 0);
    let body = with_response(resp_h, |r| r.body.clone()).unwrap_or_default();
    alloc_entry(Entry::String(body))
}

// ── fetch ASSÍNCRONO (não bloqueia a UI) ─────────────────────────────────────
// O `fetchText` síncrono trava a thread de UI ~1-2s durante o download (TLS +
// bytes). Esta variante dispara o GET numa thread separada e devolve um TICKET
// (u64) na hora; o loop de UI continua pintando e faz POLL a cada frame. Quando
// pronto, `fetchTake` entrega o corpo. Estado num mapa global thread-safe.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// `ticket → Some(corpo)` quando pronto; `None` enquanto baixa. Removido no take.
fn async_fetches() -> &'static Mutex<HashMap<u64, Option<String>>> {
    static F: OnceLock<Mutex<HashMap<u64, Option<String>>>> = OnceLock::new();
    F.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Contador de tickets (id monotônico). Simples e suficiente (1 fetch por vez no
/// browser; nunca reusa id vivo).
fn next_ticket() -> u64 {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// `fetchTextAsync(url)` → dispara o GET numa thread e devolve um TICKET (u64)
/// imediatamente (NÃO bloqueia). O corpo fica disponível via `fetchPoll`/`fetchTake`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FETCH_FETCH_TEXT_ASYNC(url_ptr: i64, url_len: i64) -> u64 {
    let url = str_from_parts(url_ptr, url_len).to_string();
    let ticket = next_ticket();
    async_fetches().lock().unwrap().insert(ticket, None); // pendente
    std::thread::spawn(move || {
        // download síncrono NA THREAD (a UI segue livre).
        let resp_h = do_fetch(&url, 0);
        let body = with_response(resp_h, |r| r.body.clone()).unwrap_or_default();
        free_handle(resp_h); // a Response já foi consumida.
        let s = String::from_utf8_lossy(&body).into_owned();
        async_fetches().lock().unwrap().insert(ticket, Some(s));
    });
    ticket
}

/// `fetchPoll(ticket)` → `1` se o download terminou (corpo pronto p/ `fetchTake`),
/// `0` se ainda baixando, `-1` se o ticket é inválido. Não bloqueia.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FETCH_FETCH_POLL(ticket: u64) -> i64 {
    match async_fetches().lock().unwrap().get(&ticket) {
        Some(Some(_)) => 1,
        Some(None) => 0,
        None => -1,
    }
}

/// `fetchTake(ticket)` → o corpo baixado como STRING (handle do pool GC) e REMOVE o
/// ticket. `""` se não está pronto / inválido. Chamar após `fetchPoll` == 1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FETCH_FETCH_TAKE(ticket: u64) -> u64 {
    let taken = async_fetches().lock().unwrap().remove(&ticket).flatten();
    alloc_entry(Entry::String(taken.unwrap_or_default().into_bytes()))
}

// ── Promise ──────────────────────────────────────────────────────────────────
//
// .then/.catch/.finally operam sobre Entry::PromiseAsync(Arc<PromiseSlot>) —
// a única representação de Promise (a sync legacy Entry::Promise(i64) foi
// removida). Encadeamento real via microtask queue (issue #417 / F6).

type CallbackFn = unsafe extern "C" fn(i64) -> i64;

enum PromiseKind {
    Async(std::sync::Arc<rts_engine::heap::handles::PromiseSlot>),
    NotPromise,
}

fn classify(handle: u64) -> PromiseKind {
    with_entry(handle, |entry| match entry {
        Some(Entry::PromiseAsync(arc)) => PromiseKind::Async(arc.clone()),
        _ => PromiseKind::NotPromise,
    })
}

/// promise.then(onFul, onRej?) — versao 2-arg.
/// Variante 1-arg eh `__RTS_FN_GL_PROMISE_THEN` que delega.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_THEN2(promise_h: u64, on_ful: u64, on_rej: u64) -> u64 {
    use crate::promise_slot;

    match classify(promise_h) {
        PromiseKind::Async(arc) => {
            // (unhandled rejection) .then/.catch da CLASSE Promise passam por aqui
            // (THEN2). Anexar handler = consome/encaminha a rejection do source.
            promise_slot::mark_handled(&arc);
            // (cross-runtime #56/#285) Fast-path: ja' settled — enfileira
            // como microtask em vez de executar sync. Sem isso, o callback
            // rodava antes do top-level continuar, violando ordem JS spec.
            let cur_state = promise_slot::current_state(&arc);
            if cur_state != promise_slot::STATE_PENDING {
                let value = promise_slot::current_value(&arc);
                let result = promise_slot::new_pending();
                let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
                let fulfilled = cur_state == promise_slot::STATE_FULFILLED;
                // Escolhe callback aplicavel: on_ful para fulfilled,
                // on_rej para rejected. Se nao tem callback aplicavel,
                // propaga sem invocar (passthrough).
                let fp = if fulfilled { on_ful } else { on_rej };
                if fp == 0 {
                    // Sem callback aplicavel: settle imediato (passthrough).
                    if fulfilled {
                        promise_slot::resolve(&result, value);
                    } else {
                        promise_slot::reject(&result, value);
                    }
                } else {
                    // Enfileira no microtask queue. Mesmo o caminho on_rej
                    // (catch) recupera: result e' resolved com retorno do cb.
                    crate::globals::text_encoding::instance::enqueue_microtask_settled(
                        fp,
                        Vec::new(),
                        value,
                        true, // sempre invoca callback como "resolved" para
                        // que o retorno vire resolve(result). Reject
                        // path do on_rej tambem segue spec: callback
                        // de catch recupera, resultado eh resolved.
                        result,
                    );
                }
                return result_handle;
            }
            // (cross-runtime #393/#116) Source PENDING: enfileira como
            // PendingThen2 na microtask queue (polling determinista no drain)
            // em vez de spawn_blocking (thread nao-deterministica). Preserva
            // ordem FIFO entre chains `.then().then()` no mesmo task sync.
            let result = promise_slot::new_pending();
            let result_clone = result.clone();
            let result_handle = alloc_entry(Entry::PromiseAsync(result));
            crate::globals::text_encoding::instance::enqueue_microtask_pending_then2(
                arc,
                on_ful,
                on_rej,
                result_clone,
            );
            result_handle
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
    use crate::promise_slot;

    match classify(promise_h) {
        PromiseKind::Async(arc) => {
            // (unhandled rejection) .finally anexa handler — encaminha o
            // settlement; marca o source como handled.
            promise_slot::mark_handled(&arc);
            // Fast-path: ja' settled — enfileira como microtask (consistente
            // com THEN2). Sem isso, callback de finally rodava sync e
            // quebrava ordem JS spec em chains paralelas
            // (.then().finally().then() vs .catch().finally()).
            let cur_state = promise_slot::current_state(&arc);
            if cur_state != promise_slot::STATE_PENDING {
                let value = promise_slot::current_value(&arc);
                let result = promise_slot::new_pending();
                let result_handle = alloc_entry(Entry::PromiseAsync(result.clone()));
                if fp == 0 {
                    // Sem callback: settle imediato (passthrough state).
                    if cur_state == promise_slot::STATE_FULFILLED {
                        promise_slot::resolve(&result, value);
                    } else {
                        promise_slot::reject(&result, value);
                    }
                } else {
                    crate::globals::text_encoding::instance::enqueue_microtask_finally(
                        fp,
                        value,
                        cur_state == promise_slot::STATE_FULFILLED,
                        result,
                    );
                }
                return result_handle;
            }
            // (cross-runtime #116) Source PENDING: enfileira como
            // PendingFinally determinista (polling no drain) em vez de
            // spawn_blocking. Preserva ordem FIFO de microtasks entre chains.
            let result = promise_slot::new_pending();
            let result_clone = result.clone();
            let result_handle = alloc_entry(Entry::PromiseAsync(result));
            crate::globals::text_encoding::instance::enqueue_microtask_pending_finally(
                arc,
                fp,
                result_clone,
            );
            result_handle
        }
        PromiseKind::NotPromise => promise_h,
    }
}

/// `Promise.resolve(v)` — JS spec: se `v` ja eh Promise, retorna ela
/// mesma; caso contrario cria nova `PromiseAsync` ja fulfilled.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_RESOLVE_EMPTY() -> i64 {
    // Promise.resolve() — equivalente a Promise.resolve(undefined).
    __RTS_FN_GL_PROMISE_RESOLVE(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_RESOLVE(value: u64) -> i64 {
    let already_promise = with_entry(value, |entry| {
        matches!(entry, Some(Entry::PromiseAsync(_)))
    });
    match already_promise {
        true => value as i64, // PromiseAsync — passthrough do handle
        false => {
            // Promise/A+: um THENABLE e' ADOTADO (`Promise.resolve({then})`
            // segue o estado do thenable); valor plano cria fulfilled direto.
            if crate::promise_slot::thenable_then_of(value as i64) != 0 {
                let slot = crate::promise_slot::new_pending();
                let h = alloc_entry(Entry::PromiseAsync(slot.clone()));
                crate::promise::resolve_assimilating(&slot, h, value as i64);
                return h as i64;
            }
            // Valor regular: cria PromiseAsync ja fulfilled.
            let slot = crate::promise_slot::new_fulfilled(value as i64);
            alloc_entry(Entry::PromiseAsync(slot)) as i64
        }
    }
}

/// `Promise.reject(reason)` — cria PromiseAsync ja rejected.
/// Antes era alias de RESOLVE, o que quebrava `.catch`, `Promise.any`
/// e `Promise.allSettled` que precisam distinguir state.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_REJECT(reason: u64) -> i64 {
    let slot = crate::promise_slot::new_rejected(reason as i64);
    alloc_entry(Entry::PromiseAsync(slot)) as i64
}

/// (cross-runtime #304) `Promise.try(fn)` — invoca fn() sincronamente e
/// retorna Promise.resolve(retval) (ou Promise.reject(err) se fn lancar).
/// Spec ES2025: equivalente a `Promise.resolve().then(fn)` mas com fn
/// executada sync em vez de microtask.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_TRY(fp: u64) -> i64 {
    use crate::gc_surface as error;
    if fp == 0 {
        let slot = crate::promise_slot::new_fulfilled(0);
        return alloc_entry(Entry::PromiseAsync(slot)) as i64;
    }
    // Limpa qualquer erro pendente antes de invocar (semantica: try
    // captura apenas erros lancados pela propria fn).
    let _ = error::__RTS_FN_RT_ERROR_GET();
    error::__RTS_FN_RT_ERROR_CLEAR();
    // Resolve fp: pode ser handle de Entry::Function (arrow/reify) ou
    // ponteiro extern "C" direto.
    let (fn_ptr, bound) = with_entry(fp, |entry| {
        if let Some(Entry::Function(fd)) = entry {
            Some((fd.fn_ptr, fd.bound_args.clone()))
        } else {
            None
        }
    })
    .unwrap_or((fp, Vec::new()));
    let retval = unsafe {
        use std::mem::transmute;
        match bound.len() {
            0 => transmute::<u64, extern "C" fn() -> i64>(fn_ptr)(),
            1 => transmute::<u64, extern "C" fn(i64) -> i64>(fn_ptr)(bound[0]),
            2 => transmute::<u64, extern "C" fn(i64, i64) -> i64>(fn_ptr)(bound[0], bound[1]),
            _ => 0,
        }
    };
    let err_h = error::__RTS_FN_RT_ERROR_GET();
    if err_h != 0 {
        error::__RTS_FN_RT_ERROR_CLEAR();
        let slot = crate::promise_slot::new_rejected(err_h as i64);
        return alloc_entry(Entry::PromiseAsync(slot)) as i64;
    }
    let slot = crate::promise_slot::new_fulfilled(retval);
    alloc_entry(Entry::PromiseAsync(slot)) as i64
}

/// (cross-runtime #84) `new Promise(executor)` — JS spec ctor.
///
/// 1. Cria slot pending.
/// 2. Cria resolve/reject como Function handles (bound a promise_h).
/// 3. Invoca executor(resolve, reject) sincronamente via INVOKE_AUTO.
///    - INVOKE_AUTO detecta Function handle (executor) e despacha
///      corretamente via FUNCTION_CALL.
/// 4. Se executor lancar (slot de erro thread-local nao-zero), rejeita
///    a promise com a excecao (JS spec).
///
/// O executor pode chamar resolve/reject sincronamente OU agendar via
/// setTimeout/spawn_blocking. Em ambos os casos, settle eh idempotente
/// e wait_blocking acorda waiters via oneshot::Sender.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_NEW(executor_h: u64) -> u64 {
    use crate::{gc_surface as error, promise_slot};
    let slot = promise_slot::new_pending();
    let promise_h = alloc_entry(Entry::PromiseAsync(slot.clone()));
    if executor_h == 0 {
        return promise_h;
    }
    let resolve_h = make_resolver_fn(promise_h, true);
    let reject_h = make_resolver_fn(promise_h, false);
    // Empacota args: [resolve_handle, reject_handle].
    let args_vec = alloc_entry(Entry::Vec(Box::new(vec![
        resolve_h as i64,
        reject_h as i64,
    ])));
    // Limpa error slot da thread atual antes de invocar (sem isso, erro
    // antigo de outra fn confundiria a propagacao).
    error::__RTS_FN_RT_ERROR_CLEAR();
    // INVOKE_AUTO detecta executor como Function handle e despacha.
    unsafe extern "C" {
        fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
    }
    let _ = unsafe { __RTS_FN_RT_INVOKE_AUTO(executor_h as i64, 0, args_vec) };
    // Se executor lancou, propaga como reject (JS spec).
    let err = error::__RTS_FN_RT_ERROR_GET();
    if err != 0 {
        error::__RTS_FN_RT_ERROR_CLEAR();
        promise_slot::reject(&slot, err as i64);
    }
    promise_h
}

/// (cross-runtime #92) `Promise.withResolvers()` — ES2024.
/// Retorna Map { promise, resolve, reject }. `resolve(v)` e `reject(e)`
/// sao Function handles que settle a promise quando chamados.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PROMISE_WITH_RESOLVERS() -> u64 {
    use rts_engine::heap::shapes::{alloc_shaped_object, handle_word_auto};
    let slot = crate::promise_slot::new_pending();
    let promise_h = alloc_entry(Entry::PromiseAsync(slot.clone()));
    // Resolve/reject sao Function handles (thunk uniforme, promise no env).
    let resolve_h = make_resolver_fn(promise_h, true);
    let reject_h = make_resolver_fn(promise_h, false);
    // Objeto SHAPED do motor novo (slots = words), nao dicionario Entry::Map.
    alloc_shaped_object(
        &["promise", "resolve", "reject"],
        &[
            handle_word_auto(promise_h) as i64,
            handle_word_auto(resolve_h) as i64,
            handle_word_auto(reject_h) as i64,
        ],
    )
}

/// Helper: cria Function handle (thunk uniforme) que ao ser invocado chama
/// resolve/reject no slot cujo handle vive no env (bound_this).
fn make_resolver_fn(promise_h: u64, is_resolve: bool) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, Entry, FunctionData};
    let fn_ptr = if is_resolve {
        resolver_resolve_thunk as *const () as usize as u64
    } else {
        resolver_reject_thunk as *const () as usize as u64
    };
    alloc_entry(Entry::Function(Box::new(FunctionData {
        fn_ptr,
        arity: 1,
        name: (if is_resolve { "resolve" } else { "reject" })
            .to_string()
            .into_boxed_str(),
        bound_this: promise_h as i64,
        has_bound_this: true,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: true,
    })))
}

/// Thunk uniforme resolve — env = promise handle, a0 = valor (word ou i64 raw,
/// normalizado downstream por rebox_settled).
extern "C" fn resolver_resolve_thunk(
    env: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64, _rest: u64,
) -> u64 {
    use crate::promise_slot; use rts_engine::heap::handles::{with_entry, Entry};
    let slot = with_entry(env, |e| match e {
        Some(Entry::PromiseAsync(s)) => Some(s.clone()),
        _ => None,
    });
    if let Some(s) = slot {
        promise_slot::resolve(&s, a0 as i64);
    }
    rts_engine::heap::poly::POLY_UNDEFINED
}

extern "C" fn resolver_reject_thunk(
    env: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64, _rest: u64,
) -> u64 {
    use crate::promise_slot; use rts_engine::heap::handles::{with_entry, Entry};
    let slot = with_entry(env, |e| match e {
        Some(Entry::PromiseAsync(s)) => Some(s.clone()),
        _ => None,
    });
    if let Some(s) = slot {
        promise_slot::reject(&s, a0 as i64);
    }
    rts_engine::heap::poly::POLY_UNDEFINED
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
    if (200..300).contains(&status) {
        1
    } else {
        0
    }
}

/// response.statusText → string handle (e.g. "OK", "Not Found")
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_STATUS_TEXT(h: u64) -> u64 {
    let status = with_response(h, |r| r.status).unwrap_or(0);
    let text = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "",
    };
    alloc_entry(Entry::String(text.as_bytes().to_vec()))
}

/// response.url → string handle
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_URL(h: u64) -> u64 {
    let url = with_response(h, |r| r.url.as_bytes().to_vec()).unwrap_or_default();
    alloc_entry(Entry::String(url))
}

/// response.text() → Promise<string>. Dual-mode: HttpResponse (fetch) OU
/// Map (new Response(body, init)). Promise resolvido p/ `await res.text()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_TEXT(h: u64) -> u64 {
    // (cross-runtime #85) Map-based Response guarda "body" como string handle.
    let map_body = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => Some(m.get("body").copied().unwrap_or(0) as u64),
        _ => None,
    });
    let str_h = if let Some(bh) = map_body {
        bh
    } else {
        let body = with_response(h, |r| r.body.clone()).unwrap_or_default();
        alloc_entry(Entry::String(body))
    };
    let slot = crate::promise_slot::new_fulfilled(str_h as i64);
    alloc_entry(Entry::PromiseAsync(slot))
}

/// response.json() → JSON handle (RTS sync — sem Promise wrapper)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_JSON(h: u64) -> u64 {
    let body = with_response(h, |r| r.body.clone()).unwrap_or_default();
    let json_val =
        serde_json::from_slice::<serde_json::Value>(&body).unwrap_or(serde_json::Value::Null);
    alloc_entry(Entry::Json(Box::new(json_val)))
}

/// response.arrayBuffer() / response.blob() → Buffer handle (RTS sync)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_ARRAY_BUFFER(h: u64) -> u64 {
    let body = with_response(h, |r| r.body.clone()).unwrap_or_default();
    alloc_entry(Entry::Buffer(body))
}

// ── new Response / new Request (#85) — instancia Map-based ─────────────────────

/// Constroi um Entry::Headers a partir de um objeto literal {name: value}.
fn headers_from_obj(map_h: u64) -> u64 {
    use indexmap::IndexMap;
    let mut hm: IndexMap<String, Vec<String>> = IndexMap::new();
    if map_h != 0 {
        with_entry(map_h, |e| {
            if let Some(Entry::Map(m)) = e {
                for (k, &vh) in m.iter() {
                    if k == "__rts_class" {
                        continue;
                    }
                    let v = with_entry(vh as u64, |ve| match ve {
                        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                        _ => String::new(),
                    });
                    hm.entry(k.to_ascii_lowercase()).or_default().push(v);
                }
            }
        });
    }
    alloc_entry(Entry::Headers(Box::new(hm)))
}

fn opt_str_field(map_h: u64, key: &str) -> Option<u64> {
    if map_h == 0 {
        return None;
    }
    with_entry(map_h, |e| match e {
        Some(Entry::Map(m)) => m.get(key).map(|&v| v as u64).filter(|&v| v != 0),
        _ => None,
    })
}

/// new Response(body, init) — body string + init.headers/status.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_NEW(body_ptr: i64, body_len: i64, init_h: u64) -> u64 {
    use indexmap::IndexMap;
    let body = str_from_parts(body_ptr, body_len).to_owned();
    let body_h = alloc_entry(Entry::String(body.into_bytes())) as i64;
    let headers_obj = opt_str_field(init_h, "headers").unwrap_or(0);
    let headers_h = headers_from_obj(headers_obj) as i64;
    let status = if init_h == 0 {
        200
    } else {
        with_entry(init_h, |e| match e {
            Some(Entry::Map(m)) => m.get("status").copied().unwrap_or(200),
            _ => 200,
        })
    };
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("body".to_string(), body_h);
    m.insert("headers".to_string(), headers_h);
    m.insert("status".to_string(), status);
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(b"Response".to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

/// response.headers → Headers handle (Map-based Response).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_HEADERS(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("headers").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// new Request(url, init) — url string + init.method/body.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REQUEST_NEW(url_ptr: i64, url_len: i64, init_h: u64) -> u64 {
    use indexmap::IndexMap;
    let url = str_from_parts(url_ptr, url_len).to_owned();
    let url_h = alloc_entry(Entry::String(url.into_bytes())) as i64;
    let method_h = opt_str_field(init_h, "method")
        .unwrap_or_else(|| alloc_entry(Entry::String(b"GET".to_vec())));
    let body_h =
        opt_str_field(init_h, "body").unwrap_or_else(|| alloc_entry(Entry::String(Vec::new())));
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("url".to_string(), url_h);
    m.insert("method".to_string(), method_h as i64);
    m.insert("body".to_string(), body_h as i64);
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(b"Request".to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REQUEST_METHOD(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("method").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REQUEST_URL(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("url").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// request.text() → Promise<string> (body).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REQUEST_TEXT(h: u64) -> u64 {
    let body_h = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("body").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    let slot = crate::promise_slot::new_fulfilled(body_h as i64);
    alloc_entry(Entry::PromiseAsync(slot))
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
        let slot = crate::promise_slot::new_fulfilled(h as i64);
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
    let slot = crate::promise_slot::new_fulfilled(result);
    alloc_entry(Entry::PromiseAsync(slot))
}

/// response.free() — libera o handle do Response.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FETCH_RESPONSE_FREE(h: u64) {
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
        assert!(
            is_promise,
            "RESPONSE_THEN(h, 0) deveria retornar PromiseAsync"
        );
    }

    extern "C" fn cb_returns_string(_h: i64) -> i64 {
        // Simula `r => r.text()` — retorna handle String puro.
        alloc_entry(Entry::String(b"hello".to_vec())) as i64
    }

    #[test]
    fn then_wraps_string_result_in_promise() {
        let h = make_response("hello");
        let cb = cb_returns_string as *const () as u64;
        let result = __RTS_FN_GL_FETCH_RESPONSE_THEN(h, cb);
        let is_promise = with_entry(result, |entry| {
            matches!(entry, Some(Entry::PromiseAsync(_)))
        });
        assert!(
            is_promise,
            "Resultado de String deve ser wrappado em PromiseAsync"
        );
    }

    extern "C" fn cb_returns_promise(h: i64) -> i64 {
        // Simula callback que ja retorna Promise: passthrough esperado.
        let slot = crate::promise_slot::new_fulfilled(h);
        alloc_entry(Entry::PromiseAsync(slot)) as i64
    }

    #[test]
    fn then_passthrough_when_callback_returns_promise() {
        let h = make_response("x");
        let cb = cb_returns_promise as *const () as u64;
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
