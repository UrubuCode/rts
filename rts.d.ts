declare module "rts" {
  /**
   * Represents an 8-bit signed integer. Range: -128 to 127.
   */
  export type i8 = number;

  /**
   * Represents an 8-bit unsigned integer. Range: 0 to 255.
   */
  export type u8 = number;

  /**
   * Represents a 16-bit signed integer. Range: -32768 to 32767.
   */
  export type i16 = number;

  /**
   * Represents a 16-bit unsigned integer. Range: 0 to 65535.
   */
  export type u16 = number;

  /**
   * Represents a 32-bit signed integer.
   */
  export type i32 = number;

  /**
   * Represents a 32-bit unsigned integer.
   */
  export type u32 = number;

  /**
   * Represents a 64-bit signed integer.
   */
  export type i64 = number;

  /**
   * Represents a 64-bit unsigned integer.
   */
  export type u64 = number;

  /**
   * Pointer-sized signed integer (platform dependent).
   */
  export type isize = number;

  /**
   * Pointer-sized unsigned integer (platform dependent).
   */
  export type usize = number;

  /**
   * 32-bit floating point number.
   */
  export type f32 = number;

  /**
   * 64-bit floating point number.
   */
  export type f64 = number;

  /**
   * RTS boolean primitive alias.
   */
  export type bool = boolean;

  /**
   * RTS UTF-8 string primitive alias.
   */
  export type str = string;

  /**
   * Minimal writable stream shape exposed by runtime.
   */
  export interface WritableStream {
    write(message: str): void;
  }

  /**
   * Base process handle exposed by runtime.
   */
  export interface Process {
    stdout: WritableStream;
    stderr: WritableStream;
    argv: globalThis.Array<str>;
    cwd(): str;
    exit(code?: i32): never;
  }

  /**
   * Runtime process object. Must be imported explicitly from "rts".
   */
  export const process: Process;

  /**
   * Base output call provided by runtime.
   */
  export function print(message: str): void;

  /**
   * Base panic call provided by runtime.
   */
  export function panic(message?: str): never;

  /**
   * Returns a monotonic timestamp in milliseconds.
   */
  export function clockNow(): f64;

  /**
   * Low-level allocator entrypoint.
   */
  export function alloc(size: usize): usize;

  /**
   * Low-level deallocator entrypoint.
   */
  export function dealloc(ptr: usize, size: usize): void;
}
