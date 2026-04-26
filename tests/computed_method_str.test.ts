import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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
const h = gc.string_from_i64(c.double(7));
print(h); gc.string_free(h); // 14

describe("fixture:computed_method_str", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("hello\n14\n");
  });
});
