import { io } from "rts";

const greet = function(name: string): void {
  io.print(`hello ${name}`);
};

const double = function doubleImpl(n: i32): i32 {
  return n * 2;
};

const triple = (n: i32): i32 => {
  return n * 3;
};

greet("world");
greet("RTS");
io.print(`double(5) = ${double(5)}`);
io.print(`triple(7) = ${triple(7)}`);
