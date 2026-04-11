import { io } from "rts";

function describe(count: number): string {
  return "count = " + count;
}

function main(): void {
  io.print(describe(42));
}
