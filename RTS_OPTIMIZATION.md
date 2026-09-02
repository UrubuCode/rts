# What is left to optimise

Three files, and this is the one that RANKS. `RTS_ENGINE_STUDY.md` holds the raw
readings under each item — what a file does today, read by someone who looked.
`RTS_REFUTATIONS.md` holds what has been refused and why. Read the study before
proposing, and the graveyard before re-proposing.

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

## 5. A PROPERTY PAST THE CACHE'S REACH — 46 ns, not the 5–8 estimated

**MEASURED 2026-08-30, and the estimate this replaces was low by six to nine
times.** Release, min of 9 over 3 M iterations, two shapes because they do not
behave alike.

A four-deep class hierarchy, calling a method at each depth:

| links | ns |
|---|---:|
| own property | 8.33 |
| 1 | 21.33 |
| 2 | 21.33 |
| **3** | **67.00** |
| 4 | 75.33 |

The step from two links to three is **3.1×**, and it is a cliff rather than a
slope — which is what a cache that stops trying looks like from the outside.

The same depth built by `Object.create` has no cliff and no cache at all:

| links | ns |
|---|---:|
| own property | 8.67 |
| 1 | 44.67 |
| 2 | 51.33 |
| 3 | 57.00 |
| 4 | 63.33 |

Linear, about 6.3 ns per extra link, and **36 of them arrive at the FIRST link** —
where the correctness argument below does hold. `RTS_CHAIN_DEBUG=1` prints
nothing for that site, and the resolver reports each of its ten refusals under
exactly that flag, so it is not being refused: **it is not being reached**. A
second question with a second answer, and the cheaper one to chase.

**The blocker for the depth itself is written down, and it is a GC argument.**
`entry/cache.rs`, above `cache_resolve_indirect`:

> At one step the argument holds: the receiver's type is discriminated by its
> link (`Context::typed_as`), so recognising the type proves the link is the cell
> whose address was remembered, and a live receiver keeps its link alive through
> `trace`. At two steps the middle cell's own link can be reassigned, and nothing
> the site compares would notice.

So the depth is one by construction rather than by oversight, and the comment
says closing it is "a separate change with a separate argument to make". That
argument is about what keeps a remembered address alive, which puts it in the
class `docs/engine/lost-roots.md` describes — the one where the failure is
silent and the answer, not the process, is what goes wrong.

**Do the `Object.create` half first.** It is 36 ns at depth one, it needs no new
claim about liveness, and why the resolver never runs there is a question
`RTS_CHAIN_DEBUG` and `rts ir` answer without a release build.

## 5b. A COMPUTED KEY IS BROKEN BY THE **SECOND** KEY, NOT THE FOURTH

Measured 2026-08-30, release, min of 9 over 3 M iterations, a site reading a
four-field object:

