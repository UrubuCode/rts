import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Computed method name com literal string: `["foo"]() {}` ≡ `foo() {}`

class C {
    ["greet"](): string {
        return "hello";
    }
    ["double"](n: number): number {
        return n * 2;
    }
}

const c = new C();
print(c.greet()); // hello
print(`${c.double(7)}`); // 14

describe("fixture:computed_method_str", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("hello\n14\n");
  });
});
