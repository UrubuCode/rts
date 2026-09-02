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
and sockets, modules — **both systems, in any file** — template literals,
regular expressions, destructuring,
spread, `for-of`, and the built-ins a program reaches by name: the `Error`
family, `Math`, `JSON`, `Map`, `Set`, `Promise`, `Date`, `Symbol`, `Intl` — its
seven services over real CLDR data, not a table of English — plus what
`node:` provides.

**CommonJS is not a second module system here, and there is no per-file
choice.** `require`, `module`, `exports`, `__filename` and `__dirname` are bound
in every module that mentions them, beside `import` and `export` in the same
file if that is what the file writes. No extension rule, no `"type"` in a
`package.json`, no refusal — which is affordable because `graph.rs` already
emits every file of a program into ONE compilation, dependencies first, so the
evaluation difference the split exists to protect against does not arise.
`docs/engine/architecture.md` has the design and the one divergence it costs: a
UMD bundle's `typeof module` sniff now takes the CommonJS branch everywhere.

`crates/rts-host/tests/running.rs` is what says so — every test in it runs
the program rather than inspecting it — and the number is measured rather than
claimed. **2026-09-02: 792 of the 845 `*.test.ts` files pass** (3 396 of 3 454
assertions), by `target/release/rts.exe test` at `891b767c`. It was 758 of 819
on 08-29, 746 of 808 on 08-22, 754 of 808 on 08-15, 756 of 799 on 08-10, 626 of
797 on 08-09 and 535 of 818 at the start of 08-08, through generators, `yield*`,
`Proxy`, native iterators, `export *`, a catchable throw, the bare `rts`
specifier, stack traces, variadic natives and wrapper objects.

**One of the 53 fails BY DESIGN, and reading the number without this reads it
wrongly.** `tests/generator_for_of_root.test.ts` records an OPEN defect — a
generator lost by the conservative stack scan — and it used to PASS on the build
that had the bug. What changed on 09-02 is `rts:test`'s own `expect()`: it
allocated eight times per assertion (a plain object plus seven freshly built
matchers) and now allocates once, through a shared prototype. Less allocation
between the `for`-`of` and the collection stopped hiding the defect, so two of
that file's five cases now fail with WRONG ANSWERS — 77994 for 120000, 1 for
20000 — on a plain release binary with no flag.

That is the file becoming a REPRODUCTION rather than a guard, which is the
direction to want: a test that passes because the harness around it is noisy is
not passing. The defect is older and worse than it was recorded as — on
`a3de2a3f`, untouched and outside the harness, a `yield*` loop that should
answer 120000 answers **5970** — and `docs/engine/lost-roots.md` carries the
reproducer plus the three candidates excluded by measurement so far.

**`--profile fast` and not `--release`, and that is allowed here for the reason
the merge-gate section gives**: `fast` differs from `release` in optimisation
quality only, and "did this file pass" is not a question a profile changes the
answer to. A NUMBER about speed still needs `release`.

**The share fell by eight between 08-15 and 08-22 and this line does not claim
to know why.** What it can say is what the drop is NOT: each of the 62 failing
files was re-run against a kept binary of the tree at `97f66385`, and **all 62
fail there too** — 13 on an assertion and 49 on an uncaught exception. So none
of them is a regression from the optimisation work of 08-21, and the comparison
is per file rather than net, which is the only form the claim takes here.

The corpus itself did not move (808 both times), so the eight are files that
stopped passing somewhere in the ninety commits between the two measurements —
or on 08-15's own machine state. `node_fs`, `node_dns`, `node_tls`, `node_dgram`,
`net_*`, `tls_*` and `gpu_compute` are 36 of the 62, which is where to look
first.

**The share fell between 08-10 and 08-14 and nothing here claims to know why.**
The corpus grew by nine files and there were commits between the two
measurements that neither was taken across, so the drop is not attributable to
anything by subtraction. What IS attributable was measured per file, against a
binary of the tree as it stood before the work: 748 → 750 → 754, six gained,
**none lost**. That is the only comparison a number of this shape supports —
and the file that used to HANG is gone from the column:
`for_await_break_return.test.ts` timed out on every run until `for`-`of`
stopped materialising its sequence.

**And the second ruler: 1 179 of 1 514 cross-runtime fixtures** (77.9 %),
measured 2026-08-28 by the `cross-runtime` job of `build-artifacts.yml`, one
process per file, against Bun and Node. That corpus is a different question from
the one above — it asks whether this engine and a real one agree about the same
program, where `*.test.ts` asks whether the program does what it says.

