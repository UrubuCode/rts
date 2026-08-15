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
| `crates/rts-core/` | its `README.md` (8 rules) + `PLAN.md` |
| `crates/rts-host/` | its `README.md` (6 rules) + `PLAN.md` |
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
`rts-core` is the runtime: it implements what the language calls out for and
never decides what to call. `rts-host` is the only crate that may name all
three at once, which is why it is where a program runs — and why the agreements
between them (the entry-point symbols, the singleton numbering, the property-key
numbering) are wired and asserted there rather than assumed anywhere.

**A JavaScript program compiles and runs today**, and the sentence that used to
be here — "arithmetic, comparisons, `if`, loops, objects and property access" —
now understates it by a long way. Classes with inheritance and private fields,
closures, `try`/`catch`/`finally` across calls, `async`/`await` over real timers
and sockets, modules, template literals, regular expressions, destructuring,
spread, `for-of`, and the built-ins a program reaches by name: the `Error`
family, `Math`, `JSON`, `Map`, `Set`, `Promise`, `Date`, `Symbol`, `Intl` — its
seven services over real CLDR data, not a table of English — plus what
`node:` provides.

`crates/rts-host/tests/running.rs` is what says so — every test in it runs
the program rather than inspecting it — and the number is measured rather than
claimed. **2026-08-15: 754 of the 808 `*.test.ts` files pass**, by
`target/release/rts.exe test`, one process per file. It was 756 of 799 on
08-10, 626 of 797 on 08-09 and 535 of 818 at the start of 08-08, through
generators, `yield*`, `Proxy`, native iterators, `export *`, a catchable throw,
the bare `rts` specifier, stack traces, variadic natives and wrapper objects.

**The share fell between 08-10 and 08-14 and nothing here claims to know why.**
The corpus grew by nine files and there were commits between the two
measurements that neither was taken across, so the drop is not attributable to
anything by subtraction. What IS attributable was measured per file, against a
binary of the tree as it stood before the work: 748 → 750 → 754, six gained,
**none lost**. That is the only comparison a number of this shape supports —
and the file that used to HANG is gone from the column:
`for_await_break_return.test.ts` timed out on every run until `for`-`of`
stopped materialising its sequence.

**And the second ruler: 674 of 708 cross-runtime fixtures** (95.2%), measured
2026-08-15 by `scripts/cross_runtime_check.sh`, one process per file, against
Bun and Node. That corpus is a different question from the one above — it asks
whether this engine and a real one agree about the same program, where
`*.test.ts` asks whether the program does what it says.

It was 666, 646, 630, 593 and 419 earlier in the same stretch. Every step between
those figures was measured PER FILE against a kept binary and cost **nothing**: the
LOST list is empty at each one, which is the only form the claim "no regression"
takes here. The net number never was.

The ceiling under the number is worth reading with it: **five of the 708 have no
comparable answer**, because Bun and Node disagree with each other and the
harness refuses to elect one of them. So the reachable total is 703, not 708.

The 08-10 figure was measured by the same `suite_run`, one process per file, on
the same corpus plus the two files that day's own work added — which is why the
denominator moved by two and is stated rather than smoothed over. The line above
said 626 of 797 for a day in which the number had already moved; a measured
number that is not re-measured becomes a claim, which is the thing this
paragraph exists to refuse.

The DENOMINATOR changed that day and both halves are stated because of it: 21
files were removed for testing surfaces this engine will not have in that shape
(`gc`, `ptr`, `mem`, `alloc`, `ffi`, `trace`), and SIX of them were passing. So
the count fell by six while the share rose, and neither number alone says that.
`crates/rts-host/examples/suite_run.rs` produced it, one process per file,
because an uncaught exception and an endless loop each take the process with
them and a single-process harness would report whatever it reached first as the
score. It compiles a file with a relative import as a GRAPH, which it did not
until that day: measuring those on their own bound every import to nothing and
reported an instrument's limit as the engine's — 14 assertions in one file.

Read the columns together rather than the first alone, and read files that move
BETWEEN them as what they are: one that starts compiling and then fails an
assertion has moved a number in the direction that looks like regression.

**The number that says how far this still is: the OLD engine passes 777 of the
same 797** — 779 until two fixtures asserting a `super` JavaScript does not have
were corrected, which the old engine passed by implementing `super` wrongly.
Both engines were measured over the same corpus by `scripts/measure_engines.sh`
— deleted with the second engine, since a script that runs two things can run
neither when one is gone — one process per file. **167 files passed only on the
old engine and 16 only on the new.**

