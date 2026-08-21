// A 4ª RÉGUA: o que é DESENHADO, contra o que o Chrome desenha.
//
//   node scripts/parity/regua_desenho.mjs
//   node scripts/parity/regua_desenho.mjs --base=out/paint-base.jsonl
//   node scripts/parity/regua_desenho.mjs --sabotagem=sem-setas    # a auto-conferência
//   node scripts/parity/regua_desenho.mjs --sabotagem=chrome-menos-marcadores
//   node scripts/parity/regua_desenho.mjs --sabotagem=chrome-menos-marcadores-137
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
// re-derivava a resposta da mesma estrutura que auditava. Esta corrompe um dos
// dumps depois de carregado e exige que a divergência se mova pelo menos o que
// foi injetado. Se sabotar e o número não mexer, sai `exit 1`.
//
// ## E audita OS DOIS LADOS, porque auditar só um deixou passar o defeito real
//
// Durante muito tempo as quatro sabotagens corrompiam só o NOSSO dump. Isso
// deixou passar um defeito que estava do outro lado e que custou uma correção
// errada quase escrita: a contagem de marcadores do Chrome vinha da árvore de
// acessibilidade, que reporta 493 numa página que pinta 795. O denominador
// estava 302 abaixo, e a régua apresentou a falta do INSTRUMENTO como desenho a
// mais do nosso lado — o líquido de -167 era -469 e +302 a cancelarem-se.
//
// Duas coisas responderam a isso, e a segunda é a que fica:
//   1. `--sabotagem=chrome-*` corrompe o lado do CHROME. O sentido faz parte da
//      asserção — tirar ao Chrome tem de fazer o que nos falta ENCOLHER, e uma
//      conferência que só exigisse "mexeu" passaria com o sinal trocado.
//   2. Um INVARIANTE permanente, que não precisa de sabotagem nenhuma: as duas
//      fontes do lado do Chrome (AX e estilo computado) respondem à mesma
//      pergunta e têm de concordar. Duas fontes para uma pergunta só são
//      seguras enquanto alguém verifica que concordam — e ninguém verificava.

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

/// COMO se reconhece um marcador NOSSO: pela forma, confirmada pelo DONO.
///
/// A versão anterior perguntava pela forma — um rect pequeno e redondo, ou um
/// texto a casar `^[0-9a-z]+\.$` — e media-se: dos 433 que contava, **107 eram
/// texto corrido da página** ("1.", "a." dentro de frases). 25% de falsos
/// positivos, e não só na secção dos marcadores: o `fragRts` usava a mesma
/// expressão para EXCLUIR marcadores do fluxo de texto, portanto apagava 107
/// palavras legítimas do nosso lado e inflacionava o "só-chrome" na mesma conta.
///
/// O dono resolve as duas de uma vez, e não é heurística de aparência: um
/// marcador é desenhado imediatamente à esquerda do content-box do seu `<li>`,
/// na mesma linha (é o que `listitem.rs` faz, e o `outside` é o único caso que
/// desenhamos). Um "1." no meio de um parágrafo não tem `<li>` nenhum ali à
/// direita — foi assim que os 107 se separaram dos 326 verdadeiros, sem um
/// único caso ambíguo.
///
/// A alternativa era um campo no `DisplayItem` a dizer "isto é um marcador".
/// É a resposta certa e continua por fazer: obriga a tocar em 89 sítios de
/// construção e no `layout.rs`. Enquanto não existir, a correlação responde à
/// mesma pergunta com os dados que o dump já tem.
function indiceDeLi(linhas) {
  const porFaixa = new Map(); // y arredondado a 8px -> [caixas de <li>]
  for (const o of linhas) {
    if (o.k !== "el" || o.tag !== "li") continue;
    for (const f of [Math.floor(o.y / 8) - 1, Math.floor(o.y / 8), Math.floor(o.y / 8) + 1]) {
      if (!porFaixa.has(f)) porFaixa.set(f, []);
      porFaixa.get(f).push(o);
    }
  }
  return (m) => {
    const cand = porFaixa.get(Math.floor(m.y / 8)) ?? [];
    for (const b of cand) {
      // Mesma linha, e a caixa do item começa logo à direita do marcador.
      if (Math.abs(b.y - m.y) > 20) continue;
      const dx = b.x - m.x;
      if (dx >= -2 && dx < 40) return b;
    }
    return null;
  };
}

