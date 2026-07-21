//! `tls` namespace — sync TLS 1.2/1.3 client via rustls (issue #238).
//!
//! Wraps a `net` TcpStream in a TLS connection. Trust store is webpki-roots
//! (embedded Mozilla bundle); no dependency on the OS trust store.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::io::{Read, Write};
use std::sync::Arc;

use rts_engine::abi::ty::{Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_CLIENT(tcp: U64, sni_ptr: *const u8, sni_len: i64) -> Handle {
    let sni = match unsafe { rts_engine::abi::str_abi::from_abi(sni_ptr, sni_len) } {
        Some(s) => s,
        None => return 0,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_SEND(stream: U64, data_ptr: *const u8, data_len: i64) -> I64 {
    let data = match unsafe { rts_engine::abi::str_abi::from_abi(data_ptr, data_len) } {
        Some(s) => s,
        None => return -1,
    };
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_RECV(stream: U64, buf_ptr: U64, len: I64) -> I64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_CLOSE(stream: U64) {
    with_entry_mut(stream, |entry| {
        if let Some(Entry::TlsClient(s)) = entry {
            s.conn.send_close_notify();
            let _ = s.conn.complete_io(&mut s.tcp);
        }
    });
    free_handle(stream);
}

/// Função `tls.f(args)`.
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
        emit: None,
    }
}

/// Registra a namespace `tls` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("tls")
        .doc("Sync TLS 1.2/1.3 client (rustls + webpki-roots).")
        .member(func(
            "client",
            "__RTS_FN_NS_TLS_CLIENT",
            Sig::new(vec![AbiType::U64, AbiType::StrPtr], AbiType::Handle),
            "client(tcp: number, sniHostname: string): number",
            "Wrap a TCP stream handle in a TLS client (SNI `sniHostname`). Handle, 0 on error.",
            __RTS_FN_NS_TLS_CLIENT as *const u8,
        ))
        .member(func(
            "send",
            "__RTS_FN_NS_TLS_SEND",
            Sig::new(vec![AbiType::U64, AbiType::StrPtr], AbiType::I64),
            "send(stream: number, data: string): number",
            "Encrypt + send `data`. Bytes written, -1 on error.",
            __RTS_FN_NS_TLS_SEND as *const u8,
        ))
        .member(func(
            "recv",
            "__RTS_FN_NS_TLS_RECV",
            Sig::new(vec![AbiType::U64, AbiType::U64, AbiType::I64], AbiType::I64),
            "recv(stream: number, bufPtr: number, len: number): number",
            "Decrypt into a raw buffer. Count, 0 on clean EOF, -1 on error.",
            __RTS_FN_NS_TLS_RECV as *const u8,
        ))
        .member(func(
            "close",
            "__RTS_FN_NS_TLS_CLOSE",
            Sig::new(vec![AbiType::U64], AbiType::Void),
            "close(stream: number): void",
            "Send close_notify and free the handle.",
            __RTS_FN_NS_TLS_CLOSE as *const u8,
        ))
        .done();
}
