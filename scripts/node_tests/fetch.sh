#!/usr/bin/env bash
# Clona `test/` do nodejs/node para .node-suite/ (nao versionado).
#
# Esparso e raso de proposito: a arvore do Node inteira sao mais de 1 GB e o que
# esta regua le e `test/parallel`, `test/common` e `test/fixtures`. Uma TAG e nao
# `main`, porque a suite muda todos os dias e uma percentagem contra um alvo
# movel nao e comparavel consigo mesma na semana seguinte.
set -euo pipefail

TAG="${1:-v22.11.0}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$ROOT/.node-suite/node"

if [ -d "$DEST/.git" ]; then
  echo "ja existe: $DEST ($(git -C "$DEST" describe --tags 2>/dev/null || echo '?'))"
  echo "para trocar de tag, apague o diretorio primeiro"
  exit 0
fi

mkdir -p "$DEST"
git -C "$DEST" init -q
git -C "$DEST" remote add origin https://github.com/nodejs/node.git
git -C "$DEST" config core.sparseCheckout true
git -C "$DEST" sparse-checkout set --no-cone 'test/parallel' 'test/common' 'test/fixtures'
echo "a buscar $TAG (raso, so test/)..."
git -C "$DEST" fetch -q --depth 1 origin "refs/tags/$TAG:refs/tags/$TAG"
git -C "$DEST" checkout -q "$TAG"

echo "$TAG" > "$ROOT/.node-suite/TAG"
echo "$(find "$DEST/test/parallel" -name 'test-*.js' | wc -l) ficheiros em test/parallel"
