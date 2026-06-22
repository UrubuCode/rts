import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator herdado e proprio em subclasse.

class Base {
  *vals() {
    yield 1;
    yield 2;
  }
}

class Derived extends Base {
  *more() {
    yield 10;
    yield 20;
  }
}

const d = new Derived();
print("base:");
for (const v of d.vals()) {
  print(`${v}`);
}
print("derived:");
for (const v of d.more()) {
  print(`${v}`);
}

describe("fixture:generator_inherited", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("base:\n1\n2\nderived:\n10\n20\n");
  });
});
