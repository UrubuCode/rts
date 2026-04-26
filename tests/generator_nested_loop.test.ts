import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator com loops aninhados.

function* pairs(n: i64, m: i64) {
  for (let i = 0; i < n; i = i + 1) {
    for (let j = 0; j < m; j = j + 1) {
      yield i * 10 + j;
    }
  }
}

for (const v of pairs(3, 2)) {
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

describe("fixture:generator_nested_loop", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n10\n11\n20\n21\n");
  });
});
