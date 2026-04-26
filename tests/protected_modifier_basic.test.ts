import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `protected` permite acesso em subclasses.

class Base {
    protected x: number = 10;
}

class Sub extends Base {
    triple(): number {
        return this.x * 3; // OK: Sub estende Base
    }
}

const s = new Sub();
const h = gc.string_from_i64(s.triple());
print(h); gc.string_free(h); // 30

describe("fixture:protected_modifier_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("30\n");
  });
});
