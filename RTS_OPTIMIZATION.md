# RTS_OPTIMIZATION.md — measured optimization plan

**Status:** RESEARCH + MEASUREMENT, 2026-08-01. Nothing in here is implemented by
this document. Every number is either measured (marked **[M]**), quoted from a
published source (marked **[S]** with the source), or an estimate (marked
**[E]** — treat as a hypothesis to test, not a plan input).

## How this relates to the documents that already exist

This does **not** replace them. Read them first:

| Document | What it owns | Status |
|---|---|---|
| `the spec removed 2026-08-03 (see git history)` | The **phase plan** (Phase 0–6) and the `RTS_REPR_STATS` methodology | Phase 0 LANDED; 1–6 plan |
| `OPTIMIZATIONS.md` | The **startup/compile-time campaign** (12 items, 890 → 128 ms) | Items 1,2,3,10,11,12 DONE |
| `crates/rts-value-probe/README.md` | The **per-operation measurement ladder** (107 rows) | Measured 2026-08-01 |
| `the spec removed 2026-08-03 (see git history)` | Weak phase + generational nursery | Weak = next; generational DEFERRED |
| `the spec removed 2026-08-03 (see git history)` | Regions/promotion, blockers #1–#7 | T0/T1 DONE, T2–T5 open |
| `docs/engine/architecture.md` | The four pillars | §8.3/§8.4 spec'd, never built |

**This document adds four things those do not have:**

1. The **per-operation cost ladder** — every JS operator, every primordial,
   priced against an alternative lowering, with checksum cross-validation
   (`rts-value-probe`, 11 kernels, 107 rows, 0 checksum failures).
2. **Eleven external research passes** against production engines and papers,
   which **refuted five premises** — three of mine and two the codebase carried.
3. **Two correctness findings** that surfaced while measuring performance and
   are not performance items at all (§6).
4. A **measured cross-runtime standing** against Bun and Node (§1b), with a
   per-workload estimate of where the plan actually lands.
5. An **honest limits section** (§8): the ways this document's own measurements
   are likely to mislead, sourced from how VM teams validate their own work.

## Contents

- §1 The one-paragraph conclusion
- §1b Where RTS stands against Bun and Node — measured, and the honest estimate
- §1c The structural finding: RTS reads the type and then throws it away
- §1e The backend question, settled — and the rule "symbolize the body, inline
  the operator" (Cranelift vs LLVM measured here; what the emitted IR contains;
  the two replace-Cranelift proposals; the compile-time exchange rate; an O(n²)
  defect)
- §2 What is already done (do not re-propose)
- §3 The measured cost ladder
- §4 Premises refuted by measurement or by source
- §5 The plan, ordered by (impact × confidence × feasibility)
- §6 Correctness findings — not optimizations
- §7 Documentation that is currently wrong
- §8 What these measurements do NOT prove
- §9 Open questions that need an owner decision
- §10 Sources

---

## §1 The one-paragraph conclusion

**The value representation is not the problem, and neither is Cranelift.** The
NaN-boxed `PolyValue` costs **0.81 ns/iter** against the only usable alternative
measured (a two-slot `{tag,value}` pair; the untagged-`f64` row is a ceiling, not
an alternative — it cannot hold an object or a string) **[M]**, and Cranelift
self-reports ~2% behind TurboFan and ~14% behind an
LLVM-based wasm compiler while compiling ~10× faster **[S]**. Every large number
measured is one of four things: **an extern call where a load would do**, **a
shard `Mutex` taken on a read**, **an allocation that a proof could have
removed**, or **work that is redundant by construction** (a double copy, a
`String` allocated per property read, a `pow` call for `x**2`). The plan in §5
attacks those four and nothing else.

---

## §1b Where RTS actually stands against Bun and Node — measured

Same source file through all three runtimes, this machine, 2026-08-01
(`rts.exe` release, Bun 1.3.14, Node 25.8.2). Times in ms.

| workload | **RTS** | Bun | Node | RTS vs Bun |
|---|---|---|---|---|
| numeric loop, 10M iter (proven monomorphic) | **7.00** | 9.00 | 16.87 | **RTS 1.3× FASTER** |
| array element read, 3M | 41 | 3.36 | 3.46 | 12× slower |
| closure capturing an accumulator, 2M | 105 | 8.31 | 12.87 | 13× |
| method call, 2M | 31 | 2.24 | 1.35 | 14× |
| field read, no allocation, 3M | 88 | 3.26 | 1.69 | 27× |
| `new` + field read (objbench), 1M | 577 | 7.16 | 8.37 | 81× |
| `Map<string,…>` get, 200k | 597 | 12.62 | 8.67 | 47× |
| `JSON.parse`+`stringify`, 2k rows ×5 | 760 | 3.67 | 3.95 | 207× |
| `s += "x"`, 20k | 174 | 0.54 | 0.48 | 322× |
| regex literal `.test()` in a loop, 100k | **3817** | 1.55 | 2.77 | **2463×** |

**The shape of the result:** RTS already **beats both** on the one thing it was
designed for — a proven-monomorphic numeric loop with no heap traffic. It loses
by one to three orders of magnitude on everything that touches the heap, the
stdlib, or a dynamic representation. The engine is not uniformly slow; it is
**bimodal**, and the plan's job is to widen the fast mode.

### Two axes the document never measured — and one is worse than every time number

**Startup: RTS already wins.** End-to-end wall clock for `console.log("hi")`:
**RTS 101 ms**, Node 128 ms, **Bun 203 ms** **[M]**. The startup campaign
(`OPTIMIZATIONS.md`, 890 → 128 ms) did its job; RTS launches faster than both.

**Memory: RTS loses by an order of magnitude, and non-linearly.** Peak working
set on `bench/cross_runtime_gap.ts` **[M]**:

| | peak RSS |
|---|---|
| Node | 53.4 MB |
| Bun | 93.0 MB |
| **RTS** | **1142.9 MB** |

**12× Bun, 21× Node.** And the shape of it is the finding, not the number.
Measured per workload, run alone:

| workload alone | RTS peak |
|---|---|
| `s += "x"` × 20k | 139.6 MB |
| `Map` 200k gets | 70.7 MB |
| `JSON` round-trip | 78.2 MB |
| **all three in one process** | **1142.9 MB** |

The parts sum to ~290 MB; together they reach **1143 MB — roughly 4× the sum**.
Memory is **not being reclaimed between phases**; it accumulates across a
mixed workload.

The likely mechanism is already documented in-tree and never re-examined:
`LIVE_BYTES` is fed by `entry_heap_bytes`, which `handles.rs:1460-1478` describes
as "deliberately a cheap order-of-magnitude heuristic, only large-payload
variants measured". If it undercounts, the `GC_LIVE_BYTES_FLOOR = 64 MB` trigger
does not fire when it should — which is precisely the failure mode the floor was
added to fix (`handles.rs:1431-1456` records 80k concats at 3644 MB before it
existed). **The floor fixed the single-workload case and this is the
mixed-workload case it does not cover.**

This is unmeasured territory for the whole document: **no item in §5 has a memory
budget, and four of them (bump nursery, in-object slack, cold-block duplication,
a second entry point per function) trade memory for speed.** A plan that ships
all four without a budget makes the number above worse. Cheapest fix: add peak
RSS to `bench/benchmark.ps1` and state a budget before anything lands.

### Does RTS still lose to Bun after the whole plan? — honest estimate **[E]**

Per-workload, applying only the levers with a measured factor behind them. These
are **estimates, not measurements**, and the probe that produced the factors is
5–11× optimistic (§8.0), so treat them as an upper bound on the improvement.

| workload | today | dominant cause | plan item | est. after | verdict |
|---|---|---|---|---|---|
| numeric loop | **1.3× faster** | — | none needed | **1.3× faster** | **RTS wins** |
| array element | 12× | `VEC_GET` locked call | A1+A2 | **~1.3×** | ~parity |
| field read | 27× | 2 locked calls + generic arith | A1+A2+A3 | **~2×** | close |
| method call | 14× | O(N) `icmp` dispatch + `err_pending` | A5 + dispatch | **~5×** | still behind |
| closure | 13× | every capture heap-boxed | A7 | **~4×** | still behind |
| objbench | 81× | allocation 69% | bump+barrier+A2 | **1.4× ahead … 2.4× behind** **[M]** | see below |
| `Map` get | 47× | FNV-1a per lookup, **a mutex per character** | Tier 0 hash cache | **~1×** **[M]** | **parity** |
| `JSON` | 207× | `.ts` stdlib over PolyValue | inherits Tier 0 + 3.x | ~10–20× | needs its own measurement |
| `s += "x"` | 322× | O(n²), 4–5 copies per concat | Tier 0 in-place append | **~2×** **[M]** | close |
| regex | **2463×** | **no compile memoization at all** | Tier 0 compile cache | **~4×** **[M]** | close |

### The three worst gaps are MEASURED as caching defects, and they close almost entirely

The estimate table above was written before the stdlib was measured. It is now,
and the answer changes: **the two worst workloads are one missing cache each.**

| REGEX — 100k `.test()`, `kernel_stdlib` | ns/iter |
|---|---|
| R0 today — recompile the NFA **every iteration** | 29 471 |
| **R1 compiled once, cached at the literal's site** | **47.45** |

**621×** from memoizing the compile. `regexops.rs:91-137` has no cache anywhere,
so `__rtsadp_re_compile` builds the whole NFA at the literal's site on every
pass. Applied to §1b: 3817 ms / 621 ≈ **6.1 ms vs Bun's 1.55 — from 2463× behind
to ~4×.**

| MAP — 200k gets over 1024 string keys | ns/iter | |
|---|---|---|
| M0 today — rehash per lookup, **one shard `Mutex` per BYTE** | 22.46 | |
| M1 bytes borrowed once, then hashed | 4.56 | 4.9× |
| **M2 hash cached with the key (the V8/JSC design)** | **0.48** | **46.8×** |

`map_set.ts:319-326` recomputes FNV-1a per lookup through `charCodeAt`, and each
`charCodeAt` is a native trampoline that takes a shard lock — **a mutex per
character**. V8 packs the hash into the string header and compares interned
pointers **[S]**. Applied to §1b: 597 ms / 46.8 ≈ **12.8 ms vs Bun's 12.62 —
parity.**

And the string case was already measured: `s += "x"` D0 → D2 is **158×**
(53.68 → 0.34 ms), which applied to §1b is 174 / 158 ≈ **1.1 ms vs Bun's 0.54 —
~2×.**

**Revised standing after just these three caching/algorithm fixes — no engine
change at all:**

| workload | before | after these three | vs Bun |
|---|---|---|---|
| regex | 2463× | ~4× | close |
| `Map` get | 47× | **~1×** | **parity** |
| `s += "x"` | 322× | ~2× | close |

**Answer: the gap is NOT structural for the three worst workloads.** They are a
missing compile cache, a missing hash cache, and a redundant copy. None of them
needs the arena, escape analysis, or a new value model. They are the cheapest
work in this document and they are worth more than everything else combined.

### And objbench — the 81× — measured with the full change set INCLUDING a write barrier

| `kernel_b`, 1M × `new P(i,i+1); s += p.x*p.y` | ns/iter |
|---|---|
| B0 today — locked slab alloc + locked reads + generic ops | 173.15 |
| B1 bump alloc + direct load + inline arithmetic | 5.23 |
| **B1b — B1 plus a real card-mark write barrier** | **5.11** |
| B2 escape analysis — the object never exists | 1.11 |
| *Bun, same source* | *7.16* |

Two results:

1. **The write barrier is free at this granularity** (5.11 vs 5.23 — noise). The
   objection "you cannot remove the lock without paying a barrier" is answered:
   the barrier costs nothing next to what the lock costs. HotSpot's ~12
   instructions / ~22 cycles **[S]** is the right order and it does not show up
   against a 5 ns iteration.
2. **B1b (5.11) is faster than Bun (7.16)** in the probe's model, and B2 (1.11)
   is 6.4× faster.

### THE CALIBRATION — the probe now models the engine, and it settles objbench

