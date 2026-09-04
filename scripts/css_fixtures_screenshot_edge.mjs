// A CAPTURA do lado Blink da régua de PINTURA: para cada fixture de
// `tests/css/`, abre-a DIRECTAMENTE (não num iframe — a régua de N usa iframe
// porque quer o `contentDocument`; esta régua quer o PIXEL, e um iframe traz
// a moldura da página-mãe para dentro do screenshot) a 1280x800 por CDP cru,
// no Edge headless, e grava `tests/css/pintura/<fixture>.blink.png`.
//
// A canalização CDP é a de `scripts/css_fixtures_medir_edge.mjs`, reutilizada
// verbatim (Target.attachToTarget, Emulation.setDeviceMetricsOverride,
// Runtime.evaluate) — só o fim muda: aqui não há colheita de `getComputedStyle`,
// há `Page.captureScreenshot`.
//
//   bun scripts/css_fixtures_serve.ts &                                  # porta 8731
//   bun scripts/css_fixtures_screenshot_edge.mjs claude-cor-e-fundo      # uma fixture
//   bun scripts/css_fixtures_screenshot_edge.mjs                        # todas
//
// `bun` e não `node`: o Node 20 desta máquina não tem `WebSocket` global (a
// mesma nota do script irmão).
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync, readdirSync } from "node:fs";
import { resolve } from "node:path";

const CHROME = [
  process.env.CHROME_BIN,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
].find((p) => p && existsSync(p));
if (!CHROME) { console.error("nem Edge nem Chrome encontrados — defina CHROME_BIN"); process.exit(2); }

const RAIZ = "tests/css";
const SAIDA = resolve(RAIZ, "pintura");
mkdirSync(SAIDA, { recursive: true });

// Sem argumentos: todas as `.html` do corpus. Com argumentos: só os nomes
// (sem `.html`) dados — o mesmo padrão de `css_fixtures.sh`.
const pedidos = process.argv.slice(2);
const fixtures = (pedidos.length
  ? pedidos.map((n) => (n.endsWith(".html") ? n : `${n}.html`))
  : readdirSync(RAIZ).filter((n) => n.endsWith(".html")).sort()
);

const PORTA = Number(process.env.CDP_PORT ?? 9338);
const perfil = resolve(process.env.TEMP ?? ".", "edge-pintura-profile");

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

let ok = 0; const falhas = [];
for (const nome of fixtures) {
  try {
    const carregou = new Promise((r) => c.quando("Page.loadEventFired", (_p, s) => { if (s === sessionId) r(); }));
    await c.envia("Page.navigate", { url: `http://127.0.0.1:8731/${nome}` }, sessionId);
    await carregou;
    // A folha pode continuar a aplicar layout um frame depois do load (fontes
    // web, reflow tardio) — a mesma razão por que `css_fixtures_medir.md` já
    // espera o `onload` do iframe e não navega direto ao screenshot.
    await new Promise((r) => setTimeout(r, 30));
    const { data } = await c.envia("Page.captureScreenshot", { format: "png", captureBeyondViewport: false }, sessionId);
    writeFileSync(resolve(SAIDA, `${nome.replace(/\.html$/, "")}.blink.png`), Buffer.from(data, "base64"));
    ok++;
  } catch (e) {
    falhas.push(`${nome}: ${e.message}`);
  }
}
console.log(`capturadas ${ok}/${fixtures.length}${falhas.length ? " — falhas: " + JSON.stringify(falhas) : ""}`);
ws.close();
chrome.kill();
