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
   * TCP networking primitives backed by std::net.
   */
  export namespace net {
    /**
     * Creates a TCP listener bound to the given host and port.
     */
    export function listen(host: str, port: u16): io.Result<u64>;
    /**
     * Accepts the next incoming TCP connection on a listener. Blocks until a client connects.
     */
    export function accept(listener: u64): io.Result<u64>;
    /**
     * Opens a TCP connection to the given host and port.
     */
    export function connect(host: str, port: u16): io.Result<u64>;
    /**
     * Reads up to maxBytes from a TCP stream. Returns the data as a UTF-8 string.
     */
    export function read(stream: u64, maxBytes?: usize): io.Result<str>;
    /**
     * Writes data to a TCP stream. Returns the number of bytes written.
     */
    export function write(stream: u64, data: str): io.Result<usize>;
    /**
     * Closes a TCP listener or stream handle.
     */
    export function close(handle: u64): void;
    /**
     * Sets the read/write timeout in milliseconds for a TCP stream. Pass 0 to disable.
     */
    export function set_timeout(stream: u64, millis: u64): void;
    /**
     * Returns the local address of a listener or stream as "host:port".
     */
    export function local_addr(handle: u64): io.Result<str>;
    /**
     * Returns the remote address of a TCP stream as "host:port".
     */
    export function peer_addr(stream: u64): io.Result<str>;
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

}
