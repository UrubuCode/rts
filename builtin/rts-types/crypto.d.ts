declare module "rts:crypto" {
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
   * Computes SHA-256 digest and returns hex string.
   */
  export function sha256(data: str): str;

  const _default: {
    sha256(data: str): str;
  };
  export default _default;
}
