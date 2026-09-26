//! The Chrome DevTools Protocol endpoint — `--inspect`, the browser's F12.
//!
//! Plan: `docs/superpowers/plans/2026-09-26-dom-inspector.md`.
//!
//! # Off means nothing exists
//!
//! [`start_if_requested`] is the only door the host opens, and it returns at
//! once when neither [`request`] nor `RTS_INSPECT` asked for anything: no port
//! is bound, no thread spawned, no loop source registered. The whole module is
//! behind this crate's `cdp` feature, so a build that never asks does not carry
//! it at all.
//!
//! # One dispatch, two transports
//!
//! A message arrives on a socket thread, which cannot touch the runtime or the
//! document — both belong to the program's thread. So the socket thread only
//! queues it, and [`pump`], a loop source, runs it on the program's thread:
//! `Runtime.*` through `session.rs`'s own dispatch (the one `session.post`
//! uses), `DOM.*`/`CSS.*`/`Overlay.*`/`Page.*` through the handler the host
//! declared with [`declare_domains`]. This crate never learns the document;
//! `rts-host` is the crate allowed to name both sides.

mod runtime;
pub(super) mod transport;

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rts_core::entry::{self, Context, Pending};
use serde_json::{Value, json};

/// What a domain method answered: its result, and the events it raises —
/// `DOM.requestChildNodes` answers `{}` and delivers the children as a
/// `DOM.setChildNodes` event, which is the protocol's shape, not ours.
pub struct Reply {
    /// The `result` member of the response.
    pub result: Value,
    /// `(method, params)` of each event, sent before the response.
    pub events: Vec<(String, Value)>,
}

impl Reply {
    /// A result with no events.
    pub fn plain(result: Value) -> Reply {
        Reply { result, events: Vec::new() }
    }
}

/// A domain handler: `None` when the method is not its own, `Err` with the
/// reason when it is and refuses.
pub type Domains = fn(method: &str, params: &Value) -> Option<Result<Reply, String>>;

/// The port asked for by the CLI (`--inspect[=port]`) or a config call.
static REQUESTED: Mutex<Option<u16>> = Mutex::new(None);
/// Set once DOM domains are declared: `/json/list` then offers a `page` target,
/// which DevTools opens with the Elements panel. A `node` target has none.
static PAGE: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The program thread's domain handler — the thread that owns the document.
    static DOMAINS: Cell<Option<Domains>> = const { Cell::new(None) };
}

/// Node's default inspector port.
pub const DEFAULT_PORT: u16 = 9229;

/// Asks for the inspector on `port`. Nothing starts here: the host starts it
/// once the program's context exists ([`start_if_requested`]).
pub fn request(port: u16) {
    *REQUESTED.lock().unwrap_or_else(|held| held.into_inner()) = Some(port);
}

/// The port asked for, by [`request`] first and `RTS_INSPECT` second
/// (`1`, `true` or empty mean [`DEFAULT_PORT`], a number means that port).
pub fn requested() -> Option<u16> {
    if let Some(port) = *REQUESTED.lock().unwrap_or_else(|held| held.into_inner()) {
        return Some(port);
    }
    let raw = std::env::var("RTS_INSPECT").ok()?;
    match raw.trim() {
        "" | "1" | "true" => Some(DEFAULT_PORT),
        "0" | "false" => None,
        other => other.parse().ok(),
    }
}

/// Declares the handler for the document domains. Called by the host, on the
/// program's thread, whether or not the inspector is ever started — a `fn`
/// pointer in a cell costs nothing.
pub fn declare_domains(domains: Domains) {
    DOMAINS.with(|cell| cell.set(Some(domains)));
    PAGE.store(true, Ordering::SeqCst);
}

/// Starts the inspector when one was asked for; otherwise does nothing at all.
pub fn start_if_requested(context: &mut Context) {
    let Some(port) = requested() else {
        return;
    };
    if let Err(reason) = start(context, port) {
        eprintln!("rts: the inspector did not start: {reason}");
    }
}

/// Binds the endpoint, registers the loop source and prints where to attach.
/// Answers the `ws://` URL.
pub fn start(context: &mut Context, port: u16) -> Result<String, String> {
    let bound = super::endpoint::open(port)?;
    entry::declare_loop_source(context, "inspector-cdp", pump);
    let url = super::endpoint::url().unwrap_or_default();
    let address = url.trim_start_matches("ws://");
    eprintln!("Debugger listening on {url}");
    eprintln!("DevTools: devtools://devtools/bundled/inspector.html?ws={address}");
    eprintln!("or open chrome://inspect and add 127.0.0.1:{bound}");
    Ok(url)
}

/// Whether the endpoint should offer a page target.
pub(super) fn page_mode() -> bool {
    PAGE.load(Ordering::SeqCst)
}

/// The loop source: runs every queued message on the program's thread.
///
/// Answers `In(..)` for as long as the endpoint is open, which holds a finished
/// program open — Node exits instead, but a headless document has no other
/// lifetime, and an inspector that closes before anything can attach is not
/// one. Ctrl-C ends it, as it ends `node --inspect-wait`.
fn pump() -> Pending {
    for incoming in transport::drain() {
        match incoming {
            transport::Incoming::Message(connection, text) => {
                for frame in answer(&text) {
                    transport::send(connection, &frame);
                }
            }
            transport::Incoming::Closed => {
                // The frontend left: nothing highlighted stays highlighted.
                let _ = dispatch("Overlay.hideHighlight", &json!({}));
            }
        }
    }
    if super::endpoint::is_open() {
        Pending::In(Duration::from_millis(16))
    } else {
        Pending::Idle
    }
}

/// The frames one request produces: its events, then its response.
pub(super) fn answer(text: &str) -> Vec<String> {
    let Ok(message) = serde_json::from_str::<Value>(text) else {
        return vec![json!({"error": {"code": -32700, "message": "parse error"}}).to_string()];
    };
    let id = message.get("id").cloned().unwrap_or(Value::Null);
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let mut frames = Vec::new();
    match dispatch(method, &params) {
        Ok(reply) => {
            for (name, event) in reply.events {
                frames.push(json!({"method": name, "params": event}).to_string());
            }
            frames.push(json!({"id": id, "result": reply.result}).to_string());
        }
        Err(reason) => frames.push(
            json!({"id": id, "error": {"code": -32601, "message": reason}}).to_string(),
        ),
    }
    frames
}

/// One method, by domain.
fn dispatch(method: &str, params: &Value) -> Result<Reply, String> {
    // Sent unconditionally by DevTools on attach; each changes nothing here and
    // refusing them would make the frontend log an error per connection.
    if matches!(
        method,
        "Inspector.enable" | "Log.enable" | "Page.enable" | "Target.setAutoAttach"
            | "Runtime.runIfWaitingForDebugger"
    ) {
        return Ok(Reply::plain(json!({})));
    }
    if let Some(domains) = DOMAINS.with(Cell::get) {
        if let Some(answered) = domains(method, params) {
            return answered;
        }
    }
    runtime::call(method, params)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame DevTools cannot parse still gets a JSON-RPC answer rather than
    /// silence, which would leave the frontend waiting.
    #[test]
    fn a_malformed_message_answers_a_parse_error() {
        let frames = answer("{not json");
        assert_eq!(frames.len(), 1);
        assert!(frames[0].contains("-32700"));
    }

    #[test]
    fn an_explicit_request_is_the_port_the_host_starts_on() {
        // `request` wins over the environment, and is what the CLI sets.
        request(9333);
        assert_eq!(requested(), Some(9333));
    }
}
