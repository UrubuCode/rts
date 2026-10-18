# How fast the new engine is, measured against the old one

> **Superseded in part, the same day.** The 130× below was measured before the
> type pass existed, and it is what motivated writing one. After it: **0.91 ns
> per pass against the old engine's 0.70** — 1.3× rather than 130×. The
> measurement that follows is kept rather than rewritten, because what it found
> is what the pass was aimed at, and a number that vanishes when it is acted on
> leaves nothing to check the action against. The new figures are at the end.


**2026-08-04.** Both release builds. The old engine's binary was rebuilt first —
the one on disk was two days old and `git log` showed its sources had changed
since, which would have made the comparison a measurement of the wrong program.

## The number

One kernel, in the only subset both engines can run:

```js
let total = 0;
for (let i = 0; i < N; i = i + 1) { total = total + i; }
return total;
```

| | per pass | for 20 M passes |
|---|---:|---:|
| old engine | **0.73 ns** | ~15 ms |
| new engine | **94 ns** | ~1 880 ms |

**The new engine is about 130× slower on this kernel.** Compilation is the other
direction and is not close: 0.16 ms against a process that spends 63 ms before
it runs anything — though those measure different things, and the section on
what this does not measure says why.

## How each number was obtained

The old engine runs as a process, so its cost includes startup. An empty
program takes 63 ms, and the kernel's own cost is the difference — which is only
meaningful if it scales, so that was checked rather than assumed:

| passes | wall | minus startup |
|---:|---:|---:|
| 20 M | 78.8 ms | 14.7 ms |
| 40 M | 90.5 ms | 26.5 ms |
| 80 M | 122.7 ms | 58.6 ms |

Doubling the passes doubles the work, so the loop is real and not folded away.
The new engine was checked the same way: 1 M passes take 96.7 ms and 20 M take
1 880 ms, which is linear to within 3 %.

Both harnesses report **best and median**, not mean. The best run is the one
least interfered with, and the median says whether that best was typical.

## Is the old engine cheating? Two ways it could be, both ruled out

`rayon` **is** in the old engine's dependency tree, and its sources use it in
several places. That makes "the loop is parallelised" a real hypothesis rather
than a rhetorical one, and 0.73 ns per pass — about two cycles — is fast enough
to deserve the suspicion.

Ruled out by comparing processor time against wall clock. A loop spread over
this machine's 16 cores would burn roughly sixteen times more CPU than clock:

| | wall | cpu | cpu/wall | threads |
|---|---:|---:|---:|---:|
| empty program | 77.6 ms | 46.9 ms | 0.60 | 4 |
| kernel, 1.5 G passes | 1 122 ms | 1 297 ms | **1.15** | 20 |

**1.15, not 16.** The loop runs on one thread. The twenty threads exist because
a thread pool is created; the loop does not go through it, and the 0.15 above
unity is the rest of the runtime idling alongside.

The other way it could have cheated is a closed form — recognising that the sum
has a formula and computing it in constant time. The scaling table above rules
that out, because the work doubles when the passes double.

Worth being precise about which check covers which: **linear scaling rules out
the closed form and says nothing about parallelism**, since a parallel loop
scales linearly too, just with a smaller constant. That is why this needed its
own measurement rather than an inference from the one already taken.

The 1.5 G run also re-measures the headline by a different route: 1 122 ms less
64 ms of startup is 0.71 ns per pass, against 0.73 ns from the 80 M run. Two
independent measurements, same answer.

## Where the 130× is, measured rather than reasoned

The explanation on offer was that every operator is a call into the runtime,
because no type pass exists and nothing has been proved about any operand. That
is a claim, so it was falsified against the alternative — that the cost is per
pass rather than per operator:

| operators in the loop | ns per pass |
|---:|---:|
| 3 | 94.2 |
| 4 | 119.3 |
| 5 | 143.1 |

**+24.5 ns per operator, linear.** The passes are identical and only the
operator count changes, so the cost is the calls. Dividing the first row by the
four calls a pass actually makes — `<`, `+`, `+`, and the `ToBoolean` the
condition needs — gives 23.6 ns each, which agrees.

