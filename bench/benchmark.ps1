param(
  [string]$RtsExe = "target\release\rts.exe",
  [int]$Runs = 20,
  [int]$Warmup = 3,
  [string]$JsonOut = ""
)

$ErrorActionPreference = "Stop"

# Working directory = raiz do projeto
Set-Location (Split-Path -Parent $PSScriptRoot)

if (-not (Test-Path $RtsExe)) {
  throw "RTS binary not found at $RtsExe - rode 'cargo build --release' antes."
}

# O AOT liga contra o archive de `rts-runtime`. Falhar aqui, com o comando, em
# vez de deixar cada `rts compile` falhar em sequencia e o script reportar treze
# benches "SKIPPED" por uma causa que nao esta em nenhum dos logs.
#
# O nome carregava um sufixo `-rwk`, de quando o crate do motor novo estava ao
# lado do antigo e o cargo nao aceita dois pacotes com um nome. O sufixo saiu a
# 2026-08-10 com o motor antigo; este script nao saiu com ele, e passou a
# procurar um pacote que nao existe. O workflow Benchmarks falhava em 35s em
# TODAS as corridas desde entao, sempre nesta linha.
$RuntimeArchive = "target\release\rts_runtime.lib"
if (-not (Test-Path $RuntimeArchive)) {
  throw "AOT runtime archive nao encontrado em $RuntimeArchive - rode 'cargo build --release -p rts-runtime' antes."
}

# -------------------------------------------------------------------
# Helpers
# -------------------------------------------------------------------
function Measure-OneRunMs([scriptblock]$Action) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  & $Action
  $sw.Stop()
  return $sw.Elapsed.TotalMilliseconds
}

function Measure-Suite([string]$Label, [scriptblock]$Action, [int]$Warm, [int]$TotalRuns) {
  Write-Host "  warmup $Label ($Warm)..."
  for ($i = 0; $i -lt $Warm; $i++) { & $Action *> $null }
  Write-Host "  bench  $Label ($TotalRuns)..."
  $results = New-Object System.Collections.Generic.List[double]
  for ($i = 0; $i -lt $TotalRuns; $i++) {
    $results.Add((Measure-OneRunMs $Action))
  }
  return $results
}

function Get-Stats([System.Collections.Generic.List[double]]$Values) {
  $sorted = $Values | Sort-Object
  $count = $sorted.Count
  $sum = ($sorted | Measure-Object -Sum).Sum
  $avg = $sum / $count
  if ($count % 2 -eq 0) {
    $median = ($sorted[($count / 2) - 1] + $sorted[$count / 2]) / 2
  } else {
    $median = $sorted[[int]($count / 2)]
  }
  $p95Index = [Math]::Min($count - 1, [Math]::Ceiling(($count - 1) * 0.95))
  return [PSCustomObject]@{
    count     = $count
    mean_ms   = [Math]::Round($avg, 3)
    median_ms = [Math]::Round($median, 3)
    p95_ms    = [Math]::Round($sorted[[int]$p95Index], 3)
    min_ms    = [Math]::Round($sorted[0], 3)
    max_ms    = [Math]::Round($sorted[$count - 1], 3)
  }
}

function Have-Cmd([string]$Name) {
  $null = Get-Command $Name -ErrorAction SilentlyContinue
  return $?
}

