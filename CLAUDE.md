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
| `crates/rts-cranelift/` | its `README.md` (13 rules) |
| `crates/rts-codegen/` | its `README.md` (10 rules) + `PLAN.md` |
| `crates/rts-core-rwk/` | its `README.md` (8 rules) + `PLAN.md` |
| `crates/rts-host-rwk/` | its `README.md` (6 rules) + `PLAN.md` |
| `crates/rts-egui/`, DOM, render, input | `docs/ui/html-engine/` + `docs/ui/egui-crate.md`; for the NEW engine's side of it, `docs/ui/new-engine-port.md` |
| anything else | this file, and `docs/README.md` for where things live |

If a change requires breaking a rule, **change the rule first, with the reason,
and get it agreed**. Never leave a rule the code contradicts.

Also: if `local-rules.md` exists at the root, reading it is mandatory. It is
per-developer, unversioned, and takes priority over general preference.

---

## RULE 0b — a README is the rule; a skill is the procedure

The tables above say what is *binding*. They do not say what to *do*, and the
steps for one change are spread across a README, a PLAN and a doc — which is how
a second value encoding and a second shape tree both got half-written inside one
crate's first three phases.

`.claude/skills/` holds those steps. Each one ends where the rules begin: it
points at the README rather than restating it, because two answers to one
question is what the rest of this file exists to prevent.

| doing | invoke |
|---|---|
| anything new in the new engine, before writing | `reuse-check` |
| an operation compiled code calls instead of emitting | `add-entry-point` |
| a built-in class, namespace or prototype method | `add-builtin-class` |
| an instruction, a layout, a machine capability | `add-ir-instruction` |
| emitting a JS/TS construct, deciding a semantic | `add-language-node` |
| any claim that something is faster or slower | `perf-claim` |

`reuse-check` is the one that is not optional: it is the anti-duplication rule the
crate READMEs state, turned into a search. **Invoking a skill does not replace
RULE 0** — the README is still read in full.

A skill that grows a rule of its own has drifted. Move the rule to the README and
leave the pointer.

---

## The engine, in one paragraph

Two crates and a boundary. `rts-codegen` is the language — JavaScript and
TypeScript semantics — and knows no machine. `rts-cranelift` is the machine — IR,
representations, GC contract, frames, calls, unwinding — and knows no language.
Either rule alone is a preference; both at once means a decision has exactly one
place it can be made. Full picture: `docs/engine/architecture.md`.

Two more crates finish the shape, and each is half of something on purpose.
`rts-core-rwk` is the runtime: it implements what the language calls out for and
never decides what to call. `rts-host-rwk` is the only crate that may name all
three at once, which is why it is where a program runs — and why the agreements
between them (the entry-point symbols, the singleton numbering, the property-key
numbering) are wired and asserted there rather than assumed anywhere.

**A JavaScript program compiles and runs today**, and the sentence that used to
be here — "arithmetic, comparisons, `if`, loops, objects and property access" —
now understates it by a long way. Classes with inheritance and private fields,
closures, `try`/`catch`/`finally` across calls, `async`/`await` over real timers
and sockets, modules, template literals, regular expressions, destructuring,
spread, `for-of`, and the built-ins a program reaches by name: the `Error`
family, `Math`, `JSON`, `Map`, `Set`, `Promise`, `Date`, `Symbol`, plus what
`node:` provides.

`crates/rts-host-rwk/tests/running.rs` is what says so — every test in it runs
the program rather than inspecting it — and the number is measured rather than
claimed. **2026-08-09: 596 of the 797 `*.test.ts` files pass** — 535 of 818 at
the start of 08-08, through generators, `yield*`, `Proxy`, native iterators,
`export *`, a catchable throw, the bare `rts` specifier, stack traces, variadic
natives and wrapper objects.

The DENOMINATOR changed that day and both halves are stated because of it: 21
files were removed for testing surfaces this engine will not have in that shape
(`gc`, `ptr`, `mem`, `alloc`, `ffi`, `trace`), and SIX of them were passing. So
the count fell by six while the share rose, and neither number alone says that.
`crates/rts-host-rwk/examples/suite_run.rs` produced it, one process per file,
because an uncaught exception and an endless loop each take the process with
them and a single-process harness would report whatever it reached first as the
score. It compiles a file with a relative import as a GRAPH, which it did not
until that day: measuring those on their own bound every import to nothing and
reported an instrument's limit as the engine's — 14 assertions in one file.

Read the columns together rather than the first alone. The run reports 596 ok,
76 fail, 51 refused and 73 that died or hung, and files moved BETWEEN those
columns for reasons that are progress: one that starts compiling and then fails
an assertion has moved a number in the direction that looks like regression.

