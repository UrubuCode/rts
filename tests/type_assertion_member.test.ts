import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Assertion no receiver de chamada: `(c as Counter).method()`.
// Usado quando o tipo estático foi perdido (any/unknown) e queremos
// rotear via classe específica.

class Counter {
    n: number = 0;
    bump(): number {
        this.n = this.n + 1;
        return this.n;
    }
}

const c = new Counter();
// Roundtrip via assertion (no-op) — exercita o passthrough.
const v = (c as Counter).bump();
print(`${v}`); // 1

// Cadeia: assertion + assertion.
const v2 = ((c as Counter) as Counter).bump();
print(`${v2}`); // 2

describe("fixture:type_assertion_member", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n");
  });
});
