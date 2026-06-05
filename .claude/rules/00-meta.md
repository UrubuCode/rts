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
| `.claude/rules/00-meta.md` | This file — meta + local-rules + regress-when-needed + roadmap |
| `.claude/rules/01-architecture.md` | Project + Architecture + ABI + Namespaces |
| `.claude/rules/02-runtime.md` | HandleTable + tokio + GC + State |
| `.claude/rules/03-features.md` | Silent parallelism + async/Promise/Function + capabilities |
| `.claude/rules/04-workflow.md` | Conventions + progress bar + issues + tests + benchmarks |
| `.claude/rules/05-codegen-notes.md` | Optimizations + backlog + artifact layout |

### Binding meta-rules (canonical list)

- **RULE #0** (this) — read all files in order
- **MANDATORY REQUIREMENT: local-rules.md** (below)
- **MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)** (below)
- **MANDATORY RULE: FOLLOW ROADMAP-CORRECAO.md** (below)
- **CROSS-RUNTIME PUSH MODE (parity ≥ 90%)** (below) — process constraints
  suspended toward 100%; honesty + build floor never lift

Adding/removing a meta-rule requires updating this list in the same commit.

## MANDATORY RULE: FOLLOW ROADMAP-CORRECAO.md

Before starting any cross-runtime parity bug fix (issues `💥 cross-runtime`,
tracking categories, TS suite failures), you **MUST** read `ROADMAP-CORRECAO.md`,
located **one level above the repo root** (`../ROADMAP-CORRECAO.md`, next to the
`rts1/` folder).

That file defines the **topological order** of fixes, based on the feature
dependency graph. The order is not arbitrary: fixing out of order causes the
"fix one, break another" pattern, because several tests share the same
foundation.

### How to apply

1. Always pick the next task from the **lowest level not yet completed**. Never
   jump to a higher level before closing the foundations it depends on.
2. Respect blocks marked ⚠️ in the roadmap (e.g. 336/387/341 are a single root;
   the 204→205→206 chain is linear).
3. On completing an item (PR merged + green suite), mark `[x]` in the roadmap in
   the same PR.
4. If the analysis changes (new dependency discovered), update the graph in the
   roadmap in the same PR — never leave the roadmap stale.
5. If the user explicitly asks to tackle a case out of order, ask for
   confirmation pointing out the missing dependency before proceeding.

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

## CROSS-RUNTIME PUSH MODE (parity ≥ 90%) — process constraints suspended

**Active when** cross-runtime parity ≥ 90% (currently 94.3%, badge in
`README.md`). The goal flips to reaching **100%**, and the *process* constraints
in this rule system are SUSPENDED so change can land at any cost — except the
honesty + build floor, which never lifts. Below 90%, this mode deactivates and
the suspended rules resume automatically.

### Suspended while active
- **`FOLLOW ROADMAP-CORRECAO.md` topological order** — pick any fixture /
  feature / epic in any order.
- **Ask-before-regression** — regressions may land without per-change
  confirmation. Still *logged* in the commit/PR body; net parity across a work
  session must not drop.
- **Small-PR scope** — large multi-crate refactors and the deferred epics
  (#195, #207, #216, #218, #219, #222, #223) are now in scope.
- **Ceremony** — progress-bar / read-everything ritual is optional.

### Never suspended (honesty + build floor)
- **The parity number stays real.** No deleting, disabling, skipping,
  hardcoding, or input-special-casing a fixture to inflate parity. A fixture
  passes only when the runtime genuinely produces the correct output through the
  same code path any other input would take.
- **No crashing / hanging code committed as "pass".** ACCESS_VIOLATION /
  verifier error / stack overflow / infinite loop = not passed.
- **Build must compile.** A broken build still blocks merge.

### Rationale
Per `MAINTENANCE.md`, the remaining fixtures need full feature completion, not
bounded patches — a half-feature crashes rather than producing wrong-but-closer
output, so there is no "regress X to pass Y" trade to police. The ceremony slows
the work without guarding the only real risk (faking the metric); the honesty
floor guards that directly.
