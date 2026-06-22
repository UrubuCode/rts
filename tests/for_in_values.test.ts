import { describe, test, expect } from "rts:test";
import { io, collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Itera keys e usa map_get pra ler valores.

const obj = { x: 10, y: 20, z: 30 };

for (const key in obj) {
    // collections.map_get aceita string handle direto via codegen Handle→StrPtr
    const val = collections.map_get(obj, key);
    print(key + "=" + `${val}`);
}

describe("fixture:for_in_values", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("x=10\ny=20\nz=30\n");
  });
});