| what the site sees | ns |
|---|---:|
| one key | 17.00 |
| **two keys** | **38.00** |
| four keys (`bench/analytic.ts`'s own row) | 38.00 |
| the `keys[q & 3]` array read ALONE | 15.67 |

Two readings, and the second is the one to act on.

**The access itself is ~1.3 ns when monomorphic.** Almost all of the one-key row
is the array read that produces the key, which prices `array index read` at
about the same 13–15 ns the benchmark reports for it directly.

**A SECOND key costs twenty-two**, and twenty-two is a runtime crossing: the
site recognises nothing and asks `CacheResolveKeyed` on every access. The plain
cached read has had two entries since a site reached by two layouts measured a
0% hit rate; a keyed site still has one.

### The obvious fix does not fit, and this is where it dies

A second entry needs four words: a layout, an offset, a base, and a key. Word
seven is free — the cold image writes it as padding — and words three, four and
five *look* free for a keyed site, because they carry the INDIRECT form's
meaning and a keyed site never takes that path.

**They are not free.** `cache::remember` shifts entry zero into words three,
four and five whenever it is called with `duplex` — and `cache_resolve`, which
`cache_resolve_keyed` delegates to, passes `duplex = true`. So a keyed site
already has a two-entry LAYOUT cache living exactly there, and the only thing it
lacks is a second KEY to pair with it.

Which does not help the case that matters. `remember` shifts **only when the
layout differs**, and the case measured above is one object with several keys —
same layout, so no shift, so entry one never receives the second key's answer.
Making it shift on a key difference means `cache_resolve_keyed` doing its own
demotion around a call that may also demote, in a cell two resolvers write with
different readings of the same words.

And it lands on the GC. `cache_keyed.rs` counts a root per remembered key cell,
and says why in the failure it fixed: a site that remembers a fresh cell per
pass exhausted the heap — `roots 63355 live 65396 freed 5` — over seven
characters that never changed. Its argument is *"a site remembers exactly ONE
key, so what has to stay alive is one cell per SITE"*. Two entries make that
sentence false, and the class of failure it belongs to is
`docs/engine/lost-roots.md`'s: silent, and wrong in the ANSWER.

### What would actually move the analytic row

Nothing above would. That row cycles FOUR keys, and four entries is refused for
a stated reason: a read entry is three words, the cell is eight, sixty-four
bytes is one cache line, and four entries need a second line **on every access
including the monomorphic ones, which are the overwhelming majority**.

So the row is a genuinely polymorphic site paying the entry tax, and the way to
reduce it is not to cross — which is item 2, not a bigger cache.

The other half of that row is cheaper and is not about caches at all: **15.67 ns
of the 38 is `keys[q & 3]`**, an ordinary array element read. That is the same
number `array index read` reports, it is paid by far more code than computed
keys are, and nothing here has looked at it.

## 5c. A METHOD CALL IS NOT A METHOD PROBLEM — it is one crossing

Measured 2026-08-30, release, min of 9 over 3 M iterations:

| | ns |
|---|---:|
| method call `callee.m(a)` | 19.00 |
| **the same function through a plain binding, `held(a)`** | **18.00** |
| a method call with FOUR arguments | 20.67 |
| the property read alone | 6.00 |
| a native, `Math.abs(a)` | 32.67 |

**Being a method costs about one nanosecond.** A real call through an ordinary
variable — one the substitution pass refuses because the binding is reassigned —
costs the same eighteen. Three extra arguments cost 1.7.

So the row is not the receiver, and it is not the property read: the read is
cached and the two do not add up (6 + 18 is not 19). It is **one runtime
crossing**, which this file prices at 15.7 ns, plus about two of bookkeeping.

That bookkeeping is `entry::called`, and it is three thread-local borrows and
four stack operations per call:

```text
with_current #1   is_class_constructor, push pending_arguments, push pending_counts
invoke        →   with_current #2   resolve the callee, push `callees`
with_current #3   pop pending_arguments, pop pending_counts
```

Every one of those maintains state JavaScript observes — `.stack`, `new.target`,
`arguments` — which is why the survey refuted a direct call between two emitted
functions three votes to nil. **A call that skips them is a different language.**

### Which is why substitution is worth what it is

A substituted call costs 1–3 ns because it removes the crossing outright. The
method form is refused for one reason and it is not semantic: `emit_substituted`
fires on a bare `Ident` callee only, so `o.m(x)` never reaches it.

### And where a guard would have to get its identity — this corrects item 10

Item 10 says the blocker is "you need something to compare AGAINST" and that the
compiler could compare the closure's code address. **It cannot, cheaply.**
`Context::mark_callable` puts the code pointer in `context.callables`, a SIDE
TABLE keyed by cell — not in the cell — so reading it is a crossing, which is
the whole cost the guard exists to avoid.

The cheapest identity a compiled site can compare is therefore the closure CELL
itself, and a cell does not exist at compile time. So the shape is forced:

- the compiler picks ONE body statically — for `const o = { m(x) { … } }` it can
  see which — and emits it inline behind a guard;
- a cache word remembers the cell that a resolver confirmed carries that body;
- the fast path compares the read callee against that word; a miss is an
  ordinary call.

That is not a JIT's inline cache by another name, and the difference is worth
stating because the survey refused things for looking like one: the BODY is
chosen at compile time and never changes, there is no deoptimisation and nothing
to bail out to, and the emitted program is identical on every compile. The cache
holds only "is this the cell we already checked", exactly as `CachedGet` holds
"is this the layout we already checked".

It needs a new terminator, a resolver, and the receiver-static analysis. It is
the largest item left that has a measured number on it: 42%, hand-priced in
JavaScript.
## 5d. `flow throw+catch` IS `new Error()`, AND IT IS NOT THE STACK

Measured 2026-09-02, release, min of 9 over 100 K iterations. The row is 1029 ns and
every one of them is the constructor:

| | ns |
|---|---:|
| `try { … } catch { … }` with no throw | 0 |
| **`throw "s"` and catch it** | **0** |
| `new Plain()` — an empty class | 60 |
| `new WithField()` — one field | 60 |
| **`new Error()`** — no message | **700** |
| `new Error("x")` | 980 |
| `new TypeError("x")` | 1060 |
| `throw new Error("x")` and catch it | 1020 |

**Throwing and catching cost nothing.** A string thrown and caught measures the same as
an empty loop. So the row is misnamed: nothing in unwinding, in the `finally` machinery
or in the catch binding is worth looking at.

**And it is NOT `.stack`, which is the finding that kills a proposed design.** The study
entry above says `.stack` is rendered and interned on every construction whether or not
anything reads it, and prescribes making it lazy. It is rendered — the code is there — but
ABLATING IT ENTIRELY CHANGES NOTHING: with the `format!`, the `stack_text` walk, the
`Str::from_str` and the `put` all removed, `new Error()` measured 690–700 against 700 and
`new Error("x")` 1000–1010 against 1000–1010. Two alternations.

Construction also does not scale with call depth — 800 ns at depth one and 790 at depth
nine — so the frame walk is not it either.

**Where it actually is.** An `Error` costs 640 ns more than a plain class instance. With
no message, `written` does a `with_current`, a `receiver` that is one `as_slot()` when
`this` is already a cell, and the stack that ablates flat. **So the cost is before
`written` is entered: the native-constructor path for a native class.** That is consistent
with the other native measurement on this list — `Math.abs(x)` at 32 ns against 19 for a
JS method call — and it is where the next probe should go.

**What this retires:** the study's "Three avoidable costs inside the Error constructor,
all in `written()`" is aimed at the wrong function, and any lazy-`.stack` design is
aimed at a cost that is not there.

**One thing worth keeping from the detour.** `Object.getOwnPropertyDescriptor(new Error(),
"stack")` answers `configurable,enumerable,get,set` in node and
`configurable,enumerable,value,writable` here. A real divergence, found while pricing a
design that turned out to be worthless — and it is a CORRECTNESS item, not a speed one.

### CORRECTION — the ablation above was measured on a stale binary

The section says `.stack` ablates flat and that the cost is elsewhere. **That is wrong, and
the way it went wrong is worth more than the claim.**

The ablation removed the `stack_text` call, which left that function with no callers.
This crate denies dead code, so `cargo build --release` **failed** — and the command was

```bash
cargo build --release 2>&1 | tail -1 && cp target/release/rts.exe target/nostack.exe
```

A pipeline's exit status is the LAST command's, so `tail` answered zero, `&&` fired, and
`cp` copied the binary from the previous build. Every number attributed to the ablation
was the unablated engine measured against itself, which is why it looked flat.

Re-run with the build actually succeeding — `stack_text` still called, its result handed
to `black_box`, so only the interning and the write are removed:

| | ns |
|---|---:|
| return immediately after `receiver` | 100 |
| the stack RENDERED, not interned or written | 420 |
| the whole constructor | 790 |

So `.stack` is **the entire 690**: about 320 to render — two `format!` and the
`stack_text` walk — and about 370 to intern the result and write the property.

**The lazy design is back on, and it is the largest single item on this list.** Deferring
it takes `new Error()` from 790 to about 100 and `flow throw+catch` from 1029 to about
250. Node makes `.stack` an accessor (`get,set` in the descriptor) where this engine makes
it a data property, so laziness is not only legal — it closes a divergence at the same
time.

**And the rule this cost:** never `cmd | filter && cp`. Check the build's own status, or
copy only after a command whose exit code is the build's.

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
