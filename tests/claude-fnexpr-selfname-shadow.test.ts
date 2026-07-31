import { describe, test, expect } from "rts:test";

// Fn-expression NOMEADA cujo corpo re-declara o PRÓPRIO nome: o `var`/`let`
// local sombreia o self-name para a função inteira, então toda referência
// interna é ao local — não à função.
//
// O bug: o lifter (`funcval::try_extract`) renomeava o self-name para o nome
// sintetizado (`__rtsn_arrow_N`) em TODO o corpo, sem respeitar a sombra. Um
// `t = e` interno virava atribuição ao nome liftado → "assignment to unbound
// `__rtsn_arrow_N`", e o script inteiro morria na compilação.
//
// Padrão real: bundle minificado da Meta (bootstrap do WhatsApp Web) —
// `function t(e){var t; … ((t=n(e))==null?void 0:t.asyncCss)}` aninhada em
// IIFE. Minificadores reciclam nomes de 1 letra o tempo todo.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

// ── aninhada: var sombreia o self-name e recebe atribuição ─────────────────
function externa1(): number {
  function t(e: any): any { var t; t = e; return t; }
  return t(7);
}
const sombraVar = externa1();

// ── a forma exata do bundle: atribuição DENTRO de expressão ────────────────
function externa2(): any {
  function t(e: any): any {
    var t;
    return (t = e) == null ? undefined : t.tag;
  }
  return t({ tag: "ok" });
}
const sombraExpr = externa2();

// ── sombra com inicializador ───────────────────────────────────────────────
function externa3(): number {
  const g = function t(e: number): number { var t = 0; t = e * 2; return t; };
  return g(21);
}
const sombraInit = externa3();

// ── SEM sombra, o self-name segue amarrando recursão ───────────────────────
const fat = function fact(n: number): number { return n < 2 ? 1 : n * fact(n - 1); };
const recursao = fat(5);

describe("self-name de fn-expr sombreado por var local", () => {
  test("var t; t = e dentro de function t", () => {
    expect(sombraVar).toBe(7);
  });

  test("atribuição ao shadow dentro de expressão (padrão minificado)", () => {
    expect(sombraExpr).toBe("ok");
  });

  test("shadow com inicializador", () => {
    expect(sombraInit).toBe(42);
  });

  test("sem sombra, recursão pelo self-name continua", () => {
    expect(recursao).toBe(120);
  });
});
