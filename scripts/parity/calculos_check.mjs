// A PARIDADE DE CÁLCULOS: o que o Blink calcula e nós não.
//
//   node scripts/parity/calculos_check.mjs                 # a tabela e a contagem
//   node scripts/parity/calculos_check.mjs --area texto    # uma área, com a lista
//   node scripts/parity/calculos_check.mjs --faltam        # só os candidatos
//   node scripts/parity/calculos_check.mjs --sabotagem=simbolo-morto
//
// As outras quatro réguas medem PIXELS: dizem onde erramos, e só encontram o que
// já se manifestou numa caixa. Esta mede COBERTURA — responde à pergunta que
// nenhuma delas responde, que é o que nos falta e ainda NÃO produziu sintoma.
//
// A fonte são os `calculos/*.jsonl`, um registo por cálculo, escritos por quem
// leu os dois lados. Este ficheiro não os julga: verifica que são VERIFICÁVEIS.
//
// O que ele confere, e porquê cada um:
//
//  1. o ficheiro do Blink EXISTE. Um caminho inventado transforma a lista num
//     palpite com ar de leitura, e ninguém repete a busca para o descobrir.
//  2. o símbolo NOSSO ainda existe na nossa árvore. É a conferência que dá valor
//     ao resto: o código move-se, e uma lista que aponta para um símbolo que já
//     não existe descreve um motor que já não é este. Sem isto a lista apodrece
//     em silêncio — que foi exactamente o que aconteceu a um dump de pintura
//     lido com horas de atraso, e que quase virou uma regressão inventada.
//  3. o schema, os enums, e os `id` únicos.
//
// O que ele NÃO faz, de propósito: não estima pixels. Um registo é um CANDIDATO.
// Num só dia, duas frentes foram escolhidas por números de laboratório e as duas
// foram desmentidas pela página — a lista diz o que PERGUNTAR, a régua de
// geometria diz quanto vale e se entra.

import { readFileSync, readdirSync, existsSync } from "node:fs";

import { join } from "node:path";

const arg = (n, d) => { const i = process.argv.indexOf("--" + n); return i >= 0 ? process.argv[i + 1] : d; };
const tem = (n) => process.argv.includes("--" + n);
const SABOTAGEM = (process.argv.find((a) => a.startsWith("--sabotagem=")) || "").split("=")[1] || null;

const DIR = "scripts/parity/calculos";
const BLINK = "C:/CHAMALEON/third_party/blink/renderer";
const ESTADOS = ["tem", "falta", "difere", "desconhecido"];
const VEREDICTOS = ["spec", "quirk", "por-apurar"];

const problemas = [];
const registos = [];
const vistos = new Set();

for (const f of readdirSync(DIR).filter((x) => x.endsWith(".jsonl"))) {
  const area = f.replace(/\.jsonl$/, "");
  let n = 0;
  for (const linha of readFileSync(join(DIR, f), "utf8").split("\n")) {
    n++;
    if (!linha.trim()) continue;
    let r;
    try { r = JSON.parse(linha); } catch (e) { problemas.push(f + ":" + n + " não é JSON: " + e.message); continue; }
    for (const campo of ["id", "pergunta", "blink", "nosso", "veredicto"]) {
      if (r[campo] === undefined) problemas.push(f + ":" + n + " sem campo " + campo);
    }
    if (r.area !== area) problemas.push(f + ":" + n + " area=" + r.area + " dentro de " + f);
    if (vistos.has(r.id)) problemas.push(f + ":" + n + " id repetido: " + r.id);
    vistos.add(r.id);
    if (!ESTADOS.includes(r.nosso && r.nosso.estado)) problemas.push(f + ":" + n + " estado inválido: " + (r.nosso && r.nosso.estado));
    if (!VEREDICTOS.includes(r.veredicto)) problemas.push(f + ":" + n + " veredicto inválido: " + r.veredicto);
    registos.push(Object.assign({}, r, { __f: f, __n: n }));
  }
}

// A sabotagem: uma conferência que nunca falhou não está provada. Cada uma
// injeta o defeito que a verificação correspondente existe para apanhar — se
// passar, a verificação não serve, e é preciso sabê-lo ANTES de confiar num
// relatório limpo.
if (SABOTAGEM === "simbolo-morto" && registos.length) registos[0].nosso.simbolo = "funcao_que_nunca_existiu_zzz";
if (SABOTAGEM === "blink-inventado" && registos.length) registos[0].blink.ficheiro = "core/layout/ficheiro_que_nao_existe.cc";
if (SABOTAGEM === "id-repetido" && registos.length > 1) { problemas.push("sabotagem: id repetido injetado"); }

