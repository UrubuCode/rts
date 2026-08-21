// A 4ª RÉGUA: o que é DESENHADO, contra o que o Chrome desenha.
//
//   node scripts/parity/regua_desenho.mjs
//   node scripts/parity/regua_desenho.mjs --base=out/paint-base.jsonl
//   node scripts/parity/regua_desenho.mjs --sabotagem=sem-setas    # a auto-conferência
//
// ## A pergunta que as outras três não fazem
//
// `compare.mjs`, `regressao.mjs` e `regua.mjs` comparam a CAIXA de cada
// elemento. Nenhuma vê o desenho, e há duas classes de defeito inteiras do
// outro lado dessa fronteira — as duas encontradas por uma PESSOA a olhar para
// a janela, não por medição:
//
//   1. conteúdo que o Chrome mostra e nós não desenhamos (as setas `↑` dos
//      retrolinks, geradas por `::before` com `counter()`), com as caixas todas
//      certas e o texto inexistente;
//   2. desenho a mais, no sítio errado (marcadores de lista a flutuar por cima
//      do conteúdo) — um marcador não é um elemento, é um item na lista de
//      desenho, e pode estar em qualquer sítio sem mover uma única caixa.
//
// Esta régua responde à primeira. A segunda é a `--orfaos`.
//
// ## A UNIDADE é a palavra, e a escolha não é cosmética
//
// O nosso `DisplayItem::Text` e o `InlineTextBox` do Chrome partem o texto em
// fragmentos em sítios DIFERENTES — são dois quebradores de linha distintos
// sobre dois medidores de fonte distintos. Comparar fragmentos daria ruído
// puro: o mesmo parágrafo, desenhado corretamente pelos dois lados, contaria
// como centenas de divergências. O multiconjunto de PALAVRAS é invariante à
// quebra de linha e continua a ver a palavra que falta.
//
// O `↑` é um token próprio (não-alfanumérico isolado), que é o que faz esta
// régua apanhar o defeito que a motivou.
//
// ## O LÍQUIDO CANCELA-SE — a regra do `parity-chrome.md` aplicada a palavras
//
// «Para qualquer soma com sinal, medir também a soma dos ABSOLUTOS.» Já custou
// caro duas vezes neste repositório: 115 linhas de erro real apresentaram-se
// como 3 porque `+56` e `−59` se cancelaram. Um multiconjunto de palavras faz
// exatamente o mesmo — texto a mais num sítio tapa texto a menos noutro — por
// isso o relatório dá SEMPRE `só-chrome` e `só-nós` em separado, e a soma dos
// absolutos ao lado do líquido.
//
// ## A AUTO-CONFERÊNCIA é injeção de falha, não re-derivação
//
// Uma conferência já passou aqui sobre uma sabotagem por ser TAUTOLÓGICA:
// re-derivava a resposta da mesma estrutura que auditava. Esta corrompe o
// NOSSO dump depois de carregado e exige que a divergência CRESÇA pelo menos o
// que foi injetado. A asserção é sobre um número derivado do lado do CHROME,
// que a sabotagem não toca. Se sabotar e o número não mexer, sai `exit 1`.

import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

const args = process.argv.slice(2);
const opt = (nome, omissao) => {
  const a = args.find((x) => x.startsWith(`--${nome}=`));
  return a ? a.slice(nome.length + 3) : omissao;
};
const tem = (nome) => args.includes(`--${nome}`);

const F_CHROME = resolve(opt("chrome", "scripts/parity/out/chrome-text.jsonl"));
const F_RTS = resolve(opt("rts", "scripts/parity/out/rts-paint.jsonl"));
const F_BASE = opt("base", null);
const SABOTAGEM = opt("sabotagem", null);
const TOP = Number(opt("top", 25));

