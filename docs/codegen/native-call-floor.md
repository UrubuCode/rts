# Is what a built-in costs the Rust that implements it?

**No. It is the ~35 ns of getting there.** Settled 2026-08-28, over `b83eac1a`.

The premise this document was opened to test, in the form it was put: *the
engine's problem is purely the Rust symbols, so the way to fix it is to make
those symbol bodies faster.* The first half names the right rows. The second
half points at the wrong crate.

---

## 1. The falsifier

Hold the CALL SHAPE constant and vary only how much the Rust body does. If a
native whose body is one type check costs the same as one that probes a hash
table, the body is not what a built-in call costs.

`target/release/rts.exe`, Windows 11 Pro 26200, node v25.9.0, bun 1.4.0, the
same calibrated harness `bench/analytic.ts` uses.

| action | rts | node | bun |
|---|---:|---:|---:|
| floor (empty loop) | 1.01 | 1.27 | 0.42 |
| `f(a)` — **inlined, not called** — see §1a | **2.84** | 0.36 | 0.44 |
| `c.m(a)` — a user method | **24.75** | 0.35 | 0.33 |
| `Array.isArray(arr)` — body is one type check | **41.67** | 0.35 | 0.44 |
| `Number.isInteger(i)` — body is one `is_finite`/`fract` | **37.56** | 0.71 | 0.44 |
| `Object.is(obj, obj)` — body compares two `u64` | **38.83** | 0.35 | 0.44 |
| `set.has(7)` — body probes a hash table | **37.16** | 2.65 | 0.44 |
| `s16.charCodeAt(0)` | 57.26 | 0.55 | 0.44 |
| `arr.indexOf(15)`, 16 elements | 86.47 | 10.58 | 4.24 |
| `obj.hasOwnProperty("a")` | 164.36 | 5.19 | 1.17 |

**`Object.is` compares two integers and costs 38.83 ns. `set.has` probes a hash
table and costs 37.16.** The hash probe is *free* against the cost of reaching
it — it is, if anything, the cheaper row. Four natives whose bodies have nothing
in common land within 4.5 ns of one another, and the cheapest is not the one
with the cheapest body.

So for these rows the answer to "which Rust symbol should be optimised" is: none
of them. There is a flat toll of roughly 35 ns on the way in, and a body is
whatever shows above it. `hasOwnProperty` at 164 and `indexOf` at 86 are the
rows where a body genuinely is the cost, and they are the minority.

**A node or bun row under about 1.2 ns is a loop the JIT deleted, not a cost** —
`action-table-2026-08-26.md` §1 states this and it is why the ratio column is
not the headline. The rows here that support a ratio are `set.has` (2.65 on
node), `hasOwnProperty` (5.19) and `arr.indexOf` (10.58): 14×, 32× and 8×.

---

## 1a. CORRECTION: the 2.84 ns row is not a call

*2026-08-28, after the fact.* The row above was labelled "static, direct call"
throughout the first version of this document, and it is **an inlined function
body with no call in it**. `emit/inline.rs` substitutes a function whose body is
one expression, and `function freeFn(x) { return x + 1; }` is one.

The difference is 9× and nothing else changes:

| | ns |
|---|---:|
| `one(x) { return x + 1 }` — one expression, **inlined** | **2.97** |
| `two(x) { const y = x + 1; return y }` — two statements, **called** | **26.72** |

Same semantics, same call site, same loop. So **a plain JavaScript function call
in this engine costs about 27 ns**, which is beside a method's 23 and a
built-in's 33 rather than an order below them — and the "static call" was never
the cheap control it was read as. Every ablation table below still stands, since
it was used as a control precisely because it does not move; what changes is the
name, and the conclusion in §3 that a method call is ~22 ns *above a call*. It
is not. It is roughly what any uninlined call costs here.

**Where this was caught.** `bench/monte_carlo_pi.ts`, by moving its RNG from an
inlinable shape to a called one and watching the loop go from 128.5 ms to 597 —
~24.5 ns per call. §7a and §7b have the whole decomposition, including the part
of it that is the calling convention rather than the crossing.

---

## 2. What "under one nanosecond" can and cannot mean

