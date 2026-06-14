#!/usr/bin/env bash
# Roda cada fixture em tests/cross-runtime/ via Bun + Node + RTS em
# paralelo, compara stdouts. Retorna 0 se todos batem; 1 se houve
# divergencia.
#
# Paralelismo em 2 niveis:
#   1. Por fixture: invoca os 3 runtimes simultaneamente (background +
#      wait) — reduz latencia individual de N*M para max(N,M).
#   2. Entre fixtures: xargs -P JOBS processa N fixtures por vez.
#
# Variaveis:
#   RTS_BIN          - path do binario rts
#   REPORT_FILE      - JSON de saida
#   FIXTURES_DIR     - dir das fixtures (default: tests/cross-runtime)
#   JOBS             - paralelismo entre fixtures (default: nproc/2 ou 4)
#   FIXTURE_TIMEOUT  - segundos antes de matar fixture travada (default: 10)

set -uo pipefail

FIXTURES_DIR="${FIXTURES_DIR:-tests/cross-runtime}"
REPORT_FILE="${REPORT_FILE:-/tmp/cross_runtime_report.json}"

# Detecta numero de CPUs para paralelismo. Cada fixture usa 3 processos
# (Bun + Node + RTS) entao queremos ~nproc/3 workers para nao saturar.
if [ -z "${JOBS:-}" ]; then
    if command -v nproc >/dev/null 2>&1; then
        cpus=$(nproc)
    else
        cpus=8
    fi
    JOBS=$(( cpus / 2 ))
    [ "$JOBS" -lt 2 ] && JOBS=2
    [ "$JOBS" -gt 16 ] && JOBS=16
fi

# Auto-detect RTS bin
if [ -z "${RTS_BIN:-}" ]; then
    if [ -x "target/release/rts.exe" ]; then
        RTS_BIN="target/release/rts.exe"
    elif [ -x "target/release/rts" ]; then
        RTS_BIN="target/release/rts"
    elif command -v rts >/dev/null 2>&1; then
        RTS_BIN="rts"
    else
        echo "error: nao encontrei binario rts (tente cargo build --release)" >&2
        exit 2
    fi
fi

if ! command -v bun >/dev/null 2>&1; then echo "error: bun nao instalado" >&2; exit 2; fi
if ! command -v node >/dev/null 2>&1; then echo "error: node nao instalado" >&2; exit 2; fi

# Resolve interpretador JSON (jq > python > node)
if command -v jq >/dev/null 2>&1; then
    JSON_TOOL=jq
elif command -v python3 >/dev/null 2>&1; then
    JSON_TOOL=python3
elif command -v python >/dev/null 2>&1; then
    JSON_TOOL=python
else
    JSON_TOOL=node
fi
export JSON_TOOL

# Cores (skip se nao TTY)
if [ -t 1 ]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; BOLD='\033[1m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; BOLD=''; NC=''
fi
export RED GREEN YELLOW NC

# Padroes que NAO podem aparecer em fixtures cross-runtime.
export RTS_ONLY_PATTERNS='(import[[:space:]]+\{[^}]*\}[[:space:]]+from[[:space:]]+["'"'"'"]rts["'"'"'"]|^|[^A-Za-z_])(JSON5|Bun|Deno|process)([^A-Za-z_]|$)'
export RTS_BIN

# Dir temporario para outputs intermediarios
TMP=$(mktemp -d -t crossrt-XXXXXX)
trap 'rm -rf "$TMP"' EXIT
export TMP

# Worker — processa 1 fixture.
# Argumento: caminho da fixture
# Saida: linha "STATUS|NAME" em stdout (consumida por main loop) e
# arquivo TMP/$(basename fixture).json com a entrada JSON completa.
process_fixture() {
    local fixture="$1"
    local name=$(basename "$fixture" .ts)
    local out_json="$TMP/${name}.json"
    local out_status="$TMP/${name}.status"

    # Reject early
    local stripped=$(sed 's|//.*$||' "$fixture")
    if printf '%s\n' "$stripped" | grep -qE "$RTS_ONLY_PATTERNS"; then
        echo "rejected" > "$out_status"
        echo "!|$name"
        return
    fi

    # Roda os 3 runtimes em paralelo com timeout (evita loop infinito
    # travar CI — ex: JSON.stringify de objeto circular em RTS sem
    # detecao de ciclo). 10s eh generoso demais pra qualquer fixture
    # legitima (que rodam em <1s).
    local TO="${FIXTURE_TIMEOUT:-10}"
    timeout "$TO" bun "$fixture" >"$TMP/${name}.bun" 2>/dev/null &
    local bun_pid=$!
    timeout "$TO" node "$fixture" >"$TMP/${name}.node" 2>/dev/null &
    local node_pid=$!
    timeout "$TO" "$RTS_BIN" run-new "$fixture" >"$TMP/${name}.rts" 2>/dev/null &
    local rts_pid=$!

    wait $bun_pid; local bun_rc=$?
    wait $node_pid; local node_rc=$?
    wait $rts_pid; local rts_rc=$?

    local bun_out=$(cat "$TMP/${name}.bun")
    local node_out=$(cat "$TMP/${name}.node")
    local rts_out=$(cat "$TMP/${name}.rts")
    [ $bun_rc -ne 0 ] && bun_out="__RUNTIME_ERROR__"
    [ $node_rc -ne 0 ] && node_out="__RUNTIME_ERROR__"
    [ $rts_rc -ne 0 ] && rts_out="__RUNTIME_ERROR__"

    rm -f "$TMP/${name}.bun" "$TMP/${name}.node" "$TMP/${name}.rts"

    local status
    if [ "$bun_out" != "$node_out" ]; then
        status="bun_node_diverge"
    elif [ "$rts_out" = "__RUNTIME_ERROR__" ]; then
        status="rts_error"
    elif [ "$rts_out" != "$bun_out" ]; then
        status="rts_diverge"
    else
        status="pass"
    fi
    echo "$status" > "$out_status"

    # JSON entry
    case "$JSON_TOOL" in
        jq)
            jq -n --arg name "$name" --arg status "$status" \
                  --arg bun "$bun_out" --arg node "$node_out" --arg rts "$rts_out" \
                  '{name:$name,status:$status,bun:$bun,node:$node,rts:$rts}' > "$out_json"
            ;;
        python|python3)
            NAME="$name" STATUS="$status" BUN="$bun_out" NODE="$node_out" RTS="$rts_out" \
                "$JSON_TOOL" -c 'import json,os; print(json.dumps({"name":os.environ["NAME"],"status":os.environ["STATUS"],"bun":os.environ["BUN"],"node":os.environ["NODE"],"rts":os.environ["RTS"]}))' > "$out_json"
            ;;
        node)
            NAME="$name" STATUS="$status" BUN="$bun_out" NODE="$node_out" RTS="$rts_out" \
                node -e 'console.log(JSON.stringify({name:process.env.NAME,status:process.env.STATUS,bun:process.env.BUN,node:process.env.NODE,rts:process.env.RTS}))' > "$out_json"
            ;;
    esac

    # Marker pra main loop reportar progresso
    case "$status" in
        pass) echo "ok|$name" ;;
        rts_diverge) echo "xx|$name (RTS difere de Bun/Node)" ;;
        bun_node_diverge) echo "~|$name (Bun != Node - skip)" ;;
        rts_error) echo "xx|$name (RTS error)" ;;
    esac
}
export -f process_fixture

