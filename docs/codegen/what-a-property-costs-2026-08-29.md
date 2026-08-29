# What a property costs, and three changes measured against it

*2026-08-29, over `a2741a34`. Every number here was produced by
`target/release/rts.exe` — a real release build, never `fast` — against a
baseline binary **built** from the same tree rather than copied out of
`target/`, on one Windows machine. The method is the repository's own: minimum
of five inside one process, alternated between binaries, stated as a pair.*

---

## 0. The instrument, and the four rows it cannot resolve

`bench/analytic.ts` was run three times against the **same** binary before
anything was changed, because a threshold that is not expressed as a multiple
of the measured spread is a number somebody invented.

| | value |
|---|---:|
| rows | 90 |
| rows whose spread exceeds 5% | 26 (29%) |
| rows whose spread exceeds 10% | 8 (9%) |
| rows whose spread exceeds 50% | **4** |
| geomean, across three identical runs | 16.28 → 17.05 (**4.7%**) |
| sum, across three identical runs | 12 734 → 13 908 (**9.2%**) |

**The four are `string template literal`, `array for-of 16`, `array filter 16`
and `array map 16`**, and the worst of them moved 288.90 → 278.39 → **1 339.62**
across three runs of one binary. Read no single-digit percentage off those four
rows, ever.

**The geomean is the aggregate to use** — 4.7% of noise against the sum's 9.2% —
and it is the one every claim below is stated against.

The baseline itself, for the record: **geomean 15.4–16.9 ns, sum 11.9–13.5 µs**
over 90 rows, depending on the day's machine state. The absolute numbers drift
by ~10% between sessions; only a pair measured on one afternoon means anything.

---

## 1. The verdict on "2x on `analytic.ts`", before anything else

**It does not come out of any list this session produced, and the reason is
arithmetic.** 89.6% of the sum lives in twenty rows, and they are `json parse`
1 751 · `json stringify` 1 564 · `regex exec+group` 1 398 · `regex replace`
1 252 · `string split 16` 1 186 · `binary alloc Uint8Array 64` 550 ·
`binary TextEncoder 16` 524 — library implementation in `rts-core`, text and
array construction. Halving the aggregate means halving those.

The two families that would move the *structural* rows are both blocked on the
same unbuilt precondition. Removing the three activation stacks (7.3–10.2 ns on
every call) and making elements addressable from the cell both need precise GC
roots, which `docs/engine/the-unwired-keystone.md` has already described and
which is months of work. That is not a new finding; it is the 2026-08-28 plan's
Stage 1, and nothing here changes it.

What is reachable without it is what this document measured: **per-operation
taxes that show up in dozens of rows at once**. Honest total for the whole
hours-effort block, estimated rather than measured: **12–25% on the geomean.**
Saying 2x would be a claim wearing a measurement's clothes.

---

## 2. What a property access costs, by where the property is

Measured over 2 × 10⁶ iterations, minimum of five, one process per row.
This is the table the rest of the document hangs off, and none of it was
written down before.

| the property is | ns |
|---|---:|
| **own, cached** | **11** |
| absent, prototype chain of 1 (`Object.create(null)`) | 39 |
| absent, chain of 2 (an object literal) | 59 |
| absent, chain of 6 (a class four deep) | 85 |
| **present, one link up, on an object literal** | **56** |
| `'nope' in o` | 55 |
| `o.hasOwnProperty('nope')` | 161 |

**Two facts, and both are worth more than the numbers.**

**A miss is ~28 ns of fixed cost plus ~11 ns per prototype link.** The slope is
readable off the three absent rows (39 / 59 / 85 at 1 / 2 / 6 links) and it is
the chain walk in `accessor::resolve`.

**An inherited property read on an object literal is not cached at all** — 56 ns
against 11 for an own one, five times. It is not that the cache is cold; the
resolver is entered and refused on *every execution*, forever. `cache.rs`
already explains why for its own reasons: `inherited_from` substitutes a
prototype by KIND for arrays, callables, text and plain objects, so a site
cached against one would recognise all the others. The refusal is correct; what
is missing is that the site is never told the walk cannot serve it, which
`cache_resolve_indirect` next door already does with a `REFUSED` marker worth a
measured 2x on a depth-2 method call.

`instanceof` is the same table read twice: it costs **119 ns**, and a single
absent-property read on the same constructor costs **118**. The operator's whole
price is the `Symbol.hasInstance` lookup that misses — a class that *defines*
`Symbol.hasInstance` runs the operator in **57 ns**. Chain length barely enters
it (112 at depth 1, 120 at depth 4), which is what says the walk is not where
the time goes.

