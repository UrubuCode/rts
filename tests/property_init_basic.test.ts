import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Initializer básico: ctor explícito + initializers

class C {
    n: number = 42;
    m: number = 7;

    constructor() {
        // ctor vazio — initializers devem rodar
    }
}

const c = new C();
const h1 = gc.string_from_i64(c.n);
print(h1); // 42
gc.string_free(h1);

const h2 = gc.string_from_i64(c.m);
print(h2); // 7
gc.string_free(h2);

describe("fixture:property_init_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n7\n");
  });
});
