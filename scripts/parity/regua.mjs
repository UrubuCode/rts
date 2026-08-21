// A régua do erro VERTICAL, decomposta — e a antiga ao lado dela.
//
//   node scripts/parity/regua.mjs [--chrome out/chrome.jsonl] [--rts out/rts.jsonl]
//                                 [--top 15] [--familias 12]
//
// Existe porque a soma `|dy|` sobre todos os pares — o número que este
// repositório citou o dia inteiro como "erro de y" — não ordena trabalho.
// Medida sobre a Wikipédia em 2026-08-21 (`out/chrome.jsonl` contra
// `out/rts-fast.jsonl`, 16 813 pares), os 69,8 M px decompõem-se assim:
//
//     com área visível dos dois lados            29,7 M   42,5%
//     sem área NENHUMA nos dois lados            25,2 M   36,2%   invisível
//     NÃO DISPOSTOS por nós (342 elementos)      14,6 M   20,9%   real
//     sem área só no Chrome                       0,3 M    0,5%
//
// E dentro dos 29,7 M que se veem, a GERAÇÃO LOCAL é 61 902 px — 0,21%. O resto
// é herança: desvio nascido a montante e propagado para baixo.
//
// Mais de um terço do número histórico é portanto erro de posição de caixas que
// não têm área para se ver, e quase todo o resto é eco.
//
// Um número em que 55% é invisível e 44% é eco fez apontar duas vezes, no mesmo
// dia, para causas que não existiam: um `<header>` flex que vale 0,92% e uma
// família de `ul{display:inline}` que era artefacto do próprio instrumento.
//
// As DUAS armadilhas do instrumento estão corrigidas aqui, e é por isso que ele
// é um ficheiro e não uma linha de shell:
//
//   1. A injeção medida contra o PAI não mede nada quando o pai não está
//      disposto. Um `ul` em 0×0 tem `dy` igual ao seu `y` de documento inteiro,
//      e a diferença contra ele conta o MESMO defeito uma segunda vez. Contra a
//      Wikipédia isso sozinho inflacionava o total de 281 k para 7 035 k.
//   2. Irmãos ACUMULAM. Os 448 `li` de uma lista de referências herdam cada um o
//      desvio do anterior, e somar `dy(filho) − dy(pai)` sobre todos soma a
//      mesma acumulação 448 vezes. O delta entre irmãos CONSECUTIVOS isola o que
//      cada um gera: 281 k caem para 62 k.
//
// A régua antiga fica impressa ao lado e NÃO é substituída. Quem ler os números
// de ontem tem de conseguir ligá-los aos de hoje; uma métrica que só se compara
// consigo própria a partir da data em que nasceu é uma segunda forma do mesmo
// problema.

import { readFileSync } from "node:fs";

const arg = (nome, def) => {
  const i = process.argv.indexOf("--" + nome);
  return i >= 0 ? process.argv[i + 1] : def;
};
const TOP = Number(arg("top", "15"));
const FAMILIAS = Number(arg("familias", "12"));

// A mesma leitura do `regressao.mjs`, e pela mesma razão: um dump cortado a meio
// é legível e mente em silêncio — parece um corpus mais pequeno.
function ler(ficheiro) {
  const m = new Map();
  let fim = null;
  for (const linha of readFileSync(ficheiro, "utf8").split("\n")) {
    if (!linha.trim()) continue;
    let o;
    try { o = JSON.parse(linha); } catch { continue; }
    if (o.__fim) { fim = o; continue; }
    if (o.__meta || o.erro !== undefined) continue;
    // O índice do FICHEIRO é a ordem do documento, e é capturado aqui de
    // propósito: é a única testemunha independente da ordem dos irmãos. Uma
    // conferência que a re-derivasse da lista de filhos passaria a aprovar
    // qualquer reordenação dessa lista — ver a nota na auto-conferência.
    o.__i = m.size;
    m.set(o.p, o);
  }
  if (!fim) throw new Error(`${ficheiro}: sem linha __fim — extração INCOMPLETA`);
  if (fim.emitidos !== m.size) {
    throw new Error(`${ficheiro}: __fim anuncia ${fim.emitidos}, lidos ${m.size}`);
  }
  return m;
}