# Coleta lista de fixtures. Aceita organizacao plana
# (`tests/cross-runtime/*.ts`) OU em subpastas por categoria
# (`tests/cross-runtime/<categoria>/*.ts`). `find` descend mas pula
# `support/` (helpers nao executaveis isoladamente).
shopt -s nullglob
mapfile -t FIXTURES < <(
    find "$FIXTURES_DIR" -type f -name '*.ts' \
        ! -path "$FIXTURES_DIR/support/*" \
        ! -path "*/support/*" \
        | sort
)
TOTAL=${#FIXTURES[@]}

if [ "$TOTAL" -eq 0 ]; then
    echo "Nenhum fixture em $FIXTURES_DIR" >&2
    exit 0
fi

echo -e "${BOLD}Rodando $TOTAL fixtures com paralelismo=$JOBS...${NC}"
start_time=$(date +%s)

# Dispara workers via xargs -P. Cada worker imprime "TAG|NAME" na stdout.
printf '%s\n' "${FIXTURES[@]}" | xargs -I{} -P "$JOBS" bash -c 'process_fixture "$@"' _ {} | while IFS='|' read -r tag rest; do
    case "$tag" in
        ok) echo -e "${GREEN}ok${NC} $rest" ;;
        xx) echo -e "${RED}xx${NC} $rest" ;;
        '~') echo -e "${YELLOW}~${NC} $rest" ;;
        '!') echo -e "${YELLOW}!${NC} $rest" ;;
    esac
done

elapsed=$(($(date +%s) - start_time))

# Conta status
pass=0; diverge_rts=0; diverge_bun_node=0; errors=0; rejected=0
for fixture in "${FIXTURES[@]}"; do
    name=$(basename "$fixture" .ts)
    s_file="$TMP/${name}.status"
    [ -f "$s_file" ] || continue
    s=$(cat "$s_file")
    case "$s" in
        pass) pass=$((pass + 1)) ;;
        rts_diverge) diverge_rts=$((diverge_rts + 1)) ;;
        bun_node_diverge) diverge_bun_node=$((diverge_bun_node + 1)) ;;
        rts_error) errors=$((errors + 1)) ;;
        rejected) rejected=$((rejected + 1)) ;;
    esac
done

# Monta JSON report final agrupando entries por ordem alfabetica de nome
{
    printf '{"results":[\n'
    first=1
    for fixture in "${FIXTURES[@]}"; do
        name=$(basename "$fixture" .ts)
        entry_file="$TMP/${name}.json"
        [ -f "$entry_file" ] || continue
        if [ $first -eq 1 ]; then
            first=0
        else
            printf ',\n'
        fi
        cat "$entry_file"
    done
    printf '\n],"summary":{"total":%d,"pass":%d,"rts_diverge":%d,"bun_node_diverge":%d,"errors":%d,"rejected":%d}}\n' \
        "$TOTAL" "$pass" "$diverge_rts" "$diverge_bun_node" "$errors" "$rejected"
} > "$REPORT_FILE"

echo ""
echo -e "${BOLD}Summary:${NC} (tempo: ${elapsed}s, $JOBS jobs paralelos)"
echo "  Total:               $TOTAL"
echo -e "  ${GREEN}Pass:                $pass${NC}"
echo -e "  ${RED}RTS divergente:      $diverge_rts${NC}"
echo -e "  ${YELLOW}Bun/Node divergem:   $diverge_bun_node${NC}"
echo -e "  ${RED}RTS runtime error:   $errors${NC}"
echo -e "  ${YELLOW}Rejeitados (RTS-only): $rejected${NC}"
echo ""
echo "Report: $REPORT_FILE"

if [ $diverge_rts -gt 0 ] || [ $errors -gt 0 ]; then
    exit 1
fi
exit 0
