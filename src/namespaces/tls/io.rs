//! TLS send/recv — encrypt/decrypt via rustls::Stream.

use std::io::{Read, Write};

use rustls::Stream;

use super::super::gc::handles::{Entry, shard_for_handle};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_SEND(
    stream: u64,
    data_ptr: *const u8,
    data_len: i64,
) -> i64 {
    if data_len < 0 || data_ptr.is_null() {
        return -1;
    }
    // SAFETY: caller contract.
    let payload = unsafe { std::slice::from_raw_parts(data_ptr, data_len as usize) };

    let t = shard_for_handle(stream);
    let mut guard = t.lock().unwrap();
    let Some(Entry::TlsClient(s)) = guard.get_mut(stream) else {
        return -1;
    };
    // Stream::new faz handshake lazy + encrypt + write.
    let mut tls = Stream::new(&mut s.conn, &mut s.tcp);
    match tls.write(payload) {
        Ok(n) => n as i64,
        Err(_) => -1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TLS_RECV(stream: u64, buf_ptr: u64, len: i64) -> i64 {
    if len < 0 || buf_ptr == 0 {
        return -1;
    }

    // Faz a leitura num buffer temporario, copia depois — evita
    // segurar a shard durante I/O bloqueante mais do que necessario.
    // Como TLS precisa do estado da conexao (que vive na shard),
    // mantemos o lock pela duracao toda mesmo. Aceitavel pra MVP.
    let t = shard_for_handle(stream);
    let mut guard = t.lock().unwrap();
    let Some(Entry::TlsClient(s)) = guard.get_mut(stream) else {
        return -1;
    };
    // SAFETY: caller passou ponteiro raw valido.
    let dst = unsafe { std::slice::from_raw_parts_mut(buf_ptr as *mut u8, len as usize) };
    let mut tls = Stream::new(&mut s.conn, &mut s.tcp);
    match tls.read(dst) {
        Ok(n) => n as i64,
        Err(e) if (&e as &std::io::Error).kind() == std::io::ErrorKind::UnexpectedEof => 0,
        Err(_) => -1,
    }
}