So the gap is not code quality and not the code generator. It is that `i + 1` is
a function call that crosses an ABI boundary and borrows a thread-local context,
where the old engine emits an add.

## What this does not measure, and must not be read as

- **Anything but arithmetic over locals.** The new engine has no functions, no
  objects, no strings and no property access. The kernel was chosen as the
  intersection of the two engines, and says nothing about a program that uses
  what only one of them has.
- **A finished engine against a finished engine.** The old one has had years of
  work; the new one reached its first executable program yesterday. The 130× is
  what the design costs *before* the work that makes those calls unnecessary —
  a floor, not a verdict.
- **Compilation, comparably.** 0.16 ms is a function call in this process;
  63 ms is a process starting, reading a file, and building a prelude. They are
  not the same activity and the ratio between them means nothing.

## What it is evidence for

That the next phase is the type pass, and that its target is now specific rather
than aspirational: **every operator whose operands can be proved numeric stops
being a call.** The measurement says what each one costs, so it also says what
proving one is worth — about 24 ns per site per pass.

It is also evidence that the machine is not the problem. The old engine reaches
0.73 ns per pass through the same code generator.

## Reproducing it

```bash
cargo build --release -p rts-host --example kernel --example perop
target/release/examples/kernel.exe 20000000 5
target/release/examples/perop.exe 2000000
```

For the old engine, a `.ts` file holding the kernel wrapped in a function and
called, run with `target/release/rts.exe run <file>`, timed against an empty
file for startup.

---

# After the type pass

Same kernel, same machine, same session, both re-measured rather than compared
against a remembered figure.

| | per pass | 20 M passes |
|---|---:|---:|
| old engine | 0.70 ns | 13.9 ms |
| new engine, before | 94 ns | 1 880 ms |
| **new engine, after** | **0.91 ns** | **18.2 ms** |

**103× faster than it was, and 1.3× slower than the old engine.**

The per-operator measurement moved the same way, and it is the one that says
why:

| operators in the loop | before | after |
|---:|---:|---:|
| 3 | 94.2 ns | 0.9 ns |
| 4 | 119.3 ns | 1.3 ns |
| 5 | 143.1 ns | 2.0 ns |

**+24.5 ns per operator became +0.4 ns.** The call is gone; what remains is the
instruction. That is the whole of the change: nothing about the code generator,
the calling convention or the runtime was touched, and the same runtime entry
points are still there for the operands nobody could prove anything about.

## What is still 1.3×, and what is not known about it

Not investigated, and stated as unknown rather than explained. Candidates, in no
order: the old engine may keep its counter in an integer where this keeps a
double; the loop may differ by an instruction or two; the measurement's own
floor at 14 ms of work is close enough to startup noise that the gap is a few
milliseconds of a number the difference method already spends.

What is measured is that the gap is now small enough that finding out requires
looking at the emitted code, which is a different activity from this one.

## What the pass does not do

- **Anything the analysis cannot prove.** A parameter, a call's result, a
  property, a captured local: nothing here has evidence about any of them, and a
  wrong answer would be `arith` on a string rather than slow code.
- **`%`.** The code generator's numeric instructions are add, subtract, multiply
  and divide; remainder on doubles stays a runtime call.
- **Truthiness of a proven number.** `if (x)` still calls `ToBoolean` when `x`
  is a number, because a number is truthy when it is neither zero nor NaN and
  that is two comparisons and a conjunction rather than one instruction. It
  costs nothing in the common case: `while (i < n)` produces a proven boolean
  already, and a condition that is already a boolean is used directly.

---

# Property access, measured before the fast path is written

E4 made property access correct and left it slow on purpose. Whether to write
the inline cache is a question with a number, and the type pass is the precedent
for asking it first: that one was worth 24.5 ns per operator, measured before it
existed, and the measurement is what made its target specific rather than
aspirational.

Same loop, same passes, only the source of the addend changing:

