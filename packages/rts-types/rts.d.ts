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
     * Reads a complete HTTP/1.1 request from a TCP stream and returns a handle.
     */
    export function http_read_request(stream: u64): io.Result<u64>;
    /**
     * Returns the HTTP method (GET, POST, ...) of a parsed request.
     */
    export function http_request_method(request: u64): io.Result<str>;
    /**
     * Returns the request path (with query string) of a parsed request.
     */
    export function http_request_path(request: u64): io.Result<str>;
    /**
     * Returns the value of a header by case-insensitive name. Empty string if absent.
     */
    export function http_request_header(request: u64, name: str): io.Result<str>;
    /**
     * Returns the body of a parsed request as a UTF-8 string.
     */
    export function http_request_body(request: u64): io.Result<str>;
    /**
     * Releases the memory for a parsed request handle.
     */
    export function http_request_free(request: u64): io.Result<bool>;
    /**
     * Writes a simple HTTP/1.1 response to a stream with status, body and optional content-type.
     */
    export function http_response_write(stream: u64, status: u32, body: str, content_type?: str): io.Result<usize>;
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
    export function args(): Array<str> | str;
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
     * Removes a key from global map. Retorna `true` se a chave existia.
     */
    export function remove(key: str): bool;
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

  /**
   * Assertion helpers and test output utilities for rts:test.
   */
  export namespace test {
    /**
     * Panics if condition is false. Optional message is shown on failure.
     */
    export function assert(condition: bool, message?: str): void;
    /**
     * Panics if a and b are not equal (string comparison). Optional message shown on failure.
     */
    export function assert_eq(a: str, b: str, message?: str): void;
    /**
     * Panics if a and b are equal (string comparison). Optional message shown on failure.
     */
    export function assert_ne(a: str, b: str, message?: str): void;
    /**
     * Emits a passing test message to stdout.
     */
    export function pass(message?: str): void;
    /**
     * Unconditionally panics with an optional message.
     */
    export function fail(message?: str): never;
    /**
     * Emits a test suite header to stdout.
     */
    export function describe(name: str): void;
    /**
     * Emits a test case header to stdout.
     */
    export function it(name: str): void;
  }

  /**
   * JavaScript Object Notation helpers backed by serde_json.
   */
  export namespace JSON {
    /**
     * Serializa um valor para string JSON. Retorna "null" para undefined ou funcoes.
     */
    export function stringify(value: any): string;
    /**
     * Desserializa uma string JSON em um valor. Retorna undefined em caso de erro.
     */
    export function parse(text: string): any;
  }

  /**
   * Raw UTF-8 string primitives. These are the machine-level building blocks for the TS String class.
   */
  export namespace str {
    /**
     * Returns the byte length of a string.
     */
    export function len(s: str): u64;
    /**
     * Concatenates two strings.
     */
    export function concat(a: str, b: str): str;
    /**
     * Returns a substring from start (inclusive) to end (exclusive). Negative indices count from end.
     */
    export function slice(s: str, start: i64, end?: i64): str;
    /**
     * Returns the string converted to uppercase.
     */
    export function to_upper(s: str): str;
    /**
     * Returns the string converted to lowercase.
     */
    export function to_lower(s: str): str;
    /**
     * Removes leading and trailing whitespace.
     */
    export function trim(s: str): str;
    /**
     * Removes leading whitespace.
     */
    export function trim_start(s: str): str;
    /**
     * Removes trailing whitespace.
     */
    export function trim_end(s: str): str;
    /**
     * Replaces the first occurrence of `from` with `to`.
     */
    export function replace(s: str, from: str, to: str): str;
    /**
     * Replaces all occurrences of `from` with `to`.
     */
    export function replace_all(s: str, from: str, to: str): str;
    /**
     * Returns true if the string contains the given substring.
     */
    export function includes(s: str, needle: str): bool;
    /**
     * Returns true if the string starts with the given prefix.
     */
    export function starts_with(s: str, prefix: str): bool;
    /**
     * Returns true if the string ends with the given suffix.
     */
    export function ends_with(s: str, suffix: str): bool;
    /**
     * Returns the byte index of the first occurrence of needle, or -1 if not found.
     */
    export function index_of(s: str, needle: str): i64;
    /**
     * Returns the byte index of the last occurrence of needle, or -1 if not found.
     */
    export function last_index_of(s: str, needle: str): i64;
    /**
     * Returns the UTF-8 character at the given char index as a str.
     */
    export function char_at(s: str, index: u64): str;
    /**
     * Splits the string by separator and returns parts joined by newline (use str.split_nth to access each part).
     */
    export function split(s: str, sep: str): str;
    /**
     * Returns the Nth part after splitting s by sep.
     */
    export function split_nth(s: str, sep: str, n: u64): str;
    /**
     * Returns the string repeated n times.
     */
    export function repeat(s: str, n: u64): str;
    /**
     * Pads the string at the start to reach target length.
     */
    export function pad_start(s: str, target_len: u64, fill?: str): str;
    /**
     * Pads the string at the end to reach target length.
     */
    export function pad_end(s: str, target_len: u64, fill?: str): str;
    /**
     * Returns the number of Unicode scalar values (chars) in the string.
     */
    export function char_count(s: str): u64;
    /**
     * Returns true if the string has zero length.
     */
    export function is_empty(s: str): bool;
    /**
     * Converts a number to its string representation.
     */
    export function from_number(n: f64): str;
    /**
     * Parses the string as an integer. Returns NaN (as f64) on failure.
     */
    export function parse_int(s: str, radix?: u64): f64;
    /**
     * Parses the string as a floating-point number. Returns NaN on failure.
     */
    export function parse_float(s: str): f64;
  }

  /**
   * Primitivas brutas de máquina: memória, escopo, funções e constantes. Rust expõe apenas tipos de máquina (i64, f64, u64, bool) — sem semântica JS.
   */
  export namespace rts {
    /**
     * Declara uma função no registry de runtime.
     */
    export function declare_fn(name_ptr: u64, arity: u64, body_ptr: u64): void;
    /**
     * Invoca função pelo ponteiro de nome, retorna ponteiro do corpo.
     */
    export function call_fn(name_ptr: u64, args_ptr: u64, args_len: u64): u64;
    /**
     * Retorna um valor do escopo atual.
     */
    export function return_val(value: u64): u64;
    /**
     * Empilha novo escopo de variáveis.
     */
    export function scope_push(): void;
    /**
     * Desempilha o escopo atual.
     */
    export function scope_pop(): void;
    /**
     * Define variável no escopo atual pelo hash do nome.
     */
    export function set_var(name_hash: u64, value: u64): void;
    /**
     * Lê variável percorrendo o stack de escopos.
     */
    export function get_var(name_hash: u64): u64;
    /**
     * Declara constante global imutável.
     */
    export function declare_const(name_hash: u64, value: u64): void;
    /**
     * Lê constante global pelo hash do nome.
     */
    export function get_const(name_hash: u64): u64;
    /**
     * Aloca `size` bytes zerados, retorna ponteiro.
     */
    export function alloc(size: u64): u64;
    /**
     * Libera bloco de memória.
     */
    export function free(ptr: u64, size: u64): void;
    /**
     * Copia `len` bytes de src para dst sem overlap.
     */
    export function mem_copy(dst: u64, src: u64, len: u64): void;
    /**
     * Soma dois inteiros i64 sem overhead JS.
     */
    export function i64_add(a: i64, b: i64): i64;
    /**
     * Multiplica dois floats f64.
     */
    export function f64_mul(a: f64, b: f64): f64;
    /**
     * Cria handle de string a partir de ponteiro e comprimento.
     */
    export function str_new(ptr: u64, len: u64): u64;
  }

  export namespace rts {
    /**
     * Extensões C nativas para coerção de tipos mistos. Injetadas pelo HIR quando operandos têm tipos incompatíveis.
     */
    export namespace natives {
      /**
       * Converte qualquer valor para string (semântica JS).
       */
      export function to_string(value: u64): u64;
      /**
       * Converte qualquer valor para número (semântica JS).
       */
      export function to_number(value: u64): f64;
      /**
       * Converte qualquer valor para bool (truthy/falsy JS).
       */
      export function to_bool(value: u64): bool;
      /**
       * Merge genérico de dois valores com coerção (string ou número).
       */
      export function merge(a: u64, b: u64): u64;
      /**
       * Operador `+` com coerção: string+qualquer=concat, número+número=soma.
       */
      export function add_mixed(a: u64, b: u64): u64;
      /**
       * Igualdade fraca `==` com coerção de tipos JS.
       */
      export function eq_loose(a: u64, b: u64): bool;
      /**
       * Comparação com coerção JS, retorna -1, 0 ou 1.
       */
      export function compare(a: u64, b: u64): i64;
    }

    /**
     * Operações inline com tipos já conhecidos pelo MIR. Sem overhead de coerção — tipos são garantidos pelo compilador.
     */
    export namespace hotops {
      /**
       * Subtração i64.
       */
      export function i64_sub(a: i64, b: i64): i64;
      /**
       * Divisão i64.
       */
      export function i64_div(a: i64, b: i64): i64;
      /**
       * Módulo i64.
       */
      export function i64_mod(a: i64, b: i64): i64;
      /**
       * Igualdade i64.
       */
      export function i64_eq(a: i64, b: i64): bool;
      /**
       * Menor que i64.
       */
      export function i64_lt(a: i64, b: i64): bool;
      /**
       * Menor ou igual i64.
       */
      export function i64_le(a: i64, b: i64): bool;
      /**
       * Adição f64.
       */
      export function f64_add(a: f64, b: f64): f64;
      /**
       * Subtração f64.
       */
      export function f64_sub(a: f64, b: f64): f64;
      /**
       * Divisão f64.
       */
      export function f64_div(a: f64, b: f64): f64;
      /**
       * Igualdade f64.
       */
      export function f64_eq(a: f64, b: f64): bool;
      /**
       * Menor que f64.
       */
      export function f64_lt(a: f64, b: f64): bool;
      /**
       * i64 para string (tabela pré-computada para 0..=255).
       */
      export function i64_to_string(n: i64): u64;
      /**
       * f64 para string.
       */
      export function f64_to_string(n: f64): u64;
    }

    /**
     * Debug info em runtime: carrega .ometa, resolve PC → source location, formata erros.
     */
    export namespace debug {
      /**
       * Carrega arquivo .ometa, retorna handle numérico.
       */
      export function load_metadata(path_ptr: u64): u64;
      /**
       * Resolve offset de PC para localização no arquivo fonte.
       */
      export function resolve_location(handle: u64, pc_offset: u64): str;
      /**
       * Formata mensagem de erro com localização fonte (modo dev).
       */
      export function format_error(message_ptr: u64, pc_offset: u64): str;
    }

  }

}