**That share is lower than the one this line used to carry, and the reason is the
denominator.** It said 728 of 762 (95.5 %) for 2026-08-15, and the corpus is now
**1 516 files**. The old one had been very nearly exhausted — which is what a
corpus is FOR, and also what makes it stop measuring anything — so it was
roughly doubled. A share that falls because the ruler got longer is not a
regression, and the two numbers are not comparable in either direction.

Read them as what they are: 728 files agreed with Bun and Node in August, and
1 179 do now. **335 are left to fix** — 247 answering differently and 88 ending
in a runtime error — and one is skipped because Bun and Node disagree with each
other, which the harness refuses to arbitrate.

The number in the README is the one CI writes, and it is generated rather than
typed: the `cross-runtime` job rewrites the block between the
`CROSS_RUNTIME_STATS` markers on every run. **Read it there rather than here**,
because this file is edited by hand and has already been the stale half of this
pair once.

**What that job cannot do is fail.** `cross-runtime`, `node-suite` and `ts-suite`
are all `continue-on-error: true`, and `node-suite` additionally runs only on
`schedule`. So every ruler in CI reports and none of them gates: the one blocking
signal in the whole workflow is that the `build` job compiled. That is a decision
recorded in each job's own comment and not an oversight, but it means a falling
share is noticed by a person reading a badge, never by a red check.

It was 674 of 708 before an earlier growth of the corpus, and 666, 646, 630, 593
and 419 earlier in the same stretch. Every step between those figures was
measured PER FILE against a kept binary and cost **nothing**: the LOST list is
empty at each one, which is the only form the claim "no regression" takes here.
The net number never was.

**The denominator moved by 54, and both halves are stated because of it.** Those
are `tests/cross-runtime/obfuscated/` — real `javascript-obfuscator` output over
seeds that each exercise one area. An obfuscator emits legal JavaScript nobody
writes by hand, which is the syntax a hand-written corpus never reaches, and the
first run of it found three bugs on a tree that had just measured 674 of 708:
**twelve programs HUNG** because a name assigned in a loop's test carried nothing
across the back edge, five were refused for `super[e]`, and five answered wrongly
because a computed method key came out enumerable. None of the three needed an
obfuscator to be reachable. `scripts/obfuscated/README.md` is how to make more,
and says why a name already in the corpus is never re-emitted.

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

**When you need a real binary and not a number**, there is a third profile:

```bash
cargo build --profile fast          # 7m11s against 9m24s, same machine
```

Measured 2026-08-20 on a clean build. It is `release` with `lto = false` and
`codegen-units = 16`, and the reason it exists is the shape of the cost: the
build is **not** limited by width. `cargo build --timings` counts 2 586 s of
CPU in 564 s of wall clock, so doubling the workers from 8 to 16 buys 10% —
against 28% for dropping the setting that removes parallelism *inside* each
crate.

It is not for measuring, and that is not a style rule. The same tree built that
way runs `bench/objbench.ts` **20.8% slower**; `kernel` 5% slower, a remainder
loop 1.6%, `mc_noparam` unchanged. A number from a `fast` binary is a number
about a build nobody ships.

`rts compile` (AOT) does not work from it either — the runtime archive is
looked up under `target/{debug,release}`, not `target/fast`. `rts run` and
`rts test` do.

**Its second use is the merge gate's test step, and that is where it pays most.**
`cargo test --profile fast` over the four engine crates is **5m07s against ~30
minutes** for the same command under `--release`, same verdict both ways. The
next section carries the measurement and why a profile cannot change the answer
to the question those tests ask. The number above — 7m11s against 9m24s for a
binary — is the *small* half of what this profile is worth: a binary is one link
and the tests are forty-one.

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
- **A green suite is not the last gate — the clock is.** A disabled optimisation
  passes every correctness test there is. A guard written on 2026-08-29 turned
  the whole inliner off; the corpus, the unit tests and the doctests were all
  green, and the only thing that said otherwise was a benchmark returning to its
  old number. So after any change justified by speed, MEASURE AGAIN even once
  the reason to measure has been satisfied. `crates/rts-codegen/README.md` rule
  11 has the other three gates and what each of them caught.
- **The worst failure here is silent, and it is a CLASS rather than an
  incident.** What the collector treats as live is decided by two hand-written
  lists, so a live reference can simply be missing from one — and what that
  produces is not a crash but a `for`-`of` that ends early, or a `JSON.parse`
  that answers objects with no properties while the process exits zero. Both
  were real on 2026-08-29, alongside a third that exhausted the heap.
  `docs/engine/lost-roots.md` is the class, the four checks that find the next
  one, and the reason to expect one; `crates/rts-core/README.md` rule 10 is the
  binding form. **Expect more of these** — every new side table, native and
  cache is a fresh chance to be missing from a list, and only the precise roots
  of `docs/engine/the-unwired-keystone.md` close the class rather than police
  it.
