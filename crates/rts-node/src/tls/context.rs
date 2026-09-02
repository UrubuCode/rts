//! `tls.SecureContext`/`createSecureContext`, `rootCertificates`,
//! `getCiphers`, `getCACertificates`, `checkServerIdentity`,
//! `DEFAULT_MIN_VERSION`/`DEFAULT_MAX_VERSION` — the parts of
//! `docs/reference/node/tls.md` §2 that need no live socket.
//!
//! # Native state, the `Hash`/`Hmac` shape
//!
//! A `SecureContext` is an ordinary object carrying one hidden id
//! (`node:crypto`'s `hash.rs` is the worked example this follows), because
//! the actual state — DER cert/key bytes, and the `rustls::{ClientConfig,
//! ServerConfig}` built from them — cannot live on a JS value.
//!
//! # What this builds from PEM
//!
//! `pem.rs`'s hand-rolled reader gets DER bytes out of a `ca`/`cert`/`key`
//! PEM string; [`super::provider::sign::KeyProvider`] (via
//! `rustls::sign::CertifiedKey`/`with_single_cert`) turns the key DER into a
//! signing key — ECDSA-P256 or Ed25519 only, per that module's own doc.
//! `pfx`/PKCS12, `passphrase`-encrypted keys, and `ca`/`cert` arrays with
//! more than one entry are not read — see `mod.rs`'s "Not implemented".

use std::collections::HashMap;
use std::sync::Mutex;

use rts_core::entry::{self, Context, Provided};
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

pub(super) struct Held {
    pub(super) ca: Vec<CertificateDer<'static>>,
    pub(super) cert_chain: Vec<CertificateDer<'static>>,
    pub(super) key: Option<Vec<u8>>,
}

static CONTEXTS: Mutex<Option<HashMap<u64, Held>>> = Mutex::new(None);
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn with_contexts<T>(body: impl FnOnce(&mut HashMap<u64, Held>) -> T) -> T {
    let mut guard = CONTEXTS.lock().unwrap_or_else(|p| p.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// The namespace `node:tls`'s context-related members.
pub(super) fn members() -> &'static [(&'static str, Provided)] {
    &[
        ("createSecureContext", create_secure_context),
        ("checkServerIdentity", check_server_identity),
        ("getCiphers", get_ciphers),
        ("getCurves", get_curves),
        ("getCACertificates", get_ca_certificates),
    ]
}

/// `tls.createSecureContext(options?)`.
extern "C" fn create_secure_context(_e: u64, _this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    build(options)
}

/// The shared half of [`create_secure_context`] — also what
/// `server.rs::create_server` calls when `options` itself carries
/// `cert`/`key`/`ca` and no separate `secureContext` was given, the common
/// `tls.createServer({cert, key})` shape.
pub(super) fn build(options: u64) -> u64 {
    let (cert_pem, key_pem, ca_pem) = entry::with_runtime(|context| {
        (
            super::common::option_text(context, options, "cert"),
            super::common::option_text(context, options, "key"),
            super::common::option_text(context, options, "ca"),
        )
    });

    let cert_chain = cert_pem
        .and_then(|pem| super::pem::first_block(&pem))
        .map(|der| vec![CertificateDer::from(der)])
        .unwrap_or_default();
    let key = key_pem.and_then(|pem| super::pem::first_block(&pem));
    let ca = ca_pem.map(|pem| super::pem::blocks(&pem).into_iter().map(CertificateDer::from).collect()).unwrap_or_default();

    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    with_contexts(|table| {
        table.insert(id, Held { ca, cert_chain, key });
    });

    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "SecureContext", &[]);
        let instance = entry::make_instance(context, prototype);
        super::common::set_num(context, instance, "__contextId", id as f64);
        instance
    })
}

pub(super) fn context_id(instance: u64) -> Option<u64> {
    let value = super::common::get_value(instance, "__contextId");
    entry::number_of(value).map(|v| v as u64)
}

