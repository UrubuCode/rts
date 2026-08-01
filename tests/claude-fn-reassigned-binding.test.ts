import { describe, test, expect } from "rts:test";

// Uma DECLARAÇÃO de função cria um binding GRAVÁVEL, como qualquer `let`.
// Reatribuí-lo é a memoização que todo async transpilado por Babel emite:
//
//   function v(a) { v = _asyncToGenerator(…); return v.apply(this, args) }
//
// O motor resolvia o nome pela tabela IMUTÁVEL de funções de topo, então a
// escrita não tinha onde pousar e o arquivo inteiro era recusado
// ("assignment to unbound `v`") — 3 dos 9 erros por carga do WhatsApp Web.
//
// Correção: `module_globals` promove a gcell uma função de topo cujo nome é
// ESCRITO (nunca por ser apenas lida — uma função comum mantém o caminho
// rápido de chamada direta), e o prólogo de `main` semeia a cell com a própria
// função, que é exatamente o hoisting de função do JS. Os três caminhos que
// resolviam pelo fn-table — chamada direta, `fn_value_word` e o receiver de
// `.call`/`.apply`/`.bind` — passaram a preferir a cell quando ela existe; sem
// o terceiro, `v.apply(...)` reificava o valor ORIGINAL e a memoização chamava
// a si mesma para sempre (medido: stack overflow).
//
// Valores conferidos contra Node e Bun (fixture cross-runtime
// tests/cross-runtime/fn-meta/421_function_reassigned_memoization.ts).

function make(): any {
  return function (a: any) { return "real:" + a; };
}

// ── a função troca a si mesma na primeira chamada ───────────────────────────
function v(a: any): any {
  v = make() as any;
  return (v as any).apply(null, [a]);
}
const self1 = v(1);
const self2 = v(2);

// ── outra função enxerga o valor NOVO, não uma captura do original ──────────
function chama(a: any): any { return (v as any).apply(null, [a]); }
const viaOutra = chama(3);

// ── reatribuição a partir do TOPO alcança quem já referenciava o nome ───────
function base(): any { return "orig"; }
function leBase(): any { return base(); }
const antes = leBase();
base = function (): any { return "trocada"; } as any;
const depois = leBase();

// ── hoisting: o binding vale acima da própria linha de declaração ───────────
const hoisted = antesDaDecl();
function antesDaDecl(): any { return "ok"; }

// ── inicialização preguiçosa: o corpo original roda UMA vez ─────────────────
let initCount = 0;
function lazy(n: any): any {
  initCount = initCount + 1;
  lazy = function (m: any): any { return m + 100; } as any;
  return (lazy as any)(n);
}
const lazy1 = lazy(1);
const lazy2 = lazy(2);

// ── uma função NUNCA reatribuída não muda de comportamento ──────────────────
function pura(x: any): any { return x * 2; }
const puraVal = pura(21);
const puraName = pura.name;

describe("função de topo reatribuída é binding gravável", () => {
  test("a função troca a si mesma na primeira chamada", () => {
    expect(self1).toBe("real:1");
    expect(self2).toBe("real:2");
  });
  test("outra função enxerga o valor novo", () => {
    expect(viaOutra).toBe("real:3");
  });
  test("reatribuição do topo alcança quem referencia o nome", () => {
    expect(antes).toBe("orig");
    expect(depois).toBe("trocada");
  });
  test("hoisting: chamável acima da própria declaração", () => {
    expect(hoisted).toBe("ok");
  });
  test("inicialização preguiçosa roda o corpo original uma vez só", () => {
    expect(lazy1).toBe(101);
    expect(lazy2).toBe(102);
    expect(initCount).toBe(1);
  });
  test("função não reatribuída segue no caminho rápido", () => {
    expect(puraVal).toBe(42);
    expect(puraName).toBe("pura");
  });
});
