#!/usr/bin/env bash
# Roda cada fixture em tests/cross-runtime/ via Bun + Node + RTS,
# compara stdouts. Retorna 0 se todos batem; 1 se houve divergencia.
#
# Saida JSON em $REPORT_FILE (default: /tmp/cross_runtime_report.json)
# para ser consumida pelo workflow CI.
#
# Variaveis de ambiente:
#   RTS_BIN        - path para o binario rts (default: target/release/rts.exe ou rts)
#   REPORT_FILE    - arquivo JSON de saida
#   FIXTURES_DIR   - dir das fixtures (default: tests/cross-runtime)

set -uo pipefail

FIXTURES_DIR="${FIXTURES_DIR:-tests/cross-runtime}"
REPORT_FILE="${REPORT_FILE:-/tmp/cross_runtime_report.json}"

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

# Sanity check
if ! command -v bun >/dev/null 2>&1; then
    echo "error: bun nao instalado" >&2
    exit 2
fi
if ! command -v node >/dev/null 2>&1; then
    echo "error: node nao instalado" >&2
    exit 2
fi

# Cores (skip se nao TTY)
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[0;33m'
    BOLD='\033[1m'
    NC='\033[0m'
else
    RED='' GREEN='' YELLOW='' BOLD='' NC=''
fi

# JSON report
echo '{"results":[' > "$REPORT_FILE"
first=1

total=0
pass=0
diverge_rts=0
diverge_bun_node=0
errors=0
rejected=0

# Padroes que NAO podem aparecer em fixtures cross-runtime (sao RTS-only
# ou runtime-specific). Se aparecer, fixture eh rejeitada antes de rodar.
RTS_ONLY_PATTERNS='(import[[:space:]]+\{[^}]*\}[[:space:]]+from[[:space:]]+["'"'"'"]rts["'"'"'"]|^|[^A-Za-z_])(JSON5|Bun|Deno|process)([^A-Za-z_]|$)'

# Encontra todos os .ts na pasta
shopt -s nullglob
for fixture in "$FIXTURES_DIR"/*.ts; do
    total=$((total + 1))
    name=$(basename "$fixture" .ts)

    # Reject early se a fixture contem APIs RTS-only ou runtime-specific.
    # Ignora linhas que comecam com `//` (comentarios). Faz check via codigo
    # com comentarios stripped.
    stripped=$(sed 's|//.*$||' "$fixture")
    if printf '%s\n' "$stripped" | grep -qE "$RTS_ONLY_PATTERNS"; then
        rejected=$((rejected + 1))
        echo -e "${YELLOW}!${NC} $name (rejeitado: usa API RTS-only/runtime-specific - mover para tests/*.test.ts)"
        continue
    fi

    # Cada runtime - captura stdout, ignora stderr (warnings de TS).
    bun_out=$(bun "$fixture" 2>/dev/null || echo "__RUNTIME_ERROR__")
    node_out=$(node "$fixture" 2>/dev/null || echo "__RUNTIME_ERROR__")
    rts_out=$("$RTS_BIN" run "$fixture" 2>/dev/null || echo "__RUNTIME_ERROR__")

    # Categoriza
    status="pass"
    if [ "$bun_out" != "$node_out" ]; then
        status="bun_node_diverge"
        diverge_bun_node=$((diverge_bun_node + 1))
    elif [ "$rts_out" = "__RUNTIME_ERROR__" ]; then
        status="rts_error"
        errors=$((errors + 1))
    elif [ "$rts_out" != "$bun_out" ]; then
        status="rts_diverge"
        diverge_rts=$((diverge_rts + 1))
    else
        pass=$((pass + 1))
    fi

    # Print humano
    case "$status" in
        pass) echo -e "${GREEN}ok${NC} $name" ;;
        rts_diverge) echo -e "${RED}xx${NC} $name (RTS difere de Bun/Node)" ;;
        bun_node_diverge) echo -e "${YELLOW}~${NC} $name (Bun != Node - skip)" ;;
        rts_error) echo -e "${RED}xx${NC} $name (RTS error)" ;;
    esac

    # JSON entry: prefere jq (mais rapido), fallback python (sempre tem),
    # ultimo recurso node. Tab/newline/control chars sao escapados certinho.
    if command -v jq >/dev/null 2>&1; then
        entry=$(jq -n \
            --arg name "$name" \
            --arg status "$status" \
            --arg bun "$bun_out" \
            --arg node "$node_out" \
            --arg rts "$rts_out" \
            '{name:$name,status:$status,bun:$bun,node:$node,rts:$rts}')
    elif command -v python3 >/dev/null 2>&1 || command -v python >/dev/null 2>&1; then
        py=$(command -v python3 || command -v python)
        entry=$(NAME="$name" STATUS="$status" BUN="$bun_out" NODE="$node_out" RTS="$rts_out" \
            "$py" -c 'import json,os; print(json.dumps({"name":os.environ["NAME"],"status":os.environ["STATUS"],"bun":os.environ["BUN"],"node":os.environ["NODE"],"rts":os.environ["RTS"]}))')
    else
        entry=$(NAME="$name" STATUS="$status" BUN="$bun_out" NODE="$node_out" RTS="$rts_out" \
            node -e 'console.log(JSON.stringify({name:process.env.NAME,status:process.env.STATUS,bun:process.env.BUN,node:process.env.NODE,rts:process.env.RTS}))')
    fi

    if [ $first -eq 1 ]; then
        first=0
    else
        echo "," >> "$REPORT_FILE"
    fi
    printf '%s' "$entry" >> "$REPORT_FILE"
done

echo "" >> "$REPORT_FILE"
echo "],\"summary\":{\"total\":$total,\"pass\":$pass,\"rts_diverge\":$diverge_rts,\"bun_node_diverge\":$diverge_bun_node,\"errors\":$errors,\"rejected\":$rejected}}" >> "$REPORT_FILE"

echo ""
echo -e "${BOLD}Summary:${NC}"
echo "  Total:               $total"
echo -e "  ${GREEN}Pass:                $pass${NC}"
echo -e "  ${RED}RTS divergente:      $diverge_rts${NC}"
echo -e "  ${YELLOW}Bun/Node divergem:   $diverge_bun_node${NC}"
echo -e "  ${RED}RTS runtime error:   $errors${NC}"
echo -e "  ${YELLOW}Rejeitados (RTS-only): $rejected${NC}"
echo ""
echo "Report: $REPORT_FILE"

# Exit code: 1 se RTS divergiu OU teve erro
if [ $diverge_rts -gt 0 ] || [ $errors -gt 0 ]; then
    exit 1
fi
exit 0
