#!/usr/bin/env bash
# Corre o corpus de fixtures CSS (tests/css/) pelo motor e compara com o Chrome.
#
#   bash scripts/css_fixtures.sh                 # tolerância 1px
#   CSS_TOL=2 bash scripts/css_fixtures.sh       # 2px
#   CSS_FILTRO=flex bash scripts/css_fixtures.sh # só as que têm "flex" no nome
#
# Não constrói nada: usa o `run_fixture` que já está em `target/release/`,
# porque um `cargo build --release` aqui são minutos e o CLAUDE.md proíbe-o no
# laço de iteração. Se o binário não existir, diz-se em vez de o construir em
# silêncio — construir por baixo do pé de quem chamou é como um script deixa de
# ser previsível.
set -u

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ" || exit 1

BIN="target/release/examples/run_fixture.exe"
[ -x "$BIN" ] || BIN="target/release/examples/run_fixture"
if [ ! -x "$BIN" ]; then
  echo "não há $BIN — construa-o com: cargo build --release -p rts-host --example run_fixture" >&2
  exit 2
fi

exec "$BIN" examples/claude-css-runner.ts