`entry-tax.md` measured the floor of one non-inlinable `extern "C"` call from a
loop, with nothing about the context reached: **1.19 ns**. So no operation that
CROSSES into `rts-core` can cost under a nanosecond however good its body is.
The rows of `bench/analytic.ts` that are already under 1.2 ns — `int add`,
`int xor`, `object literal 8` — are exactly the ones where nothing is called.

That is the useful form of the goal. **Under 1 ns means not calling the symbol**,
which is a lowering question for `rts-codegen` and `rts-cranelift`, not a body
question for `rts-core`. `Math.sqrt` and `Math.floor` are already there, at 2.87
and 2.90 ns, because `emit/call.rs`'s `machine_operation` turns them into the
instruction the hardware has rather than a call. Nothing else in the table
reaches that band while it is still a call.

---

## 3. What the toll is made of, priced by ablation

Each candidate ablated in its own binary, measured against a kept baseline,
alternated, min per row. The static call is the control throughout.

### 3a. Three per-activation stacks — **7.3 to 10.2 ns**

`called` pushes `pending_arguments` and `pending_counts`; `invoke` pushes
`callees`; all three pop. Ablating all six sites:

| action | base | no stacks | saved |
|---|---:|---:|---:|
| `f(a)` *(control)* | 2.88 | 2.94 | −0.06 |
| `c.m(a)` | 25.17 | 17.46 | **7.71** |
| `Array.isArray` | 42.29 | 32.13 | **10.16** |
| `Number.isInteger` | 38.35 | 29.66 | **8.69** |
| `Object.is` | 39.46 | 32.12 | **7.34** |
| `set.has(7)` | 35.18 | 27.78 | **7.40** |

This reproduces `action-table-2026-08-26.md` §4 independently, and its
conclusion holds: it is **the stacks, not the borrows**. Merging the borrows was
implemented in full there and bought nothing. The ablation is not shippable —
a callee's rest parameter and its `arguments` read those stacks.

**And there is a trap in the obvious fix, found here and not there.** Merging
the three `Vec`s into one `Vec<Activation>` looks like the contained version of
this. It is not contained: `callees` and `pending_arguments` are **GC roots**,
scanned by `roots::context_roots` as flat `&[u64]` slices, and `callees.len()`
is an activation depth `function_proto` reads while `throw` walks the same list
backwards for a stack trace. Three consumers, three different shapes. Anyone
taking this on prices the root scan too.

### 3b. `SetCallName`, a crossing per named call — **2.3 to 2.9 ns**

| action | base | ablated | saved |
|---|---:|---:|---:|
| `f(a)` *(control)* | 2.86 | 2.88 | −0.02 |
| `c.m(a)` | 24.97 | 22.62 | 2.35 |
| `Array.isArray` | 41.74 | 38.20 | 3.54 |
| `set.has(7)` | 34.48 | 31.85 | 2.63 |

This is the one that ships below. `emit/call.rs` already named the fix in a
comment and `action-table-2026-08-26.md` ranked it third.

---

## 4. Shipped: the callee's spelling is an operand, not a crossing

`__rts_set_call_name` was a whole runtime call emitted before every named call —
a jump, a context borrow and a literal-table read — to record a name used only
in the message of a `TypeError` that a working program never raises.

It is now the fourth operand of `__rts_call_counted`, beside the argument count
that became an operand for the same reason. Three things changed and each is
load-bearing:

- **The name travels as a NUMBER.** `Spelling::Literal(i64)` carries the literal
  index, not the text. Resolving it on the way in would put a bounds check and a
  load on every call to answer a question only the failing branch asks.
- **`invoke` resolves it in the branch that raises**, and nowhere else. The
  successful call pays one register.
- **Ordering got stronger, not merely preserved.** `emit_set_call_name` had to
  be emitted *last*, after every argument, so an argument that called something
  of its own could not overwrite what the site had recorded. A constant operand
  cannot be overwritten by anything, because nothing runs between the operands
  and the jump.

### What it moved

Five alternations per binary, min per row:

| action | before | after | saved | |
|---|---:|---:|---:|---:|
| floor *(control)* | 1.04 | 1.05 | −0.01 | −1.0% |
| `f(a)` *(control)* | 2.93 | 2.93 | 0.00 | 0.0% |
| `Array.isArray` | 42.75 | 38.18 | 4.57 | **10.7%** |
| `Number.isInteger` | 38.72 | 35.69 | 3.03 | **7.8%** |
| `c.m(a)` | 24.98 | 23.05 | 1.93 | **7.7%** |
| `set.has(7)` | 35.36 | 33.45 | 1.91 | **5.4%** |
| `Object.is` | 39.91 | 38.09 | 1.82 | 4.6% |
| `s16.charCodeAt(0)` | 59.49 | 56.95 | 2.54 | 4.3% |
| `arr.indexOf(15)` | 88.53 | 85.58 | 2.95 | 3.3% |
| `obj.hasOwnProperty` | 165.47 | 162.00 | 3.47 | 2.1% |

**And a program moved**, which rule 4 requires and which the benches in `bench/`
could not show: `objbench`, `monte_carlo_pi` and `pi_machin` are arithmetic and
allocation loops with almost no named calls in them, so the prediction was that
they would not move, and they did not. A loop of method and built-in calls,
same checksum both ways, five alternations:

| | ms |
|---|---:|
| before | 176.47 · 174.81 · 181.05 · 175.58 · 178.53 |
| after | 166.97 · 167.30 · **163.67** · 164.31 · 167.55 |

**174.81 → 163.67, 6.4%**, and every individual pairing goes the same way.

### The rows that went the wrong way

Rule 5, and the caveat matters more than the rows. Over `bench/analytic.ts` at
two runs per binary the FLOOR itself moved −26% (0.91 → 1.15), which is a row
this change cannot touch. That sets the noise band, and every row below sits
inside it: `prop instanceof` −6.6%, `arith Math.random` −7.5%, `string length`
−5.0%, `string split 16` −4.3%, `alloc class instance` −4.1%, `prop typeof`
−3.3%. None is attributed to this change and none is dismissed; the claim above
rests on the five-alternation ladder and the program, not on this table.

### What it did not break

Compared per file, never net. `target/release/rts.exe test` against a kept
baseline binary of the same tree without the change:

| | files | tests |
|---|---|---|
| before | 752 passed, 61 failed, 813 total | 3039 passed, 64 failed |
| after | 752 passed, 61 failed, 813 total | 3039 passed, 64 failed |

**The LOST list is empty and so is the GAINED list** — 59 of the 61 failing
files were recoverable by name from both reports and the two sets are identical.
That is the claim; the net equality above is not.

`cargo test --profile fast --no-fail-fast -p rts-codegen -p rts-core -p rts-host`
answers **978 passed, 3 failed**. The three —
`a_construct_still_missing_is_refused_by_name_rather_than_approximated`,
`an_iterator_carries_the_helpers_a_program_expects`,
`an_object_operand_is_converted_by_its_own_method` — are **pre-existing**, and
that is measured rather than assumed: the snippets they assert produce
byte-identical output on the baseline binary and on this one, including the
`consumed=0` that the iterator-helper test wants to be `1`.

Two of the three name areas this change touches, which is why they were checked
by hand rather than taken on the 08-23 record of "3 failed".

And the naming this operand exists to preserve was diffed on every call shape
that can produce it — a member call, a chained one, a bare name, a
non-callable primitive, more than four arguments, and a spread — identical both
ways.

### What it costs

- **A fourth stack slot at every call.** Eight parameters on the Windows x64
  ABI means four spill past the register arguments where three did. Paid, and
  the measurement above is net of it.
- **A second way to say how a callee was spelled, and it is admitted rather than
  hidden.** `set_call_name` and `Context::pending_call_name` still exist, for
  the one path that cannot carry an operand: a call with a vector
  (`CallWithArgs`, reached by more than four arguments or by a spread) has no
  operand slot to put a name on. `Spelling::Taken` is how the two meet, and
  `call_with_args` now takes the name itself rather than leaving `invoke` to —
  which is what stops a name written for one call from being reported for the
  next call that has none.
- **Three compiler-emitted calls had to learn the operand**: the
  `[Symbol.iterator]()` that `foreach.rs`, `destructure/array.rs` and
  `delegate.rs` write. They pass `None`, because a call the compiler wrote has
  no source spelling.

---

## 5. Also shipped: `ToBoolean` and `StrictEquals` cannot raise

