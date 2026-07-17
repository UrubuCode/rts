//! node:dgram — the inbound datagram reader.
//!
//! One dedicated OS thread per bound socket runs a blocking `recv_from` loop.
//! The thread NEVER touches the JS heap: it pushes the payload bytes + the
//! sender's address into the socket's event queue as PLAIN DATA, and the pump
//! turns them into a `Buffer` + `rinfo` on the JS thread.
//!
//! `close()` is observed through a short read timeout: the loop wakes at most
//! `POLL` after the flag flips, so `close()` joins the thread promptly and the
//! OS port is released instead of lingering. UDP sockets are few per process, so
//! thread-per-socket is the right v1 (docs/node-implementation/dgram.md §5.3).

use std::mem::MaybeUninit;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use super::state::{Datagram, SockEvent, SocketState};

/// How long a blocked `recv_from` waits before re-checking the closed flag.
const POLL: Duration = Duration::from_millis(50);

/// The largest datagram IPv4/IPv6 can carry (65535 − the 8-byte UDP header −
/// the 20-byte IP header). A payload never exceeds this, so one buffer of this
/// size never truncates a real datagram.
const MAX_DATAGRAM: usize = 65_507;

/// Start the reader for a freshly bound socket. Idempotent — a socket binds once.
pub fn start(this: u64, st: &Arc<SocketState>) {
    let mut slot = st.reader.lock().unwrap();
    if slot.is_some() {
        return;
    }
    if st.sock.set_read_timeout(Some(POLL)).is_err() {
        // Without a timeout the thread could not observe close(); do not start a
        // reader that would outlive its socket.
        return;
    }
    let state = st.clone();
    let handle = std::thread::Builder::new()
        .name(format!("rts-dgram-{this}"))
        .spawn(move || run(state))
        .ok();
    *slot = handle;
}

/// Signal the reader to exit and join it (bounded by [`POLL`]).
pub fn stop(st: &Arc<SocketState>) {
    st.closed.store(true, Ordering::Release);
    let handle = st.reader.lock().unwrap().take();
    if let Some(h) = handle {
        let _ = h.join();
    }
}

fn run(st: Arc<SocketState>) {
    let mut buf = [MaybeUninit::<u8>::uninit(); MAX_DATAGRAM];
    while !st.closed.load(Ordering::Acquire) {
        match st.sock.recv_from(&mut buf) {
            Ok((n, from)) => {
                let Some(from) = from.as_socket() else { continue };
                // `receiveBlockList`: a datagram from a blocked sender is dropped
                // here — it never reaches a 'message' listener. (Node's own doc:
                // this is not a security boundary behind a proxy/NAT, since the
                // address checked is the immediate sender's.)
                if super::state::blocked(&st.receive_block_list, from.ip()) {
                    continue;
                }
                // Copy out only what actually arrived, so `msg.length ===
                // rinfo.size` holds structurally.
                let bytes = unsafe { slice_assume_init(&buf[..n]) }.to_vec();
                st.push(SockEvent::Message(Datagram { bytes, from }));
            }
            Err(e) if transient(&e) => continue,
            Err(e) => {
                if st.closed.load(Ordering::Acquire) {
                    break;
                }
                let (code, msg) = super::errors::message_for(&e, "recvmsg");
                st.push(SockEvent::Error(code, msg));
                break;
            }
        }
    }
}

/// A read that returned because the timeout elapsed (or was interrupted) — not
/// a socket error, just the loop's chance to re-check the closed flag.
fn transient(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(e.kind(), WouldBlock | TimedOut | Interrupted)
}

/// The first `n` bytes have been written by `recv_from`, so they are initialized.
unsafe fn slice_assume_init(slice: &[MaybeUninit<u8>]) -> &[u8] {
    unsafe { &*(slice as *const [MaybeUninit<u8>] as *const [u8]) }
}
