import { io } from "rts";

const double = (x: i32): i32 => x * 2;
const sum = (a: i32, b: i32): i32 => a + b;
const greet = (name: string): string => `hello ${name}`;
const answer = (): i32 => 42;

const triple = (x: i32): i32 => {
  return x * 3;
};

io.print(`double(5) = ${double(5)}`);
io.print(`sum(3, 4) = ${sum(3, 4)}`);
io.print(greet("arrow"));
io.print(`answer = ${answer()}`);
io.print(`triple(7) = ${triple(7)}`);
