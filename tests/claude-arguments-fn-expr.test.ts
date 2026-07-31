import { describe, test, expect } from "rts:test";

// `arguments` numa função-EXPRESSÃO (`f = function(){ … arguments … }`).
//
// O motor já materializava o objeto (`argsobj.rs`: rest param sintético para fn
// de aridade zero, prólogo `let arguments = [p…]` quando há params declarados),
// mas só alcançava `Item::Function` de topo. Uma fn-expressão morria antes: o
// lifting de closures (`funcval`) via `arguments` como identificador livre
// desconhecido e abortava a extração.
//
// Duas correções destravaram, ambas onde o AST existe:
//   1. o `funcval` reconhece `arguments` como binding PRÓPRIO de uma função
//      não-arrow, não como captura do escopo externo;
//   2. o `argsobj` roda DE NOVO depois do lifting — uma fn-expr só existe como
//      `HirFunc` nesse ponto. O pass é idempotente.
//
// Para isso o HIR ganhou `HirExprKind::Arrow.is_real_arrow`: `Arrow` cobre TRÊS
// formas sintáticas (arrow, function-expression e `function` aninhada) e a
// diferença é semântica — uma arrow NÃO tem `arguments` próprio (lê o da função
// envolvente), uma fn-expr tem.
//
// Valores conferidos contra o Node. Pré-computado no top-level (regra do
// projeto: método dentro de test() pode perder handle pro GC).

// ── fn-expressão anônima, aridade zero ──────────────────────────────────────
const anonima = function () { return arguments.length; };
const anonimaN = anonima(1, 2, 3);

// ── fn-expressão NOMEADA ────────────────────────────────────────────────────
const nomeada = function comNome() { return arguments.length; };
const nomeadaN = nomeada(1, 2);

// ── declaração de topo (já funcionava — não pode regredir) ─────────────────
function declarada() { return arguments.length; }
const declaradaN = declarada(1, 2, 3, 4);

// ── fn-expressão COM params declarados ─────────────────────────────────────
// Caminho diferente no `argsobj`: prólogo `let arguments = [a, b]` em vez de
// rest param sintético.
const comParams = function (a: any, b: any) { return arguments.length; };
const comParamsN = comParams(1, 2);

// ── o padrão que motivou tudo: loader de página ────────────────────────────
// `requireLazy = function(){ stub.push(arguments) }` — a forma exata que o
// bootstrap da Meta usa, e que antes obrigava o DOM a reescrever o texto.
const acumulado: any[] = [];
const loader = function () { acumulado.push(arguments); };
loader(["Modulo"], 256);
loader(["Outro"], 512);
const loaderN = acumulado.length;

// ── ler um argumento por índice, não só o length ───────────────────────────
const porIndice = function () { return arguments[1]; };
const porIndiceV = porIndice("a", "b", "c");

describe("`arguments` em função-expressão", () => {
  test("anônima de aridade zero vê todos os argumentos", () => {
    expect(anonimaN).toBe(3);
  });

  test("nomeada também", () => {
    expect(nomeadaN).toBe(2);
  });

  test("declaração de topo segue funcionando", () => {
    expect(declaradaN).toBe(4);
  });

  test("com params declarados", () => {
    expect(comParamsN).toBe(2);
  });

  test("padrão do loader: repassar arguments adiante", () => {
    expect(loaderN).toBe(2);
  });

  test("indexação de arguments", () => {
    expect(porIndiceV).toBe("b");
  });
});
