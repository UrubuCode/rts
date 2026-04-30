import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Object destructuring em parametro com tipos numericos
function sumXY({ x, y }: { x: number; y: number }): number {
  return x + y;
}
print(`${sumXY({ x: 3, y: 4 })}`);

// Array destructuring em parametro
function pair([a, b]: number[]): number {
  return a * b;
}
print(`${pair([5, 6])}`);

// Default value em property destructured
function withDefault({ x = 10 }: { x?: number }): number {
  return x * 2;
}
print(`${withDefault({ x: 7 })}`);
print(`${withDefault({})}`);

// Default value em array destructured
function withArrDefault([a, b = 100]: number[]): number {
  return a + b;
}
print(`${withArrDefault([3])}`);
print(`${withArrDefault([3, 7])}`);

// Multiplos parametros, alguns destructured outros nao
function mixed(prefix: number, { x, y }: { x: number; y: number }): number {
  return prefix + x + y;
}
print(`${mixed(100, { x: 1, y: 2 })}`);

// Class method com destructuring em parametro
class Calc {
  add({ x, y }: { x: number; y: number }): number {
    return x + y;
  }
}
const c = new Calc();
print(`${c.add({ x: 8, y: 9 })}`);

describe("destructuring_params", () => {
  test("all", () => expect(__rtsCapturedOutput).toBe("7\n30\n14\n20\n103\n10\n103\n17\n"));
});
