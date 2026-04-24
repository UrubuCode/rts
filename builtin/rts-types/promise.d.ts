declare module "rts:promise" {
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

  const _default: {
    resolve(value: str): Handle;
    reject(reason: str): Handle;
    status(handle: Handle): State | undefined;
    is_settled(handle: Handle): bool;
    await(handle: Handle): str | undefined;
  };
  export default _default;
}
