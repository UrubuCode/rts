import { describe, test, expect } from "rts:test";

// Atribuir a um nome que não casa com binding léxico nenhum CRIA uma
// propriedade no objeto global — o "global implícito" do modo SLOPPY.
//
// É a contraparte de ESCRITA da cadeia de escopo (a LEITURA já resolvia contra
// o objeto global desde o #2078). O motor recusava o ARQUIVO INTEIRO
// ("assignment to unbound"), e é a forma que todo `<script>` de página produz —
// derrubava bundles da carga do WhatsApp Web.
//
// POR QUE NÃO É FIXTURE CROSS-RUNTIME: Node e Bun DISCORDAM, e não sobre
// semântica de JS — sobre o MODO de um arquivo `.ts`. O Node o trata como
// script (sloppy) e cria o global; o Bun o trata como módulo ES (strict) e
// lança ReferenceError. Uma fixture aqui documentaria a escolha de módulo do
// runtime, não o motor.
//
// O RTS não distingue strict de sloppy hoje, e adota sloppy — que é o que a
// superfície que motivou isto (script de página, bundle) realmente usa. Num
// módulo ES a resposta correta seria ReferenceError; está registrado, não
// escondido.

declare var implicito: any;
declare var outroImplicito: any;

function escreve(): any {
  implicito = 5;
  return implicito;
}
const valor = escreve();
const noGlobal = (globalThis as any).implicito;

// visível de OUTRA função, sem declaração em lugar nenhum
function le(): any { return implicito; }
const outraFn = le();

// reatribuição troca o tipo, como qualquer propriedade
function troca(): any { implicito = "texto"; return implicito; }
const reatribuido = troca();
const tipo = typeof (globalThis as any).implicito;

// o valor da EXPRESSÃO é o valor atribuído
const valorDaExpr = (outroImplicito = 42);

// um local de mesmo nome NÃO vira global
function comLocal(): any {
  let implicito = "local";
  implicito = "mudou";
  return implicito;
}
const local = comLocal();
const globalIntacto = (globalThis as any).implicito;

describe("global implícito (atribuição a nome livre)", () => {
  test("a escrita cria a propriedade global", () => {
    expect(valor).toBe(5);
    expect(noGlobal).toBe(5);
  });
  test("outra função enxerga o global criado", () => {
    expect(outraFn).toBe(5);
  });
  test("reatribuição troca valor e tipo", () => {
    expect(reatribuido).toBe("texto");
    expect(tipo).toBe("string");
  });
  test("o valor da expressão é o valor atribuído", () => {
    expect(valorDaExpr).toBe(42);
  });
  test("local de mesmo nome não vira global", () => {
    expect(local).toBe("mudou");
    expect(globalIntacto).toBe("texto");
  });
});
