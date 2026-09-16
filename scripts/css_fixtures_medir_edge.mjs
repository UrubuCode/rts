// Mede TODAS as fixtures de tests/css/ num Blink headless, por CDP cru — sem o
// MCP chrome-devtools e sem o Chrome instalado: o Edge é o mesmo Blink. A
// função de colheita é a de scripts/css_fixtures_medir.md; a canalização CDP é
// a de scripts/parity/chrome_extract.mjs.
//
//   bun scripts/css_fixtures_serve.ts &                       # porta 8731
//   bun scripts/css_fixtures_medir_edge.mjs medidas.json      # Edge por omissão
//   CHROME_BIN="C:/.../chrome.exe" bun scripts/css_fixtures_medir_edge.mjs m.json
//
// `bun` e não `node`: o Node 20 desta máquina não tem `WebSocket` global.
//
// O ficheiro de saída tem TODAS as medições; escrever um `.esperado.json` a
// partir dele é um passo à parte e deliberado — NUNCA se regrava um esperado
// que já existe para um número subir (tests/css/README.md). Antes de confiar
// num esperado novo, valide o INSTRUMENTO: a 2026-09-04 as 49 fixtures com
// esperado medido no Chrome foram re-medidas por este script no Edge 152 —
// 1 104 números, pior desvio 0. Repita essa comparação se o Edge mudar.
import { spawn } from "node:child_process";
import { existsSync, rmSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const CHROME = [
  process.env.CHROME_BIN,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
].find((p) => p && existsSync(p));
if (!CHROME) { console.error("nem Edge nem Chrome encontrados — defina CHROME_BIN"); process.exit(2); }
const saida = resolve(process.argv[2] ?? "medidas.json");
const PORTA = Number(process.env.CDP_PORT ?? 9337);
const perfil = resolve(process.env.TEMP ?? ".", "edge-fixtures-profile");

// As duas precondicoes, verificadas ANTES de lancar o browser. Nao verificar
// custou uma sessao inteira: sem o servidor, `Page.navigate` responde na
// mesma e `Page.loadEventFired` NUNCA chega, entao o script fica pendurado
// num `await` sem mensagem — e quem olha conclui que o CDP travou. Uma
// pendencia sem causa visivel e um diagnostico errado garantido.
const RAIZ_FIXTURES = "tests/css";
const PALCO = RAIZ_FIXTURES + "/__harness.tmp.html";
const SERVIDOR = `http://127.0.0.1:${process.env.CSS_PORTA ?? 8731}`;
try { await fetch(SERVIDOR + "/lista", { signal: AbortSignal.timeout(3000) }); }
catch { console.error(`${SERVIDOR} nao responde — corra primeiro: bun scripts/css_fixtures_serve.ts`); process.exit(2); }

// O palco e deste script e nao do repositorio: e um ficheiro temporario que a
// receita em scripts/css_fixtures_medir.md mandava gravar a mao, e um passo a
// mao e um passo que se esquece.
const palcoJaLaEstava = existsSync(PALCO);
if (!palcoJaLaEstava) writeFileSync(PALCO, `<!DOCTYPE html><html><head><style>html,body{margin:0;padding:0;overflow:hidden}
iframe{width:1280px;height:800px;border:0;display:block}</style></head>
<body><iframe id="palco"></iframe></body></html>`);

const chrome = spawn(CHROME, [
  "--headless=new", `--remote-debugging-port=${PORTA}`, `--user-data-dir=${perfil}`,
  "--no-first-run", "--no-default-browser-check", "--disable-extensions",
  "--disable-background-networking", "--disable-remote-fonts", "--hide-scrollbars",
  "--force-device-scale-factor=1", "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });
chrome.stderr.on("data", () => {});

async function esperarCdp() {
  const t0 = Date.now();
  for (;;) {
    try { const r = await fetch(`http://127.0.0.1:${PORTA}/json/version`); return (await r.json()).webSocketDebuggerUrl; }
    catch { if (Date.now() - t0 > 30000) throw new Error("CDP não subiu em 30s"); await new Promise((r) => setTimeout(r, 150)); }
  }
}
function cliente(ws) {
  let proximo = 0; const pendentes = new Map(); const eventos = new Map();
  ws.addEventListener("message", (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id !== undefined) { const p = pendentes.get(m.id); pendentes.delete(m.id); m.error ? p.rej(new Error(JSON.stringify(m.error))) : p.res(m.result); }
    else (eventos.get(m.method) ?? []).forEach((f) => f(m.params, m.sessionId));
  });
  return {
    envia(method, params = {}, sessionId) { const id = ++proximo; return new Promise((res, rej) => { pendentes.set(id, { res, rej }); ws.send(JSON.stringify({ id, method, params, sessionId })); }); },
    quando(method, f) { if (!eventos.has(method)) eventos.set(method, []); eventos.get(method).push(f); },
  };
}

// A função de colheita de scripts/css_fixtures_medir.md, verbatim no essencial.
const COLHEITA = `(async () => {
  const PROPS = ["display","position","color","background-color","opacity","visibility",
    "z-index","font-size","line-height","text-align","white-space","letter-spacing",
    "overflow","box-sizing","float","clear","flex-direction","flex-wrap",
    "justify-content","align-items","grid-template-columns","grid-template-rows","gap",
    "font-weight","padding-top","padding-left","margin-top","margin-bottom","border-spacing"];
  const nomes = await (await fetch("/lista")).json();
  const palco = document.getElementById("palco");
  const saida = {}; const problemas = [];
  for (const nome of nomes) {
    if (nome.startsWith("__")) continue;
    await new Promise((ok, falha) => { palco.onload = ok; palco.onerror = () => falha(new Error("onerror " + nome)); palco.src = "/" + nome; });
    const d = palco.contentDocument;
    // As propriedades que a fixture PEDE (meta fixar-estilo) entram sempre na
    // colheita, alem das 23 canonicas: o corredor compara o que a fixture pede,
    // e um esperado sem essas chaves da "ausente do .esperado.json" em vez de
    // um numero. Foi o que aconteceu as cinco fixtures de texto do lote S.
    const metaFixar = d.querySelector('meta[name="fixar-estilo"]');
    const extras = metaFixar ? metaFixar.getAttribute("content").split(",").map(s => s.trim()).filter(Boolean) : [];
    const props = PROPS.concat(extras.filter(p => !PROPS.includes(p)));
    if (d.defaultView.innerWidth !== 1280 || d.defaultView.innerHeight !== 800) problemas.push(nome + ": viewport " + d.defaultView.innerWidth + "x" + d.defaultView.innerHeight);
    if (d.documentElement.scrollHeight > 800) problemas.push(nome + ": transborda");
    const caixas = {};
    for (const el of d.querySelectorAll("[id]")) {
      const r = el.getBoundingClientRect(); const cs = d.defaultView.getComputedStyle(el); const estilo = {};
      for (const p of props) estilo[p] = cs.getPropertyValue(p);
      caixas[el.id] = { rect: [Math.round(r.x*100)/100, Math.round(r.y*100)/100, Math.round(r.width*100)/100, Math.round(r.height*100)/100], estilo };
    }
    saida[nome] = { elementos: caixas };
  }
  return JSON.stringify({ medidas: saida, pedidas: nomes.filter(n => !n.startsWith("__")).length, medidas_n: Object.keys(saida).length, problemas, ua: navigator.userAgent });
})()`;

const wsUrl = await esperarCdp();
const ws = new WebSocket(wsUrl);
await new Promise((r) => ws.addEventListener("open", r));
const c = cliente(ws);
const { targetInfos } = await c.envia("Target.getTargets");
const alvo = targetInfos.find((t) => t.type === "page");
const { sessionId } = await c.envia("Target.attachToTarget", { targetId: alvo.targetId, flatten: true });
await c.envia("Page.enable", {}, sessionId);
await c.envia("Runtime.enable", {}, sessionId);
await c.envia("Emulation.setDeviceMetricsOverride", { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false }, sessionId);
const carregou = new Promise((r) => c.quando("Page.loadEventFired", (_p, s) => { if (s === sessionId) r(); }));
await c.envia("Page.navigate", { url: SERVIDOR + "/__harness.tmp.html" }, sessionId);
await Promise.race([carregou, new Promise((_, rej) => setTimeout(() => rej(new Error("o palco nao disparou load em 30s")), 30000))]);
const { result, exceptionDetails } = await c.envia("Runtime.evaluate", { expression: COLHEITA, awaitPromise: true, returnByValue: true }, sessionId);
if (exceptionDetails) throw new Error("colheita falhou: " + JSON.stringify(exceptionDetails));
writeFileSync(saida, result.value);
const m = JSON.parse(result.value);
console.log(`pedidas=${m.pedidas} medidas=${m.medidas_n} problemas=${JSON.stringify(m.problemas)} ua=${m.ua}`);
ws.close(); chrome.kill();
if (!palcoJaLaEstava) rmSync(PALCO, { force: true });
