declare module "rts:process" {
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
   * Returns process CLI arguments.
   */
  export function args(): Array<str> | str;
  /**
   * Returns current working directory.
   */
  export function cwd(): str;
  /**
   * Changes process working directory.
   */
  export function chdir(path: str): void;
  /**
   * Reads an environment variable.
   */
  export function env_get(name: str): str | undefined;
  /**
   * Sets an environment variable.
   */
  export function env_set(name: str, value: str): void;
  /**
   * Returns target OS name.
   */
  export function platform(): str;
  /**
   * Returns target architecture.
   */
  export function arch(): str;
  /**
   * Returns current process id.
   */
  export function pid(): i32;
  /**
   * Sleeps current thread for milliseconds.
   */
  export function sleep(ms: f64): void;
  /**
   * Aborts execution with an exit code signal.
   */
  export function exit(code?: i32): never;
  /**
   * Returns wall clock time in milliseconds.
   */
  export function clock_now(): f64;

  const _default: {
    args(): Array<str> | str;
    cwd(): str;
    chdir(path: str): void;
    env_get(name: str): str | undefined;
    env_set(name: str, value: str): void;
    platform(): str;
    arch(): str;
    pid(): i32;
    sleep(ms: f64): void;
    exit(code?: i32): never;
    clock_now(): f64;
  };
  export default _default;
}
