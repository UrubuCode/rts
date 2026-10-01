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
| `crates/rts-mir/` | its `README.md` (12 rules). Two of them are checked by a command rather than by reading, and the README says which |
| `crates/rts-codegen/` | its `README.md` (10 rules) + `PLAN.md` |
| `crates/rts-core/` | its `README.md` (10 rules) + `PLAN.md` |
| `crates/rts-host/` | its `README.md` (6 rules) + `PLAN.md` |
| `crates/rts-egui/`, DOM, render, input | `docs/ui/html-engine/` + `docs/ui/egui-crate.md`; for the NEW engine's side of it, `docs/ui/new-engine-port.md` |
| `crates/rts-dom/`, `crates/rts-dom-bridge/` | the row above, PLUS `crates/rts-dom/PLAN.md` — §0 is the state (which lot is in flight, on which branch, measured how) and §1–§2 are the rules and the three rulers — and the verdict in `docs/ui/html-engine/analises/2026-09-04-auditoria-estrutural/README.md`, which is the current picture of the engine where the roadmap of June is the picture from before. **For anything that touches the LAYOUT itself, also read `docs/ui/html-engine/box-tree.md`** — the box tree is the layer this engine does not have and every other CSS engine does, and that document is binding for the five `BT-*` lots of PLAN.md §9. It carries the nine invariants that break SILENTLY when box identity stops being the DOM node; seven of them compile and lie |
| the PIPELINE itself — a new stage, a new IR, a type domain, a guard, speculation, deoptimisation | `docs/engine/four-stages.md`, and `docs/engine/deopt-lateral.md` for the second tier. The first records why `AST → machine IR` is two stages short, and the measured wrong answer that shortfall produces |
| **anything in the BASE — how a value is created, allocated, identified, traced or moved — or any new predicate, type test or fast path** | `docs/engine/principles.md` (P1–P8, each with the test a change is held against) and `docs/engine/one-form-per-question.md` (which form is canonical, and **for which actions**). Invoke `reuse-check` first: its section 0 is the search for the question rather than for the function |
| anything else | this file, and `docs/README.md` for where things live |

**Two rows were three until 2026-10-01, and the trim is the rule working on
itself.** A reading list is paid at every session and read in none of them once it
is long enough to skim — which is the failure this file's own header describes. So
a row earns its place by naming work somebody is about to do, never by being
true. If a row has not sent anyone to a document in a month, it is noise with a
citation.

If a change requires breaking a rule, **change the rule first, with the reason,
and get it agreed**. Never leave a rule the code contradicts.

Also: if `local-rules.md` exists at the root, reading it is mandatory. It is
per-developer, unversioned, and takes priority over general preference.

---

## RULE 0a — the MIR stage is the direction, and new work goes there

`emit/` is to be removed. `rts-mir` is not a tidier emitter: it is the only place
a **guess** can live, and a dynamic language needs one — no amount of
whole-program analysis settles every type, shape and callee in JavaScript, so a
compiler that only ever proves concedes every unproven case permanently.
`docs/engine/deopt-lateral.md` is the form the fall takes here (the generic body
of the same function, no interpreter) and `docs/engine/principles.md` P5 is the
rule it serves.

Two things follow, and both are binding:

- **A new language feature is written in `lower/` + `machine/`.** A change to
  `emit/` is made only to keep a program compiling while `through_mir.rs` still
  declines that function, and it is recorded as a refusal to be removed rather
  than as a feature. The audit of 2026-10-01 measured the cost of not doing this:
  37 002 lines against 11 191, with seven language features implemented twice,
  and the four-stages document records one bug that had to be found and fixed
  separately in each path.
- **Nothing is deleted from `emit/` before a differential test exists.**
  `RTS_MIR` appears in four places in this repository and **none of them is a
  test** — nothing runs one program through both paths and compares. Today the
  twenty hand-written refusals in `through_mir.rs` are what keep a disagreement
  from running; the moment `emit/` is gone they have nothing left to decline to.

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
TypeScript semantics. `rts-cranelift` is the machine — IR, representations, GC
contract, frames, calls, unwinding. The boundary means a decision has exactly one
place it can be made. Full picture: `docs/engine/architecture.md`.

**The boundary is NOT symmetric, and saying it was is what made it wrong.** This
paragraph read "the language knows no machine, the machine knows no language" —
and the first half was contradicted by the code the day it was written:
`emit/math/body.rs` says `Math.sqrt` is `FloatOp::Sqrt`, `emit/property.rs`
declares inline caches, and the door table names `RuntimeOp`s. A crate whose job
is to emit machine IR cannot not know the machine. Restated as three rules that
the code can actually hold:

