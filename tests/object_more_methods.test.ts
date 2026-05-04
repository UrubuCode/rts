import { describe, test, expect } from "rts:test";

let out = "";

const obj = { a: 1, b: 2 };

// seal — marca handle como sealed; isSealed retorna true depois.
const sealed = Object.seal(obj);
out += ((sealed as any).a + 0) + "\n";   // 1

// isFrozen — sealed nao implica frozen.
// isSealed — true apos seal()
out += (Object.isFrozen(obj) ? "y" : "n") + "\n";  // n
out += (Object.isSealed(obj) ? "y" : "n") + "\n";  // y

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
  test("seal/isFrozen/isSealed/getPrototypeOf/defineProperty", () => expect(out).toBe(
    "1\nn\ny\nhas-proto\n42\n"
  ));
});
