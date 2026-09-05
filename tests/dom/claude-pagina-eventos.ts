import { readFileSync } from "node:fs";
// Régua do lote "página como entrada": a ordem em que os scripts de uma página
// correm e os eventos DOMContentLoaded/load disparam, comparada com o Blink.
const html = readFileSync(process.argv[process.argv.length - 1], "utf8") as string;
const doc = parseDocument(html);
const n = runScriptsAt(doc, "https://localhost/");
pumpTimerCallbacks(doc);
pumpEventCallbacks(doc);
const saida = doc.getElementById("saida");
console.log("scripts corridos: " + n);
console.log("saida: " + (saida === null ? "(sem div)" : saida.textContent));