`runtime/raising.rs` is the list of operations whose throw check is dead code,
and its own header says the entries on it "are not the common ones. It is the
mechanism the collection would use." These two are the common ones: every `if`,
`while`, `&&`, `||` and `!` over a value the type pass did not prove reaches
`ToBoolean`, and every `===` over one reaches `StrictEquals`.

Both bodies were read in full, which is the standard that list sets. Neither
allocates, neither coerces, neither can reach user code: `to_boolean_in` is two
`Context` field reads and two side-table reads for `""` and `0n`, and
`strict_equals` is `bigints::same` plus `values_strict_equals` over
`context.same_text`. **ToBoolean is the one coercion in the language that never
consults the object** — no `valueOf`, no `toString`, no `Symbol.toPrimitive` —
which is exactly what separates it from `Less` and `LooseEquals`, which stay
off the list.

What it is worth is small and is stated as such:

| | |
|---|---|
| emitted blocks, `bench/analytic.ts` | 4 277 → **4 233** (−1.0%) |
| emitted IR lines | 17 634 → **17 457** (−1.0%) |
| a conditional-heavy program, six alternations | 96.79 → **95.79 ms** (−1.0%) |

Five of six pairings favoured it and none of the six went the other way by more
than noise, which is the shape the module's own estimate predicts — "between 0.1
and 0.6 ns per removed check".

`TypeOf` was read too and deliberately left off: it allocates its answer on
first use, and every entry on that list so far does not. Allocation cannot
record a throw here — `alloc::heap_exhausted` calls `process::exit` — so the
exemption is very probably correct, and "very probably" is the wrong standard
for a list whose false entries swallow throws. It is recorded at the bottom of
`raising.rs` so the next audit does not have to read it again.

---

## 5a. Shipped: the class-constructor check moves into a borrow that already happens

Every call asked whether its callee was a class constructor being reached
without `new` — a question a working program answers "no" to every time — and
asked it through a `with_current` of its own, before any other work. Ablating it
entirely priced the answer at about 2 ns on every call in every program:
`c.m(a)` −10.0%, `Number.isInteger` −7.7%, `set.has(7)` −5.1%, static-call
control −0.3%.

**Two shapes were tried and only the second paid, which inverts the obvious
diagnosis.**

*The one that did not.* The check read `class_constructors` and `invoke` read
`callables` a moment later — two `Aside<T>` keyed by the same cell, so two
bounds checks and two cache lines per call. Merging the flag into the callable's
entry makes one probe answer both. Measured, five alternations: **nothing**, and
if anything slightly worse — `c.m(a)` −2.5%, `Number.isInteger` −2.3%,
`set.has(7)` −0.8%. The entry grew from 16 bytes to 24, which is the likely
counterweight. So **the second table was not the cost.**

*The one that did.* Folding the check into the `with_current` that `called`
already takes to push its stacks — the same probe, one fewer borrow and one
fewer non-inlined call:

| action | before | after | |
|---|---:|---:|---:|
| floor *(control)* | 1.02 | 1.02 | 0.0% |
| `f(a)` *(control)* | 2.85 | 2.89 | −1.4% |
| `c.m(a)` | 22.61 | 20.92 | **7.5%** |
| `arr.indexOf(15)` | 86.98 | 80.75 | **7.2%** |
| `s16.charCodeAt(0)` | 57.05 | 53.13 | **6.9%** |
| `set.has(7)` | 32.86 | 30.75 | **6.4%** |
| `Object.is` | 37.59 | 35.38 | 5.9% |
| `Number.isInteger` | 34.19 | 32.51 | 4.9% |

`obj.hasOwnProperty` read −6.0% and is the one row that went the wrong way; it
has been the noisiest row in this document all session and no mechanism connects
it to this change.

**A program moved**, five alternations, same checksum: **161.60 → 156.99 ms**,
every pairing in the same direction.

### What it cost, and what it corrects

The merged table is kept even though it measured neutral on its own, because it
also deletes an entire `Aside` — one fewer table to allocate, and one fewer
`remove` per cell the sweep frees. The credit for the 5–7% belongs to the fold,
not to the merge, and both are stated so that nobody reads the merge as the
lesson.

**And it partly corrects `action-table-2026-08-26.md` §4**, which concluded that
`with_current` "is close to free" after a full borrow-merging rewrite bought
nothing. That remains true of the five borrows *around the jump* it rewrote. It
is not true of this one, which was a separate non-inlined function called before
any other work — the difference between merging borrows that already sit
together and removing one that sits alone.

