declare module "rts:task" {
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

  const _default: {
    sleep(ms: f64, value?: str): promise.Handle;
    hash_sha256(data: str): promise.Handle;
    read_text_file(path: str): promise.Handle;
    write_text_file(path: str, content: str): promise.Handle;
    append_text_file(path: str, content: str): promise.Handle;
  };
  export default _default;
}
