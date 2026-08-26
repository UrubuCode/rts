# What every action costs, 2026-08-26

The table of record. Every other document in this tree points at a row of it,
and a claim that something got faster is a claim that a row here moved.

**How it was produced.** `bench/analytic.ts`, one file, no imports, run under
three runtimes on one machine within the same ten minutes:

| | binary | invocation |
|---|---|---|
| rts | `target/release/rts.exe`, built from the tree at `673b9c0c` | `rts.exe run bench/analytic.ts` |
| bun | 1.4.0 | `bun run bench/analytic.ts` |
| node | 25.9.0 | `node bench/analytic.ts` |

Machine: Windows 11 Pro 26200, the same one for all three, **with nothing else
running** — see below for why that clause is now part of the method.

**Three runs of each runtime, minimum per row**, which is stricter than the
2026-08-21 table and is stated because it makes the two not quite comparable:
that one took the harness's own calibrated repeats once. The change is
deliberate. This session watched a loaded machine move rows by 20% and, in one
case, report 81 cross-runtime fixtures as regressions that were timeouts — so a
single run is no longer trusted for a table other documents subtract from.

`node` is invoked without `--experimental-strip-types`; 25.9 reads TypeScript
without it, and the flag now warns.

### What moved since 2026-08-21, and why the old numbers were not wrong

The previous table was taken at `97f66385` and `docs/codegen/object-model.md` §6
item 0 called re-measuring it the first item on the work list, because every
subtraction in that document is arithmetic on these rows and two of them were
known stale. They were, and by more than it estimated:

| row | 2026-08-21 | object-model's estimate | now |
|---|---:|---:|---:|
| `prop write own` | 9.61 | ~4.59 | **4.02** |
| `prop read own` | 4.97 | — | **4.14** |
| `alloc class instance` | 90.89 | ~86 | **74.55** |
| `alloc add prop after` | 54.12 | ~44.5 | **39.60** |

The first is a write barrier that could never report anything, removed in
`hot-path-hygiene.md`; the allocation rows contain it once and twice. The rest of
the fall is five days of work between the two commits, including this session's
four changes to `closure_new`, native arguments, enumeration and the transition
key.

**Read this against rule 1 below rather than as a speedup**: these are two
absolutes taken on two different days, which is exactly the comparison that
section says is evidence of nothing. What licenses the direction here is that
each change was A/B'd against its own kept binary when it landed, and those are
the numbers in the commits.

### An absolute here is not comparable across days. Only a same-session A/B is.

This was tested rather than assumed, on 2026-08-22, and the test is worth
repeating before anyone reads a row as a regression.

`alloc class instance` re-measured at **108–110 ns** against the 90.89 in the
table below — a 20% rise, on a tree whose only changes since were meant to make
allocation *cheaper*. That reads as a regression, and there was a plausible
mechanism for it: the region stopped pre-touching its first 8 MiB when the
zero-fill became `alloc_zeroed` (`startup.md`), so a program that allocates could
be paying page faults it used to have paid for at startup.

**It is not a regression, and that mechanism is not happening.** The same loop,
run isolated, alternated, same session:

```
target/baseline.exe  (97f66385)   112.41   103.57   106.37
HEAD                              104.99   103.11    98.51
```

The tree that was supposed to have regressed is **faster**. The 90.89 and the
108 are the same code on a different day.

Two rules follow, and they bind every number in this tree:

1. **Never compare an absolute across sessions.** Machine state moves rows by
   20% — more than most of the optimisations in `plan.md` are worth. A row that
   looks worse than a table from last week is evidence of nothing.
2. **Always keep the other binary.** `cargo build --release && cp
   target/release/rts.exe target/baseline.exe` before the first edit. The A/B
   above cost one minute and settled in three runs what a week of comparing
   against a stale table could not have settled at all.

---

## Read this before reading the table