- **`rts-cranelift` knows no language, and this one does not relax.** It is what
  makes the crate testable with no front end present (its rule 3) and it is the
  whole premise of `docs/engine/a-second-language.md`. A machine that knew what
  `Math.sqrt` was would be a machine with one possible client.
- **`rts-codegen` names machine operations freely** — that is the job — and does
  not **re-decide** what the machine owns: a layout, a convention, a barrier, a
  target's capability. Those it asks. The line is *naming* versus *deciding*.
- **A crate that declares a native may declare which OPERATION it is, never
  which instruction.** `#[rtse::…]` is used by `rts-core`, `rts-std`,
  `rts-node` and `rts-dom-bridge`, and "this function is square root" is the
  operation's identity, not a fact about any target — so it stays true on wasm,
  where there may be no instruction at all. The machine answers whether a target
  has one; the language says which JavaScript name means it.

**Why the third rule exists at all**, since it is the new one: thirteen members
of `Math` have two bodies today — the Rust one in `rts-core/src/entry/math.rs`
and the instruction in `emit/math/body.rs` — and nothing checks they agree.
Sixteen JavaScript edge cases were compared by hand on 2026-10-01 (`min(0, -0)`,
`round(-0.5)`, `round(0.49999999999999994)`, `clz32(NaN)`, `imul(NaN, 7)`) and
all sixteen agreed, so the risk is unrealised rather than absent — what keeps
them equal is care. With the operation named in the declaration, `rts-macro` can
derive the door, the five hand-written rows an entry point needs, the effect
summary MIR wants, **and the differential test that refuses a disagreement**.
Converting a module's operations into instructions is the point of the change,
and it is this rule that makes it derivable instead of hand-wired.

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
claimed. **2026-09-05: 833 of the 888 `*.test.ts` files pass** (4 038 of 4 131
assertions), by `target/release/rts.exe test`. It was 796 of 848 on 09-02, 758
of 819 on 08-29, 746
of 808 on 08-22, 754 of 808 on 08-15, 756 of 799 on 08-10, 626 of 797 on 08-09
and 535 of 818 at the start of 08-08, through generators, `yield*`, `Proxy`,
native iterators, `export *`, a catchable throw, the bare `rts` specifier, stack
traces, variadic natives and wrapper objects.

**The trail behind that number is `docs/engine/measured-history.md`** — every
earlier figure, the two drops nothing could attribute, the obfuscated corpus, and
the comparison against the engine deleted on 08-10. Moved there on 2026-10-01
because a number from 08-09 about a corpus that has since doubled is not
something a session needs in front of it. The rules those measurements produced
stayed here.

**One lesson from them is worth keeping in front of you**, because it is how to
read a number rather than a number: a generator defect was found by the SHAPE of
the answer, not by reasoning. `for (const v of g())` answered 63028 for N of
100 000, 200 000, 400 000 and 1 000 000 alike, and **an answer that saturates is
one event rather than a rate**. `docs/engine/lost-roots.md` has the account.
**`--profile fast` and not `--release`, and that is allowed here for the reason
the merge-gate section gives**: `fast` differs from `release` in optimisation
quality only, and "did this file pass" is not a question a profile changes the
answer to. A NUMBER about speed still needs `release`.

**And the second ruler: the cross-runtime fixtures, and this file no longer
carries the number.** It asks a different question from the one above — whether
this engine and a real one agree about the same program, where `*.test.ts` asks
whether the program does what it says — and it is run one process per file,
against Bun and Node, by the `cross-runtime` job of `build-artifacts.yml`.

**The share lives in `README.md`, between the `CROSS_RUNTIME_STATS` markers, and
it is generated rather than typed**: that job rewrites the block on every run.
Read it there.

What is worth keeping written here is the part the generated block cannot say.
**A share that falls because the ruler got longer is not a regression.** The
corpus went from 762 files to about 1 516, roughly doubled, because the old one
had been very nearly exhausted — which is what a corpus is FOR, and also what
makes it stop measuring anything. Two shares across that boundary are not
comparable in either direction; the counts are. And a handful of fixtures are
outside the denominator entirely, because Bun and Node disagree with each other
and the harness refuses to arbitrate.

