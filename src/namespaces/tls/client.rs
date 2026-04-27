//! TLS client — handshake + wrap de TcpStream.

use std::sync::Arc;

use rustls::{ClientConfig, ClientConnection, RootCertStore};

use super::super::gc::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

/// Stream TLS client armazenado na HandleTable.
pub struct TlsClientStream {
    pub conn: ClientConnection,
    pub tcp: std::net::TcpStream,
}

impl std::fmt::Debug for TlsClientStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsClientStream").finish_non_exhaustive()
    }
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// Constroi o ClientConfig padrao usando webpki-roots (CAs Mozilla).
/// Cacheado num OnceLock pra evitar reconstruir a cada client().
fn default_config() -> Arc<ClientConfig> {
    use std::sync::OnceLock;
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Arc::new(cfg)
    })
    .clone()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_CLIENT(
    tcp_handle: u64,
    sni_ptr: *const u8,
    sni_len: i64,
) -> u64 {
    let Some(sni_str) = str_from_abi(sni_ptr, sni_len) else {
        return 0;
    };

    // Move o TcpStream pra fora do slot do tcp_handle (transfere ownership).
    let tcp: std::net::TcpStream = {
        let t = shard_for_handle(tcp_handle);
        let mut guard = t.lock().unwrap();
        match guard.get_mut(tcp_handle) {
            Some(entry @ Entry::TcpStream(_)) => {
                let taken = std::mem::replace(entry, Entry::Free);
                if let Entry::TcpStream(boxed) = taken {
                    *boxed
                } else {
                    return 0;
                }
            }
            _ => return 0,
        }
    };
    // libera formal o slot (bump generation).
    free_handle(tcp_handle);

    // Constroi a connection com SNI.
    let server_name: rustls::pki_types::ServerName<'static> =
        match sni_str.to_string().try_into() {
            Ok(n) => n,
            Err(_) => return 0,
        };
    let conn = match ClientConnection::new(default_config(), server_name) {
        Ok(c) => c,
        Err(_) => return 0,
    };

    let stream = TlsClientStream { conn, tcp };
    alloc_entry(Entry::TlsClient(Box::new(stream)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_CLOSE(handle: u64) {
    // Tenta close_notify antes de free.
    {
        let t = shard_for_handle(handle);
        let mut guard = t.lock().unwrap();
        if let Some(Entry::TlsClient(s)) = guard.get_mut(handle) {
            s.conn.send_close_notify();
            // Best-effort flush; ignora erros.
            let _ = s.conn.complete_io(&mut s.tcp);
        }
    }
    free_handle(handle);
}
