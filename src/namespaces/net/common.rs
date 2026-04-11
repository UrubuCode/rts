use std::collections::HashMap;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::value::RuntimeValue;

// ── Net state ────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct NetState {
    pub tcp_listeners: HashMap<u64, TcpListener>,
    pub tcp_streams: HashMap<u64, TcpStream>,
    pub udp_sockets: HashMap<u64, UdpSocket>,
    next_handle: u64,
}

impl NetState {
    pub fn next_handle(&mut self) -> u64 {
        self.next_handle += 1;
        self.next_handle
    }

    pub fn insert_tcp_listener(&mut self, listener: TcpListener) -> u64 {
        let h = self.next_handle();
        self.tcp_listeners.insert(h, listener);
        h
    }

    pub fn insert_tcp_stream(&mut self, stream: TcpStream) -> u64 {
        let h = self.next_handle();
        self.tcp_streams.insert(h, stream);
        h
    }

    pub fn insert_udp_socket(&mut self, socket: UdpSocket) -> u64 {
        let h = self.next_handle();
        self.udp_sockets.insert(h, socket);
        h
    }

    pub fn remove_tcp_listener(&mut self, h: u64) -> bool {
        self.tcp_listeners.remove(&h).is_some()
    }

    pub fn remove_tcp_stream(&mut self, h: u64) -> bool {
        self.tcp_streams.remove(&h).is_some()
    }

    pub fn remove_udp_socket(&mut self, h: u64) -> bool {
        self.udp_sockets.remove(&h).is_some()
    }
}

// ── State accessor ───────────────────────────────────────────────────────────

static NET: OnceLock<Arc<Mutex<NetState>>> = OnceLock::new();

pub fn lock_net_state() -> Arc<Mutex<NetState>> {
    NET.get_or_init(|| Arc::new(Mutex::new(NetState::default())))
        .clone()
}

/// Run a closure with mutable access to net state.
pub fn with_net_state_mut<R>(f: impl FnOnce(&mut NetState) -> R) -> R {
    let arc = lock_net_state();
    let mut guard = arc.lock().unwrap();
    f(&mut *guard)
}

// ── Result helpers ───────────────────────────────────────────────────────────

pub fn result_ok(value: RuntimeValue) -> RuntimeValue {
    RuntimeValue::Object(
        [
            ("ok".to_string(), RuntimeValue::Bool(true)),
            ("value".to_string(), value),
        ]
        .into_iter()
        .collect(),
    )
}

pub fn result_err(error: String) -> RuntimeValue {
    RuntimeValue::Object(
        [
            ("ok".to_string(), RuntimeValue::Bool(false)),
            ("error".to_string(), RuntimeValue::String(error)),
        ]
        .into_iter()
        .collect(),
    )
}