# -------------------------------------------------------------------
# Matriz de benches: id|rts_src|js_src (js_src vazio = so' RTS)
# -------------------------------------------------------------------
$Benches = @(
  @{ id = "simple";               rts = "bench\rts_simple.ts";               js = "bench\bun_simple.ts";                  jsRunners = @("bun","node","deno") }
  @{ id = "monte_carlo";          rts = "bench\monte_carlo_pi.ts";           js = "bench\monte_carlo_pi.js";              jsRunners = @("bun","node","deno") }
  @{ id = "monte_carlo_jsrand";   rts = "bench\monte_carlo_pi.ts";           js = "bench\monte_carlo_pi_native_rand.js";  jsRunners = @("bun","node","deno") }
  @{ id = "pi_machin";            rts = "bench\pi_machin.ts";                js = "";                                     jsRunners = @() }
  # RTS-only, and each measures something a wrong answer would hide rather than a
  # rate: `objbench` allocates three million objects, which is what the collector
  # exists for; `objbench_noalloc` is the same loop without allocating, so the
  # difference between them is the allocation; `field_access` reads two fields
  # from classes of 2, 5, 10 and 20 so a cost that GROWS with the field count
  # would show; `string_index` doubles its input four times, so a quadratic
  # access shows as a quadrupling.
  @{ id = "objbench";             rts = "bench\objbench.ts";                 js = "";                                     jsRunners = @() }
  @{ id = "objbench_methods";     rts = "bench\objbench_methods.ts";         js = "";                                     jsRunners = @() }
  @{ id = "objbench_noalloc";     rts = "bench\objbench_noalloc.ts";         js = "";                                     jsRunners = @() }
  @{ id = "field_access";         rts = "bench\field_access.ts";             js = "";                                     jsRunners = @() }
  @{ id = "string_index";         rts = "bench\string_index.ts";             js = "";                                     jsRunners = @() }
  # `property_access` mede o MESMO laco com o estado em quatro sitios, entao a
  # diferenca entre duas das suas linhas e' o custo de um sitio e nao de um
  # programa. Estava em falta: a 2026-08-20 o `monte_carlo_pi` levava 929 ms
  # onde o mesmo algoritmo com locais levava 134, e nada aqui media essa
  # diferenca.
  # Corre tambem sob os outros: a primeira leitura deste bench foi comparada so'
  # contra o Node e concluiu que nao havia nada a ganhar, o que era falso por
  # escolha de denominador — contra o Bun ha' ~28%. Um bench cuja conclusao
  # depende de qual referencia se escolhe tem de correr contra as duas.
  # O `.js` e' o gemeo do `.ts`, nao um segundo programa: o Node do workflow
  # esta fixo na v20 e nao le TypeScript, e como esta funcao manda a saida dos
  # runners para `$null`, um Node que falha em milissegundos seria medido como
  # o runtime MAIS RAPIDO da tabela em vez de aparecer como falha.
  @{ id = "property_access";      rts = "bench\property_access.ts";          js = "bench\property_access.js";             jsRunners = @("bun","node","deno") }
)

# -------------------------------------------------------------------
# Pre-compila AOT de cada source RTS uma vez
# -------------------------------------------------------------------
$AotBin = @{}
New-Item -ItemType Directory -Force -Path "target\bench" | Out-Null
$Skipped = @()
foreach ($b in $Benches) {
  $key = ($b.rts -replace '[\\/]', '_') -replace '\.ts$', ''
  $out = "target\bench\$key"
  Write-Host "compiling AOT: $($b.rts) -> $out"
  # A bench source hitting an engine gap (e.g. 'simple' reads a module-global
  # inside a function, #195) makes `rts compile` exit non-zero. Under pwsh 7.4+
  # with the runner's `$ErrorActionPreference = 'Stop'`, a native non-zero exit
  # is a TERMINATING error that aborts the whole benchmark job BEFORE the
  # Test-Path skip below could handle it. Disable that native-error escalation
  # around the compile so a failed bench is warned-and-skipped, not fatal.
  $prevNative = $null
  try { $prevNative = $PSNativeCommandUseErrorActionPreference; $PSNativeCommandUseErrorActionPreference = $false } catch {}
  # Redirect ALL streams to a log (no `| Out-Host` pipe): under PowerShell a
  # native command's stderr through a pipe is wrapped as a terminating
  # NativeCommandError, which would abort the job regardless of the exit-code
  # preference above. Writing to a file sidesteps that; the log is echoed only
  # if the compile failed, so a bad bench is diagnosable without being fatal.
  & $RtsExe compile -p $b.rts $out --production *> "$out.compile.log"
  try { if ($null -ne $prevNative) { $PSNativeCommandUseErrorActionPreference = $prevNative } } catch {}
  $exe = "$out.exe"
  if (-not (Test-Path $exe)) {
    # Skip (with a loud warning) instead of aborting the whole suite: a bench
    # source hitting an engine gap must not hide every other result.
    Write-Warning "AOT output missing for $($b.rts) - bench '$($b.id)' SKIPPED"
    if (Test-Path "$out.compile.log") { Get-Content "$out.compile.log" | Select-Object -First 3 | ForEach-Object { Write-Host "  | $_" } }
    $Skipped += $b.id
    continue
  }
  $AotBin[$b.rts] = $exe
}
if ($Skipped.Count -gt 0) {
  $Benches = @($Benches | Where-Object { $Skipped -notcontains $_.id })
}

