import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Spread literal em `new C(...args)` — desugar via expand_spread_args.

class Pair {
    a: number;
    b: number;
    constructor(a: number, b: number) {
        this.a = a;
        this.b = b;
    }
    sum(): number { return this.a + this.b; }
}

const p = new Pair(...[7, 13]);
print(`${p.sum()}`); // 20

describe("fixture:spread_new", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("20\n");
  });
});