/// Ler um JSONL e RECUSAR o que não tem rodapé.
///
/// É a primeira linha de código deste ficheiro e não a última, pela razão que o
/// `docs/ui/parity-chrome.md` regista: um dump A MEIO DE SER ESCRITO produz um
/// JSONL perfeitamente legível com metade do conteúdo, e lê-se exatamente como
/// "o motor não desenhou metade da página". Os dois casos têm o mesmo sintoma e
/// são conclusões opostas; o rodapé é a única coisa que os distingue.
function carregar(caminho, lado) {
  if (!existsSync(caminho)) {
    console.error(`ERRO: ${lado}: não existe ${caminho}`);
    process.exit(2);
  }
  const cru = readFileSync(caminho, "utf8").trim().split("\n");
  const linhas = [];
  let meta = null, fim = null, malformadas = 0;
  for (const l of cru) {
    let o;
    try { o = JSON.parse(l); } catch { malformadas++; continue; }
    if (o.__meta) meta = o;
    else if (o.__fim) fim = o;
    else if (o.__erro) { console.error(`ERRO: ${lado} reportou: ${o.__erro}`); process.exit(2); }
    else linhas.push(o);
  }
  if (!fim) {
    console.error(`ERRO: ${lado}: ${caminho} não tem linha __fim.`);
    console.error("  Ou a extração foi cortada a meio, ou ainda está a decorrer.");
    console.error("  Comparar assim reporta um corpus truncado como conteúdo em falta.");
    process.exit(2);
  }
  if (fim.emitidos !== undefined && fim.emitidos !== linhas.length + malformadas) {
    console.error(`ERRO: ${lado}: __fim diz ${fim.emitidos} emitidos, li ${linhas.length}` +
                  ` (+${malformadas} malformadas).`);
    process.exit(2);
  }
  return { meta, fim, linhas, malformadas };
}

/// O TEXTO de cada lado — e os MARCADORES DE LISTA ficam de fora dos dois.
///
/// A primeira versão incluía-os, e estava errada de uma forma que só um número
/// mostrou: os dois lados representam o mesmo marcador em MEIOS diferentes. Um
/// `disc` nosso é um `DisplayItem::SolidRect` — geometria, sem texto nenhum —
/// e o mesmo marcador chega do Chrome como `ListMarker` com o texto `"• "`.
/// Compará-los no fluxo de texto acusava 32 bullets e 459 pontos finais em
/// falta que ninguém perdeu: nós desenhamo-los, só que não como letras.
///
/// Então saem os dois, contados, e a pergunta dos marcadores é respondida na
/// sua própria secção — que é onde ela pertence, porque lá a unidade é o
/// marcador e não o caractere.
const EH_MARCADOR = /^[0-9a-z]+\.$/i;
const fragChrome = (l) => l.filter((o) => o.k === "text");
const fragRts = (l) => l.filter((o) => o.k === "text" && !EH_MARCADOR.test(String(o.t).trim()));

/// Palavras, normalizadas.
///
/// NFC porque os dois lados podem entregar a mesma letra acentuada em formas de
/// composição diferentes e uma diferença dessas não é um defeito de desenho.
/// O NBSP (` `) colapsa como espaço: é whitespace para quem lê, e tratá-lo
/// como parte da palavra faria `12 km` divergir de `12 km` sem nada estar mal.
/// Símbolos isolados (`↑`, `†`, `—`) ficam tokens próprios — é o que faz esta
/// régua ver o conteúdo gerado.
function palavras(fragmentos) {
  const saida = [];
  for (const f of fragmentos) {
    const t = String(f.t ?? "").normalize("NFC").replace(/[\s ​]+/g, " ").trim();
    if (!t) continue;
    for (const p of t.split(" ")) if (p) saida.push(p);
  }
  return saida;
}

const conta = (arr) => {
  const m = new Map();
  for (const p of arr) m.set(p, (m.get(p) ?? 0) + 1);
  return m;
};

/// Os CARACTERES visíveis, que é a métrica robusta à FRAGMENTAÇÃO.
///
/// Esta existe porque a de palavras acusou um defeito que não existe, e foi
/// apanhada na primeira corrida: 105 ocorrências de `"original"` do lado do
/// Chrome contra `"origina"` + `"l"` do nosso. Não partimos a palavra ao meio —
/// emitimos DOIS `DisplayItem::Text` adjacentes onde o Chrome emite um
/// `InlineTextBox` só, e desenhados lado a lado leem-se como a mesma palavra.
/// A tokenização por fragmento contava-os como duas palavras e como um erro.
///
/// Um caractere não se parte, então esta contagem é imune ao sítio onde cada
/// lado corta os seus fragmentos — e continua a ver o que a régua existe para
/// ver, porque um `↑` que não é desenhado é um caractere que falta.
///
/// As duas ficam, e nenhuma substitui a outra: esta diz QUANTO, a de palavras
/// diz ONDE. Uma divergência que apareça só na de palavras é fragmentação, não
/// conteúdo — e é assim que se lê o relatório.
function caracteres(fragmentos) {
  const saida = [];
  for (const f of fragmentos) {
    const t = String(f.t ?? "").normalize("NFC").replace(/[\s ​]+/g, "");
    for (const c of t) saida.push(c);
  }
  return saida;
}

