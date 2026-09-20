// Gera `RUNTIME-NOTICE.txt`: o aviso agregado das licencas que chegam ao
// binario que o UTILIZADOR compila.
//
//   node scripts/notice/build_notice.mjs     # escreve RUNTIME-NOTICE.txt
//
// PORQUE existe, que e a parte que um inventario nao diz: o
// `THIRD-PARTY-LICENSES.txt` lista o workspace inteiro e nao tem os TEXTOS. Mas
// quem compila um programa com o `rts` liga estaticamente o `rts-runtime`, e o
// BSD-2/3-Clause e o Apache-2.0 que la estao pedem que o aviso de copyright seja
// reproduzido "na documentacao ou noutros materiais fornecidos com a
// distribuicao" — e a distribuicao, nesse caso, e o binario DELES. Sem este
// ficheiro nao tinham como cumprir uma obrigacao que passou a ser deles por
// terem usado este compilador. O THIRD-PARTY-NOTICES.md registava isto como
// devido desde antes deste script existir.
//
// O que este ficheiro NAO e: o inventario do workspace. As dependencias que so o
// COMPILADOR usa ficam de fora de proposito — correm na maquina de quem
// desenvolve e nao saem dela, portanto nao entram no binario de ninguem. Incluir
// tudo faria um aviso maior e menos verdadeiro, que e a forma mais facil de
// ninguem o ler.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const OUT = join(ROOT, "RUNTIME-NOTICE.txt");
const RAIZ = "rts-runtime"; // o staticlib que entra no binario do utilizador

// Um ficheiro de licenca chama-se uma destas coisas e nunca outra. Procurado
// pelo NOME e nao por conteudo: um `README` que cita uma licenca nao e a
// licenca, e adivinhar qual e das duas e exatamente o erro que este ficheiro
// existe para nao cometer.
const NOMES = /^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)([-.].*)?$/i;

function metadata() {
  const raw = execFileSync("cargo", ["metadata", "--format-version", "1"], {
    cwd: ROOT,
    maxBuffer: 256 * 1024 * 1024,
    encoding: "utf8",
  });
  return JSON.parse(raw);
}

// O fecho das dependencias NORMAIS a partir do staticlib.
//
// `dev` fica fora porque um teste nao entra em biblioteca nenhuma. `build` fica
// fora porque um `build.rs` corre no compilador e nao e ligado — com uma
// excecao que NAO e excecao: um `-sys` cujo `build.rs` compila C vendoriza esse
// C na propria crate, portanto a crate ja esta no fecho por si.
//
// Todos os alvos, e nao so o desta maquina: quem compila para outro sistema liga
// outra arvore, e um aviso que so valesse para Windows mentia em Linux.
function fecho(meta) {
  const nos = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const pkgs = new Map(meta.packages.map((p) => [p.id, p]));
  const raiz = meta.packages.find((p) => p.name === RAIZ);
  if (!raiz) throw new Error(`nao encontrei o pacote ${RAIZ} no workspace`);

  const vistos = new Set();
  const porVer = [raiz.id];
  while (porVer.length) {
    const id = porVer.pop();
    if (vistos.has(id)) continue;
    vistos.add(id);
    for (const d of nos.get(id)?.deps ?? []) {
      const normal = d.dep_kinds.some((k) => k.kind === null || k.kind === "normal");
      if (normal && !vistos.has(d.pkg)) porVer.push(d.pkg);
    }
  }
  return [...vistos].map((id) => pkgs.get(id)).filter(Boolean);
}

function textos(pkg) {
  const dir = dirname(pkg.manifest_path);
  const achados = [];
  // `license-file` no manifesto ganha ao nome no diretorio: e o que o autor
  // APONTOU, contra o que nos encontrariamos.
  if (pkg.license_file) {
    const p = join(dir, pkg.license_file);
    if (existsSync(p)) achados.push([pkg.license_file, readFileSync(p, "utf8")]);
  }
  let entradas = [];
  try {
    entradas = readdirSync(dir, { withFileTypes: true });
  } catch {
    return achados; // fonte nao descarregada: dito no relatorio, nao inventado
  }
  for (const e of entradas.sort((a, b) => a.name.localeCompare(b.name))) {
    if (!e.isFile() || !NOMES.test(e.name)) continue;
    if (achados.some(([n]) => n === e.name)) continue;
    achados.push([e.name, readFileSync(join(dir, e.name), "utf8")]);
  }
  return achados;
}

const meta = metadata();
const nossos = new Set(meta.workspace_members);
const alcancados = fecho(meta)
  .filter((p) => !nossos.has(p.id))
  .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

