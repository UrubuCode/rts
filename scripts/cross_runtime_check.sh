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

# Os tres runtimes correm no MESMO fuso, e esse fuso e UTC.
#
# O RTS nao tem base de fusos e nao vai ter — a decisao esta escrita em
# `crates/rts-core/src/entry/date/mod.rs` e o que ela diz e que a hora local
# deste runtime E UTC. Sem esta linha, uma maquina em -03:00 faz
# `new Date(2024, 0, 15, 10, 30)` divergir por tres horas em TODA a pasta
# `date/`, e o que se mede e o fuso do medidor e nao o motor. O CI corre em
# UTC, portanto isto tambem e o que alinha a medicao local com a do CI.
#
# A divergencia real que isto NAO esconde: um programa que pede hora local
# noutro fuso continua a receber UTC. Isso e a recusa documentada, e esta no
# README desta pasta em vez de virar uma falha diferente todos os dias.
export TZ=UTC

# Resolve interpretador JSON (jq > python > node). `command -v` nao basta no
# Windows: o alias `python` da Microsoft Store EXISTE mas so abre a loja (exit
# != 0) — o report saia com todas as entries vazias. Testa EXECUCAO real.
if command -v jq >/dev/null 2>&1; then
    JSON_TOOL=jq
elif python3 -c 1 >/dev/null 2>&1; then
    JSON_TOOL=python3