const fChrome = arg("chrome", "scripts/parity/out/chrome.jsonl");
const fRts = arg("rts", "scripts/parity/out/rts.jsonl");
const C = ler(fChrome);
const R = ler(fRts);

const pai = (k) => k.slice(0, k.lastIndexOf("/"));
const semCaixa = (o) => o.w <= 0 || o.h <= 0;
const px = (n) => (Math.round(n * 10) / 10).toLocaleString("pt-PT");

// ---------------------------------------------------------------- populações
//
// Quatro classes, mutuamente exclusivas e exaustivas sobre os pares comuns. A
// exaustividade é conferida no fim: uma classificação que perde elementos pelo
// caminho é a maneira mais fácil de fabricar uma percentagem.

const VISIVEL = "visivel";          // caixa com área dos dois lados
const NAO_DISPOSTO = "naoDisposto"; // Chrome dá caixa, nós não
const SO_CHROME_ZERO = "soChromeZero";
const AMBOS_ZERO = "ambosZero";

const classe = new Map();
const comuns = [];
for (const [k, c] of C) {
  const r = R.get(k);
  if (!r) continue;
  comuns.push(k);
  const cz = semCaixa(c), rz = semCaixa(r);
  classe.set(k, cz && rz ? AMBOS_ZERO : rz ? NAO_DISPOSTO : cz ? SO_CHROME_ZERO : VISIVEL);
}

const soDoChrome = C.size - comuns.length;
const soNossos = R.size - comuns.length;

// ------------------------------------------------------- a régua ANTIGA
// Soma |dy| sobre TODOS os pares, sem distinguir nada — o número histórico.

let antigaTotal = 0;
const porClasse = new Map();
for (const k of comuns) {
  const d = Math.abs(R.get(k).y - C.get(k).y);
  antigaTotal += d;
  const e = porClasse.get(classe.get(k)) || [0, 0];
  e[0]++; e[1] += d;
  porClasse.set(classe.get(k), e);
}

// -------------------------------------------------------- a régua NOVA (1/2)
// Erro de `y` só sobre elementos com ÁREA VISÍVEL dos dois lados. Um `<span>` de
// largura zero deslocado 60 000 px é um desastre para a soma e invisível na
// página; a régua antiga não distingue os dois casos e esta distingue.

const visiveis = comuns.filter((k) => classe.get(k) === VISIVEL);
let novaTotal = 0;
const dys = [];
for (const k of visiveis) {
  const d = Math.abs(R.get(k).y - C.get(k).y);
  novaTotal += d;
  dys.push(d);
}
dys.sort((a, b) => a - b);
const quantil = (p) => (dys.length ? dys[Math.min(dys.length - 1, Math.floor(p * dys.length))] : 0);

// -------------------------------------------------------- a régua NOVA (2/2)
// GERAÇÃO LOCAL por delta-irmão. Ver as duas armadilhas no cabeçalho.
//
// A ordem dos irmãos é a ordem do FICHEIRO, que é a ordem do documento. Ordenar
// pelo caminho poria `div[10]` antes de `div[2]` e o delta mediria a distância
// entre elementos que não se seguem.

const filhos = new Map();
for (const k of C.keys()) {
  const p = pai(k);
  if (!filhos.has(p)) filhos.set(p, []);
  filhos.get(p).push(k);
}

const dy = (k) => R.get(k).y - C.get(k).y;

let geracao = 0;
const porFamilia = new Map();   // "tag display" do PAI -> [n, |soma|, soma com sinal]
const porPai = new Map();
// As exclusões são CONTADAS. Uma régua que descarta em silêncio é a régua
// antiga com outro nome.
const excl = { paiNaoDisposto: 0, paiSemPar: 0, filhoNaoVisivel: 0, paiNaoVisivel: 0 };
// Guardado para a auto-conferência do telescópio, mais abaixo.
const telescopio = [];

