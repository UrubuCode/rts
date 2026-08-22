# What every action costs, 2026-08-21

The table of record. Every other document in this tree points at a row of it,
and a claim that something got faster is a claim that a row here moved.

**How it was produced.** `bench/analytic.ts`, one file, no imports, run under
three runtimes on one machine within the same ten minutes:

| | binary | invocation |
|---|---|---|
| rts | `target/release/rts.exe`, built from the tree at `97f66385` | `rts.exe run bench/analytic.ts` |
| bun | 1.4.0 | `bun run bench/analytic.ts` |
| node | 25.9.0 | `node --experimental-strip-types bench/analytic.ts` |

Machine: Windows 11 Pro 26200, the same one for all three. Numbers are
nanoseconds per action, best of the harness's own calibrated repeats.

**`97f66385` is a parent of this branch, not its tip.** These numbers were taken
against that commit, and against the changes that now sit on top of it; the
branch has since been rebased onto ninety commits of `rts-dom` and parity work
which touch none of the crates measured here. The *attribution* therefore stands
— it is a before/after of one change set, measured with both binaries in hand —
but the absolute figures describe a tree that no longer exists exactly.
Re-measure before quoting an absolute number.

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
floor is 1.27 ns, bun's 0.46, node's 1.37. Subtract before believing a small
number.

---

## The table

Sorted as `analytic.ts` emits them. The last column is rts against whichever of
bun and node was faster.