elif python -c 1 >/dev/null 2>&1; then
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
    # stderr NAO vai mais pra /dev/null: e o diagnostico real (mensagem do
    # erro, stack, arquivo:linha). Antes o exit!=0 virava a string sentinela
    # `__RUNTIME_ERROR__` no report — todo erro ficava indistinguivel de
    # qualquer outro, e a causa se perdia. Agora o status vem do EXIT CODE
    # (sinal correto) e o texto do erro vai pro report em campo proprio.
    timeout "$TO" bun "$fixture" >"$TMP/${name}.bun" 2>"$TMP/${name}.bun.err" &
    local bun_pid=$!
    timeout "$TO" node "$fixture" >"$TMP/${name}.node" 2>"$TMP/${name}.node.err" &
    local node_pid=$!
    timeout "$TO" "$RTS_BIN" run "$fixture" >"$TMP/${name}.rts" 2>"$TMP/${name}.rts.err" &
    local rts_pid=$!

    wait $bun_pid; local bun_rc=$?
    wait $node_pid; local node_rc=$?
    wait $rts_pid; local rts_rc=$?

    local bun_out=$(cat "$TMP/${name}.bun")
    local node_out=$(cat "$TMP/${name}.node")
    local rts_out=$(cat "$TMP/${name}.rts")

    # Diagnostico: so guardado quando o runtime falhou (stderr de um run OK e
    # ruido — warning de deprecation etc). Truncado pra nao inflar o report;
    # `timeout` mata com 124 e nao escreve nada, entao rotula explicito.
    local ERRMAX="${ERROR_TEXT_MAX:-4000}"
    local bun_err="" node_err="" rts_err=""
    [ $bun_rc -ne 0 ] && bun_err=$(head -c "$ERRMAX" "$TMP/${name}.bun.err")
    [ $node_rc -ne 0 ] && node_err=$(head -c "$ERRMAX" "$TMP/${name}.node.err")
    [ $rts_rc -ne 0 ] && rts_err=$(head -c "$ERRMAX" "$TMP/${name}.rts.err")
    [ $bun_rc -eq 124 ] && bun_err="timeout apos ${TO}s${bun_err:+$'\n'}$bun_err"
    [ $node_rc -eq 124 ] && node_err="timeout apos ${TO}s${node_err:+$'\n'}$node_err"
    [ $rts_rc -eq 124 ] && rts_err="timeout apos ${TO}s${rts_err:+$'\n'}$rts_err"

    rm -f "$TMP/${name}.bun" "$TMP/${name}.node" "$TMP/${name}.rts" \
          "$TMP/${name}.bun.err" "$TMP/${name}.node.err" "$TMP/${name}.rts.err"

    # Classificacao pelo EXIT CODE (nao mais por comparar contra a sentinela).
    # Um runtime que falhou nao tem stdout comparavel; a comparacao so vale
    # quando os dois lados terminaram com sucesso.
    local status
    if [ $bun_rc -eq 0 ] && [ $node_rc -ne 0 ]; then
        # O NODE e que nao consegue correr o ficheiro — quase sempre TypeScript
        # que ele nao sabe apagar (`constructor(public name: string)` nao e
        # sintaxe apagavel). Isto NAO e desacordo entre runtimes: e uma
        # limitacao de um deles, e a politica escrita no README desta pasta ja
        # diz que o bun e canonico. Antes, estes 18 ficheiros caiam em
        # `bun_node_diverge` e o RTS nunca chegava a ser medido neles — oito
        # falhas reais ficavam escondidas atras de um problema do medidor.
        if [ $rts_rc -ne 0 ]; then
            status="rts_error"
        elif [ "$rts_out" != "$bun_out" ]; then
            status="rts_diverge"
        else
            status="pass"
        fi
    elif [ $bun_rc -ne $node_rc ] || { [ $bun_rc -eq 0 ] && [ "$bun_out" != "$node_out" ]; }; then
        status="bun_node_diverge"
    elif [ $rts_rc -ne 0 ]; then
        status="rts_error"
    elif [ $bun_rc -ne 0 ]; then
        # Bun e Node falharam IGUAL (fixture que erra de proposito) e o RTS
        # saiu 0 — divergencia real: o RTS deveria ter falhado tambem.
        status="rts_diverge"
    elif [ "$rts_out" != "$bun_out" ]; then
        status="rts_diverge"
    else
        status="pass"
    fi

    # Um defeito de TERCEIROS, declarado na propria fixture.
    #
    # A marca vive no ficheiro e nao numa lista central de propósito: uma lista
    # e um sitio onde uma falha nossa se pode esconder, enquanto uma marca ao
    # lado do programa obriga quem a poe a escrever a razao onde ela se le. E
    # so vale enquanto o ficheiro FALHA — se passar, e `pass` como qualquer
    # outro, portanto a marca nao pode manter verde uma coisa que quebrou.
    #
    # O criterio para a por e o mesmo que a primeira cumpriu: a causa esta
    # provada FORA deste repositorio, por bisseccao de uma variavel — mudar so
    # a dependencia faz o ficheiro passar, sem tocar em nada nosso.
    if [ "$status" != "pass" ] && grep -q "cross-runtime:upstream-defect" "$fixture" 2>/dev/null; then
        status="upstream_defect"
    fi
    echo "$status" > "$out_status"

    # JSON entry
    case "$JSON_TOOL" in
        jq)
            jq -n --arg name "$name" --arg status "$status" \
                  --arg bun "$bun_out" --arg node "$node_out" --arg rts "$rts_out" \
                  --arg bun_error "$bun_err" --arg node_error "$node_err" --arg rts_error "$rts_err" \
                  --argjson bun_exit "$bun_rc" --argjson node_exit "$node_rc" --argjson rts_exit "$rts_rc" \
                  '{name:$name,status:$status,bun:$bun,node:$node,rts:$rts,
                    bun_exit:$bun_exit,node_exit:$node_exit,rts_exit:$rts_exit,
                    bun_error:$bun_error,node_error:$node_error,rts_error:$rts_error}' > "$out_json"
            ;;
        python|python3)
            NAME="$name" STATUS="$status" BUN="$bun_out" NODE="$node_out" RTS="$rts_out" \
            BUN_ERR="$bun_err" NODE_ERR="$node_err" RTS_ERR="$rts_err" \
            BUN_RC="$bun_rc" NODE_RC="$node_rc" RTS_RC="$rts_rc" \
                "$JSON_TOOL" -c 'import json,os; e=os.environ; print(json.dumps({"name":e["NAME"],"status":e["STATUS"],"bun":e["BUN"],"node":e["NODE"],"rts":e["RTS"],"bun_exit":int(e["BUN_RC"]),"node_exit":int(e["NODE_RC"]),"rts_exit":int(e["RTS_RC"]),"bun_error":e["BUN_ERR"],"node_error":e["NODE_ERR"],"rts_error":e["RTS_ERR"]}))' > "$out_json"
            ;;
        node)
            NAME="$name" STATUS="$status" BUN="$bun_out" NODE="$node_out" RTS="$rts_out" \
            BUN_ERR="$bun_err" NODE_ERR="$node_err" RTS_ERR="$rts_err" \
            BUN_RC="$bun_rc" NODE_RC="$node_rc" RTS_RC="$rts_rc" \
                node -e 'const e=process.env;console.log(JSON.stringify({name:e.NAME,status:e.STATUS,bun:e.BUN,node:e.NODE,rts:e.RTS,bun_exit:+e.BUN_RC,node_exit:+e.NODE_RC,rts_exit:+e.RTS_RC,bun_error:e.BUN_ERR,node_error:e.NODE_ERR,rts_error:e.RTS_ERR}))' > "$out_json"
            ;;
    esac

    # Marker pra main loop reportar progresso
    case "$status" in
        pass) echo "ok|$name" ;;
        rts_diverge) echo "xx|$name (RTS difere de Bun/Node)" ;;
        bun_node_diverge) echo "~|$name (Bun != Node - skip)" ;;
        upstream_defect) echo "~|$name (defeito de dependencia - skip)" ;;
        rts_error)
            # Primeira linha util do stderr direto no progresso — o motivo do
            # erro aparece durante a corrida, sem abrir o report. `|` vira `/`
            # (o main loop separa campos por `|`).
            local first=$(printf '%s' "$rts_err" | grep -m1 -v '^[[:space:]]*$' | tr '|' '/' | head -c 160)
            echo "xx|$name (RTS error rc=$rts_rc${first:+: $first})"
            ;;
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
pass=0; diverge_rts=0; diverge_bun_node=0; errors=0; rejected=0; upstream=0
for fixture in "${FIXTURES[@]}"; do
    name=$(basename "$fixture" .ts)
    s_file="$TMP/${name}.status"
    [ -f "$s_file" ] || continue
    s=$(cat "$s_file")
    case "$s" in
        pass) pass=$((pass + 1)) ;;
        rts_diverge) diverge_rts=$((diverge_rts + 1)) ;;
        bun_node_diverge) diverge_bun_node=$((diverge_bun_node + 1)) ;;
        upstream_defect) upstream=$((upstream + 1)) ;;
        rts_error) errors=$((errors + 1)) ;;
        rejected) rejected=$((rejected + 1)) ;;
    esac
