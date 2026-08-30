#!/bin/bash
# Corre a suite com o binario dado e escreve "<ficheiro> <1|0>" por linha.
# `rts test` escreve o resultado em STDERR — capturar so o stdout dava zero
# verdes em 824 ficheiros, que e um instrumento partido e nao um motor partido.
BIN="$1"; OUT="$2"
: > "$OUT"
for f in tests/*.test.ts; do
  if timeout 60 "$BIN" test "$f" 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep -qE "[0-9]+ tests? passed"; then
    echo "$f 1" >> "$OUT"
  else
    echo "$f 0" >> "$OUT"
  fi
done
echo "FEITO $OUT: $(awk '$2==1' "$OUT" | wc -l) verdes de $(wc -l < "$OUT")"