| | ns per pass | over a proven local |
|---|---:|---:|
| local, proven | 0.8 | — |
| one property read | 94.8 | **+94.0** |
| two property reads | 190.2 | +189.4 |
| one property write | 73.9 | +73.1 |

Linear in the number of accesses, so the cost is per access.

## A defect found by measuring, before any conclusion was drawn from the number

The first measurement said 132.8 ns for a read against 73.9 for a write, and a
read doing nearly twice the work of a write is not a design cost — it is a
mistake. It was: the prototype walk collected the chain into a `Vec`, so **every
property read allocated**.

Nothing required it. The heap and the shape tree are two fields of one context,
and borrowing them apart was always available; the `Vec` was a way around a
problem that did not exist. Removing it took the read from 132.8 ns to 94.8.

Worth stating as a rule rather than a fix: **a measurement's first job is to
find the thing that should not be there.** Had the number been taken as the cost
of the design, the inline cache would have been built on top of an allocation
per read and would have hidden it.

## What the remaining 94.8 ns is, decomposed against a number already taken

A bare runtime call cost **24 ns**, measured when `+` was one — before the type
pass, in the per-operator table above. So:

| | ns |
|---|---:|
| the call itself | ~24 |
| the rest: key lookup, heap access, two hashed layout lookups | ~71 |

The two lookups are `ShapeTree::index_of` — memoised, so it is two hash lookups
rather than a walk, but two hash lookups nonetheless.

**An inline cache removes both halves, not one.** `guard_type` tests the shape a
site last saw and `field_load` reads at a constant offset, so a site that keeps
seeing the same object shape makes no call and does no lookup. That is what the
machine's `cached_get` and `guard_type` are for, and they still have no caller.

So the target is specific: **~95 ns per property access, of which none is
inherent.**

---

# Is the cache missing every time? No — and asking found a broken benchmark

The previous section left 27.2 ns per property read as unexplained and stated
the obvious hypothesis: a hit should be a compare, a branch and two loads, and
27 ns is close enough to the 24 ns a bare runtime call costs that the site was
probably missing on every read.

**It was not.** Counting is what settles it — a hit never reaches the runtime, so
counting calls to `rts_cache_resolve` counts misses:

```
one property read     25.9 ns/pass    misses 0
```

Zero misses over two million reads. The cache works.

## What the 27 ns actually was

Asking the question exposed a flaw in the measurement that had been there from
the start. The case was:

```js
t = t + o.n          // measured against:    t = t + n
```

`o.n` is **not proved numeric** — nothing knows what an object holds — so the
`+` beside it is a runtime call. The `+` in the baseline is an instruction,
because `n` is a proved local. The comparison was charging a 24 ns call to the
property read.

Measuring a read with nothing attached to it — an expression statement, which
evaluates and discards — gives the real number:

| | ns per pass | over the baseline |
|---|---:|---:|
| baseline, proved local | 0.8 | — |
| one read, discarded | 1.4 | **+0.5** |
| two reads, discarded | 2.3 | +1.5 |

**A property read costs about 0.9 ns** — a compare, a branch and two loads,
which is what the design said it would be.

## The full arc, and what each step was worth

| | ns per read |
|---|---:|
| E4, object as a Rust `Vec` behind a call | 132.8 |
| after removing an allocation per read | 94.8 |
| after moving objects into the region | 90.2 |
| after `guard_type` + `cached_get` | **~0.9** |

The middle two steps look like almost nothing and were the whole point: neither
made anything fast, and together they made the last step possible. An object
that is a Rust enum holding a `Vec` cannot be guarded and cannot be loaded from.

## What is still slow, correctly attributed this time

**A property write: 71.7 ns.** Still a runtime call — nothing emits a cached
store yet, and the machine has no `cached_set` to emit.

**Arithmetic on a property: ~24 ns per operator.** Not the read's cost and not
fixable by caching. `o.n + 1` is a call because nothing proved `o.n` is a number,
and the type pass proves things about *locals*. Proving something about what an
object holds is what a shape carrying a representation would buy — `ShapeTree`
already stores one per property, and nothing reads it.

