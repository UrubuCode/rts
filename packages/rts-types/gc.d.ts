declare module "rts:gc" {
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

  const _default: {
    alloc(kind: u8, payload: str): u64;
    free(handle: u64): bool;
    collect(): void;
    collect_debt(): void;
    stats(): str;
  };
  export default _default;
}