**These are not three measurements of the same thing.** `bun` and `node` are
tracing JIT compilers measured on a loop that runs tens of millions of times,
which is the single best case they have: an action whose result is unused and
whose operands never change gets constant-folded, hoisted or deleted outright
after a few thousand iterations. Several `~0.3 ns` cells below are not "V8 is
fast at instanceof" — they are "V8 removed the instanceof".

So the ratio column is an **upper bound on the gap**, not the gap. What it is
honestly good for is direction and magnitude: a 3× is noise plus a real
difference, a 30× is a missing mechanism, and a 500× is an operation going
somewhere it should not be going at all. Those three bands are what this table
is used for, and where a document needs the true gap it re-measures against a
case the JIT cannot fold.

**And `analytic.ts` is an attribution instrument, not a program.** Its own header
says so: one action in a loop with its operands already in hand, the best case
for caches and the worst case for representativeness. Nothing here says how much
of any real program a row is. That is what `bench/objbench.ts`,
`bench/monte_carlo_pi.ts` and `bench/pi_machin.ts` are for, and rule 4 of this
tree's README requires one of them to move before a change ships on a row here.

**The floor.** The empty-loop row is what the harness itself costs, and a row at
the floor is a row whose cost the harness dominates — not a free action. rts's
floor is 0.89 ns, bun's 0.45, node's 1.31. Subtract before believing a small
number.

---

## The table

Sorted as `analytic.ts` emits them. The last column is rts against whichever of
bun and node was faster.

