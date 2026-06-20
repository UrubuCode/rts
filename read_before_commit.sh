#!/usr/bin/env bash
# read_before_commit.sh — MANDATORY gate. Run it (and READ the whole output)
# before every commit that touches the engine `crates/rts-codegen-new/`.
#
# It encodes the binding rules from CLAUDE.md / .claude/rules/ so a commit cannot
# silently violate them:
#
#   [FLOOR]  build must compile (honesty+build floor — never lifts).
#   [DOCTRINE] the engine names ONLY the PRIMORDIAL classes. A direct mention of
#              a non-primordial class (Map/Set/Date/Symbol/…) OR of the
#              rts-shared / rts-std crates is a REGRESSION — everything
#              non-primordial resolves through the Registry, never hardcoded in
#              codegen. rts-shared/rts-std are NOT native/primitive.
#   [LAYOUT]  no source file > 500 lines — split it into a folder/subfolder of
#              cohesive submodules instead.
#   [WIP]     surface todo!()/unimplemented!() so none ship disguised as "done".
#
# HARD failures (forbidden deps/uses, broken build) exit non-zero — do NOT
# commit. The doctrine class-mention scan and the >500-line scan are REVIEW
# gates: read every entry and confirm it resolves via the Registry (bridge file)
# and is genuinely unavoidable, or fix it. Pre-existing debt is listed so it
# keeps shrinking, never grows.
#
# Usage:  bash read_before_commit.sh            # full gate
#         bash read_before_commit.sh --no-build # skip the cargo build step
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$ROOT/crates/rts-codegen-new"
SRC="$ENGINE/src"
CARGO="$ENGINE/Cargo.toml"
MAX_LINES=500
NO_BUILD=0
[ "${1:-}" = "--no-build" ] && NO_BUILD=1

red()  { printf '\033[31m%s\033[0m\n' "$*"; }
grn()  { printf '\033[32m%s\033[0m\n' "$*"; }
ylw()  { printf '\033[33m%s\033[0m\n' "$*"; }
hdr()  { printf '\n\033[1m== %s ==\033[0m\n' "$*"; }

HARD_FAIL=0

# Non-primordial classes the engine MUST NOT name directly. Primordials allowed
# (NOT in this list): String Object Array Function Promise Boolean Number Error
# TypeError RangeError ReferenceError SyntaxError URIError EvalError
# AggregateError. RegExp is reclassified primitive (it has `/re/` native syntax),
# so it is NOT forbidden here.
NONPRIMORDIAL='Map|Set|WeakMap|WeakSet|WeakRef|FinalizationRegistry|Date|Symbol|URL|URLSearchParams|BigInt|Intl|Proxy|Reflect|DataView|ArrayBuffer|SharedArrayBuffer|TextEncoder|TextDecoder|EventTarget|Headers|FormData|ReadableStream'

# ---------------------------------------------------------------------------
hdr "1/5  Forbidden crate dependencies (HARD)"
# Only real dependency lines (`name = { path = ... }`), never comment lines.
BADDEP=$(grep -nE '^[[:space:]]*(rts-shared|rts-std|rts-codegen-old)[[:space:]]*=' "$CARGO" || true)
if [ -n "$BADDEP" ]; then
  red "Cargo.toml depends on a non-native / frozen crate:"; echo "$BADDEP"
  red "  -> rts-shared/rts-std are NOT primitive; rts-codegen-old is frozen."
  red "     Reach the runtime ONLY through the rts-runtime facade."
  HARD_FAIL=1
else
  grn "ok — deps are rts-parser/hir/ast + rts-runtime facade + rts-engine only."
fi

# ---------------------------------------------------------------------------
hdr "2/5  Forbidden direct use of rts-shared / rts-std / old engine (HARD)"
BADUSE=$(grep -rnE 'rts_shared::|rts_std::|rts_codegen_old::|use[[:space:]]+rts_(shared|std)' "$SRC" || true)
if [ -n "$BADUSE" ]; then
  red "Engine source reaches a non-native crate directly:"; echo "$BADUSE"
  red "  -> route through rts_runtime::* (the facade) instead."
  HARD_FAIL=1
