import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Multiplos decorators na mesma classe — executam em ordem inversa
// (TS: bottom-up). Aqui validamos a ordem de execucao.

function first(target: any): any {
  print("first");
  return target;
}

function second(target: any): any {
  print("second");
  return target;
}

function third(target: any): any {
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
