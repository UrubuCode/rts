# docs/codegen — the optimizations, and what makes one real

This tree exists because "make it faster" is the request most likely to produce
work that does nothing. An optimization is a **claim about a machine**, and a
claim about a machine that nobody measured is a rumour with an implementation
attached — worse than no change, because it is now in the code, it has to be
maintained, and the next person assumes it was measured.

So this is not a list of ideas. It is a list of **questions that were settled**,
each with the experiment that settled it, and that includes the ones settled
*against* the idea. A refuted candidate gets a document here exactly like a
successful one, and for a better reason: the successful one is visible in the
code, and the refuted one is invisible — without a document the next person
spends the same day rediscovering that the answer is no.

It is the sibling of `crates/rts-cranelift/README.md` and
`crates/rts-codegen/README.md`. Those say what the machine layer and the
language layer *are*, and their rules are binding for changes inside a crate.
This one says how a change that claims to be **faster** earns its way in, and its
rules are binding for any change whose justification is speed — in any crate.

---

## Where this sits in the documentation tree

`docs/README.md` states four homes and a rule for each, and the failure it was
written against was that nothing said where a new document belonged. This is a
fifth, and it is registered there rather than merely added here, because a home
that only one of the two files knows about is the pile coming back.

The boundary against `docs/engine/`: that tree answers **how the compiler works
and why it is built that way**. This one answers **what an action costs and what
was tried about it**. A document that would still be true if every number in it
were deleted belongs in `engine/`. A document that is *about* the numbers belongs
here.

---

## The rules

### 1. An optimization is proven in isolation before the engine is touched

Not after. Not "implement it and measure the suite". **Before.**

`bench/isolated/` is a standalone package — no workspace, no dependencies, a
release build in about a second and a half — whose whole purpose is to answer
"does shape B cost less than shape A" without paying for a full workspace build.
Write the two shapes there, run them, and only then decide whether the engine
gets touched.

Why this rule and not the obvious one: because the obvious loop is
*implement → build release (minutes) → measure the suite → discover it was
worth 0.5 ns → keep it anyway, because it is already written*. Every step of
that is a cost, and the last one is a defect. The isolated experiment costs
seconds and its answer arrives while the idea is still cheap to abandon.

**This is the rule that produced this tree's first document, and that document
says no.** See `entry-tax.md`: the leading hypothesis about why forty rows of
`bench/analytic.ts` sit at 16–30 ns was that every runtime entry point pays for
a `RefCell<Vec<Context>>`. It does. It is worth 0.55 ns. Had that been
implemented first, it would have been a day of work, a real regression risk
around re-entrancy, and a 3% win presented as the answer to a 30× gap.

### 2. Isolation is a gate, not a forecast

> **It has forecast well once, and that is not the claim.** The region zero-fill
> (`startup.md`) was priced at **1.478 ms** in `bench/isolated/` and measured at
> **1.51 ms** in the engine — 2% apart. The `ElementLoad` fast path
> (`element-load.md`) would have priced at 5–8× on the load and **broke
> programs**, because what an isolated experiment cannot see is that the
> surrounding program stopped keeping something alive. Both outcomes come from
> following this rule; only the second one is the reason it is written as a
> gate.

An isolated number says *this shape of Rust costs that much, compiled that way,
on this machine, with its operands in cache*. It does **not** say what the
engine will do, because the engine calls that shape across an `extern "C"`
boundary from compiled code, with different register pressure and a cold cache.

So the logic runs one way only:

- Loses in isolation → **it will not win in the engine.** Refuse it, document it,
  do not build.
- Wins in isolation → it *may* win in the engine. Build it, then measure the
  engine, per file, against a kept baseline binary.

A document that reports only the isolated number and calls the work done has
skipped half of itself.

### 3. A number carries its date, its machine and its ruler

`docs/README.md` rule 5 already says a percentage with neither date nor source is
a rumour. Here it is stricter, because everything in this tree is a number:
state the **date**, the **binary that produced it** (release — never `debug`,
never `--profile fast`, whose own documentation in `CLAUDE.md` measures it 20.8%
slower on `bench/objbench.ts`), the **file that was run**, and **what it was
compared against**.

A number that is not re-measured after the code moves becomes a claim. When a
document's number goes stale, re-measure it or delete it — the rule
`docs/README.md` states for lying documents is not suspended for fast ones.

### 4. The rulers, and what each one asks

Three, and they ask different questions. Quoting one to answer another is how a
green number and a slower program coexist.

| ruler | asks | run it with |
|---|---|---|
| `bench/analytic.ts` | what does one *action* cost, in ns | `rts run bench/analytic.ts`, and the same file under `bun` and `node` |
| `bench/objbench.ts`, `monte_carlo_pi.ts`, `pi_machin.ts`, `field_access.ts` | what does a *program* cost | `rts run <file>` |
| the corpus | did anything **break** | `scripts/cross_runtime_check.sh`, `rts test` |

