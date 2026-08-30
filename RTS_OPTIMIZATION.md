# What is left to optimise

The work list from `docs/codegen/inlining-survey-2026-08-30.md` — a ten-angle
survey, 50 candidates, 37 refuted by three adversarial lenses — minus what has
since shipped, plus what shipping it uncovered.

Every number here is measured on `--release`, min of 9 or more in process, with
a control in the same run. Where a figure is an estimate it says so.

**The constants everything is priced against**, established in
`docs/codegen/entry-tax.md`:

| | ns |
|---|---:|
| a runtime crossing (compiled code → an `rts-core` entry point) | ~15.7 |
| the Rust body of a typical arithmetic operator | ~1.5 |
| an operation the machine emits as an INSTRUCTION | **0.00** over the floor |
| a cached own-property read | ~6 over the floor |
| a heap allocation | 40–90 |
| a substituted JS call | ~4–9 |
| a real JS call | ~25 |

So roughly ninety per cent of what an "operation" costs is the door, and the
entry-point membership rule is what decides who pays it: *an entry point exists
if and only if the operation touches the heap, the operating system, or global
mutable state. Pure computation is instructions.*

---

## Already shipped, for context

| | measured |
|---|---|
| a negated condition keeps its proof | `!x` 10.6 → **6.8**, `!= null` 7.3 → **3.8** |
| three `typeof` answers decided by the tag | 12.8 → **2.3** (−82%) |
| `switch` reaches the settlements | `typeof` switch 24.0 → **18.4**, `case null` 7.0 → **2.2** |
| a literal condition | `while (true)` 3.95 → **0.80** (−80%) |
| the three iteration desugarings | 21 crossings → 7, **clock says zero** |
| a void body and a written binding | 25.7 → **4.3** (−83%) |
| a nested function's own name | 330 → **300** (−9%) |

Plus two defects the survey found in code committed the same day: a nested guard
clause returning from the CALLER (silent, exit code 0), and a `try`/`catch`
admission whose mechanism was never in the tree while its commit reported 4%.

---

## 1. THE CLOSURE THAT IS ALLOCATED AND IMMEDIATELY DEAD — 21×

The largest single number left, and the hardest to do correctly.

```js
function usesNested(x) { const step = (y) => y + 1; return step(x); }   162.00 ns
const stepOut = (y) => y + 1;
function usesOuter(x)  { return stepOut(x); }                            7.67 ns
function straight(x)   { return x + 1; }                                 7.67 ns
```

`rts ir` says it in three lines: the call to `step` is **already substituted** —
the body `y + 1` is emitted inline — and `__rts_closure_new` still runs on every
call of `usesNested`, allocating a cell and a `prototype`, registering two GC
roots, and handing back a value nothing reads.

Per call, `closure_new` performs: two allocations, two `external::hold`
registrations, one `callables` write, three property writes, two table lookups.
An arrow costs ~150 ns; a declaration ~300, because it carries `prototype`.

**The design that is wrong here, and why.** The obvious answer is to build the
closure lazily at the first call site that refuses to substitute. It is a JIT's
answer and it does not transfer: RTS is deterministic, and deferring moves WHEN
an allocation happens. What the conservative collector sees depends on
allocation order — `docs/engine/lost-roots.md` records a case where one
`eprintln!` inside `collect` changed a program's ANSWER — so a design resting on
"nobody can observe when the object was made" imports a freedom this compiler
does not have.

**The design that fits.** OMIT the closure entirely, decided at compile time,
same program every run. That is dead-code elimination and it needs a complete
local proof: every call to the name is CERTAIN to be substituted. Not most — the
binding would be `undefined` and a fallback call would fail.

Each clause of that proof closes one refusal:

| clause | the refusal it closes |
|---|---|
| never read as a value (`g(f)`, `f.name`, `const h = f`, `[f]`, `typeof f`, `f?.()`) | the value would not exist |
| not in `capture::captured` | a call from a nested function substitutes into THAT function |
| `ctx.inlinable(name).is_some()` | nothing to substitute with |
| no parameter or free name in `ctx.flattened` | `emit_substituted` asks exactly this |
| no `with` anywhere in the body | the gate OUTSIDE `emit_substituted` is `ctx.with_objects.is_empty()` |
| no spread at any call site | `emit_substituted` refuses it |
| the helper's own body contains no call | closes cycles without a call graph |
| no default and no rest parameter | both are refusals a call site can make |

All of it is answerable in `function::emit_body`, after `capture::captured` and
`escape::analyse` and before any statement is emitted. **The `with` clause is
the one an implementation written against `inline.rs` structurally cannot see** —
that gate lives in `call.rs` — and it is why the survey refuted the naive
version 3/3.

Ceiling: 162 → ~8. A refused omission costs one closure; a wrong one costs a
program.

