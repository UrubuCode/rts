declare module "rts:str" {
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
   * Returns the byte length of a string.
   */
  export function len(s: str): u64;
  /**
   * Concatenates two strings.
   */
  export function concat(a: str, b: str): str;
  /**
   * Returns a substring from start (inclusive) to end (exclusive). Negative indices count from end.
   */
  export function slice(s: str, start: i64, end?: i64): str;
  /**
   * Returns the string converted to uppercase.
   */
  export function to_upper(s: str): str;
  /**
   * Returns the string converted to lowercase.
   */
  export function to_lower(s: str): str;
  /**
   * Removes leading and trailing whitespace.
   */
  export function trim(s: str): str;
  /**
   * Removes leading whitespace.
   */
  export function trim_start(s: str): str;
  /**
   * Removes trailing whitespace.
   */
  export function trim_end(s: str): str;
  /**
   * Replaces the first occurrence of `from` with `to`.
   */
  export function replace(s: str, from: str, to: str): str;
  /**
   * Replaces all occurrences of `from` with `to`.
   */
  export function replace_all(s: str, from: str, to: str): str;
  /**
   * Returns true if the string contains the given substring.
   */
  export function includes(s: str, needle: str): bool;
  /**
   * Returns true if the string starts with the given prefix.
   */
  export function starts_with(s: str, prefix: str): bool;
  /**
   * Returns true if the string ends with the given suffix.
   */
  export function ends_with(s: str, suffix: str): bool;
  /**
   * Returns the byte index of the first occurrence of needle, or -1 if not found.
   */
  export function index_of(s: str, needle: str): i64;
  /**
   * Returns the byte index of the last occurrence of needle, or -1 if not found.
   */
  export function last_index_of(s: str, needle: str): i64;
  /**
   * Returns the UTF-8 character at the given char index as a str.
   */
  export function char_at(s: str, index: u64): str;
  /**
   * Splits the string by separator and returns parts joined by newline (use str.split_nth to access each part).
   */
  export function split(s: str, sep: str): str;
  /**
   * Returns the Nth part after splitting s by sep.
   */
  export function split_nth(s: str, sep: str, n: u64): str;
  /**
   * Returns the string repeated n times.
   */
  export function repeat(s: str, n: u64): str;
  /**
   * Pads the string at the start to reach target length.
   */
  export function pad_start(s: str, target_len: u64, fill?: str): str;
  /**
   * Pads the string at the end to reach target length.
   */
  export function pad_end(s: str, target_len: u64, fill?: str): str;
  /**
   * Returns the number of Unicode scalar values (chars) in the string.
   */
  export function char_count(s: str): u64;
  /**
   * Returns true if the string has zero length.
   */
  export function is_empty(s: str): bool;
  /**
   * Converts a number to its string representation.
   */
  export function from_number(n: f64): str;
  /**
   * Parses the string as an integer. Returns NaN (as f64) on failure.
   */
  export function parse_int(s: str, radix?: u64): f64;
  /**
   * Parses the string as a floating-point number. Returns NaN on failure.
   */
  export function parse_float(s: str): f64;

  const _default: {
    len(s: str): u64;
    concat(a: str, b: str): str;
    slice(s: str, start: i64, end?: i64): str;
    to_upper(s: str): str;
    to_lower(s: str): str;
    trim(s: str): str;
    trim_start(s: str): str;
    trim_end(s: str): str;
    replace(s: str, from: str, to: str): str;
    replace_all(s: str, from: str, to: str): str;
    includes(s: str, needle: str): bool;
    starts_with(s: str, prefix: str): bool;
    ends_with(s: str, suffix: str): bool;
    index_of(s: str, needle: str): i64;
    last_index_of(s: str, needle: str): i64;
    char_at(s: str, index: u64): str;
    split(s: str, sep: str): str;
    split_nth(s: str, sep: str, n: u64): str;
    repeat(s: str, n: u64): str;
    pad_start(s: str, target_len: u64, fill?: str): str;
    pad_end(s: str, target_len: u64, fill?: str): str;
    char_count(s: str): u64;
    is_empty(s: str): bool;
    from_number(n: f64): str;
    parse_int(s: str, radix?: u64): f64;
    parse_float(s: str): f64;
  };
  export default _default;
}
