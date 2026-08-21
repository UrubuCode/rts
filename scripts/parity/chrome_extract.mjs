// Lado CHROME do harness de paridade: a mesma linha JSON por elemento.
//
//   node scripts/parity/chrome_extract.mjs <html> <saida.jsonl>
//
// O Chrome aqui é RÉGUA e não capacidade: nada do que ele faz entra no nosso
// motor. Por isso o script não usa puppeteer nem nada que traga um modelo de
// página junto — fala CDP cru sobre o WebSocket do Node 22, que é o mínimo
// para pedir "navega, mede, devolve".
//
// Três decisões que mudam O QUE se compara, ditas aqui porque um número medido
// com outra escolha não é o mesmo número:
//
//  1. **JavaScript da página DESLIGADO** (`Emulation.setScriptExecutionDisabled`).
//     O nosso lado corre `parseHtml` + cascata + layout e não executa `<script>`;
//     deixar o Chrome executar compararia uma página depois do JS com uma antes.
//     O `Runtime.evaluate` deste extrator continua a correr — a flag trava o
//     script DA PÁGINA, não o do depurador.
//  2. **Viewport fixo 1280x800** por `Emulation.setDeviceMetricsOverride`, e não
//     pelo tamanho da janela: uma janela headless traz barras e DPI do sistema, e
//     o nosso `Dom` assume exatamente 1280x800.
//  3. **`getBoundingClientRect` com a página no topo.** É relativo ao VIEWPORT;
//     o nosso layout responde em coordenadas de documento. Com `scrollY === 0` os
//     dois são a mesma coisa, e o extrator afirma isso em vez de esperar — se a
//     página tiver rolado, a linha `__meta` diz e o comparador recusa.

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
const saida = resolve(process.argv[3] ?? "scripts/parity/out/chrome.jsonl");
const PORTA = Number(process.env.CDP_PORT ?? 9333);