## The rule this is the second example of

The type pass measurement found a number and acted on it. This one found a
number, doubted it, and the doubt found a broken measurement. Both mattered; the
second more.

**A measurement that cannot be wrong has not been checked.** The counter cost
four lines and turned an unexplained 27 ns into a 0.9 ns read and a 24 ns call
that was never the subject.

---

# Guarding what nothing proved

The previous section attributed 24 ns per operator to arithmetic on a property
and said the type pass could not reach it: that pass proves things about
**locals**, and `o.n` is not one — nothing knows what an object holds.

A guard needs no such knowledge. It tests the value it actually got.

```text
  guard a is a double ── not one ──┐
         │                         │
  guard b is a double ── not one ──┤
         │                         │
    instruction                  slow: the call
         │                         │
         └──────► join(value) ◄────┘
```

| | before | after |
|---|---:|---:|
| `t = t + o.n` | 27.9 ns | **3.1 ns** |
| `t = t + o.n + o.m` | 54.3 ns | 6.2 ns |
| the fully proved kernel | 18.05 ms | 18.05 ms |

Nine times, and the last row is the one that says it cost nothing: a loop the
type pass already proved is unchanged, because a proved operand never reaches
the guard.

## Why two guards rather than one test of both

A guard **narrows**, and narrowing is what makes the instruction legal. A test
answering "both are doubles" without producing the two narrowed values would
leave the operands generic, and `arith` refuses those — which is the refusal
that makes the machine layer worth having.

## What it costs when the guess is wrong

Two compares and a branch, then the call that would have happened anyway. A
program whose operands are never numbers pays that and nothing else; a guard
cannot make the slow path slower than it was.

## And the shape now records what it saw

`ShapeTree` always carried a representation per property — `transition` takes
one and `repr_of` reads it back — and the runtime wrote `Tagged` for everything,
which made that field a place where a fact could have been and was not. It now
records what the value turned out to be.

Worth being exact about what that is: an **observation about one write**, not a
promise about the property. A later write of something else takes a different
transition, so the object arrives at a different shape and every site that
remembered the old one stops recognising it. Which is what a shape is for.

Nothing reads it yet, and that is the honest state. The read side is the guard
above, which needs no shape at all — a site does not know which shape it will
see, and that is the whole reason `cached_get` exists. What the recorded
representation buys is a *layout* decision: a field the shape says holds a
double can be stored as raw bits rather than a tagged word, which is a change to
what a cell looks like and not to what a site emits.

---

# The cached store

`cached_set` did not exist in the machine; a property write was the last thing
still going through a runtime call on every pass.

| | before | after |
|---|---:|---:|
| one property write | 71.8 ns | **5.4 ns** |

Thirteen times, and it is the mirror of the read with one asymmetry that is not
symmetry: **the slow path is not a slower store.** A key the object does not have
changes what the object *is*, which is a shape transition — and a transition is
not something a site can remember, because the next object through it may be at
a different layout entirely. So the fast path is exactly the case a store
repeats: a property the object already has.

## The order this had to be built in

The write barrier first. `lower/body.rs` emits one on every reference store, for
the reason stated there — *"a barrier that was needed and skipped produces a
reference the collector never learns about"* — and the host answered
`RtEntry::WriteBarrier` with a **null pointer** while nothing emitted one.

A cached store built before `rts_write_barrier` existed would have called that
null pointer: a process dying with no diagnostic, on the first property write of
the first test. The order was not a preference.

What the barrier does today is count. There is no collector to tell, and
counting rather than doing nothing means the call site does not have to be found
again the day there is one.

## Where property access ended up

| | E4 | now |
|---|---:|---:|
| read | 132.8 ns | ~0.9 ns |
| write | 71.8 ns | 5.4 ns |
| arithmetic on a property | 24 ns / operator | ~1 ns / operator |

The counter also started reporting one or two misses where it reported none,
and the reason is that it now sees more: a write site is a cached site now, so
its cold start counts. One miss per site over two million passes.

