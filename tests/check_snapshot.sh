#!/usr/bin/env bash
# Checker do teste visual headless do egui glow. Parseia um PPM P6 e ASSERTA que
# o frame não está em branco: conta pixels que diferem do fundo escuro do clear
# (~RGB 5,5,8). Conteúdo (texto/widgets) => muitos pixels claros. Frame em branco
# (texto não pintou) => ~0 pixels de conteúdo => FALHA.
#
# uso: bash tests/check_snapshot.sh <arquivo.ppm> [min_pct]
set -euo pipefail
ppm="${1:?uso: check_snapshot.sh <arquivo.ppm> [min_pct]}"
min_pct="${2:-1}"   # mínimo % de pixels de conteúdo p/ considerar "não-branco"

if [ ! -f "$ppm" ]; then
  echo "FALHA: PPM não encontrado: $ppm"; exit 1
fi

# Conta pixels de conteúdo via Python (parse binário P6 robusto).
python3 - "$ppm" "$min_pct" <<'PY'
import sys
path, min_pct = sys.argv[1], float(sys.argv[2])
data = open(path, "rb").read()
# Header P6: "P6\n<w> <h>\n255\n" (campos separados por whitespace).
assert data[:2] == b"P6", "não é PPM P6"
# Lê 3 tokens (w, h, maxval) após o magic, pulando whitespace.
i = 2; toks = []
while len(toks) < 3:
    while i < len(data) and data[i] in b" \t\n\r": i += 1
    j = i
    while j < len(data) and data[j] not in b" \t\n\r": j += 1
    toks.append(int(data[i:j])); i = j
i += 1  # único whitespace após maxval
w, h, _ = toks
px = data[i:]
total = w*h
# O egui pinta o fundo do painel sobre o clear -> a cor DOMINANTE e o fundo. Tinta
# (texto/widgets) = pixels longe da MEDIA (texto e fracao pequena -> media ~= fundo).
# Frame em branco (so painel) -> media == fundo -> ~0 tinta.
sr = sg = sb = 0
for p in range(0, total*3, 3):
    sr += px[p]; sg += px[p+1]; sb += px[p+2]
mr, mg, mb = sr/total, sg/total, sb/total
ink = 0
for p in range(0, total*3, 3):
    if abs(px[p]-mr)+abs(px[p+1]-mg)+abs(px[p+2]-mb) > 60:
        ink += 1
pct = 100.0*ink/total if total else 0.0
print(f"PPM {w}x{h}: fundo~RGB({mr:.0f},{mg:.0f},{mb:.0f}); tinta {ink}/{total} px ({pct:.2f}%)")
if pct < min_pct:
    print(f"FALHA: frame quase uniforme (<{min_pct}% tinta) — texto/widgets não pintaram?")
    sys.exit(1)
print("OK: frame tem tinta distinta do fundo (texto/widgets pintaram).")
PY
