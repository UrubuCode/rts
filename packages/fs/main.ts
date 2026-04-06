import { fs, io } from "rts";

export type Result<T> = io.Result<T>;

export function read_to_string(path: string): io.Result<string> {
  return fs.read_to_string(path);
}

export function read(path: string): io.Result<string> {
  return fs.read(path);
}

export function write(path: string, data: string): io.Result<void> {
  return fs.write(path, data);
}

export function is_ok<T>(result: io.Result<T>): boolean {
  return Boolean(io.is_ok(result));
}

export function is_err<T>(result: io.Result<T>): boolean {
  return Boolean(io.is_err(result));
}

export function unwrap_or<T>(result: io.Result<T>, fallback: T): T {
  return io.unwrap_or(result, fallback);
}
