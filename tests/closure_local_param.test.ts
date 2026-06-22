import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Captura de parâmetro da fn enclosing.

function makeBumper(start: number): void {
    let total: number = 0;
    const cb = () => {
        total = total + start;
    };
    cb();
    cb();
    cb();
    print(`${total}`); // 21 (start=7, 3x)
}

makeBumper(7);

describe("fixture:closure_local_param", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("21\n");
  });
});
