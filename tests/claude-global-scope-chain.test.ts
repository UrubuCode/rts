import { describe, test, expect } from "rts:test";

// O ÚLTIMO ELO DA CADEIA DE ESCOPO: um nome nu que não casa com binding léxico
// nenhum resolve contra o OBJETO GLOBAL — e só um miss ali é ReferenceError,
// lançado quando o controle de fato chega no ponto.
//
// É assim que o carregador de módulos de uma página funciona: um script faz
// `globalThis.__d = fn` e todos os outros chamam `__d(…)` nu. Nenhuma análise
// léxica do script chamador enxerga esse binding — e o motor recusava o
// ARQUIVO INTEIRO por causa dele (`call to unknown function`). O caminho de
// LEITURA já lançava ReferenceError em runtime em vez de bailar, mas lançava
// direto, sem consultar o objeto global; agora os dois passam pelo mesmo
// trampolim `__rtsadp_global_ref` e não podem divergir.
//
// Junto veio um bug de curto-circuito PRÉ-EXISTENTE e independente:
// `is_effect_free` tratava QUALQUER identificador como inerte, então o caminho
// rápido de `&&`/`||` (e o ternário sem desvio) avaliava o operando direito
// avidamente. Ler um nome que não resolve não é inerte — lança. `false && nope`
// lançava onde o JS curto-circuita, e derrubava o sniff UMD
// (`typeof module !== "undefined" && module.exports`) que todo bundle abre.
// Agora só um LOCAL conta como inerte: é o único ident que comprovadamente não
// lança, e é o caso que o caminho rápido existe para servir.
//
// Valores conferidos contra o Node.

// ── publicado por "outro script", chamado/lido como nome nu ─────────────────
(globalThis as any).__pubFn = function (a: any, b: any) { return "w:" + a + b; };
(globalThis as any).__pubObj = { mode: "live" };

function chamaGlobal(): any { return (__pubFn as any)(1, 2); }
const viaChamada = chamaGlobal();
const viaLeitura = (__pubObj as any).mode;

// ── ausente de verdade: ReferenceError em RUNTIME, no ponto do uso ──────────
let missCall = "";
try { (naoExisteNoCall as any)(); missCall = "NAO-LANCOU"; }
catch (e: any) { missCall = e.constructor.name + ":" + e.message; }

let missRead = "";
try { const v = (naoExisteNoRead as any) + 1; missRead = "NAO-LANCOU:" + v; }
catch (e: any) { missRead = e.constructor.name + ":" + e.message; }

// ── curto-circuito: o operando direito NÃO pode ser avaliado ────────────────
function sniffUmd(): any {
  if (typeof module !== "undefined" && (module as any).exports) return "cjs";
  if (typeof define === "function") return "amd";
  return "global";
}
const umd = sniffUmd();

function andLiteral(): any { if (false && (semNome1 as any)) return "y"; return "n"; }
function andMembro(): any { if (false && (semNome2 as any).x) return "y"; return "n"; }
function orLiteral(): any { if (true || (semNome3 as any)) return "y"; return "n"; }
const scAnd = andLiteral();
const scMembro = andMembro();
const scOr = orLiteral();

// ternário sem desvio: o ramo não tomado também não pode ser avaliado
function ternario(c: any): any { return c ? "sim" : (semNome4 as any); }
const tern = ternario(true);

// ── local/param continuam com prioridade sobre o global ─────────────────────
function sombraParam(__pubFn: any): any { return __pubFn + "!"; }
const sombra = sombraParam("param");

// ── typeof de nome ausente segue "undefined" (spec), sem lançar ─────────────
const tof = typeof aindaNaoDefinido;

describe("cadeia de escopo termina no objeto global", () => {
  test("global publicado dinamicamente é chamável por nome nu", () => {
    expect(viaChamada).toBe("w:12");
  });
  test("global publicado dinamicamente é legível por nome nu", () => {
    expect(viaLeitura).toBe("live");
  });
  test("nome ausente lança ReferenceError na CHAMADA", () => {
    expect(missCall).toBe("ReferenceError:naoExisteNoCall is not defined");
  });
  test("nome ausente lança ReferenceError na LEITURA", () => {
    expect(missRead).toBe("ReferenceError:naoExisteNoRead is not defined");
  });
  // Respondia "global" e passou a responder "cjs" quando o motor ganhou
  // CommonJS: `module` e agora um binding de todo modulo, entao o sniff que
  // todo bundle UMD abre encontra o que procura. E o que o Node responde ao
  // correr o mesmo ficheiro como CommonJS, e e o ramo que aqui funciona de
  // facto — `module.exports` e publicado. A expectativa antiga descrevia um
  // motor sem o outro sistema de modulos, nao uma regra da linguagem.
  test("sniff UMD compila e escolhe o ramo CommonJS", () => {
    expect(umd).toBe("cjs");
  });
  test("&& curto-circuita: direito não é avaliado", () => {
    expect(scAnd).toBe("n");
    expect(scMembro).toBe("n");
  });
  test("|| curto-circuita: direito não é avaliado", () => {
    expect(scOr).toBe("y");
  });
  test("ternário não avalia o ramo não tomado", () => {
    expect(tern).toBe("sim");
  });
  test("param sombreia o global de mesmo nome", () => {
    expect(sombra).toBe("param!");
  });
  test("typeof de nome ausente é \"undefined\"", () => {
    expect(tof).toBe("undefined");
  });
});
