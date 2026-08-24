#!/usr/bin/env bash
# Corre a suite do Node contra o rts e imprime a percentagem por modulo.
#
#   bash scripts/node_tests/run.sh              # tudo
#   bash scripts/node_tests/run.sh fs path url  # so estes modulos
#   RTS_BIN=target/baseline.exe bash scripts/node_tests/run.sh   # outra arvore
#   TIMEOUT=30 JOBS=8 bash scripts/node_tests/run.sh
#
# Um processo por ficheiro, pela razao que o `suite_run` deste repositorio ja
# documenta: uma excecao nao apanhada e um ciclo infinito levam o processo com
# eles, e um arnes de processo unico reportava o que alcancasse primeiro como
# sendo o resultado de tudo.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SUITE="$ROOT/.node-suite"
SRC="$SUITE/node/test/parallel"
RTS="${RTS_BIN:-$ROOT/target/release/rts.exe}"
TIMEOUT="${TIMEOUT:-15}"
JOBS="${JOBS:-4}"

[ -d "$SRC" ] || { echo "sem corpus: corra scripts/node_tests/fetch.sh"; exit 1; }
[ -x "$RTS" ] || { echo "sem binario: $RTS (cargo build --release)"; exit 1; }

# Sem argumentos: todos. Com: so `test-<mod>.js` e `test-<mod>-*.js`.
list_files() {
  if [ $# -eq 0 ]; then
    find "$SRC" -maxdepth 1 -name 'test-*.js' | sort
  else
    for m in "$@"; do
      find "$SRC" -maxdepth 1 \( -name "test-$m.js" -o -name "test-$m-*.js" \) | sort
    done
  fi
}

# Uma linha TSV por ficheiro: modulo, nome, estado, primeira linha do erro.
#
# O modulo e o primeiro segmento a seguir a `test-`, que e como a propria suite
# os agrupa (`test-fs-open-flags` -> `fs`). Nao e uma classificacao nossa: uma
# seria uma segunda resposta a "de que modulo e este teste".
one() {
  f="$1"
  name="$(basename "$f" .js)"
  mod="$(echo "$name" | sed -E 's/^test-([a-z0-9]+).*/\1/')"
  out="$(cd "$SRC" && timeout "$TIMEOUT" "$RTS" run "$f" 2>&1)"
  code=$?
  first="$(echo "$out" | grep -m1 -E 'Error|error|panic' | head -c 200 | tr '\t\n' '  ')"
  case $code in
    0) status="ok" ;;
    124) status="timeout" ;;
    *) case "$out" in
         *AssertionError*|*"Assertion failed"*|*mustCall*) status="fail" ;;
         *) status="error" ;;
       esac ;;
  esac
  printf '%s\t%s\t%s\t%s\n' "$mod" "$name" "$status" "$first"
}
export -f one
export SRC RTS TIMEOUT

files="$(list_files "$@")"
total="$(echo "$files" | grep -c . )"
echo "$total ficheiros, $JOBS de cada vez, $TIMEOUT s cada"

rows="$SUITE/rows.tsv"
echo "$files" | grep . | xargs -P "$JOBS" -I{} bash -c 'one "$@"' _ {} > "$rows"

node "$ROOT/scripts/node_tests/report.mjs" "$rows" "$SUITE/report.json"
