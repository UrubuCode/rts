declare module "rts:net" {
  export type i8 = number;
  export type u8 = number;
  export type i16 = number;
  export type u16 = number;
  export type i32 = number;
  export type u32 = number;
  export type i64 = number;
  export type u64 = number;
  export type isize = number;
  export type usize = number;
  export type f32 = number;
  export type f64 = number;
  export type bool = boolean;
  export type str = string;
  export interface TcpConnection {
        stream: u64;
        peer_addr: str;
      }

  export interface UdpMessage {
        data: str;
        addr: str;
      }

  export interface IpAddr {
        version: "v4" | "v6";
        addr: str;
        is_loopback: bool;
        is_multicast: bool;
        is_unspecified: bool;
      }

  export interface Ipv4Addr {
        octets: str;
        addr: str;
        is_loopback: bool;
        is_multicast: bool;
        is_broadcast: bool;
        is_private: bool;
        is_link_local: bool;
      }

  export interface Ipv6Addr {
        segments: str;
        addr: str;
        is_loopback: bool;
        is_multicast: bool;
        is_unspecified: bool;
      }

  export interface SocketAddr {
        ip: str;
        port: u16;
        addr: str;
      }

  export type ShutdownHow = "Read" | "Write" | "Both";

  /**
   * Creates a TCP listener bound to the specified address.
   */
  export function tcp_listen(addr: str): io.Result<u64>;
  /**
   * Accepts a new TCP connection on this listener.
   */
  export function tcp_accept(listener: u64): io.Result<TcpConnection>;
  /**
   * Returns the local socket address of this listener.
   */
  export function tcp_local_addr(listener: u64): io.Result<str>;
  /**
   * Opens a TCP connection to a remote host.
   */
  export function tcp_connect(addr: str): io.Result<u64>;
  /**
   * Reads data from a TCP stream.
   */
  export function tcp_read(stream: u64, max_bytes?: usize): io.Result<str>;
  /**
   * Writes data to a TCP stream.
   */
  export function tcp_write(stream: u64, data: str): io.Result<usize>;
  /**
   * Flushes the TCP stream output buffer.
   */
  export function tcp_flush(stream: u64): io.Result<void>;
  /**
   * Shuts down the read, write, or both halves of this connection.
   */
  export function tcp_shutdown(stream: u64, how: ShutdownHow): io.Result<void>;
  /**
   * Returns the socket address of the remote peer.
   */
  export function tcp_peer_addr(stream: u64): io.Result<str>;
  /**
   * Sets the read timeout for TCP operations.
   */
  export function tcp_set_read_timeout(stream: u64, timeout_ms?: u64): io.Result<void>;
  /**
   * Sets the write timeout for TCP operations.
   */
  export function tcp_set_write_timeout(stream: u64, timeout_ms?: u64): io.Result<void>;
  /**
   * Sets the value of the TCP_NODELAY option on this socket.
   */
  export function tcp_set_nodelay(stream: u64, nodelay: bool): io.Result<void>;
  /**
   * Gets the value of the TCP_NODELAY option on this socket.
   */
  export function tcp_nodelay(stream: u64): io.Result<bool>;
  /**
   * Sets the value for the IP_TTL option on this socket.
   */
  export function tcp_set_ttl(stream: u64, ttl: u32): io.Result<void>;
  /**
   * Gets the value of the IP_TTL option for this socket.
   */
  export function tcp_ttl(stream: u64): io.Result<u32>;
  /**
   * Creates a UDP socket bound to the specified address.
   */
  export function udp_bind(addr: str): io.Result<u64>;
  /**
   * Connects this UDP socket to a remote address.
   */
  export function udp_connect(socket: u64, addr: str): io.Result<void>;
  /**
   * Sends data on the socket to the connected address.
   */
  export function udp_send(socket: u64, data: str): io.Result<usize>;
  /**
   * Receives data from the socket.
   */
  export function udp_recv(socket: u64, max_bytes?: usize): io.Result<str>;
  /**
   * Sends data on the socket to the given address.
   */
  export function udp_send_to(socket: u64, data: str, addr: str): io.Result<usize>;
  /**
   * Receives data from the socket.
   */
  export function udp_recv_from(socket: u64, max_bytes?: usize): io.Result<UdpMessage>;
  /**
   * Returns the socket address that this socket was created from.
   */
  export function udp_local_addr(socket: u64): io.Result<str>;
  /**
   * Returns the socket address of the remote peer this socket was connected to.
   */
  export function udp_peer_addr(socket: u64): io.Result<str>;
  /**
   * Sets the read timeout for UDP operations.
   */
  export function udp_set_read_timeout(socket: u64, timeout_ms?: u64): io.Result<void>;
  /**
   * Sets the write timeout for UDP operations.
   */
  export function udp_set_write_timeout(socket: u64, timeout_ms?: u64): io.Result<void>;
  /**
   * Sets the value of the SO_BROADCAST option for this socket.
   */
  export function udp_set_broadcast(socket: u64, broadcast: bool): io.Result<void>;
  /**
   * Gets the value of the SO_BROADCAST option for this socket.
   */
  export function udp_broadcast(socket: u64): io.Result<bool>;
  /**
   * Sets the value of the IP_MULTICAST_LOOP option for this socket.
   */
  export function udp_set_multicast_loop_v4(socket: u64, multicast_loop_v4: bool): io.Result<void>;
  /**
   * Gets the value of the IP_MULTICAST_LOOP option for this socket.
   */
  export function udp_multicast_loop_v4(socket: u64): io.Result<bool>;
  /**
   * Sets the value of the IP_MULTICAST_TTL option for this socket.
   */
  export function udp_set_multicast_ttl_v4(socket: u64, multicast_ttl_v4: u32): io.Result<void>;
  /**
   * Gets the value of the IP_MULTICAST_TTL option for this socket.
   */
  export function udp_multicast_ttl_v4(socket: u64): io.Result<u32>;
  /**
   * Sets the value for the IP_TTL option on this socket.
   */
  export function udp_set_ttl(socket: u64, ttl: u32): io.Result<void>;
  /**
   * Gets the value of the IP_TTL option for this socket.
   */
  export function udp_ttl(socket: u64): io.Result<u32>;
  /**
   * Executes an operation to join a multicast group.
   */
  export function udp_join_multicast_v4(socket: u64, multiaddr: str, interface: str): io.Result<void>;
  /**
   * Executes an operation to leave a multicast group.
   */
  export function udp_leave_multicast_v4(socket: u64, multiaddr: str, interface: str): io.Result<void>;
  /**
   * Parses a string as an IP address.
   */
  export function parse_ip_addr(addr: str): io.Result<IpAddr>;
  /**
   * Parses a string as an IPv4 address.
   */
  export function parse_ipv4_addr(addr: str): io.Result<Ipv4Addr>;
  /**
   * Parses a string as an IPv6 address.
   */
  export function parse_ipv6_addr(addr: str): io.Result<Ipv6Addr>;
  /**
   * Parses a string as a socket address.
   */
  export function parse_socket_addr(addr: str): io.Result<SocketAddr>;
  /**
   * Resolves a string to socket addresses.
   */
  export function to_socket_addrs(addr: str): io.Result<str>;
  /**
   * Closes a network resource handle.
   */
  export function close(handle: u64): bool;

  const _default: {
    tcp_listen(addr: str): io.Result<u64>;
    tcp_accept(listener: u64): io.Result<TcpConnection>;
    tcp_local_addr(listener: u64): io.Result<str>;
    tcp_connect(addr: str): io.Result<u64>;
    tcp_read(stream: u64, max_bytes?: usize): io.Result<str>;
    tcp_write(stream: u64, data: str): io.Result<usize>;
    tcp_flush(stream: u64): io.Result<void>;
    tcp_shutdown(stream: u64, how: ShutdownHow): io.Result<void>;
    tcp_peer_addr(stream: u64): io.Result<str>;
    tcp_set_read_timeout(stream: u64, timeout_ms?: u64): io.Result<void>;
    tcp_set_write_timeout(stream: u64, timeout_ms?: u64): io.Result<void>;
    tcp_set_nodelay(stream: u64, nodelay: bool): io.Result<void>;
    tcp_nodelay(stream: u64): io.Result<bool>;
    tcp_set_ttl(stream: u64, ttl: u32): io.Result<void>;
    tcp_ttl(stream: u64): io.Result<u32>;
    udp_bind(addr: str): io.Result<u64>;
    udp_connect(socket: u64, addr: str): io.Result<void>;
    udp_send(socket: u64, data: str): io.Result<usize>;
    udp_recv(socket: u64, max_bytes?: usize): io.Result<str>;
    udp_send_to(socket: u64, data: str, addr: str): io.Result<usize>;
    udp_recv_from(socket: u64, max_bytes?: usize): io.Result<UdpMessage>;
    udp_local_addr(socket: u64): io.Result<str>;
    udp_peer_addr(socket: u64): io.Result<str>;
    udp_set_read_timeout(socket: u64, timeout_ms?: u64): io.Result<void>;
    udp_set_write_timeout(socket: u64, timeout_ms?: u64): io.Result<void>;
    udp_set_broadcast(socket: u64, broadcast: bool): io.Result<void>;
    udp_broadcast(socket: u64): io.Result<bool>;
    udp_set_multicast_loop_v4(socket: u64, multicast_loop_v4: bool): io.Result<void>;
    udp_multicast_loop_v4(socket: u64): io.Result<bool>;
    udp_set_multicast_ttl_v4(socket: u64, multicast_ttl_v4: u32): io.Result<void>;
    udp_multicast_ttl_v4(socket: u64): io.Result<u32>;
    udp_set_ttl(socket: u64, ttl: u32): io.Result<void>;
    udp_ttl(socket: u64): io.Result<u32>;
    udp_join_multicast_v4(socket: u64, multiaddr: str, interface: str): io.Result<void>;
    udp_leave_multicast_v4(socket: u64, multiaddr: str, interface: str): io.Result<void>;
    parse_ip_addr(addr: str): io.Result<IpAddr>;
    parse_ipv4_addr(addr: str): io.Result<Ipv4Addr>;
    parse_ipv6_addr(addr: str): io.Result<Ipv6Addr>;
    parse_socket_addr(addr: str): io.Result<SocketAddr>;
    to_socket_addrs(addr: str): io.Result<str>;
    close(handle: u64): bool;
  };
  export default _default;
}