---

# 2026-08-21: the whole surface, three runtimes, and what the ruler turned out to be

Everything above measures **arithmetic and property access**, which were the two
areas that had been optimised. This measures the rest — calls, allocation,
arrays, strings, collections, regular expressions, typed arrays and control
flow — and its most reusable finding is not about the engine.

Produced by `target/release/rts.exe run bench/analytic.ts`, against `node`
v25.9.0 and `bun` 1.4.0 on the same machine, one process each, sequentially.
Machine: AMD Ryzen 7 5700G (Zen 3, 8 cores, 3.8 GHz base). Tree at `617e2d5f`.

## The instrument is noisier than most of the differences people want to read

This is the finding to carry forward, and it invalidated a first pass of this
same campaign.

The **same binary**, run six times over `bench/analytic.ts`, on an idle machine:

| row | min | max | spread |
|---|---:|---:|---:|
| `string index []` | 104.44 | 135.85 | **30.1 %** |
| `string slice 16` | 179.99 | 224.09 | **24.5 %** |
| `string indexOf 256` | 293.68 | 330.36 | 12.5 % |
| `coll Map.has` | 45.00 | 49.78 | 10.6 % |
| `arith int mul` | 14.33 | 14.58 | 1.7 % |

A first pass over one A/B pair reported five regressions. Measured against the
spread above, **four of them were the instrument**. The rows that vary are the
ones that allocate, and the reason is in this file already: an allocation row is
39-73 % collector, and the collector's cost depends on what the heap is holding,
which depends on every case that ran before it in the same process.

**So a row of `bench/analytic.ts` may not be compared between two builds without
the same row's baseline-against-itself spread beside it.** Three pairs is the
minimum, and the comparison is min-against-min with the spread quoted.

There is a second band underneath that one, and it is not the harness.
**Rebuilding `rts-core` moves several rows by 2-10 % in both directions, on
paths the source change provably cannot reach.** Demonstrated with
`entry_probe` (below): a change confined to symbol keys moved `array_new` by
+9.7 % and `add` by +4.0 %, neither of which touches a symbol. Nothing in that
band is a result.

### The worst case is `string split 16`, and it has a cause

That row swings by a factor of **8.6 on the unchanged binary and 10.2 after** —
978 to 8361 across fourteen baseline runs, 944 to 9608 across eleven. It is
bimodal, not noisy: a cluster near 1000 and a cluster near 4000-9600, with
nothing between.

The cause is the calibration, and it is worth knowing because it applies to
every allocating row. `measure` grows `n` by four until a case takes
`TARGET_MS`, and it decides that from **one un-warmed run**, before the warmup
and the best-of-three that produce the reported number. Five runs, printing the
`n` calibration settled on:

```
n=16384  best=57.55 ms   ->  3512 ns/op
n= 4096  best= 3.98 ms   ->   973 ns/op
n=16384  best=39.57 ms   ->  2415 ns/op
n=16384  best=171.98 ms  -> 10497 ns/op
n= 4096  best= 4.38 ms   ->  1070 ns/op
```

When a collection lands inside the calibration run it reports over 40 ms, `n`
stops at 4096, and the warmed runs then take 4 ms — ten times less than the
measurement that chose the count. When calibration is clean, `n` grows to
16384, and at four times the iterations the case genuinely does reach
collections, so the per-operation number is four to ten times higher.

**Both numbers are true of the `n` each used.** The row is not measuring a
fixed cost, because `split` allocates and its cost per operation is not
constant in `n`. The harness's own comment already knows non-linear cases
exist — it grows by a factor rather than extrapolating for exactly that reason
— and what it does not cover is that for such a case the REPORTED number
depends on where calibration happened to stop.

The cheapest honest repair is additive and changes no number: report the `n`
each row settled on, so two runs that measured different things say so. Not
done here, because changing the harness changes every figure in this file and
that is a decision with an owner.

## A second instrument, because of the first

