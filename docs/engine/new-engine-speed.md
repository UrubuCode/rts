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
cargo build --release -p rts-host-rwk --example kernel --example perop
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
