param(
  [string]$RtsExe = "target\release\rts.exe",  # Caminho para o executável do RTS
  [string]$SourceFile = "bench\rts_simple.ts",        # Código TypeScript comum
  [string]$BuildOutput = "target\rts_app",                 # Nome base do binário compilado
  [int]$Runs = 40,
  [int]$Warmup = 5
)

$ErrorActionPreference = "Stop"

# Ajusta o diretório de trabalho para a raiz do projeto
Set-Location (Split-Path -Parent $PSScriptRoot)

# Verifica se o executável do RTS existe
cargo build --release

# -------------------------------------------------------------------
# Prepara o binário compilado (uma única vez antes dos benchmarks)
# -------------------------------------------------------------------
Write-Host "=== Building standalone executable with RTS ==="
& $RtsExe compile -p $SourceFile $BuildOutput --production
$CompiledExe = "$BuildOutput.exe"
if (!(Test-Path $CompiledExe)) {
  throw "Compiled executable not found at $CompiledExe"
}
Write-Host "Build completed: $CompiledExe`n"

# -------------------------------------------------------------------
# Funções de medição
# -------------------------------------------------------------------
function Measure-OneRunMs([scriptblock]$Action) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  & $Action
  $sw.Stop()
  return $sw.Elapsed.TotalMilliseconds
}

function Measure-Suite([string]$Label, [scriptblock]$Action, [int]$Warm, [int]$TotalRuns) {
  Write-Host "Warmup $Label ($Warm runs)..."
  for ($i = 0; $i -lt $Warm; $i++) {
    & $Action *> $null
  }

  Write-Host "Benchmark $Label ($TotalRuns runs)..."
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
    $midLeft = $sorted[($count / 2) - 1]
    $midRight = $sorted[$count / 2]
    $median = ($midLeft + $midRight) / 2
  } else {
    $median = $sorted[[int]($count / 2)]
  }

  $p95Index = [Math]::Ceiling(($count - 1) * 0.95)
  $p95 = $sorted[[int]$p95Index]

  return [PSCustomObject]@{
    count = $count
    mean_ms = [Math]::Round($avg, 3)
    median_ms = [Math]::Round($median, 3)
    p95_ms = [Math]::Round($p95, 3)
    min_ms = [Math]::Round($sorted[0], 3)
    max_ms = [Math]::Round($sorted[$count - 1], 3)
  }
}

# -------------------------------------------------------------------
# Definição das ações de benchmark
# -------------------------------------------------------------------

# 1. RTS Runtime AOT (escreve .o, linka, executa o binário)
$rtsRunAction = {
  $env:RTS_JIT = ""
  & $RtsExe run $SourceFile *> $null
}

# 2. RTS Runtime JIT (compila direto para memória executável — sem
#    disco e sem linker externo).
$rtsJitAction = {
  $env:RTS_JIT = "1"
  & $RtsExe run $SourceFile *> $null
  $env:RTS_JIT = ""
}

# 3. RTS Compiled (executa o binário gerado)
$rtsCompiledAction = { & $CompiledExe *> $null }

# 4. Bun Runtime
$bunAction = { bun run "bench\bun_simple.ts" *> $null }

# 5. Node runtime
$NodeAction = { node "bench\bun_simple.ts" *> $null }

# -------------------------------------------------------------------
# Execução dos benchmarks
# -------------------------------------------------------------------
$rtsRunResults      = Measure-Suite "RTS (run, AOT)"     $rtsRunAction      $Warmup $Runs
$rtsJitResults      = Measure-Suite "RTS (run, JIT)"     $rtsJitAction      $Warmup $Runs
$rtsCompiledResults = Measure-Suite "RTS (compiled)"     $rtsCompiledAction $Warmup $Runs
$bunResults         = Measure-Suite "Bun (run)"          $bunAction         $Warmup $Runs
$NodeResults        = Measure-Suite "Node (run)"         $NodeAction        $Warmup $Runs

