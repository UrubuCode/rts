// A ÁRVORE de percentagens: lê os `relatorio.json` de uma varredura e imprime
// o corpus como ele é — uma hierarquia de pastas, cada ramo com a sua barra.
//
//   bun scripts/reftests_arvore.mjs "$TEMP/wpt-css-todas" [--prof 2] [--min 10] [--json a.json]
//
// Existe porque um total sozinho engana nos dois sentidos. "80,4 %" não diz se
// o motor faz flexbox bem e grid nada, ou o contrário; e uma pasta que este
// motor não tenta (`css-paint-api`, `WOFF2`) baixa o total sem dizer nada
// sobre o motor. A árvore mostra ONDE está a percentagem, que é a única forma
// desse número virar trabalho.
//
// Lê `resultados` (todos os testes, com o nome relativo à pasta medida) e não
// `piores` (só as falhas): a percentagem de um ramo precisa do denominador
// desse ramo, e as falhas sozinhas dão-lhe só o numerador.
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const raiz = resolve(args.find((a) => !a.startsWith("--")) ?? ".");
const opt = (n, d) => { const i = args.indexOf("--" + n); return i >= 0 ? args[i + 1] : d; };
const PROF = Number(opt("prof", "2"));      // até que nível da árvore se imprime
const MIN = Number(opt("min", "1"));        // ramos com menos testes do que isto ficam somados no pai
const JSON_OUT = opt("json", "");

// --- recolhe: cada relatorio.json é uma pasta de topo; `nome` traz o resto do caminho
const raizArvore = { filhos: new Map(), passam: 0, total: 0, erros: 0 };
function poe(caminho, estado) {
  let no = raizArvore;
  no.total++; if (estado === "passa") no.passam++; else if (estado === "erro") no.erros++;
  for (const seg of caminho) {
    if (!no.filhos.has(seg)) no.filhos.set(seg, { filhos: new Map(), passam: 0, total: 0, erros: 0 });
    no = no.filhos.get(seg);
    no.total++; if (estado === "passa") no.passam++; else if (estado === "erro") no.erros++;
  }
}

const pastas = existsSync(join(raiz, "relatorio.json"))
  ? [""]
  : readdirSync(raiz, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name).sort();

let lidos = 0;
for (const p of pastas) {
  const rel = join(raiz, p, "relatorio.json");
  if (!existsSync(rel)) continue;
  const r = JSON.parse(readFileSync(rel, "utf8"));
  if (!Array.isArray(r.resultados)) {
    console.error(`${p || "."}: relatorio.json sem \`resultados\` — foi produzido por um corredor antigo; re-meça essa pasta.`);
    continue;
  }
  lidos++;
  for (const t of r.resultados) {
    // O nome é relativo à pasta medida; o último segmento é o teste, não um ramo.
    const segs = t.nome.split("/");
    segs.pop();
    poe(p ? [p, ...segs] : segs, t.estado);
  }
}
if (lidos === 0) { console.error(`nenhum relatorio.json com \`resultados\` sob ${raiz}`); process.exit(2); }

// --- imprime
function barra(pct, largura = 20) {
  const cheios = Math.round((pct / 100) * largura);
  return "▰".repeat(cheios) + "▱".repeat(largura - cheios);
}
function linha(nome, no, nivel) {
  const pct = no.total > 0 ? (no.passam / no.total) * 100 : 0;
  const recuo = "  ".repeat(nivel);
  const rot = (recuo + nome).padEnd(38).slice(0, 38);
  const err = no.erros > 0 ? `  (${no.erros} sem raster)` : "";
  console.log(`${rot} [${barra(pct)}] ${pct.toFixed(1).padStart(5)}%  ${String(no.passam).padStart(5)}/${String(no.total).padEnd(5)}${err}`);
}
function desce(no, nivel) {
  if (nivel > PROF) return;
  const filhos = [...no.filhos.entries()].filter(([, f]) => f.total >= MIN).sort((a, b) => b[1].total - a[1].total);
  for (const [nome, f] of filhos) { linha(nome, f, nivel); desce(f, nivel + 1); }
}
linha("TOTAL", raizArvore, 0);
desce(raizArvore, 1);
console.log(`\nUm ramo que este motor não tenta baixa o TOTAL sem dizer nada sobre o motor — a árvore é o número, o total é a soma dela.`);

if (JSON_OUT) {
  const serializa = (no) => ({
    passam: no.passam, total: no.total, erros: no.erros,
    pct: no.total > 0 ? (no.passam / no.total) * 100 : 0,
    filhos: Object.fromEntries([...no.filhos].map(([k, v]) => [k, serializa(v)])),
  });
  writeFileSync(resolve(JSON_OUT), JSON.stringify(serializa(raizArvore), null, 2));
  console.log(`árvore em ${resolve(JSON_OUT)}`);
}
