import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator com yield condicional (so pares).

function* evens(n: i64) {
  for (let i = 0; i < n; i = i + 1) {
    if (i % 2 == 0) {
      yield i;
    }
  }
}

for (const v of evens(8)) {
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

describe("fixture:generator_conditional", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n2\n4\n6\n");
  });
});
