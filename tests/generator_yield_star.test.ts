import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `yield*` delega para outro iteravel.

function* inner() {
  yield 1;
  yield 2;
}

function* outer() {
  yield 0;
  yield* inner();
  yield 3;
}

for (const v of outer()) {
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

describe("fixture:generator_yield_star", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n1\n2\n3\n");
  });
});