done

# Monta JSON report final agrupando entries por ordem alfabetica de nome
# O denominador da percentagem e o ALCANCAVEL, nao o total. Calculado ANTES do
# bloco que escreve o relatorio, porque o JSON tambem o carrega — um numero que
# so existisse para o ecra sairia do ficheiro como zero.
#
# Um ficheiro em que o Bun e o Node se respondem coisas diferentes nao tem
# resposta certa que este arnes possa nomear — ele recusa eleger um deles, de
# proposito. Conta-lo contra nos e cobrar uma resposta que ninguem sabe qual e:
# a percentagem ficaria com um tecto abaixo de 100 que nenhum trabalho podia
# levantar, e um numero cujo maximo nao e atingivel deixa de dizer o que falta.
#
# O total continua impresso ao lado, e os divergentes continuam CONTADOS e
# listados. O que muda e so o que a fraccao pergunta: "de tudo o que tem uma
# resposta comparavel, quanto e que acertamos".
reachable=$((TOTAL - diverge_bun_node - upstream))
if [ "$reachable" -gt 0 ]; then
    share=$(awk -v p="$pass" -v r="$reachable" 'BEGIN { printf "%.1f", p * 100 / r }')
else
    share="n/a"
fi

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
    printf '\n],"summary":{"total":%d,"pass":%d,"reachable":%d,"rts_diverge":%d,"bun_node_diverge":%d,"errors":%d,"rejected":%d,"upstream_defect":%d}}\n' \
        "$TOTAL" "$pass" "$reachable" "$diverge_rts" "$diverge_bun_node" "$errors" "$rejected" "$upstream"
} > "$REPORT_FILE"


echo ""
echo -e "${BOLD}Summary:${NC} (tempo: ${elapsed}s, $JOBS jobs paralelos)"
echo "  Total:               $TOTAL"
echo -e "  ${GREEN}Pass:                $pass${NC}"
echo -e "  ${BOLD}Alcancavel:          $reachable  (${share}% de $reachable)${NC}"
echo -e "  ${RED}RTS divergente:      $diverge_rts${NC}"
echo -e "  ${YELLOW}Bun/Node divergem:   $diverge_bun_node  (fora do denominador)${NC}"
echo -e "  ${YELLOW}Defeito de dependencia: $upstream  (fora do denominador)${NC}"
echo -e "  ${RED}RTS runtime error:   $errors${NC}"
echo -e "  ${YELLOW}Rejeitados (RTS-only): $rejected${NC}"
echo ""
echo "Report: $REPORT_FILE"

if [ $diverge_rts -gt 0 ] || [ $errors -gt 0 ]; then
    exit 1
fi
exit 0
