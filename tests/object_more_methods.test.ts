import { describe, test, expect } from "rts:test";

let out = "";

const obj = { a: 1, b: 2 };

// seal — v0 no-op, retorna handle
const sealed = Object.seal(obj);
out += ((sealed as any).a + 0) + "\n";   // 1

// isFrozen / isSealed — v0 sempre false
out += (Object.isFrozen(obj) ? "y" : "n") + "\n";  // n
out += (Object.isSealed(obj) ? "y" : "n") + "\n";  // n

// getPrototypeOf — sem __proto__ retorna 0
const proto = { greet: 0 };  // simplified — nao testa method em proto
const child = Object.create(proto);
const got = Object.getPrototypeOf(child);
out += (got !== 0 ? "has-proto" : "no-proto") + "\n";  // has-proto

// defineProperty com { value: x }
const target: { [k: string]: number } = {};
Object.defineProperty(target, "x", { value: 42 });
out += (target as any).x + "\n";  // 42

describe("object_more_methods", () => {
  test("seal/isFrozen/isSealed/getPrototypeOf/defineProperty (#208 v0)", () => expect(out).toBe(
    "1\nn\nn\nhas-proto\n42\n"
  ));
});
