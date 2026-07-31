import { describe, test, expect } from "rts:test";

// Um generator alcançado por ALIAS (`const g = gg`) perdia o protocolo nos
// caminhos de ITERAÇÃO — `[...g()]` e `for (const x of g())` produziam NADA,
// silenciosamente, enquanto `[...gg()]` funcionava.
//
// `sigs` é indexado pelo nome DECLARADO, então `sigs.get("g")` não acha nada e a
// chamada parece uma função comum. `gen_call_kind` (o caminho de `.next()`) já
// seguia `generator_aliases`; os caminhos de iteração (`try_lazy_gen_source_word`
// em loops.rs, e a prova de `ret_array` em globals.rs) não seguiam.
//
// Importa muito mais do que parece: o parser reescreve TODA generator EXPRESSION
// para `__genexpr_N` + `const g = __genexpr_N`, então `const g = function*(){…}`
// caía exatamente nisso — e é a forma que aparece em bundle minificado.
//
// Valores conferidos contra o Node. Pré-computado no top-level.

// ── alias explícito ────────────────────────────────────────────────────────
function* eagerDecl() { yield* [1, 2]; }
const aliasEager = eagerDecl;
const spreadViaAlias = [...aliasEager()].join(",");

function* doisValores() { yield 1; yield 2; }
const aliasDois = doisValores;
let somaForOf = 0;
for (const x of aliasDois()) { somaForOf = somaForOf + x; }

// ── generator EXPRESSION (vira alias no parser) ────────────────────────────
const exprEager = function* () { yield* [3, 4]; };
const spreadExpr = [...exprEager()].join(",");

const exprLazy = function* () { let i = 1; while (i <= 3) { yield i; i = i + 1; } };
const spreadLazy = [...exprLazy()].join(",");
const nextLazy = exprLazy().next().value;

let somaLazyForOf = 0;
for (const x of exprLazy()) { somaLazyForOf = somaLazyForOf + x; }

// ── CADEIA de alias (`const b = a` onde `a` já é alias) ────────────────────
// O registro do alias resolvia o alvo por `sigs.get` direto, e um alias não está
// em `sigs` — então o segundo elo da cadeia parava de ser reconhecido.
const aliasDeAlias = aliasEager;
const spreadCadeia = [...aliasDeAlias()].join(",");

// ── o caminho de `.next()` por alias não pode regredir ─────────────────────
const nextViaAlias = aliasDois().next().value;

// ── declaração direta não pode regredir ────────────────────────────────────
const spreadDireto = [...eagerDecl()].join(",");
let somaDireta = 0;
for (const x of doisValores()) { somaDireta = somaDireta + x; }

describe("generator por alias mantém o protocolo na iteração", () => {
  test("spread via alias explícito", () => {
    expect(spreadViaAlias).toBe("1,2");
  });

  test("for-of via alias explícito", () => {
    expect(somaForOf).toBe(3);
  });

  test("spread de generator EXPRESSION (eager)", () => {
    expect(spreadExpr).toBe("3,4");
  });

  test("spread de generator EXPRESSION (lazy)", () => {
    expect(spreadLazy).toBe("1,2,3");
  });

  test("for-of de generator EXPRESSION (lazy)", () => {
    expect(somaLazyForOf).toBe(6);
  });

  test(".next() de generator EXPRESSION (lazy)", () => {
    expect(nextLazy).toBe(1);
  });

  test("cadeia de alias resolve até o generator real", () => {
    expect(spreadCadeia).toBe("1,2");
  });
});

describe("não-regressões", () => {
  test(".next() via alias continua funcionando", () => {
    expect(nextViaAlias).toBe(1);
  });

  test("spread da declaração direta", () => {
    expect(spreadDireto).toBe("1,2");
  });

  test("for-of da declaração direta", () => {
    expect(somaDireta).toBe(3);
  });
});
