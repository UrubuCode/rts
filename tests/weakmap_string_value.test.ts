import { describe, test, expect } from "rts:test";

let out = "";

// (#217) WeakMap.get() retorna Handle (nao I64) — string values funcionam
// em template literal sem mostrar handle bruto.
const wm = new WeakMap();
const k1 = { id: 1 };
const k2 = { id: 2 };

wm.set(k1, "alpha");
wm.set(k2, "beta");
out += wm.get(k1) + "\n";   // alpha
out += wm.get(k2) + "\n";   // beta

// has/delete continuam funcionando.
out += (wm.has(k1) ? "y" : "n") + "\n";   // y
wm.delete(k1);
out += (wm.has(k1) ? "y" : "n") + "\n";   // n

describe("weakmap_string_value", () => {
  test("WeakMap.get retorna Handle (#217)", () =>
    expect(out).toBe("alpha\nbeta\ny\nn\n"));
});