Semantics unchanged: `class C {}; C()` still throws, `new C()` still runs,
`extends` still works, and the refusal still happens before anything is pushed,
so a refused call leaves the activation stacks exactly as it found them.

---

## 6. REFUTED: putting a short string's bytes in the string

**Do not do this in the shape below.** It was implemented in full, measured, and
reverted. The isolated experiment that gated it was right about what it
measured and wrong about what it predicted, which is rule 2 of
`docs/codegen/README.md` doing its job in the direction that costs a day.

**The premise held.** `Repr::Latin1` holds a `Vec<u8>`, so every string costs a
`malloc` of its own on top of its region cell — and
`bench/isolated/src/bin/short_text.rs` priced removing it at **25 ns**: 33.6 ns
for a one-byte string through a `Vec`, 7.9 inline, with the past-the-bound row
flat. That is a real number and it is why the string cluster is what it is:
`String(7)` costs 126.8 ns against 75.9 for `new Callee()`, and
`"a,…,h".split(",")` is 1 286.7 ns for eight pieces — ~110 ns each, which is not
the splitting.

**And the engine agreed, on the rows that MAKE strings**, with `Bytes` inlining
22 bytes and the heap arm a `Box<[u8]>` so that `Repr` stayed at exactly 32
bytes (pinned by a test, because the first attempt grew it and that cost 17% of
`instanceof` on its own):

| row | before | after | |
|---|---:|---:|---:|
| `string concat 2` | 114.3 | 75.5 | **+33.9%** |
| `string slice 16` | 153.7 | 109.0 | **+29.1%** |
| `string number->string` | 119.0 | 88.5 | +25.7% |
| `array join 16` | 79.7 | 60.2 | +24.6% |
| `string toUpperCase 16` | 166.5 | 131.9 | +20.8% |
| `string split 16` | 1 090.8 | 874.9 | +19.8% |
| `json parse small` | 1 699.9 | 1 580.8 | +7.0% |

**It lost more than that on the rows that READ them**, five alternations,
controls flat (`floor` 0.0%, `prop read own` −0.2%):

| row | before | after | |
|---|---:|---:|---:|
| `prop typeof` | 20.6 | 27.8 | **−35.4%** |
| `prop in operator` | 31.7 | 38.3 | **−20.9%** |
| `string equals` | 9.6 | 11.0 | −14.8% |
| `prop instanceof` | 112.8 | 122.4 | −8.5% |

**The diagnosis is not the size and not the inlining — it is a branch.** That
was established rather than guessed: a second binary with inlining ablated and
everything else kept still regressed (`in` −15.5%, `instanceof` −9.8%,
`string equals` −9.0%), so the cost survives even when no string is ever inline.
`Repr` was already a two-way match, and narrow text now needs a **second** match
on every access — `Str::narrow`, `same_units`, the key lookup and `units()` all
pay it. **The engine reads text far more often than it makes it**, so a branch
per read outweighs a `malloc` per creation.

What would make it work is a `Bytes` whose `as_slice` does not branch: a
pointer and a length where the pointer addresses either the value's own inline
bytes or the heap. That is the classic small-string layout, it needs `unsafe`
and a moves-invalidate-the-pointer argument, and it is a different change from
this one — not this one with the bound tuned.

**Keep the isolated experiment.** `short_text.rs` is the evidence for the 25 ns
and for the fallback being free; what it could not see is the surrounding
program, which is exactly what `README.md` rule 2 says an isolated number does
not say.

---

## 7. WHY the call costs what it does: a reason that expired

The measurement above says where the time is. This says why it is there, and it
is not an accumulation of small mistakes — it is one decision that was correct
when it was taken and was never revisited.

`rts-core`'s `entry::functions` and `rts-codegen`'s `RuntimeOp::Call` both open
with the same argument for why a JavaScript call is a runtime crossing rather
than an indirect jump. Two reasons are given. The first is permanent:

> a callee is a *value*, and finding out whether it is code reads the heap

The second is the one that "decided the shape", in its own words:

> `1()` throws a `TypeError`. Throwing needs the machine's protected regions and
> nothing emits those yet, so the check has to live somewhere that can fail
> without them. **Compiled code cannot; this can.**

**Compiled code can.** `emit/protect.rs` opens protected regions, and a program
demonstrates it:

```js
const n = 5;
try { n(); } catch (e) { /* TypeError: n is not a function */ }
// and execution continues
```

So every JavaScript call in every program compiled by this engine takes the slow
door for a reason that stopped being true, and the door costs — measured, this
document and `machine-primitives.md`:

| | ns |
|---|---:|
| the machine's own indirect call, which it already lowers | **1.1** |
| a real, uninlined call in Rust on the same loop *(§7a)* | **~2** |
| `__rts_call_counted`, which every JavaScript call goes through | **~27–33** |

### Why nobody noticed, which is the part worth keeping

**The sentence is written in four places across two crates**, and `rts-core`'s
copy and `rts-codegen`'s copy are almost word for word. Neither crate may depend
on the other — that separation is the engine's central design, stated in
`CLAUDE.md` and in both READMEs — so **neither could see its own claim go stale
in the other**, and nothing owned the pair.

This is the failure mode `CLAUDE.md` opens by naming, arriving from the
direction it did not anticipate. Its first page says two answers to one question
is how a document comes to disagree with itself. Here two answers to one
question is how a *decision* outlived its reason: each copy corroborated the
other, and re-deriving it required a fact — "does anything emit a protected
region" — that lives in neither file.

Two more copies say the same thing about property operations, and comparing
them against the engine separates the real from the imagined:

| claim | today |
|---|---|
| `(5)()` answers `undefined` where the language throws | **throws, catchable** |
| `null.x` answers `undefined` where the language throws | **throws, catchable** |
| `"x" in 5` answers `false` where the language throws | **still `false`** — node throws |

Two of the three gaps closed and their documentation did not notice. The third
is real and is now a bug with no excuse in front of it: nothing prevents it, it
is simply not written, and it stayed unwritten because the reason beside it said
it could not be.

## 7a. The arithmetic is already at machine potential. The call is the whole bill.

`bench/monte_carlo_pi.ts` is a pure `f64` loop with every binding annotated
`: number`, and it is the cleanest available test of whether this engine can be
"100% proven" where nothing is polymorphic. Three versions of the same
arithmetic, differing only in shape, plus the same program in Rust:

All three RTS rows time the `while` loop alone with `performance.now()`, min of
four runs, so they are one measurement basis. *(An earlier version of this table
compared a loop timing against a whole `RTS_TIMING` `run` phase and drew a wrong
conclusion from it; the numbers below replace those.)*

| | ms for 10M iterations |
|---|---:|
| **RTS — no call, state a local** | **128.5** |
| Rust `-O`, state a local, `%` included | 211 |
| Rust `-O`, call not inlined | 252 |
| node, same local loop | 440 |
| **RTS — a call, state threaded (not captured)** | **597.4** |
| **RTS — a call, state captured** | **702.5** |

Three conclusions, and the first is the one nobody had measured:

**With no call in it, this engine beats `rustc -O` on the same loop** — 128.5 ms
against 211, same answer (`inside = 7852595`). The proven-double path is real:
`FloatArith` on unboxed `f64`, no boxing, no crossing. **Where nothing is
polymorphic, the engine already is deterministic**, and the value model is not
what is costing anything.

**The call is the bill: 468.9 ms for two per iteration — ~23.4 ns each.** That
is 3.7× the entire remaining loop, and it agrees with §1a's 26.7 from a
different program and different method.

**Capture is real but a quarter of it: 105.1 ms, ~2.6 ns per captured access.**
A module binding a function reads lives as a property of an environment object,
so each read is a `CachedGet` and each write a `CachedSet` — cheap, because the
inline cache works, but not free. Worth fixing after the call, not before.

### And a second cause, which is the calling convention itself

`rts-core`'s `Compiled` is the shape every compiled function has:

```rust
type Compiled = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;
```

**Six `u64`, and nothing else is expressible.** A function that takes one proven
double and returns one cannot say so: the argument is widened at the site,
guarded back in the callee, and the return makes the same round trip. So the
boxing this document's §6 refutation and `the-missing-pass.md` both study is not
only an artefact of a missing fold — **at a call boundary it is mandatory**,
because the convention has one shape and that shape is `Tagged`.