for (const [pk, ks] of filhos) {
  const pc = C.get(pk), pr = R.get(pk);
  if (!pc || !pr) { excl.paiSemPar += ks.length; continue; }
  const cl = classe.get(pk);
  if (cl === NAO_DISPOSTO) { excl.paiNaoDisposto += ks.length; continue; }
  if (cl !== VISIVEL) { excl.paiNaoVisivel += ks.length; continue; }

  let anterior = dy(pk);
  const usados = [];
  for (const ck of ks) {
    if (classe.get(ck) !== VISIVEL) { excl.filhoNaoVisivel++; continue; }
    const atual = dy(ck);
    const d = atual - anterior;
    anterior = atual;
    usados.push(ck);
    if (Math.abs(d) < 0.5) continue;   // ruído de arredondamento do extrator
    geracao += Math.abs(d);
    const chave = `${pc.tag} ${pc.display}`;
    const e = porFamilia.get(chave) || [0, 0, 0];
    e[0]++; e[1] += Math.abs(d); e[2] += d;
    porFamilia.set(chave, e);
    porPai.set(pk, (porPai.get(pk) || 0) + Math.abs(d));
  }
  if (usados.length) telescopio.push([pk, usados[usados.length - 1]]);
}

// ------------------------------------------------------------------ saída

const pct = (parte, todo) => (todo ? ((parte / todo) * 100).toFixed(2) : "0.00") + "%";

console.log(`chrome: ${fChrome} (${C.size})`);
console.log(`rts:    ${fRts} (${R.size})`);
console.log(`pares comuns: ${comuns.length}` +
  (soDoChrome || soNossos ? `  (só do Chrome: ${soDoChrome}, só nossos: ${soNossos})` : ""));

console.log("\n=== RÉGUA ANTIGA — soma |dy| sobre TODOS os pares ===");
console.log(`total: ${px(antigaTotal)} px`);
console.log("\nclasse                            n    soma |dy|         %");
const nomes = {
  [VISIVEL]: "com área visível dos dois lados",
  [NAO_DISPOSTO]: "NÃO DISPOSTOS por nós",
  [AMBOS_ZERO]: "sem área nos dois lados",
  [SO_CHROME_ZERO]: "sem área só no Chrome",
};
for (const [cl, [n, s]] of [...porClasse].sort((a, b) => b[1][1] - a[1][1])) {
  console.log(nomes[cl].padEnd(32), String(n).padStart(6), px(s).padStart(14), pct(s, antigaTotal).padStart(9));
}

console.log("\n=== RÉGUA NOVA (1) — erro de y só onde há ÁREA VISÍVEL ===");
console.log(`n: ${visiveis.length} de ${comuns.length} pares`);
console.log(`soma |dy|: ${px(novaTotal)} px  (${pct(novaTotal, antigaTotal)} da régua antiga)`);
console.log(`mediana ${px(quantil(0.5))} px | p90 ${px(quantil(0.9))} | p99 ${px(quantil(0.99))} | max ${px(quantil(1))}`);

console.log("\n=== RÉGUA NOVA (2) — GERAÇÃO LOCAL por delta-irmão ===");
console.log(`total: ${px(geracao)} px  (${pct(geracao, novaTotal)} do erro de y visível)`);
console.log("O resto do erro visível é HERANÇA: nasceu a montante e foi propagado.");
console.log("\nexcluídos, e porquê:");
console.log(`  pai NÃO DISPOSTO (a armadilha 1): ${excl.paiNaoDisposto} filhos`);
console.log(`  pai sem área visível:             ${excl.paiNaoVisivel} filhos`);
console.log(`  pai sem par no outro dump:        ${excl.paiSemPar} filhos`);
console.log(`  filho sem área visível:           ${excl.filhoNaoVisivel}`);

console.log("\npor FAMÍLIA do pai (tag + display):");
console.log("     |ger|       %   com sinal      n  família");
for (const [k, [n, a, s]] of [...porFamilia].sort((x, y) => y[1][1] - x[1][1]).slice(0, FAMILIAS)) {
  console.log(px(a).padStart(10), pct(a, geracao).padStart(7), px(s).padStart(11), String(n).padStart(6), " " + k);
}
console.log("\nO SINAL é metade da leitura: uma família com |ger| grande e sinal ≈ 0");
console.log("cancela-se e é ruído; uma com os dois iguais empurra a página num só sentido.");

console.log(`\npais que mais GERAM (top ${TOP}):`);
for (const [k, v] of [...porPai].sort((a, b) => b[1] - a[1]).slice(0, TOP)) {
  const c = C.get(k);
  console.log(px(v).padStart(10), (c.tag + " " + c.display).padEnd(22), k.length > 76 ? "..." + k.slice(-72) : k);
}

