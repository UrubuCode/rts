//! Driving one `rustls::Connection` (client or server) over byte buffers —
//! no `tokio-rustls`: this crate has no async runtime dependency (`net`'s
//! own module doc states the same for plain TCP), so the record layer is
//! driven synchronously against whatever bytes a `'data'` listener handed
//! over, per this module's own "WHEN" section in `mod.rs`.
//!
//! `ClientConnection` and `ServerConnection` each `Deref` to their OWN
//! `ConnectionCommon<_>` (different `Data` type parameters), so this cannot
//! be one generic function over a shared reference the way a first draft
//! wanted — every method below matches on [`Side`] once instead.

use std::io::{Cursor, Read, Write};

use rustls::{ClientConnection, ServerConnection};

pub(super) enum Side {
    Client(ClientConnection),
    Server(ServerConnection),
}

pub(super) struct Driver {
    pub(super) side: Side,
}

/// What one `feed` produced.
pub(super) struct Fed {
    pub(super) plaintext: Vec<u8>,
    /// `true` once, the call the handshake completes on.
    pub(super) just_connected: bool,
    pub(super) closed: bool,
}

impl Driver {
    fn is_handshaking(&self) -> bool {
        match &self.side {
            Side::Client(c) => c.is_handshaking(),
            Side::Server(s) => s.is_handshaking(),
        }
    }

    /// Hands `ciphertext` (bytes off the wire) to the connection, and
    /// answers whatever plaintext that unlocked plus handshake-completion/
    /// close state — never calls into JS itself; the caller (`socket.rs`'s
    /// `'data'` listener) does that, per this crate's nested-borrow rule.
    pub(super) fn feed(&mut self, ciphertext: &[u8]) -> Fed {
        let was_handshaking = self.is_handshaking();
        let mut cursor = Cursor::new(ciphertext);
        let closed = match &mut self.side {
            Side::Client(c) => {
                while matches!(c.read_tls(&mut cursor), Ok(n) if n > 0) {}
                c.process_new_packets().is_err()
            }
            Side::Server(s) => {
                while matches!(s.read_tls(&mut cursor), Ok(n) if n > 0) {}
                s.process_new_packets().is_err()
            }
        };
        let mut plaintext = Vec::new();
        match &mut self.side {
            Side::Client(c) => {
                let _ = c.reader().read_to_end(&mut plaintext);
            }
            Side::Server(s) => {
                let _ = s.reader().read_to_end(&mut plaintext);
            }
        }
        let just_connected = was_handshaking && !self.is_handshaking();
        Fed { plaintext, just_connected, closed }
    }

    /// Queues plaintext to send; call [`Driver::outgoing`] to get the bytes
    /// this produces to actually write to the underlying socket.
    pub(super) fn send(&mut self, plaintext: &[u8]) {
        match &mut self.side {
            Side::Client(c) => {
                let _ = c.writer().write_all(plaintext);
            }
            Side::Server(s) => {
                let _ = s.writer().write_all(plaintext);
            }
        }
    }

    /// Every ciphertext byte the connection wants written right now —
    /// handshake flights and/or the result of [`Driver::send`].
    pub(super) fn outgoing(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        match &mut self.side {
            Side::Client(c) => {
                while c.wants_write() {
                    match c.write_tls(&mut out) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            }
            Side::Server(s) => {
                while s.wants_write() {
                    match s.write_tls(&mut out) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                }
            }
        }
        out
    }
}
