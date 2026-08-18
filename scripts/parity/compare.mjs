// O COMPARADOR: junta os dois JSONL e diz onde divergimos do Chrome.
//
//   node scripts/parity/compare.mjs [--tol 1] [--top 20] [--rts a.jsonl] [--chrome b.jsonl]
//
// A tolerância é um PARÂMETRO e não uma constante escondida: 1px é o default
// porque é o arredondamento de um layout, mas a pergunta "quantos casam" tem
// respostas diferentes a 0.5 e a 2 e quem lê o número tem de poder mexer nele.
//
// A regra que manda neste ficheiro é a do CLAUDE.md — "verifique a ENTRADA, não
// só a saída". Nada é descontado do denominador em silêncio: um elemento que só
// existe num dos lados, uma linha que não fez parse, uma extração que falhou,
// tudo isso aparece no relatório com o seu número. Um "97% casam" medido sobre
// os elementos que sobreviveram à extração é uma afirmação vestida de medição.

import { readFileSync } from "node:fs";

const arg = (nome, def) => {
  const i = process.argv.indexOf("--" + nome);
  return i >= 0 ? process.argv[i + 1] : def;
};
const TOL = Number(arg("tol", "1"));
const TOP = Number(arg("top", "20"));
const F_RTS = arg("rts", "scripts/parity/out/rts.jsonl");
const F_CHROME = arg("chrome", "scripts/parity/out/chrome.jsonl");

const PROPS = ["display", "position", "color", "background-color", "font-size"];

/// Ler um JSONL de um dos lados. Devolve os elementos indexados por caminho E a
/// contabilidade da leitura — linhas ilegíveis, caminhos repetidos, o `emitidos`
/// que o extrator anunciou. É essa contabilidade que impede o denominador de
/// encolher sem se dar por isso.
function ler(ficheiro) {
  const bruto = readFileSync(ficheiro, "utf8").split("\n").filter((l) => l.trim());
  const porCaminho = new Map();
  let meta = null, fim = null, ilegiveis = 0, repetidos = 0, falhasExtracao = 0;
  for (const linha of bruto) {
    let o;
    try { o = JSON.parse(linha); } catch { ilegiveis++; continue; }
    if (o.__meta) { meta = o; continue; }
    if (o.__fim) { fim = o; continue; }
    if (o.erro !== undefined) { falhasExtracao++; continue; }
    if (porCaminho.has(o.p)) repetidos++;
    porCaminho.set(o.p, o);
  }
  return { ficheiro, porCaminho, meta, fim, ilegiveis, repetidos, falhasExtracao,
           linhas: bruto.length };
}

/// Um lado só é comparável se se anunciou completo. Um extrator cortado a meio
/// (processo morto, buffer truncado) produz um ficheiro perfeitamente legível
/// com metade dos elementos, e sem esta verificação isso lê-se como "metade da
/// árvore não existe no nosso motor" — a conclusão errada com a cara certa.
function integridade(lado) {
  const problemas = [];
  if (!lado.meta) problemas.push("sem linha __meta (extração não começou?)");
  if (!lado.fim) problemas.push("sem linha __fim — extração INCOMPLETA (cortada a meio)");
  else if (lado.fim.emitidos !== lado.porCaminho.size + lado.repetidos) {
    problemas.push(`__fim anuncia ${lado.fim.emitidos} mas foram lidos ` +
                   `${lado.porCaminho.size + lado.repetidos}`);
  }
  if (lado.ilegiveis) problemas.push(`${lado.ilegiveis} linhas não fizeram parse`);
  if (lado.repetidos) problemas.push(`${lado.repetidos} caminhos REPETIDOS (a regra de ` +
                                     `caminho não é única — o casamento é ambíguo)`);
  if (lado.falhasExtracao) problemas.push(`${lado.falhasExtracao} elementos falharam a extração`);
  return problemas;
}

const rts = ler(F_RTS);
const chrome = ler(F_CHROME);

const quantil = (xs, q) => xs.length ? xs[Math.min(xs.length - 1, Math.floor(xs.length * q))] : 0;
const n2 = (v) => Number(v).toFixed(2);

// ── casamento por caminho ────────────────────────────────────────────────────
const soChrome = [], soRts = [], comuns = [];
for (const [p, c] of chrome.porCaminho) {
  const r = rts.porCaminho.get(p);
  if (r) comuns.push([p, c, r]); else soChrome.push(p);
}
for (const p of rts.porCaminho.keys()) if (!chrome.porCaminho.has(p)) soRts.push(p);

// ── geometria ────────────────────────────────────────────────────────────────
const desvios = [];
for (const [p, c, r] of comuns) {
  const d = { x: Math.abs(c.x - r.x), y: Math.abs(c.y - r.y),
              w: Math.abs(c.w - r.w), h: Math.abs(c.h - r.h) };
  const pior = Math.max(d.x, d.y, d.w, d.h);
  desvios.push({ p, c, r, d, pior, casa: pior <= TOL });
}
const casam = desvios.filter((e) => e.casa).length;
const piores = [...desvios].sort((a, b) => b.pior - a.pior).slice(0, TOP);
const ordenados = desvios.map((e) => e.pior).sort((a, b) => a - b);