| action | rts | bun | node | rts / best |
|---|---:|---:|---:|---:|
| `floor empty loop` | 0.89 | 0.45 | 1.31 | 2× |
| `arith int add` | 1.14 | 0.45 | 0.36 | 3× |
| `arith int mul` | 3.74 | 0.67 | 0.25 | 15× |
| `arith int div` | 2.03 | 0.63 | 0.72 | 3× |
| `arith int mod` | 4.78 | 0.84 | 1.13 | 6× |
| `arith int shl` | 0.82 | 0.22 | 0.25 | 4× |
| `arith int shr` | 0.90 | 0.44 | 0.25 | 4× |
| `arith int shr unsigned` | 12.55 | 0.22 | 0.25 | 57× |
| `arith float add` | 0.90 | 0.68 | 0.67 | 1× |
| `arith float mul` | 0.90 | 0.66 | 0.67 | 1× |
| `arith compare int` | 1.33 | 0.45 | 0.36 | 4× |
| `arith int sub` | 3.62 | 0.23 | 0.25 | 16× |
| `arith float sub` | 1.11 | 0.67 | 0.67 | 2× |
| `arith float div` | 3.07 | 2.98 | 2.98 | 1× |
| `arith exponent` | 19.09 | 0.22 | 0.25 | 87× |
| `arith int and` | 0.82 | 0.23 | 0.25 | 4× |
| `arith int or` | 0.90 | 0.23 | 0.25 | 4× |
| `arith int xor` | 0.90 | 0.23 | 0.25 | 4× |
| `arith int not` | 1.14 | 0.23 | 0.25 | 5× |
| `arith negate` | 0.91 | 0.45 | 0.25 | 4× |
| `arith unary plus` | 3.72 | 0.44 | 0.25 | 15× |
| `arith logical not` | 8.06 | 0.69 | 0.56 | 14× |
| `arith strict equals int` | 1.36 | 0.67 | 0.39 | 3× |
| `arith loose equals int` | 1.58 | 0.68 | 0.39 | 4× |
| `arith Math.sqrt` | 2.93 | 1.87 | 1.89 | 2× |
| `arith Math.floor` | 2.93 | 1.18 | 0.66 | 4× |
| `arith Math.random` | 4.07 | 1.03 | 5.77 | 4× |
| `call free function` | 2.95 | 0.45 | 0.37 | 8× |
| `call arrow` | 2.89 | 0.45 | 0.36 | 8× |
| `call method` | 25.39 | 0.34 | 0.36 | 75× |
| `call varargs 3` | 209.47 | 0.45 | 0.36 | 582× |
| `call closure make+call` | 241.27 | 6.33 | 6.92 | 38× |
| `call closure var read` | 23.29 | 0.57 | 0.40 | 58× |
| `prop read own` | 4.14 | 0.45 | 0.37 | 11× |
| `prop write own` | 4.02 | 0.45 | 0.28 | 14× |
| `prop read 4 fields` | 3.85 | 0.14 | 0.17 | 27× |
| `prop computed key` | 36.12 | 2.07 | 6.71 | 17× |
| `prop proto method call` | 25.62 | 0.34 | 0.37 | 75× |
| `prop optional chain` | 3.29 | 0.67 | 0.55 | 6× |
| `prop in operator` | 32.35 | 0.22 | 0.36 | 147× |
| `prop typeof` | 20.42 | 0.45 | 0.36 | 57× |
| `prop typeof alone` | 10.68 | 0.22 | 0.36 | 49× |
| `prop instanceof` | 116.69 | 0.22 | 0.37 | 530× |
| `alloc object literal 2` | 1.03 | 0.44 | 0.56 | 2× |
| `alloc object literal 8` | 0.90 | 0.43 | 0.57 | 2× |
| `alloc class instance` | 74.55 | 0.45 | 0.36 | 207× |
| `alloc array literal 4` | 212.17 | 0.44 | 0.57 | 482× |
| `alloc add prop after` | 39.60 | 0.43 | 0.56 | 92× |
| `array index read` | 14.09 | 0.57 | 0.68 | 25× |
| `array index write` | 14.29 | 0.45 | 0.45 | 32× |
| `array push+pop` | 109.54 | 0.57 | 0.81 | 192× |
| `array for-of 16` | 44.79 | 1.08 | 0.87 | 51× |
| `array map 16` | 90.60 | 1.96 | 1.83 | 50× |
| `array filter 16` | 90.42 | 2.79 | 2.12 | 43× |
| `array indexOf 16` | 5.30 | 0.26 | 0.71 | 20× |
| `array join 16` | 99.75 | 13.59 | 11.82 | 8× |
| `string length` | 4.22 | 0.44 | 0.37 | 11× |
| `string charCodeAt` | 77.44 | 0.67 | 1.37 | 116× |
| `string index []` | 105.37 | 0.98 | 0.47 | 224× |
| `string concat 2` | 109.02 | 0.45 | 0.36 | 303× |
| `string template literal` | 391.29 | 27.28 | 14.77 | 26× |
| `string equals` | 10.10 | 0.22 | 0.37 | 46× |
| `string indexOf 256` | 197.18 | 0.45 | 0.37 | 533× |
| `string slice 16` | 183.45 | 0.44 | 0.36 | 510× |
| `string split 16` | 1327.33 | 8.82 | 33.36 | 150× |
| `string toUpperCase 16` | 184.20 | 0.34 | 0.37 | 542× |
| `string number->string` | 138.49 | 25.06 | 17.70 | 8× |
| `string parseInt` | 127.57 | 0.34 | 0.59 | 375× |
| `string parseFloat` | 135.38 | 17.75 | 33.43 | 8× |
| `coll Map.get` | 45.85 | 0.34 | 8.95 | 135× |
| `coll Map.set existing` | 57.83 | 7.54 | 9.69 | 8× |
| `coll Map.has` | 47.08 | 0.46 | 8.48 | 102× |
| `coll Set.has` | 37.77 | 0.45 | 2.71 | 84× |
| `coll Object.keys 4` | 166.87 | 0.11 | 1.96 | 1517× |
| `json stringify small` | 1848.36 | 94.79 | 132.92 | 19× |
| `json parse small` | 2279.90 | 193.88 | 409.20 | 12× |
| `regex test` | 106.70 | 1.31 | 17.39 | 81× |
| `regex exec+group` | 1552.64 | 7.85 | 31.87 | 198× |
| `regex replace` | 1167.52 | 0.22 | 33.69 | 5307× |
| `binary Uint8Array read` | 20.61 | 0.45 | 0.49 | 46× |
| `binary Uint8Array write` | 27.09 | 0.45 | 0.43 | 63× |
| `binary Float64Array rw` | 23.89 | 0.33 | 1.83 | 72× |
| `binary DataView getU32` | 34.77 | 0.47 | 0.56 | 74× |
| `binary alloc Uint8Array 64` | 599.58 | 8.26 | 27.00 | 73× |
| `binary subarray 64` | 232.41 | 28.68 | 21.84 | 11× |
| `binary TextEncoder 16` | 684.77 | 26.31 | 309.57 | 26× |
| `flow try/catch no throw` | 3.87 | 0.22 | 0.37 | 18× |
| `flow throw+catch` | 1060.43 | 388.36 | 7835.42 | 3× |
| `flow generator next` | 408.36 | 13.23 | 14.05 | 31× |
| `flow switch 8-way` | 2.98 | 1.73 | 1.81 | 2× |

