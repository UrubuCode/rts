import { describe, test, expect } from "rts:test";
import { io } from "rts";

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
print(`${c.n}`); // 42

print(`${c.m}`); // 7

describe("fixture:property_init_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n7\n");
  });
});
