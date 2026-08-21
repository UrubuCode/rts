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
let fragmentos = 0, marcadores = 0;
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
  if (vistos.has(nodo.nodeId)) { repetidos++; continue; }
  vistos.add(nodo.nodeId);
  if (papel === "ListMarker") marcadores++; else fragmentos++;
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
});
writeFileSync(saida, [cabecalho, ...linhas, rodape].join("\n") + "\n");

console.log(`chrome-text: ${fragmentos} fragmentos, ${marcadores} marcadores de lista, ` +
            `${repetidos} entradas repetidas descartadas, de ${ax.nodes.length} nós AX`);

ws.close();
chrome.kill();