---

## 2. THE ANNOTATION THAT IS MISSING — the machine-language answer

Not a candidate. The completeness critic's finding, and the reason every other
item on this list is one special case at a time.

**The C and C++ answer for a call you cannot inline is not to inline it. It is
to DESCRIBE it** — `__attribute__((const))`, `((pure))`, LLVM's `readnone` /
`readonly` / `speculatable` / `willreturn`. You do not inline `strlen` at `-O2`
in a non-LTO build; you annotate it, and the caller's mid-end then CSEs it,
hoists it out of loops, sinks it past branches, and deletes it when the result
is unused.

That is exactly this engine's situation: a fixed `extern "C"`-shaped door and a
body the compiler will never see.

**The repository diagnosed the disease and filed the wrong prescription.**
`crates/rts-cranelift/src/target/mod.rs`:

> Cranelift's default is `none`, which gates out the WHOLE egraph mid-end: no
> GVN, no LICM, no redundant-load elimination… The mid-end cannot optimize
> across an opaque call, and this engine's IR is mostly opaque calls, so there
> is little for it to see.

It concluded *the knob is worthless* instead of *the calls are undescribed*.

Evidence it is absent rather than unmentioned:

- `runtime/raising.rs` is the ONLY per-operation property list in the engine,
  and it describes a CONTROL effect. There is no memory-effect list — no
  `READS_NOTHING`, no `WRITES_NOTHING`, no idempotence.
- `emit_guarded` says it outright: *"Nothing removes it for us… A value computed
  on the fast path and used only on the slow one stays exactly where it was
  put."* The emitter is hand-sinking a widen because no sinking pass exists.
- `grep` for LICM, CSE, GVN or invariance across both engine crates returns
  nothing.

It also explains a refutation the survey recorded without diagnosing: a general
DCE pass was refuted 3/3, and correctly — 11 908 calls have unused results and
5 253 of them are `__rts_call_counted`, which is every `foo();` statement.
Liveness was never the missing ingredient. **DCE cannot delete a call unless
something asserts the call has no effect.**

Where it lands without breaking the architecture: the classification is the
language's (`RuntimeOp`, beside `raising.rs`); the flag rides on `Inst::Call`;
the pass lives beside `ir/fold.rs`, because an effect is a machine question.
`ToBoolean`, `TypeOfIs`, `StringConst` and `IsSingleton` are already in the
class today.

**The falsifier to write first**, before any of it:

```js
for (let i = 0; i < N; i++) if (typeof x === "string") s++;   // x invariant
```

`rts ir` must show `Call TypeOfIs` **once**, not N times. Loop-invariance is not
a property of a syntax node, so the emitter has no vantage point from which to
see this — which is the whole argument for the annotation over one more special
case.

**Caution, and it is the same one as item 1**: enabling Cranelift's egraph
mid-end was refuted 3/3 on correctness, because it changes which SSA values are
live at which point and this engine finds compiled-frame GC roots BY BIT PATTERN
from a conservative stack scan. An annotation-driven pass of this engine's own
is a different thing from turning that knob, and the difference has to stay
deliberate.

---

## 3. A GENERATOR UNDER `for`-`of` ENDS EARLY — open defect, not an optimisation

Recorded in `docs/engine/lost-roots.md` as its fourth instance.

| | answer |
|---|---|
| release, `RTS_GC_DEBUG=1` | **ok 60683** — the loop ended early |
| release, plain | ok 120000 |
| debug, `RTS_GC_DEBUG=1` | ok 120000 |
| debug, plain | ok 120000 |

Deterministic in all four cells, one wrong. `RTS_GC_DEBUG` changes no logic — it
is two `if gc_debug()` blocks around `eprintln!` inside `collect`. A flag that
only PRINTS changing the ANSWER means the value is found by the conservative
scan or not at all.

**What it is not**, so nobody repeats the day: naming the generator in a local
answers 60683 too, so the program does hold it; and `Context::resuming` — the
one field that names a running generator and explicitly is not a root — was
added to `context_roots` and changed NOTHING in release.

That attempt is its own lesson: it appeared to work because it was verified on a
DEBUG build, which answers 120000 with and without it. **A fix verified in a
cell that never had the bug is not verified.**

A second symptom in the same shape, also open: 300 000 rounds exhaust the heap
outright.

`tests/generator_for_of_root.test.ts` holds five cases. They PASS on the build
that has the defect; the header says so.

---

## 4. HOT/COLD CODE LAYOUT — the dual of inlining, never attempted

`__builtin_expect`, `.text.unlikely`, `-freorder-blocks-and-partition` — applied
to GENERATED code rather than to Rust.

