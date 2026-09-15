#!/usr/bin/env bash
# Clona `test/mjsunit` do V8 para .mjsunit/ (nao versionado), num SHA FIXO.
#
# Um SHA e nao `main` pela mesma razao que o test262 e a suite do Node: a suite
# muda todos os dias e uma percentagem contra um alvo movel nao e comparavel
# consigo mesma na semana seguinte.
set -euo pipefail

SHA="${1:-main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$ROOT/.mjsunit/v8"

if [ -d "$DEST/.git" ]; then
  echo "ja existe: $DEST ($(git -C "$DEST" rev-parse --short HEAD))"
  echo "para trocar de SHA, apague o diretorio primeiro"
  exit 0
fi

mkdir -p "$DEST"
git -C "$DEST" init -q
git -C "$DEST" remote add origin https://github.com/v8/v8.git
git -C "$DEST" config core.sparseCheckout true
git -C "$DEST" sparse-checkout set --no-cone 'test/mjsunit'
echo "a buscar $SHA (raso, so test/mjsunit)..."
if [ "$SHA" = "main" ]; then
  git -C "$DEST" fetch -q --depth 1 --filter=blob:none origin main
else
  git -C "$DEST" fetch -q --depth 1 --filter=blob:none origin "$SHA"
fi
git -C "$DEST" checkout -q FETCH_HEAD

git -C "$DEST" rev-parse HEAD > "$ROOT/.mjsunit/SHA"
echo "$(find "$DEST/test/mjsunit" -name '*.js' | wc -l) ficheiros"
