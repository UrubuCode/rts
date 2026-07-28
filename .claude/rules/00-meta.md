# Meta rules — mandatory reading

This file is read first. It defines how to treat the rest of the rule system.
After reading this, read the others in order (`01-` through `05-`) — each is
binding to the same degree.

## RULE #0 — MANDATORY ABSOLUTE META-RULE

**Before starting ANY task, you MUST read every file in `.claude/rules/` in full
and follow ALL rules they define — no exceptions, no omissions, no "picking the
important ones". Every rule is binding.**

### How to apply

1. On the first message of each session (and whenever any file in
   `.claude/rules/` changes), read the files in ascending numeric-prefix order
   before touching code.
2. Each `## MANDATORY RULE:` section is binding even when the task context seems
   not to require it.
3. Each `## Conventions`, `## Rules`, `## ABI ...`, `## Structure ...` section
   defines conventions that must be respected in any code change.
4. If a rule conflicts with a user instruction, ask for confirmation before
   violating the rule. Do not decide alone.
5. If a rule is stale (code no longer matches), update the corresponding file in
   the same PR — never leave a lying rule in effect.

### Reading map

Read these files in order (path relative to repo root):

| File | Content |
|---|---|
| `.claude/rules/00-meta.md` | This file — meta + local-rules + regress-when-needed + current-status |
| `.claude/rules/01-architecture.md` | Project + Architecture (single engine + rts-runtime adapters) + ABI + Namespaces |
| `.claude/rules/02-runtime.md` | HandleTable + tokio + GC (PolyValue scanner note) + State |
| `.claude/rules/03-features.md` | New value model (PolyValue/Repr/shapes/ICs) + target semantics |
| `.claude/rules/04-workflow.md` | Conventions + progress bar + issues + tests + benchmarks |
| `.claude/rules/05-codegen-notes.md` | New-engine codegen notes + artifact layout |

### Binding meta-rules (canonical list)

- **RULE #0** (this) — read all files in order
- **MANDATORY REQUIREMENT: local-rules.md** (below)
- **MANDATORY RULE: ITERATION SPEED** (in `CLAUDE.md`) — while developing, never
  `cargo build --release` and never run the full TS suite. `cargo check -p <crate>`
  to compile-check, `cargo run -- run file.ts` to execute, and only the tests of
  the area you touched. The full gate (release build + unit suite + TS suite +
  `read_before_commit.sh`) is a MERGE-TIME activity. Benchmarks are release-only.
- **MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)** (below)
- **MANDATORY RULE: PRIMORDIAL-vs-REGISTRY DOCTRINE** (in `CLAUDE.md`; engine
  names only primordials, no builtins in the engine)
- **MANDATORY RULE: DRIVE COVERAGE BY MEASURED CLUSTERS** (below) — the migration
  is done; work is picked by measure → attack biggest failure cluster → re-measure,
  with `docs/specs/rts-codegen-new-design.md` as the canonical architecture
  reference
- **MANDATORY RULE: READ THE EGUI/WEB ENGINE PLAN BEFORE TOUCHING IT** (in
  `CLAUDE.md`) — before changing anything in `crates/rts-egui/` or any
  egui/HTML/web-UI code, read the frozen plan in full
  (`docs/specs/html-engine/rts-html-roadmap.md` F0–F5 +
  `rts-html-north-star.md` + `arquitetura.md` + `docs/specs/egui-ui-crate-design.md`)
  and follow its phases in order. STRICTLY MANDATORY — no exceptions.
- **MANDATORY RULE: SINGLE SOURCE OF TRUTH — `rts-macro` + `rts-symbol-baker`**
  (in `CLAUDE.md`) — `rts-macro` is the ORCHESTRATOR: it declares, types and
  organizes every symbol in one authoring surface (`#[rtse::abi]` derives the
  `SymbolDesc` from the Rust signature, so drift is unrepresentable).
  `rts-symbol-baker` is the LINKER: it scans those declarations and bakes ONE
  static, strictly-ordered table that is both the JIT vtable and the AOT symbol
  set — and is the right place to organize how other modules publish and how
  `rts-engine` manages them. It replaces `rt.rs`, the hand-declared symbol
  system, and `rts-codegen-new/src/adapter_symbols/`. Never hand-write a symbol,
  a signature row, or a class-metadata row.
- **CRATE-DEPENDENCY-DIRECTION BANS: REMOVED** (owner decision, 2026-07-28) —
  `rts-codegen-new` may depend directly on `rts-engine`, and the analogous bans
  elsewhere ("only through the `rts-runtime` facade", "no second direct dep",
  "`rts-shared`/`rts-std` deps are a regression") are gone from the rules and
  from the gate. They were the stated reason 527 rows of mirror tables existed.
  The PRIMORDIAL naming doctrine is SEPARATE and still binding: an edge is free,
  a hardcoded non-primordial class NAME in codegen is not.
- **MANDATORY RULE: read_before_commit.sh GATE + FILE LAYOUT** (in `CLAUDE.md`;
  workflow detail in `04-workflow.md`) — run `bash scripts/read_before_commit.sh` before
  every engine commit; file-size ceilings codegen ≤1000 / engine ≤700 / rest ≤500 (split into
  folders/subfolders); engine names ONLY primordials in its control flow

The honesty + build floor (parity number stays real; no crash/hang as "pass";
build must compile) never lifts. Adding/removing a meta-rule requires updating
this list in the same commit.

## MANDATORY RULE: DRIVE COVERAGE BY MEASURED CLUSTERS

