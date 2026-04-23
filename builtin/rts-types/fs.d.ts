declare module "rts:fs" {
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

  }

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

  const _default: {
    read_to_string<P extends str>(path: P): io.Result<str>;
    read<P extends str>(path: P): io.Result<str>;
    write<P extends str>(path: P, data: str): io.Result<void>;
  };
  export default _default;
}