### 2a. The keyed cache holds exactly one key

`o[k]` where `k` is a variable costs **5.5 ns and does not scale with the key's
length** — 1, 4 and 29 characters all read 5.5 — which is the shipped
`CachedGetKeyed` doing its job. Rotate the key and it collapses:

| | ns |
|---|---:|
| one key cell | 16.5 |
| **two** distinct key texts | **36.0** |
| four distinct key texts | 35.5 |
| *(control: the array read alone)* | 15.5 |

**Two keys is already a total miss**, not a partial one. A second entry is the
same change that took the shape cache from 0% to 99.99% hit; it would not move
`analytic.ts`, whose only computed-key row rotates four, and it is recorded here
so that is a decision rather than an oversight.

And the miss is the RESOLUTION, not the interning: four distinct string cells
holding the same text all hit, because a literal is interned once by the
emitter and the four slots hold one cell.

---

## 3. The IR, counted rather than described

`rts ir` over `bench/analytic.ts`, 4 346 blocks:

| | count | share |
|---|---:|---:|
| `Guard` | 794 | |
| …of which re-guard a value already guarded, same expectation, same function | **276** | **34.8%** |
| blocks whose entire body is one `Jump` | **770** | **17.7%** |
| …of which forward arguments | 740 | |
| `CachedGet*` | 357 | |
| `Widen` | 438 | |

Blocks per statement, isolated by emitting *n* copies of one statement and
differencing:

| statement | blocks |
|---|---:|
| `z = z + 1.0` (proven `f64`) | **0** |
| `z += q[0]` | 6 |
| `z += q.a` | **9** |

The nine are: guard the receiver, the cached read, the slow-path call, its
throw check, the join, a trampoline, the shared throw block, guard the loaded
value, and the generic-add fallback. **Four of the nine per access are pure
forwarding.** Whether that costs anything at run time is not established here —
a trampoline is also what a critical-edge split produces, so Cranelift may be
building it anyway; it is recorded as a compile-time fact with the run-time
question open.

The 276 redundant guards are the readable half. A function parameter guarded
`Ref(Opaque)` for `q.a` is guarded again for the next `q.a`, because the proof
does not survive the join between the cache's hit and miss edges — which is
`emit/proven.rs` losing at a merge, the item the 2026-08-28 plan calls Stage 5.

---

## 4. Shipped, with what each was worth

Four changes, each measured against a baseline **built** from the commit
before it, each gated on `rts test` producing a **byte-identical failing set**
(758 passed, 61 failed, 3 059/3 123 throughout) and on `cargo test --profile
fast -p rts-core` (297 passed, 0 failed).

*A note on the baselines, because it is the exact trap this repository has
already paid for once: every one of them was BUILT, never copied out of
`target/`. Four documentation commits arrived from CI part-way through the
session — two parity badges and two benchmark tables — and they touched
`README.md` and `.github/cross_runtime_report.json` and nothing else, so no
code moved under any measurement here. That was checked rather than assumed.*

**`eedde0c9` — a read took three borrows and resolved its key in each.**
`get_property` now takes one, the shape `set_property` already had. Isolated:
−1.7% to −5.3% on four miss rows. **Table: −0.04% geomean. Zero.**

That zero is the result. The plan's own falsifier for this item said "if it
does not move the 66 ns, the cost is in `accessor::resolve`'s chain walk
instead". It moved 2–4 ns of 56–85. **So the chain walk is the cost**, §2 above
is where to look, and the bookkeeping half of that item is closed.

**`acf1e505` — every array a native produced was filled with holes first.**
`array_proto::built` (nineteen call sites) and `iterate` each called
`array_new(n)`, which fills `vec![hole; n]`, then assigned the real values over
it and called `set_length` a second time. Both now use `built_in_rooted`, which
exists for exactly that transfer. Eleven rows measured, **eleven moved and none
the wrong way**: `spread` −14.3%, `slice 16` −10.5%, `map 1` −8.4%, `map 64`
−7.5%, `filter 16` −6.6%. Table: **sum −1.64%, geomean −0.52%**, with the rows
the mechanism names moving as it says (`array map 16` −8.3%, `array for-of 16`
−7.3%).

**`889351d7` — two questions asked whose answers were in hand.** `put` asked
the same `refuses_key_write` twice with the same arguments; the cache
diagnostics ran a modulo before reading the `OnceLock` that gates them. Table:
**sum −0.61%, geomean −0.46%, two rows better and NONE worse**.

