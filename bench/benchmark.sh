#!/usr/bin/env bash
# Roda os benchmarks canonicos e emite JSON no stdout.
# Uso: bench/benchmark.sh [runs] [warmup]
#
# Cada bench declara um source por runner (RTS frequentemente nao roda o
# mesmo .ts que Bun/Node/Deno porque ainda faltam features de linguagem).
# Output: { meta:{...}, benches:[ { id, runs:[ {runner, source, stats} ] } ] }

set -euo pipefail

RUNS="${1:-20}"
WARMUP="${2:-3}"
RTS_EXE="${RTS_EXE:-target/release/rts}"

cd "$(dirname "$0")/.."

if [ ! -x "$RTS_EXE" ]; then
  echo "rts binary not found at $RTS_EXE" >&2
  exit 1
fi

# ---------- helpers ----------
now_ms() { date +%s%3N; }

measure_one() {
  local start end
  start=$(now_ms)
  "$@" >/dev/null 2>&1
  end=$(now_ms)
  echo $((end - start))
}

stats_json() {
  awk '
    { v[NR]=$1; sum+=$1 }
    END {
      n=NR
      if (n==0) { print "{\"count\":0}"; exit }
      for (i=1;i<=n;i++) for (j=i+1;j<=n;j++) if (v[i]>v[j]) { t=v[i]; v[i]=v[j]; v[j]=t }
      mean = sum/n
      if (n%2==1) median = v[int(n/2)+1]
      else        median = (v[n/2]+v[n/2+1])/2
      p95i = int((n-1)*0.95 + 0.999) + 1
      if (p95i<1) p95i=1
      if (p95i>n) p95i=n
      printf "{\"count\":%d,\"mean_ms\":%.3f,\"median_ms\":%.3f,\"p95_ms\":%.3f,\"min_ms\":%d,\"max_ms\":%d}", n, mean, median, v[p95i], v[1], v[n]
    }'
}

run_suite() {
  local label="$1"; shift
  echo "  warmup $label ($WARMUP)..." >&2
  for ((i=0; i<WARMUP; i++)); do "$@" >/dev/null 2>&1 || true; done
  echo "  bench  $label ($RUNS)..." >&2
  local samples=""
  for ((i=0; i<RUNS; i++)); do
    samples+="$(measure_one "$@")"$'\n'
  done
  printf '%s' "$samples" | stats_json
}

have() { command -v "$1" >/dev/null 2>&1; }

# ---------- AOT prep: compila cada source RTS uma vez ----------
mkdir -p target/bench
declare -A AOT_BIN
aot_compile() {
  local src="$1"
  local key="${src//\//_}"
  local out="target/bench/${key%.ts}"
  echo "compiling AOT: $src -> $out" >&2
  "$RTS_EXE" compile -p "$src" "$out" --production >&2
  if [ -x "$out" ]; then AOT_BIN[$src]="$out"
  elif [ -x "$out.exe" ]; then AOT_BIN[$src]="$out.exe"
  else echo "AOT output missing for $src" >&2; return 1
  fi
}

# ---------- declaracao dos benches ----------
# formato: bench_id|rts_src|js_src
# se js_src vazio: nao roda em bun/node/deno
BENCHES=(
  "simple|bench/rts_simple.ts|bench/bun_simple.ts"
  "monte_carlo|bench/monte_carlo_pi.ts|bench/monte_carlo_pi.js"
  "pi_bigfloat|bench/pi_bigfloat.ts|bench/pi_bigfloat.js"
  "monte_carlo_threaded|bench/monte_carlo_pi_threaded.ts|bench/monte_carlo_pi_threaded_bun.ts"
  "pi_machin|bench/pi_machin.ts|"
)

# pre-compila todos os sources RTS (AOT)
for entry in "${BENCHES[@]}"; do
  IFS='|' read -r _id rts_src _js_src <<<"$entry"
  aot_compile "$rts_src"
done

# ---------- run ----------
benches_json="["
first_b=1
for entry in "${BENCHES[@]}"; do
  IFS='|' read -r id rts_src js_src <<<"$entry"
  echo "=== bench: $id ===" >&2

  runs_json="["
  first_r=1

  add_run() {
    local runner="$1" source="$2" stats="$3"
    [ $first_r -eq 0 ] && runs_json+=","
    runs_json+="{\"runner\":\"$runner\",\"source\":\"$source\",\"stats\":$stats}"
    first_r=0
  }

  # RTS JIT
  add_run "rts_jit" "$rts_src" "$(run_suite "RTS JIT [$id]" "$RTS_EXE" run "$rts_src")"
  # RTS AOT
  add_run "rts_aot" "$rts_src" "$(run_suite "RTS AOT [$id]" "${AOT_BIN[$rts_src]}")"

  if [ -n "$js_src" ]; then
    have bun  && add_run "bun"  "$js_src" "$(run_suite "Bun  [$id]" bun  run "$js_src")"
    have node && add_run "node" "$js_src" "$(run_suite "Node [$id]" node "$js_src")"
    have deno && add_run "deno" "$js_src" "$(run_suite "Deno [$id]" deno run --quiet --allow-all "$js_src")"
  fi
  runs_json+="]"

  [ $first_b -eq 0 ] && benches_json+=","
  benches_json+="{\"id\":\"$id\",\"runs\":$runs_json}"
  first_b=0
done
benches_json+="]"

commit_sha="${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}"
short_sha="${commit_sha:0:7}"
created=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

cat <<JSON
{
  "meta": {
    "sha": "$commit_sha",
    "short_sha": "$short_sha",
    "created": "$created",
    "os": "$(uname -s)",
    "arch": "$(uname -m)",
    "runs": $RUNS,
    "warmup": $WARMUP,
    "run_id": "${GITHUB_RUN_ID:-local}",
    "run_number": "${GITHUB_RUN_NUMBER:-0}",
    "rts_version": "$($RTS_EXE --version 2>/dev/null || echo unknown)"
  },
  "benches": $benches_json
}
JSON
