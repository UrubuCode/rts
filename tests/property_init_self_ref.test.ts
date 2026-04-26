import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Initializer referenciando field anterior via this.x.
// Ordem de execução = ordem de declaração.

class C {
    a: number = 10;
    b: number = 20;
    c: number = 999; // sera sobrescrito no ctor

    constructor() {
        this.c = this.a + this.b; // 30
    }
}

const c = new C();
const ha = gc.string_from_i64(c.a);
print(ha); gc.string_free(ha);

const hb = gc.string_from_i64(c.b);
print(hb); gc.string_free(hb);

const hc = gc.string_from_i64(c.c);
print(hc); gc.string_free(hc);

describe("fixture:property_init_self_ref", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n20\n30\n");
  });
});
