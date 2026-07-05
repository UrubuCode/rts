# New-engine GC — weak phase now, generational copying (nursery) later

> **Status:** DESIGN / DECISION. The weak phase is the next bounded step; the
> generational copying is the long-term leap, a dedicated project **DEFERRED
> until ~90% cross-runtime working** (CLAUDE.md / `rts-codegen-new-design.md` §5.7).
> This document records the decided direction and the why — RTS has a rare
> architectural advantage that defines the best design.

## RTS's architectural advantage: handle indirection

`PolyValue` stores a **`HandleTable` slot index**, not a raw pointer
(design doc §5.4, Pilar 1). The 48-bit payload of a `TAG_STR`/`OBJECT`/
`FUNCTION` is `slot+shard`, resolved to the real address by the table.

This changes the calculus of a moving GC. In a normal copying/compacting GC the
deadly cost is **finding and rewriting EVERY pointer** that points to the moved
object (pointer-patching). In RTS the object moves in the backing store and you
update **only the slot→address in the table** — the index inside each
`PolyValue` does not change. Moving an object becomes almost free. The Achilles'
heel of every moving GC is already neutralized by the indirection that already
exists.

## Current state (precise mark+sweep)

`crates/rts-std/src/collector/` (new engine) / the `collector.rs` documented in
`.claude/rules/02-runtime.md`. Mark+sweep with Cranelift's `UserStackMap` +
conservative scanner (`SuspendThread`+`GetThreadContext`) for registered
threads. `GC_TICK_INTERVAL` allocs → `finish_cycle()` = `mark_stack_roots()`
+ `sweep_all_shards()`. The stack scanner recognizes NaN-boxed `PolyValue` words
(`(w & BOX_BASE)==BOX_BASE` and `tag(w) ∈ {STR,OBJECT,FUNCTION}` → root = 48-bit
slot; inline int/float/singleton are NOT roots).

## Step 1 (next, small) — weak phase in the current mark+sweep

`#217` (real WeakMap/WeakSet + WeakRef + FinalizationRegistry) does NOT require
rewriting the GC. A new phase between mark and sweep solves it, bounded:

1. **Normal mark** — but does NOT mark through the KEYS of a `WeakMap`/elements
   of a `WeakSet` (the key becomes a "candidate to die"); the value only
   survives if the key survives via another strong reference.
2. **Weak phase** (post-mark, pre-sweep): for each `WeakMap`/`WeakSet`/`WeakRef`,
   if the target was not marked → remove the entry / `deref`→`undefined`. For
   each `FinalizationRegistry` whose target died → enqueue the finalization
   callback (drained by the event loop).
3. **Normal sweep.**

