import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `private` keyword: acesso permitido só dentro do corpo da classe.

class C {
    private n: number;
    constructor() {
        this.n = 0;
    }
    bump(): void {
        this.n = this.n + 5;
    }
    value(): number {
        return this.n;
    }
}

const c = new C();
c.bump();
c.bump();
const h = gc.string_from_i64(c.value());
print(h); gc.string_free(h); // 10

describe("fixture:private_modifier_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n");
  });
});
