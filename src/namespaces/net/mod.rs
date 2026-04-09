use std::collections::BTreeMap;
use std::io::{Read, Write as IoWrite};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::namespaces::lang::JsValue;

use super::io;
use super::{arg_to_string, arg_to_usize_or_default, DispatchOutcome, NamespaceMember, NamespaceSpec};

// ---------------------------------------------------------------------------
// Handle storage (private to this module)
// ---------------------------------------------------------------------------

enum NetHandle {
    Listener(TcpListener),
    Stream(TcpStream),
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

static NET_STATE: OnceLock<Mutex<NetState>> = OnceLock::new();

fn lock_net() -> std::sync::MutexGuard<'static, NetState> {
    let state = NET_STATE.get_or_init(|| Mutex::new(NetState::default()));
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
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
    // Take the listener out briefly to avoid holding the lock during accept()
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

    // Put the listener back
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
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "net",
    doc: "TCP networking primitives backed by std::net.",
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
        _ => None,
    }
}
