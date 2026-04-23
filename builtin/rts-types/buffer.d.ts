declare module "rts:buffer" {
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

  const _default: {
    alloc(size: usize): Handle;
    free(handle: Handle): bool;
    len(handle: Handle): usize | undefined;
    read_u8(handle: Handle, offset: usize): u8 | undefined;
    write_u8(handle: Handle, offset: usize, value: u8): bool;
    fill(handle: Handle, value: u8): bool;
    write_text(handle: Handle, content: str, offset?: usize): usize | undefined;
    read_text(handle: Handle, offset: usize, length?: usize): str | undefined;
    copy(source: Handle, target: Handle, sourceOffset?: usize, targetOffset?: usize, length?: usize): usize | undefined;
  };
  export default _default;
}
