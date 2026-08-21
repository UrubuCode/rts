#!/usr/bin/env bash
# O COMANDO ÚNICO do harness de paridade.
#
#   bash scripts/parity/run.sh                 # tudo, tolerância 1px
#   TOL=2 bash scripts/parity/run.sh           # outra tolerância
#   PULAR_RTS=1 bash scripts/parity/run.sh     # reaproveita o rts.jsonl anterior
#
# Corre da RAIZ do repositório: os dois extratores usam caminhos relativos a ela
# e o `run_fixture` resolve `scripts/parity/…` contra o cwd.
set -euo pipefail
cd "$(dirname "$0")/../.."

HTML=${HTML:-pagina.html}
CSS=${CSS:-pagina.css}
# O diretório de saída é PARAMETRIZÁVEL, e não é conveniência: duas medições
# sobre páginas diferentes escreviam por cima uma da outra, e um agente ficou a
# comparar o nosso lado de uma página com o lado Chrome de outra sem ter como
# saber. Um corpus por página, e cada um sobrevive ao seguinte.
#
#   OUT=scripts/parity/out-bootstrap HTML=... CSS=... bash scripts/parity/run.sh
OUT=${OUT:-scripts/parity/out}
# A combinada segue o OUT, e isto é a MESMA armadilha que o `OUT` veio resolver,
# apanhada uma segunda vez no mesmo ficheiro: o diretório de saída era
# parametrizável e o ficheiro de ENTRADA não, portanto medir uma segunda página
# reescrevia a combinada da primeira em silêncio. A corrida seguinte da primeira
# página lia a segunda e reportava 420 elementos onde havia 16 813 — um
# denominador colapsado com cara de medição.
COMBINADA=${COMBINADA:-$OUT/pagina.combinada.html}
TOL=${TOL:-1}
TOP=${TOP:-20}

mkdir -p "$OUT"

if [ ! -f "$HTML" ] || [ ! -f "$CSS" ]; then
  echo "faltam $HTML/$CSS — gere com:"
  echo "  target/release/examples/run_fixture.exe examples/claude-page-dump.ts"
  exit 2
fi

# A página combinada é a ARMADILHA CENTRAL deste harness resolvida de uma vez.
# O nosso lado sabia ler o CSS por fora (`addStylesheet`) e o Chrome não; se cada
# lado montasse a sua página, o harness compararia duas páginas diferentes e
# chamaria à diferença "divergência de layout". Um único ficheiro com o CSS
# embutido como `<style>` no topo é o que faz a entrada ser literalmente a mesma.
echo "[1/4] a combinar $HTML + $CSS -> $COMBINADA"
node -e '
  const fs = require("fs");
  const [h, c, o] = process.argv.slice(1);
  fs.writeFileSync(o, "<style>" + fs.readFileSync(c, "utf8") + "</style>\n" + fs.readFileSync(h, "utf8"));
  console.log("      html=" + fs.statSync(h).size + "B css=" + fs.statSync(c).size + "B");
' "$HTML" "$CSS" "$COMBINADA"

echo "[2/4] lado Chrome"
node scripts/parity/chrome_extract.mjs "$COMBINADA" "$OUT/chrome.jsonl"

if [ "${PULAR_RTS:-0}" = "1" ] && [ -f "$OUT/rts.jsonl" ]; then
  echo "[3/4] lado RTS — PULADO (PULAR_RTS=1, a usar $OUT/rts.jsonl de antes)"
else
  # ~5 minutos na Wikipédia: são ~16k elementos x 9 chamadas de fronteira cada.
  # É o custo do instrumento, não do layout — o layout corre uma vez.
  echo "[3/4] lado RTS (leva minutos — 9 chamadas de fronteira por elemento)"
  target/release/examples/run_fixture.exe examples/claude-parity-rts.ts > "$OUT/rts.jsonl"
  echo "      $(grep -c . "$OUT/rts.jsonl") linhas"
fi

echo "[4/4] comparação (tolerância ${TOL}px)"
echo
node scripts/parity/compare.mjs --tol "$TOL" --top "$TOP" \
  --rts "$OUT/rts.jsonl" --chrome "$OUT/chrome.jsonl" | tee "$OUT/relatorio.txt"
