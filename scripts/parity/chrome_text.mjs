// O TEXTO QUE O CHROME PINTA — a metade Chrome da 4ª régua.
//
//   node scripts/parity/chrome_text.mjs [pagina.html] [out/chrome-text.jsonl]
//
// ## Porque não `innerText`, que é o que o `chrome_extract.mjs` já usa
//
// Porque o `innerText` NÃO inclui conteúdo gerado por `::before`/`::after`, e
// esse é precisamente o defeito que motivou esta régua: as setas `↑` e os
// números dos retrolinks da Wikipédia são `content: counter(...)`. Medido numa
// sonda com `content: "\2191 " counter(r) ". "`:
//
//     innerText:  "Primeira referencia\nSegunda referencia\n\nTexto normal"
//
// As setas e os números não estão lá. Uma régua construída sobre o `innerText`
// responderia "o Chrome também não mostra as setas" e passaria limpa sobre o
// defeito que existe para apanhar. Vale a pena ter isto escrito: era a fonte
// óbvia e é a fonte errada.
//
// Duas outras fontes eliminadas com número, para não voltarem a ser propostas:
//
//   - CSSOM — `getComputedStyle(el,"::before").content` devolve
//     `"↑" counter(r) ". "`, com o `counter()` POR RESOLVER. Diz que há
//     conteúdo gerado, nunca diz qual.
//   - Árvore DOM do CDP (`DOM.getDocument{pierce:true}`) — os nós `::before`,
//     `::after` e `::marker` APARECEM, e vêm todos com `children: []`.
//
// ## A fonte que funciona: a árvore de acessibilidade
//
// `Accessibility.getFullAXTree`. O papel `InlineTextBox` é a caixa de linha que
// o Chrome efetivamente pintou — o equivalente do nosso `DisplayItem::Text`,
// fragmento a fragmento — e inclui pseudo-elementos com os contadores já
// resolvidos. Na mesma sonda:
//
//     ListMarker: "1. "        <- o marcador de lista, com o texto
//     InlineTextBox: "↑"       <- o counter RESOLVIDO
//     InlineTextBox: "1"
//     InlineTextBox: ". "
//     InlineTextBox: " [gerado]"
//
// O `ListMarker` sai por isso num campo próprio: é a metade (B) da régua — os
// marcadores que o Chrome desenha, contra os que nós desenhamos.
//
// ## A DEDUPLICAÇÃO não é higiene, é o denominador
//
// `getFullAXTree` devolveu 17 entradas para 10 nós na sonda — o mesmo `nodeId`
// repetido. Sem deduplicar, o lado do Chrome teria ~70% mais texto do que a
// página tem e a régua acusaria conteúdo em falta que ninguém perdeu. A
// deduplicação é por `nodeId` e o número de repetidas vai ao `__fim`, contado,
// porque uma exclusão silenciosa é a falha mais cara que uma régua tem.
//
// As escolhas de página (JS desligado, viewport 1280x800, fontes remotas
// desligadas) são as mesmas do `chrome_extract.mjs` e pelas mesmas razões — se
// divergissem, os dois lados mediriam páginas diferentes.

import { spawn } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const CHROME = [
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
  process.env.CHROME_BIN,
].find((p) => p && existsSync(p));

if (!CHROME) {
  console.error("chrome não encontrado — defina CHROME_BIN");
  process.exit(2);
}

const alvo = resolve(process.argv[2] ?? "scripts/parity/pagina.combinada.html");
const saida = resolve(process.argv[3] ?? "scripts/parity/out/chrome-text.jsonl");
const PORTA = Number(process.env.CDP_PORT ?? 9334);

