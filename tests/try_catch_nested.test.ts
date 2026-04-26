import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// try/catch aninhado: catch interno absorve, catch externo continua sem erro.

try {
    try {
        throw "inner";
    } catch (e) {
        print(`inner caught: ${e}`);
    }
    print("after inner");
} catch (e2) {
    print(`outer would catch: ${e2}`);  // não deve disparar
}

print("end");

describe("fixture:try_catch_nested", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("inner caught: inner\nafter inner\nend\n");
  });
});
