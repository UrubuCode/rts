import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 6 campos com initializers em ordem; cada um depende do anterior.

class C {
    a: number = 1;
    b: number = 2;
    c: number = 3;
    d: number = 4;
    e: number = 5;
    f: number = 0; // sera atualizado no ctor

    constructor() {
        this.f = this.a + this.b + this.c + this.d + this.e; // 15
    }
}

const x = new C();
const h = gc.string_from_i64(x.f);
print(h);
gc.string_free(h);

describe("fixture:property_init_many", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("15\n");
  });
});
