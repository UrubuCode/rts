# Six things a hot path was doing that nothing asked it to

Settled 2026-08-21. Not one optimization — six, grouped because they are the
same *kind* of finding and because grouping them is the point: none of them is a
design change, none needed a new mechanism, and each was costing more than the
operation it sat inside — except the one that was costing nothing at all, which
is §5 and is the reason each of them had to be measured separately.

They are here rather than in six documents because the lesson is the category.
An engine accumulates these the way a house accumulates dust: a debug switch
added while chasing a bug, a defensive call that stopped being reachable, a
helper reached for out of habit when a cheaper one sits three functions below it.
Each was individually reasonable when written. What none of them had was anyone
asking, afterwards, *what does this cost on the path it is on.*

---

**One of the six turned out to be worth nothing**, and it is §5 rather than a
footnote: the escape-analysis kill-switch recovers six candidates across a
400-file corpus and **all six die under an independent rule**, so it gains
exactly zero replacements. It ships as a `refactor:` — stale reasoning removed,
three tests added — and not as a `perf:`. Finding that out cost one measurement
and would otherwise have cost a false claim in a commit message.

Together they took `rts run empty.ts` from **20.82 ms to 18.35 ms** and moved
eight rows of `bench/analytic.ts` by 7% to 51%. Both sets of numbers, with how
they were attributed, are under "What they moved".

## 1. An environment variable read on every property write

`crates/rts-core/src/entry/objects.rs`, inside `put` — the by-name property
write:

```rust
if std::env::var_os("RTS_CACHE_WHY").is_some() && context.resolves % 20_000 <= 1 {
```

`&&` evaluates left to right. So the environment was read **first**, on every
by-name property write, and the modulo that would have short-circuited it 19 999
times out of 20 000 never got the chance. The same switch was read the same way
in `cache::resolve`, and `cache::resolve_indirect` read `RTS_CHAIN_DEBUG` inside
a closure called on every refusal.

`std::env::var_os` is not a memory read on Windows. It re-encodes the name to
UTF-16, calls `GetEnvironmentVariableW`, and walks the process environment block.
`bench/isolated/src/bin/env_probe.rs`, release, 2026-08-21:

| shape | ns |
|---|---:|
| `env::var_os`, name **absent** — the case in every run where nobody asked | **172.2** |
| `env::var_os`, name present | 227.5 |
| `OnceLock<bool>` | **0.43** |
| a plain `bool` | 0.24 |
| the `resolves % 20_000` test that should have been first | 0.77 |

**Four hundred times, for a switch nobody set.**

The fix is not a new idea: `crates/rts-cranelift/src/probe/phase.rs:48` already
memoises `RTS_TIMING` in exactly this shape. What was missing was one place
saying so, which is now `crates/rts-core/src/entry/switches.rs` — four switches,
one macro, one rule.

**What it costs, stated:** a switch set *after* the first read is ignored.
Setting one from inside a running program used to take effect at the next
property write and now does not. Nothing does that, and a diagnostic that turns
on halfway through a run produces a log whose first half is missing — which is
worse than one that does not turn on. `RTS_TIMING` already took the same trade.

**Where it mattered most is not the benchmark.** `prop write own` at 9.61 ns
cannot have been paying 172 ns, which is how the site was found to be on the
*by-name* path rather than the cached one. The path that does pay is
installation: the startup investigation counted **1 497 own property
definitions** built eagerly before a program runs, and every one of them goes
through `put`.

---

## 2. A write barrier for a crossing that cannot happen

`lower_cached_set`'s hit path called `__rts_write_barrier` on **every** cached
property store, and `Inst::FieldStore` called it for every store into a traced
field.

`BarrierKind` has two variants: `None` and `CrossRegion`. So a barrier reports
exactly one thing — a reference crossing from one region into another — and
under `mem::Addressing::Single` every reference decodes into the one region.
There is no store that could produce a crossing.

`rts-core`'s `entry::barrier` already argued the remembered set was empty, from
the other side: *"A thread can only reach references its own region handed out …
So the remembered set is correct and empty."* That argument is about which
programs exist. The one acted on here is stronger and structural: under this
addressing a crossing is **unrepresentable**, which is what makes eliding it
different from skipping a barrier — normally the worst kind of unsafe.

**How it was implemented matters as much as that it was.** The first attempt put
the test in `lower/body.rs`, which is precisely the flag `gc/barrier.rs` refuses
to have — its module doc says *"There is no flag on a store, no barrier
instruction a client emits, and no place to pass `false`."* It was redone as
`gc::crossing_is_possible(regions)`, derived in the one place, from a third fact
the layer already holds. `rts-cranelift/README.md` rules 8 and 9 were updated to
say so, because they enumerated the facts and now enumerate three.

