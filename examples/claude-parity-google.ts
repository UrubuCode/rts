// Lado RTS do harness de paridade: uma linha JSON por ELEMENTO da página.
//
//   target/release/examples/run_fixture.exe examples/claude-parity-rts.ts > scripts/parity/out/rts.jsonl
//
// Lê `scripts/parity/google.combinada.html` — o MESMO ficheiro que o Chrome
// carrega. Combinado de propósito: o nosso caminho antigo era
// `parseHtml(html)` + `addStylesheet(doc, css)`, e o Chrome não tem como
// receber uma folha "por fora" sem que ela entre na cascata num sítio que
// ninguém escolheu. Um `<style>` no topo do documento é a única forma em que os
// dois lados leem a mesma entrada, e é por isso que o gerador embute o CSS em
// vez de o passar à parte.
//
// A saída é JSONL e não um JSON só: são ~16k elementos e construir uma string
// única obriga a ter o relatório todo em memória do lado do motor antes de
// qualquer coisa sair. Uma linha por elemento também sobrevive a um corte a
// meio — o comparador conta as linhas que leu contra o total anunciado, que é a
// regra "verifique a ENTRADA" aplicada a este instrumento.
import { readFileSync } from "node:fs";
import {
  parseHtml, nodeCount, querySelector,
  childCount, childAt, tagName, computedProperty, boundingRect,
} from "rts:dom";

const CAMINHO = "scripts/parity/google.combinada.html";
const fonte = readFileSync(CAMINHO, "utf8") as string;
const doc = parseHtml(fonte);

// O viewport não é escolhido aqui: `rts:dom` não expõe `setViewport`, e o
// default do `Dom` é 1280x800 (crates/rts-dom/src/dom.rs). O lado Chrome é
// forçado a esse mesmo tamanho. Se um dia a fachada expuser o viewport, é aqui
// que ele passa a ser dito em vez de assumido.
const VIEWPORT_W = 1280;
const VIEWPORT_H = 800;

const PROPS = ["display", "position", "color", "background-color", "font-size"];

/// Escapar para JSON à mão: os valores são tags, caminhos e propriedades
/// computadas — texto curto e sem controlos — mas uma aspa numa `content` ou
/// numa família de fonte partiria a linha toda, e uma linha partida é um
/// elemento que o comparador conta como FALHA de extração em vez de descontar.
function jstr(s: string): string {
  let out = "\"";
  for (let i = 0; i < s.length; i = i + 1) {
    const c = s.charAt(i);
    const code = s.charCodeAt(i);
    if (c === "\"") { out = out + "\\\""; }
    else if (c === "\\") { out = out + "\\\\"; }
    else if (code < 32) { out = out + " "; }
    else { out = out + c; }
  }
  return out + "\"";
}

const raiz = querySelector(doc, "html");
if (raiz < 0) {
  console.log("{\"__erro\":\"sem elemento <html>\"}");
} else {
  console.log("{\"__meta\":1,\"lado\":\"rts\",\"ficheiro\":" + jstr(CAMINHO) +
              ",\"bytes\":" + fonte.length + ",\"nos\":" + nodeCount(doc) +
              ",\"viewport\":[" + VIEWPORT_W + "," + VIEWPORT_H + "]}");

  // Travessia ITERATIVA com pilha explícita. A árvore da Wikipédia passa das
  // 30 gerações em sítios e uma recursão por elemento arrisca a pilha do motor
  // — um estouro aqui não daria um número pior, daria zero número.
  const pilhaNo: number[] = [];
  const pilhaCaminho: string[] = [];
  pilhaNo.push(raiz);
  pilhaCaminho.push("html[1]");
  let emitidos = 0;

  while (pilhaNo.length > 0) {
    const n = pilhaNo.pop() as number;
    const caminho = pilhaCaminho.pop() as string;

    let linha = "{\"p\":" + jstr(caminho) +
      ",\"tag\":" + jstr(tagName(doc, n) as string) +
      ",\"x\":" + boundingRect(doc, n, 0) +
      ",\"y\":" + boundingRect(doc, n, 1) +
      ",\"w\":" + boundingRect(doc, n, 2) +
      ",\"h\":" + boundingRect(doc, n, 3);
    for (let k = 0; k < PROPS.length; k = k + 1) {
      const nome = PROPS[k];
      linha = linha + "," + jstr(nome) + ":" + jstr(computedProperty(doc, n, nome) as string);
    }
    console.log(linha + "}");
    emitidos = emitidos + 1;

    // O índice do caminho conta irmãos DA MESMA TAG (1-based, como XPath). Um
    // índice de posição pura desloca-se todo assim que um lado inventa um
    // elemento implícito que o outro não tem; contando por tag, a divergência
    // fica contida na tag afetada em vez de invalidar os irmãos seguintes.
    const total = childCount(doc, n);
    const vistos: string[] = [];
    const contas: number[] = [];
    const filhos: number[] = [];
    const nomes: string[] = [];
    for (let i = 0; i < total; i = i + 1) {
      const f = childAt(doc, n, i);
      if (f < 0) { continue; }
      const t = (tagName(doc, f) as string).toLowerCase();
      let idx = -1;
      for (let j = 0; j < vistos.length; j = j + 1) { if (vistos[j] === t) { idx = j; } }
      if (idx < 0) { vistos.push(t); contas.push(1); idx = vistos.length - 1; }
      else { contas[idx] = contas[idx] + 1; }
      filhos.push(f);
      nomes.push(caminho + "/" + t + "[" + contas[idx] + "]");
    }
    // Empilhar ao contrário para que a saída fique em ordem de documento — a
    // ordem não é usada para casar (o caminho é), mas um diff de dois ficheiros
    // fora de ordem não se lê.
    for (let i = filhos.length - 1; i >= 0; i = i - 1) {
      pilhaNo.push(filhos[i]);
      pilhaCaminho.push(nomes[i]);
    }
  }

  console.log("{\"__fim\":1,\"emitidos\":" + emitidos + "}");
}
