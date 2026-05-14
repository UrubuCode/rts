import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

const obj = { a: 1 };

// Key existente: descriptor com value/writable/enumerable/configurable
const desc = Reflect.getOwnPropertyDescriptor(obj, "a");
print("value=" + Reflect.get(desc, "value"));

// Key inexistente: spec JS retorna undefined (string "undefined" no RTS),
// NAO null/0 (era o comportamento antigo).
const descMissing = Reflect.getOwnPropertyDescriptor(obj, "nope");
print("missing=" + descMissing);

describe("Reflect.getOwnPropertyDescriptor (#795)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "value=1\n" +
      "missing=undefined\n"
    )
  );
});
