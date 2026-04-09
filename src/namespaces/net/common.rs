use std::collections::HashMap;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::lang::JsValue;
use crate::namespaces::state;

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
        let handle = self.next_handle();
        self.tcp_listeners.insert(handle, listener);
        handle
    }

    pub fn insert_tcp_stream(&mut self, stream: TcpStream) -> u64 {
        let handle = self.next_handle();
        self.tcp_streams.insert(handle, stream);
        handle
    }

    pub fn insert_udp_socket(&mut self, socket: UdpSocket) -> u64 {
        let handle = self.next_handle();
        self.udp_sockets.insert(handle, socket);
        handle
    }

    pub fn remove_tcp_listener(&mut self, handle: u64) -> bool {
        self.tcp_listeners.remove(&handle).is_some()
    }

    pub fn remove_tcp_stream(&mut self, handle: u64) -> bool {
        self.tcp_streams.remove(&handle).is_some()
    }

    pub fn remove_udp_socket(&mut self, handle: u64) -> bool {
        self.udp_sockets.remove(&handle).is_some()
    }
}

static NET_STATE_STORAGE: OnceLock<Arc<Mutex<NetState>>> = OnceLock::new();

fn net_state() -> &'static Arc<Mutex<NetState>> {
    NET_STATE_STORAGE.get_or_init(|| {
        state::Mutex.get_or_init("net", Mutex::new(NetState::default()))
    })
}

pub fn lock_net_state() -> std::sync::MutexGuard<'static, NetState> {
    match net_state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

// Helper functions for io.Result
pub fn result_ok(value: JsValue) -> JsValue {
    JsValue::Object([
        ("ok".to_string(), JsValue::Bool(true)),
        ("value".to_string(), value),
    ].into_iter().collect())
}

pub fn result_err(error: String) -> JsValue {
    JsValue::Object([
        ("ok".to_string(), JsValue::Bool(false)),
        ("error".to_string(), JsValue::String(error)),
    ].into_iter().collect())
}