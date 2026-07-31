import { describe, test, expect } from "rts:test";
import dom from "rts:dom";

// Duas coisas que impediam qualquer código assíncrono de funcionar numa PÁGINA.
//
// 1) A fila de MICROTASKS nunca era drenada no escopo de página. Um `<script>`
//    que fazia `Promise.resolve(7).then(cb)` registrava o callback e ele ficava
//    na fila PARA SEMPRE — sem erro, sem sintoma. O host drena depois do
//    `__rts_startup`, mas o script da página roda DENTRO daquele task; o browser
//    fecha o task ao fim do script, e é o que `runScripts` passa a fazer
//    (`engine.run_event_loop()`).
//
// 2) `fetch` não existia na página. O `fetch` do motor é membro de namespace
//    (`rts:fetch`), e namespace não é alcançável nem do escopo de página nem de
//    um prelude `.ts` — só o motor alcança. A ponte PRIVADA `engine.fetch_text`
//    resolve isso delegando ao MESMO membro de Registry.
//
// Sem (1), (2) seria pior que inútil: `typeof fetch === "function"` passaria no
// feature-detect e o `.then` nunca chamaria de volta — falha silenciosa.

function rodaNaPagina(script: string): string {
  const html = "<html><body><div id=o></div><script>" + script + "</scr" + "ipt></body></html>";
  const d: i64 = dom.parseHtml(html);
  runScriptsAt(new Document(d), "https://exemplo/");
  return dom.getText(d, dom.querySelector(d, "#o"));
}

const microtask = rodaNaPagina(
  "var el=document.getElementById('o');" +
  "Promise.resolve(7).then(function(v){ el.textContent = 'v=' + v; });",
);

const encadeado = rodaNaPagina(
  "var el=document.getElementById('o');" +
  "Promise.resolve(5).then(function(v){ return v * 2; })" +
  ".then(function(v){ el.textContent = 'v=' + v; });",
);

const temFetch = rodaNaPagina(
  "document.getElementById('o').textContent = " +
  "'fetch=' + (typeof fetch) + ' Response=' + (typeof Response);",
);

describe("microtask da página é drenada", () => {
  test("Promise.resolve().then chama o callback", () => {
    expect(microtask).toBe("v=7");
  });

  test("cadeia de dois then resolve", () => {
    expect(encadeado).toBe("v=10");
  });
});

describe("fetch existe no escopo de página", () => {
  test("fetch e Response são funções", () => {
    expect(temFetch).toBe("fetch=function Response=function");
  });
});
