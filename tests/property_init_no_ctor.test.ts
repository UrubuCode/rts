import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

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
const h1 = gc.string_from_i64(c.n);
print(h1); // 100
gc.string_free(h1);

const h2 = gc.string_from_i64(c.m);
print(h2); // 200
gc.string_free(h2);

describe("fixture:property_init_no_ctor", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("100\n200\n");
  });
});