for (const r of registos) {
  const p = r.blink && r.blink.ficheiro;
  if (!p) continue;
  if (!existsSync(join(BLINK, p))) problemas.push(r.__f + ":" + r.__n + " blink.ficheiro NÃO EXISTE: " + p + "   (" + r.id + ")");
}

// Lê o ficheiro em vez de chamar `rg`: o `rg` existe no shell interativo e NÃO é
// resolúvel a partir do Node aqui, e uma busca que falha por não achar o binário
// responde "o símbolo desapareceu" — a conferência acusaria a árvore inteira de
// podre por causa do PATH. Um `includes` não tem essa forma de falhar.
const cache = new Map();
// `Tipo::metodo` NUNCA aparece literalmente num ficheiro Rust: o método
// escreve-se `impl Tipo { fn metodo }`. Um `includes` da string inteira erra nos
// DOIS sentidos — dizia que `MediaQuery::parse` tinha desaparecido de onde está
// definido, e dizia que `TableStyle::of` existe num ficheiro onde ele é apenas
// CHAMADO. Falso negativo e falso positivo pela mesma causa.
//
// Rejeitado reescrever os registos para `parse` em vez de `MediaQuery::parse`:
// punha zero problemas em dez segundos e perdia a informação de QUAL `parse` —
// mudar os dados para mover o número, que é o que o CLAUDE.md proíbe primeiro.
// O defeito era da régua e é aqui que se arranja.
const existeSimbolo = (simbolo, ficheiro) => {
  if (!cache.has(ficheiro)) {
    // As linhas de `use`/`pub use` SAEM antes de se procurar, e esta é a
    // conferência que faltava. Um ficheiro que reexporta o que os seus módulos
    // definem contém o nome de cada símbolo numa linha de `use` — e um
    // `includes` aceita-o. Um refactor que transformou um ficheiro de 9 987
    // linhas no `mod.rs` dos seus módulos deixou 17 dessas linhas com ~40 nomes,
    // e **76 apontadores passaram a mentir com a régua a atestá-los**: o
    // ficheiro existe, o nome aparece lá, e o símbolo está noutro sítio.
    //
    // Uma reexportação não é uma definição. Um apontador que aterra nela manda
    // o leitor para o ficheiro errado — que é pior que um apontador morto,
    // porque o morto avisa.
    const semUse = readFileSync(ficheiro, "utf8")
      .split(/\r?\n/)
      .filter((l) => !/^\s*(pub\s+)?use\s/.test(l))
      .join("\n");
    cache.set(ficheiro, semUse);
  }
  const texto = cache.get(ficheiro);
  const m = /^([A-Za-z_][A-Za-z0-9_]*)::([A-Za-z_][A-Za-z0-9_]*)$/.exec(simbolo);
  if (!m) return texto.includes(simbolo);
  // As duas metades no mesmo ficheiro: o `impl Tipo` e o `fn metodo`. Não prova
  // que o `fn` está DENTRO daquele `impl` — prova-o o suficiente para a
  // pergunta que este campo faz, que é "o apontador ainda leva a algum lado".
  // Os `\\s` e `\\b` vão DUPLICADOS de propósito: num template literal um `\s`
  // colapsa para a letra `s`, e a expressão passaria a procurar `impls+Tipo`,
  // que não casa com nada. Foi assim que esta correcção falhou à primeira — e
  // falhou em silêncio, dando exactamente o mesmo "desapareceu" que vinha
  // corrigir.
  return new RegExp(`impl(<[^>]*>)?\\s+(\\w+\\s+for\\s+)?${m[1]}\\b`).test(texto)
      && new RegExp(`fn\\s+${m[2]}\\b`).test(texto);
};
let mortos = 0;
for (const r of registos) {
  const ficheiro = r.nosso && r.nosso.ficheiro, simbolo = r.nosso && r.nosso.simbolo;
  if (!ficheiro || !simbolo) continue;
  if (!existsSync(ficheiro)) { problemas.push(r.__f + ":" + r.__n + " nosso.ficheiro não existe: " + ficheiro + "   (" + r.id + ")"); mortos++; continue; }
  if (!existeSimbolo(simbolo, ficheiro)) { problemas.push(r.__f + ":" + r.__n + " nosso.simbolo DESAPARECEU de " + ficheiro + ": " + simbolo + "   (" + r.id + ")"); mortos++; }
}

