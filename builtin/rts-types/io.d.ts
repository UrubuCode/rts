declare module "rts:io" {
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

  const _default: {
    print(message: str): void;
    panic(message?: str): never;
    stdin_read(maxBytes?: usize): str;
    stdout_write(message: str): void;
    stderr_write(message: str): void;
    is_ok<T>(result: Result<T>): bool;
    is_err<T>(result: Result<T>): bool;
    unwrap_or<T>(result: Result<T>, fallback: T): T;
  };
  export default _default;
}
