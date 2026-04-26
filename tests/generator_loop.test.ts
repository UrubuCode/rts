import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator com yield dentro de loop.

function* range(start: i64, end: i64) {
  for (let i = start; i < end; i = i + 1) {
    yield i;
  }
}

for (const n of range(2, 6)) {
  const h = gc.string_from_i64(n);
  print(h); gc.string_free(h);
}

describe("fixture:generator_loop", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("2\n3\n4\n5\n");
  });
});