---

## The shapes in it

Four patterns, and each one is a different kind of work. Reading the table row by
row hides them, which is why they are named here.

### The ~200 ns cluster in strings

`indexOf 256` 207.45, `slice 16` 206.59, `toUpperCase 16` 202.29. Three
operations doing wildly different amounts of work — a 256-byte scan, a 16-byte
copy, a case fold — landing within 3% of each other. **That is not the cost of
any of them.** It is a fixed per-call cost that dominates all three, and the
number to chase is the 200, not the difference between them.

`concat 2` at 141.88, `parseInt` at 138.48 and `number->string` at 154.76 sit
just below it, which is consistent with the same floor minus whatever the
string-in half of it does not apply to.

### The ~16–30 ns band on things that should be one instruction

`array index read` 16.63, `Uint8Array read` 24.65, `call method` 29.35,
`closure var read` 26.00, `typeof` 22.79, `Float64Array rw` 27.66. Every one of
these is an operation a machine does with a load and a compare, and every one of
them lands in the same band.

The obvious hypothesis — that they share the fixed cost of crossing into the
runtime — was tested and is **wrong**: see `entry-tax.md`. Reaching the context
costs 1.18 ns. The band is what the entry points *do*.

### Two call rows that are not measuring a call

`call free function` 3.21 and `call arrow` 3.11, against `call method` 29.35 for
the same shape of work. That is not "a method call is nine times a function
call". `crates/rts-codegen/src/emit/inline.rs` replaces a call by its body when
the callee is one expression with no `this`, no captures and no recursion, and
`analytic.ts`'s `freeFn` and `arrowFn` are both exactly that. **Those two rows
measure the inliner working; they do not measure a call.**

That file's own header records what a call costs when it is not inlined: 20.7 ns
for `function f(x) { return x + 1 }`. So the number to read as "what a call costs
here" is `call method`'s 29.35, and the two 3 ns rows are the size of the prize
for whatever else can be inlined — not evidence that calls are cheap.

`rts ir` on a call that is not inlined shows three runtime crossings per call:
`__rts_get_property` to read the callee out of the enclosing scope object,
`__rts_set_call_name` to record its spelling for a possible `TypeError`, and
`__rts_call_counted` to make the call — each followed by a throw check.

### The two rows that are already at the floor

`alloc object literal 2` at 1.03 and `alloc object literal 8` at 0.90, against a
floor of 0.89. The allocation is **gone** — `crates/rts-codegen/src/emit/escape.rs`
removed it. Which makes the interesting number not these but their neighbours:
`alloc class instance` at 74.55 and `alloc array literal 4` at 212.17, the same
shape of code with the same fate available and not taken.

### `string split 16` is bimodal and must not be quoted at all

Six runs of the same file, three per binary, alternated:

```
base:  1040.75   1054.03   9304.96
new:   8348.49   1098.83   1423.17
```

