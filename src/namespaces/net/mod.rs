mod tcp;
mod udp;

use std::collections::BTreeMap;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;

use crate::namespaces::lang::JsValue;
use crate::namespaces::state::{self, Mutex};

use super::io;
use super::{arg_to_string, arg_to_usize_or_default, DispatchOutcome, NamespaceMember, NamespaceSpec};

// ---------------------------------------------------------------------------
// Handle storage (registered in the central state)
// ---------------------------------------------------------------------------

enum NetHandle {
    Listener(TcpListener),
    Stream(TcpStream),
    Udp(UdpSocket),
}

struct NetState {
    handles: BTreeMap<u64, NetHandle>,
    next_id: u64,
}

impl Default for NetState {
    fn default() -> Self {
        Self {
            handles: BTreeMap::new(),
            next_id: 0,
        }
    }
}

fn lock_net() -> std::sync::MutexGuard<'static, NetState> {
    let state = Mutex.get_or_init("net", std::sync::Mutex::new(NetState::default()));
    let leaked: &'static std::sync::Mutex<NetState> = unsafe { &*Arc::as_ptr(&state) };
    state::lock_or_recover(leaked)
}

fn alloc_handle(handle: NetHandle) -> u64 {
    let mut state = lock_net();
    state.next_id = state.next_id.saturating_add(1);
    let id = state.next_id;
    state.handles.insert(id, handle);
    id
}

// ---------------------------------------------------------------------------
// Shared operations (work on any handle type)
// ---------------------------------------------------------------------------

fn net_close(handle_id: u64) {
    let mut state = lock_net();
    state.handles.remove(&handle_id);
}

