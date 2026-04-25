// Multiplos decorators na mesma classe — executam em ordem inversa
// (TS: bottom-up). Aqui validamos a ordem de execucao.
import { io } from "rts";

function first(target: i64): i64 {
  io.print("first");
  return target;
}

function second(target: i64): i64 {
  io.print("second");
  return target;
}

function third(target: i64): i64 {
  io.print("third");
  return target;
}

@first
@second
@third
class Stack {
  ping(): void { io.print("ping"); }
}

new Stack().ping();