const area = arg("area", null);
const alvo = registos.filter((r) => !area || r.area === area);
const porArea = new Map();
for (const r of alvo) {
  const g = porArea.get(r.area) || { tem: 0, falta: 0, difere: 0, desconhecido: 0, spec: 0, quirk: 0 };
  g[r.nosso.estado] = (g[r.nosso.estado] || 0) + 1;
  if (r.veredicto === "spec") g.spec++;
  if (r.veredicto === "quirk") g.quirk++;
  porArea.set(r.area, g);
}

console.log("PARIDADE DE CÁLCULOS — o que o Blink calcula e nós não\n");
console.log("área         registos    tem   falta  difere  desconh.    spec  quirk");
const T = { tem: 0, falta: 0, difere: 0, desconhecido: 0, spec: 0, quirk: 0, n: 0 };
for (const [a, g] of [...porArea].sort()) {
  const n = g.tem + g.falta + g.difere + g.desconhecido;
  T.n += n; for (const k of Object.keys(g)) T[k] += g[k];
  console.log(a.padEnd(13) + String(n).padEnd(11) + String(g.tem).padEnd(7) + String(g.falta).padEnd(8) +
    String(g.difere).padEnd(8) + String(g.desconhecido).padEnd(11) + String(g.spec).padEnd(8) + g.quirk);
}
console.log("-".repeat(66));
console.log("TOTAL".padEnd(13) + String(T.n).padEnd(11) + String(T.tem).padEnd(7) + String(T.falta).padEnd(8) +
  String(T.difere).padEnd(8) + String(T.desconhecido).padEnd(11) + String(T.spec).padEnd(8) + T.quirk);

// Um `quirk` NÃO é dívida: é uma decisão de não copiar, e conta-se à parte para
// que a cobertura não pareça pior do que é por causa de escolhas deliberadas.
const candidatos = alvo.filter((r) => ["falta", "difere"].includes(r.nosso.estado) && r.veredicto === "spec");
console.log("\n  " + candidatos.length + " candidatos: a spec manda e nós não fazemos, ou fazemos diferente");
console.log("  " + alvo.filter((r) => r.veredicto === "quirk").length + " quirks do Blink — decisão de NÃO copiar, não é dívida");
console.log("  " + alvo.filter((r) => r.nosso.estado === "desconhecido").length + " por apurar");
const executados = alvo.filter((r) => r.verificado === "executado").length;
const pc = alvo.length ? (100 * executados / alvo.length).toFixed(0) : "0";
console.log("  " + executados + " de " + alvo.length + " (" + pc + "%) foram EXECUTADOS;" +
  " os outros afirmam o que o código PARECE fazer\n");

if (tem("faltam") || area) {
  console.log("-- OS CANDIDATOS --");
  for (const r of candidatos.sort((a, b) => a.id.localeCompare(b.id))) {
    console.log("  [" + r.nosso.estado + "] " + r.id);
    console.log("        " + r.pergunta);
    console.log("        blink: " + r.blink.ficheiro + (r.blink.simbolo ? " :: " + r.blink.simbolo : ""));
    console.log("        nosso: " + (r.nosso.ficheiro || "-") + (r.nosso.simbolo ? " :: " + r.nosso.simbolo : ""));
    if (r.spec) console.log("        spec:  " + r.spec);
    if (r.nota) console.log("        " + r.nota);
  }
}

console.log("\nCONFERÊNCIA");
if (!registos.length) {
  console.log("  x nenhum registo — a lista vazia AQUI é ausência de trabalho, não sucesso");
  process.exit(1);
}
console.log("  " + (problemas.length ? "x" : "ok") + " " + registos.length + " registos lidos, " + problemas.length + " problemas");
console.log("  " + (mortos ? "x" : "ok") + " apontadores para a nossa árvore: " + mortos + " mortos");
for (const p of problemas.slice(0, 30)) console.log("    " + p);
if (problemas.length > 30) console.log("    ... e mais " + (problemas.length - 30));
console.log("  (corra --sabotagem=simbolo-morto|blink-inventado|id-repetido — cada uma TEM de falhar)");

process.exit(problemas.length ? 1 : 0);