Read that as a work list and not as a loss: `rts-codegen-new` was DELETED on
2026-08-10, and deleting it cost none of those 167. It had stopped running
anything at the cutover — `run`, `test` and `compile` had already moved — so
what the crate still held was `ir`, `eval` and `emit-types`, and each of those
was rebuilt on this engine first. The 167 are what this engine does not do yet,
which was true the day before as well; what changed is that there is no longer a
second engine that could be measured instead of fixed.

The rulers differ in one stated way: `rts test` also
compares stdout against a fixture where one exists, which `suite_run` never
sees; both require "ran and nothing failed", which is what makes the counts
comparable at all.

**The gap has no single cause, and its shape is the work list.** As triaged at
194 files (08-09, before this round took 27 of them):

| n | the new engine answers | reading |
|---|---|---|
| 93 | compiles, runs, FAILS an assertion | a wrong answer, not a missing feature |
| 64 | `TypeError: undefined is not a function` | was un-triageable; the message now names the callee |
| 11 | `Unbound("x")`, `Unbound("v")`, `Unbound("R")` … | ordinary LOCAL names — scope, not a missing library |
| 22 | a missing global, `rts:`/DOM surface, a hang | mostly decisions already taken elsewhere |

The third row is worth listing apart because a missing `WeakRef` is a library gap
while a missing `x` is the emitter losing a binding — those eleven were five
causes, the largest being that `var` was never distinguished from `let`.

**Naming the callee is what made the second row workable, and its answer was
"there is no single cause".** Once `atomic.*` and the unnamed optional-chain
sites are set aside, those 64 files spread over ~50 distinct missing operations,
mostly one file each. Expect volume, not a switch.

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
come first is rule 8 of `crates/rts-core/README.md`: a native that calls user
code asks whether the callee left a throw behind before it looks at the answer.
Raising without it turned one silent wrong answer into a hang, which is why the
first attempt was reverted before commit.

**An error says where it came from**, in the `at …` form Node and Bun print,
from the call stack `functions::invoke` already keeps. `.stack` is captured
where the error is CONSTRUCTED. No line numbers yet — the machine records a
source position per instruction and nothing maps an address back to one at run
time, which is `rts_cranelift::observe`'s question.

**What the `rts:` surface keeps, and what left.** The bare `rts` specifier
carries `num`, `math`, `hint`, `time`, `gc` and `atomic`. Still wanted from what
the old engine provided: `io`, `buffer`, `net`, `fs`, `process`.

**GONE by decision, and their tests with them** — `ptr`, `mem`, `alloc`, `ffi`,
`trace`, `sync`, `thread`, `promise.new_*`, and `RtsePoint`. The first five left
earlier because they return in another shape or not at all; `sync` and `thread`
left on 08-10 for a reason worth keeping written down.

`thread` needs two OS threads running JavaScript, and this engine cannot: a
`Context` is reached through a thread-local and nothing in the host spawns a
thread to run a callback. That is an architecture, not a gap, so a `thread`
namespace would have been a name with nothing behind it.

`sync` is the sharper case, because it EXISTED for a few hours on 08-10 before
being removed. Its `mutex_lock` could not block — there is nothing to block
against — so what shipped was a lock that always succeeded. That passes a test
and lies to a reader, which is worse than the missing name: a program written
against it would be correct here and wrong the day threads arrive. `atomic`
survives the same argument only because its operations are read-modify-write on
one thread, which is genuinely what they compute, and its module says so.

The rule this leaves: **a surface that cannot do what its name means does not
ship.** An absent name fails loudly at the call; a hollow one fails in
production.

