//! The WebSocket a DevTools frontend attaches through.
//!
//! The handshake and the framing are `crate::ws`'s — the same two modules
//! `import { WebSocketServer } from "ws"` runs on — so this file is only the
//! plumbing between one socket and the program's thread: a reader thread that
//! queues text messages, and a writer the program's thread sends through.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::ws::{frame, handshake};

/// What a socket thread hands the program's thread.
pub(super) enum Incoming {
    /// One text message, by connection.
    Message(u32, String),
    /// A connection ended.
    Closed,
}

static INBOX: Mutex<Vec<Incoming>> = Mutex::new(Vec::new());
static WRITERS: Mutex<Option<HashMap<u32, TcpStream>>> = Mutex::new(None);
static NEXT: AtomicU32 = AtomicU32::new(1);

/// Whether `head` asks for a WebSocket upgrade.
pub(in crate::inspector) fn is_upgrade(head: &[u8]) -> bool {
    String::from_utf8_lossy(head).to_ascii_lowercase().contains("upgrade: websocket")
}

/// Completes the handshake on `stream` and serves it from a thread of its own.
/// `head` is what the accept loop already read.
pub(in crate::inspector) fn attach(mut stream: TcpStream, head: Vec<u8>) {
    let (request, used) = match handshake::read_request(&head) {
        Some(Ok(parsed)) => parsed,
        _ => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };
    if stream.write_all(&handshake::response(&request.key)).is_err() {
        return;
    }
    let Ok(writer) = stream.try_clone() else { return };
    let connection = NEXT.fetch_add(1, Ordering::SeqCst);
    WRITERS
        .lock()
        .unwrap_or_else(|held| held.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(connection, writer);
    let rest = head[used..].to_vec();
    let _ = std::thread::Builder::new()
        .name("rts-inspector-ws".to_owned())
        .spawn(move || read_loop(connection, stream, rest));
}

/// Reads frames until the peer closes, queueing each text message.
fn read_loop(connection: u32, mut stream: TcpStream, mut buffer: Vec<u8>) {
    let mut partial: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    'outer: loop {
        loop {
            match frame::read_frame(&buffer) {
                frame::Read::Got(got, used) => {
                    buffer.drain(..used);
                    match got.opcode {
                        frame::OP_TEXT | frame::OP_CONTINUATION => {
                            partial.extend_from_slice(&got.payload);
                            if got.fin {
                                let text = String::from_utf8_lossy(&partial).into_owned();
                                partial.clear();
                                push(Incoming::Message(connection, text));
                            }
                        }
                        frame::OP_PING => send_frame(connection, frame::OP_PONG, &got.payload),
                        frame::OP_CLOSE => {
                            send_frame(connection, frame::OP_CLOSE, &[]);
                            break 'outer;
                        }
                        _ => {}
                    }
                }
                frame::Read::Incomplete => break,
                frame::Read::Invalid(_) => break 'outer,
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    }
    if let Some(writers) = WRITERS.lock().unwrap_or_else(|held| held.into_inner()).as_mut() {
        writers.remove(&connection);
    }
    push(Incoming::Closed);
}

fn push(incoming: Incoming) {
    INBOX.lock().unwrap_or_else(|held| held.into_inner()).push(incoming);
}

/// Everything queued since the last call.
pub(super) fn drain() -> Vec<Incoming> {
    std::mem::take(&mut *INBOX.lock().unwrap_or_else(|held| held.into_inner()))
}

/// Sends one text message on `connection`. A connection that went away is
/// silently skipped: its `Closed` is already queued.
pub(super) fn send(connection: u32, text: &str) {
    send_frame(connection, frame::OP_TEXT, text.as_bytes());
}

fn send_frame(connection: u32, opcode: u8, payload: &[u8]) {
    let mut writers = WRITERS.lock().unwrap_or_else(|held| held.into_inner());
    if let Some(stream) = writers.as_mut().and_then(|all| all.get_mut(&connection)) {
        // Unmasked: a server must not mask (RFC 6455 §5.1).
        let _ = stream.write_all(&frame::write_frame(opcode, payload, None));
    }
}