`crates/rts-core/examples/entry_probe.rs` calls the runtime entry points
directly from Rust — no compiled code, no `performance.now()`, no console inside
a timed region, every subject built once outside the loop, and the spread
between rounds printed beside the minimum.

It reproduces to **0.04-2 %** where `bench/analytic.ts` varies up to 30 %:

```
cargo run --release --example entry_probe -p rts-core
```

| operation | ns/op | reading |
|---|---:|---|
| `type_of(double)` | 2.50 | **the cost of reaching the runtime at all** |
| `add(1.0, 2.0)` | 11.4 | a generic operator entry point |
| `get_property` | 22.5 | the runtime property path — compiled code skips it via `CachedGet` |
| `set_property` | 26.1 | the same, writing |
| `object_new(2)` | 10.4 | allocation is not slow |
| `array_new(4)` | 104 | 10x an object |
| `closure_new` | 545 | 52x an object |
| `instance_of` | 157 | **63x the boundary — the cost is inside the function** |

The 2.50 ns settles a disagreement three separate investigations reached
differently (2-3 ns against 5.3 ns). It also kills the reading that the table
below is "how many runtime calls does this make": `Math.sqrt` and `Math.floor`
are **not calls** — `emit/call.rs` lowers them to `FloatUnary` — so the 3.09 ns
those rows report says nothing about a boundary.

## Cost is not a sum. It is the longest carried chain, or the issue width.

Measured on the same loop, varying only how many accumulator chains it carries:

| | ns/iteration | cycles |
|---|---:|---:|
| 1 add | 1.140 | 5.0 |
| 2 independent | 1.138 | 5.0 |
| 4 independent | 1.367 | 6.0 |
| 8 independent | **1.476** | 6.5 |
| 4 **dependent** | 2.731 | 12.0 |

Eight independent adds cost 29 % more than one. If costs added, they would cost
eight times. What the machine charges is `max(longest loop-carried dependency,
issue throughput)`.

Cycles derived rather than counted: one **dependent** f64 add measures 0.683 ns,
and Zen 3's documented `ADDSD` latency is 3 cycles, giving **228 ps/cycle
(~4.4 GHz under boost)**. The 1.140 ns floor is then 5.0 cycles — and it is not
three costs summed: `rts ir` shows both the accumulator and the counter as
proven `F64` in `block1`, two independent 3-cycle chains, with the extra two
cycles being the five block boundaries the loop body crosses.

**The consequence for optimisation.** Removing a guard pays when the guarded
value is **carried by the loop**, and buys nothing when it is not. Both halves
were measured here:

- **Pays.** `ToInt32` lowered a `divsd` (13-14 cycles) onto the carried chain.
  Replacing it with a multiply by `2^-32` — bit-identical, see
  `lower/body.rs` — plus letting `emit/proven.rs` claim `&`/`|`/`^` numeric so
  the accumulator stops round-tripping through `Tagged`, took `arith int mul`
  from 14.41 to **7.35 ns**.
- **Does not.** Hoisting the loop-invariant `CachedGet` of a captured binding,
  which an additive model says is pure waste: 35.53 / 40.96 with it against
  35.51 / 34.24 without. The noise between repeats of one configuration is
  larger than the difference between configurations.

## What moved, and what it cost

Seven changes, measured min-of-3-pairs against a kept `617e2d5f` binary, each
gain larger than that row's own baseline spread:

| row | before | after | |
|---|---:|---:|---|
| `arith int mul` | 14.41 | 7.35 | **-49 %** |
| `arith int div` | 4.07 | 2.40 | **-41 %** |
| `string indexOf 256` | 287.3 | 186.0 | **-35 %** |
| `array push+pop` | 170.2 | 136.7 | -20 % |
| `binary DataView getU32` | 43.4 | 35.3 | -19 % |
| `call closure make+call` | 1719.9 | 1494.1 | -13 % |
| `string parseInt` | 148.1 | 129.3 | -13 % |
| `flow switch 8-way` | 4.26 | 3.82 | -10 % |
| `binary subarray 64` | 264.4 | 241.0 | -9 % |
| `array filter 16` / `map 16` | 202.2 / 200.8 | 187.3 / 187.6 | -7 % |
| `prop in operator` | 83.9 | 79.0 | -6 % |
| `binary Uint8Array write` | 29.2 | 27.6 | -6 % |
| `array index read` | 15.5 | 14.8 | -5 % |
| `arith int mod` | 5.29 | 5.05 | -5 % |
| `string index []` | 105.2 | 100.6 | -4 % |
| `array for-of 16` | 46.3 | 44.6 | -4 % |

