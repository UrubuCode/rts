import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator como metodo static.

class Util {
  static *range(n: i64) {
    for (let i = 0; i < n; i = i + 1) {
      yield i;
    }
  }
}

for (const v of Util.range(4)) {
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

describe("fixture:generator_static", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n2\n3\n");
  });
});