// ── propriedades ─────────────────────────────────────────────────────────────
// Normalizar cor: o Chrome serializa `rgb(0, 0, 0)` / `rgba(0, 0, 0, 0)`, nós
// podemos serializar de outra forma. Comparar as strings cruas contaria como
// divergência de COR o que é divergência de FORMATO, que é outra pergunta.
function normCor(v) {
  const m = String(v).match(/rgba?\(([^)]+)\)/);
  if (!m) return String(v).trim().toLowerCase();
  const n = m[1].split(",").map((s) => Number(s.trim()));
  if (n.length === 4 && n[3] === 0) return "transparent";
  return `rgb(${n[0]},${n[1]},${n[2]})` + (n.length === 4 && n[3] !== 1 ? `/${n[3]}` : "");
}
const norm = (k, v) => (k.includes("color") ? normCor(v) : String(v).trim().toLowerCase());

const props = {};
for (const k of PROPS) props[k] = { igual: 0, diferente: 0, naoReportado: 0, exemplos: [] };
for (const [p, c, r] of comuns) {
  for (const k of PROPS) {
    const a = props[k];
    // "" do nosso lado NÃO é uma cor errada: o `get_property` do rts-dom
    // devolve o valor CASCATEADO e vazio quer dizer "ninguém disse". O Chrome
    // responde sempre — herdado ou inicial. São perguntas diferentes e por isso
    // contam em colunas diferentes.
    if (String(r[k] ?? "") === "") { a.naoReportado++; continue; }
    if (norm(k, r[k]) === norm(k, c[k])) a.igual++;
    else {
      a.diferente++;
      if (a.exemplos.length < 5) a.exemplos.push(`${p}  chrome=${c[k]}  rts=${r[k]}`);
    }
  }
}

/// Agrupar por uma chave do lado do CHROME (o lado que sabe a resposta certa).
/// É isto que transforma "20 caixas erradas" em "12 delas são `display:grid`" —
/// a diferença entre uma lista de sintomas e um trabalho a fazer.
function agrupar(entradas, chave) {
  const m = new Map();
  for (const e of entradas) {
    const k = chave(e) || "(não reportado)";
    m.set(k, (m.get(k) ?? 0) + 1);
  }
  return [...m].sort((a, b) => b[1] - a[1]);
}

// ── relatório ────────────────────────────────────────────────────────────────
const L = [];
L.push("PARIDADE DE LAYOUT — RTS x Chrome");
L.push(`medido em ${new Date().toISOString()}, tolerância ${TOL}px`);
L.push(`  chrome: ${F_CHROME}  ${chrome.meta ? JSON.stringify(chrome.meta.viewport) : "?"}`);
L.push(`  rts:    ${F_RTS}  ${rts.meta ? JSON.stringify(rts.meta.viewport) : "?"}`);
L.push("");

L.push("── INTEGRIDADE DA ENTRADA ──");
for (const [nome, lado] of [["chrome", chrome], ["rts", rts]]) {
  const p = integridade(lado);
  L.push(`  ${nome}: ${lado.porCaminho.size} elementos lidos` +
         (p.length ? "" : "  — sem problemas"));
  for (const x of p) L.push(`    ! ${x}`);
}
if (chrome.meta && rts.meta &&
    JSON.stringify(chrome.meta.viewport) !== JSON.stringify(rts.meta.viewport)) {
  L.push("    ! VIEWPORTS DIFERENTES — a comparação não é sobre a mesma página");
}
L.push("");

L.push("── ÁRVORE ──");
const uniao = chrome.porCaminho.size + soRts.length;
L.push(`  ${comuns.length} de ${uniao} caminhos existem nos dois lados ` +
       `(${n2(100 * comuns.length / uniao)}%)`);
L.push(`  ${soChrome.length} só no Chrome, ${soRts.length} só no RTS`);
if (soChrome.length) {
  L.push("  tags que faltam do nosso lado (top 10):");
  for (const [t, n] of agrupar(soChrome, (p) => p.split("/").pop().replace(/\[\d+\]$/, "")).slice(0, 10)) {
    L.push(`    ${String(n).padStart(6)}  <${t}>`);
  }
}
if (soRts.length) {
  L.push("  tags a mais do nosso lado (top 10):");
  for (const [t, n] of agrupar(soRts, (p) => p.split("/").pop().replace(/\[\d+\]$/, "")).slice(0, 10)) {
    L.push(`    ${String(n).padStart(6)}  <${t}>`);
  }
}
L.push("");

L.push("── GEOMETRIA (só os caminhos comuns) ──");
L.push(`  ${casam} de ${comuns.length} casam dentro de ${TOL}px ` +
       `(${n2(100 * casam / (comuns.length || 1))}%)`);
L.push(`  e ${casam} de ${uniao} do total da união (${n2(100 * casam / (uniao || 1))}%) ` +
       `— o segundo é o que NÃO desconta a árvore divergente`);
