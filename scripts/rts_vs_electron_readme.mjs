// Reescreve a região RTS_VS_ELECTRON do README.md com o que
// scripts/rts_vs_electron/medir.mjs acabou de medir, e lê apenas
// .github/rts_vs_electron.json — nunca um número escrito à mão. Molde:
// scripts/css_parity_readme.mjs (mesmo padrão: o ficheiro gerado é a fonte,
// o README é a vista).
//
//   node scripts/rts_vs_electron_readme.mjs
import { readFileSync, writeFileSync, existsSync } from "node:fs";

const jsonPath = ".github/rts_vs_electron.json";
const readmePath = "README.md";

if (!existsSync(jsonPath)) {
  console.error(`${jsonPath} não existe — corra scripts/rts_vs_electron/medir.mjs primeiro`);
  process.exit(2);
}
const data = JSON.parse(readFileSync(jsonPath, "utf8"));
const { electron, rts_aot, rts_jit } = data.lados;

function mb(bytes) { return bytes == null ? "—" : `${(bytes / 1024 ** 2).toFixed(1)} MB`; }
function range(obj, unidade, casas = 0) {
  if (!obj) return "—";
  return `${obj.mediana.toFixed(casas)} ${unidade} (${obj.min.toFixed(casas)}–${obj.max.toFixed(casas)})`;
}
function jsDaPagina(lado) {
  if (lado.nao_construido) return "—";
  if (lado.js_da_pagina === true) return "yes";
  if (lado.js_da_pagina === false) return "**no**";
  return "?";
}
function linhaLado(lado) {
  if (lado.nao_construido) {
    return { processos: "—", arranque: "**not built**", rss: "—", priv: "—", cpu: "—" };
  }
  return {
    processos: String(lado.processos),
    arranque: range(lado.arranque_ms, "ms"),
    rss: range(lado.rss_mb, "MB", 1),
    priv: mb(lado.private_mb * 1024 ** 2),
    cpu: `${lado.cpu_repouso_pct}%`,
  };
}
const E = linhaLado(electron);
const A = linhaLado(rts_aot);
const J = linhaLado(rts_jit);

const hoje = data.medido_em.slice(0, 10);
const m = data.maquina;
const nElectron = electron.amostras?.n ?? "?";
const nAot = rts_aot.amostras?.n ?? "?";
const nJit = rts_jit.amostras?.n ?? "?";

// A frase que explica o gap do AOT: a razão vem do próprio JSON medido (a
// mensagem de erro real do processo, lida do stderr), nunca reescrita à mão
// aqui — se o texto do motor mudar, esta frase muda sozinha na próxima corrida.
function curta(s, max = 220) {
  if (!s) return null;
  return s.length > max ? s.slice(0, max - 3) + "..." : s;
}
const razaoAotNaoConstruido = rts_aot.nao_construido ? curta(rts_aot.razao) : null;
const razaoAotJsDaPagina = !rts_aot.nao_construido && rts_aot.js_da_pagina === false
  ? curta(rts_aot.razao_js_da_pagina)
  : null;

