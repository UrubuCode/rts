import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator como metodo de classe acessando this.

class Counter {
  base: i64 = 100;
  *bumps() {
    yield this.base + 1;
    yield this.base + 2;
    yield this.base + 3;
  }
}

const c = new Counter();
for (const v of c.bumps()) {
  const h = gc.string_from_i64(v);
  print(h); gc.string_free(h);
}

describe("fixture:generator_method", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("101\n102\n103\n");
  });
});
