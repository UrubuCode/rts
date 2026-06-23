#!/usr/bin/env bash
# Real correctness: run each fixture via run-new, count ✗ failing tests.
# rts:test writes ✓/✗ to stderr, so capture 2>&1.
RTS=target/release/rts.exe
files=0; clean=0; withfail=0; bail=0
: > /tmp/correct_fail.txt
for f in tests/*.test.ts; do
  files=$((files+1))
  out=$(timeout 20 "$RTS" run-new "$f" 2>&1)
  code=$?
  if [ $code -ne 0 ]; then
    bail=$((bail+1)); echo "$f	BAIL" >> /tmp/correct_fail.txt; continue
  fi
  nfail=$(printf '%s' "$out" | grep -c '✗')
  if [ "$nfail" -eq 0 ]; then
    clean=$((clean+1))
  else
    withfail=$((withfail+1)); echo "$f	$nfail✗" >> /tmp/correct_fail.txt
  fi
done
echo "=== CORRECTNESS: $clean/$files clean-pass | $withfail had ✗ | $bail bailed (exit!=0) ==="
echo "--- files with ✗ (exit 0 but wrong output) ---"
grep -v 'BAIL' /tmp/correct_fail.txt | head -60
