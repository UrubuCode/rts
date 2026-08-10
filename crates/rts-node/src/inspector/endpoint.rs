//! The listener `open()` binds, and the discovery responses it answers.
//!
//! # What this is and what it is honestly not
//!
//! A real `TcpListener` on loopback, serving the three endpoints any
//! DevTools-class frontend probes first — `/json`, `/json/list`,
//! `/json/version`. So `url()` answers an address something can actually reach
//! and `waitForDebugger()` blocks on a real accepted connection rather than on
//! nothing.
//!
//! It is **not** a protocol server. There is no WebSocket upgrade and no
//! JSON-RPC command loop, so a frontend that probes discovery and then tries to
//! attach fails at the upgrade. `inspector.md` §5.1 calls this out as the
//! deferral it is, and the module doc repeats it — a listener that answered
//! discovery and then hung would be worse than one that refuses, because the
//! frontend would wait instead of reporting.
//!
//! # Loopback, and why that is not a default to make configurable lightly
//!
//! Node binds `127.0.0.1` for `--inspect` because the endpoint is a debugging
//! surface: anything that can reach it can, in a real implementation, evaluate
//! code in the process. A `host` argument is accepted and IGNORED here, and the
//! module doc says so — widening the bind is a security decision, and silently
//! honouring an argument that widens it is how that decision gets made by
//! accident.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};

/// What `open()` bound, if anything.
struct Endpoint {
    port: u16,
    /// The identifier in the `ws://` URL. Node's is a UUID and a program that
    /// only logs or parses the string cannot tell the difference; nothing here
    /// depends on its shape.
    id: String,
    /// Set when the accept thread has seen one connection, which is what
    /// `waitForDebugger` waits for.
    seen: mpsc::Receiver<()>,
    /// Read by the accept thread after every accept. A thread parked in
    /// `accept` cannot be interrupted, so [`close`] sets this AND connects once
    /// to wake it — setting the flag alone would leave the thread parked until
    /// something else happened to connect.
    stop: std::sync::Arc<AtomicBool>,
}

static ENDPOINT: Mutex<Option<Endpoint>> = Mutex::new(None);
/// Set while a listener is bound, so `open` can refuse a second one without
/// taking the lock in the common path.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Binds the endpoint. `Err` with a reason when one is already open or the bind
/// failed.
pub(super) fn open(port: u16) -> Result<u16, String> {
    if ACTIVE.load(Ordering::SeqCst) {
        return Err("the inspector is already open".to_owned());
    }
    let listener =
        TcpListener::bind(("127.0.0.1", port)).map_err(|error| format!("127.0.0.1: {error}"))?;
    let bound = listener.local_addr().map(|at| at.port()).unwrap_or(port);
    let id = identifier(bound);
    let (sender, seen) = mpsc::channel();
    let served = id.clone();
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let stopping = std::sync::Arc::clone(&stop);
    std::thread::Builder::new()
        .name("rts-inspector".to_owned())
        .spawn(move || {
            for accepted in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = accepted else { break };
                // Reported before the response, not after: `waitForDebugger`
                // waits for a CONNECTION, and a client that connects and never
                // sends a byte still counts — which is what a frontend probing
                // for liveness does.
                let _ = sender.send(());
                let mut request = [0u8; 1024];
                let read = stream.read(&mut request).unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..read]);
                let target = text.split_whitespace().nth(1).unwrap_or("/");
                let body = respond(target, bound, &served);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=UTF-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
                let _ = stream.flush();
            }
        })
        .map_err(|error| error.to_string())?;
    *ENDPOINT.lock().unwrap_or_else(|held| held.into_inner()) = Some(Endpoint {
        port: bound,
        id,
        seen,
        stop,
    });
    ACTIVE.store(true, Ordering::SeqCst);
    Ok(bound)
}

/// The JSON one of the three discovery endpoints answers.
///
/// Hand-written rather than built through a serializer: these are three fixed
/// shapes with one interpolated value each, and reaching for a JSON writer here
/// would be a dependency for two braces.
fn respond(target: &str, port: u16, id: &str) -> String {
    let websocket = format!("ws://127.0.0.1:{port}/{id}");
    match target {
        "/json/version" => format!(
            "{{\"Browser\":\"rts\",\"Protocol-Version\":\"1.3\",\"webSocketDebuggerUrl\":\"{websocket}\"}}"
        ),
        // `/json` and `/json/list` are the same answer in Node too.
        "/json" | "/json/list" => format!(
            "[{{\"description\":\"rts instance\",\"id\":\"{id}\",\"title\":\"rts\",\
             \"type\":\"node\",\"url\":\"file://\",\"webSocketDebuggerUrl\":\"{websocket}\"}}]"
        ),
        _ => "{}".to_owned(),
    }
}

/// The identifier in the URL.
///
/// Derived from the port rather than random, because `Math::random` is not
/// reachable from here and a real UUID would be a dependency for a string
/// nothing parses. Stated rather than dressed up as one.
fn identifier(port: u16) -> String {
    format!("rts-{port:04x}-0000-0000-0000-000000000000")
}

/// The `ws://` URL, or `None` when nothing is open.
pub(super) fn url() -> Option<String> {
    let held = ENDPOINT.lock().unwrap_or_else(|held| held.into_inner());
    held.as_ref()
        .map(|endpoint| format!("ws://127.0.0.1:{}/{}", endpoint.port, endpoint.id))
}

/// Whether an endpoint is open.
pub(super) fn is_open() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

/// Blocks until something connects, or returns at once when nothing is open.
///
/// Returning at once is deliberate and is what Node does: `waitForDebugger()`
/// without `open()` is a no-op there, and blocking forever on an endpoint that
/// does not exist is a hang a program cannot diagnose.
pub(super) fn wait() {
    let held = ENDPOINT.lock().unwrap_or_else(|held| held.into_inner());
    let Some(endpoint) = held.as_ref() else {
        return;
    };
    // `recv` on a channel the accept thread owns. The lock is held across it,
    // which is correct here and nowhere else: `wait` is defined as blocking the
    // program, so a second thread's `open` waiting behind it is the intended
    // behaviour rather than a stall.
    let _ = endpoint.seen.recv();
}

/// Closes the endpoint.
///
/// A thread parked in `accept` cannot be interrupted, and the listener lives in
/// that thread rather than in the entry — so dropping the entry does NOT end it.
/// Setting the flag and then connecting once does: the accept returns, the
/// thread reads the flag and leaves. Writing "the listener drops" here without
/// checking would have left a thread alive for the life of the process.
pub(super) fn close() {
    let held = ENDPOINT.lock().unwrap_or_else(|held| held.into_inner()).take();
    ACTIVE.store(false, Ordering::SeqCst);
    let Some(endpoint) = held else {
        return;
    };
    endpoint.stop.store(true, Ordering::SeqCst);
    let _ = std::net::TcpStream::connect(("127.0.0.1", endpoint.port));
}