**`705d9885` — `instanceof` was one failed lookup.** §2 above is the finding;
the change is that step 1 of the operator PROBES for `Symbol.hasInstance` with
`accessor::resolve` inside a borrow it was taking anyway, and reaches
`computed::get_indexed` only for the three shapes that can run user code — a
proxy, a getter, and a callee with no cell. `@@hasInstance` also joins
`CACHED_KEYS`, which is the whole of the 41.5% on the row where a hook exists.
Every arm moved 17–24% with the three controls flat, and the table agrees:
**sum −2.37%, geomean −1.32%**, `prop instanceof` −23.8%.

### 4a. What the four are worth together, measured rather than composed

Multiplying the four deltas gives −2.3% on the geomean. **Measuring the two
ends directly gives −0.62%**, and the direct number is the one to quote:
composing four pairs multiplies four optimistic draws, and this instrument
carries 4.7% of noise on the geomean.

`eedde0c9` → `705d9885`, four runs per binary in both orders, one afternoon,
one machine state:

| | |
|---|---:|
| sum | 11 995 → 11 807 (**−1.57%**) |
| geomean | 15.471 → 15.375 (**−0.62%**) |
| rows >8% better | 3 |
| rows >8% worse | 1 |

**So the aggregate is inside the noise, and the dedicated probes are what
carry the claim** — eleven of eleven array-producing operations 3–14% faster,
every `instanceof` arm 17–24% faster, with controls flat in both. A table with
90 rows where four are unusable cannot resolve a 1% aggregate, and saying it
does would be the same error as reading a single-digit percentage off `string
template literal`.

`coll Map.get` reads +16.2% here and `string indexOf 256` read +8.6%, −12.1%
and −10.5% across the four comparisons. Neither is reachable from anything
changed. They are the layout noise this document keeps naming, and they are
named again rather than netted away.

---

## 5. Refuted, and it is the most useful thing here

**Merging `pending_arguments` and `pending_counts` into one activation record.**
Implemented, measured, reverted; the full account is
`native-call-floor.md §3a-i`. The isolated call ladder made it look like a
1 ns win on five rows with the control flat. `analytic.ts` returned **+2.2% on
the geomean, nine rows more than 8% worse and none better**, reproducible with
the measurement order reversed so drift could not be the cause.

The rows that got worse cannot touch an activation stack — `string concat 2`,
`regex exec+group`, `array for-of 16`. That is the third independent negative on
rearranging this crate's call path, after `action-table-2026-08-26.md §4` and
`native-call-floor.md §5b`, and the three together say something the individual
refutations did not:

> **On `rts-core`'s hot paths, prefer a change that REMOVES work to one that
> REARRANGES it.** The three changes measured on this one afternoon split
> exactly along that line. The rearrangement cost 2.2%. The two deletions cost
> nothing anywhere and paid where their mechanism said they would.

---

## 6. Two corrections this session owes the tree

**`SetCallName` as an operand is DONE**, and three documents still rank it as
the top open item. `emit/call.rs::issue` passes `expr::name_constant` as an
operand of `RuntimeOp::Call`; `functions::call_counted` takes it as an `i64`
and `Spelling::Literal` defers reading the literal table to the branch that
raises. It shipped in `5f437eff` and `native-call-floor.md §4` records it.
`action-table-2026-08-26.md §6` item 3 and the 2026-08-28 plan's item 2c(iii)
were true when written and are not now. `RuntimeOp::SetCallName` survives for
one caller — `CallWithArgs`, which has no operand to carry a name.

**`well_known` memoises fourteen names, not six.** The 2026-08-28 plan's item 2b
says six; `context.rs` lists `length`, `prototype`, `byteLength`, `byteOffset`,
`buffer`, `toJSON`, `name`, `constructor`, `@@species`, `value`, `done`,
`@@iterator`, `next`, `return`. `well_known_text` memoises three. The item is
still open for names outside those lists — `@@toStringTag` is a full intern miss
on every activation that builds an `arguments` object — but the count is wrong
wherever it is quoted.

---

## 7. What is left, ranked, and what it is not

Produced by seventeen agents — eight reading one subsystem each, eight
adversarially trying to refute what the first eight found, one synthesising —
with the repository's own refutation lists (`plan.md §9`, the 2026-08-28 plan's
§5) handed to the verifiers as grounds for rejection. Twenty-five findings
survived, nineteen were refuted or had their mechanism corrected.

**Hours of work, and none can give a silently wrong answer:**

