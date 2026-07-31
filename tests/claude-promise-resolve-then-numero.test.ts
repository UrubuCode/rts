import { describe, test, expect } from "rts:test";
import { promise } from "rts";

// `Promise.resolve(7).then(v => …)` entregava LIXO ao callback:
// `4619567317775286272` — que são exatamente os bits de f64 de `7.0`.
//
// Só NÚMERO corrompia; string, bool, objeto e array passavam certo. E o MESMO
// valor lido por `await` vinha correto:
//
//   await Promise.resolve(7)            // 7      ✅
//   Promise.resolve(7).then(v => v)     // lixo   ❌
//
// A assimetria denuncia a causa. Um valor SEM handle (número/bool/null) tem a
// própria WORD como valor, mas era gravado num slot marcado como NÃO-word, e a
// entrega do `.then` repassava os bits crus. O `await` escapava porque
// `normalize_settled_i64` reconverte por heurística (bits de f64 → inteiro).
//
// Correção: valor sem handle é gravado com `resolve_word`, que marca o slot — aí
// a entrega reboxa direito. Valor COM handle segue pelo caminho legado, que já
// trata passthrough de Promise existente e adoção de thenable.
//
// `promise.wait` força o drain da microtask (mesmo padrão de
// `promise_microtask_order.test.ts`), tornando o teste determinístico.

const inteiro = promise.wait(Promise.resolve(7).then(function (v) { return v; }));
const negativo = promise.wait(Promise.resolve(-42).then(function (v) { return v; }));
const zero = promise.wait(Promise.resolve(0).then(function (v) { return v; }));
// o valor tem de ser USÁVEL, não só transportado
const dobrado = promise.wait(Promise.resolve(5).then(function (v) { return v * 2; }));
const encadeado = promise.wait(
  Promise.resolve(3)
    .then(function (v) { return v + 1; })
    .then(function (v) { return v * 10; }),
);
// não-regressão: o caminho do `await` sobre o mesmo valor
async function viaAwait() { return await Promise.resolve(11); }
const doAwait = promise.wait(viaAwait().then(function (v) { return v; }));

describe("Promise.resolve(num).then entrega o número, não os bits", () => {
  test("inteiro", () => expect(inteiro).toBe(7));
  test("negativo", () => expect(negativo).toBe(-42));
  test("zero", () => expect(zero).toBe(0));
  test("o valor é USÁVEL em aritmética", () => expect(dobrado).toBe(10));
  test("encadeamento de dois then", () => expect(encadeado).toBe(40));
});

describe("não-regressões", () => {
  test("await sobre Promise.resolve continua correto", () => expect(doAwait).toBe(11));
});
