use std::collections::BTreeMap;
use std::io::{Read, Write as IoWrite};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

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
    // leak the Arc so we get a 'static guard — safe because the named mutex lives forever
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
// TCP operations
// ---------------------------------------------------------------------------

fn net_listen(host: &str, port: u16) -> Result<u64, String> {
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).map_err(|e| format!("net.listen('{addr}'): {e}"))?;
    Ok(alloc_handle(NetHandle::Listener(listener)))
}

fn net_accept(listener_id: u64) -> Result<u64, String> {
    let listener = {
        let mut state = lock_net();
        match state.handles.remove(&listener_id) {
            Some(NetHandle::Listener(l)) => l,
            Some(other) => {
                state.handles.insert(listener_id, other);
                return Err("net.accept: handle is not a listener".to_string());
            }
            None => return Err("net.accept: invalid listener handle".to_string()),
        }
    };

    let result = listener.accept();

    {
        let mut state = lock_net();
        state.handles.insert(listener_id, NetHandle::Listener(listener));
    }

    match result {
        Ok((stream, _addr)) => Ok(alloc_handle(NetHandle::Stream(stream))),
        Err(e) => Err(format!("net.accept: {e}")),
    }
}

fn net_connect(host: &str, port: u16) -> Result<u64, String> {
    let addr = format!("{host}:{port}");
    let stream = TcpStream::connect(&addr).map_err(|e| format!("net.connect('{addr}'): {e}"))?;
    Ok(alloc_handle(NetHandle::Stream(stream)))
}

fn net_read(stream_id: u64, max_bytes: usize) -> Result<String, String> {
    let mut state = lock_net();
    let handle = state
        .handles
        .get_mut(&stream_id)
        .ok_or_else(|| "net.read: invalid stream handle".to_string())?;

    match handle {
        NetHandle::Stream(stream) => {
            let mut buf = vec![0u8; max_bytes];
            let n = stream.read(&mut buf).map_err(|e| format!("net.read: {e}"))?;
            Ok(String::from_utf8_lossy(&buf[..n]).to_string())
        }
        _ => Err("net.read: handle is not a stream".to_string()),
    }
}

fn net_write(stream_id: u64, data: &str) -> Result<usize, String> {
    let mut state = lock_net();
    let handle = state
        .handles
        .get_mut(&stream_id)
        .ok_or_else(|| "net.write: invalid stream handle".to_string())?;

    match handle {
        NetHandle::Stream(stream) => {
            let n = stream
                .write(data.as_bytes())
                .map_err(|e| format!("net.write: {e}"))?;
            stream.flush().map_err(|e| format!("net.write flush: {e}"))?;
            Ok(n)
        }
        _ => Err("net.write: handle is not a stream".to_string()),
    }
}

fn net_close(handle_id: u64) {
    let mut state = lock_net();
    state.handles.remove(&handle_id);
    // Drop closes the socket automatically
}

fn net_set_timeout(stream_id: u64, millis: u64) {
    let state = lock_net();
    if let Some(NetHandle::Stream(stream)) = state.handles.get(&stream_id) {
        let timeout = if millis == 0 {
            None
        } else {
            Some(Duration::from_millis(millis))
        };
        let _ = stream.set_read_timeout(timeout);
        let _ = stream.set_write_timeout(timeout);
    }
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

fn net_peer_addr(stream_id: u64) -> Result<String, String> {
    let state = lock_net();
    match state.handles.get(&stream_id) {
        Some(NetHandle::Stream(s)) => s
            .peer_addr()
            .map(|a| a.to_string())
            .map_err(|e| format!("net.peer_addr: {e}")),
        _ => Err("net.peer_addr: handle is not a stream".to_string()),
    }
}

// ---------------------------------------------------------------------------
// UDP operations
// ---------------------------------------------------------------------------

fn net_udp_bind(host: &str, port: u16) -> Result<u64, String> {
    let addr = format!("{host}:{port}");
    let socket = UdpSocket::bind(&addr).map_err(|e| format!("net.udp_bind('{addr}'): {e}"))?;
    Ok(alloc_handle(NetHandle::Udp(socket)))
}

fn net_udp_send_to(socket_id: u64, data: &str, host: &str, port: u16) -> Result<usize, String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => {
            let addr = format!("{host}:{port}");
            socket
                .send_to(data.as_bytes(), &addr)
                .map_err(|e| format!("net.udp_send_to: {e}"))
        }
        _ => Err("net.udp_send_to: handle is not a UDP socket".to_string()),
    }
}

fn net_udp_recv_from(socket_id: u64, max_bytes: usize) -> Result<(String, String), String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => {
            let mut buf = vec![0u8; max_bytes];
            let (n, addr) = socket
                .recv_from(&mut buf)
                .map_err(|e| format!("net.udp_recv_from: {e}"))?;
            let data = String::from_utf8_lossy(&buf[..n]).to_string();
            Ok((data, addr.to_string()))
        }
        _ => Err("net.udp_recv_from: handle is not a UDP socket".to_string()),
    }
}

