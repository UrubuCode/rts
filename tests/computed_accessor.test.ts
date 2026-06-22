import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Getter e setter com nome computed (literal)

class C {
    _v: number = 0;

    get ["x"](): number {
        return this._v;
    }

    set ["x"](n: number) {
        this._v = n * 2;
    }
}

const c = new C();
c.x = 7;             // setter → _v = 14
print(`${c.x}`); // getter → 14

describe("fixture:computed_accessor", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("14\n");
  });
});