**This is the direction for all new work, and now the only one.**
`crates/rts-codegen-new` was deleted on 2026-08-10, once `ir`, `eval` and
`emit-types` — everything still entering through it — had been rebuilt here.
Its doctrine (the primordial-vs-registry rule, symbols by name) went with it and
is not the model for anything: where a document still describes it, that
document is describing a crate that does not exist.

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
cargo run -- ir file.ts             # read what was emitted, without running it
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
cargo test --release --lib -p <each crate you touched>   # NAME them — see below
target/release/rts.exe test          # if the change touches runtime/codegen/GC
```

**`cargo test --release --lib` with no `-p` is not a gate.** At the workspace
root it tests the root `rts` package alone and answers `0 passed; 0 failed` —
green, and measuring nothing. It stood here bare and passed as a check for as
long as nobody read the count. Naming the four crates of a codegen change
answers 367 tests instead. This is the honesty floor's "verify the input, not
just the output" applied to our own gate.

**A suite number is compared PER FILE, never net.** `+3` is equally consistent
with three gained and with five gained against two lost, and only one of those
is shippable.

**The baseline is a BINARY you keep, not a stash you take.** Before the first
edit of a session, build and put the binary aside:

```bash
cargo build --release && cp target/release/rts.exe target/baseline.exe
RTS_BIN=target/baseline.exe REPORT_FILE=base.json bash scripts/cross_runtime_check.sh
```

Then measure the change against it, and compare the two reports PER FILE:

```bash
cargo build --release
RTS_BIN=target/release/rts.exe REPORT_FILE=now.json bash scripts/cross_runtime_check.sh
# LOST = passed in base.json, does not pass in now.json
```

After each commit, refresh the pair — `cp target/release/rts.exe target/baseline.exe`
and keep that commit's report — so the next comparison is against the last
thing that was measured rather than against the start of the session.

`git stash push -u` was the recipe here, and it is the wrong one for this. It
costs a full release build to go back (minutes), a second to come forward, and
it moves the WORKING TREE — so a measurement taken while several changes are in
flight cannot be taken at all, and an interrupted session can leave the tree
somewhere nobody asked for. A kept binary costs one copy, is measurable at any
moment, and never touches the tree. `target/` is ignored, so the baseline is
not something to remember to clean up.

An empty LOST list is the claim "no regression"; the net number never was.

A regression is acceptable when it is intentional or a necessary trade **and**
documented in the commit with the reason. "It broke and I don't know why" is
never acceptable. Silent regression is what turns a green suite into a lie.

---

## MANDATORY: one source, generated views

A runtime symbol is declared by an attribute and never written by hand. The
attribute derives the ABI signature from the Rust signature, so drift between
the two is unrepresentable rather than merely discouraged. One attribute now —
`rts-macro`, spelled `rtse` in a manifest:

```toml
rtse = { package = "rts-macro", path = "../rts-macro" }
```

There were two, under two names: this one was `rts-macro-rwk` while the old
engine's `rts-macro` (over `rts-abi`) still existed and cargo would not have two
crates under one name. The old one went on 2026-08-10 and the suffix went with
it — everywhere, which is why no crate carries `-rwk` any more. So did
`rts-symbol-baker` and its two rendered tables:
`generated/symbol_table.rs`, read by the engine that no longer exists, and
`generated/entries.rs`, which this file used to say the new engine read and
which **nothing ever read** — written as the intent and left standing as though
it were the state. There is no baker to run before a commit any more.

The new engine needs no table of symbol names, which is why: a native here is a
function pointer beside a cell, not something a linker resolves.
`#[rtse::class]` derives the wrappers, the install lists, the registration, AND
the TypeScript declaration `rts emit-types` prints — four views, one `impl`
block. `docs/engine/authoring-natives.md` is how to write one.

**Never hand-write a symbol name, a signature row, or a class-metadata row.**

One permanent exception: `rts-napi`'s 146 `napi_*` declarations. They are a
foreign C ABI whose names *are* the interface — a compiled `.node` addon links
against those exact strings. Do not convert them; their presence is not debt.
Reasoning in `docs/engine/architecture.md`.

---

## MANDATORY: file size and the commit gate

Ceilings: **codegen ≤ 1000**, **engine ≤ 700**, **everything else ≤ 500**. A file
that would pass its ceiling is split into a folder of cohesive modules. New code
lands in a small focused module, never appended to something already oversized.

**The commit gate is gone with the crate it gated.**
`scripts/read_before_commit.sh` checked `crates/rts-codegen-new/`: its
primordial-vs-registry doctrine, its 1000-line ceiling, its symbol-table
artefacts. All three are that crate's, and CLAUDE.md said so — the doctrine was
binding *for changes inside it* and was never the model for anything new. With
the crate deleted the script pointed at a directory that does not exist, so it
was deleted rather than repointed: aiming it at the new engine would have
applied the wrong ceiling and the wrong doctrine while looking like a check.

What replaces it is per-crate and already binding: the ceilings above, each
crate's README, and the release gate in the section before this one. If a
mechanical gate for the new engine is wanted, it is a new script written against
the new engine's rules, not this one with a path changed.

---

## Repository map

Fifteen crates, and every one of them is on the path a program takes. Sixteen
were deleted on 2026-08-10 — the whole old runtime and its tooling — so a name
that is not here does not exist, and `git log --diff-filter=D` is where it went.

