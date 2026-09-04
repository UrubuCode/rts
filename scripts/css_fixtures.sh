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

# Dois binários servem, e o corredor é o mesmo ficheiro TS sobre o mesmo
# `rts:dom`: o exemplo `run_fixture` (um processo sem nada do CLI no caminho) e
# o próprio `rts` (`cargo build --release` do pacote raiz — o que toda a gente
# já tem, e o que o CI usa). O exemplo tem prioridade quando existe; o CLI é o
# fallback, e foi por não existir que esta régua ficou dias sem ser corrida.
for cand in target/release/examples/run_fixture.exe target/release/examples/run_fixture; do
  if [ -x "$cand" ]; then exec "$cand" examples/claude-css-runner.ts; fi
done
for cand in target/release/rts.exe target/release/rts; do
  if [ -x "$cand" ]; then exec "$cand" run examples/claude-css-runner.ts; fi
done
echo "não há binário em target/release/ — construa um: cargo build --release (o CLI) ou cargo build --release -p rts-host --example run_fixture" >&2
exit 2
