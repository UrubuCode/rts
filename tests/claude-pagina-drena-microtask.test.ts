import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

// A fila de MICROTASKS nunca era drenada no escopo de página. Um `<script>` que
// fazia `Promise.resolve(7).then(cb)` registrava o callback e ele ficava na fila
// PARA SEMPRE — sem erro, sem sintoma, sem jeito de o autor da página descobrir.
//
// O host drena depois do `__rts_startup`, mas o script da página roda DENTRO
// daquele task. O browser fecha o task ao fim do script, e é o que `runScripts`
// passa a fazer (`engine.run_event_loop()`, ponte sobre o `__rtsn_run_event_loop`
// que já existia e o AOT já usava).
//
// Ligar o drain sozinho, porém, TROCA um problema por outro pior: os callbacks
// passam a rodar de verdade, e um único handler de terceiro que lança derruba a
// página inteira. Foi o que aconteceu numa tentativa anterior, revertida.
//
// Isolar exigiu achar ONDE o erro vive: um `try/catch` de `.ts` NÃO o contém,
// porque o erro do motor viaja num SLOT lateral que o `catch` não observa — e há
// DOIS slots (o do motor novo, `adapters/errslot`, e o legado
// `collector::error`). A ponte `engine.take_error()` consome o do motor novo;
// `runScripts` reporta e segue, como o console do browser.
//
// Valores conferidos contra o Node.

function rodaNaPagina(script: string): string {
  const html = "<html><body><div id=o></div><script>" + script + "</scr" + "ipt></body></html>";
  const d: i64 = dom.parseHtml(html);
  runScriptsAt(new Document(d), "https://exemplo/");
  return dom.getText(d, dom.querySelector(d, "#o"));
}

const simples = rodaNaPagina(
  "var el=document.getElementById('o');" +
  "Promise.resolve(7).then(function(v){ el.textContent = 'v=' + v; });",
);

const encadeado = rodaNaPagina(
  "var el=document.getElementById('o');" +
  "Promise.resolve(5).then(function(v){ return v * 2; })" +
  ".then(function(v){ el.textContent = 'v=' + v; });",
);

const comCatch = rodaNaPagina(
  "var el=document.getElementById('o');" +
  "Promise.reject(new Error('x')).catch(function(e){ el.textContent = 'capturado'; });",
);

// um handler que LANÇA não pode impedir o resto da página de rodar
const isolado = rodaNaPagina(
  "var el=document.getElementById('o');" +
  "Promise.resolve(1).then(function(){ throw new Error('boom'); });" +
  "Promise.resolve(2).then(function(v){ el.textContent = 'seguiu=' + v; });",
);

describe("microtask da página é drenada", () => {
  test("Promise.resolve().then chama o callback", () => {
    expect(simples).toBe("v=7");
  });

  test("cadeia de dois then resolve", () => {
    expect(encadeado).toBe("v=10");
  });

  test("catch de rejeição roda", () => {
    expect(comCatch).toBe("capturado");
  });
});

describe("um handler que lança não derruba a página", () => {
  test("o resto das microtasks continua rodando", () => {
    expect(isolado).toBe("seguiu=2");
  });
});