**And a rule about the standard's own conformance suite: it is NOT a ruler
here.** No share, score or rate against it is published by this project — not in
`README.md`, not in a badge, not in a crate's documentation, not in a commit
message. A percentage stated beside that suite's name is read as a result *of*
the suite however the sentence around it is written, and the licence forbids
using the authors' names to promote what derives from them. Declining to state
one is the only form of that condition which does not depend on the reader.
`THIRD-PARTY-NOTICES.md` is binding, and its *Other works consulted* entry is
where that suite is recorded: read while writing two of `rts-codegen`'s test
files, nothing of it vendored, no figure from it stated.

It covers the imported corpora that ARE rulers here — V8 `mjsunit`, WebKit
`JSTests/stress` and Node's `test/` — under their own terms, and
`docs/engine/importable-suites.md` is what else could be imported and what each
one would cost.

**A change to this engine is compared PER FILE against a kept binary**, which is
the only form the claim "no regression" takes here, and
`scripts/cross_runtime_check.sh` with `RTS_BIN` and `REPORT_FILE` is how. It
takes about seven minutes a side on eight jobs.

**What that job cannot do is fail.** `cross-runtime`, `node-suite` and `ts-suite`
are all `continue-on-error: true`, and `node-suite` additionally runs only on
`schedule`. So every ruler in CI reports and none of them gates: the one blocking
signal in the whole workflow is that the `build` job compiled. That is a decision
recorded in each job's own comment and not an oversight, but it means a falling
share is noticed by a person reading a badge, never by a red check.

The ceiling under the number is worth reading with it: **five of the 708 have no
comparable answer**, because Bun and Node disagree with each other and the
harness refuses to elect one of them. So the reachable total is 703, not 708.

The rulers differ in one stated way: `rts test` also
compares stdout against a fixture where one exists, which `suite_run` never
sees; both require "ran and nothing failed", which is what makes the counts
comparable at all.

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
carries `num`, `math`, `hint`, `time`, `gc`, `atomic` and `operators` — the
symbols that opt an object into operator overloading
(`[operators.add](other, reversed)`); only an object declaring one is
overloaded, never a method merely NAMED `add`, and `tsc` still flags the
operator expression. `docs/engine/operator-overloading.md`. Still wanted from
what the old engine provided: `io`, `buffer`, `net`, `fs`, `process`. `rts:serde` is
back — the pickle, RTSP v2, reading v1 saves too: class instances and top-level
functions by name (AOT included), schema versions. `docs/engine/pickle.md` is the
stream, the semantics and the cost; `node:v8` and `Storage` write the same bytes.
A program that cannot reach it pays NOTHING for it: the compiler emits the
class/function registration only when some module imports `rts:serde` or
`node:v8`, computes an `import()`/`require()` specifier, or uses `eval`, and
`serde_declare_gate.rs` counts it in the IR.

**GONE by decision, and their tests with them** — `ptr`, `mem`, `alloc`, `ffi`,
`trace`, `sync`, `thread`, `promise.new_*`, and `RtsePoint`. The first five left
earlier because they return in another shape or not at all; `sync` and `thread`
left on 08-10 for a reason worth keeping written down.

`thread` needs a program to be able to SPAWN one and to hand it work, and that
is what does not exist — not the threads. **Compiled code already runs on
several OS threads, each with a heap of its own, and it is tested**: the JIT's
`Compiled::run_on(n)` takes one `Region` per thread out of a program placed by
`compile_for(source, n)`, opens a `std::thread::scope`, and `crates/rts-host/
tests/threads.rs` asserts that N threads run at once, that each allocates in
its own region, and that two never hand out the same reference. The reference
encoding was built for it — `(cell << selector_bits) | region` — and
`gc::barrier_for` emits `BarrierKind::CrossRegion` on a reference store the
moment a program is placed for more than one region.

This paragraph said the opposite until 2026-10-01, and the sentence was read
straight out of the `sync` argument below without checking the host. What is
genuinely absent is **sharing**: nothing publishes a reference from one thread
to another — no channel, no shared global, no way to pass one — so
`entry::barrier`'s remembered set is correct and empty, `Region::Shared` is a
case the machine models and nothing allocates into, and the collector scans
one thread's own stack. So a `thread` namespace would still be a name with
nothing behind it, for a different reason than this file gave: the threads run,
and a program can neither start one nor say anything to it.

**And the AOT side is a region behind the JIT**: `object/mod.rs` hardcodes
`RegionBases::single` with a symbolic base, and `rts-runtime-boot` builds one
`Region::with_capacity` and one `Context`. A compiled binary is single-threaded
by construction rather than by configuration, which is three named pieces of
work — the count through `compile_to_object`, a symbolic base TABLE instead of
one base, and the count in the object's manifest — and not a difference in
kind.

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
cargo run -- prove file.ts          # where it is settled, and where it falls back
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