1. **`iterate` and `array_proto::built`** — shipped above as `acf1e505`.
2. **`put`'s duplicated refusal** — shipped above as `889351d7`.
3. **A `switch` is a linear chain, and on an unproven discriminant every link
   is a runtime crossing.** Read out of `rts ir` here rather than taken on
   report: an eight-case `switch` over an `any` emits **seven
   `__rts_strict_equals` call sites and zero `Compare`**; the same eight cases
   over `i & 7` emit **seven `Compare(Eq)` and no call at all**. So there is no
   jump table in either shape — the difference is only whether each link costs
   a crossing or an instruction.

   `flow switch 8-way` therefore does **not** move: its `i & 7` is already on
   the cheap side, which is why the table shows 3.25 ns and says nothing about
   the case that matters. The gain is entirely off the table; the agent that
   found it measured 5.2–5.7 ns per case tested with a ~17 ns intercept, and
   that figure is theirs rather than re-measured here. 34 files and 318 `case`
   labels in the corpus.

   The fix is to guard the subject for `F64` **once** before the chain, keeping
   today's chain on the guard's failure edge — which `expr.rs:1313-1317`
   already names in a comment. A jump table is a separate, machine-layer
   change and is not this item.
4. **`arguments_object` mints a region cell and two `String`s per activation**
   of every function that mentions `arguments`, including one for the constant
   text `"Arguments"` — the same mistake `closure_new` already fixed and
   recorded. Not on any `analytic.ts` row.
5. **`well_known`'s memo list**, per §6.
6. **160 of the 438 `Widen` in `analytic.ts` have a `Const` operand**, all
   `F64`, measured at 0.47 ns each and invisible to Cranelift's egraph. Folding
   them requires teaching `fold::guard_answer` and `widened_source` to see
   through `Const(Tagged)` in the same change, or a guard decided at compile
   time becomes a real one — a regression the original finding did not mention.

**Days of work, large, still no silent wrong answer:**

7. **An inherited data read never arms a cache** (§2). The machine already has
   the terminator — `CachedGetIndirect` is in production for callees and carries
   the `REFUSED` marker. The risk is not correctness, it is taxing every OWN
   read by a load and a compare on the hottest path in the engine, and that has
   to be measured rather than assumed.
8. **The `for-of` prologue allocates a throwaway `[]` and walks two prototype
   chains before the first element** — 203 ns per loop ENTRY, a figure
   `entry/pattern.rs` already carries from this tree, of which 66 ns is an array
   allocated only to read a method off it. A nested `for-of` pays it per outer
   iteration. `pattern.rs::direct` answers the same question in one crossing.
   **This one CAN answer wrongly** and needs a clause the finding did not have:
   the cell's prototype must BE `array_prototype`, or a subclass carrying its
   own `@@iterator` takes the array arm.
9. **A cached store can never reach an object's overflow**, so every write to a
   15th-or-later property is a full resolver plus `set_property`, forever.
   Measured: on an object grown to 20 properties by assignment, `o.p1 = i` is
   2 ns and `o.p16 = i` is **72**, with the census reporting one refused
   resolver entry per write. A class with 20 fields pays twelve refused entries
   per `new`. `lower_cached_set` needs the word-2 indirection `lower_cached_get`
   already has — and that word is contested with item 7, so the two are one
   word budget and not two independent changes.
10. **`Math.floor(x)` de-proves the whole accumulator**: `proven::is_numeric`
    has no arm for a call, so one `Math` call in a loop turns the block header
    from `(F64, F64)` into `(Tagged, F64)` and every operand after it into a
    guard. `bench/monte_carlo_pi.ts` spends 705 of 790 ms in exactly that loop.

**Found and not fixed**: `for (const x of aSet)` **inside a function** throws
`TypeError: __rts_of_it.next is not a function`. It reproduces on `a2741a34`,
so it predates everything here. At top level the same loop runs.

And a defect this document inherits rather than creates, **reproduced here
rather than taken on report**: 5 000 `JSON.parse` calls over a 120-key object
**segfault** — exit 139, `target/release/rts.exe` at `a2741a34`. A single parse
of the same text is correct, and 5 000 parses of a small object are correct.
Anything that moves more objects onto the region's spanning path should fix
that first.

```js
let src = "{"; for (let i = 0; i < 120; i++) { if (i) src += ","; src += '"k' + i + '":' + i; } src += "}";
for (let i = 0; i < 5000; i++) JSON.parse(src);
```

That is a crash, so by the honesty floor it is not a passing anything, and it
is written here because no row of `bench/analytic.ts` would ever have found it:
every object in that file has fewer than fifteen members.
