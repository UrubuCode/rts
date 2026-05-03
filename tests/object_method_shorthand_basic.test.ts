import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#261) Method shorthand em object literal eh desugared para
// FnExpr e hoisted. Chamada via obj.method() roteada por
// MAP_GET_CHAIN + call_indirect.

const o: any = {
  greet(): number { return 42; },
  add(a: number, b: number): number { return a + b; },
};

const r1: number = o.greet();
const r2: number = o.add(3, 4);
print("greet=" + r1);
print("add=" + r2);

describe("method shorthand basico (#261)", () => {
  test("greet sem args; add com args", () =>
    expect(__rtsCapturedOutput).toBe(
      "greet=42\n" +
      "add=7\n"
    ));
});
