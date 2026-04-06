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

  export interface Process {
    stdout: WritableStream;
    stderr: WritableStream;
    argv: globalThis.Array<str>;
    cwd(): str;
    exit(code?: i32): never;
  }

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

    export function print(message: str): void;
    export function panic(message?: str): never;
    export function stdin_read(maxBytes?: usize): str;
    export function stdout_write(message: str): void;
    export function stderr_write(message: str): void;
    export function is_ok<T>(result: Result<T>): bool;
    export function is_err<T>(result: Result<T>): bool;
    export function unwrap_or<T>(result: Result<T>, fallback: T): T;
  }

  export namespace fs {
    export function read_to_string<P extends str>(path: P): io.Result<str>;
    export function read<P extends str>(path: P): io.Result<str>;
    export function write<P extends str>(path: P, data: str): io.Result<void>;
  }

  export namespace process {
    export function args(): globalThis.Array<str> | str;
    export function cwd(): str;
    export function chdir(path: str): void;
    export function env_get(name: str): str | undefined;
    export function env_set(name: str, value: str): void;
    export function platform(): str;
    export function arch(): str;
    export function pid(): i32;
    export function sleep(ms: f64): void;
    export function exit(code?: i32): never;
    export function clock_now(): f64;
  }

  export namespace crypto {
    export function sha256(data: str): str;
  }

  export namespace global {
    export function set(key: str, value: str): void;
    export function get(key: str): str | undefined;
    export function has(key: str): bool;
    export function delete(key: str): bool;
    export function keys(): str;
  }

  export namespace buffer {
    export type Handle = usize;

    export function alloc(size: usize): Handle;
    export function free(handle: Handle): bool;
    export function len(handle: Handle): usize | undefined;
    export function read_u8(handle: Handle, offset: usize): u8 | undefined;
    export function write_u8(handle: Handle, offset: usize, value: u8): bool;
    export function fill(handle: Handle, value: u8): bool;
    export function write_text(
      handle: Handle,
      content: str,
      offset?: usize,
    ): usize | undefined;
    export function read_text(
      handle: Handle,
      offset: usize,
      length?: usize,
    ): str | undefined;
    export function copy(
      source: Handle,
      target: Handle,
      sourceOffset?: usize,
      targetOffset?: usize,
      length?: usize,
    ): usize | undefined;
  }

  export namespace promise {
    export type Handle = usize;
    export type State = "pending" | "fulfilled" | "rejected";

    export function resolve(value: str): Handle;
    export function reject(reason: str): Handle;
    export function status(handle: Handle): State | undefined;
    export function is_settled(handle: Handle): bool;
    export function await(handle: Handle): str | undefined;
  }

  export namespace task {
    export function sleep(ms: f64, value?: str): promise.Handle;
    export function hash_sha256(data: str): promise.Handle;
    export function read_text_file(path: str): promise.Handle;
    export function write_text_file(path: str, content: str): promise.Handle;
    export function append_text_file(path: str, content: str): promise.Handle;
  }
}
