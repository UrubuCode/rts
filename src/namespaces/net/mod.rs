//! `net` namespace — sockets TCP/UDP, HTTP client, helpers IP.
//!
//! Cada protocolo em arquivo proprio (`tcp.rs`, `udp.rs`, `http.rs`, `ip.rs`);
//! `common.rs` agrupa helpers compartilhados. Sockets retornam handle `u64`.

pub mod common;
mod http;
mod ip;
mod tcp;
mod udp;

use crate::namespaces::value::RuntimeValue;
use crate::namespaces::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_u64};

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "net",
    doc: "Network utilities backed by std::net with TCP, UDP and IP address support.",
    members: MEMBERS,
    ts_prelude: TS_PRELUDE,
};

const MEMBERS: &[NamespaceMember] = &[
    // TcpListener
    NamespaceMember {
        name: "tcp_listen",
        callee: "net.tcp_listen",
        doc: "Creates a TCP listener bound to the specified address.",
        ts_signature: "tcp_listen(addr: str): io.Result<u64>",
    },
    NamespaceMember {
        name: "tcp_accept",
        callee: "net.tcp_accept",
        doc: "Accepts a new TCP connection on this listener.",
        ts_signature: "tcp_accept(listener: u64): io.Result<TcpConnection>",
    },
    NamespaceMember {
        name: "tcp_local_addr",
        callee: "net.tcp_local_addr",
        doc: "Returns the local socket address of this listener.",
        ts_signature: "tcp_local_addr(listener: u64): io.Result<str>",
    },
    // TcpStream
    NamespaceMember {
        name: "tcp_connect",
        callee: "net.tcp_connect",
        doc: "Opens a TCP connection to a remote host.",
        ts_signature: "tcp_connect(addr: str): io.Result<u64>",
    },
    NamespaceMember {
        name: "tcp_read",
        callee: "net.tcp_read",
        doc: "Reads data from a TCP stream.",
        ts_signature: "tcp_read(stream: u64, max_bytes?: usize): io.Result<str>",
    },
    NamespaceMember {
        name: "tcp_write",
        callee: "net.tcp_write",
        doc: "Writes data to a TCP stream.",
        ts_signature: "tcp_write(stream: u64, data: str): io.Result<usize>",
    },
    NamespaceMember {
        name: "tcp_flush",
        callee: "net.tcp_flush",
        doc: "Flushes the TCP stream output buffer.",
        ts_signature: "tcp_flush(stream: u64): io.Result<void>",
    },
    NamespaceMember {
        name: "tcp_shutdown",
        callee: "net.tcp_shutdown",
        doc: "Shuts down the read, write, or both halves of this connection.",
        ts_signature: "tcp_shutdown(stream: u64, how: ShutdownHow): io.Result<void>",
    },
    NamespaceMember {
        name: "tcp_peer_addr",
        callee: "net.tcp_peer_addr",
        doc: "Returns the socket address of the remote peer.",
        ts_signature: "tcp_peer_addr(stream: u64): io.Result<str>",
    },
    NamespaceMember {
        name: "tcp_set_read_timeout",
        callee: "net.tcp_set_read_timeout",
        doc: "Sets the read timeout for TCP operations.",
        ts_signature: "tcp_set_read_timeout(stream: u64, timeout_ms?: u64): io.Result<void>",
    },
    NamespaceMember {
        name: "tcp_set_write_timeout",
        callee: "net.tcp_set_write_timeout",
        doc: "Sets the write timeout for TCP operations.",
        ts_signature: "tcp_set_write_timeout(stream: u64, timeout_ms?: u64): io.Result<void>",
    },
    NamespaceMember {
        name: "tcp_set_nodelay",
        callee: "net.tcp_set_nodelay",
        doc: "Sets the value of the TCP_NODELAY option on this socket.",
        ts_signature: "tcp_set_nodelay(stream: u64, nodelay: bool): io.Result<void>",
    },
    NamespaceMember {
        name: "tcp_nodelay",
        callee: "net.tcp_nodelay",
        doc: "Gets the value of the TCP_NODELAY option on this socket.",
        ts_signature: "tcp_nodelay(stream: u64): io.Result<bool>",
    },
    NamespaceMember {
        name: "tcp_set_ttl",
        callee: "net.tcp_set_ttl",
        doc: "Sets the value for the IP_TTL option on this socket.",
        ts_signature: "tcp_set_ttl(stream: u64, ttl: u32): io.Result<void>",
    },
    NamespaceMember {
        name: "tcp_ttl",
        callee: "net.tcp_ttl",
        doc: "Gets the value of the IP_TTL option for this socket.",
        ts_signature: "tcp_ttl(stream: u64): io.Result<u32>",
    },
    // UdpSocket
    NamespaceMember {
        name: "udp_bind",
        callee: "net.udp_bind",
        doc: "Creates a UDP socket bound to the specified address.",
        ts_signature: "udp_bind(addr: str): io.Result<u64>",
    },
    NamespaceMember {
        name: "udp_connect",
        callee: "net.udp_connect",
        doc: "Connects this UDP socket to a remote address.",
        ts_signature: "udp_connect(socket: u64, addr: str): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_send",
        callee: "net.udp_send",
        doc: "Sends data on the socket to the connected address.",
        ts_signature: "udp_send(socket: u64, data: str): io.Result<usize>",
    },
    NamespaceMember {
        name: "udp_recv",
        callee: "net.udp_recv",
        doc: "Receives data from the socket.",
        ts_signature: "udp_recv(socket: u64, max_bytes?: usize): io.Result<str>",
    },
    NamespaceMember {
        name: "udp_send_to",
        callee: "net.udp_send_to",
        doc: "Sends data on the socket to the given address.",
        ts_signature: "udp_send_to(socket: u64, data: str, addr: str): io.Result<usize>",
    },
    NamespaceMember {
        name: "udp_recv_from",
        callee: "net.udp_recv_from",
        doc: "Receives data from the socket.",
        ts_signature: "udp_recv_from(socket: u64, max_bytes?: usize): io.Result<UdpMessage>",
    },
    NamespaceMember {
        name: "udp_local_addr",
        callee: "net.udp_local_addr",
        doc: "Returns the socket address that this socket was created from.",
        ts_signature: "udp_local_addr(socket: u64): io.Result<str>",
    },
    NamespaceMember {
        name: "udp_peer_addr",
        callee: "net.udp_peer_addr",
        doc: "Returns the socket address of the remote peer this socket was connected to.",
        ts_signature: "udp_peer_addr(socket: u64): io.Result<str>",
    },
    NamespaceMember {
        name: "udp_set_read_timeout",
        callee: "net.udp_set_read_timeout",
        doc: "Sets the read timeout for UDP operations.",
        ts_signature: "udp_set_read_timeout(socket: u64, timeout_ms?: u64): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_set_write_timeout",
        callee: "net.udp_set_write_timeout",
        doc: "Sets the write timeout for UDP operations.",
        ts_signature: "udp_set_write_timeout(socket: u64, timeout_ms?: u64): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_set_broadcast",
        callee: "net.udp_set_broadcast",
        doc: "Sets the value of the SO_BROADCAST option for this socket.",
        ts_signature: "udp_set_broadcast(socket: u64, broadcast: bool): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_broadcast",
        callee: "net.udp_broadcast",
        doc: "Gets the value of the SO_BROADCAST option for this socket.",
        ts_signature: "udp_broadcast(socket: u64): io.Result<bool>",
    },
    NamespaceMember {
        name: "udp_set_multicast_loop_v4",
        callee: "net.udp_set_multicast_loop_v4",
        doc: "Sets the value of the IP_MULTICAST_LOOP option for this socket.",
        ts_signature: "udp_set_multicast_loop_v4(socket: u64, multicast_loop_v4: bool): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_multicast_loop_v4",
        callee: "net.udp_multicast_loop_v4",
        doc: "Gets the value of the IP_MULTICAST_LOOP option for this socket.",
        ts_signature: "udp_multicast_loop_v4(socket: u64): io.Result<bool>",
    },
    NamespaceMember {
        name: "udp_set_multicast_ttl_v4",
        callee: "net.udp_set_multicast_ttl_v4",
        doc: "Sets the value of the IP_MULTICAST_TTL option for this socket.",
        ts_signature: "udp_set_multicast_ttl_v4(socket: u64, multicast_ttl_v4: u32): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_multicast_ttl_v4",
        callee: "net.udp_multicast_ttl_v4",
        doc: "Gets the value of the IP_MULTICAST_TTL option for this socket.",
        ts_signature: "udp_multicast_ttl_v4(socket: u64): io.Result<u32>",
    },
    NamespaceMember {
        name: "udp_set_ttl",
        callee: "net.udp_set_ttl",
        doc: "Sets the value for the IP_TTL option on this socket.",
        ts_signature: "udp_set_ttl(socket: u64, ttl: u32): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_ttl",
        callee: "net.udp_ttl",
        doc: "Gets the value of the IP_TTL option for this socket.",
        ts_signature: "udp_ttl(socket: u64): io.Result<u32>",
    },
    NamespaceMember {
        name: "udp_join_multicast_v4",
        callee: "net.udp_join_multicast_v4",
        doc: "Executes an operation to join a multicast group.",
        ts_signature: "udp_join_multicast_v4(socket: u64, multiaddr: str, interface: str): io.Result<void>",
    },
    NamespaceMember {
        name: "udp_leave_multicast_v4",
        callee: "net.udp_leave_multicast_v4",
        doc: "Executes an operation to leave a multicast group.",
        ts_signature: "udp_leave_multicast_v4(socket: u64, multiaddr: str, interface: str): io.Result<void>",
    },
    // HTTP/1.1 server primitives (sobre tcp_listen/tcp_accept)
    NamespaceMember {
        name: "http_read_request",
        callee: "net.http_read_request",
        doc: "Reads a complete HTTP/1.1 request from a TCP stream and returns a handle.",
        ts_signature: "http_read_request(stream: u64): io.Result<u64>",
    },
    NamespaceMember {
        name: "http_request_method",
        callee: "net.http_request_method",
        doc: "Returns the HTTP method (GET, POST, ...) of a parsed request.",
        ts_signature: "http_request_method(request: u64): io.Result<str>",
    },
    NamespaceMember {
        name: "http_request_path",
        callee: "net.http_request_path",
        doc: "Returns the request path (with query string) of a parsed request.",
        ts_signature: "http_request_path(request: u64): io.Result<str>",
    },
    NamespaceMember {
        name: "http_request_header",
        callee: "net.http_request_header",
        doc: "Returns the value of a header by case-insensitive name. Empty string if absent.",
        ts_signature: "http_request_header(request: u64, name: str): io.Result<str>",
    },
    NamespaceMember {
        name: "http_request_body",
        callee: "net.http_request_body",
        doc: "Returns the body of a parsed request as a UTF-8 string.",
        ts_signature: "http_request_body(request: u64): io.Result<str>",
    },
    NamespaceMember {
        name: "http_request_free",
        callee: "net.http_request_free",
        doc: "Releases the memory for a parsed request handle.",
        ts_signature: "http_request_free(request: u64): io.Result<bool>",
    },
    NamespaceMember {
        name: "http_response_write",
        callee: "net.http_response_write",
        doc: "Writes a simple HTTP/1.1 response to a stream with status, body and optional content-type.",
        ts_signature: "http_response_write(stream: u64, status: u32, body: str, content_type?: str): io.Result<usize>",
    },
    // IP Address utilities
    NamespaceMember {
        name: "parse_ip_addr",
        callee: "net.parse_ip_addr",
        doc: "Parses a string as an IP address.",
        ts_signature: "parse_ip_addr(addr: str): io.Result<IpAddr>",
    },
    NamespaceMember {
        name: "parse_ipv4_addr",
        callee: "net.parse_ipv4_addr",
        doc: "Parses a string as an IPv4 address.",
        ts_signature: "parse_ipv4_addr(addr: str): io.Result<Ipv4Addr>",
    },
    NamespaceMember {
        name: "parse_ipv6_addr",
        callee: "net.parse_ipv6_addr",
        doc: "Parses a string as an IPv6 address.",
        ts_signature: "parse_ipv6_addr(addr: str): io.Result<Ipv6Addr>",
    },
    NamespaceMember {
        name: "parse_socket_addr",
        callee: "net.parse_socket_addr",
        doc: "Parses a string as a socket address.",
        ts_signature: "parse_socket_addr(addr: str): io.Result<SocketAddr>",
    },
    NamespaceMember {
        name: "to_socket_addrs",
        callee: "net.to_socket_addrs",
        doc: "Resolves a string to socket addresses.",
        ts_signature: "to_socket_addrs(addr: str): io.Result<str>",
    },
    // Utilities
    NamespaceMember {
        name: "close",
        callee: "net.close",
        doc: "Closes a network resource handle.",
        ts_signature: "close(handle: u64): bool",
    },
];

