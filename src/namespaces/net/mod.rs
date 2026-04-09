use crate::namespaces::lang::JsValue;

use super::io;
use super::{arg_to_string, arg_to_usize_or_default, DispatchOutcome, NamespaceMember, NamespaceSpec};

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
            let result = match super::state::net_listen(&host, port) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.accept" if !args.is_empty() => {
            let listener_id = args[0].to_number() as u64;
            let result = match super::state::net_accept(listener_id) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.connect" if args.len() >= 2 => {
            let host = arg_to_string(args, 0);
            let port = arg_to_usize_or_default(args, 1, 0) as u16;
            let result = match super::state::net_connect(&host, port) {
                Ok(id) => io::result_ok(JsValue::Number(id as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.read" if !args.is_empty() => {
            let stream_id = args[0].to_number() as u64;
            let max_bytes = arg_to_usize_or_default(args, 1, 4096);
            let result = match super::state::net_read(stream_id, max_bytes) {
                Ok(data) => io::result_ok(JsValue::String(data)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.write" if args.len() >= 2 => {
            let stream_id = args[0].to_number() as u64;
            let data = arg_to_string(args, 1);
            let result = match super::state::net_write(stream_id, &data) {
                Ok(n) => io::result_ok(JsValue::Number(n as f64)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.close" if !args.is_empty() => {
            let handle_id = args[0].to_number() as u64;
            super::state::net_close(handle_id);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "net.set_timeout" if args.len() >= 2 => {
            let stream_id = args[0].to_number() as u64;
            let millis = arg_to_usize_or_default(args, 1, 0) as u64;
            super::state::net_set_timeout(stream_id, millis);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "net.local_addr" if !args.is_empty() => {
            let handle_id = args[0].to_number() as u64;
            let result = match super::state::net_local_addr(handle_id) {
                Ok(addr) => io::result_ok(JsValue::String(addr)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        "net.peer_addr" if !args.is_empty() => {
            let handle_id = args[0].to_number() as u64;
            let result = match super::state::net_peer_addr(handle_id) {
                Ok(addr) => io::result_ok(JsValue::String(addr)),
                Err(e) => io::result_err(&e),
            };
            Some(DispatchOutcome::Value(result))
        }
        _ => None,
    }
}
