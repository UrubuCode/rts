//! `tls` namespace — sync TLS 1.2/1.3 client via rustls (issue #238).
//!
//! Wraps a `net` TcpStream in a TLS connection. Trust store is webpki-roots
//! (embedded Mozilla bundle); no dependency on the OS trust store.
//!
//! Convertido pro modelo de autoria `#[rtse::function]` (fonte única de
//! símbolos — ver `docs/engine/architecture.md`).

use std::io::{Read, Write};
use std::sync::Arc;

use rts_engine::Engine;
use rts_engine::abi::ty::{Handle, I64, U64};
use rustls::{ClientConfig, ClientConnection, RootCertStore, Stream};

// `TlsClientStream` migrou pro heap do motor (`rts_engine::heap::handles`);
// o I/O do TLS continua aqui e referencia o tipo via facade.
use rts_engine::heap::handles::{
    Entry, TlsClientStream, alloc_entry, free_handle, with_entry_mut,
};

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

/// Wrap a TCP stream handle in a TLS client (SNI `sniHostname`). Handle, 0 on error.
#[rtse::function(module = "tls", value = "client", ret_ts = "number")]
fn client(tcp: U64, sni: &str) -> Handle {
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
#[rtse::function(module = "tls", value = "send")]
fn send(stream: U64, data: &str) -> I64 {
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
#[rtse::function(module = "tls", value = "recv")]
fn recv(stream: U64, buf_ptr: U64, len: I64) -> I64 {
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
#[rtse::function(module = "tls", value = "close")]
fn close(stream: U64) {
    with_entry_mut(stream, |entry| {
        if let Some(Entry::TlsClient(s)) = entry {
            s.conn.send_close_notify();
            let _ = s.conn.complete_io(&mut s.tcp);
        }
    });
    free_handle(stream);
}

/// Registra a namespace `tls` no motor.
pub fn register(e: &mut Engine) {
    e.module("tls", |m| {
        m.doc("Sync TLS 1.2/1.3 client (rustls + webpki-roots).");
        m.registry(client_entry());
        m.registry(send_entry());
        m.registry(recv_entry());
        m.registry(close_entry());
    });
}
