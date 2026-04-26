import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Override de generator com virtual dispatch.

class Base {
  *vals() {
    yield 1;
    yield 2;
  }
}

class Derived extends Base {
  *vals() {
    yield 100;
    yield 200;
  }
}

const d: Base = new Derived();
for (const v of d.vals()) {
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

describe("fixture:generator_override", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("100\n200\n");
  });
});
