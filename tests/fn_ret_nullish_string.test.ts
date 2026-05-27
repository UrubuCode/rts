import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#374) fn/metodo que retorna `expr ?? "fallback"` ou `expr || "fallback"`
// deve inferir retorno string. Antes inferia Number e o handle voltava cru.
class LT {
  static #table: Map<number, string>;
  static { LT.#table = new Map([[1, "one"], [3, "three"]]); }
  static toWord(n: number) { return LT.#table.get(n) ?? "unknown"; }
}
function pick(s: string) { return s || "default"; }

print("a=" + LT.toWord(3));
print("b=" + LT.toWord(99));
print("c=" + pick(""));
print("d=" + pick("x"));

describe("fn ret nullish/or string (#374)", () => {
  test("`?? str` / `|| str` infere retorno string", () =>
    expect(out).toBe("a=three\nb=unknown\nc=default\nd=x\n"));
});