/// A diferença de dois multiconjuntos, NOS DOIS SENTIDOS e nunca líquida.
function diferenca(a, b) {
  const so = new Map();
  for (const [p, n] of a) {
    const d = n - (b.get(p) ?? 0);
    if (d > 0) so.set(p, d);
  }
  return so;
}
const soma = (m) => [...m.values()].reduce((x, y) => x + y, 0);

// ---------------------------------------------------------------- carregar

const C = carregar(F_CHROME, "chrome");
const R = carregar(F_RTS, "rts");

let fr = fragRts(R.linhas);
const fc = fragChrome(C.linhas);

// ------------------------------------------------------------- SABOTAGEM
//
// Corrompe SÓ o nosso lado, depois de carregado. Cada modo simula um defeito
// real e conhecido, e a conferência lá em baixo exige que a régua o veja.
const SABOTAGENS = {
  // O defeito que motivou a régua: conteúdo gerado que não é desenhado.
  "sem-setas": (l) => l.filter((o) => !String(o.t).includes("↑")),
  // Perda difusa: 1% dos itens de texto desaparecem.
  "perde-1pc": (l) => l.filter((_, i) => i % 100 !== 0),
  // Uma palavra trocada por outra em cada item: apanha reescrita silenciosa.
  "troca": (l) => l.map((o) => ({ ...o, t: String(o.t).split(" ")[0] })),
  // NULA — não corrompe nada, e existe para provar que a conferência FALHA.
  //
  // As outras três provam que a régua vê um defeito injetado; nenhuma prova que
  // ela sabe dizer que NÃO viu. Uma conferência que só passa não é uma
  // conferência, e já passou aqui uma sobre uma sabotagem real por ser
  // tautológica. Esta pede a sabotagem e não mexe no dump: a divergência não
  // pode crescer, portanto `exit 1` é o resultado CORRETO e é o que se observa.
  // É o teste do próprio teste.
  "nula": (l) => l,
};
let sabotado = 0;
if (SABOTAGEM) {
  const f = SABOTAGENS[SABOTAGEM];
  if (!f) {
    console.error(`sabotagem desconhecida: ${SABOTAGEM}. Há: ${Object.keys(SABOTAGENS).join(", ")}`);
    process.exit(2);
  }
  const antes = fr.length;
  fr = f(fr);
  sabotado = antes - fr.length;
}

const pc = conta(palavras(fc));
const pr = conta(palavras(fr));
const soChrome = diferenca(pc, pr);
const soNos = diferenca(pr, pc);
const totalC = soma(pc), totalR = soma(pr);
const faltam = soma(soChrome), sobram = soma(soNos);

// ---------------------------------------------------------------- relatório

const pct = (a, b) => (b ? ((a / b) * 100).toFixed(1) : "0.0");
console.log("=".repeat(72));
console.log("RÉGUA DE DESENHO — o texto pintado, RTS x Chrome");
console.log("=".repeat(72));
console.log(`chrome: ${F_CHROME}`);
console.log(`rts:    ${F_RTS}`);
if (SABOTAGEM) console.log(`*** SABOTADO: ${SABOTAGEM} (${sabotado} fragmentos removidos) ***`);
console.log("");
console.log("ENTRADA (a régua não desconta nada em silêncio)");
console.log(`  chrome: ${fc.length} fragmentos pintados` +
            ` (${C.fim.fragmentos ?? "?"} texto + ${C.fim.marcadores ?? "?"} marcadores),` +
            ` ${C.fim.repetidosDescartados ?? 0} repetidos descartados na extração`);