const stats = `## 📦 RTS vs Electron: packaging the same app

Three builds of the **same** app (\`${data.app}\`, ~145 KB, no network calls) — an Electron ${electron.versao} bundle, a native RTS \`.exe\` (AOT, \`rts compile\`), and the RTS \`rts.exe\` engine running the \`.ts\` source (JIT, \`rts run\`) — measured by \`scripts/rts_vs_electron/medir.mjs\`: ${nElectron} Electron runs, ${nAot} AOT runs, ${nJit} JIT runs, startup timed to a visible window, memory and CPU sampled over the *whole* process tree, median reported. Measured ${hoje} on ${m.so}, ${m.cpu} (${m.nucleos_logicos} logical cores), ${m.ram_gb} GB RAM.

| | Electron | RTS \`.exe\` AOT | RTS \`rts.exe\` + app |
|---|---:|---:|---:|
| Exe size | ${mb(electron.bytes_exe)} | ${mb(rts_aot.bytes_exe)} | ${mb(rts_jit.bytes_exe)} |
| Folder size | ${mb(electron.bytes_pasta)} | ${mb(rts_aot.bytes_pasta)} | ${mb(rts_jit.bytes_pasta)} |
| Files in folder | ${electron.ficheiros_na_pasta ?? "—"} | ${rts_aot.ficheiros_na_pasta ?? "—"} | ${rts_jit.ficheiros_na_pasta ?? "—"} |
| Page JavaScript runs | ${jsDaPagina(electron)} | ${jsDaPagina(rts_aot)} | ${jsDaPagina(rts_jit)} |
| Processes | ${E.processos} | ${A.processos} | ${J.processos} |
| Startup (median, min–max) | ${E.arranque} | ${A.arranque} | ${J.arranque} |
| RSS (median, min–max) | ${E.rss} | ${A.rss} | ${J.rss} |
| Private bytes (median) | ${E.priv} | ${A.priv} | ${J.priv} |
| CPU at rest (median) | ${E.cpu} | ${A.cpu} | ${J.cpu} |

${razaoAotNaoConstruido ? `**RTS's AOT \`.exe\` does not run at all today**: ${razaoAotNaoConstruido} (\`scripts/rts_vs_electron/rts/README.md\` has the full account). Its exe/folder size above is real — the binary compiled and links — but every runtime row is unmeasurable until it does.\n\n` : ""}${razaoAotJsDaPagina ? `**RTS's AOT \`.exe\` starts and paints the static HTML/CSS**, same as the JIT side — but its page \`<script>\`s do not run: "${razaoAotJsDaPagina}". An AOT binary carries no compiler, so any JavaScript that only exists as page-script text (not referenced statically by the \`.ts\` that got compiled) has nothing to run it; the window opens and stays blank for *this* app, which is React mounted entirely from a \`<script>\` block. \`scripts/rts_vs_electron/rts/README.md\` has the full account.\n\n` : ""}**Honesty: the side comparable to Electron today is the last column, not the middle one.** Electron ships a JS engine (V8) inside its runtime, so any page script runs regardless of how the app was built. RTS's AOT \`.exe\` does not — it only runs the JavaScript that was compiled INTO it ahead of time, so it fits an app with no page \`<script>\` (or one whose HTML is generated from already-compiled TS, not read as text). \`rts.exe\` + the app is the actual Electron-equivalent pair (engine binary with a compiler + the page it opens, same relationship as Chromium + \`app.asar\`), and it is the only RTS column above where the same React page the Electron column renders also renders here.

**What this does NOT measure**: no GPU workload, no network I/O, one tiny static-plus-React page — the size and idle-memory difference between the three is the whole comparison, not a claim about any of them under load.

*Updated ${hoje} by \`scripts/rts_vs_electron/medir.mjs\`.*`;

let readme = readFileSync(readmePath, "utf8");
if (/<!-- RTS_VS_ELECTRON_START -->/.test(readme)) {
  readme = readme.replace(
    /<!-- RTS_VS_ELECTRON_START -->[\s\S]*?<!-- RTS_VS_ELECTRON_END -->/,
    `<!-- RTS_VS_ELECTRON_START -->\n${stats}\n<!-- RTS_VS_ELECTRON_END -->`,
  );
} else {
  const anchor = "<!-- CSS_DOM_STATS_END -->";
  if (!readme.includes(anchor)) {
    console.error(`README.md não tem ${anchor} nem RTS_VS_ELECTRON_START — não sei onde inserir`);
    process.exit(2);
  }
  readme = readme.replace(
    anchor,
    `${anchor}\n\n<!-- RTS_VS_ELECTRON_START -->\n${stats}\n<!-- RTS_VS_ELECTRON_END -->`,
  );
}
writeFileSync(readmePath, readme);
console.log(`RTS vs Electron: Electron ${E.arranque} arranque, ${E.rss} RSS | AOT: ${rts_aot.nao_construido ? "não construído" : A.arranque} | JIT: ${rts_jit.nao_construido ? "não construído" : J.arranque}`);
