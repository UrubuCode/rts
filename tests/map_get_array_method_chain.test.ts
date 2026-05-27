import { describe, test, expect } from "rts:test";

// Regression (cross-runtime #222): `m.get(k).join(",")` (e outros array
// methods) chamado DIRETO sobre o resultado de Map.get crashava com SIGILL.
// Causa: `m.get(k)` retorna i64 AMBIGUO (handle do array, nao ValTy::Handle),
// entao o `.join` nao tentava lower_array_builtin e caia no fallback
// chain-Map -> MAP_GET("join") + trapz -> SIGILL. Var intermediaria
// funcionava. Fix: receiver Call com tipo i64/u64 + array method conhecido
// tenta lower_array_builtin com o recv coerido antes do fallback.

const m = new Map<string, string[]>();
m.set("k", ["x", "y", "z"]);
const r1 = m.get("k").join(",");
const r2 = m.get("k").slice(1).join("-");
const r3 = m.get("k").includes("y");
const r4 = m.get("k").indexOf("z");

const mn = new Map<string, number[]>();
mn.set("n", [10, 20, 30]);
const r5 = mn.get("n").join("|");
const r6 = mn.get("n").reverse().join(",");

describe("map.get array method chain", () => {
  test("join direto", () => expect(r1).toBe("x,y,z"));
  test("slice().join()", () => expect(r2).toBe("y-z"));
  test("includes", () => expect(r3).toBe(true));
  test("indexOf", () => expect("" + r4).toBe("2"));
  test("num join", () => expect(r5).toBe("10|20|30"));
  test("reverse().join()", () => expect(r6).toBe("30,20,10"));
});