`cranelift_frontend::FunctionBuilder::set_cold_block` exists in the pinned 0.131
and this repository calls it **zero times**. Meanwhile `raising.rs` measures the
throw check alone at **1 423 of 6 164 blocks in `analytic.ts`, about 46%
counting continuations** — and every raise block, every guard slow path and
every cache-miss block is laid out inline in the hot path.

C++ inlines aggressively precisely because it can then sink the cold half out of
the instruction cache. Every item on this list makes it matter more.

Unmeasured. The first thing to do is measure, not build.

---

## 5. A PROPERTY MORE THAN TWO LINKS UP — ~5–8 ns of pure waste

`cache.rs` walks two prototype links and then refuses the chain cache
PERMANENTLY, and the refusal marker is read AFTER the own-property attempt — so
a three-deep chain pays the wasted own lookup plus a full `get_property`
crossing on every access.

Estimated 5–8 ns per access on top of a ~14 ns `get_property`. Frequency:
occasional, and the survey's own note is that the census that produced it is a
hand-written corpus rather than real class hierarchies.

**Measure before building.** A census is not a clock, and this file records one
ranked item that died exactly that way.

---

## 6. `type_of_is` RE-DERIVES A CONSTANT STRING COMPARISON — probably wrong

Kept on the list with its own warning attached, which is why it is not higher.

The claim is 5.6–7.8 ns from `11.52 − 3.68`. The completeness critic's objection
stands: the two sides differ by more than the string comparison — `type_of_is`
also does three side-table lookups — and `settled.rs`'s own 2026-08-29
measurement puts bare `typeof` at **8.3 ns, not 3.68**. Nobody reconciled the
two.

Item 7 below partly subsumes it: with `number`/`boolean`/`undefined` now decided
by the tag, what remains are `string`, `object` and `function` — the three that
genuinely need the cell header.

**Re-measure before believing anything here.**

---

## 7. `symbol` AND `bigint` AS TAG TESTS — deliberately deferred

They ARE tag-decidable, and were left out of the shipped `typeof` work for a
stated reason: their tag numbers are RUNTIME values (`context.kinds.symbol`), so
the emitter would need a compile-time agreement asserted in `rts-host` — the
shape of work the singleton numbering is.

Corpus census: `symbol` 6 occurrences, `bigint` 8, against 45 for `number`.

~11 ns each, for 14 occurrences. The work is real and the payoff is small; it is
here so the decision is not silently re-taken.

---

## 8. THE `switch` OMISSION IS A CLASS, AND THE CENSUS MAY BE STALE

Shipped: `switch` labels now reach `settled`, and the three iteration
desugarings ask "is it callable" once. The fact was written down once:

> **a comparison not built by `emit_binary_inner` is unsettled**

What has NOT been done is re-running the census after those two changes. Nothing
prevents the next desugaring from doing it again, and the survey found the first
four by grep rather than by construction.

Mechanical, cheap, and the kind of thing that rots.

---

## 9. A LOOP IN AN INLINABLE BODY — refused 3/3, and correctly

`straight_line` refuses every loop because "each can leave the body somewhere
other than the end". A loop with no `break`/`continue`/`return` escaping it is
self-contained, and 77% of loop-bearing helper bodies qualify syntactically.

It was refuted, and the refutation is why it stays on the list rather than
leaving it: it would have WIDENED the nested-guard miscompile that was live at
the time. That defect is now fixed, so the refutation's ground is gone — but the
same shape is what made it dangerous, and admitting a loop means
`emit_substituted` walking the body the way `straight_line` does and merging
from any depth.

24.50 ns per call. Prerequisite: the written-binding gate, which shipped.

---

## 10. A METHOD CALLEE BEHIND AN IDENTITY GUARD — 42%, priced by hand

The pass fires on a bare `Ident` callee only, so `o.m(x)` is always a real call
at 25.75 ns.

The published design is speculate/guard/deoptimise. RTS is AOT and has nothing
to bail out to. The compatible variant — a guard whose miss is an ordinary call
— was written out by hand in JavaScript and measured:

| | ns |
|---|---:|
| `holder.m(a)` as it is today | 20.00 |
| **a guard plus the inlined body, real call on a miss** | **11.50** |
| the guard alone — a property read and an identity compare | 11.50 |

42%, with the inlined body adding nothing. **This corrects an earlier claim in
this repository** that the method case needs a whole-program proof that the
property is never written: that is true only of UNGUARDED inlining.

The real blocker is narrower. To compare identities you need something to
compare AGAINST, and in the hand-written probe that is a captured binding. The
compiler would have to materialise the known callee or compare the closure's
code address — a machine question, and one that must not become a JIT's inline
cache by another name.

---

## 11. THE STATEMENT BUDGET BOUNDS THE WRONG QUANTITY

