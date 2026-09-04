// Mede arranque/memória/CPU/tamanho dos dois artefactos empacotados da MESMA
// app React (examples/react-app.html) — um `.exe` Electron e um `.exe` AOT do
// RTS — e grava .github/rts_vs_electron.json (histórico legível por máquina,
// no mesmo espírito do css_parity_report.json: o número fica no ficheiro
// gerado, não escrito à mão).
//
//   node scripts/rts_vs_electron/medir.mjs
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
import { existsSync, readdirSync, statSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { cpus, totalmem, platform, release, tmpdir } from "node:os";
import { spawnSync } from "node:child_process";

// N por variável de ambiente: localmente 5 chega (poucos minutos), mas o job
// de CI (.github/workflows/build-artifacts.yml, `rts-vs-electron`) empacota o
// Electron do zero a cada corrida — 3 chega para um número estável dentro do
// orçamento de tempo do runner, e o default local fica intocado.
const N = Number(process.env.RTS_VS_ELECTRON_N) || 5;
const WINDOW_TIMEOUT_MS = 15000; // generoso: a Electron abre em <1s, o RTS falha em <1s hoje
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

// Caminhos dos dois artefactos, com override por env var para quando o lado
// RTS voltar a compilar (ver scripts/rts_vs_electron/rts/README.md) sem
// precisar editar este ficheiro.
const SIDES = {
  rts: {
    label: "RTS",
    exe: process.env.RTS_VS_ELECTRON_RTS_EXE || join(TEMP_ROOT, "rts", "app.exe"),
  },
  electron: {
    label: "Electron",
    exe: process.env.RTS_VS_ELECTRON_ELECTRON_EXE ||
      join(TEMP_ROOT, "electron", "dist", "rts-vs-electron-win32-x64", "rts-vs-electron.exe"),
  },
};

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

// O script PowerShell de UMA corrida: arranca, espera a janela (processo ou
// filho direto — no Electron é sempre o principal), espera 4s, soma
// RSS/private de TODA a árvore (recursivo por ParentProcessId — a Electron
// tem main+gpu+utility+renderer), amostra CPU 2s, mata a árvore SEMPRE
// (try/finally) mesmo que a medição falhe a meio.
function buildMeasureScript({ exe, cwd, stderrFile }) {
  return `
$ProgressPreference = 'SilentlyContinue'
$ErrorActionPreference = 'Stop'
$exe = ${psQuote(exe)}
$wd = ${psQuote(cwd)}
$stderrFile = ${psQuote(stderrFile)}
$result = [ordered]@{ ok=$false; razao=$null; arranque_ms=$null; rss_mb=$null; private_mb=$null; cpu_pct=$null; processos=$null }
try {
  $proc = Start-Process -FilePath $exe -WorkingDirectory $wd -PassThru -RedirectStandardError $stderrFile
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

function measureOnce(exe, cwd, idx) {
  mkdirSync(MED_DIR, { recursive: true });
  const stderrFile = join(MED_DIR, `stderr_${Date.now()}_${idx}.txt`);
  const script = buildMeasureScript({ exe, cwd, stderrFile });
  const b64 = Buffer.from(script, "utf16le").toString("base64");
  const r = spawnSync(
    "powershell.exe",
    ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", b64],
    { encoding: "utf8", timeout: WINDOW_TIMEOUT_MS + POST_WINDOW_WAIT_MS + CPU_SAMPLE_MS + 15000 },
  );
  const out = (r.stdout || "").trim();
  try {
    return JSON.parse(out);
  } catch {
    return { ok: false, razao: `saída do PowerShell não era JSON: ${out.slice(0, 300)} | stderr: ${(r.stderr || "").slice(0, 300)}` };
  }
}

function medirLado(key, cfg) {
  console.log(`\n== ${cfg.label} ==`);
  if (!existsSync(cfg.exe)) {
    return { exe: cfg.exe, bytes_exe: null, bytes_pasta: null, ficheiros_na_pasta: null,
      nao_construido: true, razao: `ficheiro não existe: ${cfg.exe}`,
      arranque_ms: null, rss_mb: null, private_mb: null, cpu_repouso_pct: null, processos: null,
      amostras: { n: N, ok: 0 } };
  }
  const bytes_exe = statSync(cfg.exe).size;
  const pasta = dirname(cfg.exe);
  const { bytes_pasta, ficheiros_na_pasta } = folderStats(pasta);

  const runs = [];
  for (let i = 1; i <= N; i++) {
    process.stdout.write(`  corrida ${i}/${N}... `);
    const res = measureOnce(cfg.exe, pasta, i);
    runs.push(res);
    console.log(res.ok ? `ok (${res.arranque_ms}ms, ${res.rss_mb}MB RSS)` : `falhou (${res.razao})`);
  }
  const oks = runs.filter((r) => r.ok);
  if (oks.length === 0) {
    return { exe: cfg.exe, bytes_exe, bytes_pasta, ficheiros_na_pasta,
      nao_construido: true, razao: runs[0]?.razao ?? "todas as corridas falharam sem razão reportada",
      arranque_ms: null, rss_mb: null, private_mb: null, cpu_repouso_pct: null, processos: null,
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

function fmtMB(mb) { return mb == null ? "—" : `${mb.toFixed(1)} MB`; }
function fmtRange(obj, unidade) {
  if (!obj) return "—";
  return `${obj.mediana}${unidade} (${obj.min}–${obj.max})`;
}

function imprimeTabela(json) {
  const { rts, electron } = json.lados;
  const linhas = [
    ["exe", rts.bytes_exe != null ? `${(rts.bytes_exe / 1024 ** 2).toFixed(1)} MB` : "—",
      electron.bytes_exe != null ? `${(electron.bytes_exe / 1024 ** 2).toFixed(1)} MB` : "—"],
    ["pasta", rts.bytes_pasta != null ? `${(rts.bytes_pasta / 1024 ** 2).toFixed(1)} MB` : "—",
      electron.bytes_pasta != null ? `${(electron.bytes_pasta / 1024 ** 2).toFixed(1)} MB` : "—"],
    ["ficheiros", rts.ficheiros_na_pasta ?? "—", electron.ficheiros_na_pasta ?? "—"],
    ["processos", rts.nao_construido ? "—" : rts.processos, electron.nao_construido ? "—" : electron.processos],
    ["arranque", rts.nao_construido ? "não construído" : fmtRange(rts.arranque_ms, "ms"),
      electron.nao_construido ? "não construído" : fmtRange(electron.arranque_ms, "ms")],
    ["RSS", rts.nao_construido ? "—" : fmtRange(rts.rss_mb, "MB"), electron.nao_construido ? "—" : fmtRange(electron.rss_mb, "MB")],
    ["private", rts.nao_construido ? "—" : fmtMB(rts.private_mb), electron.nao_construido ? "—" : fmtMB(electron.private_mb)],
    ["CPU repouso", rts.nao_construido ? "—" : `${rts.cpu_repouso_pct}%`, electron.nao_construido ? "—" : `${electron.cpu_repouso_pct}%`],
  ];
  const w0 = Math.max(...linhas.map((l) => l[0].length), "métrica".length);
  const w1 = Math.max(...linhas.map((l) => String(l[1]).length), "RTS".length);
  const w2 = Math.max(...linhas.map((l) => String(l[2]).length), "Electron".length);
  const pad = (s, w) => String(s).padEnd(w);
  console.log(`\n${pad("métrica", w0)} | ${pad("RTS", w1)} | ${pad("Electron", w2)}`);
  console.log(`${"-".repeat(w0)}-|-${"-".repeat(w1)}-|-${"-".repeat(w2)}`);
  for (const [k, a, b] of linhas) console.log(`${pad(k, w0)} | ${pad(a, w1)} | ${pad(b, w2)}`);
  if (rts.nao_construido) console.log(`\nRTS não construído: ${rts.razao}`);
  if (electron.nao_construido) console.log(`\nElectron não construído: ${electron.razao}`);
}

function main() {
  const ladoRts = medirLado("rts", SIDES.rts);
  const ladoElectron = medirLado("electron", SIDES.electron);
  ladoElectron.versao = versaoElectron(SIDES.electron.exe);

  const json = {
    medido_em: new Date().toISOString(),
    maquina: maquinaInfo(),
    app: "examples/react-app.html",
    lados: { rts: ladoRts, electron: ladoElectron },
  };
  mkdirSync(dirname(OUT_JSON), { recursive: true });
  writeFileSync(OUT_JSON, JSON.stringify(json, null, 2) + "\n");
  imprimeTabela(json);
  console.log(`\nGravado em ${OUT_JSON}`);
}

main();
