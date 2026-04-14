import { io, process } from "rts";

function main(): void {
  let i: number = 0;
  while (i < 1000) {
    const result = process.arch() + ":" + process.arch();
    if (i % 100 == 0) {
      io.print(i + " => " + result);
    }
    i = i + 1;
  }
  io.print("done");
}