| action | rts | bun | node | rts / best |
|---|---:|---:|---:|---:|
| `floor empty loop` | 1.27 | 0.46 | 1.37 | 3× |
| `arith int add` | 1.22 | 0.49 | 0.38 | 3× |
| `arith int mul` | 7.55 | 0.72 | 0.26 | 29× |
| `arith int div` | 2.62 | 0.68 | 0.77 | 4× |
| `arith int mod` | 5.23 | 0.94 | 1.16 | 6× |
| `arith float add` | 1.22 | 0.70 | 0.70 | 2× |
| `arith float mul` | 1.29 | 0.71 | 0.70 | 2× |
| `arith compare int` | 2.08 | 0.51 | 0.38 | 5× |
| `arith Math.sqrt` | 3.18 | 2.03 | 1.99 | 2× |
| `arith Math.floor` | 3.29 | 1.34 | 3.79 | 2× |
| `arith Math.random` | 5.01 | 1.13 | 6.30 | 4× |
| `call free function` | 3.21 | 0.38 | 0.39 | 8× |
| `call arrow` | 3.11 | 0.55 | 0.39 | 8× |
| `call method` | 29.35 | 0.52 | 0.38 | 77× |
| `call varargs 3` | 253.36 | 0.55 | 0.38 | 667× |
| `call closure make+call` | 1672.46 | 10.25 | 8.40 | 199× |
| `call closure var read` | 26.00 | 0.67 | 0.40 | 65× |
| `prop read own` | 4.97 | 0.54 | 0.38 | 13× |
| `prop write own` | 9.61 | 0.31 | 0.30 | 32× |
| `prop read 4 fields` | 4.18 | 0.17 | 0.18 | 25× |
| `prop computed key` | 107.86 | 2.33 | 7.53 | 46× |
| `prop proto method call` | 27.79 | 0.66 | 0.38 | 73× |
| `prop optional chain` | 4.13 | 0.76 | 0.57 | 7× |
| `prop in operator` | 85.53 | 0.26 | 0.38 | 329× |
| `prop typeof` | 22.79 | 0.54 | 0.38 | 60× |
| `prop typeof alone` | 13.10 | 0.26 | 0.38 | 50× |
| `prop instanceof` | 240.80 | 0.30 | 0.37 | 803× |
| `alloc object literal 2` | 1.22 | 0.47 | 0.61 | 3× |
| `alloc object literal 8` | 1.24 | 0.50 | 0.62 | 2× |
| `alloc class instance` | 90.89 | 0.53 | 0.38 | 239× |
| `alloc array literal 4` | 231.34 | 0.52 | 0.62 | 445× |
| `alloc add prop after` | 54.12 | 0.48 | 0.61 | 113× |
| `array index read` | 16.63 | 0.71 | 0.70 | 24× |
| `array index write` | 16.88 | 0.52 | 0.47 | 36× |
| `array push+pop` | 148.78 | 0.70 | 0.86 | 213× |
| `array for-of 16` | 55.95 | 1.26 | 3.73 | 44× |
| `array map 16` | 220.89 | 2.38 | 1.75 | 126× |
| `array filter 16` | 223.51 | 3.86 | 2.28 | 98× |
| `array indexOf 16` | 6.58 | 0.54 | 0.74 | 12× |
| `array join 16` | 151.56 | 15.33 | 12.82 | 12× |
| `string length` | 5.41 | 0.49 | 0.39 | 14× |
| `string charCodeAt` | 97.78 | 0.81 | 1.46 | 121× |
| `string index []` | 123.03 | 1.39 | 0.50 | 246× |
| `string concat 2` | 141.88 | 0.52 | 0.38 | 373× |
| `string template literal` | 477.50 | 35.93 | 15.36 | 31× |
| `string equals` | 10.66 | 0.25 | 0.38 | 43× |
| `string indexOf 256` | 207.45 | 0.50 | 0.39 | 532× |
| `string slice 16` | 206.59 | 0.60 | 0.38 | 544× |
| `string split 16` | 4799.01 | 9.86 | 35.74 | 487× |
| `string toUpperCase 16` | 202.29 | 0.38 | 0.38 | 532× |
| `string number->string` | 154.76 | 28.65 | 18.57 | 8× |
| `string parseInt` | 138.48 | 0.50 | 0.63 | 277× |
| `string parseFloat` | 167.20 | 19.72 | 35.83 | 8× |
| `coll Map.get` | 52.10 | 0.51 | 6.98 | 102× |
| `coll Map.set existing` | 66.82 | 8.80 | 7.70 | 9× |
| `coll Map.has` | 50.86 | 0.47 | 6.52 | 108× |
| `coll Set.has` | 43.49 | 0.48 | 2.85 | 91× |
| `coll Object.keys 4` | 308.09 | 0.14 | 2.32 | 2201× |
| `json stringify small` | 5014.88 | 104.90 | 144.63 | 48× |
| `json parse small` | 2859.54 | 220.71 | 428.93 | 13× |
| `regex test` | 117.28 | 1.44 | 18.92 | 81× |
| `regex exec+group` | 2268.25 | 10.05 | 36.09 | 226× |
| `regex replace` | 1503.12 | 0.50 | 38.75 | 3006× |
| `binary Uint8Array read` | 24.65 | 0.49 | 0.52 | 50× |
| `binary Uint8Array write` | 30.76 | 0.47 | 0.44 | 70× |
| `binary Float64Array rw` | 27.66 | 0.35 | 1.90 | 79× |
| `binary DataView getU32` | 39.27 | 0.50 | 0.57 | 79× |
| `binary alloc Uint8Array 64` | 944.93 | 10.02 | 29.37 | 94× |
| `binary subarray 64` | 294.13 | 34.73 | 24.17 | 12× |
| `binary TextEncoder 16` | 1250.23 | 28.76 | 357.55 | 43× |
| `flow try/catch no throw` | 10.31 | 0.23 | 0.38 | 45× |
| `flow throw+catch` | 1628.61 | 424.15 | 8020.97 | 4× |
| `flow generator next` | 805.09 | 3.82 | 15.80 | 211× |
| `flow switch 8-way` | 4.22 | 1.83 | 1.71 | 2× |

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

`alloc object literal 2` at 1.22 and `alloc object literal 8` at 1.24, against a
floor of 1.27. The allocation is **gone** — `crates/rts-codegen/src/emit/escape.rs`
removed it. Which makes the interesting number not these but their neighbours:
`alloc class instance` at 90.89 and `alloc array literal 4` at 231.34, the same
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
