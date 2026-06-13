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
| `.claude/rules/00-meta.md` | This file — meta + local-rules + regress-when-needed + redesign-status |
| `.claude/rules/01-architecture.md` | Project + Architecture (two codegen crates) + ABI + Namespaces |
| `.claude/rules/02-runtime.md` | HandleTable + tokio + GC (PolyValue scanner note) + State |
| `.claude/rules/03-features.md` | New value model (PolyValue/Repr/shapes/ICs) + target semantics |
| `.claude/rules/04-workflow.md` | Conventions + progress bar + issues + tests + benchmarks |
| `.claude/rules/05-codegen-notes.md` | New-engine codegen notes + artifact layout |

### Binding meta-rules (canonical list)

- **RULE #0** (this) — read all files in order
- **MANDATORY REQUIREMENT: local-rules.md** (below)
- **MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)** (below)
- **MANDATORY RULE: PRIMORDIAL-vs-REGISTRY DOCTRINE** (in `CLAUDE.md`; survives
  the redesign — engine names only primordials, no builtins in the engine)
- **MANDATORY RULE: FOLLOW THE REDESIGN DESIGN DOC** (below) — work is picked
  from the migration phases of `docs/specs/rts-codegen-new-design.md`, not from a
  fixture-grind roadmap

The honesty + build floor (parity number stays real; no crash/hang as "pass";
build must compile) never lifts. Adding/removing a meta-rule requires updating
this list in the same commit.

## MANDATORY RULE: FOLLOW THE REDESIGN DESIGN DOC

The project is mid-redesign of its codegen engine (strangler-fig). The canonical
plan is **`docs/specs/rts-codegen-new-design.md`** — read it before any engine
work. The frozen old engine is `crates/rts-codegen-old/`; the active redesign is
`crates/rts-codegen-new/`.

There is no longer a topological fixture-fix roadmap (the old
`ROADMAP-CORRECAO.md` and `MAINTENANCE.md` are deleted — that grind was the local
max of a hardcoded approach on an unsound value model). Instead:

### How to apply

1. Pick work from the design doc's **migration phases (P0→P5, §12)**,
   highest-leverage first. Do not jump ahead of a phase's prerequisites.
2. Each phase runs the suite incrementally (not only at the end) and keeps an
   A/B guard against the old engine where the doc specifies one.
3. The honesty + build floor never lifts: no fixture deleted/disabled/hardcoded
   to inflate the number; nothing that crashes/hangs committed as "pass"; build
   must compile. At cutover (P5) parity must be ≥ the `v0.0-202606072107` tag,
   measured real.
4. If the design changes (new constraint discovered), update
   `docs/specs/rts-codegen-new-design.md` in the same PR — never leave the spec
   stale.
5. If the user asks to work out of phase order, confirm first, pointing out the
   missing prerequisite.

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

## HONEST CURRENT STATUS — redesign in progress

Do not act on stale numbers. The "parity ≥90% push mode" / "94.3%" / "100%"
framing is **dead**.

- The OLD engine reached 100% cross-runtime parity (372/372, tag
  `v0.0-202606072107`, commit `27e16378`; TS suite 1719/1719). That was the
  **local maximum of a hardcoded approach** on an unsound value model (a single
  overloaded `i64` ABI slot + 4 compile-time side-tables), admitted as the wall
  by the now-deleted `MAINTENANCE.md`.
- The fixture set then grew 391 → **612** (harder cases) and parity is now
  **70.7%** — the honest figure to quote.
- A ground-up engine (`crates/rts-codegen-new/`) is being built **strangler-fig
  behind the frozen `crates/rts-codegen-old/`**. Canonical plan:
  `docs/specs/rts-codegen-new-design.md`.

### The floor (NEVER lifts, no mode suspends it)
- **The parity number stays real.** No deleting, disabling, skipping,
  hardcoding, or input-special-casing a fixture to inflate parity. A fixture
  passes only when the runtime genuinely produces the correct output through the
  same code path any other input would take.
- **No crashing / hanging code committed as "pass".** ACCESS_VIOLATION /
  verifier error / stack overflow / infinite loop = not passed.
- **Build must compile.** A broken build still blocks merge.
- **At cutover (design doc P5) parity must be ≥ the `v0.0-202606072107` tag,
  measured real.** The redesign exists to clear the wall, not to trade the number
  away.

### Why the old fixture-grind ceremony is gone
The remaining fixtures need **full feature completion on a sound value model**,
not bounded patches against a hardcoded switchboard — a half-feature crashes
rather than producing wrong-but-closer output, so there is no "regress X to pass
Y" trade to police. The work is now organized by the design doc's phases; the
honesty floor guards the only real risk (faking the metric) directly.