const TS_PRELUDE: &[&str] = &[
    r#"export interface TcpConnection {
      stream: u64;
      peer_addr: str;
    }"#,
    r#"export interface UdpMessage {
      data: str;
      addr: str;
    }"#,
    r#"export interface IpAddr {
      version: "v4" | "v6";
      addr: str;
      is_loopback: bool;
      is_multicast: bool;
      is_unspecified: bool;
    }"#,
    r#"export interface Ipv4Addr {
      octets: str;
      addr: str;
      is_loopback: bool;
      is_multicast: bool;
      is_broadcast: bool;
      is_private: bool;
      is_link_local: bool;
    }"#,
    r#"export interface Ipv6Addr {
      segments: str;
      addr: str;
      is_loopback: bool;
      is_multicast: bool;
      is_unspecified: bool;
    }"#,
    r#"export interface SocketAddr {
      ip: str;
      port: u16;
      addr: str;
    }"#,
    r#"export type ShutdownHow = "Read" | "Write" | "Both";"#,
];

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        // TcpListener
        "net.tcp_listen" => Some(tcp::tcp_listen(args)),
        "net.tcp_accept" => Some(tcp::tcp_accept(args)),
        "net.tcp_local_addr" => Some(tcp::tcp_local_addr(args)),

        // TcpStream
        "net.tcp_connect" => Some(tcp::tcp_connect(args)),
        "net.tcp_read" => Some(tcp::tcp_read(args)),
        "net.tcp_write" => Some(tcp::tcp_write(args)),
        "net.tcp_flush" => Some(tcp::tcp_flush(args)),
        "net.tcp_shutdown" => Some(tcp::tcp_shutdown(args)),
        "net.tcp_peer_addr" => Some(tcp::tcp_peer_addr(args)),
        "net.tcp_set_read_timeout" => Some(tcp::tcp_set_read_timeout(args)),
        "net.tcp_set_write_timeout" => Some(tcp::tcp_set_write_timeout(args)),
        "net.tcp_set_nodelay" => Some(tcp::tcp_set_nodelay(args)),
        "net.tcp_nodelay" => Some(tcp::tcp_nodelay(args)),
        "net.tcp_set_ttl" => Some(tcp::tcp_set_ttl(args)),
        "net.tcp_ttl" => Some(tcp::tcp_ttl(args)),

        // UdpSocket
        "net.udp_bind" => Some(udp::udp_bind(args)),
        "net.udp_connect" => Some(udp::udp_connect(args)),
        "net.udp_send" => Some(udp::udp_send(args)),
        "net.udp_recv" => Some(udp::udp_recv(args)),
        "net.udp_send_to" => Some(udp::udp_send_to(args)),
        "net.udp_recv_from" => Some(udp::udp_recv_from(args)),
        "net.udp_local_addr" => Some(udp::udp_local_addr(args)),
        "net.udp_peer_addr" => Some(udp::udp_peer_addr(args)),
        "net.udp_set_read_timeout" => Some(udp::udp_set_read_timeout(args)),
        "net.udp_set_write_timeout" => Some(udp::udp_set_write_timeout(args)),
        "net.udp_set_broadcast" => Some(udp::udp_set_broadcast(args)),
        "net.udp_broadcast" => Some(udp::udp_broadcast(args)),
        "net.udp_set_multicast_loop_v4" => Some(udp::udp_set_multicast_loop_v4(args)),
        "net.udp_multicast_loop_v4" => Some(udp::udp_multicast_loop_v4(args)),
        "net.udp_set_multicast_ttl_v4" => Some(udp::udp_set_multicast_ttl_v4(args)),
        "net.udp_multicast_ttl_v4" => Some(udp::udp_multicast_ttl_v4(args)),
        "net.udp_set_ttl" => Some(udp::udp_set_ttl(args)),
        "net.udp_ttl" => Some(udp::udp_ttl(args)),
        "net.udp_join_multicast_v4" => Some(udp::udp_join_multicast_v4(args)),
        "net.udp_leave_multicast_v4" => Some(udp::udp_leave_multicast_v4(args)),

        // HTTP/1.1 server primitives
        "net.http_read_request" => Some(http::http_read_request(args)),
        "net.http_request_method" => Some(http::http_request_method(args)),
        "net.http_request_path" => Some(http::http_request_path(args)),
        "net.http_request_header" => Some(http::http_request_header(args)),
        "net.http_request_body" => Some(http::http_request_body(args)),
        "net.http_request_free" => Some(http::http_request_free(args)),
        "net.http_response_write" => Some(http::http_response_write(args)),

        // IP Address utilities
        "net.parse_ip_addr" => Some(ip::parse_ip_addr(args)),
        "net.parse_ipv4_addr" => Some(ip::parse_ipv4_addr(args)),
        "net.parse_ipv6_addr" => Some(ip::parse_ipv6_addr(args)),
        "net.parse_socket_addr" => Some(ip::parse_socket_addr(args)),
        "net.to_socket_addrs" => Some(ip::to_socket_addrs(args)),

        // Utilities
        "net.close" => Some(close(args)),

        _ => None,
    }
}

// Utilities
fn close(args: &[RuntimeValue]) -> DispatchOutcome {
    let handle = arg_to_u64(args, 0);
    let net_state = common::lock_net_state();
    let mut state = net_state.lock().unwrap();

    let closed = state.remove_tcp_listener(handle)
        || state.remove_tcp_stream(handle)
        || state.remove_udp_socket(handle);

    DispatchOutcome::Value(RuntimeValue::Bool(closed))
}
