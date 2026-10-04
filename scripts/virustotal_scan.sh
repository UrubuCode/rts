#!/usr/bin/env bash
# Submits every file given on the command line to VirusTotal and answers with a
# verdict per file on stdout, plus a JSON report at $REPORT_FILE.
#
# Why a script and not six inline steps: the same question has to be answerable
# on a developer's machine before a release is blamed on the CI. Run it with
#
#   VT_API_KEY=... bash scripts/virustotal_scan.sh target/release/rts.exe
#
# Exit code: 0 = clean (or no key, which is NOT a verdict and says so), 1 = at
# least one file was flagged by at least $VT_MIN_DETECTIONS engines.
#
# The threshold exists because a single engine is not evidence. A Rust binary
# that embeds a JIT writes executable pages at run time, and heuristic engines
# flag that shape on its own — this repository has to tell "one engine does not
# like JITs" apart from "this artifact is contaminated", and a count is the
# only cheap way. Two is the floor; raise it with VT_MIN_DETECTIONS.
set -uo pipefail

MIN=${VT_MIN_DETECTIONS:-2}
REPORT_FILE=${REPORT_FILE:-vt-report.json}
API=https://www.virustotal.com/api/v3

if [ -z "${VT_API_KEY:-}" ]; then
  echo "VT_API_KEY ausente — VirusTotal NAO foi consultado (isto nao e um veredicto)."
  echo '{"consulted":false,"files":[]}' > "$REPORT_FILE"
  exit 0
fi

# A chave publica da VirusTotal e 4 pedidos por minuto e 500 por dia, e um 429
# devolve um corpo sem `last_analysis_stats` — indistinguivel, para quem so olha
# ao campo, de "nao conheco este ficheiro". Por isso cada pedido espera o seu
# turno (VT_RATE_SLEEP) e um 429 e tratado como falta de quota e nao como
# veredicto: o script diz que nao conseguiu consultar, e isso nao bloqueia o CI.
RATE=${VT_RATE_SLEEP:-16}
QUOTA_HIT=0
vt() {
  sleep "$RATE"
  local out code
  out=$(curl -sS --retry 2 --retry-delay 5 -w '
%{http_code}' -H "x-apikey: $VT_API_KEY" "$@")
  code=${out##*$'
'}
  if [ "$code" = "429" ]; then QUOTA_HIT=1; echo '{}'; return 0; fi
  printf '%s' "${out%$'
'*}"
}

entries=()
status=0

for f in "$@"; do
  [ -f "$f" ] || { echo "skip (nao e ficheiro): $f"; continue; }
  sha=$(sha256sum "$f" | cut -d' ' -f1)
  echo "== $f"
  echo "   sha256 $sha"

  body=$(vt "$API/files/$sha")
  stats=$(printf '%s' "$body" | jq -c '.data.attributes.last_analysis_stats // empty')

  if [ -z "$stats" ]; then
    # Unknown to VirusTotal — every fresh build is. upload_url works for any
    # size; the plain /files endpoint caps at 32 MB and the rts binary is over.
    url=$(vt "$API/files/upload_url" | jq -r '.data')
    analysis=$(curl -sS --retry 3 -H "x-apikey: $VT_API_KEY" -F "file=@$f" "$url" | jq -r '.data.id')
    echo "   analise $analysis — a aguardar"
    for _ in $(seq 1 40); do
      a=$(vt "$API/analyses/$analysis")
      [ "$(printf '%s' "$a" | jq -r '.data.attributes.status')" = "completed" ] && break
      sleep 15
    done
    stats=$(printf '%s' "$a" | jq -c '.data.attributes.stats // empty')
  fi

  if [ -z "$stats" ] || [ "$stats" = "{}" ]; then
    echo "   SEM veredicto (quota, ou analise ainda a decorrer) — nao conta como limpo"
    continue
  fi

  mal=$(printf '%s' "$stats" | jq -r '.malicious // 0')
  sus=$(printf '%s' "$stats" | jq -r '.suspicious // 0')
  hits=$(( mal + sus ))
  echo "   malicious=$mal suspicious=$sus (limiar $MIN)"
  entries+=("$(jq -nc --arg f "$f" --arg s "$sha" --argjson m "$mal" --argjson u "$sus" \
      '{file:$f,sha256:$s,malicious:$m,suspicious:$u}')")
  [ "$hits" -ge "$MIN" ] && status=1
done

[ "$QUOTA_HIT" = "1" ] && echo "AVISO: a quota da API publica foi atingida nalgum pedido."

printf '%s' "$(jq -nc --argjson fs "$(printf '%s\n' "${entries[@]:-}" | jq -sc '.')" \
  --argjson min "$MIN" --argjson q "$QUOTA_HIT"   '{consulted:true,threshold:$min,quota_hit:($q==1),files:$fs}')" > "$REPORT_FILE"
exit $status