L.push(`  erro máximo por elemento: mediana ${n2(quantil(ordenados, 0.5))}px, ` +
       `p90 ${n2(quantil(ordenados, 0.9))}px, p99 ${n2(quantil(ordenados, 0.99))}px, ` +
       `máx ${n2(ordenados[ordenados.length - 1] ?? 0)}px`);
for (const eixo of ["x", "y", "w", "h"]) {
  const xs = desvios.map((e) => e.d[eixo]).sort((a, b) => a - b);
  L.push(`    ${eixo}: mediana ${n2(quantil(xs, 0.5))}  p90 ${n2(quantil(xs, 0.9))}  ` +
         `máx ${n2(xs[xs.length - 1] ?? 0)}  ` +
         `(${xs.filter((v) => v > TOL).length} fora de tolerância)`);
}
L.push("");

const falham = desvios.filter((e) => !e.casa);
L.push(`── OS ${TOP} PIORES DESVIOS ──`);
for (const e of piores) {
  L.push(`  ${n2(e.pior).padStart(9)}px  ${e.p}`);
  L.push(`${" ".repeat(13)}chrome  x=${n2(e.c.x)} y=${n2(e.c.y)} w=${n2(e.c.w)} h=${n2(e.c.h)}` +
         `  display=${e.c.display} position=${e.c.position}`);
  L.push(`${" ".repeat(13)}rts     x=${n2(e.r.x)} y=${n2(e.r.y)} w=${n2(e.r.w)} h=${n2(e.r.h)}`);
}
L.push("");

L.push("── O QUE AGRUPA OS DESVIOS ──");
// "sem caixa" e "caixa no sítio errado" são bugs diferentes e a lista de piores
// desvios mistura os dois: um elemento a que não damos caixa nenhuma aparece com
// o erro do TAMANHO da página (dezenas de milhares de px) e afoga tudo o resto.
// Separá-los é o que torna o número accionável.
const semCaixa = falham.filter((e) => e.r.w === 0 && e.r.h === 0 && (e.c.w > 0 || e.c.h > 0));
const comCaixa = falham.filter((e) => !(e.r.w === 0 && e.r.h === 0 && (e.c.w > 0 || e.c.h > 0)));
L.push(`  ${semCaixa.length} dos ${falham.length} não têm caixa NENHUMA do nosso lado ` +
       `(w=h=0 onde o Chrome dá área); ${comCaixa.length} têm caixa no sítio errado`);
L.push("  os sem caixa, por display do Chrome (top 6):");
for (const [k, n] of agrupar(semCaixa, (e) => e.c.display).slice(0, 6)) {
  L.push(`    ${String(n).padStart(6)}  display:${k}`);
}
L.push("  os com caixa errada, erro máximo: mediana " +
       n2(quantil(comCaixa.map((e) => e.pior).sort((a, b) => a - b), 0.5)) + "px, p90 " +
       n2(quantil(comCaixa.map((e) => e.pior).sort((a, b) => a - b), 0.9)) + "px");
L.push("");
L.push(`  dos ${TOP} piores, por display do Chrome:`);
for (const [k, n] of agrupar(piores, (e) => e.c.display)) L.push(`    ${String(n).padStart(6)}  display:${k}`);
L.push(`  dos ${TOP} piores, por tag:`);
for (const [k, n] of agrupar(piores, (e) => e.c.tag).slice(0, 10)) L.push(`    ${String(n).padStart(6)}  <${k}>`);
L.push(`  dos ${falham.length} que falham a ${TOL}px, por display do Chrome (top 10):`);
for (const [k, n] of agrupar(falham, (e) => e.c.display).slice(0, 10)) {
  L.push(`    ${String(n).padStart(6)}  display:${k}`);
}
L.push(`  dos ${falham.length} que falham, por position do Chrome:`);
for (const [k, n] of agrupar(falham, (e) => e.c.position).slice(0, 6)) {
  L.push(`    ${String(n).padStart(6)}  position:${k}`);
}
L.push(`  dos ${falham.length} que falham, por tag (top 10):`);
for (const [k, n] of agrupar(falham, (e) => e.c.tag).slice(0, 10)) L.push(`    ${String(n).padStart(6)}  <${k}>`);
L.push("");

L.push("── PROPRIEDADES COMPUTADAS (só os caminhos comuns) ──");
L.push("  'não reportado' = o rts-dom devolveu \"\" — valor não cascateado, que é");
L.push("  outra pergunta que 'valor errado' e por isso não entra no denominador.");
for (const k of PROPS) {
  const a = props[k];
  const base = a.igual + a.diferente;
  L.push(`  ${k.padEnd(18)} ${String(a.igual).padStart(6)} iguais / ${base} comparáveis` +
         (base ? ` (${n2(100 * a.igual / base)}%)` : "") +
         `   ${a.naoReportado} não reportados`);
  for (const ex of a.exemplos) L.push(`      ${ex}`);
}

console.log(L.join("\n"));
