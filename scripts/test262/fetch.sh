#!/usr/bin/env bash
# Clona o test262 para .test262/ (nao versionado), num SHA FIXO.
#
# Um SHA e nao `main`: a suite muda todos os dias e uma percentagem contra um
# alvo movel nao e comparavel consigo mesma na semana seguinte. E o mesmo
# argumento que `scripts/node_tests/fetch.sh` faz para a tag do Node.
#
# Nada do test262 entra neste repositorio. A licenca proibe usar o nome da Ecma
# para endossar, e o que este arnes produz e uma medicao sobre NOS — nunca uma
# afirmacao de conformidade. THIRD-PARTY-NOTICES.md tem a licenca.
set -euo pipefail

SHA="${1:-90dd8d86507e9b2d194b6a77e63b76041a971fa9}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEST="$ROOT/.test262/test262"

if [ -d "$DEST/.git" ]; then
  echo "ja existe: $DEST ($(git -C "$DEST" rev-parse --short HEAD))"
  echo "para trocar de SHA, apague o diretorio primeiro"
  exit 0
fi

mkdir -p "$DEST"
git -C "$DEST" init -q
git -C "$DEST" remote add origin https://github.com/tc39/test262.git
git -C "$DEST" config core.longpaths true
git -C "$DEST" config core.sparseCheckout true
git -C "$DEST" sparse-checkout set --no-cone 'test' 'harness'
echo "a buscar $SHA (raso, so test/ e harness/)..."
git -C "$DEST" fetch -q --depth 1 --filter=blob:none origin "$SHA"
git -C "$DEST" checkout -q FETCH_HEAD

echo "$SHA" > "$ROOT/.test262/SHA"
# Contado aqui e nao assumido: um checkout que traz menos do que diz e o erro
# que ja custou 0,8 pontos a este repositorio uma vez.
echo "$(find "$DEST/test" -name '*.js' | wc -l) ficheiros em test/"
