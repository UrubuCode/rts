import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Itera keys de um objeto literal e lê valores por acesso computado `obj[key]`.
// (Antes usava o escape hatch `collections.map_get(obj, key)` do motor velho,
// onde objetos ERAM Maps; no motor novo objetos são shapes — `obj[key]` é a via
// canônica JS.)

const obj = { x: 10, y: 20, z: 30 };

for (const key in obj) {
    const val = obj[key];
    print(key + "=" + `${val}`);
}

describe("fixture:for_in_values", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("x=10\ny=20\nz=30\n");
  });
});
