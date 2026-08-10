//! `node:tls` — over `docs/reference/node/tls.md`, on a pure-Rust
//! `rustls::CryptoProvider` built from the RustCrypto crates `node:crypto`
//! already vets (`docs/reference/node/crates.md` §6, owner-chosen option
//! (b)). [`provider`] is the provider itself; everything else here is the
//! JS-facing surface over it.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md` found nothing to call instead —
//! see [`provider`]'s own doc for the machine-layer half of that search.
//! Inside this crate, `net/socket.rs` and `net/registry.rs` are the worked
//! examples this module's own `socket.rs`/`registry.rs` follow for the
//! background-thread/JS-thread handoff and the `Duplex`-chaining shape;
//! `stream`'s prototypes are reused by name (`entry::make_prototype`), never
//! rebuilt.
//!
//! # WHEN a `'secureConnect'`/`'data'` listener runs
//!
//! `socket.rs`'s own "WHEN" section is the answer, and `server.rs`'s
//! `'secureConnection'` is the same answer restated server-side: a queued
//! event's listener runs synchronously, at the start of the next
//! `node:net`/`node:tls` call this crate's natives make — never on a timer,
//! and a program that stops calling either module never observes one fire.
//! Named, not hidden, the same limit `net/mod.rs` states for its own
//! events.
//!
//! # Coverage
//!
//! See [`provider`]'s own doc for exactly which cipher suites/key exchange
//! groups/signature algorithms this ships with today, and what is
//! deliberately deferred.
//!
//! # Not implemented, by name
//!
//! Beyond `socket.rs`'s and `server.rs`'s own sections: `https`/`http2` (not
//! built on this yet), `tls.setDefaultCACertificates`,
//! `tls.DEFAULT_ECDH_CURVE`/`DEFAULT_CIPHERS`/`CLIENT_RENEG_LIMIT`/
//! `CLIENT_RENEG_WINDOW` (no renegotiation exists to bound), PFX/PKCS12,
//! passphrase-encrypted keys, multi-entry `cert`/`ca` chains beyond the
//! first PEM block, `onread`, `lookup`, IPC/`path` sockets (the same gap
//! `node:net` itself names).

mod common;
mod conn;
mod context;
mod pem;
mod provider;
mod registry;
mod server;
mod socket;

use rts_core::entry::{self, Provided};

/// The namespace `node:tls` is.
pub fn namespace(context: &mut entry::Context) -> u64 {
    let mut members: Vec<(&str, Provided)> = vec![
        ("connect", socket::connect),
        ("createServer", server::create_server),
    ];
    members.extend_from_slice(context::members());
    let namespace = entry::make_namespace(context, &members);

    let roots = context::root_certificates(context);
    entry::put_member(context, namespace, "rootCertificates", roots);
    let min_version = entry::make_string(context, "TLSv1.2");
    entry::put_member(context, namespace, "DEFAULT_MIN_VERSION", min_version);
    let max_version = entry::make_string(context, "TLSv1.3");
    entry::put_member(context, namespace, "DEFAULT_MAX_VERSION", max_version);

    namespace
}