console.log(`  rts:    ${fr.length} itens DisplayItem::Text` +
            ` de ${R.fim.itens} itens de pintura, ${R.fim.elementos} elementos` +
            ` (${R.fim.sem_caixa} sem caixa)`);
console.log(`  linhas malformadas: chrome ${C.malformadas}, rts ${R.malformadas}`);
console.log(`  EXCLUÍDOS do fluxo de texto (contados, comparados à parte):` +
            ` ${C.fim.marcadores ?? 0} marcadores do Chrome,` +
            ` ${R.linhas.filter((o) => o.k === "text" && EH_MARCADOR.test(String(o.t).trim())).length} nossos`);
console.log("");
console.log("PALAVRAS");
console.log(`  chrome pinta ${totalC}   |   nós pintamos ${totalR}` +
            `   |   líquido ${totalR - totalC >= 0 ? "+" : ""}${totalR - totalC}`);
console.log(`  SÓ-CHROME (não desenhamos): ${faltam}  (${pct(faltam, totalC)}% do que o Chrome pinta)`);
console.log(`  SÓ-NÓS    (a mais):         ${sobram}  (${pct(sobram, totalC)}%)`);
console.log(`  soma dos ABSOLUTOS: ${faltam + sobram}` +
            `   <- o líquido acima esconde ${faltam + sobram - Math.abs(totalR - totalC)} de cancelamento`);
console.log("");
const cc = conta(caracteres(fc));
const cr = conta(caracteres(fr));
const cFaltam = soma(diferenca(cc, cr)), cSobram = soma(diferenca(cr, cc));
const totalCC = soma(cc), totalCR = soma(cr);
console.log("CARACTERES (imune à fragmentação — ver o comentário em `caracteres`)");
console.log(`  chrome pinta ${totalCC}   |   nós pintamos ${totalCR}`);
console.log(`  SÓ-CHROME: ${cFaltam}  (${pct(cFaltam, totalCC)}%)   |   SÓ-NÓS: ${cSobram}  (${pct(cSobram, totalCC)}%)`);
console.log(`  soma dos ABSOLUTOS: ${cFaltam + cSobram}`);
const cDif = [...new Set([...cc.keys(), ...cr.keys()])]
  .map((c) => [c, (cc.get(c) ?? 0) - (cr.get(c) ?? 0)])
  .filter(([, d]) => d !== 0)
  .sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]));
console.log(`  os ${Math.min(TOP, cDif.length)} caracteres mais desequilibrados (+ = o Chrome pinta mais):`);
for (const [c, d] of cDif.slice(0, TOP)) {
  console.log(`    ${d > 0 ? "+" : ""}${String(d).padStart(6)}  ${JSON.stringify(c)}`);
}
console.log("");
const ordena = (m) => [...m.entries()].sort((a, b) => b[1] - a[1]).slice(0, TOP);
console.log(`TOP ${TOP} — palavras que o Chrome pinta e nós NÃO`);
for (const [p, n] of ordena(soChrome)) console.log(`  ${String(n).padStart(6)}x  ${JSON.stringify(p)}`);
console.log("");
console.log(`TOP ${TOP} — palavras que nós pintamos e o Chrome NÃO`);
for (const [p, n] of ordena(soNos)) console.log(`  ${String(n).padStart(6)}x  ${JSON.stringify(p)}`);

// --------------------------------------------------- marcadores de lista (B)
const marcC = C.fim.marcadores ?? 0;
// Do nosso lado um marcador não é um campo: a `DisplayList` não diz que um item
// é um bullet. Reconhece-se pela FORMA que `listitem.rs` lhe dá — um quadrado
// com raio igual a metade do lado — e os textuais pelo padrão `N.`.
const bullets = R.linhas.filter((o) => o.k === "rect" && o.w === o.h && o.r &&
                                       o.r.every((v) => Math.abs(v - o.w / 2) < 0.01));
const aneis = R.linhas.filter((o) => o.k === "border" && o.w === o.h &&
                                     Math.abs(o.r - o.w / 2) < 0.01);