**A ninefold spread within one binary.** The row lands near 1 050 most of the
time and near 8 500–9 300 sometimes, and both binaries do both. The 4 799.01 in
the table above is a single draw from that distribution and is not a
measurement of anything.

This matters beyond the row, because two conclusions were drawn from single
draws of it and both are void: a −59% "improvement" from one comparison, and a
2.7× "unrecorded regression" against the 1 755 ns that
`crates/rts-core/src/entry/string/split.rs` recorded on 2026-08-13 — which sits
comfortably inside the low mode, so there may be no regression to bisect at all.

What causes the bimodality is not established here. The harness grows `n` until
a case takes 40 ms (`analytic.ts:585`), so a case near a threshold can land at
two very different counts, and `split` allocates heavily enough to make a
collection's timing part of the result.

**Rule for this row until it is fixed: report the median of at least three runs,
or do not report it.** The same caution applies to any row whose runs disagree
by more than a few per cent — check the spread before believing a delta.

### `regex replace` at 3006×

The largest ratio in the table, and the one most likely to be a JIT artefact
rather than a gap — bun's 0.50 ns for a regex replace producing a new string is
below what allocating the result can cost, so bun deleted the call. Treat this
row as "unmeasured against bun", and use node's 38.75 as the comparison until
someone writes a case whose result is consumed.

This is exactly what the warning at the top of this file is for. It is named here
rather than quietly dropped because a 3006× in a table is the number someone
will quote.

---

## Startup

A separate question from every row above, and measured separately.
PowerShell `Measure-Command`, minimum of 12 runs, same machine, same day.

| | min | what it is |
|---|---:|---|
| `cmd /c exit` | 11.2 ms | what PowerShell costs to spawn *anything*. The floor of this instrument, not of a process. |
| `rts.exe` rejecting a bad flag | 12.8 ms | process load: mapping a 33 MB image, imports, relocations, CRT and TLS init. **~1.6 ms over the floor.** |
| `rts run empty.ts` | 19.9 ms | the whole thing |
| `rts run hello.ts` | 20.4 ms | one `console.log` |
| `bun -e ''` | 8.5 ms | below the `cmd` floor — so the floor is an artefact of `cmd.exe`, not a bound on a process |
| `node -e ''` | 24.4 ms | |

`bun` coming in under `cmd /c exit` is the important line in that table: it means
**~20 ms is not a lower bound imposed by Windows**, and the 8 ms between rts's
process load and its finished empty program is real, attributable work.

Where that work goes, from `RTS_TIMING=1 rts run empty.ts`:

| phase | ms |
|---|---:|
| `seed-context` | **4.380** |
| ↳ `install-node` | 1.809 |
| ↳ `install-std` | 0.909 |
| ↳ `install-dom` | 0.067 |
| ↳ `install-physics` | 0.008 |
| `place` | 0.599 |
| `front-end` | 0.871 |
| `lower+compile` | 0.391 |
| `emit` | 0.064 |
| `prepare` + `plan` + `define` | 0.051 |
| **`run` (the total these sit inside)** | **5.101** |

Two things follow. **`seed-context` is 86% of `run`** — building the built-in
world costs seventeen times what compiling and placing the program costs. And
`19.9 − 12.8 − 5.1 ≈ 2.0 ms` is spent between the process being loaded and `run`
starting, which nothing currently times.

---

## Re-measuring this

```bash
cargo build --release
./target/release/rts.exe run bench/analytic.ts   > rts.txt
bun run bench/analytic.ts                        > bun.txt
node --experimental-strip-types bench/analytic.ts > node.txt
RTS_TIMING=1 ./target/release/rts.exe run <(echo '') 
```

Run the three within the same session — a machine that has been busy reports
different numbers, and the ratio between columns is the part that survives that.

When a row here moves, **update it and say what moved it**. A table dated
2026-08-21 sitting beside code from October is the stale document
`docs/README.md` rule 1 is about, and this one is worse than most, because
everything in this tree is measured against it.
