// Mede arranque/memória/CPU/tamanho de TRÊS artefactos da MESMA app React
// (o `.html` de scripts/rts_vs_electron/app/index.html) — um `.exe` Electron,
// um `.exe` AOT do RTS, e o `rts.exe` do motor a correr `.ts` fonte — e grava
// .github/rts_vs_electron.json (histórico legível por máquina, no mesmo
// espírito do css_parity_report.json: o número fica no ficheiro gerado, não
// escrito à mão).
//
//   node scripts/rts_vs_electron/medir.mjs
//
// PORQUÊ TRÊS e não dois: o `.exe` AOT (`rts compile`) e o `rts.exe` que
// corre `.ts` (JIT) são coisas DIFERENTES — o AOT não leva o compilador
// consigo (por isso o JS da própria página, compilado em runtime via
// `DomScope.run`, falha nele — ver `js_da_pagina` abaixo), o JIT leva. O lado
// comparável ao "Chromium + app.asar" do Electron é o `rts.exe` (o binário do
// MOTOR, com compilador) + a página — não o `.exe` AOT, que só serve uma app
// sem JS de página (ou TS já compilado nela). Medir os dois lados do RTS lado
// a lado é o que torna essa distinção visível em vez de escondida atrás de um
// "RTS" genérico.
//
// PORQUÊ uma corrida = uma invocação do PowerShell (start+poll+medir+matar),
// e não Node a arrancar o processo e o PowerShell só a olhar para ele: a
// janela de tempo entre "Node lança" e "PowerShell primeiro vê o PID" seria
// relógio cruzado entre dois processos, e cada `powershell.exe` novo custa
// ~150-300ms de arranque — inaceitável dentro de um poll de 50ms. Uma única
// invocação mede do lado de dentro, com o mesmo relógio do início ao fim.
//
// PORQUÊ PowerShell e não Node puro: MainWindowHandle e a árvore de processos
// por ParentProcessId (Win32_Process) não têm equivalente no `child_process`
// do Node — são API do Windows. Tamanho de ficheiros/pastas e informação da
// máquina (SO, CPU, RAM) o `fs`/`os` do Node já dão, por isso ficam em JS.
//
// PORQUÊ CPU "em repouso" como tempo-de-CPU-somado/tempo-de-parede (convenção
// `top` do Unix, pode passar de 100% com várias threads ocupadas em vários
// núcleos) e não dividido pelo nº de núcleos: a árvore Electron tem 4
// processos (main+gpu+utility+renderer) e cada um pode usar um núcleo
// diferente ao mesmo tempo; dividir pelos núcleos esconderia isso.
import { existsSync, readdirSync, statSync, readFileSync, writeFileSync, mkdirSync, copyFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { cpus, totalmem, platform, release, tmpdir } from "node:os";
import { spawnSync } from "node:child_process";

// N por variável de ambiente: localmente 5 chega (poucos minutos), mas o job
// de CI (.github/workflows/build-artifacts.yml, `rts-vs-electron`) empacota o
// Electron do zero a cada corrida — 3 chega para um número estável dentro do
// orçamento de tempo do runner, e o default local fica intocado.
const N = Number(process.env.RTS_VS_ELECTRON_N) || 5;
const WINDOW_TIMEOUT_MS = 15000; // generoso: as três janelas abrem em <1s hoje
const POST_WINDOW_WAIT_MS = 4000; // pedido: deixar assentar antes de medir memória
const CPU_SAMPLE_MS = 2000; // pedido: amostra de CPU em repouso

// Derivados do ambiente, não fixos à máquina do Marcos: `tmpdir()` dá o mesmo
// caminho localmente (é o que o Node já resolvia por trás do valor fixo
// anterior) e dá o TEMP efémero do runner em CI — sem isto o script falhava
// fora dessa uma máquina. REPO_ROOT vem da localização do PRÓPRIO ficheiro
// (scripts/rts_vs_electron/medir.mjs, dois níveis acima da raiz), não do cwd,
// para funcionar seja qual for o diretório de onde `node` é invocado.
const TEMP_ROOT = join(tmpdir(), "rts-vs-electron");
const MED_DIR = join(TEMP_ROOT, "_medicoes"); // stderr por corrida — efémero, fora do repo
const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const OUT_JSON = join(REPO_ROOT, ".github", "rts_vs_electron.json");

// A app React que os três lados abrem. UMA fonte committada
// (scripts/rts_vs_electron/app/index.html — 144 KB, bundles React/ReactDOM
// UMD do CDN + um componente com contador e lista, sem chamadas de rede) —
// nunca `examples/react-app.html`: esse nome cai na regra `react-*.html` do
// `.gitignore` ("são entrada de uma medição, não fonte"), então não existe
// fora da máquina de quem a criou à mão. O lado JIT lê `"react-app.html"`
// RELATIVO AO CWD (`examples/claude-react-janela.ts`), por isso é copiada
// para a pasta de trabalho do JIT com esse nome — ver `prepararLadoJit`.
const APP_HTML = join(REPO_ROOT, "scripts", "rts_vs_electron", "app", "index.html");

function psQuote(s) {
  return "'" + String(s).replace(/'/g, "''") + "'";
}

// Soma bytes e conta ficheiros recursivamente — o que o `Get-ChildItem
// -Recurse | Measure-Object` fazia na construção dos artefactos, mas em Node
// porque não precisa de nada específico do Windows.
function folderStats(dir) {
  let bytes = 0;
  let ficheiros = 0;
  const stack = [dir];
  while (stack.length) {
    const d = stack.pop();
    let entries;
    try { entries = readdirSync(d, { withFileTypes: true }); } catch { continue; }
    for (const e of entries) {
      const p = join(d, e.name);
      if (e.isDirectory()) { stack.push(p); continue; }
      try { bytes += statSync(p).size; ficheiros++; } catch { /* ficheiro pode ter desaparecido entre listar e stat */ }
    }
  }
  return { bytes_pasta: bytes, ficheiros_na_pasta: ficheiros };
}

function mediana(nums) {
  const a = [...nums].sort((x, y) => x - y);
  const mid = Math.floor(a.length / 2);
  return a.length % 2 ? a[mid] : (a[mid - 1] + a[mid]) / 2;
}
function agregaMinMedMax(nums, casas = 0) {
  const r = (v) => Math.round(v * 10 ** casas) / 10 ** casas;
  return { mediana: r(mediana(nums)), min: r(Math.min(...nums)), max: r(Math.max(...nums)) };
}

// Lê o stderr de uma corrida à procura do sinal de que o JS DA PÁGINA (os
// `<script>` do HTML, não o `.ts`/`.exe` em si) não correu — a mensagem exata
// que `DomScope.run` (crates/rts-dom-bridge/src/scope.rs) escreve quando o
// binário não tem o compilador consigo: "<script> N de <url> falhou: a fonte
// não compilou". Procurado no stderr em vez de assumido por qual LADO está a
// correr, para o número vir do processo real e não de uma etiqueta escrita à
// mão — se um dia o AOT ganhar o compilador, este texto simplesmente para de
// aparecer e o campo muda sozinho.
function lerFalhaJsDaPagina(stderrFile) {
  if (!stderrFile || !existsSync(stderrFile)) return null;
  let texto;
  try { texto = readFileSync(stderrFile, "utf8"); } catch { return null; }
  const linhas = texto.split(/\r?\n/).filter((l) => /<script>\s*\d+\s*de\s.*falhou:/.test(l));
  if (linhas.length === 0) return null;
  const exemplo = linhas[0].trim();
  return linhas.length > 1 ? `${exemplo} (×${linhas.length} nesta corrida)` : exemplo;
}

// O script PowerShell de UMA corrida: arranca (com argumentos, para o lado
// JIT — `rts.exe run ficheiro.ts` — os outros dois vão sem nenhum), espera a
// janela (processo ou filho direto — no Electron é sempre o principal),
// espera 4s, soma RSS/private de TODA a árvore (recursivo por
// ParentProcessId — a Electron tem main+gpu+utility+renderer), amostra CPU
// 2s, mata a árvore SEMPRE (try/finally) mesmo que a medição falhe a meio.
function buildMeasureScript({ exe, cwd, stderrFile, stdoutFile, args = [] }) {
  const argsLiteral = args.length ? `@(${args.map(psQuote).join(", ")})` : "@()";
  return `
$ProgressPreference = 'SilentlyContinue'
$ErrorActionPreference = 'Stop'
$exe = ${psQuote(exe)}
$wd = ${psQuote(cwd)}
$stderrFile = ${psQuote(stderrFile)}
$stdoutFile = ${psQuote(stdoutFile)}
$procArgs = ${argsLiteral}
$result = [ordered]@{ ok=$false; razao=$null; arranque_ms=$null; rss_mb=$null; private_mb=$null; cpu_pct=$null; processos=$null }
try {
  # RedirectStandardOutput e OBRIGATORIO aqui, nao so o Error: app.exe/rts.exe
  # sao binarios de subsistema CONSOLE (tem console.log antes/durante abrir a
  # janela) e, sem redirecionar, o Start-Process herda o stdout do PROPRIO
  # powershell.exe -- que e o pipe que este script Node esta a ler como a
  # SAIDA do PowerShell. Observado sem este redirect: as linhas de
  # console.log do processo filho misturadas com o JSON deste script no mesmo
  # stdout, e o JSON deixa de parsear. O Electron nunca mostrou este bug
  # (subsistema GUI, sem consola), por isso passou despercebido na medicao
  # anterior, so com dois lados.
  $startParams = @{ FilePath = $exe; WorkingDirectory = $wd; PassThru = $true; RedirectStandardError = $stderrFile; RedirectStandardOutput = $stdoutFile }
  if ($procArgs.Count -gt 0) { $startParams.ArgumentList = $procArgs }
  $proc = Start-Process @startParams
} catch {
  $result.razao = "Start-Process falhou: " + $_.Exception.Message
  $result | ConvertTo-Json -Compress
  exit 0
}
$rootId = $proc.Id
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$found = $false
while ($sw.ElapsedMilliseconds -lt ${WINDOW_TIMEOUT_MS}) {
  $alive = Get-Process -Id $rootId -ErrorAction SilentlyContinue
  if (-not $alive) { break }
  if ($alive.MainWindowHandle -ne 0) { $found = $true; break }
  $kids = Get-CimInstance Win32_Process -Filter "ParentProcessId=$rootId" -ErrorAction SilentlyContinue
  foreach ($k in $kids) {
    $kp = Get-Process -Id $k.ProcessId -ErrorAction SilentlyContinue
    if ($kp -and $kp.MainWindowHandle -ne 0) { $found = $true; break }
  }
  if ($found) { break }
  Start-Sleep -Milliseconds 50
}
$sw.Stop()
function Get-Tree($rid) {
  $all = Get-CimInstance Win32_Process
  $ids = New-Object System.Collections.Generic.HashSet[int]
  [void]$ids.Add($rid)
  $frontier = @($rid)
  while ($frontier.Count -gt 0) {
    $next = @()
    foreach ($f in $frontier) {
      foreach ($c in ($all | Where-Object { $_.ParentProcessId -eq $f })) {
        $cid = [int]$c.ProcessId
        if (-not $ids.Contains($cid)) { [void]$ids.Add($cid); $next += $cid }
      }
    }
    $frontier = $next
  }
  return ,$ids
}
if (-not $found) {
  $alive2 = Get-Process -Id $rootId -ErrorAction SilentlyContinue
  if (-not $alive2) {
    $code = 'desconhecido'
    try { if ($null -ne $proc.ExitCode) { $code = $proc.ExitCode } } catch {}
    $err = ''
    if (Test-Path $stderrFile) {
      # Get-Content -Raw, sem -Encoding, decodifica como ANSI da máquina (sem BOM);
      # o rts.exe escreve UTF-8 puro (ex.: o travessão "—" nas mensagens), e essa
      # leitura corrompia-o em bytes que por vezes quebravam o JSON abaixo.
      $err = [System.IO.File]::ReadAllText($stderrFile, [System.Text.Encoding]::UTF8)
    }
    $result.razao = "processo terminou (exit code $code) antes de abrir janela. stderr: " + ("$err".Trim())
  } else {
    $result.razao = "janela nao apareceu em ${WINDOW_TIMEOUT_MS} ms"
  }
  foreach ($tid in (Get-Tree $rootId)) { Stop-Process -Id $tid -Force -ErrorAction SilentlyContinue }
  $result | ConvertTo-Json -Compress
  exit 0
}
$result.arranque_ms = [math]::Round($sw.Elapsed.TotalMilliseconds)
try {
  Start-Sleep -Milliseconds ${POST_WINDOW_WAIT_MS}
  $treeIds = Get-Tree $rootId
  # Get-CimInstance falha às vezes de forma transitória (observado: 2 em 5
  # corridas viram só o processo principal onde deviam ver 4) — uma nova
  # tentativa curta corrige-o; nunca visto precisar de mais que uma.
  $retries = 0
  while ($treeIds.Count -le 1 -and $retries -lt 3) {
    Start-Sleep -Milliseconds 300
    $treeIds = Get-Tree $rootId
    $retries++
  }
  $procs1 = @()
  foreach ($tid in $treeIds) { $p = Get-Process -Id $tid -ErrorAction SilentlyContinue; if ($p) { $procs1 += $p } }
  $rss = ($procs1 | Measure-Object -Property WorkingSet64 -Sum).Sum
  $priv = ($procs1 | Measure-Object -Property PrivateMemorySize64 -Sum).Sum
  $cpu0ms = 0
  foreach ($p in $procs1) { $cpu0ms += $p.TotalProcessorTime.TotalMilliseconds }
  $t0 = Get-Date
  Start-Sleep -Milliseconds ${CPU_SAMPLE_MS}
  $treeIds2 = Get-Tree $rootId
  if ($treeIds2.Count -le 1 -and $treeIds.Count -gt 1) { $treeIds2 = $treeIds } # a árvore não encolhe sozinha nesta janela de 2s; usa a última leitura boa
  $procs2 = @()
  foreach ($tid in $treeIds2) { $p = Get-Process -Id $tid -ErrorAction SilentlyContinue; if ($p) { $procs2 += $p } }
  $cpu1ms = 0
  foreach ($p in $procs2) { $cpu1ms += $p.TotalProcessorTime.TotalMilliseconds }
  $elapsedMs = ((Get-Date) - $t0).TotalMilliseconds
  $cpuPct = 0
  if ($elapsedMs -gt 0) { $cpuPct = [math]::Round((($cpu1ms - $cpu0ms) / $elapsedMs) * 100, 2) }
  $result.ok = $true
  $result.rss_mb = [math]::Round($rss / 1MB, 2)
  $result.private_mb = [math]::Round($priv / 1MB, 2)
  $result.cpu_pct = $cpuPct
  $result.processos = $treeIds2.Count
} finally {
  foreach ($tid in (Get-Tree $rootId)) { Stop-Process -Id $tid -Force -ErrorAction SilentlyContinue }
}
$result | ConvertTo-Json -Compress
`;
}

function measureOnce(exe, cwd, idx, args = []) {
  mkdirSync(MED_DIR, { recursive: true });
  const stamp = `${Date.now()}_${idx}`;
  const stderrFile = join(MED_DIR, `stderr_${stamp}.txt`);
  const stdoutFile = join(MED_DIR, `stdout_${stamp}.txt`); // nunca lido de volta — só para não corromper o JSON do PowerShell, ver buildMeasureScript
  const script = buildMeasureScript({ exe, cwd, stderrFile, stdoutFile, args });
  const b64 = Buffer.from(script, "utf16le").toString("base64");
  const r = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", b64],
    { encoding: "utf8", timeout: WINDOW_TIMEOUT_MS + POST_WINDOW_WAIT_MS + CPU_SAMPLE_MS + 15000 },
  );
  const out = (r.stdout || "").trim();
  let parsed;
  try {
    parsed = JSON.parse(out);
  } catch {
    parsed = { ok: false, razao: `saída do PowerShell não era JSON: ${out.slice(0, 300)} | stderr: ${(r.stderr || "").slice(0, 300)}` };
  }
  parsed._stderrFile = stderrFile; // lido depois por lerFalhaJsDaPagina, nunca gravado no JSON final
  return parsed;
}

// Junta rts.exe + a MESMA página (copiada de APP_HTML, nunca duplicada à
// mão) + o `.ts` que a abre numa pasta própria — como os outros dois lados já
// têm a sua. Necessário (não só arrumação) porque
// `examples/claude-react-janela.ts` lê `"react-app.html"` RELATIVO AO CWD:
// correr `rts.exe` direto de `target/release` sem a página ao lado falharia
// a ler o ficheiro. Também dá o tamanho de "pasta" certo deste lado: o
// binário do MOTOR + a página + o `.ts`, o equivalente ao par
// Chromium+app.asar do Electron.
function prepararLadoJit() {
  const srcExe = process.env.RTS_VS_ELECTRON_RTS_EXE_JIT || join(REPO_ROOT, "target", "release", "rts.exe");
  const srcTs = join(REPO_ROOT, "examples", "claude-react-janela.ts");
  const dir = join(TEMP_ROOT, "jit");
  mkdirSync(dir, { recursive: true });
  const destExe = join(dir, "rts.exe");
  const destHtml = join(dir, "react-app.html");
  const destTs = join(dir, "claude-react-janela.ts");
  for (const [src, dest] of [[srcExe, destExe], [APP_HTML, destHtml], [srcTs, destTs]]) {
    if (existsSync(src)) {
      try { copyFileSync(src, dest); } catch { /* falha aqui vira "ficheiro não existe" em medirLado abaixo */ }
    }
  }
  return destExe;
}

// Os três lados. `rts_aot` e `rts_jit` não fixam `js_da_pagina` aqui — é
// determinado por lado a partir do stderr real de uma corrida (ver
// `medirLado`), para o número vir da medição e não de uma etiqueta.
function buildSides() {
  return {
    electron: {
      label: "Electron",
      exe: process.env.RTS_VS_ELECTRON_ELECTRON_EXE ||
        join(TEMP_ROOT, "electron", "dist", "rts-vs-electron-win32-x64", "rts-vs-electron.exe"),
      args: [],
      js_da_pagina: true, // Chromium real: nunca falha a compilar JS
    },
    rts_aot: {
      label: "RTS .exe AOT",
      exe: process.env.RTS_VS_ELECTRON_RTS_EXE || join(TEMP_ROOT, "rts", "app.exe"),
      args: [],
    },
    rts_jit: {
      label: "RTS rts.exe + app",
      exe: prepararLadoJit(),
      args: ["run", "claude-react-janela.ts"],
      js_da_pagina: true, // motor com compilador: corre os <script> da página
    },
  };
}

function medirLado(key, cfg) {
  console.log(`\n== ${cfg.label} ==`);
  if (!existsSync(cfg.exe)) {
    return { exe: cfg.exe, bytes_exe: null, bytes_pasta: null, ficheiros_na_pasta: null,
      nao_construido: true, razao: `ficheiro não existe: ${cfg.exe}`,
      arranque_ms: null, rss_mb: null, private_mb: null, cpu_repouso_pct: null, processos: null,
      js_da_pagina: cfg.js_da_pagina ?? null, razao_js_da_pagina: null,
      amostras: { n: N, ok: 0 } };
  }
  const bytes_exe = statSync(cfg.exe).size;
  const pasta = dirname(cfg.exe);
  const { bytes_pasta, ficheiros_na_pasta } = folderStats(pasta);

  const runs = [];
  for (let i = 1; i <= N; i++) {
    process.stdout.write(`  corrida ${i}/${N}... `);
    const res = measureOnce(cfg.exe, pasta, i, cfg.args || []);
    runs.push(res);
    console.log(res.ok ? `ok (${res.arranque_ms}ms, ${res.rss_mb}MB RSS)` : `falhou (${res.razao})`);
  }

  // js_da_pagina: fixo em SIDES para Electron/JIT; para o AOT, procurado no
  // stderr real (corridas OK primeiro — é nelas que o processo viveu tempo
  // suficiente para os <script> falharem e escreverem a mensagem; as
  // falhadas já têm a SUA própria razão, capturada acima).
  let js_da_pagina = cfg.js_da_pagina;
  let razao_js_da_pagina = null;
  if (js_da_pagina === undefined) {
    const oksFirst = [...runs.filter((r) => r.ok), ...runs.filter((r) => !r.ok)];
    for (const r of oksFirst) {
      const achado = lerFalhaJsDaPagina(r._stderrFile);
      if (achado) { js_da_pagina = false; razao_js_da_pagina = achado; break; }
    }
    if (js_da_pagina === undefined) js_da_pagina = null; // stderr não tinha nem sucesso nem a falha conhecida
  }

  const oks = runs.filter((r) => r.ok);
  if (oks.length === 0) {
    return { exe: cfg.exe, bytes_exe, bytes_pasta, ficheiros_na_pasta,
      nao_construido: true, razao: runs[0]?.razao ?? "todas as corridas falharam sem razão reportada",
      arranque_ms: null, rss_mb: null, private_mb: null, cpu_repouso_pct: null, processos: null,
      js_da_pagina, razao_js_da_pagina,
      amostras: { n: N, ok: 0 } };
  }
  const result = {
    exe: cfg.exe, bytes_exe, bytes_pasta, ficheiros_na_pasta,
    nao_construido: false, razao: null,
    arranque_ms: agregaMinMedMax(oks.map((r) => r.arranque_ms), 0),
    rss_mb: agregaMinMedMax(oks.map((r) => r.rss_mb), 2),
    private_mb: agregaMinMedMax(oks.map((r) => r.private_mb), 2).mediana,
    cpu_repouso_pct: agregaMinMedMax(oks.map((r) => r.cpu_pct), 2).mediana,
    processos: Math.round(mediana(oks.map((r) => r.processos))),
    js_da_pagina, razao_js_da_pagina,
    amostras: { n: N, ok: oks.length },
  };
  return result;
}

function versaoElectron(exeLado) {
  // O ficheiro `version` na raiz da pasta empacotada sobrevive mesmo que
  // node_modules seja limpo mais tarde — mais fiável do que reler package.json.
  const vfile = join(dirname(exeLado), "version");
  if (existsSync(vfile)) { try { return readFileSync(vfile, "utf8").trim(); } catch { /* ignore */ } }
  return "desconhecida";
}

function maquinaInfo() {
  const c = cpus();
  return {
    so: `${platform()} ${release()}`,
    cpu: c[0]?.model?.trim() ?? "desconhecido",
    nucleos_logicos: c.length,
    ram_gb: Math.round((totalmem() / 1024 ** 3) * 10) / 10,
  };
}

function fmtBytes(b) { return b == null ? "—" : `${(b / 1024 ** 2).toFixed(1)} MB`; }
function fmtMB(mb) { return mb == null ? "—" : `${mb.toFixed(1)} MB`; }
function fmtRange(obj, unidade) {
  if (!obj) return "—";
  return `${obj.mediana}${unidade} (${obj.min}–${obj.max})`;
}
function fmtJsDaPagina(lado) {
  if (lado.nao_construido) return "—";
  if (lado.js_da_pagina === true) return "sim";
  if (lado.js_da_pagina === false) return "NÃO";
  return "?";
}

const ORDEM = [["electron", "Electron"], ["rts_aot", "RTS .exe AOT"], ["rts_jit", "RTS rts.exe+app"]];

function imprimeTabela(json) {
  const L = json.lados;
  const linhas = [
    ["exe", ...ORDEM.map(([k]) => fmtBytes(L[k].bytes_exe))],
    ["pasta", ...ORDEM.map(([k]) => fmtBytes(L[k].bytes_pasta))],
    ["ficheiros", ...ORDEM.map(([k]) => L[k].ficheiros_na_pasta ?? "—")],
    ["JS da página", ...ORDEM.map(([k]) => fmtJsDaPagina(L[k]))],
    ["processos", ...ORDEM.map(([k]) => (L[k].nao_construido ? "—" : L[k].processos))],
    ["arranque", ...ORDEM.map(([k]) => (L[k].nao_construido ? "não construído" : fmtRange(L[k].arranque_ms, "ms")))],
    ["RSS", ...ORDEM.map(([k]) => (L[k].nao_construido ? "—" : fmtRange(L[k].rss_mb, "MB")))],
    ["private", ...ORDEM.map(([k]) => (L[k].nao_construido ? "—" : fmtMB(L[k].private_mb)))],
    ["CPU repouso", ...ORDEM.map(([k]) => (L[k].nao_construido ? "—" : `${L[k].cpu_repouso_pct}%`))],
  ];
  const headers = ["métrica", ...ORDEM.map(([, label]) => label)];
  const widths = headers.map((h, i) => Math.max(h.length, ...linhas.map((l) => String(l[i]).length)));
  const pad = (s, w) => String(s).padEnd(w);
  console.log(`\n${headers.map((h, i) => pad(h, widths[i])).join(" | ")}`);
  console.log(widths.map((w) => "-".repeat(w)).join("-|-"));
  for (const l of linhas) console.log(l.map((c, i) => pad(c, widths[i])).join(" | "));
  for (const [k, label] of ORDEM) {
    if (L[k].nao_construido) console.log(`\n${label} não construído: ${L[k].razao}`);
    else if (L[k].js_da_pagina === false) console.log(`\n${label}: JS da página NÃO corre — ${L[k].razao_js_da_pagina}`);
  }
}

function main() {
  const sides = buildSides();
  const lados = {};
  for (const [key, cfg] of Object.entries(sides)) {
    lados[key] = medirLado(key, cfg);
  }
  lados.electron.versao = versaoElectron(sides.electron.exe);

  const json = {
    medido_em: new Date().toISOString(),
    maquina: maquinaInfo(),
    // UMA fonte para os três: os dois lados empacotados (Electron, RTS AOT)
    // abrem-na diretamente; o lado JIT lê uma cópia dela sob o nome
    // `react-app.html` (ver `prepararLadoJit`) — mesmo ficheiro, dois nomes.
    app: "scripts/rts_vs_electron/app/index.html",
    lados,
  };
  mkdirSync(dirname(OUT_JSON), { recursive: true });
  writeFileSync(OUT_JSON, JSON.stringify(json, null, 2) + "\n");
  imprimeTabela(json);
  console.log(`\nGravado em ${OUT_JSON}`);
}

main();