const perfil = resolve("scripts/parity/out/.chrome-profile");
const chrome = spawn(CHROME, [
  "--headless=new",
  `--remote-debugging-port=${PORTA}`,
  `--user-data-dir=${perfil}`,
  "--no-first-run", "--no-default-browser-check",
  "--disable-extensions", "--disable-background-networking",
  // A página é local e o harness é sobre LAYOUT: qualquer ida à rede muda o
  // que está medido (uma fonte que chega tarde re-mede texto) e não muda nada
  // do nosso lado, que nunca a faria.
  "--disable-remote-fonts",
  "--hide-scrollbars",
  "--force-device-scale-factor=1",
  "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });
chrome.stderr.on("data", () => {});

/// O `/json/version` só responde depois de o servidor de depuração subir. Sem
/// esta espera o primeiro fetch falha e o erro que sai é "connection refused",
/// que aponta para o sítio errado.
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

/// Cliente CDP mínimo: um id por pedido, uma promessa por id, e as sessões
/// achatadas (`flatten`) para que a mensagem de um alvo chegue no mesmo socket.
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

/// O extrator, como fonte a avaliar DENTRO da página. Vive numa string porque é
/// isso que o `Runtime.evaluate` aceita; o formato de cada linha e a regra do
/// caminho são os mesmos de `examples/claude-parity-rts.ts` e é essa igualdade
/// que faz os dois ficheiros casarem elemento a elemento.
const EXTRATOR = `(() => {
  const PROPS = ["display", "position", "color", "background-color", "font-size"];
  const BLOCO = /^(block|list-item|flow-root|table-cell|table-caption)$/;
  // Um bloco cujo conteudo e SO texto e inline: nenhum descendente com display
  // de bloco. E a unica forma de a altura da caixa contar as linhas DESTE
  // elemento, que e o que a medida do avanco por caractere precisa.
  const ehBlocoDeTexto = (el, cs) => {
    if (!BLOCO.test(cs.display)) return false;
    for (const d of el.querySelectorAll("*")) {
      const dd = getComputedStyle(d).display;
      if (dd !== "none" && BLOCO.test(dd)) return false;
    }
    return true;
  };
  const linhas = [];
  const falhas = [];
  const pilha = [[document.documentElement, "html[1]"]];
  while (pilha.length) {
    const [el, caminho] = pilha.pop();
    try {
      const r = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      const o = { p: caminho, tag: el.tagName.toLowerCase(),
                  x: r.x, y: r.y, w: r.width, h: r.height };
      for (const k of PROPS) o[k] = cs.getPropertyValue(k);
      // CAMPOS EXTRA, so onde a pergunta faz sentido: "quantos caracteres cabem
      // numa linha desta largura". Isso pede um bloco cujo conteudo seja texto
      // corrido e mais nada — se houver um bloco la dentro, a altura ja nao
      // conta linhas deste. O criterio existe por custo: 'innerText' forca
      // layout, e pedi-lo aos 16 814 elementos tornava a extracao lenta e o
      // ficheiro maior sem responder a mais nada. Quantos elementos passam nao
      // esta medido — o campo 'chars' no dump e que o diz.
      //
      // 'chars' e o texto RENDERIZADO (o 'innerText' colapsa o whitespace e
      // ignora o que esta escondido), que e o que o motor tem de quebrar em
      // linhas — nao o 'textContent', que conta indentacao do HTML e texto de
      // elementos com display:none.
      if (ehBlocoDeTexto(el, cs)) {
        const t = el.innerText.replace(/\\s+/g, " ").trim();
        if (t) {
          o.chars = t.length;
          o.lh = cs.getPropertyValue("line-height");
        }
      }
      linhas.push(JSON.stringify(o));
    } catch (e) {
      falhas.push(JSON.stringify({ p: caminho, erro: String(e && e.message || e) }));
    }
    const contas = new Map();
    const filhos = [];
    for (const f of el.children) {
      const t = f.tagName.toLowerCase();
      const n = (contas.get(t) ?? 0) + 1;
      contas.set(t, n);
      filhos.push([f, caminho + "/" + t + "[" + n + "]"]);
    }
    for (let i = filhos.length - 1; i >= 0; i--) pilha.push(filhos[i]);
  }
  window.__paridade = linhas;
  window.__paridadeFalhas = falhas;
  return JSON.stringify({ total: linhas.length, falhas: falhas.length,
                          scrollY: window.scrollY, scrollX: window.scrollX,
                          vw: window.innerWidth, vh: window.innerHeight });
})()`;

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
await c.envia("Page.navigate", { url: "file:///" + alvo.replace(/\\/g, "/") }, sessionId);
await carregou;
// O `load` diz que os recursos chegaram, não que o layout assentou. Ler
// `offsetHeight` força um layout síncrono, que é o que garante que o
// `getBoundingClientRect` a seguir não mede uma árvore ainda suja.
//
// Esperar por `requestAnimationFrame` seria o instinto e NÃO funciona aqui: com
// a execução de script da página desligada o callback nunca corre e a promessa
// é recolhida — o CDP responde `-32000 Promise was collected`, um erro que
// aponta para o depurador quando a causa é a flag três linhas acima.
await new Promise((r) => setTimeout(r, 300));
await c.envia("Runtime.evaluate",
  { expression: "document.documentElement.offsetHeight" }, sessionId);

const { result, exceptionDetails } = await c.envia("Runtime.evaluate",
  { expression: EXTRATOR, returnByValue: true }, sessionId);
if (exceptionDetails) throw new Error("extrator falhou: " + JSON.stringify(exceptionDetails));
const meta = JSON.parse(result.value);

// Fatiado porque devolver ~16k linhas numa string só passa dos limites
// confortáveis de uma mensagem CDP; o número de fatias é aritmética do total,
// então uma fatia em falta aparece como linhas em falta no ficheiro.
const partes = [];
const LOTE = 2000;
for (let i = 0; i < meta.total; i += LOTE) {
  const r = await c.envia("Runtime.evaluate", {
    expression: `window.__paridade.slice(${i}, ${i + LOTE}).join("\\n")`,
    returnByValue: true,
  }, sessionId);
  partes.push(r.result.value);
}
const falhas = meta.falhas
  ? (await c.envia("Runtime.evaluate",
      { expression: 'window.__paridadeFalhas.join("\\n")', returnByValue: true },
      sessionId)).result.value
  : "";

const cabecalho = JSON.stringify({
  __meta: 1, lado: "chrome", ficheiro: alvo, viewport: [meta.vw, meta.vh],
  scroll: [meta.scrollX, meta.scrollY], jsDaPagina: "desligado",
  falhasDeExtracao: meta.falhas,
});
const rodape = JSON.stringify({ __fim: 1, emitidos: meta.total });
writeFileSync(saida,
  [cabecalho, ...partes.filter(Boolean), falhas, rodape].filter(Boolean).join("\n") + "\n");

console.log(`chrome: ${meta.total} elementos, ${meta.falhas} falhas de extração, ` +
            `viewport ${meta.vw}x${meta.vh}, scroll ${meta.scrollX},${meta.scrollY}`);

ws.close();
chrome.kill();
