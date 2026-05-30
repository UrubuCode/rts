import { describe, test, expect } from "rts:test";

// (#275) `const r = yield* gen()` — yield* em posicao de init de VarDecl,
// capturando o ret_value (`return X`) do generator delegado. Antes o
// desugar deixava o yield cru (catch-all) e o codegen rejeitava
// ("unsupported expression: yield"). Agora reescreve para:
//   const __yt = inner(); for (const __t of __yt) __gen_buf.push(__t);
//   const r = __RTS_GEN_GET_RET(__yt);
// e transform_stmt devolve Vec<Stmt> (inline) p/ preservar escopo de `r`.
function* inner() { yield 1; yield 2; return 9; }
function* outer() { const r = yield* inner(); yield r + 1; return r + 2; }

const g = outer();
const s0 = g.next();
const s1 = g.next();
const s2 = g.next();
const s3 = g.next();
const steps =
  s0.value + ":" + s0.done + "|" +
  s1.value + ":" + s1.done + "|" +
  s2.value + ":" + s2.done + "|" +
  s3.value + ":" + s3.done;

// spread sobre generator com yield* delegando array/string.
function* multi() {
  yield 0;
  yield* [10, 11];
  yield* "ab";
}
const spread = [...multi()].join(",");

describe("generator_yield_star_return (#275)", () => {
  test("yield* captura ret_value do delegado", () =>
    expect(steps).toBe("1:false|2:false|10:false|11:true"));
  test("yield* delega array e string", () =>
    expect(spread).toBe("0,10,11,a,b"));
});
