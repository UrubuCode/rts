import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator override consumindo super.vals() via for-of.

class Base {
  *vals() {
    yield 1;
    yield 2;
  }
}

class Derived extends Base {
  *vals() {
    for (const v of super.vals()) {
      yield v * 10;
    }
    yield 99;
  }
}

const d = new Derived();
for (const v of d.vals()) {
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

describe("fixture:generator_super", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n20\n99\n");
  });
});