`analytic.ts` is an **attribution** instrument and says so in its own header: it
runs one action in a loop with its operands in hand, which is the best case for
caches and the worst case for representativeness. An action that is fast there
can still be slow in a program that reaches it once. Never ship a change on an
`analytic.ts` row alone — a program ruler has to move too, or the document has to
say why it did not.

### 5. Compared per row, never net

`CLAUDE.md` states this for the test suite and it is the same rule for speed:
"+3%" is equally consistent with everything improving and with two things
improving while a third regressed. Keep the *before* table, keep the *after*
table, and put the rows that went the wrong way in the document.

The baseline is a **binary you keep**, not a stash you take:

```bash
cargo build --release && cp target/release/rts.exe target/baseline.exe
./target/baseline.exe run bench/analytic.ts > before.txt
# ... make the change ...
cargo build --release
./target/release/rts.exe run bench/analytic.ts > after.txt
```

### 6. Say what it costs

Every optimization costs something: memory, compile time, binary size, a
semantic that now has to be guarded, a rule in a crate README that has to
change, or complexity someone will maintain. A document that names no cost has
not finished looking. `crates/rts-core/src/heap/region/mod.rs` is the model —
raising `INLINE_SLOTS` from 7 to 15 is documented there with the 23.5% it cost
`objbench.ts` stated beside the 94% it bought a property read.

### 7. One question, one document, and the index is here

`docs/README.md` rule 3. Two documents about one cost will disagree, and the
first person to notice will have read the wrong one. The table below is the
index; a document not in it is not findable.

### 8. A landed optimization says what it actually moved

Write the *before* numbers when the experiment is done, and come back and write
the *after* numbers when it ships. A document that stops at "expected win" is a
plan, and plans live with the thing they plan (`docs/README.md` rule 2), not
here.

---

## How to run an experiment

```bash
cd bench/isolated
cargo run --release --bin <name>          # about 1.5 s from cold
```

Each experiment is one `src/bin/*.rs`. It states the question in its module
documentation, quotes the engine code it is modelling with a `file:line`, and
prints a table whose **first row is the shape the engine has today** — so the
ratio column reads as "what changing it would buy" rather than "how far the
winner is from the loser".

`src/lib.rs` is the harness: a calibrated iteration count (a case is grown until
it runs 40 ms, so a 0.5 ns case and a 5 µs case can share a table), best-of-three
rather than the mean (the distribution is one-sided — interrupts only make a run
slower), and a checksum every case feeds, so an optimiser that deletes a body
shows up as a number too good to be true rather than as a fast one.

The rules that make an experiment honest are the ones `bench/analytic.ts`
already documents for itself. This harness inherits them deliberately.

---

## The index

| document | the question | settled |
|---|---|---|
| [`measurements.md`](measurements.md) | what does every action cost today, against `bun` and `node` | 2026-08-21 |
| [`plan.md`](plan.md) | what is worth doing next, and what is already settled against | 2026-08-21 |
| [`entry-tax.md`](entry-tax.md) | is the `RefCell<Vec<Context>>` behind every entry point why the runtime costs 16–30 ns | **no** — 0.53 ns of it |
| [`hot-path-hygiene.md`](hot-path-hygiene.md) | four things a hot path was doing that nothing asked it to | done, measured |
| [`startup.md`](startup.md) | where the 19.9 ms of `rts run empty.ts` goes | attributed; three items fixed |
| [`the-missing-pass.md`](the-missing-pass.md) | what the absent IR pass costs a loop | measured; `ToInt32`-of-a-constant is 3.08 ns |
| [`element-load.md`](element-load.md) | can the dead bounded-load fast path for arrays simply be switched on | **no** — it drops the array's only root |

---

## What is not here

**A crate's own queue.** A phase list for one crate is a plan and lives in that
crate — `crates/rts-codegen/PLAN.md`, `crates/rts-core/PLAN.md`.

[`plan.md`](plan.md) is the exception, and it is stated rather than quietly
allowed because this file said the opposite when it was written this morning.
The reason it changed: **a performance work list is cross-crate by construction.**
The largest item on it removes a call emitted in `rts-codegen`, deletes an entry
point in `rts-core`, and changes a signature `rts-host` wires — filing it under
any one crate would put two-thirds of it somewhere nobody looking for it would
read. The rule that survives is the one underneath: a plan lives with the thing
it plans, and the thing this one plans is the boundary between four crates.

It is also not a list of ideas. Every item on it carries the experiment that
would settle it and the file:line its cause was verified at, and the items that
were *refuted* are on it too, with the reason — which is what keeps it from
becoming the pile of stale intentions `docs/README.md` rule 2 is written against.

**How the compiler works.** `docs/engine/architecture.md` and the crate READMEs.
A document here may say *what a mechanism costs*; it does not re-explain the
mechanism, because that would be two answers to one question.

**Claims.** If it has no experiment, it is not a document, it is an idea. Ideas
go in an issue.
