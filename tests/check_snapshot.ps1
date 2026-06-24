# Checker do teste visual headless do egui glow. Parseia um PPM P6 e ASSERTA que
# o frame nao esta em branco: conta pixels que diferem do fundo escuro do clear
# (~RGB 5,5,8). Conteudo (texto/widgets) => muitos pixels claros; frame em branco
# => ~0 => FALHA (exit 1).
#
# uso: powershell -File tests/check_snapshot.ps1 <arquivo.ppm> [min_pct]
param([Parameter(Mandatory=$true)][string]$Ppm, [double]$MinPct = 1.0)

if (-not (Test-Path $Ppm)) { Write-Output "FALHA: PPM nao encontrado: $Ppm"; exit 1 }
$bytes = [System.IO.File]::ReadAllBytes($Ppm)
if ($bytes[0] -ne 80 -or $bytes[1] -ne 54) { Write-Output "FALHA: nao e PPM P6"; exit 1 }

# Le 3 tokens (w,h,maxval) apos o magic, pulando whitespace.
$i = 2; $toks = @()
while ($toks.Count -lt 3) {
  while ($i -lt $bytes.Length -and ($bytes[$i] -eq 32 -or $bytes[$i] -eq 9 -or $bytes[$i] -eq 10 -or $bytes[$i] -eq 13)) { $i++ }
  $j = $i
  while ($j -lt $bytes.Length -and -not ($bytes[$j] -eq 32 -or $bytes[$j] -eq 9 -or $bytes[$j] -eq 10 -or $bytes[$j] -eq 13)) { $j++ }
  $toks += [int][string]([System.Text.Encoding]::ASCII.GetString($bytes[$i..($j-1)])); $i = $j
}
$i++  # whitespace unico apos maxval
$w = $toks[0]; $h = $toks[1]; $total = $w * $h
$end = $bytes.Length - 2

# O egui pinta o fundo do painel sobre o clear -> a cor DOMINANTE e o fundo, nao
# o clear. "Tinta" (texto/widgets) = pixels longe da MEDIA (que ~= o fundo, pois
# texto e fracao pequena). Frame em branco (so painel) -> media == fundo -> ~0
# tinta. Com texto -> glifos claros distantes da media.
$sr = 0.0; $sg = 0.0; $sb = 0.0
for ($p = $i; $p -lt $end; $p += 3) { $sr += $bytes[$p]; $sg += $bytes[$p+1]; $sb += $bytes[$p+2] }
$mr = $sr / $total; $mg = $sg / $total; $mb = $sb / $total
$ink = 0
for ($p = $i; $p -lt $end; $p += 3) {
  $d = [math]::Abs($bytes[$p] - $mr) + [math]::Abs($bytes[$p+1] - $mg) + [math]::Abs($bytes[$p+2] - $mb)
  if ($d -gt 60) { $ink++ }
}
$pct = if ($total) { 100.0 * $ink / $total } else { 0.0 }
"PPM {0}x{1}: fundo~RGB({2:N0},{3:N0},{4:N0}); tinta {5}/{6} px ({7:N2}%)" -f $w,$h,$mr,$mg,$mb,$ink,$total,$pct
if ($pct -lt $MinPct) {
  "FALHA: frame quase uniforme (menos de $MinPct% tinta) - texto/widgets nao pintaram?"
  exit 1
}
"OK: frame tem tinta distinta do fundo (texto/widgets pintaram)."
exit 0
