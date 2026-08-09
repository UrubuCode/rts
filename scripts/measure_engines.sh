#!/usr/bin/env bash
# Both engines over the same corpus, one process per file, same timeout.
#
# Why one script and not two: the number that matters is the DIFFERENCE, and two
# scripts run months apart against different corpora is how a comparison stops
# meaning anything. This runs them back to back over `tests/*.test.ts`.
#
# The rulers are not identical and that is stated rather than smoothed over:
#   old  `rts test <file>`  — runs the file AND compares stdout against a fixture
#                             where one exists; exit 0 means everything passed.
#   new  `suite_run <file>` — runs the file and reads what `rts:test` recorded;
#                             a file registering nothing is `empty`, not `ok`.
# Both require "ran and nothing failed", which is what makes the counts
# comparable; the old one can also fail on a stdout mismatch the new never sees.
#
# Build first: cargo build --release -p rts-cli -p rts-host-rwk --example suite_run
cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1

OLD=target/release/rts.exe
NEW=target/release/examples/suite_run.exe
LIMIT=25

old_ok=0; old_bad=0; new_ok=0; new_bad=0; total=0
: > /tmp/engines_old.txt
: > /tmp/engines_new.txt

for f in tests/*.test.ts; do
  total=$((total+1))

  timeout $LIMIT "$OLD" test "$f" >/dev/null 2>&1
  if [ $? -eq 0 ]; then old_ok=$((old_ok+1)); echo "ok $f" >> /tmp/engines_old.txt
  else old_bad=$((old_bad+1)); echo "not-ok $f" >> /tmp/engines_old.txt; fi

  line=$(timeout $LIMIT "$NEW" "$f" 2>/dev/null | head -1)
  case "$line" in
    ok\ *) new_ok=$((new_ok+1)); echo "ok $f" >> /tmp/engines_new.txt;;
    *) new_bad=$((new_bad+1)); echo "not-ok $f" >> /tmp/engines_new.txt;;
  esac
done

echo "corpus $total"
echo "old $old_ok pass, $old_bad not"
echo "new $new_ok pass, $new_bad not"
echo "--- files only the OLD engine passes ---"
comm -23 <(grep '^ok ' /tmp/engines_old.txt | awk '{print $2}' | LC_ALL=C sort) \
         <(grep '^ok ' /tmp/engines_new.txt | awk '{print $2}' | LC_ALL=C sort) | wc -l
echo "--- files only the NEW engine passes ---"
comm -13 <(grep '^ok ' /tmp/engines_old.txt | awk '{print $2}' | LC_ALL=C sort) \
         <(grep '^ok ' /tmp/engines_new.txt | awk '{print $2}' | LC_ALL=C sort) | wc -l
