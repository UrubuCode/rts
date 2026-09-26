// Measures the ADVANCE of every character the engine's text measurer knows,
// in Blink, for the four fonts the generic families resolve to on Windows —
// the horizontal half of `crates/rts-dom/src/layout/measure/font_metrics.rs`, whose
// vertical half came from `tests/css/claude-fm-metricas-por-familia`.
//
//   bun scripts/fonte_avancos_medir_edge.mjs avancos.json
//
// Method: `canvas.measureText` at a font size of 2048px, which is the em of
// these fonts — the width of one character IS its `hmtx` advance in font
// units, with no rounding to guess at. Then the INSTRUMENT is checked against
// the layout it stands in for: real strings at 16px in a `<span>`, measured by
// `getBoundingClientRect`, against the sum of the advances. The difference is
// kerning plus subpixel rounding, and it is printed per font so that nobody
// has to believe the tables are linear — they can read how far they are not.
//
// `bun` and not `node`: Node 20 on this machine has no global `WebSocket`.
import { spawn } from "node:child_process";
import { existsSync, rmSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const CHROME = [
  process.env.CHROME_BIN,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
].find((p) => p && existsSync(p));
if (!CHROME) { console.error("neither Edge nor Chrome found — set CHROME_BIN"); process.exit(2); }
const saida = resolve(process.argv[2] ?? "avancos.json");
const PORTA = Number(process.env.CDP_PORT ?? 9338);
const perfil = resolve(process.env.TEMP ?? ".", "edge-avancos-profile");

const chrome = spawn(CHROME, [
  "--headless=new", `--remote-debugging-port=${PORTA}`, `--user-data-dir=${perfil}`,
  "--no-first-run", "--no-default-browser-check", "--disable-extensions",
  "--disable-background-networking", "--disable-remote-fonts", "--force-device-scale-factor=1", "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });
chrome.stderr.on("data", () => {});

async function esperarCdp() {
  const t0 = Date.now();
  for (;;) {
    try { return (await (await fetch(`http://127.0.0.1:${PORTA}/json/version`)).json()).webSocketDebuggerUrl; }
    catch { if (Date.now() - t0 > 30000) throw new Error("CDP did not come up in 30s"); await new Promise((r) => setTimeout(r, 150)); }
  }
}

const MEDIR = `(() => {
  const familias = { serif: "serif", sans: "sans-serif", mono: "monospace", sistema: "system-ui" };
  const chars = [];
  for (let c = 0x20; c <= 0x7e; c++) chars.push(c);
  for (let c = 0xa0; c <= 0xff; c++) chars.push(c);
  for (const c of [0x2013, 0x2014, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2026, 0x20ac, 0x2122]) chars.push(c);
  const ctx = document.createElement("canvas").getContext("2d");
  const frases = ["Hello, World!", "The quick brown fox jumps over the lazy dog.", "AVATAR Toy To. WAVE", "Título 1 — ação çã 100%"];
  const out = { chars, fontes: {}, instrumento: [] };
  for (const [nome, css] of Object.entries(familias)) for (const peso of ["normal", "bold"]) {
    ctx.font = peso + " 2048px " + css;
    out.fontes[nome + "-" + peso] = chars.map((c) => Math.round(ctx.measureText(String.fromCodePoint(c)).width));
    for (const f of frases) {
      const s = document.createElement("span");
      s.style.cssText = "position:absolute;white-space:pre;font:" + peso + " 16px " + css;
      s.textContent = f; document.body.appendChild(s);
      const real = s.getBoundingClientRect().width; s.remove();
      let soma = 0; for (const ch of f) soma += ctx.measureText(ch).width;
      out.instrumento.push({ fonte: nome + "-" + peso, frase: f, blink: real, tabela: soma * 16 / 2048 });
    }
  }
  out.ua = navigator.userAgent;
  return JSON.stringify(out);
})()`;

try {
  const ws = new WebSocket(await esperarCdp());
  await new Promise((r) => ws.addEventListener("open", r));
  let proximo = 0; const pendentes = new Map();
  ws.addEventListener("message", (ev) => { const m = JSON.parse(ev.data); const p = pendentes.get(m.id); if (p) { pendentes.delete(m.id); m.error ? p.rej(new Error(JSON.stringify(m.error))) : p.res(m.result); } });
  const envia = (method, params = {}, sessionId) => new Promise((res, rej) => { const id = ++proximo; pendentes.set(id, { res, rej }); ws.send(JSON.stringify({ id, method, params, sessionId })); });
  const { targetInfos } = await envia("Target.getTargets");
  const { sessionId } = await envia("Target.attachToTarget", { targetId: targetInfos.find((t) => t.type === "page").targetId, flatten: true });
  const { result, exceptionDetails } = await envia("Runtime.evaluate", { expression: MEDIR, returnByValue: true }, sessionId);
  if (exceptionDetails) throw new Error(JSON.stringify(exceptionDetails));
  writeFileSync(saida, result.value);
  const m = JSON.parse(result.value);
  console.log(`${m.chars.length} characters × ${Object.keys(m.fontes).length} fonts -> ${saida}`);
  const pior = {};
  for (const i of m.instrumento) { const d = Math.abs(i.blink - i.tabela); pior[i.fonte] = Math.max(pior[i.fonte] ?? 0, d); }
  console.log("instrument (worst |Blink span − sum of advances| at 16px, per font):");
  for (const [f, d] of Object.entries(pior)) console.log(`  ${f.padEnd(16)} ${d.toFixed(2)}px`);
  ws.close();
} finally {
  chrome.kill();
  try { rmSync(perfil, { recursive: true, force: true }); } catch {}
}
