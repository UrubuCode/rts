// Reescreve as regiões CSS_PARITY_BADGE / CSS_DOM_STATS do README.md com o
// que o corpus de tests/css/ acabou de medir contra o Blink, e grava o mesmo
// em .github/css_parity_report.json (histórico legível por máquina). Corre no
// job `dom-rulers` do CI (push a main) — o mesmo padrão do bloco cross-runtime:
// o número do README é o que o CI escreveu, nunca o que alguém digitou.
//
//   node scripts/css_parity_readme.mjs corpus.txt
//
// Duas percentagens, e as duas dizem coisas diferentes: FIXTURES que passam
// (um ficheiro passa quando TODAS as suas medições batem a 1px) e MEDIÇÕES
// que batem (cada x/y/w/h e cada propriedade computada, contadas uma a uma).
// A segunda é a que sobe devagar e mede o progresso dentro de uma fixture que
// ainda falha; a primeira é o contrato por ficheiro de tests/css/README.md.
// O estado do DOM vem do PLAN §0 do rts-dom: os lotes ☑/◐/☐ da tabela.
import { readFileSync, writeFileSync, existsSync } from "node:fs";

const corpusPath = process.argv[2] ?? "corpus.txt";
const corpus = readFileSync(corpusPath, "utf8");
const readmePath = "README.md";
const planPath = "crates/rts-dom/PLAN.md";

const mFix = corpus.match(/^passam: (\d+) \| falham: (\d+) \| desvios: (\d+)/m);
const mMed = corpus.match(/^medicoes: (\d+)\/(\d+)/m);
if (!mFix || !mMed) {
  console.error("corpus.txt sem as linhas `passam:` e `medicoes:` — o corredor mudou?");
  process.exit(2);
}
const passam = Number(mFix[1]);
const fixtures = passam + Number(mFix[2]);
const batem = Number(mMed[1]);
const medicoes = Number(mMed[2]);
const pctFix = fixtures ? Math.round((passam / fixtures) * 1000) / 10 : 0;
const pctMed = medicoes ? Math.round((batem / medicoes) * 1000) / 10 : 0;

