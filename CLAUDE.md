# CLAUDE.md

RTS compiles TypeScript to native code. This file is the entry point: it holds
what is binding everywhere, and says where everything else is written.

**It deliberately does not restate what a crate README or a document already
says.** Two answers to one question is how the previous version of this file
reached 1222 lines beside a six-file rules tree that repeated most of it — they
disagreed in places, and there was no way to tell which was current. Both were
loaded into every session, so the duplication was paid twice.

---

## RULE 0 — read the rules that own what you are about to change

Before editing a crate, read its `README.md` **in full**. It is a precondition,
not background reading, and the rules in it are binding for changes inside it.

| Editing | Read first |
|---|---|
| `crates/rts-cranelift/` | its `README.md` (13 rules) + `RTS_CRANELIFT.md` |
| `crates/rts-codegen/` | its `README.md` (10 rules) + `PLAN.md` |
| `crates/rts-egui/`, DOM, render, input | `docs/ui/html-engine/` + `docs/ui/egui-crate.md` |
| anything else | this file, and `docs/README.md` for where things live |

If a change requires breaking a rule, **change the rule first, with the reason,
and get it agreed**. Never leave a rule the code contradicts.

Also: if `local-rules.md` exists at the root, reading it is mandatory. It is
per-developer, unversioned, and takes priority over general preference.

---

## The engine, in one paragraph

Two crates and a boundary. `rts-codegen` is the language — JavaScript and
TypeScript semantics — and knows no machine. `rts-cranelift` is the machine — IR,
representations, GC contract, frames, calls, unwinding — and knows no language.
Either rule alone is a preference; both at once means a decision has exactly one
place it can be made. Full picture: `docs/engine/architecture.md`.

**This is the direction for all new work.** `crates/rts-codegen-new` is the
engine that currently runs `rts run` / `compile` / `test`, and it is being
replaced rather than extended. Its own doctrine (the primordial-vs-registry rule,
symbols by name) remains binding *for changes inside it* and is not the model for
anything new.

---

## MANDATORY: iteration speed

Release builds here are minutes — `lto = "thin"`, `codegen-units = 1`,
`opt-level = "z"`. The full TS suite is ~740 files. **Both are merge-time
activities.**

While working:

```bash
cargo check -p <crate>              # does it compile — the default loop
cargo test -p <crate> <filter>      # only the area you touched
cargo run -- run file.ts            # execute without a release build
```

Never `cargo build --release` and never the full suite while iterating. Never
benchmark a debug build — a debug number is not a number.

This rule exists because it was measured: a session spent more wall clock on
repeated release builds than on the engineering, which also pushes toward
guessing instead of checking, because checking became expensive.

---

## MANDATORY: the honesty floor

Never lifts. No mode suspends it.

- **A measured number stays real.** No deleting, disabling, skipping, or
  input-special-casing a test to move it. State what produced it and when.
- **Nothing that crashes or hangs is committed as passing.** Access violation,
  verifier error, stack overflow, infinite loop — that is not a pass.
- **The build compiles.** A broken build blocks merge.
- **Verify the input, not just the output.** A number measured against a corpus
  quietly smaller than claimed is a claim wearing a measurement's clothes. This
  is not hypothetical: a test262 score was published 0.8 points high because 503
  of 24 007 files silently failed to check out.

---

## MANDATORY: regress explicitly, never silently

Regression is allowed when necessary. It must be **stated**.

Before merge:

```bash
cargo build --release
cargo test --release --lib
target/release/rts.exe test          # if the change touches runtime/codegen/GC
bash scripts/read_before_commit.sh   # if the change touches the engine
```

A regression is acceptable when it is intentional or a necessary trade **and**
documented in the commit with the reason. "It broke and I don't know why" is
never acceptable. Silent regression is what turns a green suite into a lie.

---

## MANDATORY: one source, generated views

`rts-macro` (`#[rtse::*]`) is the only place a runtime symbol is declared. It
derives the ABI signature from the Rust signature, so drift between the two is
unrepresentable rather than merely discouraged.

`rts-symbol-baker` renders that one declaration set twice:

| artefact | addressing | read by |
|---|---|---|
| `generated/symbol_table.rs` | by name | the current engine |
| `generated/entries.rs` | by index, with `TABLE_HASH` | the new engine |

**Never hand-write a symbol name, a signature row, or a class-metadata row.**
After adding or renaming, run `cargo run -p rts-symbol-baker` and commit the
artefacts; `-- --check` must be clean.