- **A second silent class, and it is not about memory: a rule applied in the
  wrong ORDER.** Every test here asserts an ANSWER, and an answer cannot tell a
  conversion that was needed from one that was not. `x == null` ran
  `ToPrimitive` on the object — two `valueOf` calls per comparison where the
  specification calls it zero times — and answered correctly the whole time, at
  180 times the cost. What found it was a benchmark row reading 1 456 ns where
  the model said 14; what PROVES it is a counter on the side effect, never an
  assertion on the result. **Expect more** wherever this runtime converts before
  it dispatches: the specification almost always has cheap arms ahead of the
  conversion, and putting the conversion first is the natural way to write the
  function. `docs/codegen/entry-tax.md` part five is the class and the shape of
  the test that catches it.

---

## MANDATORY: regress explicitly, never silently

Regression is allowed when necessary. It must be **stated**.

Before merge:

```bash
cargo build --release
cargo test --profile fast --no-fail-fast -p <each crate you touched>   # NAME them, and see below
target/release/rts.exe test          # if the change touches runtime/codegen/GC
```

**`--profile fast` and NOT `--release`, and the difference is 25 minutes.**
Measured 2026-08-23, same four crates, same tree, same verdict — `309 passed;
3 failed` both ways:

| | wall clock |
|---|---:|
| `cargo test --release` | **~30 min** |
| `cargo test --profile fast` | **5 min 07 s** |

The cost is not the tests, it is the LINK. `[profile.release]` carries
`lto = "thin"` and `codegen-units = 1`, and **every test target is its own
binary** that inherits both — 41 files across `tests/` in the four gated crates,
so 41 thin-LTO links of the whole engine. That is why the binary alone builds in
1m21s and its tests take thirty. `fast` is the same profile with `lto = false`
and `codegen-units = 16`.

**Why this is safe for a gate and not for a number.** `fast` differs from
`release` in optimization quality only; the per-package `opt-level = 3`
overrides, `debug-assertions`, and everything a test can observe are inherited
unchanged. Cargo also forces unwinding for test targets, so `panic = "abort"`
never applied to them either way. Checked rather than assumed: **no test in the
gated crates does AOT or names a `target/…` path** — `exhaustion.rs`
re-invokes `current_exe()` and `test262.rs` invokes `git`, both profile-agnostic.

**What it is still NOT for**, and the ITERATION SPEED section already says both:
a `fast` binary runs `bench/objbench.ts` 20.8% slower, and `rts compile` cannot
find the runtime archive under `target/fast`. So: **`fast` answers "is it
correct", `release` answers "how fast is it".** A benchmark number from a `fast`
binary is a number about a build nobody ships.

**And the limit worth stating, because this repository has already paid it once.**
A green suite is not proof that two builds are the same program. The
`single_pass` register allocator passed all 800 `*.test.ts` files and segfaulted
the largest program in this workspace, every run — the corpus is small files and
the defect needed a big one. So the release build above the test line stays, and
`target/release/rts.exe test` stays: what `--profile fast` replaces is the Rust
unit and integration tests, which are the part that costs thirty minutes and asks
a question the profile cannot change the answer to.

**`--no-fail-fast`, and `--lib` is not the whole crate.** Cargo runs a crate's
test targets in NAME order and stops at the first that fails, so **how much
coverage a red test hides is decided by the alphabet**. In `rts-codegen`, two
stale fixtures in `tests/bridge.rs` stopped the run before `early_errors`,
`language`, `regexp_patterns` and `test262` — **93 tests did not run for six
days**, and nobody knew whether they passed. Had the red target been the last
one, it would have hidden nothing.

The same line is why `--lib` alone is not enough where a crate has a `tests/`
directory: `rts-cranelift` has 67 unit tests and **230 integration tests**, and
`--lib` reports the first number as if it were the answer.

This is a different failure from a wrong number, and worse: a wrong number is
corrected when someone compares it to reality, but **a suite that does not run
produces nothing to compare** — and empty looks exactly like green at the place
where anyone looks. The mechanism is not a broken tool; it is a correct tool
that gives up early.

**`cargo test --lib` with no `-p` is not a gate**, whichever profile it runs
under. At the workspace
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

Ceilings: **the two engine crates ≤ 1000**, **everything else ≤ 500** — as each
crate's README states, and they are the binding text. This line said
"engine ≤ 700" and that number appears in no README: `rts-cranelift` and
`rts-codegen` both say 1000, `rts-core` and `rts-host` both say 500, and
`rts-core`'s says *"the same ceiling as the rest of the workspace outside the two
engine crates"*, which settles it. A summary that contradicts its sources sends
work to a file that already complies — one file of 1 366 lines was on a list for
that reason alone. A file
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