The migration is over — there is ONE engine (`crates/rts-codegen-new/`). The
AOT-linked runtime trampolines (PolyValue + `__rtsadp_*`) live in
`crates/rts-runtime/src/adapters/` (folded in from the former standalone
`rts-adapters` crate — `rts-runtime` was already the direct dependency both it
and the `rts` bin needed, so the separate crate added nothing); the
lowering-time slices (Repr lattice, shapes, codegen-state reset) live in
`rts-codegen-new/src/`. The phase roadmap (P0→P5) is done through the cutover.
`docs/specs/rts-codegen-new-design.md` is still the canonical **architecture**
reference (PolyValue / Repr lattice / shapes + data ICs / data-driven dispatch /
single lowering); read it before any engine work. Note its file-path map is
**stale** — it describes `value.rs`/`ic.rs`/`abi_gen.rs` under
`rts-codegen-new/src/`, but the value model lives in
`rts-runtime/src/adapters/`, `ic.rs` no longer exists, and there is no
`abi_gen.rs` (the JIT symbol table is harvested in
`crates/rts-codegen-new/src/adapter_symbols/`). Trust the tree on disk over the
doc's paths.

### How to apply

1. Work is picked by the loop **measure → attack the biggest failure cluster →
   re-measure**. Measure with `bash scripts/measure_new.sh` (per-file pass/bail histogram)
   and the cross-runtime report (`.github/cross_runtime_report.json`). The biggest cluster
   is the biggest lever; resolving it reveals the next.
2. Run the suite incrementally (not only at the end).
3. The honesty + build floor never lifts: no fixture deleted/disabled/hardcoded to
   inflate the number; nothing that crashes/hangs committed as "pass"; build must
   compile. The bar to re-clear is the deleted old engine's peak (`v0.0-202606072107`),
   measured real.
4. If the architecture changes (new constraint discovered), update
   `docs/specs/rts-codegen-new-design.md` in the same PR — never leave the spec
   stale.

## MANDATORY REQUIREMENT: local-rules.md

Before starting any task, you **MUST** check whether `local-rules.md` exists at
the project root.

**If it exists, reading it is mandatory** — not optional, do not skip, do not
assume content, do not proceed without reading. If it does not exist, proceed
normally.

When present, treat its content as additional rules set by the developer working
on this local copy. These rules take priority over generic preferences and must
be respected throughout the session.

`local-rules.md` is per-developer and **must not be versioned** (already in
`.gitignore`).

## MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)

Regression is allowed when necessary — but it must **always be explicit and
justified**, never silent. This replaces the old "zero regression" rule.

Minimum suite before merge:

```bash
cargo build --release             # clean build
cargo test --release --lib        # unit + integration
target/release/rts.exe test       # TS suite (if PR touches runtime/codegen/GC)
```

### Practical rules

- **Run the full suite before merge.** You MUST know exactly which tests pass and
  which regress. "It broke and I don't know why" is never acceptable.
- **A regression is acceptable only when** (a) it is intentional (changed
  behavior / removed feature) or a necessary tradeoff for the change, **and**
  (b) it is documented explicitly in the commit/PR with justification.
- **Silent or unexplained regression still blocks merge.** Each regressing test
  must be either updated to the new expected behavior, or listed explicitly as a
  known regression with reason + tracking issue.
- **A broken build blocks merge** unless explicitly justified in the same PR.
- **Codegen fixtures (`tests/fixtures/*.ts/.out`) are part of the suite.** If
  behavior changed on purpose, update `.out` and justify.
- **Large multi-area PRs run the suite incrementally** during development, not
  only at the end.

### Why this rule exists

With 2 devs + AI accelerating velocity, the danger is *silent* regression piling
up until the suite becomes a lie (green tests, broken uncovered paths). The
discipline here is not "never break a test" — it is "never break a test without
knowing and saying so". Explicit, justified regression is acceptable; invisible
regression rots the project.

## HONEST CURRENT STATUS — cutover done, one engine

Do not act on stale numbers or a stale architecture. The "94.3%" / "100%" /
"70.7%" framings are **dead** — they were the deleted old engine.

- **The cutover happened.** The old engine (`rts-codegen-old`) and `rts-mir` are
  DELETED. `crates/rts-codegen-new/` is the only engine (runtime trampolines in
  `crates/rts-runtime/src/adapters/`); `rts run`/`compile`/`test`/`eval` run it. AOT works
  (`rts compile` emits `.o` + native link). Canonical architecture:
  `docs/specs/rts-codegen-new-design.md` (path map stale — see the rule above).
- **Honest parity now: ~76.5%** as of 2026-07-05 (auto-updated badge; climbed
  from 31.5% on 2026-06-23). The engine has the sound value model and keeps
  re-filling JS/TS coverage. **Always re-measure
  (`.github/cross_runtime_report.json`); never quote a remembered number.**
- **Surface + threading direction docs** (2026-07-05):
  `docs/specs/rts-std-surface.md` (public-surface redesign — read before any
  namespace/public-API change) and `docs/specs/rts-threading-model.md`
  (engine threading / regional heap).
- The old engine once hit 100% (372/372, tag `v0.0-202606072107`) on an unsound
  value model — that is the bar to re-clear, not a current figure.

### The floor (NEVER lifts, no mode suspends it)
- **The parity number stays real.** No deleting, disabling, skipping,
  hardcoding, or input-special-casing a fixture to inflate parity. A fixture
  passes only when the runtime genuinely produces the correct output through the
  same code path any other input would take.
- **No crashing / hanging code committed as "pass".** ACCESS_VIOLATION /
  verifier error / stack overflow / infinite loop = not passed.
- **Build must compile.** A broken build still blocks merge.
- **The bar to re-clear is the deleted old engine's peak (`v0.0-202606072107`),
  measured real.**
