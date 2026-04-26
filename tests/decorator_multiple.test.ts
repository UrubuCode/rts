import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Multiplos decorators na mesma classe — executam em ordem inversa
// (TS: bottom-up). Aqui validamos a ordem de execucao.

function first(target: i64): i64 {
  print("first");
  return target;
}

function second(target: i64): i64 {
  print("second");
  return target;
}

function third(target: i64): i64 {
  print("third");
  return target;
}

@first
@second
@third
class Stack {
  ping(): void { print("ping"); }
}

new Stack().ping();

describe("fixture:decorator_multiple", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("third\nsecond\nfirst\nping\n");
  });
});
