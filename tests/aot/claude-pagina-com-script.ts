// Prova mínima do lote `aot-scripts-de-pagina`: um `.exe` AOT compilado com
// `rts compile --html claude-pagina-com-script.html` roda o <script> da
// página SEM o compilador do JIT — `context.eval_compiler_with_receiver` é o
// hook que `crates/rts-runtime/src/aot/page_scripts.rs` instala, achando a
// função pré-compilada pelo hash da fonte extraída em tempo de build.
//
// Sem `--html`, este mesmo programa compila e RODA (o `rts:dom` está no
// runtime AOT desde o PR #2671) mas o <script> falha: "[page] <script> 0 de
// https://localhost/ falhou: this script was not pre-compiled…" — e
// `textContent` fica vazio. Com `--html`, o script roda e imprime "ok".
import { readFileSync } from "node:fs";

const html = readFileSync("tests/aot/claude-pagina-com-script.html", "utf8") as string;
const doc = parseDocument(html);
const rodou = runScriptsAt(doc, "https://localhost/");
const alvo = doc.getElementById("x");
console.log("scripts corridos: " + rodou);
console.log("textContent: " + (alvo === null ? "(sem elemento)" : alvo.textContent));