# -------------------------------------------------------------------
# Runners disponiveis
# -------------------------------------------------------------------
$HaveBun  = Have-Cmd "bun"
$HaveNode = Have-Cmd "node"
$HaveDeno = Have-Cmd "deno"

# -------------------------------------------------------------------
# Roda
# -------------------------------------------------------------------
$benchResults = @()
foreach ($b in $Benches) {
  Write-Host "=== bench: $($b.id) ==="
  $runEntries = @()

  $stats = Get-Stats (Measure-Suite "RTS JIT [$($b.id)]" { & $RtsExe run $b.rts *> $null } $Warmup $Runs)
  $runEntries += [PSCustomObject]@{ runner = "rts_jit"; source = $b.rts; stats = $stats }

  $compiled = $AotBin[$b.rts]
  $stats = Get-Stats (Measure-Suite "RTS AOT [$($b.id)]" { & $compiled *> $null } $Warmup $Runs)
  $runEntries += [PSCustomObject]@{ runner = "rts_aot"; source = $b.rts; stats = $stats }

  if ($b.js -and $b.jsRunners.Count -gt 0) {
    if ($HaveBun  -and $b.jsRunners -contains "bun") {
      $stats = Get-Stats (Measure-Suite "Bun  [$($b.id)]" { bun run $b.js *> $null } $Warmup $Runs)
      $runEntries += [PSCustomObject]@{ runner = "bun"; source = $b.js; stats = $stats }
    }
    if ($HaveNode -and $b.jsRunners -contains "node") {
      $stats = Get-Stats (Measure-Suite "Node [$($b.id)]" { node $b.js *> $null } $Warmup $Runs)
      $runEntries += [PSCustomObject]@{ runner = "node"; source = $b.js; stats = $stats }
    }
    if ($HaveDeno -and $b.jsRunners -contains "deno") {
      $stats = Get-Stats (Measure-Suite "Deno [$($b.id)]" { deno run --quiet --allow-all $b.js *> $null } $Warmup $Runs)
      $runEntries += [PSCustomObject]@{ runner = "deno"; source = $b.js; stats = $stats }
    }
  }

  $benchResults += [PSCustomObject]@{ id = $b.id; runs = $runEntries }
}

# -------------------------------------------------------------------
# Meta + JSON
# -------------------------------------------------------------------
$sha = if ($env:GITHUB_SHA) { $env:GITHUB_SHA } else { (git rev-parse HEAD 2>$null) }
if (-not $sha) { $sha = "unknown" }
$shortSha = if ($sha.Length -ge 7) { $sha.Substring(0, 7) } else { $sha }
$rtsVersion = "unknown"
try {
  $v = (& $RtsExe --version 2>$null) -join " "
  if ($LASTEXITCODE -eq 0 -and $v) { $rtsVersion = $v }
} catch {}

$report = [PSCustomObject]@{
  meta = [PSCustomObject]@{
    sha         = $sha
    short_sha   = $shortSha
    created     = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    os          = "Windows"
    arch        = $env:PROCESSOR_ARCHITECTURE
    runs        = $Runs
    warmup      = $Warmup
    run_id      = if ($env:GITHUB_RUN_ID) { $env:GITHUB_RUN_ID } else { "local" }
    run_number  = if ($env:GITHUB_RUN_NUMBER) { $env:GITHUB_RUN_NUMBER } else { "0" }
    rts_version = $rtsVersion
  }
  benches = $benchResults
  skipped = $Skipped
}

$json = $report | ConvertTo-Json -Depth 10
Write-Host ""
Write-Host "=== JSON ==="
Write-Host $json

if ($JsonOut) {
  $dir = Split-Path -Parent $JsonOut
  if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
  $json | Out-File -FilePath $JsonOut -Encoding utf8
  Write-Host "wrote $JsonOut"
}

# Reaching here means the run completed and the JSON was written — that is
# success. Exit 0 explicitly so a leftover `$LASTEXITCODE` from the last native
# command (e.g. a bench deliberately SKIPPED for an engine gap, whose failed
# `rts compile` set it to 1) does not fail the whole job. A real measurement
# failure would have surfaced as a terminating error before this point.
exit 0
