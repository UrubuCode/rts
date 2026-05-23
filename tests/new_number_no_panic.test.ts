import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// `new Number(42)` agora retorna NumberBox (Entry::Handle) — typeof "object",
// stringification via Object.prototype.toString = "[object Object]" (RTS shim).
// valueOf() recupera o primitive. Bate spec ECMA Number wrapper.
const n = new Number(42);
print("n=" + n.valueOf());

const z = new Number(0);
print("z=" + z.valueOf());

// Verifica que tambem nao quebra outros usos comuns.
const arr = [new Number(1), new Number(2), new Number(3)];
print("arr_len=" + arr.length);

describe("new Number(x) preserves Cranelift type (panic fix)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "n=42\n" +
      "z=0\n" +
      "arr_len=3\n"
    )
  );
});
