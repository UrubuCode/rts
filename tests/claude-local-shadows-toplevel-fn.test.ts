import { describe, test, expect } from "rts:test";

// Um LOCAL sombreia uma função de topo de mesmo nome — escopo léxico básico do
// JS. O lifter de closures decidia "esse nome livre é a função de topo
// homônima" ANTES de consultar o escopo, e descartava a captura: a função
// aninhada passava a ler a FUNÇÃO em vez do local, e a escrita nunca chegava ao
// local.
//
// O que torna isso sério: NÃO era um bail. Saía um valor errado EM SILÊNCIO
// (`sess-undefined` onde Node/Bun dão `sess-ID:Env`) — verificado revertendo o
// fix. Um minificador reusa `e`/`t`/`n`/`r` em todo módulo, então a colisão é a
// regra num bundle real; na carga do WhatsApp Web ela também aparecia como
// "assignment to unbound `e`", que era só a face visível do mesmo defeito.
//
// A correção dá à checagem de função-de-topo a MESMA guarda de sombreamento que
// a checagem de namespace do Registry logo abaixo dela já tinha: só vale quando
// o nome NÃO está no escopo.
//
// Valores conferidos contra Node e Bun (fixture cross-runtime
// tests/cross-runtime/syntax/423_local_shadows_toplevel_fn.ts).

// ── função de topo chamada `e` ──────────────────────────────────────────────
var e = function (): any { return "TOPO"; };

// ── fábrica com o SEU PRÓPRIO `e`, capturado E escrito por fn aninhada ──────
function fabrica(req: any): any {
  var e, s = null;
  function pega(): any {
    if (s != null) return s;
    const v = (e || (e = req("Env"))).id;
    s = "sess-" + v;
    return s;
  }
  return pega;
}
const p = fabrica(function (n: any) { return { id: "ID:" + n }; });
const primeira = p();
const memoizada = p();
const topoIntacto = (e as any)();

// ── mesmo nome como PARÂMETRO ───────────────────────────────────────────────
function comParam(e: any): any {
  const inner = function (): any { return "param:" + e; };
  return inner();
}
const viaParam = comParam("X");

// ── duas closures irmãs escrevendo o MESMO local sombreador ─────────────────
function duasIrmas(): any {
  var e = 0;
  const inc = function (): void { e = e + 1; };
  const ler = function (): any { return e; };
  inc();
  inc();
  return ler();
}
const irmas = duasIrmas();

// ── sombreamento com o local só LIDO ────────────────────────────────────────
function soLeitura(): any {
  const e = "local";
  const f = function (): any { return e; };
  return f();
}
const leitura = soLeitura();

describe("local sombreia função de topo homônima", () => {
  test("captura escrita alcança o LOCAL, não a função de topo", () => {
    expect(primeira).toBe("sess-ID:Env");
  });
  test("memoização devolve o valor guardado na segunda chamada", () => {
    expect(memoizada).toBe("sess-ID:Env");
  });
  test("a função de topo continua intacta", () => {
    expect(topoIntacto).toBe("TOPO");
  });
  test("param sombreia a função de topo", () => {
    expect(viaParam).toBe("param:X");
  });
  test("closures irmãs compartilham o local sombreador", () => {
    expect(irmas).toBe(2);
  });
  test("sombreamento vale também para local só lido", () => {
    expect(leitura).toBe("local");
  });
});