const perfil = resolve("scripts/parity/out/.chrome-profile-text");
const chrome = spawn(CHROME, [
  "--headless=new",
  `--remote-debugging-port=${PORTA}`,
  `--user-data-dir=${perfil}`,
  "--no-first-run", "--no-default-browser-check",
  "--disable-extensions", "--disable-background-networking",
  "--disable-remote-fonts",
  "--hide-scrollbars",
  "--force-device-scale-factor=1",
  "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });
chrome.stderr.on("data", () => {});

async function esperarCdp() {
  const t0 = Date.now();
  for (;;) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORTA}/json/version`);
      return (await r.json()).webSocketDebuggerUrl;
    } catch {
      if (Date.now() - t0 > 30000) throw new Error("CDP não subiu em 30s");
      await new Promise((r) => setTimeout(r, 150));
    }
  }
}

function cliente(ws) {
  let proximo = 0;
  const pendentes = new Map();
  const eventos = new Map();
  ws.addEventListener("message", (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id !== undefined) {
      const p = pendentes.get(m.id);
      pendentes.delete(m.id);
      if (m.error) p.rej(new Error(`${m.method ?? ""} ${JSON.stringify(m.error)}`));
      else p.res(m.result);
    } else {
      (eventos.get(m.method) ?? []).forEach((f) => f(m.params, m.sessionId));
    }
  });
  return {
    envia(method, params = {}, sessionId) {
      const id = ++proximo;
      return new Promise((res, rej) => {
        pendentes.set(id, { res, rej });
        ws.send(JSON.stringify({ id, method, params, sessionId }));
      });
    },
    quando(method, f) {
      if (!eventos.has(method)) eventos.set(method, []);
      eventos.get(method).push(f);
    },
  };
}

const ws = new WebSocket(await esperarCdp());
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
const c = cliente(ws);

const { targetId } = await c.envia("Target.createTarget", { url: "about:blank" });
const { sessionId } = await c.envia("Target.attachToTarget", { targetId, flatten: true });

await c.envia("Emulation.setScriptExecutionDisabled", { value: true }, sessionId);
await c.envia("Emulation.setDeviceMetricsOverride",
  { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
await c.envia("Page.enable", {}, sessionId);

const carregou = new Promise((r) => c.quando("Page.loadEventFired", r));
await c.envia("Page.navigate", { url: "file:///" + alvo.split("\\").join("/") }, sessionId);
await carregou;
await new Promise((r) => setTimeout(r, 300));
// Força um layout síncrono: o `load` diz que os recursos chegaram, não que a
// árvore assentou, e a AX tree é construída a partir da árvore de layout.
await c.envia("Runtime.evaluate",
  { expression: "document.documentElement.offsetHeight" }, sessionId);

// O CENSO DOS MARCADORES, pelo estilo computado e NÃO pela AX.
//
// A AX é a fonte do resto deste ficheiro e é a errada para esta pergunta:
// mediu-se, e ela reporta um `ListMarker` para 32 dos 334 bullets que a página
// da Wikipédia desenha. O denominador do Chrome saía **302 abaixo** do que o
// Chrome pinta, e a régua apresentava essa falta do instrumento como marcadores
// a mais do nosso lado — 294 bullets corretos que quase foram suprimidos para
// acertar num número que estava errado.
//
// O estilo computado responde exatamente: um `display:list-item` desenha
// marcador quando o `list-style-type` não é `none` e não há imagem a
// substituí-lo. É a mesma regra que o `listitem.rs` aplica, o que torna os dois
// lados comparáveis por construção em vez de por coincidência.
const censo = await c.envia("Runtime.evaluate", {
  returnByValue: true,
  expression: `(() => {
    let pinta = 0, ocultos = 0; const porTipo = {};
    for (const li of document.querySelectorAll('*')) {
      const cs = getComputedStyle(li);
      if (cs.display !== 'list-item') continue;
      if (cs.listStyleType === 'none') continue;
      if (cs.listStyleImage !== 'none') continue;
      // RENDERIZA de facto. Um antepassado \`display:none\` deixa o
      // \`getComputedStyle\` a responder \`list-item\` para um elemento que não
      // gera caixa nenhuma — e um marcador que ninguém desenha não é um
      // marcador. Sem esta linha o censo contava 302 a mais e o erro era o
      // simétrico do que veio corrigir.
      if (li.getClientRects().length === 0) { ocultos++; continue; }
      pinta++; porTipo[cs.listStyleType] = (porTipo[cs.listStyleType] || 0) + 1;
    }
    return { pinta, ocultos, porTipo };
  })()`,
}, sessionId);
const marcComputados = censo?.result?.value ?? null;

// O CENSO DO TEXTO, pela mesma razão que o dos marcadores existe.
//
// O corpo deste ficheiro lê o texto da árvore de acessibilidade, e a AX já foi
// apanhada a subcontar 294 marcadores. Nada garante que não subconte texto
// também — e a régua atribui-nos 712 palavras "a mais" que ninguém explicou.
// Este censo conta os caracteres que o DOM REALMENTE desenha (um `Range` por nó
// de texto, com caixa), independente da AX. Se os dois discordarem muito, a
// diferença é do instrumento e não do motor, e quem lê o relatório tem de o ver
// ANTES de corrigir seja o que for.
const censoTexto = await c.envia("Runtime.evaluate", {
  returnByValue: true,
  expression: `(() => {
    const it = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    let chars = 0, nos = 0, ocultos = 0; const textos = [];
    const r = document.createRange();
    for (let n = it.nextNode(); n; n = it.nextNode()) {
      const t = n.nodeValue;
      if (!t || !t.trim()) continue;
      r.selectNodeContents(n);
      if (r.getClientRects().length === 0) { ocultos++; continue; }
      // Ter caixa NÃO é o mesmo que pintar: um \`visibility:hidden\` mantém a
      // caixa e não desenha glifo nenhum, e o mesmo vale para \`opacity:0\` e
      // para o conteúdo saltado por \`content-visibility\`. Contá-los era o
      // sobre-conto deste censo — a versão simétrica do erro que ele veio medir.
      const pai = n.parentElement;
      if (pai && pai.checkVisibility && !pai.checkVisibility({
        visibilityProperty: true, opacityProperty: true, contentVisibilityAuto: true,
      })) { ocultos++; continue; }
      nos++;
      // O TEXTO, e não só a contagem: sem ele a fenda entre a AX e o DOM é um
      // número sem sítio, e localizá-la é o trabalho seguinte.
      textos.push(t);
      for (const ch of t) if (!/\\s/.test(ch)) chars++;
    }
    // O array FICA NA PAGINA e e puxado por paginas (ver puxarTextos).
    // Devolve-lo inteiro aqui perdia 17 536 caracteres em silencio.
    window.__rtsTextos = textos;
    // O AUTO-CONTROLO dentro da pagina: os mesmos caracteres, recontados a
    // partir do array que vai ser puxado. Se este bater com o que se reconta
    // do lado de ca, o transporte esta limpo; se nao bater com o contador, e o
    // contador que esta errado. Sem os dois nao se sabe qual dos dois culpar.
    let charsDoArray = 0;
    for (const t of textos) for (const ch of t) if (!/\\s/.test(ch)) charsDoArray++;
    return { chars, charsDoArray, nos, ocultos };
  })()`,
}, sessionId);
const txtComputado = censoTexto?.result?.value ?? null;

/// Puxa o corpus do DOM POR PAGINAS, e reconta o que chegou.
///
/// Um array de 10 000 strings devolvido de uma vez por `returnByValue` chega
/// CORTADO: mediu-se 169 564 caracteres contados dentro da pagina contra
/// 152 028 recontaveis do que chegou, sobre o mesmo numero de nos — e o
/// contador, sendo um numero, chega sempre inteiro, portanto a perda e
/// invisivel a quem so olhe para ele.
///
/// Isso importa mais do que parece: um corpus truncado le-se exatamente como
/// "o Chrome nao desenha este texto", que e a mesma leitura que um motor a
/// falhar produz. E a razao pela qual o ficheiro principal tem rodape.
///
/// Fatias pequenas atravessam inteiras. Quem chama CONFERE o total contra o
/// contador da pagina em vez de assumir que atravessaram.
async function puxarTextos(total, tamanho = 400) {
  const out = [];
  for (let i = 0; i < total; i += tamanho) {
    const r = await c.envia("Runtime.evaluate", {
      returnByValue: true,
      expression: `window.__rtsTextos.slice(${i}, ${i + tamanho})`,
    }, sessionId);
    const fatia = r?.result?.value;
    if (!Array.isArray(fatia)) {
      console.error(`ERRO: a fatia [${i}, ${i + tamanho}) do corpus do DOM nao voltou como array.`);
      process.exit(2);
    }
    // CADA PAGINA e conferida, e nao so o total no fim.
    //
    // Um total curto diz que se perdeu alguma coisa; nao diz ONDE, e uma pagina
    // que volte curta no meio de vinte e indistinguivel de vinte que voltem
    // completas menos uma. Aqui a pagina que faltar grita com o seu indice.
    const esperado = Math.min(tamanho, total - i);
    if (fatia.length !== esperado) {
      console.error(`ERRO: a fatia [${i}, ${i + tamanho}) voltou com ${fatia.length}` +
                    ` textos, esperavam-se ${esperado}. O corpus do DOM esta incompleto` +
                    " e le-se como texto que o Chrome nao desenha.");
      process.exit(2);
    }
    out.push(...fatia);
  }
  if (out.length !== total) {
    console.error(`ERRO: o corpus do DOM tem ${out.length} textos e a pagina contou ${total}.`);
    process.exit(2);
  }
  return out;
}
const textosDom = txtComputado ? await puxarTextos(txtComputado.nos) : [];

await c.envia("DOM.enable", {}, sessionId);
await c.envia("Accessibility.enable", {}, sessionId);
const ax = await c.envia("Accessibility.getFullAXTree", {}, sessionId);

// A subida por `parentId` até ao primeiro antepassado com `backendDOMNodeId`:
// um `InlineTextBox` não tem nó de DOM próprio (é uma caixa de layout), e sem
// esta subida o texto sai sem sítio — o que serve para o multiconjunto e não
// serve para dizer ONDE falta.
const porId = new Map(ax.nodes.map((n) => [n.nodeId, n]));
function ancoraDom(n) {
  let cur = n;
  for (let i = 0; i < 64 && cur; i++) {
    if (cur.backendDOMNodeId !== undefined) return cur.backendDOMNodeId;
    cur = porId.get(cur.parentId);
  }
  return null;
}

const vistos = new Set();
let repetidos = 0;
const linhas = [];
let fragmentos = 0, marcadores = 0, charsRepetidos = 0, charsEmitidos = 0;
for (const nodo of ax.nodes) {
  const papel = nodo.role?.value;
  if (papel !== "InlineTextBox" && papel !== "ListMarker") continue;
  const t = nodo.name?.value;
  if (typeof t !== "string" || !t) continue;
  // O `ListMarker` tem um `StaticText`/`InlineTextBox` por baixo com o mesmo
  // texto. Contá-lo nos dois sítios duplicaria "1. " no multiconjunto, então o
  // marcador sai APENAS como marcador e o seu InlineTextBox filho é saltado.
  if (papel === "InlineTextBox") {
    const pai = porId.get(nodo.parentId);
    const avo = pai && porId.get(pai.parentId);
    if (pai?.role?.value === "ListMarker" || avo?.role?.value === "ListMarker") continue;
  }
  if (vistos.has(nodo.nodeId)) { repetidos++; charsRepetidos += [...t].filter((c) => !/\s/.test(c)).length; continue; }
  vistos.add(nodo.nodeId);
  if (papel === "ListMarker") marcadores++; else {
    fragmentos++;
    charsEmitidos += [...t].filter((c) => !/\s/.test(c)).length;
  }
  linhas.push(JSON.stringify({
    k: papel === "ListMarker" ? "marker" : "text",
    t,
    dom: ancoraDom(nodo),
  }));
}

const cabecalho = JSON.stringify({
  __meta: 1, lado: "chrome-text", ficheiro: alvo, viewport: [1280, 800],
  jsDaPagina: "desligado", fonte: "Accessibility.getFullAXTree",
  axNodes: ax.nodes.length,
});
const rodape = JSON.stringify({
  __fim: 1, emitidos: linhas.length, fragmentos, marcadores,
  repetidosDescartados: repetidos,
  // A MASSA dos descartados e a dos emitidos, em caracteres. O número de nós
  // não diz se o que se descartou era um pedaço grande do corpus — e a AX está
  // 16 440 caracteres abaixo do que o DOM desenha, portanto a pergunta "quanto
  // é que a deduplicação levou?" tem de ter resposta em vez de suposição.
  charsRepetidosDescartados: charsRepetidos,
  charsEmitidos,
  // `marcadores` acima é o que a AX REPORTA; este é o que a página PINTA. Os
  // dois ficam, e ficam com nomes diferentes, porque a diferença entre eles é
  // uma propriedade do instrumento que quem lê o relatório tem de poder ver.
  marcadoresPintados: marcComputados?.pinta ?? null,
  marcadoresPorTipo: marcComputados?.porTipo ?? null,
  // Os que computam marcador mas não geram caixa (antepassado `display:none`).
  // Contados e NÃO somados — a régua tem de poder ver que foram excluídos.
  marcadoresOcultos: marcComputados?.ocultos ?? null,
  // O texto que o DOM desenha, contado fora da AX (ver o censo acima).
  textoCaracteres: txtComputado?.chars ?? null,
  textoNos: txtComputado?.nos ?? null,
  textoNosOcultos: txtComputado?.ocultos ?? null,
  textoCharsDoArray: txtComputado?.charsDoArray ?? null,
});
writeFileSync(saida, [cabecalho, ...linhas, rodape].join("\n") + "\n");

// O corpus do DOM sai num ficheiro A PARTE, e de proposito: o contrato do
// ficheiro principal e "uma linha por fragmento da AX, e o rodape confere o
// total". Misturar as duas fontes no mesmo sitio faria a regua somar duas
// leituras da mesma pagina - que e a classe de erro que este dia documenta.
// Aqui ficam lado a lado, comparaveis e nunca somadas.
const saidaDom = saida.replace(/[.]jsonl$/, "") + ".domtext.jsonl";
// AUTO-CONTROLO DO TRANSPORTE.
//
// O censo conta os caracteres DENTRO da pagina; as strings atravessam o CDP em
// fatias (ver puxarTextos). Este controlo reconta do lado de ca e compara.
//
// Existe por uma razao especifica: um corpus truncado le-se exatamente como
// "o Chrome nao desenha este texto" — indistinguivel de um motor a falhar — e
// o contador, sendo um numero, atravessa sempre inteiro, portanto a perda seria
// invisivel a quem so olhasse para ele.
//
// E ja apanhou um erro, embora nao o que veio procurar. Durante algumas horas
// os dois numeros discordavam em 17 536 caracteres e a leitura obvia era
// truncagem. Nao era: a expressao do censo viaja num TEMPLATE LITERAL, onde
// \s nao e a classe de espaco mas a letra "s" — a pagina recebia /s/ e contava
// tudo o que nao fosse um "s". O contador estava errado e o transporte limpo.
// Por isso a mensagem abaixo diz que os dois discordam, e nao qual deles mente.
const charsRecontados = textosDom.reduce(
  (n, t) => n + [...String(t)].filter((c) => !/\s/.test(c)).length, 0);
if (txtComputado && charsRecontados !== txtComputado.chars) {
  console.error(`AVISO: as duas contagens do corpus do DOM DISCORDAM — ` +
                `${txtComputado.chars} contados na pagina, ${charsRecontados} recontados aqui ` +
                `(diferenca ${txtComputado.chars - charsRecontados}).`);
  console.error("  Uma das duas esta errada e este aviso NAO diz qual: pode ser o corpus a");
  console.error("  chegar cortado, ou o contador da pagina a contar mal. Nao use nenhum dos");
  console.error("  dois numeros ate saber qual — os dois lados sao codigo deste ficheiro.");
}
writeFileSync(saidaDom, [
  JSON.stringify({ __meta: 1, lado: "chrome-domtext", ficheiro: alvo,
                   fonte: "TreeWalker(SHOW_TEXT) + Range.getClientRects + checkVisibility" }),
  ...textosDom.map((t) => JSON.stringify({ k: "text", t })),
  JSON.stringify({ __fim: 1, emitidos: textosDom.length,
                   // DOIS totais, e nao um: o contado NA PAGINA e o que se
                   // consegue recontar do que chegou. Ver a verificacao abaixo.
                   caracteres: txtComputado?.chars ?? 0,
                   caracteresRecontados: charsRecontados }),
].join("\n") + "\n");

console.log(`chrome-text: ${fragmentos} fragmentos, ${marcadores} marcadores de lista pela AX, ` +
            `${marcComputados?.pinta ?? "?"} PINTADOS pelo estilo computado, ` +
            `${repetidos} entradas repetidas descartadas, de ${ax.nodes.length} nós AX`);

ws.close();
chrome.kill();
