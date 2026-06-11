//! `http_server` namespace — native HTTP/1.1 server via actix-web over the
//! shared tokio runtime.
//!
//! Sync TS → async bridge: `serve(addr, handler)` blocks, each request enters a
//! sharded slot map, the TS handler is called directly on the async thread, and
//! `respond(...)` returns the response via a oneshot. Keep-alive, pipelining and
//! header parsing come from actix; workers = `available_parallelism()`.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};
use tokio::sync::oneshot;

use rts_engine::heap::handles::{Entry, alloc_entry};

const SLOT_SHARDS: usize = 32;

/// `Send`-safe payload crossing the oneshot (HttpResponse is not Send).
struct ResponsePayload {
    status: u16,
    content_type: String,
    body: Vec<u8>,
}

/// Active request slot; `tx` receives the payload from the TS handler.
struct RequestSlot {
    method: String,
    path: String,
    body: Vec<u8>,
    tx: Mutex<Option<oneshot::Sender<ResponsePayload>>>,
}

#[allow(clippy::type_complexity)]
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
    // SAFETY: codegen contract — `handler_fn` is `extern "C" fn(u64)`.
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

    dispatch_to_ts(**handler_fn, req_id);

    match rx.await {
        Ok(payload) => {
            let status = actix_web::http::StatusCode::from_u16(payload.status)
                .unwrap_or(actix_web::http::StatusCode::OK);
            HttpResponse::build(status)
                .insert_header(("content-type", payload.content_type.as_str()))
                .body(payload.body)
        }
        Err(_) => HttpResponse::InternalServerError().body("handler did not respond"),
    }
}

fn num_cpus_or(default: usize) -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(default)
}

/// Blocks serving `addr`, dispatching each request to `handler(req)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_SERVE(addr_ptr: *const u8, addr_len: i64, handler: U64) {
    let addr = match unsafe { rts_engine::abi::str_abi::from_abi(addr_ptr, addr_len) } {
        Some(s) => s,
        None => return,
    };
    if handler == 0 {
        return;
    }
    let addr = addr.to_string();
    crate::runtime::async_rt::rt().block_on(async move {
        let server = match HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(handler))
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

/// Request method (e.g. "GET"). String handle, 0 if the req is gone.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_REQ_METHOD(req: U64) -> Handle {
    let bytes = {
        let map = shard_for(req).lock().unwrap_or_else(|e| e.into_inner());
        let Some(slot) = map.get(&req) else { return 0 };
        slot.method.as_bytes().to_vec()
    };
    alloc_entry(Entry::String(bytes))
}

/// Request path. String handle, 0 if the req is gone.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_REQ_PATH(req: U64) -> Handle {
    let bytes = {
        let map = shard_for(req).lock().unwrap_or_else(|e| e.into_inner());
        let Some(slot) = map.get(&req) else { return 0 };
        slot.path.as_bytes().to_vec()
    };
    alloc_entry(Entry::String(bytes))
}

/// Request body. String handle, 0 if the req is gone.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_REQ_BODY(req: U64) -> Handle {
    let bytes = {
        let map = shard_for(req).lock().unwrap_or_else(|e| e.into_inner());
        let Some(slot) = map.get(&req) else { return 0 };
        slot.body.clone()
    };
    alloc_entry(Entry::String(bytes))
}

/// Responds to `req` with `status`, `contentType` and `body`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_HTTP_SERVER_RESPOND(
    req: U64,
    status: I64,
    content_type_ptr: *const u8,
    content_type_len: i64,
    body_ptr: *const u8,
    body_len: i64,
) {
    let content_type = match unsafe { rts_engine::abi::str_abi::from_abi(content_type_ptr, content_type_len) } {
        Some(s) => s,
        None => return,
    };
    let body = match unsafe { rts_engine::abi::str_abi::from_abi(body_ptr, body_len) } {
        Some(s) => s,
        None => return,
    };
    // Empty content-type falls back to text/plain (legacy behaviour).
    let content_type = if content_type.is_empty() {
        "text/plain".to_string()
    } else {
        content_type.to_string()
    };
    let slot = {
        let mut map = shard_for(req).lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&req)
    };
    let Some(slot) = slot else { return };
    let tx = slot.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(tx) = tx {
        let payload = ResponsePayload {
            status: status.clamp(100, 599) as u16,
            content_type,
            body: body.as_bytes().to_vec(),
        };
        let _ = tx.send(payload);
    }
}

/// Função `http_server.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `http_server` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("http_server")
        .doc("Native HTTP/1.1 server (actix-web over the shared tokio runtime).")
        .member(func(
            "serve",
            "__RTS_FN_NS_HTTP_SERVER_SERVE",
            Sig::new(vec![AbiType::StrPtr, AbiType::U64], AbiType::Void),
            "serve(addr: string, handler: (req: number) => void): void",
            "Blocks serving `addr`, dispatching each request to `handler(req)`.",
            __RTS_FN_NS_HTTP_SERVER_SERVE as *const u8,
        ))
        .member(func(
            "req_method",
            "__RTS_FN_NS_HTTP_SERVER_REQ_METHOD",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "req_method(req: number): string",
            "Request method (e.g. \"GET\"). String handle, 0 if the req is gone.",
            __RTS_FN_NS_HTTP_SERVER_REQ_METHOD as *const u8,
        ))
        .member(func(
            "req_path",
            "__RTS_FN_NS_HTTP_SERVER_REQ_PATH",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "req_path(req: number): string",
            "Request path. String handle, 0 if the req is gone.",
            __RTS_FN_NS_HTTP_SERVER_REQ_PATH as *const u8,
        ))
        .member(func(
            "req_body",
            "__RTS_FN_NS_HTTP_SERVER_REQ_BODY",
            Sig::new(vec![AbiType::U64], AbiType::Handle),
            "req_body(req: number): string",
            "Request body. String handle, 0 if the req is gone.",
            __RTS_FN_NS_HTTP_SERVER_REQ_BODY as *const u8,
        ))
        .member(func(
            "respond",
            "__RTS_FN_NS_HTTP_SERVER_RESPOND",
            Sig::new(
                vec![AbiType::U64, AbiType::I64, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Void,
            ),
            "respond(req: number, status: number, contentType: string, body: string): void",
            "Responds to `req` with `status`, `contentType` and `body`.",
            __RTS_FN_NS_HTTP_SERVER_RESPOND as *const u8,
        ))
        .done();
}