fn net_udp_send(socket_id: u64, data: &str) -> Result<usize, String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => socket
            .send(data.as_bytes())
            .map_err(|e| format!("net.udp_send: {e}")),
        _ => Err("net.udp_send: handle is not a UDP socket".to_string()),
    }
}

fn net_udp_connect(socket_id: u64, host: &str, port: u16) -> Result<(), String> {
    let state = lock_net();
    match state.handles.get(&socket_id) {
        Some(NetHandle::Udp(socket)) => {
            let addr = format!("{host}:{port}");
            socket
                .connect(&addr)
                .map_err(|e| format!("net.udp_connect: {e}"))
        }
        _ => Err("net.udp_connect: handle is not a UDP socket".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Namespace spec + dispatch
// ---------------------------------------------------------------------------

const MEMBERS: &[NamespaceMember] = &[
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
        doc: "Closes a TCP listener or stream handle.",
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
        doc: "Returns the local address of a listener or stream as \"host:port\".",
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
        doc: "Associates the UDP socket with a remote address for use with udp_send/read.",
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

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "net.listen" if args.len() >= 2 => {
            let host = arg_to_string(args, 0);
            let port = arg_to_usize_or_default(args, 1, 0) as u16;
            let result = match net_listen(&host, port) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.accept" if !args.is_empty() => {
            let listener_id = args[0].to_number() as u64;
            let result = match net_accept(listener_id) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.connect" if args.len() >= 2 => {
            let host = arg_to_string(args, 0);
            let port = arg_to_usize_or_default(args, 1, 0) as u16;
            let result = match net_connect(&host, port) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.read" if !args.is_empty() => {
            let stream_id = args[0].to_number() as u64;
            let max_bytes = arg_to_usize_or_default(args, 1, 4096);
            let result = match net_read(stream_id, max_bytes) {
                Ok(data) => io::result_ok(JsValue::String(data)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.write" if args.len() >= 2 => {
            let stream_id = args[0].to_number() as u64;
            let data = arg_to_string(args, 1);
            let result = match net_write(stream_id, &data) {
                Ok(n) => io::result_ok(JsValue::Number(n as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.close" if !args.is_empty() => {
            let handle_id = args[0].to_number() as u64;
            net_close(handle_id);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "net.set_timeout" if args.len() >= 2 => {
            let stream_id = args[0].to_number() as u64;
            let millis = arg_to_usize_or_default(args, 1, 0) as u64;
            net_set_timeout(stream_id, millis);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "net.local_addr" if !args.is_empty() => {
            let handle_id = args[0].to_number() as u64;
            let result = match net_local_addr(handle_id) {
                Ok(addr) => io::result_ok(JsValue::String(addr)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.peer_addr" if !args.is_empty() => {
            let handle_id = args[0].to_number() as u64;
            let result = match net_peer_addr(handle_id) {
                Ok(addr) => io::result_ok(JsValue::String(addr)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        // UDP
        "net.udp_bind" if args.len() >= 2 => {
            let host = arg_to_string(args, 0);
            let port = arg_to_usize_or_default(args, 1, 0) as u16;
            let result = match net_udp_bind(&host, port) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.udp_send_to" if args.len() >= 4 => {
            let socket_id = args[0].to_number() as u64;
            let data = arg_to_string(args, 1);
            let host = arg_to_string(args, 2);
            let port = arg_to_usize_or_default(args, 3, 0) as u16;
            let result = match net_udp_send_to(socket_id, &data, &host, port) {
                Ok(n) => io::result_ok(JsValue::Number(n as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.udp_recv_from" if !args.is_empty() => {
            let socket_id = args[0].to_number() as u64;
            let max_bytes = arg_to_usize_or_default(args, 1, 4096);
            let result = match net_udp_recv_from(socket_id, max_bytes) {
                Ok((data, addr)) => io::result_ok(JsValue::String(format!("{data}\0{addr}"))),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.udp_connect" if args.len() >= 3 => {
            let socket_id = args[0].to_number() as u64;
            let host = arg_to_string(args, 1);
            let port = arg_to_usize_or_default(args, 2, 0) as u16;
            let result = match net_udp_connect(socket_id, &host, port) {
                Ok(()) => io::result_ok(JsValue::Undefined),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.udp_send" if args.len() >= 2 => {
            let socket_id = args[0].to_number() as u64;
            let data = arg_to_string(args, 1);
            let result = match net_udp_send(socket_id, &data) {
                Ok(n) => io::result_ok(JsValue::Number(n as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        _ => None,
    }
}