The handle indirection helps again: a `WeakRef` stores `(slot, generation)`. If
the slot was freed/reused (the slab's 16-bit generation bumped), `deref`
returns `undefined` — **O(1) detection with no scan**. (The generation does not
fit in `PolyValue`'s 48-bit payload; only WeakRef/FinalizationRegistry need the
full 64-bit handle — design doc §5.5.)

Today `WeakMap`/`WeakSet` are `.ts` with STRONG-ref semantics (interim). The
weak phase makes them REAL without changing the architecture.

## Step 2 (long term, big) — generational copying (nursery)

The generational hypothesis is very strong in JS: the overwhelming majority of
objects die young (loop temporaries, intermediate `{}`/`[]`). The recommended
design:

- **Young gen (nursery):** bump-allocate (alloc = increment a pointer,
  extremely fast). Full → **minor GC**: copies only the survivors to the old
  gen. Scans only the young gen + the remembered set. Most temporaries die
  here → never promoted → dirt-cheap collection.
- **Old gen:** mark-sweep / mark-compact for the survivors, runs rarely
  (**major GC**).
- **Moving = trivial** (handle indirection) → no fragmentation, no
  pointer-patching: updates only the slot→address.
- **Write barrier:** records old→young references (remembered set) so the minor
  GC does not sweep the entire old gen. Small cost on a property-write of an old
  object.
- **Multi-thread:** per-thread nursery (TLAB) → lock-free alloc. The
  shard-aware `HandleTable` already fits this.
- **Precise roots:** the Cranelift stack maps already exist.

Why it is the best for RTS on native machines: fast alloc (bump), cheap minor
GC (the common case), rare major GC, compacts (no fragmentation), and the cost
of moving — the problem of every moving GC — already comes for free from the
indirection. It is what V8/JSC do; in RTS the expensive part is free.

## Practical path

| Step | Effort | Gain |
|-------|---------|-------|
| Weak phase in the current mark+sweep | small | real `#217` (WeakMap/WeakSet/WeakRef/FinalizationRegistry) without rewriting the GC |
| Generational copying (nursery)  | big  | throughput + latency (short pauses), no fragmentation — the long-term leap |

**Order:** weak phase when `#217` enters the agenda (closes the weak story
honestly, unblocks real WeakMap/WeakSet); the generational as a dedicated
project **after ~90% cross-runtime** — it is the right upgrade, and RTS is
uniquely positioned for it.

## Why NOT earlier

Swapping the GC while the new engine is still filling in language semantics only
adds an unstable variable on the critical path. The current mark+sweep is
correct and sufficient until then; the weak phase is additive (does not change
the architecture). The generational only pays off when the engine already runs
most real programs and the bottleneck becomes GC throughput/pause, not feature
coverage.

## Phased executable plan (A1 → A2)

> Each step below is **compiles + green-suite + reversible** in isolation, with
> the honesty-floor guard (NOTHING that crashes/hangs committed as "pass";
> build always compiles). The live collector (`rts-engine/src/collector/collector.rs`
> mark+sweep + `rts-std/src/collector/`) is only touched ADDITIVELY until A2.
> Starting state (2026-06-22): correctness ~51% (323/626); legacy gc.* already drained.

### A1 — weak phase (#217), bounded, additive

A1 does NOT rewrite the GC: it adds a phase between mark and sweep + moves the
WeakMap/WeakSet storage from the strong-ref `.ts` to the native `Entry` the
collector understands.

- **A1.0 (infra, green):** ensure `Entry::WeakMap(HashMap<u64,i64>)` /
  `WeakSet(HashSet<u64>)` / `WeakRef(u64 handle 64-bit)` /
  `FinalizationRegistry{callback,entries}` exist (they already exist in the
  enum) and that the collector's scanner does NOT mark through the contents of
  those variants (today, if not allocated as roots, it already does not mark —
  confirm with a collector unit test).
- **A1.1 (WeakRef deref O(1), the most bounded) — DONE in the runtime, BLOCKED
  in TS:** `WeakRef` stores the FULL 64-bit handle. `deref()` only returns the
  target if `with_entry(target)` is still `Some` (gen+slot match); otherwise
  `undefined`. `Entry`'s `Traceable::trace_children` already falls into `_ => {}`
  for WeakRef — the target is NOT kept alive, so the staleness check is the
  complete weak semantics (+ fixes a latent use-after-free from v0). Implemented
  in `rts-shared/src/globals/weakref/mod.rs` + unit test.
  **BLOCKER discovered:** the new engine does NOT yet support `new WeakRef(..)`
  ("class WeakRef is not a user class — global/Registry class é incremento
  posterior"). So A1.1 is correct but NOT exercisable from TS until the engine
  instantiates global/Registry classes via `new`. That is the real prerequisite
  of ALL of A1 (Rule C: resolve the blocker first). The unit test does not run
  standalone (pre-existing limitation: lib-test does not link codegen symbols
  like STRING_NEW).
- **A1.2 (native WeakMap/WeakSet storage):** new ABI externs
  `__RTS_FN_NS_GC_WEAKMAP_*`/`WEAKSET_*` (new/set/get/has/delete) over
  `Entry::WeakMap`/`WeakSet`; rewrite `rts-shared/src/stdlib/weakmap_set.ts`
  to delegate to them instead of strong-ref PolyValue arrays. STILL strong until
  A1.3 (the storage exists, but the collector still lacks the weak phase) —
  behavior unchanged, green suite.
- **A1.3 (weak phase in the collector):** between `mark_stack_roots()` and
  `sweep_all_shards()` in `finish_cycle()`, sweep each `Entry::WeakMap`/`WeakSet`:
  remove entries whose KEY-handle was not marked; each `FinalizationRegistry`
  with a dead target → enqueue callback on the event loop. Only HERE does the
  behavior become REAL weak. Test: a weakmap loses an entry when the only strong
  ref to the key goes away + collect runs.
- **A1.4 (FinalizationRegistry drain):** the event loop drains the callback
  queue enqueued by A1.3. Test: callback fires after collection.

### A2 — generational copying (nursery), DEFERRED until ~90% cross-runtime

> Do NOT start before ~90% by design (section "Why NOT earlier"). Order only
> when unblocked. Each step behind the `RTS_GC_GENERATIONAL` flag (default OFF) —
> the current mark+sweep remains the production path until the flag flips.

- **A2.0:** flag + dual-path in `finish_cycle` (OFF = current mark+sweep, intact).
- **A2.1:** per-thread nursery bump-alloc (TLAB) behind the flag; new allocs go
  to the nursery when ON.
- **A2.2:** write barrier on property-writes of old objects → remembered set.
- **A2.3:** minor GC = copy nursery survivors to the old gen, scan only
  nursery + remembered set; moving = update slot→address in the HandleTable (the
  indirection makes this ≈ free, no pointer-patching).
- **A2.4:** old gen mark-compact (major GC), runs rarely.
- **A2.5:** A/B against the mark+sweep (identical correctness) + pause/throughput
  bench before flipping the default.

**Recommendation:** execute A1 now (sanctioned, bounded) in the order A1.1 → A1.4;
keep A2 behind the flag and only turn it on post-~90% cross-runtime, with A2.5
as the gate.
