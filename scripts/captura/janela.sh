#!/usr/bin/env bash
# Invólucro de bash para `janela.ps1` — a captura em si é PowerShell porque quem
# fotografa uma janela é a API do Windows (`PrintWindow`), e não há equivalente
# do lado do bash. Existe para que a captura se chame do mesmo sítio de onde se
# corre tudo o resto neste repositório.
#
#   bash scripts/captura/janela.sh examples/claude-page-janela.ts saida.png '*wikipedia*'
set -euo pipefail

if [ $# -lt 2 ]; then
  echo "uso: bash scripts/captura/janela.sh <programa.ts> <saida.png> [filtro-do-titulo]" >&2
  exit 2
fi

aqui="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
powershell -NoProfile -ExecutionPolicy Bypass -File "$aqui/janela.ps1" \
  -Programa "$1" -Saida "$2" ${3:+-Titulo "$3"}
