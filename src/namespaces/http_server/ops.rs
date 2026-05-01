//! `http_server` — implementacao via actix-web sobre tokio.
//!
//! Modelo de bridge sync TS → async actix:
//! 1. `serve(addr, handler_fn)` cria runtime tokio multi-thread, sobe
//!    HttpServer com N workers (N = CPUs disponiveis), bloqueia thread
//!    chamadora.
//! 2. Cada request entra num async handler que aloca `RequestSlot`
//!    (com tx oneshot), guarda num `Mutex<HashMap>` indexado por handle
//!    `u64`, e dispatcha o handler TS via `spawn_blocking` (necessario
//!    porque o handler TS chama codigo JIT sincrono que pode bloquear).
//! 3. Handler TS chama `respond(req_id, status, ct, body)` que encontra
//!    o slot, envia `ResponsePayload` (Send) pelo tx, e o async handler
//!    da actix monta o `HttpResponse` final e devolve pro cliente.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use tokio::sync::oneshot;

use super::super::gc::handles::{Entry, alloc_entry};

/// Numero de shards no slot map para reduzir contencao quando muitas
/// requests concorrentes alocam slots simultaneamente.
const SLOT_SHARDS: usize = 32;

/// Payload `Send`-safe que cruza o oneshot. `HttpResponse` direto nao
/// e' Send (body pode conter `dyn Any`).
struct ResponsePayload {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

/// Slot de request ativo. `tx` recebe o payload do handler TS.
struct RequestSlot {
    method: String,
    path: String,
    body: Vec<u8>,
    tx: Mutex<Option<oneshot::Sender<ResponsePayload>>>,
}

fn slots() -> &'static [Mutex<HashMap<u64, std::sync::Arc<RequestSlot>>>; SLOT_SHARDS] {
    static S: OnceLock<[Mutex<HashMap<u64, std::sync::Arc<RequestSlot>>>; SLOT_SHARDS]> =
        OnceLock::new();
    S.get_or_init(|| std::array::from_fn(|_| Mutex::new(HashMap::new())))
}

#[inline]
fn shard_for(req_id: u64) -> &'static Mutex<HashMap<u64, std::sync::Arc<RequestSlot>>> {
    &slots()[(req_id as usize) & (SLOT_SHARDS - 1)]
}

fn next_req_id() -> u64 {
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

fn dispatch_to_ts(handler_fn: u64, req_id: u64) {
    if handler_fn == 0 {
        return;
    }
    // SAFETY: contrato com codegen — `handler_fn` aponta pra
    // `extern "C" fn(u64) -> void`.
    let f: extern "C" fn(u64) = unsafe { std::mem::transmute(handler_fn as usize) };
    f(req_id);
}

async fn handle_request(
    req: HttpRequest,
    body: web::Bytes,
    handler_fn: web::Data<u64>,
) -> HttpResponse {
    let method = req.method().as_str().to_string();
    let path = req.path().to_string();
    let body_vec = body.to_vec();

    let (tx, rx) = oneshot::channel::<ResponsePayload>();
    let req_id = next_req_id();

    let slot = std::sync::Arc::new(RequestSlot {
        method,
        path,
        body: body_vec,
        tx: Mutex::new(Some(tx)),
    });
    {
        let mut map = shard_for(req_id).lock().unwrap_or_else(|e| e.into_inner());
        map.insert(req_id, slot);
    }

    let fn_ptr = **handler_fn;
    // Chamada direta na thread async — handler TS deve ser rapido
    // (sem I/O bloqueante). Se o handler precisar bloquear, devera
    // dispatch manualmente. Trade-off: elimina overhead de
    // spawn_blocking + thread switch (~10× ganho de throughput).
    dispatch_to_ts(fn_ptr, req_id);

    match rx.await {
        Ok(payload) => {
            let status = actix_web::http::StatusCode::from_u16(payload.status)
                .unwrap_or(actix_web::http::StatusCode::OK);
            let ct = payload.content_type;
            HttpResponse::build(status)
                .insert_header(("content-type", ct.as_str()))
                .body(payload.body)
        }
        Err(_) => HttpResponse::InternalServerError()
            .body("handler did not respond"),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_SERVE(
    addr_ptr: *const u8,
    addr_len: i64,
    handler_fn: u64,
) {
    if addr_ptr.is_null() || addr_len <= 0 || handler_fn == 0 {
        return;
    }
    let addr = unsafe {
        let slice = std::slice::from_raw_parts(addr_ptr, addr_len as usize);
        match std::str::from_utf8(slice) {
            Ok(s) => s.to_string(),
            Err(_) => return,
        }
    };

    // Usa o runtime tokio global compartilhado (issue #399). Antes
    // criavamos um runtime local — isolava do GC scanner e impedia
    // reuso por outras features async (fetch, fs, ws).
    crate::runtime::async_rt::rt().block_on(async move {
        let server = match HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(handler_fn))
                .default_service(web::to(handle_request))
        })
        .workers(num_cpus_or(4))
        .bind(&addr)
        {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[http_server] bind failed on {addr}: {e}");
                return;
            }
        };
        let _ = server.run().await;
    });
}

fn num_cpus_or(default: usize) -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(default)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_REQ_METHOD(req_id: u64) -> u64 {
    let bytes = {
        let map = shard_for(req_id).lock().unwrap_or_else(|e| e.into_inner());
        let Some(slot) = map.get(&req_id) else { return 0 };
        slot.method.as_bytes().to_vec()
    };
    alloc_entry(Entry::String(bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_REQ_PATH(req_id: u64) -> u64 {
    let bytes = {
        let map = shard_for(req_id).lock().unwrap_or_else(|e| e.into_inner());
        let Some(slot) = map.get(&req_id) else { return 0 };
        slot.path.as_bytes().to_vec()
    };
    alloc_entry(Entry::String(bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_REQ_BODY(req_id: u64) -> u64 {
    let bytes = {
        let map = shard_for(req_id).lock().unwrap_or_else(|e| e.into_inner());
        let Some(slot) = map.get(&req_id) else { return 0 };
        slot.body.clone()
    };
    alloc_entry(Entry::String(bytes))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_RESPOND(
    req_id: u64,
    status: i64,
    ct_ptr: *const u8,
    ct_len: i64,
    body_ptr: *const u8,
    body_len: i64,
) {
    let ct = if ct_ptr.is_null() || ct_len <= 0 {
        "text/plain".to_string()
    } else {
        unsafe {
            std::str::from_utf8(std::slice::from_raw_parts(ct_ptr, ct_len as usize))
                .unwrap_or("text/plain")
                .to_string()
        }
    };
    let body = if body_ptr.is_null() || body_len <= 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(body_ptr, body_len as usize).to_vec() }
    };

    let slot = {
        let mut map = shard_for(req_id).lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&req_id)
    };
    let Some(slot) = slot else { return };
    let tx = slot.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(tx) = tx {
        let payload = ResponsePayload {
            status: status.clamp(100, 599) as u16,
            content_type: ct,
            body,
        };
        let _ = tx.send(payload);
    }
}
