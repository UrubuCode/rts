//! `tls` namespace — sync TLS 1.2/1.3 client via rustls (issue #238).
//!
//! Wraps a `net` TcpStream in a TLS connection. Trust store is webpki-roots
//! (embedded Mozilla bundle); no dependency on the OS trust store.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::io::{Read, Write};
use std::sync::Arc;

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_macro::rts_namespace;
use rustls::{ClientConfig, ClientConnection, RootCertStore, Stream};

use crate::namespaces::gc::handles::{Entry, alloc_entry, free_handle, with_entry_mut};

/// A TLS client stream stored in the HandleTable.
pub struct TlsClientStream {
    pub conn: ClientConnection,
    pub tcp: std::net::TcpStream,
}

impl std::fmt::Debug for TlsClientStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsClientStream").finish_non_exhaustive()
    }
}

/// Default ClientConfig using webpki-roots (Mozilla CAs), cached.
fn default_config() -> Arc<ClientConfig> {
    use std::sync::OnceLock;
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut roots = RootCertStore::empty();
        for ta in webpki_roots::TLS_SERVER_ROOTS {
            roots.roots.push(rustls::pki_types::TrustAnchor {
                subject: rustls::pki_types::Der::from_slice(ta.subject.as_ref()),
                subject_public_key_info: rustls::pki_types::Der::from_slice(
                    ta.subject_public_key_info.as_ref(),
                ),
                name_constraints: ta
                    .name_constraints
                    .as_ref()
                    .map(|nc| rustls::pki_types::Der::from_slice(nc.as_ref())),
            });
        }
        let cfg = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    })
    .clone()
}

/// Sync TLS 1.2/1.3 client (rustls + webpki-roots).
#[rts_namespace(tls)]
impl TlsNs {
    /// Wrap a TCP stream handle in a TLS client (SNI `sniHostname`). Handle, 0 on error.
    #[rts_fn(ts = "client(tcp: number, sniHostname: string): number")]
    pub fn client(tcp: U64, sni: Str) -> Handle {
        // Take the TcpStream out of `tcp`'s slot (transfer ownership).
        let tcp_stream: Option<std::net::TcpStream> = with_entry_mut(tcp, |entry| match entry {
            Some(e @ Entry::TcpStream(_)) => {
                let taken = std::mem::replace(e, Entry::Free);
                if let Entry::TcpStream(boxed) = taken {
                    Some(*boxed)
                } else {
                    None
                }
            }
            _ => None,
        });
        let Some(tcp_stream) = tcp_stream else {
            return 0;
        };
        free_handle(tcp);

        let server_name: rustls::pki_types::ServerName<'static> = match sni.to_string().try_into() {
            Ok(n) => n,
            Err(_) => return 0,
        };
        let conn = match ClientConnection::new(default_config(), server_name) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        alloc_entry(Entry::TlsClient(Box::new(TlsClientStream {
            conn,
            tcp: tcp_stream,
        })))
    }

    /// Encrypt + send `data`. Bytes written, -1 on error.
    #[rts_fn(on_null = -1)]
    pub fn send(stream: U64, data: Str) -> I64 {
        with_entry_mut(stream, |entry| {
            let Some(Entry::TlsClient(s)) = entry else {
                return -1;
            };
            let mut tls = Stream::new(&mut s.conn, &mut s.tcp);
            match tls.write(data.as_bytes()) {
                Ok(n) => n as i64,
                Err(_) => -1,
            }
        })
    }

    /// Decrypt into a raw buffer. Count, 0 on clean EOF, -1 on error.
    #[rts_fn(ts = "recv(stream: number, bufPtr: number, len: number): number")]
    pub fn recv(stream: U64, buf_ptr: U64, len: I64) -> I64 {
        if len < 0 || buf_ptr == 0 {
            return -1;
        }
        with_entry_mut(stream, |entry| {
            let Some(Entry::TlsClient(s)) = entry else {
                return -1;
            };
            // SAFETY: caller passes a valid raw pointer.
            let dst = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, len as usize) };
            let mut tls = Stream::new(&mut s.conn, &mut s.tcp);
            match tls.read(dst) {
                Ok(n) => n as i64,
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => 0,
                Err(_) => -1,
            }
        })
    }

    /// Send close_notify and free the handle.
    #[rts_fn]
    pub fn close(stream: U64) {
        with_entry_mut(stream, |entry| {
            if let Some(Entry::TlsClient(s)) = entry {
                s.conn.send_close_notify();
                let _ = s.conn.complete_io(&mut s.tcp);
            }
        });
        free_handle(stream);
    }
}
