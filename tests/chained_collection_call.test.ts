import { describe, test, expect } from "rts:test";

// (#394) Method-call em receiver que eh uma Call entre parens/cast:
// `(m.get(k) as Set).has(v)` / `m.get(k).has(v)`. Antes:
//  - `(...).has(v)` (Paren/TsAs envolvendo Call) nao batia em nenhum branch
//    Call-receiver -> path que deixava block dangling -> Cranelift verifier
//    error "invalid block reference".
//  - `m.get(k).has(v)` onde o valor eh Set procurava um method-handle "has"
//    no Set -> trapz.
// Agora: peel de Paren/TsAs/TsNonNull no receiver + dispatch SET_HAS/MAP_GET
// quando o receiver eh `.get/.at/.pop/.shift` de colecao.
const m = new Map<string, Set<number>>([["k", new Set([1, 2, 3])]]);

const hasDirect = m.get("k")!.has(2);          // true
const missDirect = m.get("k")!.has(99);        // false
const hasCast = (m.get("k") as Set<number>).has(3);   // true
const hasParen = (m.get("k"))!.has(1);         // true

// chain de operator em classe NAO deve ser interceptado (regressao guard).
class Acc {
  v: number;
  constructor(v: number) { this.v = v; }
  add(n: number): Acc { return new Acc(this.v + n); }
}
const chained = new Acc(0).add(5).add(3).v;    // 8

describe("chained_collection_call (#394)", () => {
  test("m.get(k).has(2)", () => expect(hasDirect).toBe(true));
  test("m.get(k).has(99)", () => expect(missDirect).toBe(false));
  test("(m.get(k) as Set).has(3)", () => expect(hasCast).toBe(true));
  test("(m.get(k)).has(1)", () => expect(hasParen).toBe(true));
  test("operator chain de classe intacto", () => expect(chained).toBe(8));
});
