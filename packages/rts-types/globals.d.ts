declare module "rts:globals" {
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
   * Defines a global variable accessible from anywhere in the program.
   */
  export function set(name: str, value: any): void;
  /**
   * Reads a global variable by name.
   */
  export function get(name: str): any;
  /**
   * Returns true when a global variable is defined.
   */
  export function has(name: str): bool;
  /**
   * Removes a global variable by name.
   */
  export function remove(name: str): bool;
  /**
   * Returns a comma-separated list with every global key.
   */
  export function keys(): str;

  const _default: {
    set(name: str, value: any): void;
    get(name: str): any;
    has(name: str): bool;
    remove(name: str): bool;
    keys(): str;
  };
  export default _default;
}
