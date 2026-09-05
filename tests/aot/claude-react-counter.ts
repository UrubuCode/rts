// Prova do lote `page-script-window-dynamico`: o React 18 REAL — três
// <script> inline, um bundle UMD que faz `global.React = factory()` sobre
// `this` — monta dentro de um `.exe` AOT compilado com
// `rts compile --html scripts/rts_vs_electron/app/index.html`.
//
// Sem o fallback dinâmico (Scoped::Page lendo `RuntimeOp::PageGlobalGet`
// contra o `window` do script em vez de recusar em compile-time ou de
// consultar o objeto global do PROCESSO), o terceiro <script> — a app —
// lançava "ReferenceError: React is not defined": `React`/`ReactDOM` só
// existem como propriedade do `window` depois de o PRIMEIRO script (o
// bundle) correr, e o `enclosing` estático que `object::page` constrói em
// tempo de build nunca viu esse nome, porque nada no TEXTO do bundle atribui
// `React` livre — só `global.React = {}`, uma escrita de propriedade sobre
// `this`.
import { readFileSync } from "node:fs";

const html = readFileSync("scripts/rts_vs_electron/app/index.html", "utf8") as string;
const doc = parseDocument(html);
const rodaram = runScriptsAt(doc, "https://localhost/");
const raiz = doc.getElementById("root");
console.log("scripts corridos: " + rodaram);
console.log("textContent do #root: " + (raiz === null ? "(sem elemento)" : raiz.textContent));
