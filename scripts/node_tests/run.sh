#!/usr/bin/env bash
# Corre a suite do Node contra o rts e imprime a percentagem por modulo.
#
#   bash scripts/node_tests/run.sh              # TUDO (~3 500 ficheiros)
#   bash scripts/node_tests/run.sh fs path url  # so estes modulos
#   RESUME=1 bash scripts/node_tests/run.sh     # continua uma corrida interrompida
#   RTS_BIN=target/baseline.exe REPORT=base.json bash scripts/node_tests/run.sh
#   TIMEOUT=10 JOBS=12 bash scripts/node_tests/run.sh
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
TIMEOUT="${TIMEOUT:-10}"
# Deixa quatro cores de fora: cada trabalho e um processo que compila antes de
# correr, e saturar a maquina faz o `timeout` disparar por espera em vez de por
# ciclo infinito — o que reportaria como `t/o` um ficheiro que estava so na fila.
JOBS="${JOBS:-$(( $(nproc 2>/dev/null || echo 8) - 4 ))}"
[ "$JOBS" -lt 1 ] && JOBS=1
ROWS="${ROWS:-$SUITE/rows.tsv}"
REPORT="${REPORT:-$SUITE/report.json}"

[ -d "$SRC" ] || { echo "sem corpus: corra scripts/node_tests/fetch.sh"; exit 1; }
[ -x "$RTS" ] || { echo "sem binario: $RTS (cargo build --release)"; exit 1; }
command -v node >/dev/null || { echo "o relatorio corre em node; instale-o"; exit 1; }

# `.mjs` tambem: a suite tem 65 deles e sao a metade ESM da mesma pergunta.
list_files() {
  if [ $# -eq 0 ]; then
    find "$SRC" -maxdepth 1 \( -name 'test-*.js' -o -name 'test-*.mjs' \) | sort
  else
    for m in "$@"; do
      find "$SRC" -maxdepth 1 \
        \( -name "test-$m.js" -o -name "test-$m-*.js" \
        -o -name "test-$m.mjs" -o -name "test-$m-*.mjs" \) | sort
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
  name="$(basename "$f")"
  name="${name%.*}"
  mod="$(echo "$name" | sed -E 's/^test-([a-z0-9]+).*/\1/')"
  # Corrido a partir do diretorio do ficheiro: a suite escreve e le caminhos
  # relativos ao seu proprio lugar, e uma corrida a partir da raiz do repo
  # mediria a nossa escolha de diretorio em vez do teste.
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

# RESUME=1 continua de onde parou. Uma corrida completa sao dezenas de minutos e
# um Ctrl-C a meio deitava fora tudo o que ja custou; as linhas ja escritas sao
# resultados, nao estado intermedio.
if [ "${RESUME:-0}" = "1" ] && [ -s "$ROWS" ]; then
  done_names="$(cut -f2 "$ROWS" | sort -u)"
  files="$(echo "$files" | while IFS= read -r f; do
    n="$(basename "$f")"; n="${n%.*}"
    echo "$done_names" | grep -qx "$n" || echo "$f"
  done)"
  echo "a continuar: $(echo "$files" | grep -c .) por correr, $(wc -l < "$ROWS") ja feitos"
else
  : > "$ROWS"
fi

total="$(echo "$files" | grep -c .)"
[ "$total" -eq 0 ] && { echo "nada por correr"; node "$ROOT/scripts/node_tests/report.mjs" "$ROWS" "$REPORT"; exit 0; }
echo "$total ficheiros, $JOBS de cada vez, $TIMEOUT s cada"

# O progresso vem do proprio fluxo de linhas, contado a medida que passam para o
# ficheiro — nao ha segundo contador que possa discordar do que foi escrito.
echo "$files" | grep . | xargs -P "$JOBS" -I{} bash -c 'one "$@"' _ {} \
  | tee -a "$ROWS" \
  | awk -v total="$total" '{ n++; if (n % 100 == 0) printf "  ... %d/%d\n", n, total > "/dev/stderr" }'

node "$ROOT/scripts/node_tests/report.mjs" "$ROWS" "$REPORT"