// as fixtures que falham DE PROPÓSITO, com a razão (o comentário acima delas)
const esperadas = [];
if (existsSync("tests/css/esperado-a-falhar.txt")) {
  let razao = [];
  for (const l of readFileSync("tests/css/esperado-a-falhar.txt", "utf8").split("\n")) {
    if (l.startsWith("#")) { razao.push(l.replace(/^#\s?/, "")); continue; }
    if (!l.trim()) { razao = []; continue; }
    esperadas.push({ fixture: l.trim(), razao: razao.join(" ") });
  }
}

// o estado do DOM: os lotes do PLAN §0
const lotes = { feitos: [], parciais: [], pendentes: [] };
if (existsSync(planPath)) {
  for (const l of readFileSync(planPath, "utf8").split("\n")) {
    const m = l.match(/^\| ([^|]+?) \| ([^|]+?) \| [^|]+? \| (☑|◐|☐)/);
    if (!m) continue;
    const item = `${m[1].trim()} — ${m[2].trim()}`;
    if (m[3] === "☑") lotes.feitos.push(item);
    else if (m[3] === "◐") lotes.parciais.push(item);
    else lotes.pendentes.push(item);
  }
}
const totalLotes = lotes.feitos.length + lotes.parciais.length + lotes.pendentes.length;

// A quarta régua, quando o job a correu: os reftests do WPT (auto-consistência,
// `scripts/wpt_reftests.md`). Ausente = o bloco não a menciona.
const wpt = existsSync(".github/wpt_report.json") ? JSON.parse(readFileSync(".github/wpt_report.json", "utf8")) : null;
const pctWpt = wpt && wpt.total ? Math.round((wpt.passam / wpt.total) * 1000) / 10 : 0;

function barra(pct) {
  const cheios = Math.round(pct / 5);
  return "[" + "▰".repeat(cheios) + "▱".repeat(20 - cheios) + "]";
}
function cor(pct) {
  if (pct >= 95) return "brightgreen";
  if (pct >= 85) return "green";
  if (pct >= 70) return "yellowgreen";
  if (pct >= 50) return "yellow";
  return "red";
}
const hoje = new Date().toISOString().slice(0, 10);

const badge =
  `[![CSS vs Chrome](https://img.shields.io/badge/CSS%20vs%20Chrome-${pctMed}%25-${cor(pctMed)}?style=flat-square)](tests/css/README.md)`;

const linhasEsperadas = esperadas.length
  ? esperadas.map((e) => `- \`${e.fixture}\` — ${e.razao}`).join("\n")
  : "- nenhuma";
const stats = `## 🎨 CSS and DOM parity

Layout and computed style measured against **Chrome/Blink** (Edge headless, 1280×800, 1 px tolerance) over the fixtures in \`tests/css/\`. Two numbers, on purpose: a *fixture* passes only when every measurement in it matches; *measurements* count each x/y/w/h and each computed property one by one. **Read it as "what we implemented is right", not as a share of CSS**: the corpus measures what has a fixture, and each new fixture is written to fail first (\`tests/css/README.md\`).

\`\`\`
${barra(pctMed)} ${pctMed}%   ${batem}/${medicoes} measurements matching Blink
${barra(pctFix)} ${pctFix}%   ${passam}/${fixtures} fixtures passing${wpt ? `
${barra(pctWpt)} ${pctWpt}%   ${wpt.passam}/${wpt.total} WPT reftests (css-flexbox) rendering test == reference` : ""}
\`\`\`${wpt ? `

The WPT line is **self-consistency**, the way browsers run reftests: test and reference are both rendered by this engine and compared pixel by pixel, no browser involved (\`scripts/wpt_reftests.md\`). It measures coherence, not Blink parity.` : ""}

Fixtures that fail **on purpose** (each names a measured gap; \`tests/css/esperado-a-falhar.txt\`):
${linhasEsperadas}

**DOM engine state** (\`crates/rts-dom/PLAN.md\` §0): **${lotes.feitos.length}/${totalLotes} lots done**${lotes.parciais.length ? `, ${lotes.parciais.length} partial` : ""}${lotes.pendentes.length ? `, pending: ${lotes.pendentes.map((p) => p.split(" — ")[0]).join(", ")}` : ""}. The paint ruler (pixels against Blink, \`scripts/css_pintura.md\`) needs a browser and runs locally; its last number is recorded there.

*Updated ${hoje} by CI (\`dom-rulers\`).*`;

let readme = readFileSync(readmePath, "utf8");
const antes = readme;
readme = readme.replace(
  /<!-- CSS_PARITY_BADGE_START -->[\s\S]*?<!-- CSS_PARITY_BADGE_END -->/,
  `<!-- CSS_PARITY_BADGE_START -->\n${badge}\n<!-- CSS_PARITY_BADGE_END -->`,
);
readme = readme.replace(
  /<!-- CSS_DOM_STATS_START -->[\s\S]*?<!-- CSS_DOM_STATS_END -->/,
  `<!-- CSS_DOM_STATS_START -->\n${stats}\n<!-- CSS_DOM_STATS_END -->`,
);
if (readme === antes && !/CSS_DOM_STATS_START/.test(antes)) {
  console.error("README.md sem as regiões CSS_PARITY_BADGE / CSS_DOM_STATS");
  process.exit(2);
}
writeFileSync(readmePath, readme);
writeFileSync(
  ".github/css_parity_report.json",
  JSON.stringify(
    { date: hoje, fixtures: { passing: passam, total: fixtures, pct: pctFix },
      measurements: { matching: batem, total: medicoes, pct: pctMed },
      wpt: wpt ? { suite: "css/css-flexbox", passing: wpt.passam, total: wpt.total, pct: pctWpt } : null,
      expected_failures: esperadas, dom_lots: lotes },
    null, 2) + "\n",
);
console.log(`CSS vs Chrome: ${pctMed}% (${batem}/${medicoes}) | fixtures ${pctFix}% (${passam}/${fixtures}) | DOM ${lotes.feitos.length}/${totalLotes} lotes`);
