import { describe, test, expect } from "rts:test";
import { io } from "rts";

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
print(`${c.a}`);

print(`${c.b}`);

print(`${c.c}`);

describe("fixture:property_init_self_ref", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n20\n30\n");
  });
});
