#!/usr/bin/env bash
# One process per `tests/*.test.ts`, one verdict per line: `<file>\tPASS|FAIL|TIMEOUT`.
#
# Why this exists beside `rts test`: the merge gate compares a suite PER FILE
# against a kept binary, and `rts test` over a directory answers one number. A
# net number cannot tell three gained from five gained against two lost, so the
# gate needs a file-keyed report from each binary and a join. This is the join's
# input.
#
# Usage:  RTS_BIN=target/baseline.exe bash scripts/suite_per_file.sh > base.tsv
#         RTS_BIN=target/release/rts.exe bash scripts/suite_per_file.sh > now.tsv
#         join base.tsv now.tsv | awk '$2=="PASS" && $3!="PASS"'      # LOST
set -u
RTS_BIN="${RTS_BIN:-target/release/rts.exe}"
LIMIT="${LIMIT:-60}"
for f in tests/*.test.ts; do
  if timeout "$LIMIT" "$RTS_BIN" test "$f" >/dev/null 2>&1; then
    printf '%s\tPASS\n' "$f"
  else
    code=$?
    if [ "$code" -eq 124 ]; then printf '%s\tTIMEOUT\n' "$f"; else printf '%s\tFAIL\n' "$f"; fi
  fi
done