`binary alloc Uint8Array 64` is quoted apart because its baseline spread is
larger than most rows' values: 1783-2406 across six runs, against 847-859 for
every run after. The distributions do not overlap, which is the only form that
comparison takes.

**Regressions, stated.** `string charCodeAt` +6.8 % and `prop instanceof`
+6.2 %, both above their rows' spread. Neither is attributed to a code path, and
what was ruled out is recorded rather than the conclusion guessed: for
`instanceof` the emitted IR is **byte-identical** between the two binaries
(1328 lines, zero differences), and the same runtime function measured through
`entry_probe` moves +1.6 %. Both sit inside the 2-10 % rebuild band measured
above. Three further rows — `json stringify` +4.8 %, `slice` +3.6 %,
`parseFloat` +2.7 % — are within their own spread and are not results.

**One change was written, measured and dropped.** Folding the bare-class-
constructor test into the borrow that pushes a call's markers takes `called`
from three context borrows before the jump to two, which is sound and reads
better. It produced **no measurable win on any call row**, and cost 7.6 points
on `instanceof`. Reasoned, not measured, with a measured cost: it is out.

## A correctness defect found by measuring, not by reading

`Context` declares 22 `Aside` side tables. `collect_cycle::release` cleared 21.
The one it missed was `detached`, and that table is **read as a live fact** —
`buffers::window` refuses a view whose cell is marked — so the next object to be
handed a reclaimed cell index was born detached.

80 000 iterations, each making a fresh `ArrayBuffer` and transferring it once,
which the language always permits because each pass's buffer is a different
object: `TypeError: ArrayBuffer is detached`, uncaught, on `617e2d5f`. Node runs
the same program to completion, and so does the tree after one line.

`collect_cycle`'s own module documentation predicts a forgotten table and calls
it a leak. For a table that is read, it is corruption.

## What this does not say

- **Nothing about a program.** Every case runs one action in a loop with its
  operands already in hand — the best case for caches and the worst for
  representativeness. It says which actions are expensive, not how much of any
  program they are.
- **Nothing in the 2-10 % band on a single row.** See above.
- Node's and bun's columns are not targets where they sit at their own floor:
  both hoist loop-invariant work, so `toUpperCase`, `indexOf`, `slice` and
  `instanceof` at 0.37/0.46 ns there are work removed from the loop, not cheap
  work.

## The work list, with the evidence already taken

Ranked by measured size, not by expected ease:

1. **`species::made` runs two full prototype-chain walks per `map`/`filter`/
   `slice`/`concat` call** — measured at ~3.3 us of fixed cost per call, against
   32 ns per element. `array_proto/species.rs`.
2. **`closure_new` builds a `prototype` object for every callable**, which is
   also a divergence: `(x=>x).prototype` is an object here and `undefined` in
   node and bun, and `new (x=>x)()` does not throw. The four uncached property
   writes it performs are ~1.4 us of the row's ~1.5.
3. **`Inst::ElementLoad` is built, verified, lowered — and unreachable.** Its
   only producer is gated on a condition that is a compile-time constant
   `false`; `bench/analytic.ts` emits zero of them.
4. **`for-of` allocates an empty array per loop ENTRY** to read its
   `@@iterator`, and copies the source array. ~527 ns before the first element.
5. **An inherited property read as a VALUE misses its cache forever** —
   3 001 049 misses over 3 000 000 reads. `bench/analytic.ts` has no row for it.