One permanent exception: `rts-napi`'s 157 `napi_*` declarations. They are a
foreign C ABI whose names *are* the interface — a compiled `.node` addon links
against those exact strings. Do not convert them; their presence is not debt.
Reasoning in `docs/engine/architecture.md`.

---

## MANDATORY: file size and the commit gate

Ceilings: **codegen ≤ 1000**, **engine ≤ 700**, **everything else ≤ 500**. A file
that would pass its ceiling is split into a folder of cohesive modules. New code
lands in a small focused module, never appended to something already oversized.

For any commit touching `crates/rts-codegen-new/`, run the gate and read all of
its output:

```bash
bash scripts/read_before_commit.sh              # full
bash scripts/read_before_commit.sh --no-build   # static only, while iterating
```

HARD failures never ship. REVIEW lists must only shrink.

---

## Repository map

```
crates/
  rts-cranelift/     the machine: IR, repr, GC contract, frames, calls, unwind
  rts-codegen/       the language: JS/TS tree, parser bridge, semantics
  rts-codegen-new/   the engine that currently runs. Being replaced.

  rts-abi/           the ABI contract, dependency-free, at the bottom
  rts-macro/         #[rtse::*] — the only place a symbol is declared
  rts-symbol-baker/  renders the declarations: by name, and by index

  rts-engine/        heap, GC, registry
  rts-primitives/    primordial classes        ┐ today's layering splits on
  rts-shared/        universal non-primordial  │ PERMISSION, which index linkage
  rts-std/           backend: io, net, tokio   │ makes unnecessary. The split
  rts-runtime/       facade + AOT staticlib    │ that stays is by AVAILABILITY —
  rts-natives/       machine-level natives     ┘ what a target has. architecture.md

  rts-parser/        SWC → the old AST      rts-ast/  rts-hir/  rts-diagnostics/
  rts-node/          Node builtins          rts-napi/ N-API
  rts-egui/ rts-dom/ rts-render/ rts-input/ the UI engine
  rts-linker/        native link            rts-cli/  the CLI

docs/
  engine/     how the compiler works and why      guides/  how to do a thing
  reference/  surfaces we implement against       ui/      the graphical engine
```

`docs/README.md` states which of the four a new document belongs in, and the
rules that keep them from becoming a pile again.

---

## Conventions

- **Code:** Rust, English identifiers. **Docs:** English. **Conversation:**
  Portuguese.
- **Commits:** conventional — `feat:`, `fix:`, `perf:`, `refactor:`, `docs:`,
  `chore:`. The body says *why*, and names what was rejected.
- **No dead code.** Deleted in the change that stopped reaching it — never
  commented out, never "just in case". `todo!()` is an acceptable marker;
  commented code is not.
- **Documentation says why.** A comment restating the code is worth nothing; the
  code already says that, and says it correctly. Name the alternative and the
  reason it lost.
- **Tests name the behaviour they pin**, not the function they call. A test
  asserting that our code does what our code does proves nothing.

---

## Running things

```bash
$env:RUST_BACKTRACE = "full"          # always — the crash handler needs it

cargo run -- run file.ts              # JIT
target/release/rts.exe compile -p file.ts out   # AOT
target/release/rts.exe ir file.ts     # Cranelift IR, no execution
target/release/rts.exe test tests/one.test.ts   # a single file
```

**AOT needs a two-step build.** `cargo build -p rts-runtime` before building
`rts`: Cargo emits a `staticlib` only for a package built as a direct target, and
being a dependency is not the same thing. Skipping it leaves a stale archive and
`rts compile` dies in the linker. JIT is unaffected.

When a test fails, run that file alone before the suite — it avoids timeout and
noise. `rts ir` diagnoses the rest: an unknown namespace member is a missing
handler, SIGILL is invalid IR, an access violation is a null load.

---

## Progress bar

For multi-step work, show one per significant change — file created, build
passed, test ran, commit made:

```
[▰▰▰▱▱▱▱▱▱▱] 30% — short description of the current step
```

Ten segments, real percentage. On error, prefix `❌ erro:` and roll back to where
confidence dropped.

---

## GitHub issues

Mark an issue taken before starting (`gh issue comment`, and
`gh issue edit --add-assignee @me` if a collaborator). On finishing, comment with
the PR link and close when appropriate.
