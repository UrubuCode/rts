#!/usr/bin/env bash
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1
# Run every tests/*.test.ts through the NEW engine (run-new), bucket pass/fail.
# A file "runs" if exit 0 (compiled + executed without an Unsupported bail).
RTS=target/release/rts.exe
pass=0; fail=0
: > /tmp/new_fail.txt
: > /tmp/new_pass.txt
for f in tests/*.test.ts; do
  # Per-file 20s cap: a single hanging fixture (async/thread stub without a real
  # event loop, #207) must not block the whole measure. Timeout → exit 124 → bailed.
  out=$(timeout 20 "$RTS" run-new "$f" 2>&1)
  code=$?
  if [ $code -eq 0 ]; then
    pass=$((pass+1)); echo "$f" >> /tmp/new_pass.txt
  else
    fail=$((fail+1))
    reason=$(printf '%s' "$out" | tr '\r' '\n' | grep -m1 -oiE 'unsupported in the numeric subset:.*' | sed 's/unsupported in the numeric subset: //I')
    [ -z "$reason" ] && reason=$(printf '%s' "$out" | tr '\r' '\n' | grep -m1 -iE 'error|panic' | head -c 120)
    printf '%s\t%s\n' "$f" "$reason" >> /tmp/new_fail.txt
  fi
done
total=$((pass+fail))
echo "=== NEW ENGINE rts:test coverage: $pass/$total ran (exit 0), $fail bailed ==="
echo "=== TOP FAILURE CLUSTERS ==="
cut -f2 /tmp/new_fail.txt | sed -E 's/`[^`]*`/`X`/g; s/[0-9]+/N/g' | sort | uniq -c | sort -rn | head -30