# Estatísticas
$rtsRunStats      = Get-Stats $rtsRunResults
$rtsJitStats      = Get-Stats $rtsJitResults
$rtsCompiledStats = Get-Stats $rtsCompiledResults
$bunStats         = Get-Stats $bunResults
$NodeStats        = Get-Stats $NodeResults

# -------------------------------------------------------------------
# Exibição dos resultados
# -------------------------------------------------------------------
Write-Host ""
Write-Host "=== Benchmark Summary (ms) ==="
$summary = @()
$summary += [PSCustomObject]@{
  runner    = "RTS (run, AOT)"
  mean_ms   = $rtsRunStats.mean_ms
  median_ms = $rtsRunStats.median_ms
  p95_ms    = $rtsRunStats.p95_ms
  min_ms    = $rtsRunStats.min_ms
  max_ms    = $rtsRunStats.max_ms
}
$summary += [PSCustomObject]@{
  runner    = "RTS (run, JIT)"
  mean_ms   = $rtsJitStats.mean_ms
  median_ms = $rtsJitStats.median_ms
  p95_ms    = $rtsJitStats.p95_ms
  min_ms    = $rtsJitStats.min_ms
  max_ms    = $rtsJitStats.max_ms
}
$summary += [PSCustomObject]@{
  runner    = "RTS (compiled)"
  mean_ms   = $rtsCompiledStats.mean_ms
  median_ms = $rtsCompiledStats.median_ms
  p95_ms    = $rtsCompiledStats.p95_ms
  min_ms    = $rtsCompiledStats.min_ms
  max_ms    = $rtsCompiledStats.max_ms
}
$summary += [PSCustomObject]@{
  runner    = "Bun (run)"
  mean_ms   = $bunStats.mean_ms
  median_ms = $bunStats.median_ms
  p95_ms    = $bunStats.p95_ms
  min_ms    = $bunStats.min_ms
  max_ms    = $bunStats.max_ms
}
$summary += [PSCustomObject]@{
  runner    = "Node (run)"
  mean_ms   = $NodeStats.mean_ms
  median_ms = $NodeStats.median_ms
  p95_ms    = $NodeStats.p95_ms
  min_ms    = $NodeStats.min_ms
  max_ms    = $NodeStats.max_ms
}

$summary | Format-Table -AutoSize

# Comparações relativas (tomando RTS compiled como base)
Write-Host "`n=== Relative Comparisons ==="
if ($rtsCompiledStats.mean_ms -gt 0) {
  $rtsRunVsCompiled = $rtsRunStats.mean_ms / $rtsCompiledStats.mean_ms
  $rtsJitVsCompiled = $rtsJitStats.mean_ms / $rtsCompiledStats.mean_ms
  $bunVsCompiled    = $bunStats.mean_ms    / $rtsCompiledStats.mean_ms
  $nodeVsCompiled   = $NodeStats.mean_ms   / $rtsCompiledStats.mean_ms
  Write-Host ("RTS (run, AOT) vs RTS compiled : {0:F2}x slower" -f $rtsRunVsCompiled)
  Write-Host ("RTS (run, JIT) vs RTS compiled : {0:F2}x slower" -f $rtsJitVsCompiled)
  Write-Host ("Bun (run)      vs RTS compiled : {0:F2}x slower" -f $bunVsCompiled)
  Write-Host ("Node (run)     vs RTS compiled : {0:F2}x slower" -f $nodeVsCompiled)
}

# Comparações vs Bun
if ($bunStats.mean_ms -gt 0 -and $rtsJitStats.mean_ms -gt 0) {
  $rtsJitVsBun      = $bunStats.mean_ms / $rtsJitStats.mean_ms
  $rtsCompiledVsBun = $bunStats.mean_ms / $rtsCompiledStats.mean_ms
  Write-Host ("RTS (run, JIT) vs Bun          : {0:F2}x faster" -f $rtsJitVsBun)
  Write-Host ("RTS (compiled) vs Bun          : {0:F2}x faster" -f $rtsCompiledVsBun)
}