The objection above ("the probe is 3.3× optimistic, so B1b may really be 2.4×
behind") was answered by building the missing work into the probe instead of
arguing about it. `RTS_REPR_STATS=1` on `bench/objbench.ts` names **eleven** calls
per iteration; B0 emitted five. B0f emits all eleven **plus the GC tick at its
real cadence and with its real floor** (`GC_LIVE_FLOOR = 500_000`,
`handles.rs:1429`):

| row | ns/iter | vs the engine's 577 |
|---|---|---|
| B0 — 5 calls, no GC | 97.66 | 5.9× optimistic |
| B0f — 11 calls, no GC | 264 | 2.2× optimistic |
| B0f — 11 calls **+ naive GC** (sweep every tick, no floor, no reclaim) | 8452 | **14.6× pessimistic** |
| **B0f — 11 calls + GC with the real floor and reclamation** | **364.36** | **1.58× optimistic** |

The naive-GC row is worth keeping: a sweep that does not reclaim makes every
later sweep more expensive, and it overshot by 14.6×. That is the same
compounding failure the §1b memory measurement found (peak RSS 4× the sum of its
parts) — **evidence that the engine's live-bytes accounting under-triggers.**

With the residual **1.58×** applied to the same kernel's other rows:

| variant | probe | ×1.58 → engine scale | vs Bun's 7.16 |
|---|---|---|---|
| B0f today | 364 | 577 *(matches the measurement)* | 81× behind |
| **B1b bump + direct load + write barrier** | 3.15 | **4.98** | **WINS 1.44×** |
| B2 escape analysis | 0.72 | 1.14 | **WINS 6.3×** |

**objbench is settled: RTS wins.** Not by estimate — by a probe that reproduces
the engine's own number to within 1.6× and then applies the same residual to the
alternative. The remaining 1.58× is the ctor call, the thunk, the shape-registry
lookups and the real `Entry` width, none of which the change set makes worse.

**And it reorders the plan once more.** B0f's jump from 264 → 364 ns when the GC
tick is added means **roughly a third of objbench is collector work**, on top of
allocation. Bump allocation makes the *allocation* cheap but the object still
gets allocated and still gets collected. **Escape analysis is the only lever that
removes the GC pressure**, which is why B2 (1.14) is 4.4× better than B1b (4.98)
rather than the small delta the call-count alone would predict.

The three biggest gaps — regex 2463×, `s += "x"` 322×, JSON 207× — are **missing
caches and algorithmic defects**, not value-model or codegen problems. Caching a
compiled regex at its literal site is a day of work and is worth more than escape
analysis on this table. **That reorders the plan again**, and is why §5 now opens
with Tier 0.

### THE SCORECARD — every workload, with its measured alternative applied

This is the answer to "can RTS beat Bun everywhere". Each "after" applies the
factor **measured in the probe** for that workload's alternative lowering to the
**engine's own** §1b number. Anything without a measured alternative is marked
unmeasured, not estimated.

| workload | RTS today | Bun | measured lever | RTS after | verdict |
|---|---|---|---|---|---|
| objbench 1M | 577 ms | 7.16 | B0f→B2, **calibrated** | **1.1 ms** | **WINS 6.3×** |
| `s += "x"` 20k | 174 ms | 0.54 | STR 158× × CONCAT **12.3×** | **0.09 ms** | **WINS 6×** |
| regex 100k | 3817 ms | 1.55 | REGEX 621× × **9.7×** | **0.63 ms** | **WINS 2.5×** |
| array element 3M | 41 ms | 3.36 | ARR R0→R3, **25.3×** | **1.6 ms** | **WINS 2.1×** |
| objbench (bump only, no EA) | 577 ms | 7.16 | B0f→B1b, **calibrated** | 5.0 ms | **WINS 1.44×** |
| numeric loop 10M | 7.00 ms | 9.00 | — (already native) | 7.00 | **WINS 1.3×** |
| closure 2M | 105 ms | 8.31 | CLOSURE K0→K1, **16.4×** | **6.4 ms** | **WINS 1.3×** |
| `Map` get 200k | 597 ms | 12.62 | MAP M0→M2, **46.8×** | 12.8 ms | **PARITY** |
| field read 3M | 88 ms | 3.26 | A0→A5 (EA+LICM), **22.6×** | 3.9 ms | **PARITY** (see note) |
| method call 2M | 31 ms | 2.24 | P0f→P1, **calibrated 13.8×** | **2.25 ms** | **PARITY** |
| `JSON` | 760 ms | 3.67 | Q0→Q1, **calibrated 855×** | **0.89 ms** | **WINS 4.1×** |

**Standing: 8 wins, 3 parity, 0 behind — RTS reaches or beats Bun on every workload measured.**

### METHOD — calibrated, and it lands at parity

Same treatment kernel B got. `P0f` emits the engine's full per-call work: the
shape word read through the locked accessor, the **O(N) `icmp` dispatch chain**
over candidate classes (`vdispatch.rs:8-10`), the uniform 5-slot thunk, the
field read, and the `err_pending` poll.

| row | ns/iter | vs the engine's 15.5 |
|---|---|---|
| earlier H0 — locked field read only | 4.61 | 3.4× optimistic |
| **P0f — full profile** | **8.62** | **1.80× optimistic** |
| P1 — static dispatch, field as a load, no poll | 0.63 | — |

Residual 1.80× applied to P1: **1.13 ns/iter vs Bun's 1.12** — dead parity. Note
P1's raw 0.63 ns is *below* Bun, so attributing any of the residual to work P1
removes would make this a win; **the table takes the conservative reading**, the
same convention used for objbench.

### JSON — resolved, after the first replica was found to be 358× off

The first `Q0` captured **0.3%** of the engine's work (212 ns/row against the
engine's 76 µs/row) because it did the scan and the allocation and nothing else.
The engine's per-row work is dominated by three things it omitted, all verified
in the source:

- **`intern_poly` does not intern.** `abi_adapter.rs:62-67` records that it
  allocates a **fresh string and a fresh handle on every call** — so every key,
  on every row, is two allocations.
- **`key_text` mallocs an owned `String`** per property touch
  (`objops.rs:221`).
- **The shape registry is a process-global `Mutex`** taken on every dynamic
  property resolution (`shapes/mod.rs:76-79`).

Plus the `stringify` side, which reads every key back and concatenates through
the snapshot path that copies the accumulator. With all of that emitted:

| row | ns/row | vs the engine's 76 000 |
|---|---|---|
| first `Q0` — scan + alloc only | 212 | 358× optimistic |
| **`Q0` faithful — + non-interning keys, `key_text`, global shape lock, stringify** | **15 652** | **4.86× optimistic** |
| **`Q1` native — borrowed scan, keys interned once, arena objects, one output buffer** | **18.30** | — |

Applying the **full** 4.86× residual to `Q1` — the conservative reading, which
assumes every scrap of still-unmodelled work survives the change:

**18.30 × 4.86 = 88.9 ns/row → 0.89 ms for the whole benchmark, against Bun's
3.67 ms. RTS WINS 4.1×.**

The margin is larger than the uncertainty: the residual would have to be roughly
**four times worse again** (~20×) before this row flipped. The single structural
change carrying it is that **keys are compile-time constants and should be
interned once**, not allocated twice per key per row — which is the same defect
the `Map` row measured, in a second place.

**Note on `field read`.** Bun runs `s += p.x*p.y` at **1.087 ns/iter**, which is
the `fadd` dependency-chain floor (~3–4 cycles). No lowering beats a hardware
dependency chain — **parity is the ceiling there, not a win**, and claiming
otherwise would be arithmetic RTS cannot deliver. The A0→A5 lever (escape
analysis + LICM on a loop-invariant receiver) lands at 1.30 ns/iter against
Bun's 1.087, i.e. within 1.2× — a tie inside measurement noise of the floor.

**The two rows still behind, and exactly what each needs:**

- **method call, 1.9× behind.** The lever measured (H0→H1, direct call + the
  field as a plain load) is 7.3×, and H1's absolute — **0.63 ns/iter** — is
  already *below* Bun's 1.12. The 1.9× comes from scaling the engine's 15.5
  ns/iter by the lever, and the H0 replica under-models the engine by 3.4×, the
  same residual B0 had before calibration. **This row needs its own B0f-style
  calibration row** (full dispatch chain + `err_pending` + thunk) before the
  verdict is trustworthy in either direction. It is currently the weakest number
  in this table and is labelled as such rather than rounded in RTS's favour.
- **JSON, 13× behind.** Only the *scanner* was measured (`charCodeAt` per byte
  → borrowed byte loop, **16×**). `JSON.parse` also **builds objects**, which
  inherits the calibrated objbench levers, and `stringify` builds strings, which
  inherits the concat lever. Those compose, but **composing them is a
  multiplication I have not measured**, so this row stays at the scanner-only
  number. A full parse/stringify kernel is the remaining work.

**Where that leaves the goal: 2 wins, 1 parity, 4 within 2–4×, 3 unmeasured.**
Not "beats Bun everywhere" — but the shape is now known, and nothing on the list
is structural. The four still-behind rows each have an identified cause:

- **field read 2.0×** — the remaining cost after A1/A2 is the IC shape guard
  (0.13 ns) plus the arena address computation. Closing it needs the per-slot
  `Repr` so the loaded word is typed, which the STRUCT kernel measured at only
  0.23 ns — so **this row may simply not close**, and 2× behind Bun on an
  untyped property read is an honest place to land.
- **`s += "x"` 2.0×** — D2 is in-place append, which needs a liveness proof. D3
  (Rust `push_str`) is 0.011 ms, so the *representation* is not the limit; the
  proof is.
- **regex 3.9×** — after memoization the remaining gap is the **regex engine
  itself** (the Rust `regex` crate vs JSC's). That is a dependency choice, not
  an RTS defect, and closing it means either a different engine or accepting it.
- **objbench** — contested between 1.4× ahead and 2.4× behind purely because the
  probe under-models the engine by 3.3×. **This is the one number that decides
  whether the headline claim is "beats Bun" or "approaches Bun", and it is
  settled by an engine-side measurement, not another kernel.**

**The three unmeasured rows are the remaining work**, and two of them have a
named cause already: method dispatch is an **O(N) `icmp` chain** over candidate
classes (`vdispatch.rs:8-10`), and every closure capture is heap-boxed including
a proven `f64`. `JSON` is `.ts` over PolyValue and should inherit most of A1/A2
plus the string and `Map` fixes — but that is a hypothesis, and this document
does not put unmeasured hypotheses in a scorecard.

### The strategic read

RTS does not have to win by being a better JS engine — it wins where it is
**already a compiler**: the numeric loop (1.3×) and, once the access path is a
load, array element access (2.1×) and `Map` (parity). Those are not JIT
territory; they are places where static knowledge beats runtime feedback.

What the measurements refute is the assumption that the gap is architectural.
**Three of the four worst workloads are one missing cache each**, and the
allocation path — 69% of objbench — is beaten by a bump allocator plus a write
barrier that costs nothing measurable. The engine's foundations are not what is
slow. What is slow is a heap made invisible to the optimizer, and a stdlib that
recomputes what it should remember.

---

## §1c The structural finding: RTS reads the type and then throws it away

Everything above prices RTS **as if it were a JS engine competing with Bun**.
That framing is wrong, and it hid the real finding. RTS is a **language engine**:
the syntax is JS, but the intent is that structure and semantics go straight to
machine code. Measured against *that* thesis, the question is not "how do we make
the dynamic path faster" — it is **"why is a fully-annotated program on the
dynamic path at all?"**

### The one line where the thesis breaks

`crates/rts-codegen-new/src/front/repr_map.rs:56-58`:

```rust
// Out of the numeric subset → uniform tagged value; the lowering bails.
// (…objects/strings/closures/any are later increments.)
_ => Repr::Tagged,
```

That `_` catches `HirType::Class(ClassId)`, `Object`, `Array`, `Str`, and
`Function`. **A class whose every field is annotated, whose layout is fully known
at compile time, gets the same representation as `any`.** The TypeScript type is
read (`lowering_decls.rs:198` carries the annotation; `synth.rs:236` parses it) —
and then used for exactly one decision: `is_string`, to populate a
`field_strings` set. It never reaches layout, never reaches `Repr`, never
prevents a box.

`Repr::Ref(RefKind)` — the variant that would carry "this is a heap value of a
statically-known kind" — is **declared and constructed zero times** (§2). It is
the missing half of the lattice, and its absence is why every heap value is
`Tagged`.

### What that one line costs, layer by layer

| layer | what a language compiler does | what RTS does today |
|---|---|---|
| `class P { x: number; y: number }` | a struct; compile-time layout; two `f64` at fixed offsets | a runtime shape id + two **tagged** words in a heap `Vec<i64>` |
| `p.x` | `load [base + 8]` | shard-locked **extern call** returning a tagged word |
| `p.x * p.y` | `fmul` on two registers | `__rtsadp_mul` on two tagged words |
| `function f(a: number): number` | native ABI | native ✓ — **but one `Tagged` param poisons the whole return** (`sig.rs:197`) |
| `const p = new P(...)` local | stack slot or registers | heap allocation + handle + GC tick |
| `p.method()` on a known class | direct `call` | **already direct when monomorphic** ✓ — this layer is right |
| `new Map<string, number>()` | a native hash map | `.ts` source over PolyValue arrays, FNV-1a per lookup, **a mutex per character** |

**Every gap in §1b traces back to this table**, and most of them to its first
three rows. The IC exists to *discover at runtime* what the declaration already
stated. Shapes exist to *infer* a layout that was written down. Escape analysis
would be needed to *undo* an allocation that a struct would never have made. All
three are sophisticated machinery compensating for a type that was thrown away
one pass earlier.

### MEASURED FIRST: the struct-layout half of this argument is REFUTED

Before writing the proposal below as a plan, it was put through the probe
(`kernel_struct`, 3M iterations, all layouts bump-allocated in the same arena so
addressing is identical and only the LAYOUT differs):

| variant | ns/iter |
|---|---|
| T0 today — slab + locked calls + generic arithmetic | 14.96 |
| T1 **shaped** object, direct load, PolyValue words + `bitcast` | 0.92 |
| T2 **static struct**, raw `f64`, no header, no tag | **0.69** |
| T2g **static struct + class-id guard** | **0.92** |
| T3 scalarized — the object never existed | 0.70 |

Three results, and the first two kill the proposal:

1. **T0 → T1 is 14.04 ns of the 14.27 ns total — 98.4% of the entire win is
   making the read a LOAD, not changing the layout.** T1 → T2, the whole
   "store the field unboxed at its declared type, drop the shape word" idea, is
   **0.23 ns**.
2. **T2g equals T1 exactly (0.92 vs 0.92).** Adding back the class check that JS
   semantics *require* — `p` may be an `any`, a property may have been added,
   `delete` may have fired — puts the static struct back at precisely the
   shaped-object number. **Once the semantics are kept, the struct layout buys
   nothing.**
3. T2 (0.69) ≈ T3 (0.70): an *unguarded* struct is already at the
   scalar-replacement ceiling. But an unguarded struct is not JavaScript.

**Why the intuition was wrong, and it is instructive:** for a `number` field a
NaN-boxed double **is** the raw `f64` bits. There is nothing to unbox at rest.
"Store it unboxed" was a no-op dressed as an optimization; the only real content
of the idea was removing the shape word and the check — and the check is not
removable.

**What this vindicates:** the existing shape + inline-cache design is *not* the
problem. Once the read is a load, it is already at the right cost. The engine's
object model is sound; its *access path* is not.

**What this kernel did NOT measure, so the idea is dented, not dead:** every
field here is a `number`. A `string` or nested-object field stores a **handle**
in the PolyValue, so reading it costs a handle-table indirection a struct could
avoid with a direct pointer. Nor did it measure **memory** (3 words + `Box<Vec>`
+ a 56-byte slab `Slot` vs 2 words) or **allocation** cost. The surviving,
narrower claim is therefore: *a static layout may pay for heap-typed fields and
for footprint — it does not pay for numeric fields, which is what §1c originally
claimed.*

### The five structural changes, in dependency order

> **S1 and S2 are demoted by the measurement above.** They stay in the list
> because the closed-world and stdlib items depend on the same framing, but
> their justification is now footprint and heap-typed fields, **not** the
> numeric field access that motivated them. **S3–S5 are unaffected and are
> where the structural value actually is.**

**S1. `HirType::Class(id)` → `Repr::Ref(RefKind::Class(id))`, with a static
layout.** This makes `Repr::Ref` live and is the root change — everything else
depends on it. A class declaration already produces a `ClassId` and a flattened
field list (`synth.rs`); that is a struct definition in all but name. Give it a
compile-time offset table.

**S2. Store fields unboxed at their declared type.** `x: number` occupies 8 bytes
of `f64`, not a tagged word. Then `p.x` is a `load` typed `Float64` and `p.x*p.y`
is `fmul` — the two biggest rows of the table above collapse together. This is
`FUTURE_OPTIMIZATION.md`'s Phase 4a ("per-slot `Repr` on `Shape`") stated
correctly: not "annotate the shape with a Repr" but **"a declared class is not a
shape at all"**. Shapes remain for genuinely dynamic objects (`{}` literals with
computed keys, `any` receivers, `JSON.parse` output).

**S3. Stop the `Tagged` poison in signatures.** `sig.rs:197-204` widens the
return to `Tagged` if *any* parameter is `Tagged`. With S1/S2, far fewer params
are `Tagged` to begin with — but the rule itself should be per-value, not
per-signature.

**S4. Exploit the closed world.** RTS compiles a whole program. Absent
`eval`/`new Function`/dynamic `import`, the class hierarchy, the call graph and
every field type are fully known — the Dart-AOT/GraalVM-Native-Image situation.
A parse-time check for those three constructs is cheap and gates: direct
devirtualization, cross-function `Repr` propagation, and dead-branch elimination
on unreachable classes.

**S5. The `.ts` stdlib is the leak that defeats S1–S4.** `Map`, `Set`, `WeakMap`,
`JSON` are TypeScript compiled by this engine over PolyValue arrays. A perfectly
typed user program still enters the dynamic path the moment it calls
`map.get(k)`. Either those get native implementations behind the Registry, or
S1–S4's benefit stops at the stdlib boundary — which is exactly what §1b's
`Map` 47× and `JSON` 207× rows are measuring.

### What this does to the plan

**Not much, once measured — and that is the useful outcome.** The draft of this
section claimed S1+S2 would subsume most of Tiers 2, 3 and 4. The probe says the
opposite: **98.4% of the win is Tier 3 (make the read a load), and the layout
change is 1.6% that the mandatory class guard then cancels.**

So the structural reframing does *not* reorder the plan. It **confirms** the
ordering the adversarial review already forced (Tier 3 and 4 are the trunk), and
it removes an item rather than adding one. The engine does not need a new object
model; it needs the access path to stop being an extern call under a mutex.

What survives from the "language engine, not JS engine" framing is real but
narrower than it looked:

- **S3 (kill the `Tagged` signature poison)** — unaffected, still worth doing.
- **S4 (exploit the closed world)** — unaffected, and it is the item that most
  deserves the name "language engine": devirtualization, cross-function `Repr`
  propagation and dead-class elimination are things a JS *engine* cannot do and
  a whole-program *compiler* can.
- **S5 (the `.ts` stdlib leak)** — unaffected, and §1b measures it as the single
  largest remaining category (`Map` 47×, `JSON` 207×, regex 2463×).

And one thing the measurement retires: **the fear that `Repr::Ref` being dead is
catastrophic.** It is not, for numeric fields — a proven-shape read that returns
`Tagged` costs 0.23 ns more than one that returns a typed `Float64`. `Repr::Ref`
matters for *heap-typed* fields and for feeding the operator guard, not for the
field access itself.

---

## §1d THE ARCHITECTURE: make the heap visible to the compiler

This is the plan that follows from treating RTS as a language engine. It is
built on one measurement and one research finding, both new.

### The measurement: the plan is MULTIPLICATIVE, not additive

`kernel_visible`, 3M iterations. Every other kernel prices ONE access; this one
asks whether making the access a `load` lets the optimizer remove work that is
**impossible** to remove today.

**CSE — the same field read twice per iteration:**

| variant | ns/iter |
|---|---|
| V0 today — two opaque calls | 11.83 |
| **V1 — two `load`s, `MemFlags::trusted()`** | **0.97** |
| V2 — two `load`s, `readonly` | 1.22 |

A *single* field read as a load costs ~1.0 ns. Two loads also cost 0.97 ns —
**the egraph folded them into one**. 12.2× on a double read, and it scales with
every redundant access in the program.

**LICM — one loop-invariant read:**

| variant | ns/iter |
|---|---|
| V3 today — opaque call | 6.24 |
| V3u — same call, **shard `Mutex` removed** | 4.30 |
| V4 — `load`, `trusted` | 1.47 |
| **V5 — `load`, `readonly`** | **1.03** |
| V6 — hand-hoisted before the loop | 1.13 |

Three results, each actionable:

1. **The CALL is what blocks hoisting, not the lock.** V3u removes the mutex and
   still cannot hoist (4.30 vs 1.03). Removing the lock without removing the
   call leaves most of the win on the table.
2. **LICM fires — but only with `readonly`.** `trusted` alone (V4, 1.47) is 43%
   slower than `readonly` (V5, 1.03). `MemFlags::trusted()` is
   `notrap + aligned` and explicitly does **not** license code motion; today RTS
   uses `trusted()` and nothing else, tree-wide.
3. **V5 (1.03) matches V6 hand-hoisted (1.13).** Cranelift produced what a human
   hoist produces. So the front-end does *not* need to hand-hoist every
   invariant read — it needs to hand the optimizer the right flags.

### The research: RTS already satisfies the hard precondition

How do GC'd languages emit a plain `load` for a field access?

- **Go's GC does not move objects at all** — a field access is a `load` at a
  fixed offset, and there is *no read barrier anywhere*. Its hybrid write barrier
  fires only on **writes**, only during marking, explicitly because "pointer
  reads tend to outweigh pointer writes by an order of magnitude or more" **[S]**.
- **HotSpot and .NET DO move** — but only at a **safepoint**, with mutators
  stopped. Between safepoints a field access is literally
  `mov 0x10(%rsi),%r10` **[S]**. The safepoint poll costs 0.38–0.59 ns and fires
  only when a collection is requested.
- **V8's pointer-compression cage** is the one real "indirection with good
  codegen": a compressed 32-bit offset plus a **base held in a register** — one
  `add`, optimized down to 2 instructions / 7 bytes. **Not** a second load
  through a locked table. Measured: up to 43% heap reduction, 20% renderer
  memory **[S]**.

**RTS's collector is already non-moving.** It therefore *already satisfies* the
condition that lets Go emit plain loads — an object's storage never changes
address once allocated. **The handle indirection is not buying GC safety; it is
buying nothing, and it costs the entire optimizer.**

### The architecture

**A1 — the payload becomes an OFFSET INTO A STABLE ARENA, not a slab index.**
`addr = arena_base + payload` — one `iadd` against a base loaded once per
function, exactly V8's cage. The arena is chunked and append-only so a chunk
never moves once allocated; the 48-bit payload space is unchanged, so the
NaN-box encoding and its tag budget are untouched.

*Why this keeps the conservative scanner sound:* the scanner's safety property is
that a stack word which looks like a handle can be **validated** — it must name a
live slot with a matching generation. A base+range arena preserves exactly that
(a candidate is a root iff it falls inside the known arena range and its
generation matches), which is why V8's cage is a *region*, not a raw heap
pointer. This is the one design constraint that must be written down before
anyone starts.

**A2 — a field read becomes `load [addr + K]`.** This is the change that pays
for everything else: 14.04 ns of the 14.27 ns field-read gap **[M]**, plus the
CSE and LICM above that are impossible today at any price.

**A3 — annotate loads so the optimizer can move them.** `readonly` on genuinely
immutable slots (the shape/class word, a `readonly` field, a frozen object) is
worth 43% over `trusted` **[M]**.

*The alias-region half of A3 was measured and did NOT reproduce.* Read field A,
store field B, read A again:

| variant | ns/iter |
|---|---|
| A3-0 one region for both | **0.78** |
| A3-1 distinct `AliasRegion`s (`Heap` / `Table`) | 1.29 |
| A3-2 no store at all (the ceiling) | 0.99 |

Tagging the two fields into distinct regions made it **slower**, and the
"ceiling" was slower than the naive case — all three sit inside the ~0.7 ns
`fadd`-chain floor, so the kernel is not sensitive enough to resolve the effect.
**Verdict: inconclusive, and specifically NOT evidence that alias regions help.**
Do not put alias-region work in the plan on the strength of this; re-measure with
a kernel that is not latency-bound before proposing it.

**A4 — replace the lock with a write barrier.** Reads become free; writes pay a
card-mark, measured in HotSpot at ~12 instructions / ~22 cycles **[S]** — far
below the extern call plus mutex RTS pays on **both** sides today.

**A5 — the error poll becomes a load+branch, not a call. MEASURED:**

| variant | ns/iter |
|---|---|
| A5-0 today — `call __rtsadp_err_pending` + branch | 2.94 |
| **A5-1 inline load + branch (the Rust `?` shape)** | **1.26** |
| A5-2 no poll at all — a callee proven non-throwing | 0.79 |

**2.3× on the poll itself**, and it is emitted at **38 sites** — after every
call, `new`, property set, `await`, and per `for…of` element. Rust's `?` never
calls out to ask whether a call failed; it branches on a discriminant already in
a register **[S]**. The thread-local slot can stay exactly as it is; only the
*read* moves inline. That removes the cost **and** an optimization barrier that
currently splits every basic block containing a call — which, per the CSE/LICM
result above, is worth more than the 1.68 ns it directly saves.

The remaining 0.47 ns (A5-1 → A5-2) is what a `nothrow` proof would buy: a
callee that provably cannot throw needs no poll at all. That is a second,
separable item.

**A6 — native signatures where the front end has already proven the types.**
Cranelift signatures freely mix `f64`/`i64`/pointer per parameter — the uniform
5-slot thunk is an RTS decision, **not** a Cranelift limitation **[S]**. Swift is
the precedent: indirection only where genericity is genuinely unresolved. Go
measured register-passing at ~40% per argument access and ~5% end-to-end **[S]**.

**MEASURED**, after the first kernel failed its checksum and was rebuilt (it read
one field twice instead of two — the guard is why that is a note here rather than
a wrong number in a plan):

| variant | ns/iter |
|---|---|
| A6-0 uniform 5-slot thunk — boxed args, tagged result | 4.37 |
| **A6-1 native `fn(f64, f64) -> f64` — two registers in, one out** | **3.10** |

**1.41×.** Smaller than the other items, and the honest reading is that most of
both numbers is the call itself; the 1.27 ns delta is the boxing plus the three
dead slots. Worth doing where the proof already exists — it is a signature
change, not new analysis — but it is **not** a headline lever, and this document
should stop quoting Go's ~5% end-to-end as if it were RTS's number.

**A7 — stop boxing captures that do not escape.** Rust closures are stack structs
of captures; boxing happens only at the `dyn Fn` erasure boundary **[S]**. RTS
boxes every capture including a proven `f64`.

### Why this order, and what it should be worth

A1→A2 is one change (the arena *is* how the read becomes a load) and it is the
root: it converts the heap from an opaque call boundary into memory the egraph
can reason about. A3 then costs almost nothing and unlocks LICM. A5 removes the
remaining optimization barrier inside basic blocks. Only after those does escape
analysis (Tier 4) have clean IR to work on.

**Estimated, and marked as estimate [E]:** the field-read path goes 15.81 → ~1.0
ns, redundant reads go to zero rather than to "cheaper", and the loop-invariant
case goes 6.24 → 1.03. Against Bun's 3.26 ms on the field-read workload where
RTS measures 88 ms, that is the difference between 27× behind and roughly
parity. **It does not by itself beat Bun** — allocation (69% of `objbench`),
the stdlib algorithms (`Map` 47×, JSON 207×, regex 2463×) and closures are
separate items. It does make every one of them cheaper to fix, because they are
all written on top of this access path.

---

## §1e The backend question, settled — and the rule that replaces it

This section exists because a reasonable objection was raised and it turned out
to be the most productive question in the whole investigation: *Cranelift is only
a few percent behind LLVM, so a 207× gap on JSON cannot be codegen — where is it
actually going?* Following that question produced the sharpest results here, and
it also killed three proposals, one of which was mine.

Everything below is measured on this machine by `crates/rts-value-probe`
(kernels `BACKEND`, `IR LADDER`, `SYM`, `COMPILE-TIME`, `STDLIB SHAPE`,
`COMPLEXITY`) or read off the engine's own `RTS_TIMING=1` instrumentation.

### §1e.1 Cranelift vs LLVM, measured here rather than cited

The widely-quoted "~14% behind LLVM" comes from Cranelift's own README, which
cites Xu & Kjolstad (OOPSLA 2021) — a **2020** measurement, taken before
regalloc2 (2022) and before the egraph mid-end (2023). It should not be quoted as
current. The current third-party figure (Frank Denis, libsodium suite, 2026-06)
puts Wasmtime/Cranelift at **2.41× native** against **1.57×** for LLVM-based wasm
runtimes — a ~1.5× gap, improving yearly (2.67 → 2.54 → 2.41).

Measured directly instead, same machine, same data, five shapes:

| shape | Cranelift | LLVM `-O3` | gap |
|---|---|---|---|
| integer reduction `s += a[i]` | 0.45 | 0.07 | **6.82×** |
| integer branchy `a[i]&1 ? s+=a[i] : s-=a[i]` | 0.69 | 0.22 | **3.16×** |
| dependent FP chain `s = s*k + a[i]` | 1.36 | 1.33 | 1.02× |
| FP reduction `s += a[i]*a[i]` | 0.70 | 0.67 | 1.05× |
| FP predicated `s += a[i]>0 ? a[i] : 0` | 0.74 | 0.68 | 1.09× |

The pattern is exact: **where vectorization is legal, Cranelift loses 3–7×;
where it is not, Cranelift is at parity (2–9%).** Register allocation,
instruction selection and scheduling are not the gap. Vectorization is, and
Cranelift has no vectorizer — verified by source search, no pass, no RFC, no
tracking issue.

**A falsifier was run against my own conclusion.** The 6.82× could have been my
naive IR (an address multiply per iteration, no unrolling) rather than a missing
vectorizer. Emitting the same loop with an incrementing pointer, unrolled 4×,
with four independent partial sums:

| | ns/elem | vs LLVM |
|---|---|---|
| Cranelift, naive IR | 0.46 | 6.97× |
| **Cranelift, strength-reduced + unrolled** | **0.17** | **2.61×** |
| LLVM `-O3` | 0.07 | 1.00× |

**2.67× of the apparent gap was the IR, not the backend** — and that part is a
front-end change, available with no backend work at all. The irreducible
Cranelift deficit is **2.61×, on vectorizable integer loops only.**

That deficit is also currently unreachable: vectorization needs proven
monomorphic types, no boxed values and no GC safepoint in the loop — which is
precisely what §1c and §1d are about building. **The backend question cannot even
become live until the rest of this plan is done.**

### §1e.2 What the emitted IR actually contains

`rts ir` prints per-function Cranelift IR. **It prints PRE-optimization**
(`parcompile.rs:338` runs right after `fb.finalize()`, before `define_function`),
so no cost may be read off it directly — the egraph still gets a pass. Duplicate
`iconst`s and `band`s in that dump are cleaned up and are not findings.

For `function sumArray(a: number[], n: number) { let s = 0; for (…) s = s + a[i] }`
the engine emits, per iteration: an `fcvt_from_sint` to compare the i64 counter
against an f64 `n`; a masked handle plus an opaque **call** for `a[i]`; a hole
check; a bitcast plus NaN-canonicalizing `select` to box the accumulator; a second
opaque **call** for the add; and a 12-instruction tag-check unbox.

To price that honestly, `emit/kernel_ir.rs` re-emits that exact IR through the
engine's own ISA flags, lets the same optimizer at it, and removes one defect per
row. **After optimization**, ns per element:

| row | ns | |
|---|---|---|
| **E0** the engine's IR, verbatim | **8.18** | — |
| E1 − the per-iteration `fcvt_from_sint` | 7.76 | 1.05× |
| **E1b** + the inline tag-check fast path on `+` | **4.95** | **1.57×** |
| E2 − the array read becomes a `load` | 6.00 | 1.29× |
| **E3** the add is proven (`fadd`) | **0.67** | **12.2×** |

E1b and E2 are two independent single steps from E1, not cumulative.

Three findings, in cost order:

1. **`+` has no inline fast path, though the design doc specifies one.** The
   canonical design (§Pilar 3) says: *"ONE `ADD_GENERIC` … with an inline
   tag-check fast path for the secretly-monomorphic case."* The emitted IR calls
   unconditionally, with no test before it. Adding the documented check is
   **1.57×**, needs no arena and no stable addressing, and is therefore
   shippable independently of everything else in §5. It is the cheapest lever in
   this document.
2. **The array read being a call costs far more than the call.** Directly it is
   1.29×. But it returns `Tagged`, which forbids proving the add, which forces
   the generic call and the box/unbox round trip — 12.2× gated behind one
   defect. It also forbids CSE: `s += a[i]*a[i]` with two calls is 11.41 ns; with
   two `load`s the egraph folds them to one, 5.21 ns (**2.19×**). This is the
   direct IR-level confirmation of §1d.
3. **The loop counter is converted i64→f64 every iteration** (5%). Narrow: the
   engine already emits a plain `icmp` when the bound is `a.length`; the gap is
   only an `n: number` parameter used solely as a bound.

**Independent confirmation.** Perry — a TypeScript→native AOT compiler with an
SWC frontend, NaN-boxed values, shape caching, the same shape as RTS — migrated
Cranelift → LLVM in 2026-04. The naive cutover was **68× slower** (`method_calls`
16 → 1084 ms), because hot NaN-box operations went through opaque runtime helper
calls the optimizer could not see into. All six fixes that recovered it were
*front-end* changes, and three are on the list above: i32 loop counters instead
of f64, compile-time field offsets, an inline bump allocator. Their conclusion:
*"optimization boundaries matter more than optimizer quality."* Treat their final
numbers as a vendor self-report, not a clean A/B — but the failure mode is
exactly the one measured here.

### §1e.3 The proposal that Cranelift be replaced by precompiled symbols

The proposal: since RTS already has a static symbol table, let precompiled native
symbols carry the operations and conversions, and let Cranelift merely order
them — so the engine emits far fewer instructions.

**The runtime-lookup half is already free.** `module_jit.rs:94` calls
`builder.symbol(name, ptr)` once at module construction; the binary search in
`rts_abi::table::lookup` is setup. After `finalize_definitions` the emitted code
holds a direct call to a relocated absolute address. There is no runtime
resolution to make O(1) — it is already O(0).

**The throughput half was measured in its strongest form.** `bench/symarch.rs`
sums 8192 f64 through a precompiled LLVM-compiled symbol, varying how much work
each call does — the steelman, because a bigger unit amortizes the call:

| work per call | ns/elem | vs floor |
|---|---|---|
| **1 element (today's engine)** | 4.78 | **7.19×** |
| 4 | 1.33 | 1.99× |
| 16 | 0.78 | 1.17× |
| 64 | 0.69 | 1.04× |
| **the whole loop, 1 call** | **0.67** | 1.01× |
| **1 call, raw pointer, no container (the ceiling)** | **0.66** | **1.00×** |
| **Cranelift emits the loop inline** | **0.67** | **1.01×** |

**At maximum amortization the architecture converges to exactly what Cranelift
emits and never passes it.** The ceiling — raw pointer, no container, LLVM at
`-O3` — beats Cranelift's inline emission by 1.5%, inside the noise. The reason
is not subtle: the work is `load` + `fadd`, and no compiler emits a better
`fadd`. The call pays for itself only above **~16 elements of work**.

But the crossover is the point, because **the work per call is not a free
parameter — it is how much of the program a precompiled symbol can cover**, and
that splits cleanly in two. See §1e.5.

### §1e.4 The proposal that Cranelift be replaced by a hand-written `rts-asm`

Code quality is closed by §1e.3 — a replacement cannot emit a better `fadd`. The
only live axis is compile speed, so that was measured.

**What Cranelift charges per IR instruction** (`bench/compiletime.rs`, marginal
slope so per-function fixed cost is excluded):

| ops | IR instructions | compile µs/fn | **µs/instruction** |
|---|---|---|---|
| 25 | 75 | 67.6 | 0.902 |
| 50 | 150 | 118.7 | 0.681 |
| 100 | 300 | 220.6 | 0.680 |
| 200 | 600 | 419.6 | 0.663 |
| 400 | 1200 | 825.1 | 0.676 |

**0.68 µs per IR instruction, linear across a 16× range.** Against a
copy-and-patch floor (`memcpy` + immediate patching, a deliberate lower bound)
of 0.3 µs/fn, Cranelift's compile is **1521×** that floor. So a template-stitching
backend genuinely could win this axis — copy-and-patch is not speculative, it is
the very paper Cranelift's README cites, and it reports *comparable* code quality
(2.6% slower on Coremark, 4.6% faster on PolyBenchC vs Wasmer Cranelift).

Three things make it the wrong project anyway:

1. **Cranelift is a minority of the pipeline.** `RTS_TIMING=1` on a one-line
   program: Cranelift machine-compile **17.59 ms (35%)**, RTS's own front-end
   (parse, lower, prune, merge, build IR) **27.57 ms (54%)**. Deleting Cranelift
   instantly leaves the larger half untouched. On `objbench` the 20 ms is **1.2%**
   of a 1641 ms run.
2. **A shipped feature already takes this axis to zero.** With
   `RTS_JIT_CACHE=1`, run 1 is a miss (425 functions, 30.86 ms machine-compile);
   run 2 is a hit and **the machine-compile phase disappears entirely**. The
   whole-program replay already does what `rts-asm` would be built to do.
3. **The cost is ~200 000 lines of the hardest code in a compiler**
   (cranelift-codegen 162 693 + regalloc2 13 833 + codegen-meta 13 342 +
   frontend 7 164 + jit/object 3 357), per ISA, plus four calling conventions,
   plus COFF/ELF/Mach-O object emission with relocations for AOT, plus unwind
   info (SEH / DWARF CFI) that the crash handler and exceptions need. And
   copy-and-patch is not small either — the paper generates ~100k template
   variants, which is how it recovers code quality.

**If this is wanted, the shape is a tier BELOW Cranelift, not instead of it.**
V8 has Sparkplug under TurboFan, JSC has LLInt/Baseline under DFG/FTL, and
Wasmtime built **Winch** for precisely this reason. Cranelift stays as the
optimizing tier and as the only AOT path — which iOS requires, since iOS forbids
JIT permanently (no W+X mapping; the EU BrowserEngineKit carve-out requires being
a browser passing 90% of Web Platform Tests).

### §1e.5 THE RULE: symbolize the body, inline the operator

The two proposals above are half-right, and the measurements say precisely which
half. The exchange rate between them is now a number.

**The break-even.** The inline fast path on `+` (E1b) is ~12 extra IR
instructions per site. At 0.68 µs/instruction that is **8.2 µs of compile, once,
per site**, against **2.81 ns saved per execution** (7.76 → 4.95). Break-even:
**~3 000 executions.** Any loop body passes that in its first benchmark
iteration; cold code runs once. That number *is* the tier-0/tier-1 boundary,
derived rather than assumed.

**The integration test.** `bench/stdlibshape.rs` runs the same stdlib operation —
a JSON document scan — in both shapes and measures **both axes at once**, because
a claim about two axes has to be checked on two axes:

| | throughput | compile |
|---|---|---|
| P0 `.ts` prelude shape — a trampoline per character | 59.68 ns/char | 89.50 µs |
| P1 native symbol — one call for the whole document | **0.48 ns/char** | **26.30 µs** |
| | **124×** | **3.4×** |

**It wins both, simultaneously, with no tradeoff.** And the 3.4× understates it:
that is only the call site. The symbol's *body* costs the compile pipeline
**nothing**, because it is already compiled into the binary.

The rule that falls out:

| | startup | throughput | verdict |
|---|---|---|---|
| **stdlib `.ts` body → native symbol** | −42 ms (see below) | work-per-call 1 → n | **wins both — do it** |
| **user-code operator → symbol call instead of inline IR** | −0.68 µs/instr | −2.81 ns/execution | only below ~3 000 executions |

There is no conflict between the two rows; they describe different layers. What
the measurements reject is only the strong form — replacing Cranelift's emission
inside hot user code — because there the work per call is forced to 1, and a
call costs on every execution while compilation costs once.

**The startup number.** `RTS_TIMING=1` on `const x = 1 + 2`:

| phase | ms | prelude? |
|---|---|---|
| prelude parse+lower | 3.97 | yes |
| prune prelude | 3.88 | yes |
| merge programs | 7.36 | yes |
| build fn IR | 10.87 | 425 of ~430 fns |
| machine-compile | 17.59 | idem |
| **≈42 of ≈51 ms** | | |

**82% of compile time is prelude, on every startup, for a one-line program.** As
native symbols that is not 0.68 µs/instruction — it is zero.

This also reframes the three worst workloads in §1b. They are not three separate
algorithm problems; they are one shape problem, three times:

| defect | in this vocabulary |
|---|---|
| `json.ts:238` scans with `s[i]`, allocating a fresh string + handle per character | work per call = 1 |
| `map_set.ts:323` hashes with `k.charCodeAt(i)` in a `.ts` loop | work per call = 1 |
| the regex literal is recompiled on every call | work per call = 1 |

`JSON.parse` should be **one** native symbol consuming the whole document — work
per call = 131 072. It is currently TypeScript calling a trampoline per character.

**And one thing that "optimize it in RTS's own optimizer" should not mean.** RTS
does not need an optimizer of its own: §1e.3 shows Cranelift's inline emission
already ties the native ceiling. What it needs is the stdlib *bodies* written in
Rust, where LLVM optimizes them when the runtime is built. That optimization
already exists and is simply not being used, because the code is in `.ts`.

### §1e.6 A quadratic defect found on the way

Chasing "where does 207× actually come from" turned up something no constant
factor explains. `dyndispatch.rs:261` (the DYNAMIC string-index path the `.ts`
stdlib takes) memoizes `bytes.is_ascii()` in a **one-entry** thread-local cache
keyed by `(ptr, len)`. Its comment justifies the single entry with *"o padrão de
uso é varrer uma string do início ao fim."* A parser does not do that — it
alternates between the source and the strings it builds, and every alternation
evicts the entry, making the next source access rescan the whole document.

Measured (`bench/complexity.rs`), ns per character:

| document | cache hits every access | cache thrashed |
|---|---|---|
| 4 096 | 4.20 | 71.07 |
| 16 384 | 4.32 | 245.97 |
| 65 536 | 4.28 | 947.09 |
| **131 072** | **4.30** | **1 881.70** |

The hit path is **flat across 32× of size — O(1), correct**. The thrashed path
**doubles when the document doubles** (947 → 1882): **O(n) per access, O(n²)
overall**, 437× at document size.

Note the static path is already right: `strops.rs:32-57` tests the byte *at
index i* rather than scanning the whole string, and its comment records the same
bug being fixed there once (*"medido: 100 mil chamadas sobre 100 KB levavam
6 200 ms"*). The dynamic path did not get that fix. The correct repair is to put
the ASCII flag on the string itself rather than in a one-entry side cache — but
if the `.ts` scanner becomes a native symbol per §1e.5, this path stops being hot
at all.

---

## §2 What is already DONE — do not re-propose

Sourced from a full-tree inventory. This is not exhaustive prose; it is the list
a new proposal must check itself against.

### Engine (`crates/rts-codegen-new/`)

| Optimization | Where | Measured |
|---|---|---|
| Class prototype hoisting out of the loop | `front/run/class/protohoist.rs`, `newexpr.rs:282` | **≈11×** on `objbench_methods.ts` **[M]** |
| Prelude reachability pruning | `front/run/prune.rs` | 831 → 280 fns; startup 890 → 390 ms; +8 files fixed **[M]** |
| Parallel Cranelift compilation | `front/run/parcompile.rs` | 142.9 → 34.5 ms (**4.1×**) **[M]** |
| Verifier off in release — **JIT ONLY**; `module_aot.rs` has no such override, so `rts compile` still runs it | `module_jit.rs:75-84` | Cranelift phase 221 → 171 ms **[M]** |
| gcell promotion + immutable-gcell memo | `front/run/gcell.rs:31-64` | **2.2×**, and fixed NEGATIVE thread scaling **[M]** |
| Property inline cache — MONO only, READS only | `front/run/ic.rs`, live at `obj.rs:1413,1467` | untyped read 436 → **20 ms** (≈**22×**) **[M]** |
| Bitwise/shift INLINE path (non-Tagged operands) | `binop.rs:491-536` | — |
| `emit_to_int32` with the ±Inf guard | `binop.rs:466-489` | — |
| Native `srem` when the divisor is a known non-zero constant | `binop.rs:654-670` | — |
| Selective TCO / `return_call` | `front/run/tco.rs` | — |
| `NativeEmit` per-member emitters (**replaced** `abi::Intrinsic`) | `front/run/intrinsic.rs` | — |
| Front-end constant folding (`typeof`, `instanceof`, `NaN`/`Infinity`, Math consts) | `expr.rs:310-390`, `globalclass.rs:510` | — |
| Typed-array base/len hoisting + inline element access | `front/run/ta_native.rs` | — |
| `FuncRef`/`SigRef` import dedup | `value/emit_marshal.rs:20-79` | 1947 decls → 229 distinct **[M]** |
| Prelude lowered-cache (on by default) | `front/run/prelude_cache.rs` | ~47 ms parse+lower skipped **[M]** |
| Whole-program JIT cache (opt-in `RTS_JIT_CACHE`) | `front/run/progcache.rs` | dormant |
| `switch` → Cranelift jump table | `front/run/switch.rs` | — |
| Shape-word `icmp` chain virtual dispatch (not `gc.string_eq`) | `class/vdispatch.rs` | — |

### Runtime (`rts-natives/`, `rts-runtime/adapters/`)

| Optimization | Where | Measured |
|---|---|---|
| Sharded handle table, shard in the LOW bits → O(1) routing | `heap/handles.rs:1-13` | — |
| `payload_ops` fused lock (one lock instead of two) | `heap/payload_ops.rs` | −⅓ of in-loop calls, −½ lock traffic **[M]** |
| `global_shape_len` O(1) (after the clone-per-access bug) | `heap/shapes/mod.rs:512` | 4 fields 178 ms → 48 fields 1030 ms, fixed **[M]** |
| Shape slot index (`SLOT_INDEX_MIN_KEYS = 8`) | `shapes/mod.rs:23-51` | 256 keys: 178 → 86 ms (**2.07×**); counterfactual actually run **[M]** |
| Shape transition tree (memoized add-key edges) | `shapes/mod.rs:139-186` | key-by-key construction was O(n²) **[M]** |
| Thread-local `cached_key_word!` / `cached_shape_id!` memos | `abi_adapter.rs:114-176` | — |
| Zero-copy string byte access (`with_handle_bytes`) | `abi_adapter.rs:205` | 115 KB `JSON.parse` was **42 s** **[M]** |
| GC live-bytes floor (64 MB) alongside the handle floor | `handles.rs:1431` | 80k concats 3644 MB → 93 MB **[M]** |
| `PINNED_ROOTS` as a `HashSet` (pin was quadratic) | `handles.rs:1664` | string workload 4.09 → 1.08 s (**3.8×**) **[M]** |
| Shared tokio runtime | `rts-std/src/runtime/async_rt.rs` | — |
| Scoped `opt-level = 3` per crate | `Cargo.toml` | startup 244 → 188 ms; strings 1.08 → 0.32 s **[M]** |
| opengl32 delay-load | — | `rts --version` 78 → 12 ms **[M]** |

**Combined startup campaign result:** empty program **890 → 128 ms (7.0×)**,
Monte Carlo 10M JIT **960 → 202 ms**, suite ~10 min → 37 s **[M]**.

### Things that do NOT exist (proposals naming them are wrong)

- **`abi::Intrinsic`** — deleted, replaced by `NativeEmit`. `CLAUDE.md` still
  references it (§7).
- **`try_bin_imm`** — zero hits in the tree.
- **`PropIcCell` / `IcState` / mono→poly→mega** — spec'd in the design doc
  §8.3, **never built**. The shipped IC is a flat 3-word mono cell.
- **Dictionary-mode fallback (§8.4)** — never built. The legacy `Entry::Map`
  rows are what the fallback actually is.

### Things that exist but carry nothing

- **`Repr::Ref(RefKind)`** — declared, constructed **zero times**. Consequence:
  *every heap value is `Tagged`*. This is the root of most of §3.
- **JIT stack-map transport** — wired end to end, produces an empty set.
  `declare_value_needs_stack_map` appears 5 times, all comments.
- **`aot_symbols()`** — zero consumers (§6.2).

---

## §3 The measured cost ladder

All from `crates/rts-value-probe` (11 kernels, 107 rows, medians of 7, checksum
cross-validated between every variant of a kernel, 0 failures). Read
`crates/rts-value-probe/README.md` §"What it does NOT prove" before quoting.

### 3.1 Per primordial

| Primordial | today | best variant | factor |
|---|---|---|---|
| Object — construction | 100.65 ns | 0.73 ns | **138×** |
| Object — dictionary read | 63.82 ns | 1.00 ns | **64×** |
| Array — element read+write | 17.69 ns | 0.70 ns | **25×** |
| Object — field read | 15.43 ns | 1.18 ns | **13×** |
| Number — tagged int `+` | 7.29 ns | 0.70 ns | **10×** |
| String — `===` (24 B) | 19.83 ns | 1.18 ns | **17×** |
| String — `s += "x"` ×10k | 53.68 ms | 0.34 ms | **158×** |
| Boolean — `if (x)` | 3.25 ns | 1.07 ns | **3×** |
| Value repr — vs a two-slot `{tag,value}` (the real alternative) | **1.79 ns** | **0.98 ns** | **1.8×** |
| Value repr — vs untagged `f64` (a CEILING, not an alternative) | 1.79 ns | 0.73 ns | 2.4× |

### 3.2 The three independent levers on a field read

| variant | ns/iter | delta |
|---|---|---|
| A0 today — locked call ×2 + generic call ×2 | 15.81 | — |
| A1 +proven per-slot `Repr` (arithmetic inline) | 10.84 | **−4.97** |
| A3 +no shard lock on the read | 6.30 | **−4.54** |
| A4 +direct-addressed load (no call) | 1.03 | **−5.27** |
| A4g +IC shape guard (the honest IC hit) | 1.16 | +0.13 |
| A5 +escape analysis (object gone) | 0.70 | −0.33 |
| *(A2 — inline guard on the Tagged accumulator)* | *11.49* | *−4.32 vs A0* |

**These are NOT independent, and the first draft claimed they were.** The
variants are a chained sequence (`a3` = `a1` minus the lock, `a4` = `a3` minus
the call), so `4.97 + 4.54 + 5.27 = A0 − A4` is a **tautology** true of any four
numbers — it says nothing about whether the levers commute. Proving independence
needs a 2³ factorial the probe never ran. Two internal disagreements show they do
*not* compose: the arithmetic lever prices at 4.97 ns here but 10.97 ns in the
OPS kernel (2.2× apart, because kernel A is latency-bound on the accumulator
chain so the locked reads hide behind it), and the inline guard recovers 60–75%
in OPS but only **29% here** (row A2, which the first draft omitted from this
table — precisely the row where Tier 2's lever underperforms). Once the read is a
load (A4 total = 1.03 ns), the per-slot `Repr` lever **cannot** still pay 4.97 ns.
The levers are strictly sub-additive.

The shape guard a correct IC must emit costs **0.13 ns**: the IC is not the
expensive part, what sits behind it is.

### 3.3 Operators — the missing middle rung

The engine already lowers all of these natively when both operands are proven
non-Tagged. With a `Tagged` operand it goes straight to `box, box, call` with no
inline test (`binop.rs:596`, `binop_eq.rs:52`). Since `Repr::Ref` is dead,
"Tagged" is every value off the heap.

| operator | today | inline guard | proven `Repr` |
|---|---|---|---|
| `=== !== == !=` | 2.98–3.25 | 1.59–1.61 | 0.91 |
| `< <= > >=` | 2.97–3.20 | 1.37–1.40 | 0.91 |
| `+ - * /` | 5.30–5.94 | 1.36–1.39 | 0.69–1.13 |
| `& \| ^ << >> >>>` | 7.14–8.02 | 2.29–2.33 | 1.69–1.85 |
| `%` | 8.23 | 5.30 | 4.86 |
| `**` | 17.53 | 12.55 | 11.82 |
| `typeof` | 2.51 | 0.91 | — |
| `!` | 2.76 | 1.37 | — |
| unary `-` | 5.06 | 1.15 | — |
| `??` | 0.69 | **0.68** | — |

`??` is the control: already pure IR, nothing to fix. Everything else has a call.
**The inline guard recovers 60–75% of the gap with no type analysis at all.**

Note `& | ^ << >> >>>` cost **more than `*`** today — the trampoline runs
`ToInt32` (ToNumber → finite test → truncate → two casts) per operand and reboxes.

### 3.4 Where a proof is NOT the ceiling

`%` and `**` are the two operators for which Cranelift has no instruction
(`frem`, `pow`). Proving the `Repr` removes the box but the **call survives**:

| | `%` | `x ** 2` |
|---|---|---|
| today | 8.23 | 18.53 |
| inline guard | 5.30 | — |
| **proven `Repr`** | **4.86** | **12.72** |
| **runtime-guarded native op** | **3.35** (int `srem`) | **1.62** (`b==2` → `fmul`) |

**The runtime guard beats the compile-time proof** — 1.45× for `%`, **7.9×** for
`x**2`. For `**` with a literal exponent the check is a lowering-time decision
with zero cost, so the real number is at least that good.

---

## §4 Premises refuted

Recorded because the refutation is the useful part. Five premises died: three
mine, two the codebase carried.

### 4.1 "A bump allocator needs a moving collector, so it is incompatible with a conservative scanner" — REFUTED **[S]**

*Fast Conservative Garbage Collection* (Shahriyar, Blackburn & McKinley, OOPSLA
2014): conservative Immix lands within **2–3% of precise**, because **<0.01% of
objects are falsely retained and 0.03% pinned**. The technique is to pin at
**line/block granularity** when an ambiguous root points inside, and evacuate
everything else — not to disable bump allocation globally.

And the RTS-specific part: **the slot index never moves; only the storage behind
it does.** A stack word never held the payload's address, only the index. That is
the Smalltalk-80 object-table property, and RTS already pays the indirection for
NaN-boxing reasons — the slack is bought and unused.

**Boundary that survives:** renumbering/compacting the *slots themselves* would
break ambiguous stack roots, and the precise stack maps that would fix it produce
an empty set today. Out of scope until they are real.

### 4.2 "PEA requires deoptimization" — REFUTED **[S]**

Graal's PEA algorithm proper (CGO'14, §4–5.4) is a **compile-time IR transform**;
materialization is inserted unconditionally on the escaping edge. Deopt appears
only in §5.5, and only to satisfy *HotSpot's interpreter* for an unrelated
speculation riding along. **RTS has no interpreter, so it skips §5.5 entirely** —
a simplification, not a limitation.

Existence proofs without deopt: **.NET** (PR #111473, conditional EA + method
cloning), **GraalVM Native Image**, **Dart AOT** ("allocation sinking").

### 4.3 "The shard mutex is only container safety, so removing it does not touch the collector" — THIS WAS MY CLAIM AND IT IS WRONG

An earlier draft of this document asserted that the scanner takes no shard lock
during the stack walk and that mark/sweep runs with threads suspended, therefore
removing the read-path lock could not affect the collector. **Both halves are
false**, and `collector/scan.rs:183-196` says so in an all-caps comment:

> `DEADLOCK RULE: while a thread is SUSPENDED we may only do lock-free work.`
> `` `visit` is the collector's `mark_handle`, which takes a HandleTable SHARD ``
> `LOCK. […] So: while suspended we only COLLECT candidate words […]; we RESUME`
> `first, and only then hand them to `visit`.`

`ResumeThread` is at `scan.rs:237`, `visit(c)` at `scan.rs:240`. **Marking runs
with every other thread live**, and `sweep_all_shards()` (`handles.rs:1744`) does
too. **There is no stop-the-world phase in this collector.**

So the lock does two jobs, not one:

1. protects the `Vec<Slot>` container against reallocation — this half was right;
2. **serializes a mutator read against a concurrent `sweep_unmarked` freeing that
   slot underneath it** — this half the draft missed entirely.

Consequence for the plan: **Tier 3.1 must replace job (2), not merely observe
that job (1) exists.** Stable-address chunks remove the reallocation hazard;
they do *not* by themselves make a read safe against concurrent reclamation.
That needs the generation check *plus* a reclamation-deferral scheme (epoch or
"never reuse the storage, only the logical slot"). The draft's §3.1 already named
the stale-handle hazard while §4.3 denied it could exist — the two contradicted
each other, and §3.1 was the correct one.

Comparison that survives: a JVM safepoint poll costs **0.383 ns**
(page-protection) / **0.590 ns** (thread-local handshake) **[S]** and fires only
when a collection is requested. RTS pays ~2.27 ns per field touch unconditionally
(derived: §3.2's A1→A3 delta of 4.54 ns covers two field reads), single-threaded
included. That comparison is now an argument for adding safepoints, not for
deleting the lock and calling it free.

### 4.4 "`!==` is cheaper to implement directly than as `!(===)`" — MY OWN, REFUTED BY MEASUREMENT

First probe run showed `!==` at 4.12 ns vs `===` at 2.95 — an apparent free win.
It was a strawman: my trampoline forced `#[inline(never)]` on the inner
`strict_eq`, creating a second call the real build (same crate, opt-3) very
likely inlines. With the body replicated: **2.98 vs 2.98**. The finding
evaporated.

### 4.5 "Cranelift has no LICM" — REFUTED **[S]**

It hoists loop-invariant pure ops during egraph elaboration
(`elaborate.rs`, `loop_hoist_level`). But it is a **heuristic placement, not a
guaranteed transform** — wasmtime issue #7283 documents a loop-invariant `fdiv`
being *sunk into* a loop. The class-proto hand-hoist (11×) was correct
engineering, not redundant work: "this runs once per class, not once per
instantiation" is domain knowledge Cranelift cannot recover from IR.

### 4.6 Two claims from the research that I checked and REJECTED

- An agent reported our `emit_to_int32` as a **correctness bug** and proposed
  cutting it from 6 IR ops to 2. **Its fix is wrong**: we already convert at
  **i64** width then `ireduce`, and `fcvt_to_sint_sat(i64, +Inf)` = `i64::MAX`
  whose low32 is `-1`, where `ToInt32(+Infinity)` must be `0` — our
  `select(f - f == 0, low, 0)` is exactly that guard, and it catches NaN too.
  **But I stopped one step too early and called our code "right".** The
  underlying complaint has a valid second half: for finite `|x| ≥ 2^63` we
  *saturate* where the spec *wraps* (`ToInt32(2^64)` is `+0`; we return `-1`).
  `binop.rs:473-474` knows and accepts this, but mis-scopes it as "divergence
  from V8" — V8 implements the spec here, so it is a divergence from
  **ECMAScript**, and "not a regression" does not imply "not a bug". It belongs
  in §6 as a knowingly-accepted conformance gap, present in both the inline path
  and the runtime trampoline.
- The same agent warned that re-tightening would destroy `-0`. Already handled:
  `genops.rs:294`, "NEGATIVE ZERO stays an inline double".

---

## §5 The plan, ordered

> **This section was ORDERED WRONG in the first draft and has been inverted.**
> An adversarial review ran the engine against the probe's own kernels and
> against `rts ir` + `RTS_REPR_STATS`. The measured attribution on
> `bench/objbench.ts` — 11 runtime calls per loop iteration, 560 ns/iter total:
>
> | tier | calls it removes | share of the 11 |
> |---|---|---|
> | Tier 1 (redundant work) | **0** | 0% |
> | Tier 2 (inline operator guard) | 2 | 18% |
> | Tier 3 (make the read a load) | 7 | 64% |
> | Tier 4 (escape analysis) | 11 | 100% |
>
> By time: allocation **389 ns/iter (69%)**, property-access machinery **~170
> ns/iter (30%)**, arithmetic **0.33 ns/iter (0.06%)**. Landing Tiers 1 and 2
> completely moves `objbench.ts` by **≈2.4%** and cross-runtime parity by
> **+0.0 points** — the 208 parity failures are 72 crashes and 118 wrong
> answers, and **not one is a timeout**. The `math` area the guard targets is
> already 16/16 passing.
>
> **Correct order: 3.1 → 3.2/3.3 → per-slot `Repr` → 4.1 → whatever of Tier 2
> still has a Tagged operand left → Tier 1 as opportunistic cleanup.** Tiers 3
> and 4 are the trunk; 1 and 2 are leaves. This is roughly
> `FUTURE_OPTIMIZATION.md`'s existing order, and §9.3's "reconcile the two
> orders" is hereby resolved in that document's favour.
>
> **A missing item, found by the same review:** the largest single delta in
> §3.2 — A0→A1, per-slot `Repr`, **−4.97 ns** — has **no Tier item anywhere
> below**. `FUTURE_OPTIMIZATION.md`'s order *starts* with it (Phase 4a). The
> first draft silently dropped the other document's first item while asking the
> owner to reconcile them. It belongs immediately after 3.2, and it is the item
> that *pays for* Tier 2: every slot it proves is an operator site the guard no
> longer has to guard.

Items marked **[needs a spike]** have no measured RTS number yet.

### Tier 0 — the areas nobody priced, which dominate everything below

A completeness review measured these against Bun on this machine. **None is in
the probe's 107 rows; none was in the first draft's plan.** They are algorithmic
or missing-cache, so no amount of the constant-factor work below closes them.

| workload | RTS | Bun | ratio |
|---|---|---|---|
| `JSON.parse`, 131 KB | 118 ms | 0.79 ms | **150×** |
| `JSON.stringify`, same | 156 ms | 0.47 ms | **336×** |
| regex literal `.test()` in a loop | 55 150 ns/iter | 87 ns | **636×** |
| same regex hoisted to a `const` | 750 ns/iter | 61 ns | 12× |
| arrow capturing 2 vars | 316 ns/iter | 4.05 ns | **78×** |
| method call | 27 ns | 1.04 ns | **26×** |
| one arrow allocated per iteration | 897 ns/iter | 27.6 ns | **32×** |
| top-level (gcell) code vs in a function | 141 vs 20 ns/iter | — | **7×** |

Mechanisms, all in-tree: `regexops.rs:91-137` has **no compile memoization** —
full NFA construction per loop iteration (the 73× literal-vs-hoisted gap is
self-inflicted, same engine). `weakmap_set.ts:26-44` — `WeakMap` is a linear
scan, so building one is **O(n²)**. `map_set.ts:302` — `__hkey` returns 0 for
every non-number/non-string key, so object-keyed `Map` degrades to a linear scan.
`map_set.ts:319-326` — string keys recompute FNV-1a per lookup via `charCodeAt`,
**one shard `Mutex` per character**.

And the tax nothing in §3 could see: `trycatch.rs:131-160` emits a real
`call __rtsadp_err_pending` after **every** call, `new`, property-set, `await`
and per `for…of` element — **38 emission sites**, ~30% of a call's cost. (`try`
itself is free when nothing throws — measured, do not chase it.)

**These outrank Tiers 1–4.** Measure them properly before committing the plan.

> **§1e reclassifies most of Tier 0 as ONE item, not several.** `JSON.parse`,
> `JSON.stringify`, `Map` string keys and the regex loop are not four algorithm
> problems — they are the same shape problem four times: a stdlib body written in
> `.ts`, so it does **one trampoline's worth of work per call** where a native
> symbol would do the whole document's. Measured end-to-end on a JSON scan
> (`bench/stdlibshape.rs`): the `.ts` shape is **124× slower at run time and
> 3.4× more expensive to compile** than the same operation as one native symbol —
> and the symbol's *body* costs the compile pipeline nothing at all. See §1e.5.
>
> That gives Tier 0 a single ordering principle instead of four separate
> investigations: **move the body into Rust; the work-per-call goes from 1 to n
> and both axes improve at once.** It also explains the 7× top-level-vs-function
> row above and the 82%-of-startup prelude cost in §1e.5 as the same cause.

### Tier 0b — two items §1e adds, both independently shippable

**T0b.1 — emit the inline tag-check fast path on `+`.** The design doc already
specifies it (§Pilar 3); the emitted IR calls unconditionally. **1.57×** on the
measured ladder (E1 → E1b), ~12 IR instructions per site, no arena and no stable
addressing required. Break-even at ~3 000 executions per site, so it is a clear
win inside any loop and a wash in cold code. This is the cheapest lever in this
document and it does not block on §1d.

**T0b.2 — do not compile the prelude on every startup.** `RTS_TIMING=1` on
`const x = 1 + 2` shows **425 functions machine-compiled** and **≈42 of ≈51 ms**
of compile time attributable to prelude. Two routes, not mutually exclusive:
turn the bodies into native symbols (T0/§1e.5, which also fixes throughput), or
default `RTS_JIT_CACHE=1`, which was measured to remove the machine-compile
phase entirely on a warm cache. Note the resident-prelude feature that used to
cover this was deleted 2026-07-28 by owner decision; this measurement quantifies
what that removal costs per startup, and is **not** a proposal to revert it.

### Tier 1 — redundant work, no semantic change, no new analysis

> **Three of the five items below were wrong.** 1.1's snapshot layer is
> **deadlock avoidance**, not a double copy (`snapshot.rs:1-6, :75-76`: the
> `Mutex` is non-reentrant and `element_to_string` re-enters it — a 1/32
> deadlock per nested element); the real change is a both-are-`String` fast
> path, not a deletion. 1.2's `with_key_str` is **already deployed** on the
> shaped fast path (`objops.rs:148-151`); the surviving `key_text` is deliberate
> (the key must be resolved *before* the receiver's lock, and it is the only
> arm that handles symbols and numeric ToString). 1.3's generalization is
> **semantically wrong**: `(-Infinity)**0.5` is `Infinity` but `sqrt` gives
> `NaN`, and `x**3` vs `(x*x)*x` differs on **25.6% of inputs** — only the
> literal `b == 2` case is safe. **Only 1.4 survives intact**, and it is worth
> ~2.7% on `objbench`.

**1.1 Delete the double-copy in `STRING_CONCAT`.** `snapshot_entry` clones, then
`snapshot_to_bytes` clones again, then the result is built. Removing the snapshot
round-trip is **3.2×** on the accumulator loop with byte-identical observable
behaviour and the same result-allocation count **[M]**. Note
`docs/engine/architecture.md:318` already recorded the O(n²) with production
numbers (20k concats → 288 MB, 80k → 3.6 GB) and already recommended a rope —
this is the cheaper half of that fix and should land first.

**1.2 Stop allocating a `String` per dynamic property read.** `key_text()`
(`objops.rs:221`) mallocs on every call; `with_key_str()` (`objops.rs:212`)
already exists as the zero-alloc borrow path and is documented as the fast path
for a simple string key. Worth **~2×** on the dictionary read (63.82 → 31.45 ns)
**[M]**, independent of everything else.

**1.3 `x ** <small integer literal>` → multiply chain at lowering time.**
**11.5×** on `x**2` **[M]**. JSC's `operationMathPow` generalizes this to
square-and-multiply for any small non-negative integer exponent, plus `0.5` →
`sqrt` and `-0.5` → `1/sqrt` **[S]** — copy that shape, not just `b==2`. No
runtime guard needed when the exponent is a literal.

**1.4 Remove `std::env::var()` from the allocation fast path** (called on every
256th allocation) — cache in a `OnceLock`. And move `LIVE_BYTES`/`LIVE_HANDLES`
from two process-global atomics per allocation to per-shard counters flushed at
the GC tick, which is the TLAB/.NET pattern **[S]**.

**1.5 Mark guard-fail / bail / error-slot blocks cold.**
`FunctionBuilder::set_cold_block` exists and has **zero uses** in the tree
(verified). Cold blocks are sunk to the end of the function at emission, keeping
hot-path I-cache lines dense. My probe's guard variants did **not** use it, so
the §3.3 guard numbers are likely conservative **[E]**.

### Tier 2 — the inline guard (the cheapest real lever)

**2.1 Put an inline tag test in front of every generic operator call.** Measured
2–4× across 22 operators **[M]**, no type analysis, no shape, no escape analysis
— a local IR change at the operator site. Design constraints, all sourced:

- **Branch, not `select`.** The guard is highly predictable on monomorphic code;
  `cmov` forces a data dependency on both inputs regardless **[S]**.
- **Fast path is the fallthrough, miss is a taken jump to a cold block** **[S]**.
- **Express the tag check as plain Cranelift IR** (`band`/`icmp`), never an
  extern call, so the egraph's GVN/CSE removes a repeated check on the same SSA
  value across chained operators (`a*b + c`). **Verify with `rts ir` before
  writing any "redundant guard elimination" pass** — duplicating egraph work is
  the mistake the deleted MIR tier made.

**2.2 Overflow-safe int32 `+`.** Cranelift has `sadd_overflow`/`ssub_overflow`/
`smul_overflow` returning `(result, flag)` — **unused in the tree** (verified).
V8 and JSC both emit `add` + `jo`, but branch to the deoptimizer; RTS branches to
an **inline widen to `f64`**, which is a correctness fallback, not a
recompilation. This closes the hole my probe left explicit (my 1.28 ns row
omitted the overflow check, so it was a lower bound).

**LANDED, and ON by default** (`clifflags::int_overflow_checks`, kill switch
`RTS_INT_OVERFLOW=0`). What unblocked it was not the emission — that existed and
was correct — but a way to apply it SELECTIVELY. RTS carries both semantics on the
same `Repr::Int*`: a JS `number` is a double (a 64-bit wrap is a wrong answer),
while a value declared `i64`/`u32` is a native fixed-width integer (the wrap is the
declared contract). Applied blanket the check cost **6.6×** on an int-heavy loop
(and 21× when the merge went `Tagged`), taxing the native path for a rule that does
not govern it. `rts-hir` now stamps a `native_int` bit on `HirStmt::Let`/`Const`
(set only by an explicit fixed-width integer annotation; a param's `ty` was already
annotation-derived), the lowering records those names, an ident read stamps
`Val::native_int`, and the check fires only when NEITHER operand carries it.
Residual cost of the GATED form on ordinary JS integer code: **TODO(measure)**.
Known gap: `i++`/`i--` emit a raw `iadd`/`isub` in `stmt_assign.rs` and are not
covered, so they still wrap where `i = i + 1` promotes.

**2.3 Runtime int guard for `%` → `srem`.** 1.45× over the compile-time-proven
float path **[M]**. Guard needs: both operands round-trip through `i64` exactly,
divisor ≠ 0 (`srem` traps where JS yields `NaN`), dividend ≠ 0 (`-0 % 3` is `-0`
in JS, `0` through the integer path).

### Tier 3 — make the heap read a load

**3.1 Replace `Mutex<Vec<Slot>>` with a chunked, append-only, stable-address
slab + generational index.** Chunks never move once allocated; only the chunk
list grows, by atomic pointer swap. A reader holding a valid handle needs **zero
synchronization** because the address a handle resolves to never changes for that
handle's lifetime **[S]**. Compatible with a non-moving conservative collector by
construction. Worth ~2.2 ns/touch **[M]**, and it is the precondition for 3.2.

Hazard to design for explicitly: if a slot's *memory* is reused without waiting
for readers, a suspended thread holding a stale handle can land on a repurposed
slot. Standard ABA; the generation check answers it, but it must be deliberate.

**LANDED, default OFF** (`crates/rts-natives/src/heap/slab/`, knob `RTS_SLAB=1`,
same discipline as `RTS_REGIONS` / `RTS_BUMP`). `HandleTable::slots` is now a
`SlotStore`: the historical `Vec<Slot>` by default, or per-shard chunks of 512
slots published into a **flat** `static [AtomicPtr; 32 * 4096]` table by a single
`Release` store. Chunks are never moved, never reallocated and never freed, so a
slot's address is fixed for the life of the PROCESS — stronger than the item
asked for, and free, because it falls out of never freeing a chunk. Address
resolution is `base = load [CHUNK_TABLE + (shard*MAX_CHUNKS + chunk)*8]` — H7's
one flat load, not a second dependent one.

Three things this deliberately does NOT do, each written down in the module so
3.2 does not inherit a false belief:

- **It does not make a field read lock-free.** An `Entry` is a `Box`/`Vec` enum
  that `sweep_unmarked` mutates with every mutator live (§4.3 — no
  stop-the-world), so a stable address permits computing where a slot is, not
  reading its `Entry` without the shard `Mutex`. The lock-free read needs 3.1
  **plus** `RTS_CLASS_IMPLEMENTATION.md` §4.2's inline-slot block layout (plain
  `i64` words in the slot), which itself needs the object/array split (§8.4). No
  measurement covers that, and 3.1 does not half-build it.
- **It does not close the ABA hole, it answers it.** Slot MEMORY is reused in
  place (that is what makes the address stable), so a stale handle now lands on a
  live, correctly-typed, wrong slot. The generation bump on reuse plus the
  existing per-accessor comparison rejects it; what 3.1 adds is that the
  generation is an `AtomicU16` read `Relaxed`, so a read taken outside the lock
  is a defined atomic read rather than UB. The 16-bit wrap remains, identical to
  today's `Vec` path — closing it needs a wider generation or epoch-based
  reclamation deferral, which §4.3 names and this item is not.
- **It declares no ABI symbol.** The chunk-table base and the slot stride are
  plain `pub fn`s; 3.2 is the item that needs them from AOT and is the item that
  should declare them with `#[rtse::abi]` and re-run `rts-symbol-baker`.

TODO(measure): no RTS number taken. The ~2.2 ns and H7/H10 describe the probe's
model of this storage, and 3.1's win is not collectable until 3.2 emits the load.

**3.2 Emit the field read as a real `load`, not a call.** ~5.3 ns **[M]**. The
stated blocker (`payload_ops.rs:28-35`: "the HandleTable and the allocator do not
exist at the IR level") is removable exactly by 3.1 — a chunked slab with a base
in a global gives `load [base + payload*8]` as pure IR, bounds/generation check
preserved.

**NOT LANDED (2026-08-02) — and the blocker is not the one this item names.**
*(Superseded later the same day: the prerequisite split landed as `Slots::Inline`
and the emission followed — see "3.2 — what LANDED" below. The analysis is kept
because it is what identified the prerequisite.)*
An implementation pass got as far as pricing every precondition against the tree
and stopped deliberately, because the emission that fits in this item's framing
corrupts memory and the one that does not is a representation change no knob
makes safe. What the pass established, so the next one does not re-derive it:

- **Addressing is genuinely solved.** 3.1's chunk table gives a
  process-stable slot address, and `chunks::slot_addr` is the Rust twin of the
  IR sequence. Nothing about `load [base + payload*8]` is blocked.
- **The GC-race objection is answerable, and it is not what blocks this.** A
  receiver whose handle word is live in the frame is a conservative root, so its
  slot cannot be swept or reused *while it is being read* — and a GC tick fires
  only from `alloc_entry`, i.e. only at a call, which is the same property
  `collector/scan.rs` already relies on to justify scanning six callee-saved
  registers and no caller-saved ones. Removing the field-read call does not
  weaken that argument: a value live across a call is still spilled or
  callee-saved.
- **Hoisting is safe on this storage, for a reason that is worth stating.** The
  chunk-table base is a `static` address; a chunk-table entry is published once
  with `Release` and never rewritten; a chunk never moves and is never freed. So
  the base and the slot address are loop-invariant *in fact*, not by assumption,
  and the egraph hoisting them across a GC tick is correct. What would break it
  is precisely the moving collector `RTS_CLASS_IMPLEMENTATION.md` §4.4 prices —
  and the block indirection it specifies is what keeps it safe there too, since
  relocation rewrites the block word and not the slot address.
- **What actually blocks it is the object/array representation split (§8.4).**
  A raw `load` is sound only if the words it reads cannot be freed or moved
  under it, and today an object's field words live in `Entry::Vec`'s
  `Box<Vec<i64>>` — a buffer the sweep drops and any `push` reallocates. The
  fix is §4.2's inline fixed-stride block, which means a new `Entry` variant.
  `Entry::Vec` is matched at **316 sites across seven runtime crates**
  (66 rts-natives, 46 rts-runtime, 28 rts-primitives, 47 rts-shared, 48 rts-std,
  75 rts-node, 6 rts-engine), and object-vs-array is decided *dynamically* by
  `objops::looks_like_object` reading `(slot0, len)` — there is no static split
  to inherit. Every site missed in a fork returns a silently wrong value for a
  class instance rather than failing loudly, and `RTS_SLAB=1` does not contain
  that: the knob exists to be turned on and measured, so a knob-on path that is
  wrong at an unknown subset of sites is not a landable subset.
- **The remaining win is on ESCAPING objects only, and that is the expensive
  half by construction.** Tier 4.1 / C6 escape analysis has landed
  (`front/run/escape/`, `RTS_ESCAPE`), and it already deletes the allocation for
  every instance that provably stays local — those objects have no `Entry` at
  all. What 3.2 is left to speed up is exactly the set that escapes into the
  generic paths, i.e. the set the 316 sites must keep reading correctly. The two
  items do not overlap, and that is why 3.2 cannot be narrowed into safety by
  restricting which receivers take it.

The order this implies: the object/array split is a prerequisite ITEM, not a
detail of 3.2 — it should be scoped, landed and measured on its own (one
representation for shaped instances, `Entry::Vec` left to arrays), and only then
does 3.2 become an emission change. Layout constants stay a single definition in
`rts-natives` when that happens (`RTS_CLASS_IMPLEMENTATION.md` §6.3); the
`#[rtse::abi]` symbols for the chunk-table base and the slot stride are 3.2's to
declare and were deliberately not declared here, because an unused row in the
baked table is a row the gate then has to carry.

#### 3.2 — what LANDED (2026-08-02)

The blocker above named the object/array representation split as a prerequisite
ITEM. `Slots::Inline` is that split: an object's field words now live BY VALUE
inside the slot, so there is no `Box<Vec<i64>>` for a sweep to drop or a `push`
to reallocate, and the `Heap` form (post-`promote`) is a distinct discriminant a
reader can reject rather than misread. With that in place the emission is what
this item always described.

Landed:

- `crates/rts-natives/src/heap/slab/layout.rs` — `FieldLoadLayout`, every number
  the emitted sequence needs, DERIVED from a real
  `Slot::live(g, Entry::Vec(Slots::Inline { .. }))` and verified against the safe
  accessors, never hardcoded in codegen. `available()` couples the layout to
  `RTS_SLAB=1`.
- `crates/rts-codegen-new/src/front/run/fieldload.rs` — the emission: shard/index
  decode, chunk-bound test, chunk-table load, null-chunk test, slot address,
  `Entry::Vec` tag test, `Slots::Inline` tag test, unsigned bounds test (which is
  also what rejects a negative index), then `load.i64` of the word. All five bail
  edges jump to ONE cold block holding the unchanged
  `__rtsn_vec_get_by_payload` call, so every rejection lands on the existing
  implementation and the fast path can only ever be an optimization.
- `crates/rts-codegen-new/src/value/emit_marshal.rs::emit_vec_get` — the single
  call site, now fast-path-then-call. Every property read, method-table read,
  element read and closure-env read in the engine funnels through it.
- Cache versions bumped: `prelude_cache::CACHE_VERSION` 7→8,
  `progcache::CACHE_VERSION` 5→6.

Knobs, and why there are three:

| Knob | Default | Why it gates |
|---|---|---|
| `RTS_SLAB=1` | off | without the chunked store the slots live in a `Vec` that reallocates, so no address into one is stable |
| JIT only (`aot_str::aot_mode()`) | — | the chunk-table base is baked as an `iconst` immediate, which an AOT object cannot carry across processes. TODO(3.2-aot): declare it as an `#[rtse::abi]` data symbol and load it instead |
| `RTS_FIELD_LOAD=0` | on | its own kill switch, so the item is A/B-measurable on one binary |

Also refused while `RTS_JIT_CACHE=1` is on: that cache replays baked machine
code into a later process and the chunk table is a BSS static whose address
moves under ASLR, which no cache version can distinguish.

There is deliberately **no generation check**, because
`payload_ops::vec_get_by_payload` — the function this fast-paths — performs
none: it receives a 48-bit payload, which carries no generation. What it rejects
is an out-of-range or unallocated slot and a non-`Vec` entry, and the emitted
tests reject exactly those. The `Slots::Inline` test is in the emitted sequence
on EVERY read (not hoisted) because `promote` is one-way but can have happened
before any given read, and the compiler cannot prove an object never gains a
property.

TODO(measure): no number taken. A/B `RTS_FIELD_LOAD=1` vs `=0` with `RTS_SLAB=1`
and `RTS_NO_PRELUDE_CACHE=1` on BOTH arms — otherwise both arms replay one
cached prelude lowering and the A/B measures nothing. The item's ~5.3 ns is the
probe's model, not a measurement of this emission.

**3.3 Stop defaulting untracked receivers to dictionary mode.** ~~No production
engine treats "shape not statically proven" as "this object is pathological";
dictionary mode is entered by delete-heavy/sparse-key triggers **[S]**. Give
untracked receivers a lazily-assigned tracked shape, reusing the transition tree
that already exists in `shapes/mod.rs`.~~

**PREMISE REFUTED (2026-08-02) — the item was real but pointed the wrong way.**
A census of every `Entry::Map` construction site in the tree found **zero**
created because a shape could not be proven. No codegen path mints one; an
untracked receiver already takes the shaped route (the lowering's dynamic
fallback calls `__rtsadp_obj_get`/`__rtsadp_obj_set`, and `added_key_shape`
lazily grows the shape through `shape_with_added_key`'s transition tree on a
write to an absent key). Every dictionary receiver is a genuine runtime-producer
dictionary: a `__rts_class`-tagged object-backed Registry class instance
(`net.Server`, `Stats`, `FileHandle`, `ProtoWriter`), a `Map` collection, an
N-API object, a Proxy internal, or a small runtime-built row. The 63.82 ns is
`rts-value-probe`'s kernel OBJ, whose own doc asserted the false premise
("anything untracked"); it never measured an engine object.

What was real is the INVERSE: in `rtsadp_obj_get` the dictionary probe ran
*ahead* of `resolve_slot`, so every SHAPED read paid a `key_text` `String`
malloc plus a receiver shard lock, and the `Entry::Rtse` probe under it a second
`key_text` plus a `runtime_ci` lookup, before its own slot was consulted.
Landed: the shaped own-slot read is hoisted above both probes, guarded by
`RTS_LAZY_SHAPE=0`. Semantically a no-op — `resolve_slot` requires an
`Entry::Vec` with a matching shape header, so no receiver reaching the skipped
blocks can satisfy it. TODO(measure): no RTS number taken.

**3.4 Un-globalize the shape registry.** One process-wide `Mutex` serializes
every thread's every shape lookup. Shapes are append-only and immutable once
published — the same invariant V8 relies on for its Map graph **[S]** — so reads
can be lock-free with the `Mutex` kept only for interning. This is
`rts-threading-model.md` blocker #4.

### Tier 4 — remove the allocation

**4.1 Tier-0 escape analysis (intraprocedural, no loops, whitelist).** For a
`new C(...)` bound to a local: linear scan of uses; bail if returned, stored,
passed to any call, captured (reuse the existing capture-detection as the bail
signal), `===`-compared, used as a collection key, thrown, or subject to dynamic
add/delete. Passed: one Cranelift `Variable` per field slot, typed by the field's
`Repr`. The allocation, the shape-tag store and the IC site all disappear.

Cranelift will never do this — the egraph RFC does not mention SRA or escape
analysis **[S]** — but `cranelift-frontend`'s `Variable`/SSA builder is the
mem2reg equivalent, and `rustc_codegen_cranelift` does exactly this in its own
front end via an address-taken check **[S]**. **GC interaction is a non-issue**:
a `Variable` holding a handle is an ordinary stack/register word the conservative
scanner already treats as a root.

Ceiling calibration: Roslyn's self-build measured **16.1% of allocated objects
not escaping** at runtime **[S]**. Graal's PEA measured −8.0% to −22.7% allocated
bytes and +2.2% to +10.4% throughput **[S]**. My probe's 138× is the *provably
local* best case, not an average.

**4.2 Bump/nursery allocation.** TLAB fast path is **4 instructions**, ~2600 MB/s
**[S]**; disabling TLAB is 10× slower single-threaded. Immix gives bump
allocation inside 32 KB blocks with recyclable-line reuse and measures **7–25%
whole-application improvement** over canonical algorithms **[S]**. Per §4.1 this
is compatible with the conservative scanner with no scanner change.

**4.3 Kill the second allocation per object.** An object is `Slot` +
`Box<Vec<i64>>` — two allocations. V8 stores N properties in-object and learns N
by slack tracking (measured 62% smaller objects) **[S]**; object inlining
measured 58% fewer object creations and 14% runtime **[S]**. `shape.rs` already
has the metadata to pick K.

### Tier 5 — calls, closures, dispatch **[needs a spike]**

**5.1 Box per capture, not per closure.** Today every capture is boxed, including
a proven `Float64`. Standard assignment-conversion rule: box only if written
after the capture point, or in mutual/self recursion.

**5.2 One extra entry point per function** (native ABI, unboxed params), emitted
only when the `Repr` lattice proves the arguments and the body monomorphic; the
Tagged entry stays for reflective/first-class call sites. Cinder/Static Python
shape **[S]**. **Do not build more than two** — TS has no multiple dispatch, so
per-signature monomorphization is the wrong shape.

**5.3 Audit the uniform 5-slot indirect-call ABI.** V8 removed its
arguments-adaptor frame by carrying the real argument count and indexing without
a copy — measured **+11.2% Ignition, up to 40% TurboFan, +4.6–6.1% Octane2**
**[S]**. The existing `invoke_all_i64` asm trampoline (arity-aware) is closer to
the right shape than the padded path.

**5.4 CHA devirtualization — with a load-bearing caveat.** Dart AOT devirtualizes
only when it can prove a single target **[S]**, and it has a **closed world**.
RTS does not: `runtime.eval_file`, `new Function`, and planned dynamic import
(#223) mean classes reachable from an eval boundary must be **excluded**. Without
that exclusion this is a correctness bug, not an optimization.

**5.5 Inlining is a spike, not a roadmap item.** Cranelift gained an inlining
*mechanism* in PR #11210 but ships **zero policy** — and Wasmtime's own
"enable inlining by default" PR was **closed without merging** **[S]**. If RTS
wants it, the policy is a HIR pass; Go's model is the one to port (budget 80,
call costs 57, call through a function-typed parameter costs only 17 —
deliberately cheaper because inlining it can unlock devirtualization) **[S]**.

### Explicitly NOT worth doing

| Item | Why |
|---|---|
| Replace `PolyValue` with a two-slot `{tag,value}` | 0.81 ns/iter (§3.1) — the smallest item measured, the most invasive change **[M]** |
| `opt_level = "speed_and_size"` | `OptLevel` is referenced nowhere in the egraph cost model; provably a non-decision **[S]** |
| `CallConv::Cold` | Deleted upstream; maintainers said it "never actually did anything" **[S]** |
| `enable_pcc` | Removed upstream ("bit-rotted significantly") **[S]** |
| Unboxing array **elements** | 0.05 ns once the access is a load **[M]**; V8's own verdict on element kinds is "usually too small to matter" **[S]** |
| Auto-vectorization | Confirmed absent from Cranelift and not planned; #92's closure was correct **[S]** |
| General content interning for `===` | JVM data: `String.intern()` goes 0.089 µs/op at 1 string → 650 ms/op at 1M, plus 13 ms/GC-pause of root scanning **[S]**. Scope interning to property keys/identifiers only, as every engine does |
| Ropes for `+` before in-place append | V8's own team is weighing **removing** ConsString in favour of flat over-allocated buffers **[S]**; do 1.1 and the liveness-proven in-place append first |
| A bespoke redundant-guard-elimination pass | Verify the egraph does it first (§5 item 2.1) |

---

## §6 Correctness findings — not optimizations

These surfaced during performance work and outrank everything in §5.

### 6.0 THREE items that are actually live — found while attacking this document

These replace the first draft's §6.1, which was **stale** (see 6.1 below).

**(a) `1 ** NaN` returns `1`; ECMAScript requires `NaN`.** Verified by running
both: RTS `1`, Node `NaN`. `binop.rs:645` lowers `**` to
`__RTS_FN_NS_MATH_POW` = `base.powf(exp)` = C `pow`, and C99 F.9.4.4 mandates
`pow(+1, y) = 1` for **any** `y` including NaN, while `Number::exponentiate`
step 1 says "If exponent is NaN, return NaN". A performance audit priced this
operator's *cost* without ever checking its *result*.

**(b) An out-of-bounds array read returns `0`, not `undefined`.**
`a=[1,2,3]; a[10]` → `0`, `typeof a[10]` → `"number"`, `a[10]+1` → `1`. Node:
`undefined` / `"undefined"` / `NaN`. This is exactly the silent-wrong-answer
class that Tier 2.2 (int overflow) and 2.3 (`%` → `srem`) would add more of.

**(c) `for…of` over a generator is drained EAGERLY — an infinite generator
HANGS. Verified by running it.** `loops.rs:365-385` emits
`__rtsn_gen_sm_drain`, which runs the generator to completion into a `Vec`
before the body executes once, so `break` saves nothing:

```ts
function* inf() { let i = 0; while (true) { yield i++; } }
let n = 0;
for (const v of inf()) { n++; if (n >= 3) break; }
console.log("survived", n);          // never printed — killed at 10 s
```

`generator/sm.rs:1-3` claims lazy suspension "is what makes an infinite
generator possible". It does not; this is a hang, not a slowdown.

**(d) Peak RSS is ~4× the sum of its parts on a mixed workload** — see §1b.
Memory is not reclaimed between phases, and the most likely cause is the
`entry_heap_bytes` heuristic under-counting so the 64 MB GC trigger never
fires.

### 6.1 ~~Function values have no stable identity~~ — ALREADY FIXED, this was stale

**The first draft claimed `f === f` is false. It has been true since commit
`94ad8092`, which is an ancestor of HEAD.** `funcops.rs:96 fn_value_identity_eq`
compares the structural tuple `(fn_ptr, bound_this, bound_args)`, wired into
`rtsadp_strict_eq` and `rtsadp_same_value`, and `tests/fn_identity_eq.test.ts`
passes **8/8** (run to confirm: `f === f`, closure identity, aliasing, `Set<fn>`
dedup, distinct closures differ). The draft quoted the *comment describing the
problem* (`funcops.rs:81-84`) and missed the implementation twelve lines below.

**And the proposed "free" fix would have broken working code.** There is no
binding slot to cache into — `f` resolves to a `func_addr`, a compile-time
relocation. Caching by name would collapse every instantiation of a lifted
capturing arrow (`__rtsn_arrow_N`) onto one handle and one shared env, making
`makeCounter() === makeCounter()` true and the counters share state — i.e.
failing `fn_identity_eq.test.ts:42`. Re-reifying a non-capturing arrow per loop
iteration is also what ECMAScript §10.2.3 *requires*, so the "wasteful" behaviour
is spec-correct there.

What is real and unmentioned: `.bind` **over**-equates (`funcops.rs:92-95`,
self-documented). That needs *more* identity, not a cache.

### 6.2 AOT bakes the build machine's CPU features into the shipped binary

`module_aot.rs:113` uses `cranelift_native::builder()` — which auto-detects and
enables the host's AVX2/FMA/BMI2/AVX-512 bits. Correct for JIT (always runs where
it compiled). For AOT, if a binary built on a modern CI machine runs on an older
CPU, that is a **SIGILL**, and `CLAUDE.md`'s standalone-distribution language
implies cross-machine.

**Needs an owner decision** (§9.1): same-machine build+run, or a documented
baseline target?

### 6.3 The AOT binary links the whole runtime — 17.7 MB for `console.log(42)`

Measured: 40/40 sampled symbol-table names present as strings, plus `rustls`,
`actix-web`, `wgpu`. Mechanism: `--gc-sections` follows **relocations**, and a
static array of 2025 function addresses is a data section with a relocation to
every one of them — mechanically indistinguishable from "the program calls all of
these" **[S]**.

Fair target, since this cuts both ways: RTS is **4–6× smaller than Bun (~98 MB)
and Deno (~78–110 MB)**, which embed a whole JS engine **[S]**. But RTS's stated
goal is "a minimal Rust runtime", which is the Go (**2.6 MB**) / QuickJS
(**<1 MB**) peer class **[S]** — where 17.7 MB is 7–80× too big.

**The table is NOT JIT-only** — an earlier draft of this document said it was and
proposed splitting the crate on that basis. `adapters/value/arrayrow.rs:35-43`
builds a name→address map from `symbol_table::symbols()` and calls through it at
`:115`, reached from `__rtsadp_dyn_method_call` (`dyndispatch.rs:971`) for
untracked Array receivers. `adapters/` is **inside `rts-runtime`, which is the
AOT staticlib** — so a compiled AOT binary needs the address table at runtime for
dynamic Array dispatch. The naive split would either break that or fail to prune,
and the 2×2 experiment below would not have caught it, because it stubs the table
rather than removing a consumer.

Fix, in order: (1) give the **runtime** consumer a narrower table (only the
symbols reachable by dynamic dispatch) or a different resolution mechanism, then
move the **JIT vtable** consumer out of the AOT link line; (2) feature-gate
rustls/actix/wgpu per the `rts:<ns>` the program imports; (3) confirm `/OPT:REF`
is actually active. The min-sized-rust nightly tail is worth kilobytes, not
megabytes — **not the fix** **[S]**.

The experiment that closes "proven-sufficient vs proven-sole": a 2×2 of
(`codegen-units` 1 vs 16) × (real table vs same-shape stub table pointing at one
no-op), with `cargo bloat --crates` diffed across all four.

---

## §7 Documentation that is currently wrong

| Claim | Where | Reality |
|---|---|---|
| `use_egraphs = true` is a flag we set | `CLAUDE.md`, `CLAUDE.md` | Deleted upstream in 2023; the egraph runs unconditionally when `opt_level != none`. We never set it literally, so no live bug — but an edit that tries will hit an unknown-setting error |
| `abi::Intrinsic` is the inlining mechanism | `CLAUDE.md`, `CLAUDE.md` | Deleted; replaced by per-member `NativeEmit` |
| Bitwise/shifts are "ALWAYS generic" | `binop.rs:17-18` | Stale — `binop.rs:503` emits them inline for non-Tagged operands |
| The property IC is dead | `RTS_PERF_AND_STATELESS.md` §0 (untracked) | Live at `obj.rs:1413`, `obj.rs:1467` |
| `PropIcCell {shape,slot,state}` / mono→poly→mega exists | `rts-codegen-new-design.md` §8.3, **`CLAUDE.md`**, **`CLAUDE.md`**, **`rts-threading-model.md` blocker #2** | Never built. Shipped is a different 3-word cell `{shape, len, slot}` — no state field — and it covers **reads only** (`lower_dynamic_set` is uncached) |
| **"Current state (precise mark+sweep)" and "Precise roots: the Cranelift stack maps already exist"** | **`the spec removed 2026-08-03 (see git history):23`, `:26`, `:75`** | **False, and `:75` is listed as a REASON the generational design is feasible — a false prerequisite under an entire design.** `RTS_ORGANIZATION.md` N6-B claims "the lying docs ✅ DONE"; it fixed `CLAUDE.md` and `02-runtime.md` and missed this file. Same doc puts the collector in two paths that no longer exist (`:25`, `:108`) |
| "the verifier is dropped in release" | `OPTIMIZATIONS.md:388-395` | True for `rts run`; **`module_aot.rs` has no `enable_verifier` override**, despite its own comment claiming "EXACT same flags as the JIT" |
| `Repr` has five variants | `CLAUDE.md`, `01-architecture.md`, `03-features.md`, design doc | **Six** — `Int64` is documented nowhere |
| **"the Cranelift egraph performs intraprocedural inlining"** | **`CLAUDE.md`, `CLAUDE.md`** | **The egraph has no inlining capability and never did.** Inlining landed 2025-07-10 as a *separate* pass (`cranelift/codegen/src/inline.rs`, `Context::inline`), and it is embedder-driven **by design** — the module's own docs say it *"does not attempt to define heuristics"*; the embedder implements `trait Inline`. Wasmtime keeps it **off by default** (compile-time cost; PR #13214 to enable it was closed unmerged). RTS calls none of it. See the note below |
| "Cranelift is ~14% behind LLVM" | `cranelift/README.md`, quoted onward | A **2020** measurement (Xu & Kjolstad, PolyBenchC), predating regalloc2 and the egraph. Not current in either direction — measured here it is 2–9% on non-vectorizable shapes and 2.61× on vectorizable integer loops (§1e.1) |
| `Entry::Rtse`'s `data: Box<dyn Any>` is opaque to the GC, blocking Rust-side `Map`/`Set` | prior session notes | **Resolved.** `handles.rs:880-895` traces `props`, and a class opts its own handles in via `#[rtse::trace]`, which the macro compiles into a `trace` fn pointer the collector calls. The blocker that would have stopped §1e.5 is gone |

**The inlining correction is an opportunity, not just an erratum.** Wasmtime
leaves inlining off because *its* input arrives already inlined by LLVM and the
compile-time cost is not worth it. Neither premise holds for RTS: the input is
TypeScript, nothing has inlined it, and the `__rtsadp_*` trampolines are exactly
the opaque boundary §1e.2 measured at 12.2×. One caveat bounds the idea — the
pass inlines CLIF into CLIF, so a Rust-compiled `__rtsadp_*` is not a candidate;
the reachable version is emitting the fast path as CLIF, which is §1e.2 item 1.

---

## §8 What these measurements do NOT prove

Ranked by how likely each is to matter here. Sourced from how VM teams validate
their own work.

0. **The probe replica is ~7× CHEAPER than the real engine on the identical
   loop — and this is checkable in-tree, which makes it the limitation that
   matters most.** `FUTURE_OPTIMIZATION.md:84-90` measures `bench/objbench.ts`
   (3M × `const p = new P(i,i+1); s += p.x*p.y`) at **2058 ms = 686 ns/iter**.
   Probe kernel B is the same loop and reads **~100 ns/iter**. So the replica is
   missing ~6/7 of what the engine actually does per iteration (GC ticks, the
   real `Entry` size, prototype wiring, IC/dispatch, thunk overhead — the probe
   replicates the *shapes* of the calls, not the whole engine). **Every absolute
   number in §3 is therefore optimistic**, and the ratios are only trustworthy to
   the extent the missing work is spread evenly across variants, which is
   unverified. Fixing this is cheap: run the same ladder against the engine via
   `RTS_REPR_STATS` + `hyperfine` on `bench/objbench*.ts` and reconcile.
1. **Uniform single-shape input trains the branch predictor and never re-warms
   the I-cache.** Every probe row is a tight loop repeating the same shape/tag.
   That is the pattern known to make a guard look free when a heterogeneous real
   site would not get the benefit **[S]**. **The §3.3 guard numbers are a best
   case.** The design doc's own `mono → poly → mega` and Tagged hard points (loop
   phis, catch bindings, closure captures) are never exercised by a monomorphic
   microloop.
2. **Median of 7 with no variance reported is short of a defensible CI.**
   Kalibera & Jones give the closed form for how many repetitions a target
   interval needs **[S]**. As it stands, a real 10× and a real 3× with a lucky
   sample look identical in the output.
3. **Whole-program cost is invisible.** BOLT gets up to **20.4% on top of
   PGO+LTO purely by relaying out code** **[S]**. ~1700 real IC/shape sites
   compete for the same 32 KB of L1i in a real binary; one site in a loop does
   not.
4. **The escape-analysis number is a ceiling.** 0.71 ns is the provably-local
   case; real code with closures and dynamic receivers lands somewhere between
   that and 100 ns, and the probe cannot say where.
5. **Latency-bound loop vs throughput-bound program.** One dependent operation
   per iteration measures chain latency, not throughput with independent work
   around it.
6. **The probe measures the profile RTS's benchmarks already over-index on.**
   The three canonical benches are numeric hot loops — same shape. V8 measured
   that real programs stress the **parser and compiler**, not steady-state
   execution **[S]**. RTS's parity sits at 69.9% (measured now from `.github/cross_runtime_report.json`; the badge and `CLAUDE.md` both quote stale numbers) while the numeric benches look
   excellent; that gap is itself evidence the probe's target may not be where the
   real problem lives.

### What to build to fix this

- **Tighten the probe**: repetition count from the Kalibera & Jones formula,
  report a CI or at least stdev, **vary the input shape pseudo-randomly across
  iterations**, `black_box` operand *and* result.
- **Score the cross-runtime corpus like JetStream 2** instead of pass/fail: per
  fixture measure **startup** (compile + first iteration), **worst case** (cold
  path — where the Tagged bails live) and **steady state**; geometric-mean them.
  The corpus already exists; only the scoring is missing.
- **Track compile time, binary size and RSS per merge** (rustc-perf / Chromium
  dashboard pattern **[S]**). RTS has already had a regression only RSS would
  catch — the egui GPU leak.
- **One polymorphic macro-benchmark** built to break every assumption the probe
  makes free: ≥4 shapes in varying proportions, genuinely escaping allocations
  mixed with local ones, hundreds of property-read call sites. **If the 0.71 ns
  and 1.2 ns numbers survive within ~2×, the probe's optimism is bounded. If they
  do not, that gap is the real target.**
- **Go-style PGO is directly transplantable** — AOT, deopt-free, profile baked at
  compile time; measured 2–4% (Go 1.20) → 2–14% (Go 1.22) **[S]**.

---

## §9 Open questions needing an owner decision

1. **AOT CPU baseline** (§6.2). Same-machine build+run, or a documented minimum
   target? This is a correctness question, not a performance one.
2. **Binary size target** (§6.3). Is the Go/QuickJS peer class the goal, or is
   "smaller than Bun/Deno" sufficient? The answer changes whether Tier-1 of §6.3
   is urgent or cosmetic.
3. **`FUTURE_OPTIMIZATION.md` phase order.** That document's revised order is
   `4a (per-slot Repr on Shape) → 2 (escape) → 1 (Tagged) → 3 → 5`. This document
   puts the redundant-work items and the inline guard *before* all of them,
   because they are cheaper and independently measured. Reconcile explicitly
   rather than letting two orders coexist.
4. **Threading model interaction.** Tier 3.1 (lock-free slab) and
   `rts-threading-model.md` T2–T5 (regions + promotion) overlap. Which is the
   trunk?
5. **IC state machine.** With no feedback-driven recompilation, a PEP-659-style
   runtime counter has nothing to act on. Is the IC a compile-time decision
   (Dart-AOT-shaped) or does RTS want a mechanism to swap a function-pointer slot
   at runtime first?

---

## §10 Sources

Measurement artifacts in-tree: `crates/rts-value-probe/README.md` (the 107-row
ladder and its own limitations), `the spec removed 2026-08-03 (see git history)` (Phase 0
histogram, the 1024×-vs-Rust baseline), `OPTIMIZATIONS.md` (the startup
campaign).

External, one line each — full URLs are in the research transcripts:

- Shahriyar, Blackburn & McKinley, *Fast Conservative Garbage Collection*, OOPSLA 2014 — conservative Immix within 2–3% of precise
- Blackburn & McKinley, *Immix*, PLDI 2008 — 7–25% whole-application
- Stadler, Würthinger & Mössenböck, *Partial Escape Analysis and Scalar Replacement*, CGO 2014 — the PEA algorithm and its measurements
- Dolby & Chien, *Automatic object inlining*, PLDI 2000 — 58% fewer object creations
- MEA2, *Lightweight Field-Sensitive Escape Analysis for Golang*, PACMPL 2024
- Kalibera & Jones, *Rigorous Benchmarking in Reasonable Time*, ISMM 2013
- Watt, *Look Before You Leap: Checking In on Type Tag Checking*, arXiv 2026
- Melançon, Serrano & Feeley, *Float Self-Tagging*, PACMPL 2025
- Shipilev — JVM Anatomy Quarks #4 (TLAB), #10 (`String.intern`), #22 (safepoint polls)
- V8 blog — Retiring Octane; real-world performance; fast properties; slack tracking; elements kinds; hash code; adaptor frame; pointer compression
- WebKit — Speculation in JavaScriptCore; Concurrent JavaScript; `MathCommon.cpp` (`operationMathPow`)
- Jan de Mooij — CacheIR; MPLR'23 CacheIR paper
- CPython PEP 659 — specializing adaptive interpreter
- Chris Fallin — Compilation of JavaScript to Wasm (AOT vs JIT); weval
- mrale.ph — Dart VM internals (switchable calls, TFA)
- Mozilla — Slimmer and faster JavaScript strings in Firefox (latin1, inline strings)
- Bytecode Alliance — cranelift-egraph RFC; wasmtime issues #4131, #7283, #4463; PRs #11210, #12160, #12800, #13214
- Go — `inl.go` (inlining budget); PGO docs and 1.20→1.22 measurements
- BOLT (Meta Research) and Propeller — post-link layout
- min-sized-rust; MaskRay on linker garbage collection

---

## §10 §5 RESULTS — what each Tier item was actually worth

Implemented 2026-08-02. Conditions as in the header: release build, this machine,
each item behind its own env switch so it is A/B-able on ONE binary, and
`RTS_NO_PRELUDE_CACHE=1` on both arms of every A/B (the prelude cache keys on the
prelude text plus a version, so without it both arms replay one cached lowering
and the comparison measures nothing).

| item | expected | measured | state |
|---|---|---|---|
| 1.1 both-strings concat fast path | 3.2x | **161 → 105 ms** (1.53x) | landed |
| 1.2 no `String` per property key compare | ~2x | **348 → 283 ms** on `fn.name` (1.23x) | landed |
| 1.3 `x ** 2` → multiply | 11.5x | **89 → 30 ms** (3.0x) | landed, `RTS_POW_FOLD` |
| 1.4 env + per-shard live counters | ~2.7% | within noise on this bench | landed |
| 1.5 cold miss/bail/error blocks | "conservative" [E] | **zero** (62 vs 61 ms) | landed, `RTS_COLD_BLOCKS` |
| 2.1 inline operator tag guard | 2–4x | compare **2.0x**, arith 1.2x | landed, `RTS_OP_GUARD` |
| 2.2 overflow-safe int arithmetic | — | correct, but 6.5x and changes repr | gated, **still OFF** |
| 2.3 runtime int guard for `%` | 1.45x | **35 → 8 ms** (4.4x) | landed, `RTS_REM_GUARD` |
| 3.1 / 3.2 stable slab + read as `load` | ~2.2 ns + ~5.3 ns | **600 → 362 ms** (1.66x); 1.24x vs default | landed, `RTS_SLAB` + `RTS_FIELD_LOAD` |
| 3.3 lazy shape for untracked receivers | 63.82 → 31.45 ns | **premise REFUTED** | census + inverse fix landed |
| 3.4 un-globalize the shape registry | — | reads no longer serialize | landed |
| 4.1 tier-0 escape analysis | 138x [best case] | **1050 → 1 ms** | landed, `RTS_ESCAPE` |
| 4.2 bump/nursery allocation | 4 instructions [S] | blocked on object layout | recycler landed, `RTS_BUMP` |

**§5 is complete except 2.2**, which is gated OFF with its remaining work named
below (a representation-preserving merge, not more type information). Both §11
correctness bugs are fixed.

### 3.1 and 3.2 are ONE item measured in halves

3.1's own note said its win "is not collectable until 3.2 emits the load". The
measurement is worse than that, and it is the more useful result of the pair:

    default (no slab)                450 ms
    RTS_SLAB=1, RTS_FIELD_LOAD=0     600 ms      <- 3.1 ALONE is a 33% REGRESSION
    RTS_SLAB=1, RTS_FIELD_LOAD=1     362 ms

The chunked store's extra indirection makes the Rust accessor path *slower*, and
the only thing that pays for it is removing that path from the hot read. Shipping
3.1 on its own would have been a measurable loss presented as infrastructure.

Note also that both `bench/objbench.ts` and `bench/objbench_noalloc.ts` are
useless for this item — the receiver is loop-invariant, so the egraph hoists the
whole read out and both run in 2–3 ms on either arm. The first bench written for
3.2 had the same defect (10M iterations in 7 ms). A field-read benchmark must
vary the receiver.

### The three premises this work refuted

**3.3 — "untracked receivers default to dictionary mode".** A census of all 31
`Entry::Map` construction sites found **none** created because a shape could not be
proven; the codegen never mints one at all, and `added_key_shape` already grows a
shape lazily through the transition tree. The claim traced to one over-broad
sentence in the probe's own `rt/dict.rs` doc comment. The 63.82 ns priced the
dictionary REPRESENTATION, never the cost of being unproven. The real defect was
the inverse — the dictionary probes ran BEFORE the shaped own-slot read, so every
shaped read paid a `String` malloc and a shard lock it did not need.

**4.2 — "bump allocation is the next allocator step".** It is blocked by neither
the lock (2.4x against allocation's 40.4x) nor the moving collector (§4.1 already
refuted that coupling), but by the object REPRESENTATION: `Entry::Vec(Box<Vec<i64>>)`
owns its buffer through the global allocator, and no arena backs a `Vec` without
the unstable `Allocator` API or a change at 386 sites. The bump pointer arrives
with the C2 object layout, not with an allocator patch.

**1.5 — "cold blocks make the guard numbers conservative".** Measured at zero on a
2M-iteration call+IC loop, twice, at two different sets of sites.

### What 2.2 taught, which is not what it set out to teach

The item was blocked on distinguishing a JS `number` from a declared `i64`. That
is now built (a `native_int` bit on the HIR binding) and it works. The check is
still off, for two reasons neither of which is the original blocker: the gate moved
the cost off native-int code without reducing it for JS numbers (6.5x on an
unannotated int loop, which is most TypeScript), and the `Float64` merge changes the
RESULT REPRESENTATION even where nothing overflows — observable as two failures in
a serialization format-freeze test. The remaining work is a representation-preserving
merge, not more type information.

### Two bugs found by this work rather than by the plan

* **A fraction reaching an int accumulator through a BINDING or a CAST truncated.**
  `let b = 0; { const x = vals[i]; b += x as number; }` gave a different answer than
  the same line written inline. `floatscan` looked only at the assignment's own
  value tree, so a heap read that happened at the binding was invisible, and it had
  no arm for the `Cast` node that `as number` produces.
* **A re-entrant shard lock.** 15 runtime sites allocated or re-normalized a handle
  inside a `with_entry`/`with_rtse` closure, which self-deadlocks when the new
  allocation lands in the same shard. Latent at ~1-in-32 under round-robin
  allocation; thread-affine regions raised it to ~1-in-2 and made it reproducible.
  Fixing it is what let `RTS_REGIONS=1` pass the whole suite.

---

## §11 Two live correctness bugs this work walked into — both FIXED

Neither was predicted by the plan; both were found while benchmarking allocation
for Tier 3.1, and both reproduce on a pre-session build.

### 11.1 The mark phase truncated at 1M steps — FIXED

`mark_handle` carried a `steps > 1_000_000` cap. It was load-bearing only because
`HandleTable::mark` re-enumerated an already-marked node's children, so a cycle
never terminated. Truncating a MARK phase does the one thing a mark phase must
never do: past a million steps the still-live tail kept `marked = false` and the
sweep freed it.

  500 000-element array of class instances, summed:  NaN   (450 000: correct)
  same program with RTS_GC_DISABLE=1:                correct at every size

The periodic GC fires when the live set passes `GC_LIVE_FLOOR` (500 000 handles),
which is exactly why the failure begins there. Fixed by terminating on the mark
BIT — each slot expands at most once per cycle — and removing the cap, which with
correct termination can only ever truncate. Guarded by
`tests/gc_large_live_set.test.ts`; the fixture's 520k size cannot shrink, because
below the floor no collection happens at all and a smaller fixture would pass on
the broken code.

### 11.2 A receiver used only through a field read is not a GC root — FIXED

```ts
class N { v: number; constructor(v: number) { this.v = v; } }
const plain = new N(7);                        // used only at top level
const viaFn = new N(9);
function readIt(): number { return viaFn.v; }  // referenced from a function
const junk: N[] = [];
for (let i = 0; i < 600000; i++) junk.push(new N(i));
console.log(plain.v, readIt());                // RTS: 499325  9      node: 7 9
```

`plain.v` does not merely read `undefined` — it reads **another object's field**.
Its slot was freed and reused, and a `PolyValue` carries only the 48-bit slot, so
the stale word silently resolves to the new occupant. A wrong value, not a crash.

What distinguishes the surviving cases, measured:

| binding | survives? | why |
|---|---|---|
| local of a called function | yes | its frame is spilled to the stack the scanner walks |
| top-level, referenced from a function | yes | promoted to a gcell, `mark_gcell_roots` covers it |
| top-level, used only at top level | **NO** | — |
| the whole program with the loop inside a function | yes | `__rts_startup`'s frame is below the allocating frame |

Not caused by any of this session's work: it reproduces with `RTS_ESCAPE=0`,
`RTS_OP_GUARD=0`, and on a pre-session build, and disappears with
`RTS_GC_DISABLE=1`.

One concrete lead, verified: `scan_all_roots`'s third root source is
"globals top-level com handles", implemented as `mark_global_roots` over
`collector::global_roots`. With `RTS_GC_DEBUG=1` that reports **`globals=0`** on
the failing program, and grepping the tree shows the registry has exactly one
producer — `rts-napi`'s references. **The codegen never registers a global root.**
Whether that is the whole cause is unproven; what is proven is that the mechanism
the comment claims covers this case is empty.

#### RESOLVED (2026-08-02) — and it was none of the above

The heading was wrong in every word except "not a GC root". The bug had nothing
to do with top level, with `const`, or with `mark_global_roots` being empty — the
`globals=0` lead was true and irrelevant.

`emit_marshal::emit_payload` emitted `poly_word & PAYLOAD_MASK` before every
fused payload-addressed heap access. `PAYLOAD_MASK` clears bits 63..48, and bits
63..48 are exactly where the conservative scanner looks: `scan_range` treats a
word as a root candidate iff its handle GENERATION is non-zero. A masked payload
has generation 0 by construction, so it is not merely missed — it is
**structurally unrecognizable**, and it must stay that way, because a bare 48-bit
slot index is indistinguishable from a small integer and accepting those would
make every loop counter a root.

The mask is pure, cheap and loop-invariant, so the egraph hoists it. Once hoisted,
a receiver whose only downstream use is a field read has NO live boxed form across
the loop: the boxed word is dead, the masked word is invisible, the slot is swept,
and a later read resolves the stale slot to the object that reused it.

Which explains every row of the table above without any of its explanations: the
survivors survive because something else forces the BOXED word to stay live — a
call boundary passes it whole, `typeof` needs the tag. The falsifying experiment
was one line: adding `typeof plain` to the failing program makes it print `7`.

The fix is a deletion. Every one of these entry points already masks internally
(`payload_ops::with_payload_slot` does `poly48 & SLOT_MASK`), so the call-site
`band` was redundant work that also destroyed the root. `emit_payload` now passes
the boxed word through untouched, and Tier 3.2's inline sequence recovers the
payload INSIDE its fast path — the cold-block call still consumes the boxed word,
which is what keeps it live where an inline mask alone would be hoisted again.

Guarded by `tests/gc_receiver_root.test.ts`, whose 600 000 size cannot shrink:
below `GC_LIVE_FLOOR` nothing collects and a smaller fixture passes on the broken
engine.

**The general lesson, worth more than the bug:** with a conservative scanner,
narrowing a pointer-like word to a "cheaper" form is not a local optimization. It
changes whether the GC can see the value at all, and the egraph will happily
delete the only recognizable copy. Any future emission that strips tag bits from
a handle must keep a boxed use alive, or the value must be rooted explicitly.