fn net_local_addr(handle_id: u64) -> Result<String, String> {
    let state = lock_net();
    match state.handles.get(&handle_id) {
        Some(NetHandle::Listener(l)) => l
            .local_addr()
            .map(|a| a.to_string())
            .map_err(|e| format!("net.local_addr: {e}")),
        Some(NetHandle::Stream(s)) => s
            .local_addr()
            .map(|a| a.to_string())
            .map_err(|e| format!("net.local_addr: {e}")),
        Some(NetHandle::Udp(u)) => u
            .local_addr()
            .map(|a| a.to_string())
            .map_err(|e| format!("net.local_addr: {e}")),
        None => Err("net.local_addr: invalid handle".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Namespace spec
// ---------------------------------------------------------------------------

const MEMBERS: &[NamespaceMember] = &[
    // TCP
    NamespaceMember {
        name: "listen",
        callee: "net.listen",
        doc: "Creates a TCP listener bound to the given host and port.",
        ts_signature: "listen(host: str, port: u16): io.Result<u64>",
    },
    NamespaceMember {
        name: "accept",
        callee: "net.accept",
        doc: "Accepts the next incoming TCP connection on a listener. Blocks until a client connects.",
        ts_signature: "accept(listener: u64): io.Result<u64>",
    },
    NamespaceMember {
        name: "connect",
        callee: "net.connect",
        doc: "Opens a TCP connection to the given host and port.",
        ts_signature: "connect(host: str, port: u16): io.Result<u64>",
    },
    NamespaceMember {
        name: "read",
        callee: "net.read",
        doc: "Reads up to maxBytes from a TCP stream. Returns the data as a UTF-8 string.",
        ts_signature: "read(stream: u64, maxBytes?: usize): io.Result<str>",
    },
    NamespaceMember {
        name: "write",
        callee: "net.write",
        doc: "Writes data to a TCP stream. Returns the number of bytes written.",
        ts_signature: "write(stream: u64, data: str): io.Result<usize>",
    },
    NamespaceMember {
        name: "close",
        callee: "net.close",
        doc: "Closes a TCP listener, stream, or UDP socket handle.",
        ts_signature: "close(handle: u64): void",
    },
    NamespaceMember {
        name: "set_timeout",
        callee: "net.set_timeout",
        doc: "Sets the read/write timeout in milliseconds for a TCP stream. Pass 0 to disable.",
        ts_signature: "set_timeout(stream: u64, millis: u64): void",
    },
    NamespaceMember {
        name: "local_addr",
        callee: "net.local_addr",
        doc: "Returns the local address of a listener, stream, or UDP socket as \"host:port\".",
        ts_signature: "local_addr(handle: u64): io.Result<str>",
    },
    NamespaceMember {
        name: "peer_addr",
        callee: "net.peer_addr",
        doc: "Returns the remote address of a TCP stream as \"host:port\".",
        ts_signature: "peer_addr(stream: u64): io.Result<str>",
    },
    // UDP
    NamespaceMember {
        name: "udp_bind",
        callee: "net.udp_bind",
        doc: "Creates a UDP socket bound to the given host and port.",
        ts_signature: "udp_bind(host: str, port: u16): io.Result<u64>",
    },
    NamespaceMember {
        name: "udp_send_to",
        callee: "net.udp_send_to",
        doc: "Sends data to a specific address via UDP. Returns bytes sent.",
        ts_signature: "udp_send_to(socket: u64, data: str, host: str, port: u16): io.Result<usize>",
    },
    NamespaceMember {
        name: "udp_recv_from",
        callee: "net.udp_recv_from",
        doc: "Receives data from UDP socket. Returns \"data\\0sender_addr\" (null-separated).",
        ts_signature: "udp_recv_from(socket: u64, maxBytes?: usize): io.Result<str>",
    },
    NamespaceMember {
        name: "udp_connect",
        callee: "net.udp_connect",
        doc: "Associates the UDP socket with a remote address for use with udp_send.",
        ts_signature: "udp_connect(socket: u64, host: str, port: u16): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_send",
        callee: "net.udp_send",
        doc: "Sends data on a connected UDP socket. Returns bytes sent.",
        ts_signature: "udp_send(socket: u64, data: str): io.Result<usize>",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "net",
    doc: "TCP/UDP networking primitives backed by std::net.",
    members: MEMBERS,
    ts_prelude: &[],
};

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        // TCP
        "net.listen" if args.len() >= 2 => {
            let host = arg_to_string(args, 0);
            let port = arg_to_usize_or_default(args, 1, 0) as u16;
            Some(dispatch_ok_num(tcp::listen(&host, port)))
        }
        "net.accept" if !args.is_empty() => {
            Some(dispatch_ok_num(tcp::accept(args[0].to_number() as u64)))
        }
        "net.connect" if args.len() >= 2 => {
            let host = arg_to_string(args, 0);
            let port = arg_to_usize_or_default(args, 1, 0) as u16;
            Some(dispatch_ok_num(tcp::connect(&host, port)))
        }
        "net.read" if !args.is_empty() => {
            let id = args[0].to_number() as u64;
            let max = arg_to_usize_or_default(args, 1, 4096);
            Some(dispatch_result_str(tcp::read(id, max)))
        }
        "net.write" if args.len() >= 2 => {
            let id = args[0].to_number() as u64;
            let data = arg_to_string(args, 1);
            Some(dispatch_ok_usize(tcp::write(id, &data)))
        }
        "net.set_timeout" if args.len() >= 2 => {
            tcp::set_timeout(
                args[0].to_number() as u64,
                arg_to_usize_or_default(args, 1, 0) as u64,
            );
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "net.peer_addr" if !args.is_empty() => {
            Some(dispatch_result_str(tcp::peer_addr(args[0].to_number() as u64)))
        }
        // Shared
        "net.close" if !args.is_empty() => {
            net_close(args[0].to_number() as u64);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "net.local_addr" if !args.is_empty() => {
            Some(dispatch_result_str(net_local_addr(args[0].to_number() as u64)))
        }
        // UDP
        "net.udp_bind" if args.len() >= 2 => {
            let host = arg_to_string(args, 0);
            let port = arg_to_usize_or_default(args, 1, 0) as u16;
            Some(dispatch_ok_num(udp::bind(&host, port)))
        }
        "net.udp_send_to" if args.len() >= 4 => {
            let id = args[0].to_number() as u64;
            let data = arg_to_string(args, 1);
            let host = arg_to_string(args, 2);
            let port = arg_to_usize_or_default(args, 3, 0) as u16;
            Some(dispatch_ok_usize(udp::send_to(id, &data, &host, port)))
        }
        "net.udp_recv_from" if !args.is_empty() => {
            let id = args[0].to_number() as u64;
            let max = arg_to_usize_or_default(args, 1, 4096);
            let result = match udp::recv_from(id, max) {
                Ok((data, addr)) => io::result_ok(JsValue::String(format!("{data}\0{addr}"))),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.udp_connect" if args.len() >= 3 => {
            let id = args[0].to_number() as u64;
            let host = arg_to_string(args, 1);
            let port = arg_to_usize_or_default(args, 2, 0) as u16;
            let result = match udp::connect(id, &host, port) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.udp_send" if args.len() >= 2 => {
            let id = args[0].to_number() as u64;
            let data = arg_to_string(args, 1);
            Some(dispatch_ok_usize(udp::send(id, &data)))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Dispatch helpers
// ---------------------------------------------------------------------------

fn dispatch_ok_num(result: Result<u64, String>) -> DispatchOutcome {
    let value = match result {
        Ok(n) => io::result_ok(JsValue::Number(n as f64)),
        Err(e) => io::result_err(&e),
    };
    DispatchOutcome::Value(value)
}

fn dispatch_ok_usize(result: Result<usize, String>) -> DispatchOutcome {
    let value = match result {
        Ok(n) => io::result_ok(JsValue::Number(n as f64)),
        Err(e) => io::result_err(&e),
    };
    DispatchOutcome::Value(value)
}

fn dispatch_result_str(result: Result<String, String>) -> DispatchOutcome {
    let value = match result {
        Ok(s) => io::result_ok(JsValue::String(s)),
        Err(e) => io::result_err(&e),
    };
    DispatchOutcome::Value(value)
}
