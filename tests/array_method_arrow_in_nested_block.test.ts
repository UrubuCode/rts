import { describe, test, expect } from "rts:test";

// (cross-runtime #369) arr.map/filter/reduce com arrow INLINE dentro de
// bloco aninhado (if / try-catch / loop). Antes do fix, os passes de lift
// e rewrite de array methods so' desciam em statements top-level (Expr/
// Decl/Return), nao em blocos aninhados. O arrow inline em `.map` dentro
// de if/catch/loop nao era liftado, e o codegen do `.map(arrowInline)` em
// bloco aninhado corrompia o fluxo de controle (merge/after block orfao ->
// "invalid block reference block2" no verifier do Cranelift).
//
// Os casos abaixo computam o resultado num `return` dentro do bloco
// aninhado (consumido imediatamente, sem assign a var externa promovida a
// global — esse subcaso assign-em-global continua sendo follow-up).

function mapInIf(): string {
  if (true) {
    return [1, 2, 3].map(x => x * 2).join(",");
  }
  return "";
}

function filterInIf(): string {
  if (1 < 2) {
    return [1, 2, 3, 4, 5, 6].filter(x => x % 2 === 0).join(",");
  }
  return "";
}

function mapInCatch(): string {
  try {
    throw new Error("boom");
  } catch (e: any) {
    return [10, 20, 30].map(x => x + 1).join(",");
  }
}

function reduceInIf(): number {
  if (true) {
    return [1, 2, 3, 4].reduce((acc, x) => acc + x, 0);
  }
  return -1;
}

const a = mapInIf();
const b = filterInIf();
const c = mapInCatch();
const e = reduceInIf();

describe("array method arrow in nested block (#369)", () => {
  test("map in if", () => expect(a).toBe("2,4,6"));
  test("filter in if", () => expect(b).toBe("2,4,6"));
  test("map in catch", () => expect(c).toBe("11,21,31"));
  test("reduce in if", () => expect(`${e}`).toBe("10"));
});
