#!/usr/bin/env bash
# read_before_commit.sh — MANDATORY gate. Run it (and READ the whole output)
# before every commit that touches the engine `crates/rts-codegen-new/`.
#
# It encodes the binding rules from CLAUDE.md / .claude/rules/ so a commit cannot
# silently violate them:
#
#   [FLOOR]  build must compile (honesty+build floor — never lifts).
#   [SOURCE] the symbol table is GENERATED. `rts-macro` declares a symbol,
#            `rts-symbol-baker` bakes the table; a hand-written (name, fn_ptr)
#            or (name, signature) row is a REGRESSION. The baker's checked-in
#            artefact must match the source (`--check`).
#   [DOCTRINE] the engine names ONLY the PRIMORDIAL classes. A direct mention of
#              a non-primordial class (Map/Set/Date/…) in codegen control flow
#              is a REGRESSION — everything non-primordial resolves through the
#              Registry, never hardcoded in codegen.
#   [LAYOUT]  no codegen source file > 1000 lines — split it into a
#              folder/subfolder of cohesive submodules instead.
#   [WIP]     surface todo!()/unimplemented!() so none ship disguised as "done".
#
# HARD failures (symbol-table drift, broken build) exit non-zero — do NOT
# commit. The doctrine class-mention scan, the hand-list scan and the
# >1000-line scan are REVIEW gates: read every entry and confirm it resolves via
# the Registry / the baker and is genuinely unavoidable, or fix it. Pre-existing
# debt is listed so it keeps shrinking, never grows.
#
# REMOVED (owner decision, 2026-07-28): the crate-dependency-direction bans.
# `rts-codegen-new` may depend directly on `rts-engine` — and on any other
# runtime crate it needs — because the single source of truth
# (`rts-macro` + `rts-symbol-baker`) has to be reachable without a hand-written
# mirror standing in for it. Those bans were the stated reason three parallel
# tables existed (`adapter_symbols/list_*.rs`, `adapters/dispatch.rs`,
# `value/abi_sig.rs`); removing the bans is what lets them be deleted. The
# PRIMORDIAL naming doctrine below is a SEPARATE rule and still binding: a
# dependency edge is allowed, hardcoding a non-primordial class NAME in codegen
# control flow is not.
#
# Usage:  bash read_before_commit.sh            # full gate
#         bash read_before_commit.sh --no-build # skip the cargo build step
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE="$ROOT/crates/rts-codegen-new"
SRC="$ENGINE/src"
CARGO="$ENGINE/Cargo.toml"
MAX_LINES=1000
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
# AggregateError Symbol. RegExp is reclassified primitive (it has `/re/` native
# syntax), so it is NOT forbidden here. Symbol is a PRIMORDIAL (a fundamental
# language built-in with a unique primitive type + `typeof "symbol"`); the engine
# MAY name it (its impl/metadata still live in rts-shared, like every primitive).
# PRIMORDIAL since 2026-07-03 (owner decision) — they define/intercept the
# VALUE MODEL itself, so the engine MAY name them (impls stay in the runtime
# layer): BigInt (new primitive, `123n`, typeof "bigint"), Proxy (traps the
# engine's own property paths consult), Reflect (1:1 mirrors of the engine's
# internal ops), ArrayBuffer/SharedArrayBuffer/DataView/TypedArrays (the raw
# MEMORY model; ta indexing is engine-lowered), Atomics (memory-model ops),
# WeakRef/FinalizationRegistry (GC-coupled, #217), Math (its core ops are IR
# intrinsics already). Iterator/generator PROTOCOL is primordial (engine
# interactions); the API bodies stay in classes so the surface can evolve
# without touching the engine.
NONPRIMORDIAL='Map|Set|WeakMap|WeakSet|Date|URL|URLSearchParams|Intl|TextEncoder|TextDecoder|EventTarget|Headers|FormData|ReadableStream'

# ---------------------------------------------------------------------------
hdr "1/5  Baked symbol table is in sync with the source (HARD)"
# `rts-symbol-baker` scans the source for `#[rtse::*]`-declared symbols and emits
# the checked-in table. If the artefact drifts from the source, the JIT vtable and
# the AOT symbol set are a lie — that is the bug class this gate exists for.
if [ "$NO_BUILD" -eq 0 ]; then
  if (cd "$ROOT" && cargo run -q -p rts-symbol-baker -- --check >/dev/null 2>&1); then
    grn "ok — generated/symbol_table.rs matches the source."
  else
    red "SYMBOL TABLE DRIFT — generated/symbol_table.rs does not match the source."
    red "  -> run: cargo run -p rts-symbol-baker    (then review the diff)"
    HARD_FAIL=1
  fi
else
  ylw "baker --check skipped (--no-build)."
fi

# ---------------------------------------------------------------------------
hdr "2/5  Hand-written symbol / signature tables (REVIEW — must shrink to zero)"
# The single source of truth is `rts-macro` (declares) + `rts-symbol-baker`
# (bakes). Every row below is a hand-maintained mirror of a definition the baker
# can emit; each is a draining target, and NOTHING may be added to them.
HANDROWS=0
for f in "$SRC/adapter_symbols/list_a.rs" "$SRC/adapter_symbols/list_b.rs"; do
  [ -f "$f" ] || continue
  N=$(grep -cE '^[[:space:]]*sym\(' "$f" || true)
  [ "$N" -gt 0 ] && { ylw "  $N hand-listed JIT symbol row(s) in ${f#$SRC/}"; HANDROWS=$((HANDROWS+N)); }
done
if [ -f "$SRC/value/abi_sig.rs" ]; then
  N=$(grep -cE 'SymSig[[:space:]]*\{' "$SRC/value/abi_sig.rs" || true)
  [ "$N" -gt 0 ] && { ylw "  $N hand-written SymSig row(s) in value/abi_sig.rs"; HANDROWS=$((HANDROWS+N)); }
fi
DISPATCH="$ROOT/crates/rts-runtime/src/adapters/dispatch.rs"
if [ -f "$DISPATCH" ]; then
  N=$(grep -cE 'MethodSpec[[:space:]]*\{' "$DISPATCH" || true)
  [ "$N" -gt 0 ] && { ylw "  $N hand-written MethodSpec row(s) in rts-runtime/src/adapters/dispatch.rs"; HANDROWS=$((HANDROWS+N)); }
fi
if [ "$HANDROWS" -gt 0 ]; then
  ylw "$HANDROWS hand-maintained row(s) total. Each mirrors a definition the baker"
  ylw "can emit and the Registry can resolve. Drain them; never ADD one."
else
  grn "ok — no hand-written symbol/signature tables left."
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
