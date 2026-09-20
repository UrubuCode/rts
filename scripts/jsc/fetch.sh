#!/usr/bin/env bash
# Clona `JSTests/stress` do WebKit para .jsc/ (nao versionado), num SHA FIXO.
set -euo pipefail

SHA="${1:-main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$ROOT/.jsc/webkit"

if [ -d "$DEST/.git" ]; then
  echo "ja existe: $DEST ($(git -C "$DEST" rev-parse --short HEAD))"
  exit 0
fi

mkdir -p "$DEST"
git -C "$DEST" init -q
git -C "$DEST" remote add origin https://github.com/WebKit/WebKit.git
git -C "$DEST" config core.sparseCheckout true
# O LICENSE vem com o corpus de proposito: nada daqui e copiado para este
# repositorio, portanto nada obriga a isto — mas assim os termos estao ao lado
# dos ficheiros na revisao fixada, que e a unica copia que pode ser autoritativa.
# Uma licenca citada de memoria e uma afirmacao, nao um aviso.
git -C "$DEST" sparse-checkout set --no-cone 'JSTests/stress' 'LICENSE'
echo "a buscar $SHA (raso, so JSTests/stress)..."
git -C "$DEST" fetch -q --depth 1 --filter=blob:none origin "$SHA"
git -C "$DEST" checkout -q FETCH_HEAD

git -C "$DEST" rev-parse HEAD > "$ROOT/.jsc/SHA"
echo "$(find "$DEST/JSTests/stress" -name '*.js' | wc -l) ficheiros"