/// A FORMA continua a ser o pré-filtro, e o dono é que decide.
///
/// A correlação sozinha não serve e vale a pena dizer porquê, porque é o erro
/// que esta linha quase teve: o texto do PRÓPRIO item também começa junto à
/// caixa do `<li>` (com `padding-left` o marcador cai à direita de `b.x`, sem
/// ele cai à esquerda), portanto "tem um `<li>` ali" apanharia a primeira
/// palavra de cada item de lista da página. A forma sozinha tem 107 falsos
/// positivos; as duas juntas não têm nenhum — foi o que a separação mediu.
const EH_MARCADOR = /^[0-9a-z]+\.$/i;
const fragChrome = (l) => l.filter((o) => o.k === "text");
const fragRts = (l) => {
  const dono = indiceDeLi(l);
  return l.filter(
    (o) => o.k === "text" && !(EH_MARCADOR.test(String(o.t).trim()) && dono(o)),
  );
};

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
let fc = fragChrome(C.linhas);

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

/// SABOTAGEM DO LADO DO CHROME — as quatro de cima auditam metade.
///
/// **Isto existe porque a régua já mentiu por aqui e nenhuma sabotagem a
/// apanhou.** A contagem de marcadores do Chrome vinha da árvore de
/// acessibilidade, que reporta um `ListMarker` para 32 dos 334 bullets que a
/// página desenha: o DENOMINADOR estava 302 abaixo do real, e a régua
/// apresentou essa falta do instrumento como desenho a mais do nosso lado. Os
/// quatro modos acima corrompem só o NOSSO dump, portanto nenhum deles podia
/// ver um defeito que estava do outro lado — uma conferência que só audita
/// metade não audita.
///
/// Cada modo devolve o par (dados do Chrome, quanto foi injetado), e a
/// conferência exige que a divergência ENCOLHA pelo menos o injetado — o
/// sentido oposto ao das sabotagens do nosso lado, porque tirar ao Chrome tira
/// ao que nos falta. O sentido faz parte da asserção: uma conferência que só
/// exigisse "mexeu" passaria com o sinal trocado.
const SABOTAGENS_CHROME = {
  // O defeito REAL, reproduzido: o lado do Chrome conta menos marcadores do que
  // a página pinta — exatamente os 302 que a AX não reportava. Se a régua não
  // vir isto, não vê o que já lhe aconteceu.
  "chrome-menos-marcadores": ({ fc, fim }) => ({
    fc,
    fim: { ...fim, marcadoresPintados: Math.max(0, (fim.marcadoresPintados ?? 0) - 302) },
    injetado: Math.min(302, fim.marcadoresPintados ?? 0),
    alvo: "marcadores",
  }),
  // A MESMA sabotagem com um número que NÃO é o da fenda da AX, e existe por
  // uma razão que só apareceu ao conferir a conferência: injetar exatamente 302
  // leva o valor sabotado a coincidir com o que a AX reporta (493), portanto
  // "a régua usou o valor sabotado" e "a régua leu a AX por engano" produzem o
  // MESMO número e nenhuma asserção os distingue. 137 não coincide com nada, e
  // é este o modo que prova que a asserção apanha uma régua cega.
  "chrome-menos-marcadores-137": ({ fc, fim }) => ({
    fc,
    fim: { ...fim, marcadoresPintados: Math.max(0, (fim.marcadoresPintados ?? 0) - 137) },
    injetado: Math.min(137, fim.marcadoresPintados ?? 0),
    alvo: "marcadores",
  }),
  // Perda difusa no corpus do Chrome — "a extração trouxe menos página do que a
  // página tem", que é a mesma classe de defeito noutra métrica.
  "chrome-perde-1pc": ({ fc, fim }) => ({
    fc: fc.filter((_, i) => i % 100 !== 0),
    fim,
    injetado: Math.floor(fc.length / 100),
    alvo: "palavras",
  }),
  // A NULA do lado do Chrome, pela mesma razão que a outra existe: provar que
  // esta conferência sabe FALHAR. Não corrompe nada, logo nada pode encolher.
  "chrome-nula": ({ fc, fim }) => ({ fc, fim, injetado: 0, alvo: "marcadores" }),
};
let sabotado = 0;
// O LADO sabotado decide o SENTIDO que a conferência exige. Tirar ao nosso dump
// faz a divergência crescer; tirar ao do Chrome fá-la encolher. Uma conferência
// que só exigisse "mexeu" passaria com o sinal trocado, que é meia asserção.
let ladoSabotado = null, injetadoChrome = 0, alvoChrome = null;
if (SABOTAGEM) {
  const f = SABOTAGENS[SABOTAGEM], g = SABOTAGENS_CHROME[SABOTAGEM];
  if (!f && !g) {
    console.error(`sabotagem desconhecida: ${SABOTAGEM}. Há: ` +
                  `${[...Object.keys(SABOTAGENS), ...Object.keys(SABOTAGENS_CHROME)].join(", ")}`);
    process.exit(2);
  }
  if (f) {
    ladoSabotado = "nosso";
    const antes = fr.length;
    fr = f(fr);
    sabotado = antes - fr.length;
  } else {
    ladoSabotado = "chrome";
    const antes = fc.length;
    const r = g({ fc, fim: C.fim });
    fc = r.fc;
    C.fim = r.fim;
    injetadoChrome = r.injetado;
    alvoChrome = r.alvo;
    sabotado = antes - fc.length;
  }
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
// O índice do dono é construído UMA vez: as duas secções que perguntam "isto é
// um marcador?" partilham-no, e reconstruí-lo por item era O(n²) sobre 13 000
// itens de pintura.
const dono = indiceDeLi(R.linhas);
// A MESMA definição que a secção dos marcadores usa (forma + dono). Esta linha
// dizia 110 enquanto a outra dizia 0 — duas contagens da mesma coisa no mesmo
// relatório, e a diferença entre elas eram exatamente os falsos positivos.
const excluidosNossos = R.linhas.filter(
  (o) => o.k === "text" && EH_MARCADOR.test(String(o.t).trim()) && dono(o),
).length;
console.log(`  EXCLUÍDOS do fluxo de texto (contados, comparados à parte):` +
            ` ${C.fim.marcadoresPintados ?? C.fim.marcadores ?? 0} marcadores do Chrome,` +
            ` ${excluidosNossos} nossos`);
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
// O corpus de texto do lado do Chrome tem DUAS leituras, e ficam lado a lado
// pela mesma razão que as dos marcadores: a AX descreve a árvore, o DOM descreve
// o que sobrevive ao desenho, e já se mediu que discordam.
//
// Nos MARCADORES discordam muito (294 num total de 787) e a AX é a errada. No
// TEXTO discordam pouco — ~1 100 caracteres em 152 000, com a AX ligeiramente
// ACIMA, o que é esperado: um `::before` tem `InlineTextBox` na AX e não tem nó
// de texto no DOM, portanto o conteúdo gerado só existe de um dos lados.
//
// Isto está escrito porque uma versão anterior desta secção afirmou uma fenda de
// 16 440 caracteres que NÃO EXISTE. O censo do DOM viajava num template literal,
// onde `\s` não é a classe de espaço mas a letra "s": a página recebia `/s/` e
// contava tudo o que não fosse um "s". O número saiu 11% alto e a conclusão que
// se tirou dele — que as 712 palavras a mais eram do instrumento — era falsa.
const txtDom = C.fim.textoCaracteres ?? null;
if (txtDom !== null) {
  const fendaT = txtDom - totalCC;
  console.log(`  o DOM do Chrome desenha ${txtDom} caracteres; a AX entregou ${totalCC}` +
              ` (fenda ${fendaT})`);
  if (Math.abs(fendaT) > totalCC * 0.02) {
    console.log(`  ⚠ as duas leituras do lado do Chrome discordam em ${Math.abs(fendaT)}` +
                ` (${pct(Math.abs(fendaT), totalCC)}%) — acima do que o conteúdo gerado explica.`);
    console.log("    Enquanto isso durar, nada nesta secção é atribuível ao motor.");
  }
}
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
// O NÚMERO DO CHROME é o que ele PINTA, e não o que a AX reporta.
//
// A AX reportava 493 numa página que desenha 795 — faltavam-lhe 302 dos 334
// bullets. A régua apresentava essa falta do INSTRUMENTO como marcadores a mais
// do nosso lado, e o líquido (-167) escondia -457 num sentido e +294 no outro.
// Quase se suprimiram 294 bullets corretos para acertar num número errado.
// `chrome_text.mjs` passou a censar por estilo computado; o valor da AX fica ao
// lado, porque a diferença entre os dois é uma propriedade do instrumento.
const marcAX = C.fim.marcadores ?? 0;
const marcC = C.fim.marcadoresPintados ?? null;
// Do nosso lado um marcador não é um campo — a `DisplayList` não diz que um
// item é um bullet — por isso é FORMA (o que `listitem.rs` desenha) confirmada
// pelo DONO (o `<li>` que lhe fica ao lado). Ver `indiceDeLi`.
const bullets = R.linhas.filter((o) => o.k === "rect" && o.w === o.h && o.r &&
                                       o.r.every((v) => Math.abs(v - o.w / 2) < 0.01) && dono(o));
const aneis = R.linhas.filter((o) => o.k === "border" && o.w === o.h &&
                                     Math.abs(o.r - o.w / 2) < 0.01 && dono(o));
const textuais = R.linhas.filter((o) => o.k === "text" &&
                                        EH_MARCADOR.test(String(o.t).trim()) && dono(o));
const marcR = bullets.length + aneis.length + textuais.length;
console.log("");
console.log("MARCADORES DE LISTA");
if (marcC === null) {
  console.log(`  chrome: ${marcAX} (SÓ a árvore de acessibilidade — re-extraia com o`);
  console.log("          chrome_text.mjs novo para ter o número que a página PINTA)");
} else {
  const tipos = Object.entries(C.fim.marcadoresPorTipo ?? {})
    .map(([k, v]) => `${v} ${k}`).join(" + ");
  console.log(`  chrome PINTA: ${marcC}${tipos ? ` (${tipos})` : ""}`);
  console.log(`  chrome pela AX: ${marcAX}` +
              `  <- ${marcC - marcAX} que a AX não reporta; NÃO é diferença de motor`);
}
console.log(`  nós: ${marcR}` +
            ` (${bullets.length} disc, ${aneis.length} circle, ${textuais.length} textuais)`);
// Os dois números REPORTADOS, ao alcance da conferência.
//
// Ela compara o que esta secção IMPRIME, e não uma recontagem própria — se
// alguém trocar aqui `marcC` por `marcAX`, a conferência tem de o ver. Uma
// asserção que re-derivasse os números da mesma fonte que audita seria
// tautológica, que é a crítica que já matou uma sabotagem neste ficheiro.
let faltamM = 0, sobramM = 0;
// A REGRA DOS ABSOLUTOS, que esta secção era a única a não aplicar. O líquido
// de -167 era -457 e +294 a cancelarem-se, e as duas metades são trabalhos
// diferentes: uma é desenho em falta, a outra seria desenho a apagar.
if (marcC !== null) {
  faltamM = Math.max(0, marcC - marcR);
  sobramM = Math.max(0, marcR - marcC);
  console.log(`  líquido: ${marcR - marcC >= 0 ? "+" : ""}${marcR - marcC}` +
              `   |   em falta: ${faltamM}   |   a mais: ${sobramM}`);
}
console.log("  (o nosso lado é FORMA confirmada pelo DONO — a DisplayList não marca um");
console.log("   item como bullet. Um campo no DisplayItem tirava a heurística toda.)");

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

// O INVARIANTE PERMANENTE sobre o lado do Chrome, que não precisa de sabotagem
// nenhuma para disparar.
//
// É este que teria apanhado o defeito real: as duas fontes do lado do Chrome —
// a árvore de acessibilidade e o estilo computado — respondem à MESMA pergunta
// e discordavam em 302. Enquanto ninguém as comparou, a régua usou a mais baixa
// e apresentou a falta como desenho a mais do nosso lado. Duas fontes para uma
// pergunta só são seguras enquanto alguém verifica que concordam.
if (marcC !== null) {
  const fenda = marcC - marcAX;
  console.log(`  fontes do Chrome para os marcadores: AX ${marcAX}, computado ${marcC}` +
              ` (fenda ${fenda})`);
  // O invariante audita o dump REAL. Com o lado do Chrome deliberadamente
  // corrompido ele tem de partir — é a sabotagem a funcionar — e exigi-lo aqui
  // confundiria "a régua viu a sabotagem" com "o instrumento está partido" no
  // mesmo código de saída.
  if (ladoSabotado !== "chrome") {
    exige(fenda >= 0,
      "a AX não reporta MAIS marcadores do que a página pinta (se reportasse, uma das duas leituras está errada)");
  } else {
    console.log("  (invariante AX-vs-computado suspenso: o lado do Chrome está sabotado de propósito)");
  }
  if (fenda > marcC * 0.05) {
    console.log(`  ⚠ a AX subconta ${fenda} marcadores (${pct(fenda, marcC)}%).` +
                " NÃO a use como denominador — é o defeito que esta secção já teve.");
  }
} else {
  exige(false,
    "o dump do Chrome traz `marcadoresPintados` (sem ele a contagem de marcadores vem da AX, que subconta)");
}

if (SABOTAGEM && ladoSabotado === "nosso") {
  // O limpo, recalculado do MESMO ficheiro, para o crescimento ser atribuível.
  const faltamLimpo = soma(diferenca(pc, conta(palavras(fragRts(R.linhas)))));
  const cresceu = faltam - faltamLimpo;
  console.log(`  sabotagem "${SABOTAGEM}" (nosso lado): SÓ-CHROME passou de ${faltamLimpo} para ${faltam} (+${cresceu})`);
  exige(cresceu > 0,
    `a régua VÊ a sabotagem (se não crescesse, o instrumento não vê o que audita)`);
  if (SABOTAGEM === "sem-setas") {
    exige((soChrome.get("↑") ?? 0) > 0, "a seta ↑ aparece nomeada no SÓ-CHROME");
  }
} else if (SABOTAGEM && ladoSabotado === "chrome") {
  // O SENTIDO INVERSO, e é por isso que esta asserção é separada e não um
  // `!==`: tirar ao Chrome tem de fazer o que nos falta ENCOLHER, e pelo menos
  // o que foi injetado. Um instrumento que reagisse na mesma direção nos dois
  // casos estaria a medir outra coisa.
  if (alvoChrome === "marcadores") {
    // QUANTO a divergência TEM de se mover — derivado, não escolhido.
    //
    // A primeira versão exigia que ela encolhesse pelo menos o injetado, e isso
    // é insatisfazível quando o défice real é MENOR que a injeção: com 8 em
    // falta, tirar 302 ao Chrome só pode fechar 8. Deu alarme falso contra o
    // dump já corrigido — a régua via a sabotagem, cega estava a asserção.
    //
    // `min(injetado, divergência)` foi a correção proposta e NÃO serve, pela
    // razão oposta: com os dois lados a bater (divergência 0) o mínimo é 0, a
    // asserção vira `movimento >= 0` e uma régua COMPLETAMENTE CEGA passa. Ou
    // seja, desligava-se exatamente no regime em que hoje vivemos.
    //
    // Exigir um MOVIMENTO MÍNIMO também não serve, e isto custou uma conferência
    // da conferência a descobrir: uma régua cega (a ler a AX em vez do valor
    // sabotado) reporta uma divergência CONSTANTE de 294, que por ser grande
    // satisfaz qualquer limiar de movimento. Passava com voo cego.
    //
    // O que se exige é a IGUALDADE com o valor que uma régua correta reportaria,
    // derivado do défice limpo (relido do ficheiro, que a sabotagem não toca) e
    // da injeção. Não é re-derivar pelo caminho auditado: é comparar o que a
    // secção IMPRIMIU com o que a entrada por corromper implica.
    const limpoC = carregar(F_CHROME, "chrome").fim.marcadoresPintados ?? 0;
    const dLimpo = limpoC - marcR;
    const divLimpo = Math.abs(dLimpo);
    const divAgora = faltamM + sobramM; // o que a secção IMPRIMIU
    const esperado = Math.abs(dLimpo - injetadoChrome);
    console.log(`  sabotagem "${SABOTAGEM}" (lado do Chrome): divergência ABSOLUTA de` +
                ` marcadores passou de ${divLimpo} para ${divAgora}` +
                ` (injetado ${injetadoChrome}, esperado ${esperado})`);
    exige(injetadoChrome > 0 && divAgora === esperado,
      "a régua reporta EXATAMENTE a divergência que o denominador sabotado implica");
  } else {
    const pcLimpo = conta(palavras(fragChrome(carregar(F_CHROME, "chrome").linhas)));
    const faltamLimpo = soma(diferenca(pcLimpo, pr));
    const encolheu = faltamLimpo - faltam;
    console.log(`  sabotagem "${SABOTAGEM}" (lado do Chrome): SÓ-CHROME passou de` +
                ` ${faltamLimpo} para ${faltam} (-${encolheu})`);
    exige(encolheu > 0,
      "a régua VÊ um corpus do Chrome menor do que a página tem");
  }
} else {
  console.log("  (corra com --sabotagem=sem-setas|perde-1pc|troca|nula para provar que dispara");
  console.log("   do NOSSO lado, e --sabotagem=chrome-menos-marcadores[-137]|chrome-perde-1pc|chrome-nula");
  console.log("   para o lado do Chrome — uma conferência que só audita metade não audita)");
}

if (falhou) {
  console.log("");
  console.log("CONFERÊNCIA FALHOU — o número acima não vale.");
  process.exit(1);
}