else
  grn "ok — no direct rts_shared/rts_std/rts_codegen_old paths in src."
fi

# ---------------------------------------------------------------------------
hdr "3/5  Non-primordial class names in codegen (REVIEW)"
# Capital-cased class token, code lines only (drop pure-comment lines). Test
# files (front/run/tests/, fixture_check/) legitimately name classes in fixture
# strings — split them out so the real codegen signal stands alone.
MENTIONS=$(grep -rnE "\\b(${NONPRIMORDIAL})\\b" "$SRC" \
  | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)
CODEGEN=$(printf '%s\n' "$MENTIONS" | grep -vE '/tests/|/fixture_check/' | grep . || true)
TESTS=$(printf '%s\n'   "$MENTIONS" | grep -E  '/tests/|/fixture_check/'  | grep . || true)
if [ -n "$CODEGEN" ]; then
  COUNT=$(printf '%s\n' "$CODEGEN" | wc -l | tr -d ' ')
  ylw "$COUNT codegen line(s) name a non-primordial class. Per file:"
  printf '%s\n' "$CODEGEN" | cut -d: -f1 | sort | uniq -c | sort -rn | sed "s|$SRC/||"
  ylw "REVIEW each: it MUST resolve through the Registry (registry.rs /"
  ylw "registry_call.rs bridge), never a hardcoded per-class path. A dedicated"
  ylw "*class.rs is a draining target — never ADD a non-primordial path there."
else
  grn "ok — no non-primordial class named in codegen."
fi
if [ -n "$TESTS" ]; then
  ylw "(info) $(printf '%s\n' "$TESTS" | wc -l | tr -d ' ') mention(s) in test/fixture files — expected (class names in fixtures)."
fi

# ---------------------------------------------------------------------------
hdr "4/5  Source files over ${MAX_LINES} lines (REVIEW)"
BIG=$(find "$SRC" -name '*.rs' -print0 \
  | xargs -0 wc -l 2>/dev/null \
  | awk -v m="$MAX_LINES" '$1>m && $2!="total"{print $1"\t"$2}' \
  | sort -rn)
if [ -n "$BIG" ]; then
  N=$(printf '%s\n' "$BIG" | wc -l | tr -d ' ')
  ylw "$N file(s) exceed ${MAX_LINES} lines — split into a folder/subfolder of"
  ylw "cohesive submodules. Do NOT let this list grow:"
  printf '%s\n' "$BIG" | sed "s|$SRC/||"
else
  grn "ok — every source file is <= ${MAX_LINES} lines."
fi

# ---------------------------------------------------------------------------
hdr "5/5  WIP markers todo!/unimplemented! (INFO)"
WIP=$(grep -rnE 'todo!\(|unimplemented!\(' "$SRC" || true)
if [ -n "$WIP" ]; then
  ylw "$(printf '%s\n' "$WIP" | wc -l | tr -d ' ') marker(s) — fine as WIP, never as a shipped 'pass':"
  printf '%s\n' "$WIP" | sed "s|$SRC/||"
else
  grn "ok — no todo!/unimplemented! markers."
fi

# ---------------------------------------------------------------------------
if [ "$NO_BUILD" -eq 0 ]; then
  hdr "build  cargo build -p rts-codegen-new (HARD)"
  if (cd "$ROOT" && cargo build -p rts-codegen-new 2>&1 | tail -8); then
    grn "ok — engine crate compiles."
  else
    red "BUILD FAILED — a broken build blocks commit (honesty+build floor)."
    HARD_FAIL=1
  fi
else
  ylw "build skipped (--no-build)."
fi

# ---------------------------------------------------------------------------
hdr "VERDICT"
if [ "$HARD_FAIL" -ne 0 ]; then
  red "HARD violation present. Do NOT commit until it is fixed."
  exit 1
fi
grn "No hard violation. Review the yellow sections, then commit."
exit 0