`STATEMENT_BUDGET = 8`. The corpus says it is inert: of 1710 named functions, 9
(0.5%) have more than 8 statements before the return, and 99.4% have 7 or fewer.

So raising or lowering it changes almost nothing, and the real finding filed
under this heading is a COMPILE-TIME one — 59.5 s at unbounded substitution
depth. There is a cycle check (`ctx.substituting`) and no depth or growth bound
at all.

Not a run-time number. Belongs to `perf-claim`, and the honest form is a bound
on emitted size rather than on source statements — which is what a C++ compiler
uses.

---

## 12. AN INLINED BODY HAS NO FRAME, AND `.stack` SEES IT

Not a performance item; the standing cost of everything above.

A substituted body has no frame for `functions::invoke` to push, so it is absent
from `.stack`. `running.rs::an_error_says_where_it_came_from` now asserts this
in two directions: the frame that throws is named, and the two that were inlined
away are asserted ABSENT.

The fix is inlining metadata — a record of which bodies were spliced where, so
`throw::stack_text` can name them. It does not exist. Until it does, the trade
is stated rather than hidden: a call that costs 25.7 ns costs 4.3, and the frame
is gone.

---

## 13. TWO STALE COMMENTS, AND THE CLASS THEY BELONG TO

`emit/inline.rs`, above `straight_line`:

> `Declare` is absent, and it was there for one build… a body with no
> declarations is the whole of what can be spliced

The code four lines below admits `StmtKind::Declare`. That is the contradiction
CLAUDE.md says never to leave standing.

**Three of these were found in one survey**, and each had cost something: the
`from_bool` "no unary path yet" (fixed, and worth 3.5–4.2 ns on every negated
condition once believed), the `is_singleton` comment claiming defaulted
parameters had been moved over when they had not, and this one.

The class is *a comment that describes an intent is read as a description of the
state*. It is the same failure that produced the `try`/`catch` commit whose
mechanism was never in the tree.

Cheap to fix and it rots continuously. Worth a mechanical pass.

---

## 14. THE MEASUREMENT DISCIPLINE THIS LIST ASSUMES

Not an optimisation. The rules that made the list trustworthy, so the next
session does not re-learn them.

**Check the mechanism before believing the number.** `rts ir` answers in
seconds; a release build costs two minutes and a corpus run twenty. Two commits
in this campaign reported a win with no mechanism in the tree — one because a
patch script died on its second hunk and the shell correctly refused the write,
one because a `match` arm was in the wrong position. **A pass that silently
refuses gives the same answers, only slower, so no test can see it.**

**Verify the instrument before the result.** A regex resolving `__rts_type_of`
also matched `__rts_type_of_is` and reported crossings going UP when they went
down. A file-set comparison counted one file as both LOST and GAINED because the
extractor appended `[EXC]` to the name.

**Check that the binary is the one you built.** `cargo build --release` died
with `Acesso negado` — the linker could not write `rts.exe` because a
measurement was running it — and the composite command exited zero. What caught
it was the binary's timestamp against the time of the edit.

**Run the control in the cell that has the bug.** A GC root fix was verified on a
debug build that answers correctly with and without it, and was one command from
being committed as a fix.

**A count is not a clock.** Every census here — 1 079 `to_boolean` sites, 11 908
dead-result calls, 230 `string_const` sites — is a reason to measure, never a
result. One ranked item died exactly that way at 0.33 ns.

**Prefer removing work to moving it.** Six rearrangements have been refuted in
this campaign against zero that shipped.

---

## What was refuted, so it is not re-proposed

Thirty-seven candidates died to three lenses. The ones most likely to be
suggested again:

- **A direct call instead of an indirect one** — already measured at exactly
  zero: 1.154 both.
- **`string_const` as a `WordLoad`** — struck at 0.33 ns; the 230 sites are
  compile-time.
- **Turning on Cranelift's optimiser for AOT** — changes which SSA values are
  live, and GC roots are found by bit pattern.
- **A direct `Inst::Call` between two emitted JS functions**, **`CallIndirect`
  with a monomorphic cache**, **C++-style function versioning** — all three fail
  the same way: `entry::called`/`invoke` maintains three per-activation stacks
  JavaScript observes. `context.callees` IS the call stack `.stack` prints,
  `new.target` is decided by `depth + 1 == callees.len()`, `RunningFunction`
  answers `callees.last()`. A call that skips them is a different language.
- **Emitting a body twice** — `Ctx::template` deliberately mints a fresh
  identity per site, because each tagged template gets its own strings object.
- **A separate tag for strings** — the larger half was already collected by
  `Context::is_text_at`.
- **`#[inline]` sweeps** — release is `lto = "thin"`, `codegen-units = 1`; LLVM
  already has what the attribute would give it.
- **A general DCE pass** — see item 2: the missing precondition is an
  annotation, not liveness.
