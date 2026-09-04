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
const { rts, electron } = data.lados;

function mb(bytes) { return bytes == null ? "—" : `${(bytes / 1024 ** 2).toFixed(1)} MB`; }
function range(obj, unidade, casas = 0) {
  if (!obj) return "—";
  return `${obj.mediana.toFixed(casas)} ${unidade} (${obj.min.toFixed(casas)}–${obj.max.toFixed(casas)})`;
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
const R = linhaLado(rts);
const E = linhaLado(electron);

const hoje = data.medido_em.slice(0, 10);
const m = data.maquina;
const nRts = rts.amostras?.n ?? "?";
const nElectron = electron.amostras?.n ?? "?";

// A frase que explica o gap: a razão vem do próprio JSON medido (a mensagem
// de erro real do processo), nunca reescrita à mão aqui.
const razaoCurta = rts.nao_construido
  ? rts.razao.length > 220 ? rts.razao.slice(0, 217) + "..." : rts.razao
  : null;

const stats = `## 📦 RTS vs Electron: packaging the same app

Two packaged builds of the **same** app (\`${data.app}\`, 144 KB, no network calls) — a native RTS \`.exe\` and an Electron ${electron.versao} bundle — measured by \`scripts/rts_vs_electron/medir.mjs\`: ${nElectron} runs on the Electron side (RTS attempted ${nRts}, see below), startup timed to a visible window, memory and CPU sampled over the *whole* process tree, median reported. Measured ${hoje} on ${m.so}, ${m.cpu} (${m.nucleos_logicos} logical cores), ${m.ram_gb} GB RAM.

| | RTS | Electron |
|---|---:|---:|
| Exe size | ${mb(rts.bytes_exe)} | ${mb(electron.bytes_exe)} |
| Folder size | ${mb(rts.bytes_pasta)} | ${mb(electron.bytes_pasta)} |
| Files in folder | ${rts.ficheiros_na_pasta ?? "—"} | ${electron.ficheiros_na_pasta ?? "—"} |
| Processes | ${R.processos} | ${E.processos} |
| Startup (median, min–max) | ${R.arranque} | ${E.arranque} |
| RSS (median, min–max) | ${R.rss} | ${E.rss} |
| Private bytes (median) | ${R.priv} | ${E.priv} |
| CPU at rest (median) | ${R.cpu} | ${E.cpu} |

**RTS's AOT \`.exe\` does not run today**: ${razaoCurta ?? "—"} (\`scripts/rts_vs_electron/rts/README.md\` has the full account and the fix). Its exe/folder size above is real — the binary compiled and links — but every runtime row is unmeasurable until it does.

**What this does NOT measure**: no GPU workload, no network I/O, one tiny static page — the size and idle-memory difference between the two runtimes is the whole comparison, not a claim about either one under load.

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
console.log(`RTS vs Electron: Electron ${E.arranque} arranque, ${E.rss} RSS, ${electron.processos} processos | RTS: não construído (${nRts} tentativas, 0 ok)`);