**The death column is what letting a native THROW did**, and it is the one to
read first. It was 10. An operation this engine does not have used to answer
`undefined`, be called, and let the program carry on failing assertions; it is
now an uncaught `TypeError`, which ends the process exactly as it would in Node.
83 of those files call `rts:ptr`, `rts:atomic`, `rts:gc` and the rest of what the
old engine provided — so the throw did not break them, it stopped them hiding.
Two real bugs surfaced that way within an hour: a class body not binding its own
name, so every `static { … }` block assigned a property of nothing, and
`new Function` answering something uncallable.

**Generators run, `yield*` included.** They were the largest single gap — 38
files — and nothing is left of that entry. What `yield*` does not do is forward
`next`, `throw` and `return` to the inner iterator, which is the same limit
`for`-`of` has and is held in one place for that reason;
`docs/engine/generators.md` is the design and says which of it was taken.

**`Proxy` answers through its handler** — `get`, `set`, `has`,
`deleteProperty`, `ownKeys`, `getPrototypeOf`, `setPrototypeOf`, `apply`,
`construct`, `defineProperty`, `getOwnPropertyDescriptor` — and nothing in the
compiled fast path changed to allow it: a cached access encodes an OWN slot and
a proxy has none, so every access to one already missed to the entry point.
Absent are `revocable` and `preventExtensions`.

**`values()`, `keys()` and `entries()` answer an iterator**, with the ES2025
helpers on it. They answered the materialised array, so `.next()` did not
exist; the list is still built eagerly, which `entry/list_iterator.rs` states
as the thing a lazy form replaces rather than joins.

**A native can raise a catchable error now**, and the discipline that had to
come first is rule 8 of `crates/rts-core-rwk/README.md`: a native that calls user
code asks whether the callee left a throw behind before it looks at the answer.
Raising without it turned one silent wrong answer into a hang, which is why the
first attempt was reverted before commit.

**An error says where it came from**, in the `at …` form Node and Bun print,
from the call stack `functions::invoke` already keeps. `.stack` is captured
where the error is CONSTRUCTED. No line numbers yet — the machine records a
source position per instruction and nothing maps an address back to one at run
time, which is `rts_cranelift::observe`'s question.

The largest gap now is the **`rts:` surface the old engine provided** that this
one keeps: `atomic`, `sync`, `thread`, `io`, `buffer`, `net`, `fs`, `process`.
The bare `rts` specifier exists and carries `num`, `math`, `hint` and `time`.
`ptr`, `mem`, `alloc`, `ffi` and `trace` are NOT on that list — they return in
another shape or not at all, and their tests were removed rather than left
pinning an interface nothing will implement.

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

A runtime symbol is declared by an attribute and never written by hand, in both
engines. The attribute derives the ABI signature from the Rust signature, so
drift between the two is unrepresentable rather than merely discouraged.

**Which attribute depends on which engine**, and a crate says so in one line of
its manifest because cargo renames a dependency:

```toml
rtse = { package = "rts-macro-rwk", path = "../rts-macro-rwk" }   # new
rtse = { package = "rts-macro",     path = "../rts-macro" }       # old
```

They are separate because `rts-macro` depends on `rts-abi`, which is what
`rts_cranelift::abi` replaced — so a new-engine crate reaching for the old
attribute reached, through its build graph, for the interface being removed.

The rest of this section is the OLD engine's, and stays binding there.

`rts-symbol-baker` renders that one declaration set twice:

| artefact | addressing | read by |
|---|---|---|
| `generated/symbol_table.rs` | by name | the current engine |
| `generated/entries.rs` | by index, with `TABLE_HASH` | **nobody, yet** |

That second row said "the new engine" and it was **not true**: no crate reads
`generated/entries.rs`, `TABLE_HASH` and `ENTRY_COUNT` appear nowhere outside the
baker, and the baker does not scan `rts-core-rwk`, `rts-cranelift` or
`rts-codegen` at all. It was written as the intent and read as the state.

The intent is still right and `docs/engine/authoring-natives.md` is where it now
lives, with what the new engine actually needs — which is not a table of symbol
names, because a native there is a function pointer beside a cell rather than
something a linker resolves. A built-in class is declared there with
`#[rtse::class]`, which derives the wrappers, the install lists and the
registration from one `impl` block.

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
  rts-codegen/       the language: JS/TS tree, parser bridge, emit, type pass
  rts-core-rwk/      the runtime: values, heap, objects, coercion, entry points
  rts-host-rwk/      where the three meet, and where a program runs
  rts-macro-rwk/     #[rtse::entry] — declares one, derives its shape
  rts-std-rwk/       the `rts:` surface, and the globals
  rts-ui-rwk/        `rts:egui` + `rts:input`, where a target has a screen
  rts-codegen-new/   the engine that currently runs `rts`. Being replaced.

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
