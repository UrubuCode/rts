declare module "rts" {
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

  export interface WritableStream {
    write(message: str): void;
  }

  export interface ReadableStream {
    read(maxBytes?: usize): str;
  }

  export interface FileHandle {
    close(): void;
  }
  /**
   * Input/output utilities and Result helpers.
   */
  export namespace io {
    export interface Error {
      message: str;
    }

    export interface Ok<T> {
      ok: true;
      tag: "ok";
      value: T;
      error: undefined;
    }

    export interface Err {
      ok: false;
      tag: "err";
      value: undefined;
      error: Error;
    }

    export type Result<T> = Ok<T> | Err;

    /**
     * Writes a message to stdout.
     */
    export function print(message: str): void;
    /**
     * Aborts execution with a runtime panic message.
     */
    export function panic(message?: str): never;
    /**
     * Reads a line or payload from stdin.
     */
    export function stdin_read(maxBytes?: usize): str;
    /**
     * Writes raw text to stdout.
     */
    export function stdout_write(message: str): void;
    /**
     * Writes raw text to stderr.
     */
    export function stderr_write(message: str): void;
    /**
     * Returns true when an io.Result is successful.
     */
    export function is_ok<T>(result: Result<T>): bool;
    /**
     * Returns true when an io.Result is an error.
     */
    export function is_err<T>(result: Result<T>): bool;
    /**
     * Returns the inner value or a fallback when the result is an error.
     */
    export function unwrap_or<T>(result: Result<T>, fallback: T): T;
  }

  /**
   * Filesystem operations backed by std::fs.
   */
  export namespace fs {
    /**
     * Reads an UTF-8 file and returns io.Result<string>.
     */
    export function read_to_string<P extends str>(path: P): io.Result<str>;
    /**
     * Reads a file as bytes encoded as a hex payload string in io.Result.
     */
    export function read<P extends str>(path: P): io.Result<str>;
    /**
     * Writes text or hex payload bytes to a file path.
     */
    export function write<P extends str>(path: P, data: str): io.Result<void>;
  }

  /**
   * Network utilities backed by std::net with TCP, UDP and IP address support.
   */
  export namespace net {
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
  }

  /**
   * Process utilities such as env, cwd, pid and time.
   */
  export namespace process {
    /**
     * Returns process CLI arguments.
     */
    export function args(): globalThis.Array<str> | str;
    /**
     * Returns current working directory.
     */
    export function cwd(): str;
    /**
     * Changes process working directory.
     */
    export function chdir(path: str): void;
    /**
     * Reads an environment variable.
     */
    export function env_get(name: str): str | undefined;
    /**
     * Sets an environment variable.
     */
    export function env_set(name: str, value: str): void;
    /**
     * Returns target OS name.
     */
    export function platform(): str;
    /**
     * Returns target architecture.
     */
    export function arch(): str;
    /**
     * Returns current process id.
     */
    export function pid(): i32;
    /**
     * Sleeps current thread for milliseconds.
     */
    export function sleep(ms: f64): void;
    /**
     * Aborts execution with an exit code signal.
     */
    export function exit(code?: i32): never;
    /**
     * Returns wall clock time in milliseconds.
     */
    export function clock_now(): f64;
  }

  /**
   * Cryptographic helpers backed by Rust implementations.
   */
  export namespace crypto {
    /**
     * Computes SHA-256 digest and returns hex string.
     */
    export function sha256(data: str): str;
  }

  /**
   * Small runtime key-value storage for bootstrap state.
   */
  export namespace global {
    /**
     * Stores a string value in runtime global map.
     */
    export function set(key: str, value: str): void;
    /**
     * Reads a string value from runtime global map.
     */
    export function get(key: str): str | undefined;
    /**
     * Checks whether a key exists in global map.
     */
    export function has(key: str): bool;
    /**
     * Deletes a key from global map.
     */
    export function delete(key: str): bool;
    /**
     * Returns global keys joined by commas.
     */
    export function keys(): str;
  }

  /**
   * Low-level byte buffer API with explicit handles.
   */
  export namespace buffer {
    export type Handle = usize;

    /**
     * Allocates a runtime buffer and returns its handle.
     */
    export function alloc(size: usize): Handle;
    /**
     * Releases a runtime buffer handle.
     */
    export function free(handle: Handle): bool;
    /**
     * Returns current buffer length.
     */
    export function len(handle: Handle): usize | undefined;
    /**
     * Reads an unsigned byte from offset.
     */
    export function read_u8(handle: Handle, offset: usize): u8 | undefined;
    /**
     * Writes an unsigned byte at offset.
     */
    export function write_u8(handle: Handle, offset: usize, value: u8): bool;
    /**
     * Fills entire buffer with a byte value.
     */
    export function fill(handle: Handle, value: u8): bool;
    /**
     * Writes UTF-8 text into a buffer from optional offset.
     */
    export function write_text(handle: Handle, content: str, offset?: usize): usize | undefined;
    /**
     * Reads UTF-8 text from buffer range.
     */
    export function read_text(handle: Handle, offset: usize, length?: usize): str | undefined;
    /**
     * Copies bytes between two runtime buffers.
     */
    export function copy(source: Handle, target: Handle, sourceOffset?: usize, targetOffset?: usize, length?: usize): usize | undefined;
  }

  /**
   * Promise handles and synchronous await bridge.
   */
  export namespace promise {
    export type Handle = usize;

    export type State = "pending" | "fulfilled" | "rejected";

    /**
     * Creates a fulfilled promise handle.
     */
    export function resolve(value: str): Handle;
    /**
     * Creates a rejected promise handle.
     */
    export function reject(reason: str): Handle;
    /**
     * Returns current state of a promise handle.
     */
    export function status(handle: Handle): State | undefined;
    /**
     * Checks whether promise is fulfilled or rejected.
     */
    export function is_settled(handle: Handle): bool;
    /**
     * Waits for promise completion and returns its payload.
     */
    export function await(handle: Handle): str | undefined;
  }

  /**
   * Async task scheduler helpers that resolve into promise handles.
   */
  export namespace task {
    /**
     * Spawns an async sleep task resolved as a promise handle.
     */
    export function sleep(ms: f64, value?: str): promise.Handle;
    /**
     * Spawns an async SHA-256 task resolved as a promise handle.
     */
    export function hash_sha256(data: str): promise.Handle;
    /**
     * Spawns async text file read task.
     */
    export function read_text_file(path: str): promise.Handle;
    /**
     * Spawns async text file write task.
     */
    export function write_text_file(path: str, content: str): promise.Handle;
    /**
     * Spawns async text file append task.
     */
    export function append_text_file(path: str, content: str): promise.Handle;
  }

  /**
   * Deterministic garbage collector (gc-arena). Arena-based allocation with safe collection at quiescence points after function/class/closure execution.
   */
  export namespace gc {
    /**
     * Allocate a tagged blob into the GC arena. Returns a u64 handle.
     */
    export function alloc(kind: u8, payload: str): u64;
    /**
     * Release a handle, making the blob eligible for collection. Returns true if the handle was live.
     */
    export function free(handle: u64): bool;
    /**
     * Full GC collection. Only call at a safe quiescence point (no live handles on stack).
     */
    export function collect(): void;
    /**
     * Amortised GC — collect proportional to allocation debt. Safe to call at any time.
     */
    export function collect_debt(): void;
    /**
     * Returns a JSON string with GC diagnostics: allocated_bytes, generation, live_slots.
     */
    export function stats(): str;
  }

}
