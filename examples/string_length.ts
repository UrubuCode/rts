import { io } from "rts";

function main(): void {
  const a = "hello";
  io.print(a.length);       // 5
  io.print("".length);      // 0
  io.print("rts".length);   // 3
}