// Um texto, uma vez, com a lista de quem o traz.
//
// A alternativa — o texto inteiro por cada crate — dava 5 MB para 598 pacotes,
// porque trezentos deles trazem O MESMO Apache-2.0 palavra por palavra. Nenhuma
// licenca aqui pede que o texto apareca uma vez POR obra; pedem que apareca com
// a distribuicao, e uma vez basta desde que se diga a quem se aplica. E um aviso
// de 5 MB tem um problema que um de 300 kB nao tem: ninguem o abre.
//
// A chave e o texto NORMALIZADO — espacos colapsados — porque a mesma licenca
// chega com quebras de linha diferentes conforme quem a copiou, e isso nao e uma
// diferenca de termos.
const semTexto = [];
const porTexto = new Map();
for (const p of alcancados) {
  const achados = textos(p);
  const quem = `${p.name} ${p.version}`;
  if (!achados.length) {
    semTexto.push(`${quem} (${p.license ?? "sem campo license"})`);
    continue;
  }
  for (const [nome, texto] of achados) {
    const chave = texto.replace(/\s+/g, " ").trim();
    if (!porTexto.has(chave)) porTexto.set(chave, { nome, texto: texto.trimEnd(), quem: [] });
    porTexto.get(chave).quem.push(quem);
  }
}

const grupos = [...porTexto.values()].sort((a, b) => b.quem.length - a.quem.length);
const blocos = grupos.map((g, i) => {
  const lista = g.quem.sort().map((q) => `  ${q}`).join("\n");
  return (
    `${"=".repeat(78)}\n` +
    `[${i + 1}/${grupos.length}] ${g.nome} — aplica-se a ${g.quem.length} ` +
    `${g.quem.length === 1 ? "pacote" : "pacotes"}:\n${lista}\n` +
    `${"=".repeat(78)}\n\n${g.texto}\n`
  );
});

const inventario = alcancados
  .map((p) => `  ${p.name} ${p.version}  —  ${p.license ?? "(nao declarado)"}`)
  .join("\n");

const hoje = new Date().toISOString().slice(0, 10);
const cabecalho = `RTS — aviso agregado do que e LIGADO ao seu binario
${"=".repeat(52)}

FICHEIRO GERADO. Nao edite a mao.
Regenerar: node scripts/notice/build_notice.mjs

O que isto e
------------
Um programa compilado com o \`rts\` liga estaticamente o runtime deste projeto,
e com ele ${alcancados.length} bibliotecas de terceiros. O codigo do proprio RTS e MIT e nao
lhe impoe nada. Estas nao sao nossas, e varias — BSD-2-Clause, BSD-3-Clause,
Apache-2.0 e as licencas Unicode — pedem que o aviso de copyright e o texto da
licenca sejam reproduzidos "na documentacao ou noutros materiais fornecidos com
a distribuicao".

Se distribui o binario que compilou, a distribuicao e sua. Entregue este
ficheiro com ele — ou o texto dele dentro de um "sobre"/"licencas" da sua
aplicacao — e a obrigacao fica cumprida. Ele vem ao lado do \`rts\` na release;
para o refazer a partir das fontes, o comando esta no topo deste ficheiro.

O que isto NAO e
----------------
Nao e o inventario deste repositorio. As dependencias que so o COMPILADOR usa
ficam de fora: correm na maquina de quem desenvolve e nao entram no binario de
ninguem. \`THIRD-PARTY-LICENSES.txt\` tem o workspace inteiro e
\`THIRD-PARTY-NOTICES.md\` tem o raciocinio, incluindo porque e que a divisao
entre as duas metades e a coisa que importa.

Tambem nao e aconselhamento juridico, e nao afirma que a sua utilizacao esta
conforme. Afirma uma coisa so: isto e o que esta la dentro, e estes sao os
textos que vieram com cada um.

Fecho a partir de \`${RAIZ}\`, dependencias normais, TODOS os alvos — porque quem
compila para outro sistema liga outra arvore, e um aviso que so valesse para um
deles falhava em silencio nos outros. E um superconjunto de proposito: cobrir a
mais atribui uma crate que talvez nao esteja no seu binario, cobrir a menos
deixa-o sem cumprir. So a segunda e uma falha.

${alcancados.length} pacotes, ${grupos.length} textos distintos. Gerado em ${hoje}.
${semTexto.length ? `\nSEM TEXTO LOCAL (${semTexto.length}), ditos em vez de omitidos — o campo SPDX do\nmanifesto vale, o texto e que nao estava na copia desta maquina:\n${semTexto.map((s) => `  - ${s}`).join("\n")}\n` : ""}

Os pacotes, e o que cada um declara
-----------------------------------
${inventario}
`;

const saida = `${cabecalho}\n${blocos.join("\n")}`;

// Sem `--check`, e a razao e a mesma que tira o ficheiro do repositorio: este
// aviso e uma funcao do `Cargo.lock`, portanto uma copia commitada so podia
// estar em dia ou mentir, e manter 838 kB em dia a cada bump de dependencia e
// custo sem leitor. Gerado onde e preciso — a release — nao ha nada que
// envelheca e nao ha check que valha a pena escrever.
writeFileSync(OUT, saida, "utf8");
console.log(`RUNTIME-NOTICE.txt: ${alcancados.length} pacotes, ${saida.length} bytes`);
if (semTexto.length) console.log(`  ${semTexto.length} sem texto local (listados no ficheiro)`);
