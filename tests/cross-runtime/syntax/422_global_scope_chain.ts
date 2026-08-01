// Cross-runtime: o ÚLTIMO ELO da cadeia de escopo é o OBJETO GLOBAL. Um nome nu
// que não casa com binding léxico nenhum resolve contra `globalThis`, e só um
// miss ali é ReferenceError — lançado quando o controle chega no ponto, não em
// tempo de compilação. É assim que um script publica algo para os outros.
//
// Junto: `&&`/`||`/ternário precisam CURTO-CIRCUITAR de verdade — avaliar o lado
// não tomado transformaria o sniff de ambiente que todo bundle abre num throw.
//
// A MENSAGEM do ReferenceError diverge entre engines, então só o TIPO é impresso.

declare var __xrtFn: (a: number, b: number) => string;
declare var __xrtObj: { mode: string };

(globalThis as Record<string, unknown>)["__xrtFn"] = function (a: number, b: number) {
  return "w:" + a + b;
};
(globalThis as Record<string, unknown>)["__xrtObj"] = { mode: "live" };

// 1. publicado dinamicamente, alcançado por NOME NU (chamada e leitura)
function chamaGlobal(): string {
  return __xrtFn(1, 2);
}
console.log("call=" + chamaGlobal());
console.log("read=" + __xrtObj.mode);

// 2. ausente de verdade → ReferenceError em RUNTIME, no ponto do uso
let tipoCall = "NAO-LANCOU";
try {
  (globalThis as Record<string, unknown>)["naoLido"];
  (naoExisteChamada as () => void)();
} catch (e) {
  tipoCall = (e as Error).constructor.name;
}
console.log("miss_call=" + tipoCall);

let tipoRead = "NAO-LANCOU";
try {
  const x = (naoExisteLeitura as number) + 1;
  tipoRead = "NAO-LANCOU:" + x;
} catch (e) {
  tipoRead = (e as Error).constructor.name;
}
console.log("miss_read=" + tipoRead);

// 3. curto-circuito: o lado não tomado NÃO pode ser avaliado
function andLiteral(): string {
  if (false && (semNome1 as boolean)) return "y";
  return "n";
}
function andMembro(): string {
  if (false && (semNome2 as { x: number }).x) return "y";
  return "n";
}
function orLiteral(): string {
  if (true || (semNome3 as boolean)) return "y";
  return "n";
}
function ternario(c: boolean): string {
  return c ? "sim" : (semNome4 as string);
}
console.log("sc_and=" + andLiteral());
console.log("sc_membro=" + andMembro());
console.log("sc_or=" + orLiteral());
console.log("sc_tern=" + ternario(true));

// 4. sniff de ambiente: o ramo não tomado nomeia coisas que não existem
function sniff(): string {
  if (typeof naoDefinidoA !== "undefined" && (naoDefinidoA as { x: number }).x) return "a";
  if (typeof naoDefinidoB === "function") return "b";
  return "nenhum";
}
console.log("sniff=" + sniff());

// 5. param sombreia o global de mesmo nome
function sombra(__xrtFn: string): string {
  return __xrtFn + "!";
}
console.log("sombra=" + sombra("param"));

// 6. typeof de nome ausente é "undefined" e NÃO lança
console.log("typeof=" + typeof aindaNaoDefinido);
