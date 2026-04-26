import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator simples com yield literal.

function* gen() {
  yield 1;
  yield 2;
  yield 3;
}

for (const n of gen()) {
  const h = gc.string_from_i64(n);
  print(h); gc.string_free(h);
}

describe("fixture:generator_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n3\n");
  });
});
