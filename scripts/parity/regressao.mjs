// A lista LOST entre duas medições do nosso lado, elemento a elemento.
//
//   node scripts/parity/regressao.mjs --base out-base/rts.jsonl --novo out/rts.jsonl \
//        [--chrome out/chrome.jsonl] [--tol 1] [--top 20]
//
// Existe porque o `compare.mjs` responde "quantos casam AGORA" e essa pergunta
// não distingue +3 ganhos de +5 ganhos com 2 perdidos. O CLAUDE.md diz que a
// afirmação "sem regressão" é uma lista VAZIA e nunca um número líquido, e sem
// esta ferramenta essa lista teria de ser feita à mão a cada medição — que é
// como não ser feita.
//
// O árbitro é o mesmo Chrome nos dois lados: um elemento é GANHO se passou a
// casar com o Chrome e PERDIDO se deixou de casar. Comparar os dois dumps
// nossos entre si diria apenas "mudou", que é o que se quer quando se está a
// mudar código de propósito.

import { readFileSync } from "node:fs";

const arg = (nome, def) => {
  const i = process.argv.indexOf("--" + nome);
  return i >= 0 ? process.argv[i + 1] : def;
};
const TOL = Number(arg("tol", "1"));
const TOP = Number(arg("top", "20"));

function ler(ficheiro) {
  const m = new Map();
  let fim = null;
  for (const linha of readFileSync(ficheiro, "utf8").split("\n")) {
    if (!linha.trim()) continue;
    let o;
    try { o = JSON.parse(linha); } catch { continue; }
    if (o.__fim) { fim = o; continue; }
    if (o.__meta || o.erro !== undefined) continue;
    m.set(o.p, o);
  }
  // A mesma recusa do compare.mjs, e pela mesma razão: um dump cortado a meio é
  // legível e lê-se como "estes elementos deixaram de existir" — uma lista LOST
  // inteira produzida por um processo morto, não por uma regressão.
  if (!fim) throw new Error(`${ficheiro}: sem linha __fim — extração INCOMPLETA`);
  if (fim.emitidos !== m.size) {
    throw new Error(`${ficheiro}: __fim anuncia ${fim.emitidos}, lidos ${m.size}`);
  }
  return m;
}

const base = ler(arg("base", "scripts/parity/out-base/rts.jsonl"));
const novo = ler(arg("novo", "scripts/parity/out/rts.jsonl"));
const chrome = ler(arg("chrome", "scripts/parity/out/chrome.jsonl"));

const casa = (a, b) =>
  Math.abs(a.x - b.x) <= TOL && Math.abs(a.y - b.y) <= TOL &&
  Math.abs(a.w - b.w) <= TOL && Math.abs(a.h - b.h) <= TOL;

const erro = (a, b) => Math.max(
  Math.abs(a.x - b.x), Math.abs(a.y - b.y),
  Math.abs(a.w - b.w), Math.abs(a.h - b.h));

const ganhos = [], perdidos = [], sumiram = [], surgiram = [];
let comuns = 0, casavamAntes = 0, casamAgora = 0, erroAntes = 0, erroAgora = 0;

for (const [p, c] of chrome) {
  const a = base.get(p), b = novo.get(p);
  if (!a && !b) continue;
  if (a && !b) { sumiram.push(p); continue; }
  if (!a && b) { surgiram.push(p); continue; }
  comuns++;
  const ea = erro(c, a), eb = erro(c, b);
  erroAntes += ea; erroAgora += eb;
  const ka = ea <= TOL, kb = eb <= TOL;
  if (ka) casavamAntes++;
  if (kb) casamAgora++;
  if (!ka && kb) ganhos.push({ p, tag: c.tag, ea });
  if (ka && !kb) perdidos.push({ p, tag: c.tag, eb, c, b });
}

const n2 = (v) => Number(v).toFixed(2);
console.log(`REGRESSÃO POR ELEMENTO — tolerância ${TOL}px`);
console.log(`  base: ${arg("base", "scripts/parity/out-base/rts.jsonl")}`);
console.log(`  novo: ${arg("novo", "scripts/parity/out/rts.jsonl")}`);
console.log(`  árbitro: ${arg("chrome", "scripts/parity/out/chrome.jsonl")}\n`);
console.log(`  ${comuns} elementos presentes nas duas medições e no Chrome`);
if (sumiram.length) console.log(`  ! ${sumiram.length} estavam na base e SUMIRAM do novo dump`);
if (surgiram.length) console.log(`  + ${surgiram.length} não estavam na base e SURGIRAM`);
console.log(`  casavam ${casavamAntes}  →  casam ${casamAgora}   (líquido ${casamAgora - casavamAntes >= 0 ? "+" : ""}${casamAgora - casavamAntes})`);
console.log(`  soma do erro máximo: ${n2(erroAntes)}px  →  ${n2(erroAgora)}px\n`);
console.log(`  GANHOS   ${ganhos.length}`);
console.log(`  PERDIDOS ${perdidos.length}${perdidos.length === 0 ? "   ← a lista vazia É a afirmação \"sem regressão\"" : "   ← NÃO é sem regressão"}`);

if (perdidos.length) {
  console.log(`\n── OS ${Math.min(TOP, perdidos.length)} PIORES PERDIDOS ──`);
  for (const d of perdidos.sort((a, b) => b.eb - a.eb).slice(0, TOP)) {
    console.log(`  ${n2(d.eb)}px  <${d.tag}>  ${d.p}`);
    console.log(`             chrome  x=${n2(d.c.x)} y=${n2(d.c.y)} w=${n2(d.c.w)} h=${n2(d.c.h)}`);
    console.log(`             agora   x=${n2(d.b.x)} y=${n2(d.b.y)} w=${n2(d.b.w)} h=${n2(d.b.h)}`);
  }
}

const porTag = (xs) => {
  const g = new Map();
  for (const x of xs) g.set(x.tag, (g.get(x.tag) || 0) + 1);
  return [...g].sort((a, b) => b[1] - a[1]).slice(0, 8)
    .map(([t, n]) => `${n} <${t}>`).join(", ");
};
if (ganhos.length) console.log(`\n  ganhos por tag: ${porTag(ganhos)}`);
if (perdidos.length) console.log(`  perdidos por tag: ${porTag(perdidos)}`);

// Um perdido é um facto, não um alarme configurável: quem mede decide se a
// troca vale, mas o código de saída obriga a olhar.
process.exit(perdidos.length ? 1 : 0);
