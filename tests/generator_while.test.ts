import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator com while loop.

function* fib_until(max: i64) {
  let a: i64 = 0;
  let b: i64 = 1;
  while (a < max) {
    yield a;
    const t = a + b;
    a = b;
    b = t;
  }
}

for (const n of fib_until(20)) {
  const h = gc.string_from_i64(n);
  print(h); gc.string_free(h);
}

describe("fixture:generator_while", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n1\n2\n3\n5\n8\n13\n");
  });
});
