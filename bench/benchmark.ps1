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
  @{ id = "pi_bigfloat";          rts = "bench\pi_bigfloat.ts";              js = "bench\pi_bigfloat.js";                 jsRunners = @("bun","node","deno") }
  @{ id = "monte_carlo_threaded"; rts = "bench\monte_carlo_pi_threaded.ts";  js = "bench\monte_carlo_pi_threaded_bun.ts"; jsRunners = @("bun") }
  @{ id = "pi_machin";            rts = "bench\pi_machin.ts";                js = "";                                     jsRunners = @() }
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
  & $RtsExe compile -p $b.rts $out --production | Out-Host
  $exe = "$out.exe"
  if (-not (Test-Path $exe)) {
    # Skip (with a loud warning) instead of aborting the whole suite: a bench
    # source hitting an engine gap must not hide every other result.
    Write-Warning "AOT output missing for $($b.rts) - bench '$($b.id)' SKIPPED"
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