**A test caught the first attempt**, which is the system working:
`storing_a_reference_reaches_the_barrier` compiles a program with a single-region
heap, runs it, and counts barriers. Its file's own header says *"A barrier that
is documented as unforgettable and never emitted is exactly the failure this file
exists to catch, and it was a real one until this commit."* It was not weakened.
It now runs against a **sharded** heap, and a second test pins the elision
against a single-region one — two tests and two heaps, because a single test
asserting the weaker claim would let either mistake through.

**What must be revisited:** a second non-`None` `BarrierKind`. A generational or
card-marking barrier is not about crossing regions and would fire in a
single-region heap. `crossing_is_possible`'s own documentation says this.

---

## 3. Cloning an array to ask how long it is

`array_proto::iterate::len_of` — the length every `forEach`, `map`, `filter`,
`find`, `some`, `every` and `reduce` reads once before its first callback:

```rust
with_current(|context| staged(context, this).map(|(_, elements)| elements.len()))
```

`staged` **copies the whole element vector**. That is what `staged` is for, and
its own documentation says so: a method that calls user code has to drop the
borrow before calling, so it takes a snapshot. `len_of` calls nothing. It reads
a length and returns a `usize`.

`super::borrowed` sits three functions below `staged` for exactly this
distinction, and its documentation already names the same mistake — *"copying a
thousand-element array to answer whether it contains a number is the whole cost
of the answer."*

Cost is proportional to the array and invisible in a small one. `xs.map(f)` on
sixteen elements copied sixteen words; on a hundred thousand it copied a hundred
thousand, once per call, to ask a question the `Vec` answers from its header.
`analytic.ts` measures `map 16` and `filter 16`, so this barely moves those rows
— and that is the point of writing it down: **the benchmark does not see it and
a real program does.**

---

## 4. An 8 MiB heap built and thrown away, every run — and another zero-filled for nothing

`Context::over` ended with `..Context::new(singletons, kinds)` to borrow `new`'s
field list. Rust evaluates that base expression **in full** before moving the
fields it keeps, and `Context::new` constructs `Region::with_capacity(1 << 16)`.

So every `Context::over` reserved 64 MiB of address space, zero-filled 8 MiB of
it through `Region::sharded`'s `resize`, and freed it — in addition to the region
the host had already built and handed in.

The two lists had also drifted into being one list written twice. The only
difference was that `over` re-stated twenty-two `Aside`s to give them the
region's width, and `Aside::new()` is *defined as* `Aside::in_region(0)` — so the
override passed zero where zero already was, for the single-region case, and the
real width for the sharded one. Passing the region to one constructor says the
same thing once, and deletes 28 lines.

**The regression is now unrepresentable rather than merely documented**, which is
the part worth keeping: `new` delegates to `over`, so writing `..Context::new()`
inside `over` again is infinite recursion — a very loud failure — instead of a
silent 8 MiB.

Verified field by field rather than by "it compiles": all 31 fields the old
`over` set are present in the merged constructor with the same values, every
`Aside` at `in_region(bits)` and `promises` at `in_region(region_index)`. A
struct literal makes a *missing* field a compile error; it does not make a
*wrong* one anything at all.

**And then the region the host *does* keep was zero-filling itself for nothing.**
`Region::sharded` did `reserve_exact(64 MiB)` then `resize(8 MiB, 0)`, and
`Vec::resize` with a zero fill is a `memset` — over memory the operating system
had just handed over, which is necessarily already zero. `vec![0; n]` is
specialised to `alloc_zeroed`, which asks for demand-zero pages and never writes
them; `truncate` then lowers the length without moving the allocation, so
`Region::base` — an immediate in compiled code that may never move — is
untouched. Measured at **1 515 547 ns against 37 814**, and confirmed in the
engine at 1.51 ms. `startup.md` has the full table and the invariant that makes
it safe.

---

## 5. A kill-switch defending against a walker that was fixed

`crates/rts-codegen/src/emit/escape.rs` discarded **every** scalar-replacement
candidate in a function body containing a template literal, a tagged template,
or either `super` form — `state.everything = true` — under a comment saying the
shared walker did not descend into those four.

