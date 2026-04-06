param(
  [int]$Runs = 40,
  [int]$Warmup = 5
)

$ErrorActionPreference = "Stop"

Set-Location (Split-Path -Parent $PSScriptRoot)

$rtsSource = "examples/bench_simple.ts"
$rtsOutput = "target/bench_simple"
$rtsExe = "target/bench_simple.exe"
$bunSource = "bench/bun_simple.ts"

Write-Host "Building RTS benchmark binary..."
cargo run --quiet -- build $rtsSource $rtsOutput | Out-Null

if (!(Test-Path $rtsExe)) {
  throw "RTS binary not found at $rtsExe"
}

function Measure-OneRunMs([scriptblock]$Action) {
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  & $Action
  $sw.Stop()
  return $sw.Elapsed.TotalMilliseconds
}

function Measure-Suite([string]$Label, [scriptblock]$Action, [int]$Warm, [int]$TotalRuns) {
  Write-Host "Warmup $Label ($Warm runs)..."
  for ($i = 0; $i -lt $Warm; $i++) {
    & $Action
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

$rtsAction = { & ".\target\bench_simple.exe" *> $null }
$bunAction = { bun run ".\bench\bun_simple.ts" *> $null }

$rtsResults = Measure-Suite "RTS binary" $rtsAction $Warmup $Runs
$bunResults = Measure-Suite "bun run" $bunAction $Warmup $Runs

$rtsStats = Get-Stats $rtsResults
$bunStats = Get-Stats $bunResults

$summary = @()
$summary += [PSCustomObject]@{
  runner = "RTS binary"
  mean_ms = $rtsStats.mean_ms
  median_ms = $rtsStats.median_ms
  p95_ms = $rtsStats.p95_ms
  min_ms = $rtsStats.min_ms
  max_ms = $rtsStats.max_ms
}
$summary += [PSCustomObject]@{
  runner = "bun run"
  mean_ms = $bunStats.mean_ms
  median_ms = $bunStats.median_ms
  p95_ms = $bunStats.p95_ms
  min_ms = $bunStats.min_ms
  max_ms = $bunStats.max_ms
}

Write-Host ""
Write-Host "Benchmark summary (ms):"
$summary | Format-Table -AutoSize

if ($rtsStats.mean_ms -gt 0) {
  $speedup = $bunStats.mean_ms / $rtsStats.mean_ms
  Write-Host ("Relative (bun_mean / rts_mean): {0}x" -f [Math]::Round($speedup, 3))
}
