import { io } from "rts";
import { str } from "rts";

function main(): void {
  // str.slice, str.index_of, str.starts_with, str.to_upper
  const msg = "hello world";

  io.print(str.slice(msg, 0, 5));       // "hello"
  io.print(str.slice(msg, 6, 11));      // "world"
  io.print(str.index_of(msg, "world")); // 6
  io.print(str.starts_with(msg, "hel")); // true
  io.print(str.to_upper(msg));          // "HELLO WORLD"
  io.print(str.replace(msg, "world", "rts")); // "hello rts"
  io.print(str.len(msg));               // 11
}
