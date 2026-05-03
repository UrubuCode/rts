import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #432: cadeias `a?.b?.c` com qualquer link null curto-circuitam corretamente
// porque o resultado intermediario eh 0 e o proximo `?.` testa == 0.

const o1: { a: { b: { c: number } } } | null = null;
print(`r1=${o1?.a?.b?.c}`);

const o2: { a: { b: { c: number } } | null } = { a: null };
print(`r2=${o2?.a?.b?.c}`);

// Cadeia completa nao-null
const o3 = { a: { b: { c: 99 } } };
print(`r3=${o3?.a?.b?.c}`);

describe("?. cadeia com null intermediario (#432)", () => {
  test("curto-circuito em qualquer null da cadeia", () => {
    expect(__rtsCapturedOutput).toBe(
      "r1=undefined\n" +
      "r2=undefined\n" +
      "r3=99\n"
    );
  });
});