It did not, when that was written. `capture.rs` then taught `walk_expr` to
descend into all four, and says so in its own comment, because a name mentioned
only inside a substitution was not being counted as captured and
`` function f() { return `${x}`; } `` failed to compile. The arm here was never
revisited. So **a single backtick anywhere in a function body disabled scalar
replacement for that whole body**, for a reason that had stopped existing.

The four are ordinary nodes now. A substitution is a value position, so
`` `${o.a}` `` keeps a candidate and `` `${o}` `` kills it — right, because an
object interpolated whole escapes into `ToString`. A tagged template is handled
beside `Call`, because `` o.m`…` `` hands `o` over as a receiver exactly as
`o.m()` does.

**And it is worth zero nanoseconds.** Measured across 400 files: six candidates
are recovered and all six die under an independent rule, so not one additional
allocation is removed. The change is still right — it deletes reasoning the code
contradicts, which `CLAUDE.md` requires, and adds three tests pinning behaviour
nothing pinned. It is a `refactor:`.

That it measured zero is the most useful thing about it. It was the most
*obviously* valuable of the six by inspection — a whole optimisation switched
off by a stale comment — and inspection was wrong.

---

## What they moved

Measured 2026-08-21, `bench/analytic.ts`, **median of three runs per binary,
alternated** so that machine drift falls on both. Against `target/baseline.exe`,
built from `97f66385`.

| row | before | after | |
|---|---:|---:|---|
| `prop write own` | 9.37 | **4.59** | −51.0% |
| `flow try/catch no throw` | 9.48 | **5.26** | −44.5% |
| `flow generator next` | 752.18 | **421.78** | −43.9% |
| `call closure make+call` | 1562.20 | **915.02** | −41.4% |
| `binary TextEncoder 16` | 1119.44 | **792.72** | −29.2% |
| `regex exec+group` | 2224.48 | **1651.64** | −25.8% |
| `flow throw+catch` | 1433.08 | **1090.84** | −23.9% |
| `array map 16` | 193.71 | **151.56** | −21.8% |
| `json stringify small` | 4360.40 | **4043.44** | −7.3% |
| `prop instanceof` | 233.71 | **217.12** | −7.1% |

**Attributed by bisection, not by assumption.** A build with *only* the switches
change reverted measures `call closure make+call` at 1554/1526/1602 — the
baseline's number — so the `var_os` fix owns that −41% entirely, and the barrier
owns the −51% on `prop write own` (present in both modified builds). The
arithmetic corroborates: `closure_new` performs two `objects::put` calls, so
~4 × 172 ns = 688 ns of `var_os`, against a measured −647 ns.

**`string split 16` is not in the table and must not be.** Six runs of it, three
per binary: base 1040 / 1054 / **9305**, new 8348 / 1099 / **1423**. A ninefold
spread inside one binary. See `measurements.md`.

**Two rows moved the wrong way and the cause is not any of these changes.**
`coll Map.get` +9% and `binary Float64Array rw` +11% in the full table. Run
down:

- the emitted IR for a `Map.get` loop is **byte-identical** between the two
  binaries, so nothing about the program changed;
- isolated, the row is 48.3 → 49.3 ns — **about one nanosecond**, not nine; the
  larger figure is what that row does inside the whole 76-case module;
- and a build with the barrier change **reverted** measures it at 52.3, *slower
  than the build that has it*.

A change whose removal makes a row slower is not the cause of that row being
slower. What is left is code layout: a 33 MB image whose functions shift when
any of them changes size, moving branch and cache-line alignment for code that
was not edited. It is real, it is not attributable to work added, and no version
of these changes avoids it.

The honest reading of that pair is the one this tree's rule 5 asks for: they are
in the *before* and *after* tables, they went the wrong way, and this paragraph
is why they are not being treated as a regression to fix.

## What these six have in common

Each was found by reading a hot path and asking what every line on it costs —
not by profiling, and not by suspecting the thing that turned out to be wrong.
The leading hypothesis of the whole investigation was that the runtime's context
lookup was the tax; `entry-tax.md` records that it is worth 0.53 ns. Four of
these are worth far more than that, one is worth nothing, and not one of them
was anybody's first guess.

The rule that would have caught them: **when a line is added to a path that runs
per operation, say what it costs on that path.** Five of the six had thorough
documentation explaining what they did and why they were correct. None of them
had a number.

And the rule that keeps the sixth honest: **measure each one separately, even
when they ship together.** §5 was the most obviously valuable of the six by
inspection and is worth zero. Had all six been measured only as a package, its
zero would have been hidden inside the others' −41% and it would have gone into
the record as a performance fix.