```
crates/
  rts-cranelift/     the machine: IR, repr, GC contract, frames, calls, unwind
  rts-codegen/       the language: JS/TS tree, parser bridge, emit, type pass
  rts-core/          the runtime: values, heap, objects, coercion, entry points
  rts-host/          where the three meet, and where a program runs
  rts-macro/         #[rtse::entry] / #[rtse::class] — declare one, derive it
  rts-std/           the `rts:` surface, and the globals
  rts-node/          the `node:` surface
  rts-ui/            `rts:egui` + `rts:input`, where a target has a screen
  rts-runtime/       the AOT staticlib the compiled program links against

  rts-egui/ rts-dom/ rts-render/ rts-input/   the UI engine, engine-agnostic
  rts-linker/        native link            rts-cli/  the CLI

  rts-napi/          N-API here, and a real npm addon RUNS: 146 symbols
                     exported, `rts napi <file.node>` loads and calls one
```

**One crate again.** There were two under this name — the second carrying an
`-rwk` suffix — while the old engine's version stood beside the rewrite to be
read from, because cargo will not have two crates of one name. The old one was
deleted on 2026-08-10 and the suffix came off the same day.

**What ended it was a number**: the old crate exported 145 distinct `napi_*`
names, this one exports 146, and the diff in the direction that matters is
empty. That is a claim about NAMES and not about behaviour — eight of the 146
answer a status rather than doing the work, each saying why where it is defined
— but it is the claim the suffix encoded: **a phase is finished when the old
code is gone** rather than when the new code exists.
`crates/rts-napi/README.md` keeps why it was a rewrite rather than a port, and
`PLAN.md` there has what is left.

**What went, and the one thing it cost.** `rts-engine`, `rts-primitives`,
`rts-shared`, `rts-std`, `rts-runtime`, `rts-natives`, `rts-abi`, `rts-macro`,
`rts-symbol-baker`, `rts-parser`, `rts-ast`, `rts-hir`, `rts-node`,
`rts-value-probe` and `rts-diagnostics` — the old runtime, the old ABI, the old
symbol table, the old front end and the old diagnostics. Nothing on the new
engine's path named any of them, which is why the deletion is mechanical rather
than a port. The exception was the old `rts-napi`, which named two of them
directly and was therefore never built again — it was deleted on 2026-08-10,
once the rewrite beside it exported every symbol it had.

`rts-diagnostics` is worth its own line because it looked alive. 733 lines of
rich diagnostics — codes, spans, notes, a snippet renderer, a process-global
engine — with **zero producers**: `emit()` was called from nowhere outside the
crate once the old parser went, so `has_errors()` was a constant `false` and the
branch reading it in `main` was unreachable. What replaces it is
`rts-cli::errors`, which is the `anyhow`-chain formatter that was doing all the
printing already. When a span comes back it will come from
`rts_cranelift::fault::Position`, and the renderer belongs beside that.

The four UI crates stay because they were never the old engine's: `rts-egui`,
`rts-dom`, `rts-render` and `rts-input` each had an `old-engine` feature holding
their ABI surface, and that feature is what was deleted. `rts-ui` consumes
them through their plain Rust API and always did.

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

cargo run -- run file.ts              # JIT — the NEW engine
cargo run -- -e "console.log(1)"      # inline source, same engine
cargo run -- ir file.ts               # the new engine's IR, no execution
target/release/rts.exe compile -p file.ts out   # AOT
target/release/rts.exe test tests/one.test.ts   # a single file
```

**Every command in that block now runs the NEW engine**, and the sentence that
used to be here said the opposite — truthfully, at the time. `run`, `test` and
`compile` cut over first; `ir` and `eval`/`-e` were the two left behind, and
being left behind was worse for them than for anything else: `rts ir` printed
the OLD engine's Cranelift IR, so the one command whose entire job is to show
what was emitted was showing a different compiler's output, and `-e` could
answer differently from the same source saved to a file.

`rts ir` prints `rts_cranelift::ir` — this engine's own representation, with a
callee legend at the top — and NOT Cranelift's `.clif`, which only exists inside
`lower/` after every decision this engine makes has been taken. `rts emit-types`
answers from `#[rtse::class]`, which is what let `rts-codegen-new` be deleted.

The two examples remain the way to run one program with nothing of the CLI in
the way:

```bash
cargo run -q -p rts-host --example run_fixture file.ts   # one program
cargo run -q -p rts-host --example suite_run tests/x.test.ts   # one test
```

`run_fixture` and `suite_run` are one process per file on purpose: an uncaught
exception and an endless loop each take the process with them.

**AOT links `rts-runtime`, and it is a direct dependency of the `rts` bin
for that reason** — Cargo emits a `staticlib` only for a package built as a
direct target, and being a dependency-of-a-dependency does not count. So an
ordinary `cargo build` produces it. `cargo build -p rts-runtime` is still
what to run after editing `rts-core`/`rts-std`/`rts-node`: nothing
rebuilds the archive because `rts` was rebuilt, and `rts compile` refuses a
stale one by name rather than linking it.

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
