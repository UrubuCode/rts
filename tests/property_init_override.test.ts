import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Ctor pode sobrescrever initializer (initializer roda primeiro)

class C {
    n: number = 1;

    constructor(arg: number) {
        // initializer rodou: this.n = 1 antes desta linha
        this.n = arg;  // sobrescreve
    }
}

const c = new C(99);
const h = gc.string_from_i64(c.n);
print(h); // 99
gc.string_free(h);

describe("fixture:property_init_override", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("99\n");
  });
});