**`rts compile` works from it now, and the sentence that stood here said it did
not.** It was true and it was two hardcoded strings: `runtime_archive()` looked
under `target/{release,debug}` and nowhere else, so a freshly built
`target/fast/rts_runtime.lib` was invisible and the command fell through to the
embedded archive — a placeholder unless that binary's own build had found a
staticlib. It asks `current_exe()` for the profile it is running under first,
which is the question that was being guessed at. `cargo build --profile fast -p
rts-runtime` beside the binary, as with every other profile.

A binary compiled that way is still a `fast` binary and still runs
`bench/objbench.ts` 20.8% slower. `fast` answers "is it correct"; a NUMBER
needs `release`.

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
  is not hypothetical: a share over a public corpus was published 0.8 points high because 503
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
re-invokes `current_exe()`, which is profile-agnostic.

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
`language` and `regexp_patterns` — **93 tests did not run for six
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

**The ceiling binds the system's own code, and not a test or benchmark corpus**
— agreed 2026-09-06, when `bench/analytic.ts` passed two thousand lines. What
the rule protects is coupling: that a change lands in a focused module instead
of being appended to something oversized. A corpus has no coupling to protect —
its cases are independent, and case 200 cannot be made worse by case 199
existing — so length there is coverage rather than debt. `bench/analytic.ts`
carries the argument in its own header, including the reason it cannot be split
at all: it runs unmodified under `rts`, `node` and `bun`, so it can have no
imports.

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

**Twenty-one crates, counted on 2026-09-20 rather than carried forward.** Every
one of them is on the path a program takes. Sixteen
were deleted on 2026-08-10 — the whole old runtime and its tooling — so a name
that is not here does not exist, and `git log --diff-filter=D` is where it went.

This line has now been wrong twice in the same way. It said "Fifteen" while the
block below listed sixteen and the directory held eighteen; the structural audit
of 2026-09-04 corrected it to eighteen, and the directory held **twenty** that
day — `rts-runtime-boot` and `rts-runtime-jit` are in the workspace and have
never been in this block. So the count is now taken from `ls crates | wc -l` and
the two missing names are listed, which is the only form the honesty floor's
"verify the input" takes for a map.

```
crates/
  rts-cranelift/     the machine: IR, repr, GC contract, frames, calls, unwind
  rts-mir/           the shared mid-level IR: CFG in SSA, effects, guards, two
                     tiers, and a type domain the LANGUAGE declares. Neither
                     front end's, which is why it is not inside either
  rts-codegen/       the language: JS/TS tree, parser bridge, emit, type pass
  rts-core/          the runtime: values, heap, objects, coercion, entry points
  rts-host/          where the three meet, and where a program runs
  rts-macro/         #[rtse::entry] / #[rtse::class] — declare one, derive it
  rts-std/           the `rts:` surface, and the globals
  rts-node/          the `node:` surface
  rts-ui/            `rts:egui` + `rts:input`, where a target has a screen
  rts-runtime/       the AOT staticlib the compiled program links against
  rts-runtime-boot/  and rts-runtime-jit/ — the two halves of that archive a
                     compiled program and a JIT run need separately. Absent from
                     this block until 2026-09-20, which is why the count was two
                     short twice
  rts-physics/       `rts:rigid` — the rayon rigid-body solver, the CPU
                     fallback for a GPU-first scene; its own crate because wasm
                     has no threads

  rts-egui/ rts-dom/ rts-render/ rts-input/   the UI engine, engine-agnostic
  rts-dom-bridge/    `rts:dom` — the document reachable from TypeScript without
                     a window, and the scope a page `<script>` compiles against
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
cargo run -- prove file.ts            # the two tiers, counted per function
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
`lower/` after every decision this engine makes has been taken.

**`rts prove` is the summary over that same IR, and it answers a different
question**: not *what does this compile to* but *where did the proofs stop*. It
splits every function in two — what runs when every speculation holds, and the
second tier under it — and counts the widenings, guards, cached accesses and
runtime operations in each. That split is the point: counting them together says
a class method asks the runtime four times to read two fields, when the armed
path asks it none, and the first version of the command did exactly that.

Read it as counts and never as costs, and compare two reports of **the same
program** across a change. There is no honest denominator for ranking two
different programs, and its own module says why. It is what found that a string
literal written inside a loop was crossing into the runtime on every pass.

`rts emit-types`
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