That is a machine-layer question in the exact sense
`crates/rts-cranelift/README.md` means: the ABI has types (`abi/` — "types,
conventions, aggregate classification, multiple returns") and the language layer
uses one row of it for everything. A convention that could say "this callee
takes an `F64` and answers an `F64`" would remove the round trip at every
monomorphic call site, and it needs no new machine capability — `Signature`
already carries a `Vec<Repr>`.

### 7b. The call, decomposed — and what each stage is worth

Ablation on `mc_call_nocapture.ts` (two calls per iteration, 10M iterations),
removing **all** of the activation bookkeeping — the class-constructor check and
the three stacks — and keeping the crossing, the resolution and the jump:

| | ms | ns per call |
|---|---:|---:|
| as it is today *(min of 4: 597.4)* | **597** | — |
| **bookkeeping ablated** *(3 runs: 453–456)* | **454** | **−7.2** |
| no call at all (the arithmetic inline) | 128.5 | −16.3 more |
| *Rust, same loop, real call* | 211 | |

The same ablation on the ladder, for cross-checking: `c.m(a)` 23.0 → **13.8**
(−9.2), `set.has(7)` 33.1 → **24.3** (−8.8). Three instruments, one figure:
**the bookkeeping is about 7-9 ns and the rest of the door is about 16.** It
also agrees with §3a's stacks-only ablation of 7.3–10.2 ns, which is the
independent check that matters.

*A correction, since the first version of this section said 12.2.* That came
from a single run of the unablated program which happened to read 697 ms;
measured properly it reads 597. The ladder disagreed with 12.2 and agreed with
7.2, which is what prompted the re-measurement — a figure that two instruments
contradict is the instrument's problem, not the engine's.

So a JavaScript call here decomposes into three pieces, and each has a different
owner:

| piece | ns | whose |
|---|---:|---|
| activation bookkeeping — class check, three stacks | **~8-9** | `rts-core` |
| the crossing, `resolve`, the jump, and boxing every operand | **~16** | the boundary and the convention |
| what the machine actually charges for a call | **1.1** | already paid |

### How to solve it

Four stages, each with the number that justifies it and each shippable on its
own. Together they take `mc_call_nocapture` from 597 ms toward the 128.5 ms the
same arithmetic costs with no call — which is already **faster than `rustc -O`**
on the identical loop.

**Stage 1 — ~~one probe instead of two~~ SHIPPED, and for a different reason
than predicted.** See §5a: the prediction was that the second *table* was the
cost. It was not — merging the tables bought nothing. What paid was folding the
check into the borrow `called` already takes.

**Stage 2 — an activation record instead of three stacks (~6-7 ns).** §3a, with
the GC-root trap priced. The ablation above says the whole bookkeeping is 8-9 ns
and stage 1 is about 2 of it.

**Stage 3 — a direct call to a statically known callee (~16 ns).** The big one,
and §7's expired sentence is what has been blocking it. `emit/inline.rs` already
proves exactly what this needs — *which* function a name denotes, that nothing
shadows it, that no other declaration spells it — and then uses that proof only
to substitute one-expression bodies. The same proof licenses
`Inst::Call { callee }` for any body, at the 1.1 ns the machine charges. A
callee that reads `arguments`, takes a rest parameter or is re-assigned keeps
the door it has now.

**Stage 4 — a convention that can say `F64` (§7a).** Even a direct call boxes
today, because `Compiled` is six `u64`. `Signature` already carries a
`Vec<Repr>`, so the machine can express `(F64) -> F64`; what is missing is the
language layer emitting it for a callee whose parameters and return are proven.
This is what removes the last of the round trip, and it is the one stage that is
a machine-layer conversation rather than a runtime one.

The ordering is deliberate: stages 1 and 2 are `rts-core` alone and need no
agreement with anyone. Stage 3 needs `add-language-node` and `add-ir-instruction`
together. Stage 4 changes a contract three crates share, and `rts-host` is where
that agreement is asserted.

---

#### The one-line version of stages 1 and 2, if only one thing is done

1. **Fold the class-constructor check into a borrow that already happens** —
   **~2 ns on every call**, measured 2026-08-28 by ablation (`c.m(a)` −10.0%,
   `Number.isInteger` −7.7%, `set.has` −5.1%, static-call control −0.3%).
   `called` takes a whole `with_current` plus a side-table probe, before any
   other work, to ask whether the callee is a class constructor called without
   `new` — a question a working program answers "no" to every time. `invoke`
   resolves the callee in a borrow of its own a moment later, and the flag can
   be read there.
2. **The three per-activation stacks — 7.3 to 10.2 ns.** §3a, with the GC-root
   trap priced. The isolated experiment says merging them recovers 2.6 of 6.
3. **A guard at the call site, and `Inst::CallIndirect`.** This is the one the
   expired sentence was blocking. Every prerequisite exists: the machine lowers
   `CallIndirect` (`lower/body.rs:602`), this crate already declares inline
   caches and uses them for property reads, and the failure path — the thing
   that was genuinely missing — is emittable now. A monomorphic site caches the
   callee's code and environment, guards on identity, and jumps; anything else
   falls through to the door that exists today.

   What it must still answer is the first reason, which did not expire: the
   guard has to prove the value is code before jumping, and a miss has to reach
   the slow path with the activation bookkeeping intact. That is a
   `add-language-node` and `add-ir-instruction` change together, not a tidy-up,
   and it is the only item on this list whose ceiling is the 1.1 ns the machine
   charges.

---

## 8. What is worth doing next, away from the call path

The call path's own list is §7's "How to solve it" and is not repeated here.
What is left is everything that is not a call:

1. **`__rts_to_boolean` as a crossing.** `if (s.has(7))` emits *three* runtime
   calls — `set_call_name` (now gone), `call_counted`, and `to_boolean` on the
   result. Truthiness of a NaN-boxed value is a handful of compares for the
   common cases; it is a call today. Unpriced, and now cheap to price: the
   machine probe resolves to about 0.02 ns.
2. **`"x" in 5` does not throw**, which §7 turns from a documented gap into an
   ordinary bug. Small, and the kind that a corpus comparison finds rather than
   a benchmark.
3. **A stack slot in the machine.** `grep StackSlot crates/rts-cranelift/src`
   returns nothing: the machine cannot hand a caller a scratch area at all, and
   `rts-core` names that absence three times as the reason its argument vector
   is a `Vec` — which is where the ~8 ns of §3a comes from. It is the one item
   here that is a machine-layer gap rather than a language-layer one, and it is
   what `functions.rs` means by "a calling convention with a stack slot".
4. Everything `action-table-2026-08-26.md` §6 already ranks: `exec`'s result
   construction (~1050 ns), `string split 16`, `binary alloc Uint8Array 64`.

---

## 9. A defect found on the way, not fixed here

**`bench/analytic.ts` intermittently loses the last 14 of its 90 rows**, and the
cause is not the harness: an in-bounds array read answers `undefined` while the
heap is under pressure. It reproduces at `b83eac1a` in about **1 run in 10** on
90 objects held in an array, iterated while the loop body allocates — through
`for`-`of` and through `arr[i]` alike, so it is the element read and not the
iterator. `CASES.length` stays 90 throughout, and a re-scan after the loop finds
every element present, so what is wrong is transient.

It is intermittent on the *baseline* binary too, at the same rate, so it is not
this change and not the 08-27 optimisation round. `object-model.md` §9's refusal
of "bound `trace::edges_of` by the shape" describes this failure mode exactly —
"a live reference is never marked, and the sweep frees it. Use-after-free, not
retention" — and is where to look first.

Read it as an instrument warning as well as a bug: **a run of `analytic.ts` that
reports `UNAVAILABLE` rows has also been under memory pressure the other rows
did not expect**, and its numbers should be discarded rather than quoted.

---

## 10. One correction this owes the tree

`roots.rs`'s classification lists `pending_call_name` under "**No references at
all**", with the reason "(an index into `literals`, not a value)". It was not an
index: `set_call_name` wrote `context.literals.get(…).copied()`, which is the
resolved encoded `Value`. Nothing leaked, because `literals` is itself a root
and holds the same word — but the stated reason was wrong, and a false sentence
about what is and is not a GC root is the most expensive kind in this crate.

After this change the hot path really does carry an index (`Spelling::Literal`),
and only the vector path still stores a value there.
