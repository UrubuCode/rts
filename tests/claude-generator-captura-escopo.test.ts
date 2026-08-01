import { describe, test, expect } from "rts:test";

// A lista de CAPTURAS de um generator levantado (`__genexpr_N`) era calculada
// por uma varredura PLANA: um único conjunto `ligados` alimentado por todo
// `visit_param` / `visit_var_declarator` / `visit_fn_decl` da subárvore, funções
// aninhadas incluídas. Um PARÂMETRO de função aninhada portanto "ligava" o nome
// EXTERNO que ele apenas sombreia, e esse nome saía da lista de captura.
//
// Como a captura vira parâmetro do levantado e argumento do wrapper, um nome que
// falta não é otimização perdida — é um nome que deixa de resolver depois do
// hoist. Medido num bundle real do WhatsApp Web (`WAWebRunInBatches`):
//
//   var e;
//   function s(){ return gen(function*(){
//       yield 1, yield (e = function (e) { return 1 })   // param `e` sombreia
//   }) }
//
// A varredura agora tem PILHA DE ESCOPOS: um ligador só cobre usos dentro da
// função que o introduz. O resultado é um superconjunto do anterior (uma
// varredura plana só pode ligar DEMAIS), então a mudança pode acrescentar
// capturas e nunca remover uma.
//
// Todos os valores conferidos contra o Node. Pré-computado no top-level.

// ── 1. Captura só-LEITURA sombreada por parâmetro de fn aninhada ──────────────
// Antes: `ReferenceError: k is not defined`. Node: "7 16".
function leituraSombreada() {
  var k = 7;
  function s() {
    return (function* () {
      const a = yield k;
      yield a + (function (k) { return k * 2; })(3);
    })();
  }
  const it = s();
  const first = it.next().value;
  const second = it.next(10).value;
  return "" + first + " " + second;
}

// ── 2. O sombreamento não pode vazar: o parâmetro interno ganha do externo ────
// Node: "9 4" — o `n` de dentro é o argumento 2, não o 9 de fora.
function sombraGanhaDentro() {
  var n = 9;
  function s() {
    return (function* () {
      yield n;
      yield (function (n) { return n * 2; })(2);
    })();
  }
  const it = s();
  return "" + it.next().value + " " + it.next().value;
}

// ── 3. Sombreamento por `var` de fn aninhada (não só por parâmetro) ───────────
// Node: "5 11".
function sombraPorVarInterna() {
  var v = 5;
  function s() {
    return (function* () {
      yield v;
      yield (function () { var v = 11; return v; })();
    })();
  }
  const it = s();
  return "" + it.next().value + " " + it.next().value;
}

// ── 4. Sombreamento por parâmetro REST e por parâmetro de arrow ───────────────
// Node: "3 8 12".
//
// A forma por DESTRUCTURING (`function ({ d }) { … }` / `function ([d]) { … }`)
// ficou de fora de propósito: um parâmetro destructurado não sombreia o nome
// externo NO MOTOR, e isso é independente deste caminho e anterior a ele —
// `var d = 3; (function ({ d }) { return d; })({ d: 8 })` devolve `3` onde o
// Node devolve `8`, sem generator nenhum envolvido. Volta quando aquilo for
// corrigido.
function sombraPorRestEArrow() {
  var d = 3;
  function s() {
    return (function* () {
      yield d;
      yield (function (...d) { return d[0]; })(8);
      yield ((d) => d)(12);
    })();
  }
  const it = s();
  return "" + it.next().value + " " + it.next().value + " " + it.next().value;
}

// ── 5. `catch (x)` sombreia, e o nome externo continua capturado ──────────────
// Node: "6 boom".
function sombraPorCatch() {
  var x = 6;
  function s() {
    return (function* () {
      yield x;
      yield (function () { try { throw "boom"; } catch (x) { return x; } })();
    })();
  }
  const it = s();
  return "" + it.next().value + " " + it.next().value;
}

// ── 6. Duas capturas, uma sombreada e outra não, na ordem de primeiro uso ─────
// Node: "1 2 30".
function duasCapturas() {
  var a = 1;
  var b = 2;
  function s() {
    return (function* () {
      yield a;
      yield b;
      yield (function (a) { return a * b; })(15);
    })();
  }
  const it = s();
  return "" + it.next().value + " " + it.next().value + " " + it.next().value;
}

const r1 = leituraSombreada();
const r2 = sombraGanhaDentro();
const r3 = sombraPorVarInterna();
const r4 = sombraPorRestEArrow();
const r5 = sombraPorCatch();
const r6 = duasCapturas();

describe("generator levantado: captura com escopo", () => {
  test("captura só-leitura sombreada por parâmetro de fn aninhada", () => {
    expect(r1).toBe("7 16");
  });
  test("o parâmetro interno sombreia de fato o nome externo", () => {
    expect(r2).toBe("9 4");
  });
  test("`var` de função aninhada sombreia sem apagar a captura", () => {
    expect(r3).toBe("5 11");
  });
  test("parâmetro rest e parâmetro de arrow sombreiam sem apagar a captura", () => {
    expect(r4).toBe("3 8 12");
  });
  test("parâmetro de `catch` sombreia sem apagar a captura", () => {
    expect(r5).toBe("6 boom");
  });
  test("duas capturas, uma delas sombreada", () => {
    expect(r6).toBe("1 2 30");
  });
});
