import { io } from "rts";
import { str } from "rts";

function main(): void {
  const s = "foo bar foo baz foo";
  io.print(str.replace_all(s, "foo", "X"));       // "X bar X baz X"
  io.print(str.replace(s, "foo", "X"));           // "X bar foo baz foo" (so o primeiro)
  io.print(str.replace_all("aaaa", "a", "bb"));   // "bbbbbbbb"
  io.print(str.replace_all("hello", "x", "y"));   // "hello" (sem match)
}
