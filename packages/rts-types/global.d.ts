declare module "rts:global" {
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

  const _default: {
    set(key: str, value: str): void;
    get(key: str): str | undefined;
    has(key: str): bool;
    delete(key: str): bool;
    keys(): str;
  };
  export default _default;
}
