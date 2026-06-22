import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Sem ctor explícito: initializers ainda rodam

class C {
    n: number = 100;
    m: number = 200;
}

const c = new C();
print(`${c.n}`); // 100

print(`${c.m}`); // 200

describe("fixture:property_init_no_ctor", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("100\n200\n");
  });
});
