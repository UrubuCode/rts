import { readFileSync } from "node:fs";
// Régua do lote "ciclo de vida de scripts de página" — a ordem em que os
// scripts de uma NAVEGAÇÃO (`loadDocument`) correm e `DOMContentLoaded`/`load`
// disparam, medida no Edge headless a 2026-09-05. Adaptada de
// `parseDocument`+`runScriptsAt` — o par que só corria os scripts, sem os
// dois eventos, que é o que esta fixture media antes deste lote ("scripts
// corridos: 2", "saida" vazio) — para `loadDocument`, o caminho que os
// implementa.
const html = readFileSync(process.argv[process.argv.length - 1], "utf8") as string;

const doc = loadDocument(html, "https://localhost/");
const saida = doc.getElementById("saida");
console.log("saida: " + (saida === null ? "(sem div)" : saida.textContent));

// Segundo caso: `parseDocument` continua 100% inerte — scripting DESLIGADO,
// como um `DOMParser` real. Um `<script>` criado e ligado por `appendChild`
// depois do parse (o MESMO caminho que corre dentro de `loadDocument`) não
// corre aqui: nenhuma navegação aconteceu.
const provaDoc = parseDocument("<body><div id='alvo'></div></body>");
const bodyProva = provaDoc.querySelector("body");
if (bodyProva !== null) {
  const script = provaDoc.createElement("script");
  script.textContent = "document.getElementById('alvo').textContent = 'correu'";
  bodyProva.appendChild(script);
}
const alvo = provaDoc.getElementById("alvo");
console.log("parseDocument inerte: " + (alvo !== null && alvo.textContent === "" ? "sim" : "nao"));
