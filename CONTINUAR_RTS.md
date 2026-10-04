# CONTINUAR_RTS.md

Where the performance work stands, what it measured, and what to pick up next.
Written 2026-10-03. English because `CLAUDE.md`'s conventions say documents are.

**This file is a handover, not a plan.** It records numbers and the method that
produced them, so the next session starts from a reading instead of from an
intuition. Every figure here was taken with `target/release/rts.exe`, one row
per process, interleaved against a kept baseline binary. Where a figure is a
*bound* rather than a reading, it says so.

---

## 0. READ THIS FIRST — the repository state is not what it looks like

`main` here is **not** `origin/main`, and they have diverged:

```
git rev-list --left-right --count main...origin/main
9   27
```

- **9 commits are local only** and are yours from 2026-10-03: the frame-pointer
  chain, the trace join, `rts-profile` and its deletion, the crate-map recount.
  They have never been pushed.
- **27 commits are on the remote only**, and they are real work — decorators
  (#2904), a lookbehind of unbounded width inside a branch (#2891/#2900), an
  ambient module erased whole (#2901/#2902), an antivirus gate (#2908), plus the
  generated badge and benchmark commits.

**Everything below was measured against `08939486f`** — the tip of the LOCAL
`main` — and not against the remote. A base 27 commits newer is a different
program: the numbers need retaking and the per-file suite comparison needs
rerunning before any of it is quoted on top of `origin/main`.

Nothing was pushed and `main` was not touched.

---

## 1. What is committed, on `fix/instruments-that-could-not-report`

Four commits on top of `08939486f`. All four passed the gate below.

| commit | what |
|---|---|
| `e65ad7fc3` | `RtEntry::LAST` + a third test: a variant missing from `ALL` is now a failing test |
| `0e3069065` | the trace diagnostic survives a bad link, counts what it attributed, and records the measurement that refutes its own module's prescribed remedy |
| `d5e1e5425` | `new Map()` / `new Set()` without the construction door — **172 → 120 ns** |
| `d8e9e0213` | two text caches — `.stack` **−188 ns**, `Error` construction **−78**, small-integer `toString` **70 → 12** |

### The gate every one of them passed

```bash
cargo build --release
cargo test --profile fast --no-fail-fast -p rts-cranelift -p rts-core -p rts-codegen -p rts-host
target/release/rts.exe test          # 939 of 975 files
```

and then the only form in which "no regression" is a claim here — **per file**,
never net:

```bash
target/baseline.exe test > base.raw 2>&1
target/release/rts.exe test > now.raw 2>&1
# LOST = fails now, passed on the baseline
```

A net number cannot carry the claim: this session went 939 → 935 and the net
said "−4" while the per-file set named one file, `tests/map_set_size_chain.test.ts`,
and that file was the whole of it.

---

## 2. The map, in measured order

A sweep of 38 common operations, one row per process, `08939486f`. Rows that
iterate a 64-element array are given per element.

| | ns/op | what it is |
|---|---:|---|
| `regex.exec` with a group | 1064 | ~990 above `RegExp.test` — materialising the match. Plan item 10.2b. |
| `arr.join` | 44 /el | the result's own allocation |
| `obj.spread {...}` | 261 /field | own-key enumeration |
| `arr.map` | 31.8 /el | callback door + allocation |
| `arr.forEach` | 24.1 /el | the callback door |
| `str.split(",")` | 150 /piece | |
| `Object.keys` | 162 /key | already optimised twice; near this design's floor |
| `arr.for-of` | 9.6 /el | compiled, no door — the contrast that prices `forEach` |
| `class.getter` | 98.4 | against **19.9** for a method |
| `new Error()` | 351 | still through the generic native construction door |
| `hidden` | 104 /call | per-object attribute map |
| `put` | 43 /call | |
| `control.arith` | 2.2 | the ruler |

### The two best next targets

**`class.getter` at 98.4 ns against a method's 19.9.** `cache.rs` says why in
its own words — *"Not cacheable — inherited, an accessor, a proxy above"* — so
**every** getter read takes the full resolve: walk the chain, probe the
accessors map, then call. The ~78 ns above a method call is that walk, paid on
every read. `object/mod.rs` already names the fix and calls it a machine change:
accessor-ness belongs in the SHAPE, not on the object. Nothing cheaper was found
that does not also need a machine change, because the site shape (`CachedGet`)
is a machine terminator.

**`hidden` at 104 ns a call.** Systemic rather than local: every native that
installs a non-enumerable property pays it, and `Error`'s constructor pays it
twice — once for `message`, once for `stack`. It is the per-object attribute map
`object/mod.rs` records as debt in its own header. This one is entirely inside
`rts-core` and needs no machine change, which makes it the cheaper of the two.

### Named and left open

- `arr.join` at 44 ns an element is the **result's** allocation. The
  small-integer text cache does not touch it, and the commit says why.
- `new Error()` at 351 and `new Date(0)` at 342 are still on the generic
  construction door. The `*_direct` shape that fixed `Map` and `Set` fits them,
  but `CoreEntry`'s ceiling test asks for the LIST-level argument before a
  thirteenth hand-written row, and that argument is: `#[rtse::class]` already
  knows the class, its arity and its constructor, which is everything those rows
  state by hand. **Derive the row; do not write a third one.**
- `class_support::prototype` is a linear scan with a string comparison, and it
  is now on the path of every collection construction.

---

## 3. Items 1, 2 and 4 of the earlier plan

**1 — the direct call.** `rts prove` reports `0 direct` calls in every program
tried: a JavaScript call is either substituted by `emit/inline.rs` (one
expression, no captures) or pays the full door. Measured on one program, same
computation either way: **1.17 ns inlined against 15.31 through the door.**

The precondition was built in full and **refuted**. `machine_trace.rs` prescribed
one machine capability — compiled code records its own frame pointer before
calling out — and it was implemented across all three layers and reverted on
four readings:

| start of the walk | frames | attributed |
|---|---:|---:|
| this frame, host as usual | 1 | 0 |
| the anchor, host as usual | 1 | 0 |
| the anchor, host `force-frame-pointers` | 2 | 0 |
| this frame, host `force-frame-pointers` | 1 | 0 |

Two corrections to that module came out of it and are now in its header: the
anchor **is** the better start, and `force-frame-pointers=yes` is **not** inert —
it had been tried when the walk could not take a first step. And the reason no
chain fix reaches it: **the return address stored in a frame names that frame's
CALLER**, so a walk from the innermost compiled frame yields an address inside
the runtime, which the code map has no range for. The innermost frame's own
program counter is in a register at the moment of the crossing and nowhere on
the stack. A working version records the call site as well as the frame, as V8's
entry frames do — a second capability, not a refinement of the first.

**2 — `Inst::ElementLoad` inline.** Declared, built, verified, lowered, **zero
producers** in `rts-codegen`; `Inst::IntArith` the same. `a[i]` crosses to the
runtime for a load the machine can already do inline — 12.67 ns against .NET's
≤0.71. **Do not emit it for ordinary arrays without precise roots**: an element
loaded inline is a reference the collector must find without being told, which
is `docs/engine/lost-roots.md`'s class and a use-after-free rather than a
temporary regression. **Typed arrays are unblocked today** — their elements are
bytes, never references — and that slice is the one to take first.

**4 — the differential test.** `RTS_MIR` appears in four places in the
repository and **none of them is a test**. Nothing runs one program down both
`emit/` and `lower/` and compares. `CLAUDE.md` RULE 0a makes this the
precondition for deleting `emit/` (37,698 lines against `lower/`'s 8,563 plus
`rts-mir`'s 3,801). Not started.

---

## 4. Method — what actually worked, and what did not

This is the part worth reading before touching a number.

**Four of my stated causes were wrong this session, and measurement killed each
one.** The pattern was identical every time: I tested a path that *resembled*
the one in question instead of the one in question.

| claimed | was |
|---|---|
| the `Map` table's five `Vec`s are the cost | `new Object()` has no table and costs the same — it is the construction door |
| `collections::fresh` has no class registration | it has one, whenever something read the global name first |
| `fresh` and the door produce different layouts | identical; a site warmed on one reads the other correctly |
| the small-integer text cache feeds four rows | it feeds one |

**Ask `rts prove` for the occurrence count before reading any result.** This
caught three empty fixtures in a row. Two of them were empty because *I* had
disabled the whole-program proof inside my own test — once with
`class Mine extends Map`, once with a read of `Map.prototype` — and both are
legitimate clauses of `primordial::only_a_base`. A fixture must prove the new
path was taken before it can prove the path is correct.

**A fixture at the top level, or in a one-expression function, may exercise
nothing.** The first correctness fixture for `new Map()` passed identically on
both binaries because every case sat in a position the op does not reach. Six
identical lines proved nothing, and I read "identical" as "correct".

**An index-is-position table takes an APPEND and never an INSERT.** I broke this
twice in one day: `Js::ENTRIES` in `rts-codegen` (the rule is written on `PRIMS`
three lines below it) and nearly in `CACHED_KEYS`. Inserting into `ENTRIES`
shifted `ObjectNew` and `OwnKeys` by two, and `new Map()` then produced an object
whose typed reads answered `undefined` while `m.set`/`m.get` still worked. Two
wrong theories were chased before the rule was read.

**Install the measurement where the need is, not where it is convenient.**
Putting a class installation inside `collections::fresh` made every program that
declares a name install the whole class, because the pickle's name registry is a
`Map` built through `fresh` — the shape tree grew by 19 layouts where it grows by
3. `pickle::names_tests::a_declaration_makes_no_layout_of_its_own` catches that
as a COUNT and not as a clock, which is why it is reliable on a busy machine.

**Remove your instruments before measuring the change.** A ladder and a counter
left in the construction path cost 15 ns and showed the optimisation as a
regression — 191 ns against the baseline's 176 — until they were deleted.
`closure_new`'s ladder is the precedent: keep the numbers in the doc comment,
delete the machinery.

**A `git checkout` of a file with two changes in it reverts both.** That muddled
a bisect into reading "rts-core is clean" as "this hunk is innocent". Reconstruct
the full set and remove exactly one thing at a time.

**In a CRLF repository, check the diff after any `sed -i` or `node -e` patch.**
Two files were rewritten whole — a 3,197-line diff for a 4-line change, and one
left with mixed `CRLF, CR` terminators. No test catches it; the diff does.

---

## 5. How to re-measure

```bash
# the kept baseline, before the first edit of a session
cargo build --release && cp target/release/rts.exe target/baseline.exe

# the sweep and the fixtures used above live in the session scratchpad, not in
# the repo; the shape of one is:
#   one row per process, a sink the loop cannot drop, a varying index so no
#   answer is loop-invariant, and no imports so node and bun run it unchanged.

# a cumulative ladder, when a cost needs attributing rather than confirming:
#   an env-gated early return per step, the last rung being the unmodified
#   body, so the parts have to add up to the real number. Delete it after.
```

Two rulers that are already calibrated and worth running first:

```bash
cd bench/isolated
cargo run --release --bin crossing_price     # the parts of one runtime crossing
cargo run --release --bin activation_stacks  # what the three per-call stacks cost
```

Fresh readings from them, 2026-10-03: a direct call and an indirect call cost
the same (1.136 / 1.135), the pending-throw check costs **0.008 ns** — the third
independent instrument to say so — and `with_current` costs **1.011**. The three
activation stacks are 5.87 ns of a 10.14 ns call shape.

**Do not re-propose these.** Each is refuted, two of them more than once: the
throw check, making the entry points direct, and fusing the `with_current`
borrows (written in full, measured, reverted — mixed signs, and the control
moved as much as the targets). Dropping the activation push on the light path
was ablated this session and is worth **1.5 ns of 17, and nothing at all on a
row that allocates**.
