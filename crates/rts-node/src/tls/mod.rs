//! `node:tls` — the socket-independent surface: `getCiphers()` (the TLS cipher
//! suites RTS's rustls backend supports) and `getCurves()` (the key-exchange
//! groups). Real data — the actual suites/groups rustls implements by default,
//! lowercased as Node presents them. Not fabricated.
//!
//! Deferred (need the event-loop / socket / stream subsystems): the `TLSSocket`/
//! `Server` classes (`connect`/`createServer` + their data/secureConnect
//! events), `SecureContext`, `checkServerIdentity`, `rootCertificates` (the CA
//! bundle), the whole networked TLS machinery.
//!
//! Layout: `mod` (data + registration).

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::string_word;
use rts_engine::Engine;

/// The cipher suites rustls implements (TLS 1.3 + the TLS 1.2 ECDHE-AEAD set),
/// in Node's lowercase form.
const CIPHERS: &[&str] = &[
    "tls_aes_256_gcm_sha384",
    "tls_aes_128_gcm_sha256",
    "tls_chacha20_poly1305_sha256",
    "ecdhe-ecdsa-aes256-gcm-sha384",
    "ecdhe-rsa-aes256-gcm-sha384",
    "ecdhe-ecdsa-chacha20-poly1305",
    "ecdhe-rsa-chacha20-poly1305",
    "ecdhe-ecdsa-aes128-gcm-sha256",
    "ecdhe-rsa-aes128-gcm-sha256",
];

/// The key-exchange groups rustls offers by default.
const CURVES: &[&str] = &["x25519", "secp256r1", "secp384r1"];

fn str_array(items: &[&str]) -> Handle {
    let words: Vec<i64> = items.iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// `tls.getCiphers()`.
#[rtse::function(module = "node:tls", value = "getCiphers", pure)]
fn get_ciphers() -> Handle {
    str_array(CIPHERS)
}

/// `tls.getCurves()`.
#[rtse::function(module = "node:tls", value = "getCurves", pure)]
fn get_curves() -> Handle {
    str_array(CURVES)
}

/// Registers the `node:tls` surface.
pub fn register(e: &mut Engine) {
    // `.registry(...)` is a `ModuleScope` method — the closure form — not one on
    // the fluent `ModuleBuilder` that `e.ns(...)` returns.
    e.module("node:tls", |m| {
        m.doc("TLS (node:tls): getCiphers, getCurves.");
        m.registry(get_ciphers_entry());
        m.registry(get_curves_entry());
    });
}
