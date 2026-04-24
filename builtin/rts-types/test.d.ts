declare module "rts:test" {
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
   * Panics if condition is false. Optional message is shown on failure.
   */
  export function assert(condition: bool, message?: str): void;
  /**
   * Panics if a and b are not equal (string comparison). Optional message shown on failure.
   */
  export function assert_eq(a: str, b: str, message?: str): void;
  /**
   * Panics if a and b are equal (string comparison). Optional message shown on failure.
   */
  export function assert_ne(a: str, b: str, message?: str): void;
  /**
   * Emits a passing test message to stdout.
   */
  export function pass(message?: str): void;
  /**
   * Unconditionally panics with an optional message.
   */
  export function fail(message?: str): never;
  /**
   * Emits a test suite header to stdout.
   */
  export function describe(name: str): void;
  /**
   * Emits a test case header to stdout.
   */
  export function it(name: str): void;

  const _default: {
    assert(condition: bool, message?: str): void;
    assert_eq(a: str, b: str, message?: str): void;
    assert_ne(a: str, b: str, message?: str): void;
    pass(message?: str): void;
    fail(message?: str): never;
    describe(name: str): void;
    it(name: str): void;
  };
  export default _default;
}
