# Captura a JANELA do motor num PNG: corre um `.ts` no `ui_fixture`, espera que a
# janela apareça e ASSENTE, fotografa-a e mata o processo.
#
# Existe porque a tela branca enganou três vezes num dia: o layout, a lista de
# pintura e as métricas diziam todas que estava certo, e o que resolveu foi olhar
# para os pixels. Isso era feito com vinte linhas de PowerShell reescritas à mão
# de cada vez, que não sobrevivem à sessão.
#
#   powershell -File scripts/captura/janela.ps1 -Programa examples/x.ts -Saida x.png
#
# `PrintWindow` com a flag 2 (PW_RENDERFULLCONTENT) é o que fotografa uma janela
# desenhada pela GPU; sem ela o wgpu devolve preto. É também por isso que a
# captura NÃO exige que a janela esteja à frente — o que evita uma corrida com
# quem estiver a usar o computador.

param(
  # O `.ts` a correr (o que abre a janela).
  [Parameter(Mandatory=$true)][string]$Programa,
  # Onde gravar o PNG. Por PARÂMETRO de propósito: duas capturas em paralelo
  # escreviam por cima uma da outra quando o caminho era fixo.
  [Parameter(Mandatory=$true)][string]$Saida,
  # Filtro do título da janela (o `MainWindowTitle`). O default apanha qualquer
  # janela do motor; passe um pedaço do título quando houver mais de uma.
  [string]$Titulo = '*',
  # Segundos a esperar pela janela e pelo assentamento.
  [int]$Espera = 60,
  # Variáveis de ambiente para o processo, `NOME=valor` (ex.: RTS_DOM_PAINT=1).
  [string[]]$Ambiente = @()
)

$ErrorActionPreference = 'Stop'
$raiz = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exe  = Join-Path $raiz 'target/release/examples/ui_fixture.exe'

if (-not (Test-Path $exe)) {
  Write-Error "falta $exe — construa com: cargo build --release -p rts-host --features ui --example ui_fixture"
}
if (-not (Test-Path $Programa)) { Write-Error "não existe: $Programa" }

Add-Type @'
using System;using System.Runtime.InteropServices;using System.Drawing;
public class CapturaJanela {
 [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
 [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
 public struct RECT { public int L,T,R,B; }
 // PW_RENDERFULLCONTENT (2): manda a janela redesenhar-se para o nosso DC. Sem
 // isto uma superfície de GPU sai preta, que era o modo de a captura mentir.
 public static Bitmap Tirar(IntPtr h) {
   RECT r; if (!GetWindowRect(h, out r)) return null;
   int w = r.R - r.L, alt = r.B - r.T;
   if (w <= 0 || alt <= 0) return null;
   var bmp = new Bitmap(w, alt);
   using (var g = Graphics.FromImage(bmp)) {
     IntPtr dc = g.GetHdc();
     bool ok = PrintWindow(h, dc, 2);
     g.ReleaseHdc(dc);
     if (!ok) { bmp.Dispose(); return null; }
   }
   return bmp;
 }
}
'@ -ReferencedAssemblies System.Drawing

# Bytes PNG da janela, ou $null enquanto ela não puder ser fotografada. Comparar
# BYTES é o que deteta o assentamento sem saber nada sobre o conteúdo.
function Fotografar([IntPtr]$hwnd) {
  $bmp = [CapturaJanela]::Tirar($hwnd)
  if ($null -eq $bmp) { return $null }
  try {
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    return $ms.ToArray()
  } finally { $bmp.Dispose() }
}

$proc = $null
try {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $exe
  $psi.Arguments = '"' + $Programa + '"'
  $psi.WorkingDirectory = $raiz
  $psi.UseShellExecute = $false
  foreach ($par in $Ambiente) {
    $i = $par.IndexOf('=')
    if ($i -gt 0) { $psi.EnvironmentVariables[$par.Substring(0,$i)] = $par.Substring($i+1) }
  }
  $proc = [System.Diagnostics.Process]::Start($psi)
  Write-Output ("processo " + $proc.Id + " — " + $Programa)

  # 1. esperar a JANELA. Um `.ts` que compila e só depois abre a janela leva
  #    segundos; um que rebenta morre — por isso o laço também vigia o processo.
  $limite = (Get-Date).AddSeconds($Espera)
  $hwnd = [IntPtr]::Zero
  while ((Get-Date) -lt $limite) {
    if ($proc.HasExited) { Write-Error ("o processo morreu antes de abrir janela (saída " + $proc.ExitCode + ")") }
    $proc.Refresh()
    if ($proc.MainWindowHandle -ne [IntPtr]::Zero -and $proc.MainWindowTitle -like $Titulo) {
      $hwnd = $proc.MainWindowHandle
      break
    }
    Start-Sleep -Milliseconds 250
  }
  if ($hwnd -eq [IntPtr]::Zero) { Write-Error "a janela não apareceu em $Espera s (filtro de título: $Titulo)" }
  Write-Output ("janela: " + $proc.MainWindowTitle)

  # 2. esperar que ASSENTE: dois retratos seguidos iguais. Uma página real leva
  #    frames a chegar ao estado final (fontes, layout, primeira pintura), e
  #    fotografar a meio dava uma imagem que não é o que o motor mostra. Um
  #    tempo fixo de sono ou era curto demais ou desperdiçava a diferença.
  $anterior = $null; $atual = $null; $estavel = $false
  while ((Get-Date) -lt $limite) {
    if ($proc.HasExited) { break }
    $atual = Fotografar $hwnd
    if ($null -ne $atual -and $null -ne $anterior -and
        $atual.Length -eq $anterior.Length -and
        [System.Linq.Enumerable]::SequenceEqual([byte[]]$atual, [byte[]]$anterior)) {
      $estavel = $true
      break
    }
    $anterior = $atual
    Start-Sleep -Milliseconds 500
  }
  if ($null -eq $atual) { Write-Error "não consegui fotografar a janela (PrintWindow falhou)" }
  if (-not $estavel) { Write-Output "aviso: a janela ainda mudava ao fim de $Espera s — gravo o último retrato" }

  $destino = [System.IO.Path]::GetFullPath((Join-Path (Get-Location) $Saida))
  $pasta = Split-Path -Parent $destino
  if ($pasta -and -not (Test-Path $pasta)) { New-Item -ItemType Directory -Force $pasta | Out-Null }
  [System.IO.File]::WriteAllBytes($destino, $atual)
  Write-Output ("gravado " + $destino + " (" + $atual.Length + " bytes)")
}
finally {
  # SEMPRE, e é a razão de haver um `finally`: um `ui_fixture` esquecido segura o
  # .exe e o próximo `cargo build` falha com LNK1104 — custou três builds no dia
  # em que este script não existia.
  if ($null -ne $proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Write-Output ("processo " + $proc.Id + " terminado")
  }
}
