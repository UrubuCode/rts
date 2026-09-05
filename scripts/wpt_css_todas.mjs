// A régua do CSS INTEIRO: corre `wpt_reftests.mjs` pasta a pasta sobre
// `wpt/css/` e agrega uma tabela por pasta mais um total.
//
//   bun scripts/wpt_css_todas.mjs "$TEMP/wpt/css" [--out dir] [--pastas a,b,c] [--pares match|sufixo]
//
// Porque um script à parte e não uma flag do corredor: o corredor responde
// "esta pasta" e o seu relatório é a unidade que se compara por nome entre
// duas medições. Juntar as pastas num só relatório perderia essa unidade — e
// perder-se-ia também a capacidade de re-medir SÓ a pasta que um lote tocou,
// que é o que torna a régua utilizável durante o trabalho. Aqui cada pasta
// continua a ter o seu `relatorio.json`; o que este script acrescenta é a
// tabela por cima deles.
//
// O total NÃO é uma percentagem para exibir sozinha. Uma pasta que este motor
// não tenta (`css-paint-api`, `css-layout-api`, `WOFF2`) baixa-o sem dizer
// nada sobre o motor, e uma pasta que ele faz bem sobe-o do mesmo modo. A
// tabela por pasta é o número; o total é a soma dela.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const args = process.argv.slice(2);
const raiz = resolve(args.find((a) => !a.startsWith("--")) ?? ".");
const opt = (n, d) => { const i = args.indexOf("--" + n); return i >= 0 ? args[i + 1] : d; };
const OUT = resolve(opt("out", join(process.env.TEMP ?? ".", "wpt-css-todas")));
const SO = opt("pastas", "") ? opt("pastas", "").split(",").map((s) => s.trim()) : null;
// `--pares` chega ao corredor tal e qual. Sem esta linha o agregador media
// os web_tests do Blink com a convencao do WPT (`<link rel=match>`), que
// eles nao usam — e o resultado nao seria um erro, seria "0 reftests" em
// cada pasta, com ar de medicao.
const PARES = opt("pares", "match");
const CORREDOR = resolve("scripts/wpt_reftests.mjs");
if (!existsSync(CORREDOR)) { console.error(`não encontrei ${CORREDOR} — corra a partir da raiz do repositório`); process.exit(2); }
mkdirSync(OUT, { recursive: true });

const pastas = readdirSync(raiz, { withFileTypes: true })
  .filter((e) => e.isDirectory() && (!SO || SO.includes(e.name)))
  .map((e) => e.name)
  .sort();

const linhas = [];
let tp = 0, tf = 0, te = 0;
for (const nome of pastas) {
  const saida = join(OUT, nome);
  let txt = "";
  try {
    txt = execFileSync("bun", [CORREDOR, join(raiz, nome), "--out", saida, "--pares", PARES], {
      encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], maxBuffer: 64 * 1024 * 1024,
    });
  } catch (e) {
    // Uma pasta que rebenta o corredor não é uma pasta com 0 %: é uma pasta
    // sem medição, e dizê-lo é a diferença entre um número e uma alegação.
    linhas.push({ nome, passam: null, total: null, erro: String(e.message ?? e).slice(0, 120) });
    console.log(`${nome.padEnd(28)} ERRO`);
    continue;
  }
  const rel = join(saida, "relatorio.json");
  if (!existsSync(rel)) { linhas.push({ nome, passam: null, total: null, erro: "sem relatorio.json" }); continue; }
  const r = JSON.parse(readFileSync(rel, "utf8"));
  // O `relatorio.json` e lido AQUI e os PNG dessa pasta deixam de ser precisos:
  // a varredura larga quer o numero, e quem for investigar uma falha corre essa
  // pasta sozinha. Guardados, o `css` inteiro deixou 103 GB numa so pasta e
  // encheu o disco desta maquina a meio de uma medicao (2026-09-05).
  for (const f of readdirSync(saida)) if (f.endsWith(".png") || f.endsWith(".mask.json")) rmSync(join(saida, f), { force: true });
  tp += r.passam; tf += r.falham; te += r.erros;
  const pct = r.total > 0 ? (r.passam / r.total) * 100 : 0;
  linhas.push({ nome, passam: r.passam, total: r.total, erros: r.erros, pct });
  console.log(`${nome.padEnd(28)} ${String(r.passam).padStart(5)}/${String(r.total).padEnd(5)} ${pct.toFixed(1).padStart(5)}%${r.erros ? `  (${r.erros} não rasterizaram)` : ""}`);
}

const total = tp + tf + te;
console.log(`\nTOTAL ${tp}/${total} (${((tp / Math.max(total, 1)) * 100).toFixed(1)}%), ${tf} falham, ${te} não rasterizaram`);
console.log("A tabela por pasta é o número; o total é a soma dela — uma pasta que este motor não tenta baixa-o sem dizer nada sobre o motor.");
writeFileSync(join(OUT, "todas.json"), JSON.stringify({ raiz, pastas: linhas, passam: tp, falham: tf, erros: te, total }, null, 2));