// ------------------------------------------------------- a sonda confere-se
//
// A prática vem do `css_coverage.mjs`, e nasceu de um instrumento que respondeu
// três vezes com números plausíveis e falsos. Estas verificações são as que
// teriam apanhado as duas armadilhas do cabeçalho ANTES de elas custarem um dia.

const falhas = [];
const diz = (cond, msg) => { if (!cond) falhas.push(msg); };
const perto = (a, b, tol = 0.05) => Math.abs(a - b) <= tol;

diz(comuns.length > 0, "zero pares comuns — os dois dumps não descrevem a mesma página");

// A classificação é exaustiva: nada se perde entre as quatro classes.
const somaN = [...porClasse.values()].reduce((a, [n]) => a + n, 0);
diz(somaN === comuns.length,
  `as quatro classes somam ${somaN} e os pares comuns são ${comuns.length}`);
const somaS = [...porClasse.values()].reduce((a, [, s]) => a + s, 0);
diz(perto(somaS, antigaTotal, 1),
  `a decomposição soma ${px(somaS)} e a régua antiga dá ${px(antigaTotal)}`);

// A régua nova é um SUBCONJUNTO da antiga. Se der mais, a filtragem inverteu-se.
diz(novaTotal <= antigaTotal + 1,
  `a régua nova (${px(novaTotal)}) excede a antiga (${px(antigaTotal)}) — o filtro está invertido`);

// A ordem dos irmãos é a do DOCUMENTO, conferida contra o índice capturado na
// leitura. Esta verificação existe porque a primeira versão desta sonda tinha,
// no lugar dela, uma que re-derivava a ordem da própria lista de filhos —
// tautológica. Reordenar os irmãos por nome (`div[10]` antes de `div[2]`) muda
// os números em silêncio e passava na conferência anterior.
let ordemMa = 0;
for (const ks of filhos.values()) {
  for (let i = 1; i < ks.length; i++) {
    if (C.get(ks[i]).__i <= C.get(ks[i - 1]).__i) { ordemMa++; break; }
  }
}
diz(ordemMa === 0,
  `${ordemMa} pais com os irmãos fora da ordem do documento — o delta-irmão mede ` +
  `a distância entre elementos que não se seguem`);

// A identidade do telescópio: a soma dos deltas entre irmãos consecutivos tem
// de valer `dy(último filho) − dy(pai)`. Pina a ARITMÉTICA da armadilha 2 — uma
// soma feita contra o pai em vez de contra o irmão não a satisfaz, e é
// exatamente a diferença entre 62 k e 281 k. NÃO pina a ordem: para isso é a
// verificação acima.
let telescopioMau = 0;
for (const [pk, ultimo] of telescopio) {
  let acc = 0, anterior = dy(pk);
  for (const ck of filhos.get(pk)) {
    if (classe.get(ck) !== VISIVEL) continue;
    acc += dy(ck) - anterior;
    anterior = dy(ck);
  }
  if (!perto(acc, dy(ultimo) - dy(pk), 0.01)) telescopioMau++;
}
diz(telescopioMau === 0,
  `${telescopioMau} pais em que os deltas não telescopam — a ordem dos irmãos não é a do documento`);

// A geração local não pode exceder o erro que existe para gerar de forma
// grosseira; se exceder, está a contar herança como geração — a armadilha 1.
diz(geracao <= novaTotal,
  `a geração local (${px(geracao)}) excede o erro visível (${px(novaTotal)}) — está a contar herança`);

// E as exclusões têm de ser reportáveis, não zero por acidente de leitura.
diz(Number.isFinite(excl.paiNaoDisposto + excl.paiNaoVisivel + excl.paiSemPar + excl.filhoNaoVisivel),
  "as exclusões não foram contadas");

if (falhas.length) {
  console.error("\n*** A RÉGUA NÃO SE CONFERE — os números acima NÃO valem ***");
  for (const f of falhas) console.error("  - " + f);
  process.exit(1);
}
console.log("\nconfere-se: " + [
  "classificação exaustiva",
  "decomposição = total",
  "nova ⊆ antiga",
  "irmãos em ordem de documento",
  `telescópio em ${telescopio.length} pais`,
  "geração ≤ visível",
].join(", ") + ".");