/// The roots this context declares, falling back to the bundled default
/// store (`rootCertificates`) when none were given.
pub(crate) fn roots_for(id: Option<u64>) -> RootCertStore {
    let extra = id.map(|id| with_contexts(|table| table.get(&id).map(|held| held.ca.clone()).unwrap_or_default())).unwrap_or_default();
    let mut store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for der in extra {
        let _ = store.add(der);
    }
    store
}

/// The end-entity chain and key this context declares, if it declared one —
/// `None` for a client-only context with no `cert`/`key`.
pub(super) fn identity_for(id: u64) -> Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    with_contexts(|table| {
        let held = table.get(&id)?;
        if held.cert_chain.is_empty() {
            return None;
        }
        let key = held.key.clone()?;
        Some((held.cert_chain.clone(), PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key))))
    })
}

/// `tls.checkServerIdentity(hostname, cert)` — matches `cert.subjectaltname`
/// (the string form `getPeerCertificate` already answers, `"DNS:a, DNS:b"`)
/// against `hostname`; a bare wildcard label (`*.example.com`) is honored,
/// nothing more exotic (IP SANs, `CN` fallback) is — see `mod.rs`'s "Not
/// implemented". Returns an `Error`-shaped object on mismatch, `undefined`
/// on match, matching Node's own return-not-throw contract.
extern "C" fn check_server_identity(_e: u64, _this: u64, hostname: u64, cert: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    let Some(host) = entry::text_of(hostname) else { return absent };
    let names = entry::with_runtime(|context| {
        let value = entry::get_member(context, cert, "subjectaltname");
        entry::text_in(context, value)
    });
    let matches = names.is_some_and(|names| {
        names.split(',').map(str::trim).any(|entry| {
            let Some(name) = entry.strip_prefix("DNS:") else { return false };
            match name.strip_prefix("*.") {
                Some(rest) => host.ends_with(rest) && host.len() > rest.len() && host.as_bytes()[host.len() - rest.len() - 1] == b'.',
                None => name.eq_ignore_ascii_case(&host),
            }
        })
    });
    if matches {
        return absent;
    }
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        let message = entry::make_string(context, &format!("Hostname/IP does not match certificate's altnames: Host: {host}."));
        let reason = entry::make_string(context, "altname mismatch");
        entry::put_member(context, object, "message", message);
        entry::put_member(context, object, "reason", reason);
        object
    })
}

/// `tls.getCiphers()`.
extern "C" fn get_ciphers(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let values = super::provider::cipher_names().iter().map(|name| entry::make_string(context, name)).collect();
        entry::make_array_in(context, values)
    })
}

/// `tls.getCurves()` — the key-exchange groups [`super::provider`] actually
/// wires (`kx_groups: vec![&X25519, &SECP256R1]`), named the way OpenSSL
/// (and so Node) names them: `x25519`, `prime256v1` for P-256. Real Node
/// lists dozens more OpenSSL knows about; this crate answers only the ones a
/// handshake here can actually complete with, which is the same "no
/// fabricated capability" rule [`get_ca_certificates`] states.
extern "C" fn get_curves(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| {
        let values = ["x25519", "prime256v1"].iter().map(|name| entry::make_string(context, name)).collect();
        entry::make_array_in(context, values)
    })
}

/// `tls.getCACertificates(type?)` — always `[]`. `webpki_roots::
/// TLS_SERVER_ROOTS` (what [`roots_for`] verifies against, and the only CA
/// data this module holds) is shipped as `TrustAnchor`s — subject, SPKI and
/// name-constraints only, not the certificate's own DER/PEM — so there is no
/// real PEM text to hand back for `'default'`/`'bundled'`, and `'system'`/
/// `'extra'` have no OS-store/`NODE_EXTRA_CA_CERTS` integration at all. An
/// empty, honest answer rather than a fabricated one; see `mod.rs`'s "Not
/// implemented".
extern "C" fn get_ca_certificates(_e: u64, _this: u64, _kind: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::with_runtime(|context| entry::make_array_in(context, Vec::new()))
}

/// `tls.rootCertificates` — always `[]`, for the same reason
/// [`get_ca_certificates`] is.
pub(super) fn root_certificates(context: &mut Context) -> u64 {
    entry::make_array_in(context, Vec::new())
}