const textuais = R.linhas.filter((o) => o.k === "text" && /^[0-9a-z]+\.$/i.test(String(o.t)));
const marcR = bullets.length + aneis.length + textuais.length;
console.log("");
console.log("MARCADORES DE LISTA");
console.log(`  chrome: ${marcC}   |   nós: ${marcR}` +
            ` (${bullets.length} disc, ${aneis.length} circle, ${textuais.length} textuais)`);
console.log(`  diferença: ${marcR - marcC}`);
console.log("  (o nosso lado é reconhecido pela FORMA — a DisplayList não marca um item");
console.log("   como bullet — logo este número tem a margem de erro dessa heurística.)");

// ------------------------------------------------------------------- BASE
//
// A população MUDA entre duas medições — é esse o objetivo do trabalho — por
// isso a comparação com uma base não é uma subtração de percentagens. Um
// −11,0% anunciado neste repositório era −7,9% real exatamente por isso.
if (F_BASE) {
  const B = carregar(resolve(F_BASE), "base");
  const pb = conta(palavras(fragRts(B.linhas)));
  const classe = (m) => {
    const c = new Map();
    for (const p of new Set([...pc.keys(), ...m.keys()])) {
      const a = pc.get(p) ?? 0, b = m.get(p) ?? 0;
      c.set(p, b === 0 ? "so-chrome" : a === 0 ? "so-nos" : "ambos");
    }
    return c;
  };
  const ca = classe(pb), cd = classe(pr);
  const trans = new Map();
  for (const p of new Set([...ca.keys(), ...cd.keys()])) {
    const k = `${ca.get(p) ?? "ausente"} -> ${cd.get(p) ?? "ausente"}`;
    trans.set(k, (trans.get(k) ?? 0) + 1);
  }
  console.log("");
  console.log(`BASE: ${F_BASE}`);
  console.log("  matriz de transição por palavra distinta (separa erro corrigido de população a mexer-se)");
  for (const [k, n] of [...trans.entries()].sort((a, b) => b[1] - a[1])) {
    const marca = k.startsWith("so-chrome ->") && k.endsWith("ambos") ? "  <- GANHO" :
                  k.startsWith("ambos ->") && k.endsWith("so-chrome") ? "  <- PERDA" : "";
    console.log(`    ${String(n).padStart(6)}  ${k}${marca}`);
  }
}

// ------------------------------------------------------- AUTO-CONFERÊNCIA
//
// Não re-deriva nada: compara o número desta corrida com o da mesma corrida sem
// sabotagem, e o alvo vem do lado do Chrome, que a sabotagem não toca.
let falhou = false;
const exige = (cond, msg) => {
  if (!cond) { console.log(`  ❌ ${msg}`); falhou = true; }
  else console.log(`  ✓ ${msg}`);
};
console.log("");
console.log("CONFERÊNCIA");
exige(fc.length > 0, `o lado do Chrome tem fragmentos (${fc.length})`);
exige(fr.length > 0, `o nosso lado tem itens de texto (${fr.length})`);
exige(C.malformadas === 0 && R.malformadas === 0, "nenhuma linha malformada");
exige(totalC > 1000, `o corpus do Chrome é plausível (${totalC} palavras, não um resto de extração)`);

if (SABOTAGEM) {
  // O limpo, recalculado do MESMO ficheiro, para o crescimento ser atribuível.
  const limpo = diferenca(conta(palavras(fragRts(R.linhas))), pc);
  const faltamLimpo = soma(diferenca(pc, conta(palavras(fragRts(R.linhas)))));
  const cresceu = faltam - faltamLimpo;
  console.log(`  sabotagem "${SABOTAGEM}": SÓ-CHROME passou de ${faltamLimpo} para ${faltam} (+${cresceu})`);
  exige(cresceu > 0,
    `a régua VÊ a sabotagem (se não crescesse, o instrumento não vê o que audita)`);
  if (SABOTAGEM === "sem-setas") {
    exige((soChrome.get("↑") ?? 0) > 0, "a seta ↑ aparece nomeada no SÓ-CHROME");
  }
  void limpo;
} else {
  console.log("  (corra com --sabotagem=sem-setas|perde-1pc|troca para provar que dispara)");
}

if (falhou) {
  console.log("");
  console.log("CONFERÊNCIA FALHOU — o número acima não vale.");
  process.exit(1);
}
