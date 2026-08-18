// Sonda de uma fixture só — o que o `rts:dom` diz das caixas de um ficheiro.
//
//   target/release/examples/run_fixture.exe examples/claude-css-probe.ts
//
// Existe para responder à pergunta que precede o corpus: o `<style>` embutido
// entra na cascata sem `addStylesheet`, e a origem do `boundingRect` é a mesma
// que o Chrome usa? Sem isso medido, um corpus de 40 ficheiros mede o
// instrumento e não o motor.
import { readFileSync } from "node:fs";
import {
  parseHtml, querySelectorAllCount, querySelectorAllAt,
  getAttribute, tagName, boundingRect,
} from "rts:dom";

const caminho = "tests/css/claude-box-model.html";
const doc = parseHtml(readFileSync(caminho, "utf8") as string);

const total = querySelectorAllCount(doc, "*");
for (let i = 0; i < total; i = i + 1) {
  const n = querySelectorAllAt(doc, "*", i);
  const id = getAttribute(doc, n, "id") as string;
  if (id.length === 0) { continue; }
  console.log(
    id + " <" + tagName(doc, n) + ">",
    boundingRect(doc, n, 0) + "," + boundingRect(doc, n, 1),
    boundingRect(doc, n, 2) + "x" + boundingRect(doc, n, 3),
  );
}
