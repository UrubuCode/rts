//! `node:https` — `createServer`/`Server`, `request`/`get`, `Agent`/
//! `globalAgent`, against `docs/reference/node/https.md`.
//!
//! # Reuse-check, before anything here was written
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search: `rts-cranelift` has
//! nothing to say about a socket, a TLS handshake or an HTTP parser (its
//! table is value encodings, shapes, ABI — none of which this module
//! touches). Inside this crate the answer was "almost all of it, reuse
//! rather than re-derive": `node:http` already IS the HTTP/1.1 parser, the
//! chunked/`Content-Length` body decoder, `IncomingMessage` and
//! `ServerResponse`/`ClientRequest`'s `write`/`end`, and `node:tls` already
//! IS the handshake and the encrypted `Duplex`. `https` IS `http` over a
//! TLS socket, and being unable to reach either crate module's Rust
//! internals directly (`http`'s and `tls`'s submodules are private, per
//! `docs/reference/node/STATUS.md`'s rule and every existing module's own
//! doc) turned out not to matter: both modules' PARSING and
//! REQUEST-WRITING logic is reached anyway, at the JS level, by handing a
//! `TLSSocket` to the exact machinery `http` already runs on a `net.Socket`
//! — see [`server`]'s and [`client`]'s own docs for the two different shapes
//! that reuse takes (an event relay for the server side, a shared prototype
//! for the client side). **No HTTP parsing code and no second
//! `IncomingMessage`/`ServerResponse`/`ClientRequest` exist anywhere in this
//! module** — that was the one risk this task named, and avoiding it is
//! this module's whole design.
//!
//! What was NOT reusable, and is small enough here to not be a duplicate
//! parser: each request's `hostname`/`port`/`path`/`method`/`headers`
//! reader (`client.rs`), because `http::client`'s own copy is private to
//! that crate module — the same "read your own options object" cost every
//! module in this crate pays once.
//!
//! # This module inherits every limit `node:tls` names
//!
//! `docs/reference/node/STATUS.md`'s `node:tls` section is the authority:
//! TLS 1.3 only, X25519 preferred with P-256 as the fallback, AES-128-GCM and
//! ChaCha20-Poly1305 only, no TLS 1.2, no client certificates, no session
//! resumption. **This is not a client that works against an arbitrary
//! HTTPS server on the internet** — it works against a server (real or one
//! this crate's own `tls.createServer` built) configured to accept what the
//! provider offers. `getPeerCertificate()` answers `{}` (`tls/socket.rs`'s
//! own note: no X.509 field reader).
//!
//! # The rule this module pays like every other one here
//!
//! `with_runtime` holds a `RefCell` borrow for its body; an `extern "C"`
//! frame cannot unwind, so a call into a JS listener from inside one aborts
//! the process. Every native here collects what it needs, drops any borrow,
//! THEN calls a listener or another native — the same collect-then-call
//! shape `http`'s and `tls`'s own docs describe.
//!
//! # Not implemented, by name
//!
//! Everything `http`'s own module doc already lists as absent (keep-alive/
//! pipelining — `Connection: close` always, no second request read off one
//! socket; `100-continue`; `CONNECT`/`Upgrade`; write-side trailers;
//! `AbortSignal`; Unix domain sockets; a streaming `ClientRequest` — `.end()`
//! blocks for the whole exchange, per `client.rs`'s doc) applies here
//! unchanged, since this module runs those same `http` methods over a
//! `TLSSocket`. On top of that: **no separate `https.Agent`** — [`agent`]'s
//! own doc explains why handing back `http.Agent` itself is the honest
//! answer rather than a second no-op class. **`https.Server`'s `addContext`/
//! SNI-per-hostname routing** — `tls.createServer`'s own doc says one
//! `SecureContext` serves every connection; this module cannot offer more
//! than what it wraps. **`opts.ca`/custom CA validation on the client
//! side beyond `tls.connect`'s own** — read straight through, nothing added.
//! **`https.request`'s `agent: false` / socket-pooling options** — accepted
//! nowhere, since nothing here pools.

mod agent;
mod client;
mod common;
mod server;

use rts_core::entry::{self, Context, Provided};

/// The namespace `node:https` is — `createServer`/`Server`, `request`/
/// `get`, `Agent`/`globalAgent`, built the way this module's own doc
/// describes.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("createServer", server::create_server),
        ("request", client::request),
        ("get", client::get),
    ];
    let namespace = entry::make_namespace(context, members);

    let server_ctor = entry::make_callable(context, server::construct);
    entry::put_member(context, namespace, "Server", server_ctor);

    let agent_ctor = agent::agent_ctor(context);
    entry::put_member(context, namespace, "Agent", agent_ctor);
    let agent_prototype = entry::get_member(context, agent_ctor, "prototype");
    let global_agent = entry::make_instance(context, agent_prototype);
    entry::put_member(context, namespace, "globalAgent", global_agent);

    namespace
}
