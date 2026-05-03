import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Symbol.for — mesma chave = mesmo handle
const r1 = Symbol.for("key1");
const r2 = Symbol.for("key1");
print((r1 === r2) ? "for-same" : "for-different");

// Symbol.keyFor — registrado retorna chave
const k = Symbol.keyFor(r1);
print(k);

// Symbol.for diferentes chaves = handles diferentes
const r3 = Symbol.for("other");
print((r1 !== r3) ? "different" : "same");

describe("symbol_v0", () => {
  test("acceptance #216 (subset v0)", () => expect(out).toBe(
    "for-same\nkey1\ndifferent\n"
  ));
});
