# RTS Threading Model — engine-level multithreading + regional heap (v0, proposal)

> **Status: PROPOSAL** (2026-07-05), **T0+T1 LANDED** (2026-07-16 — see Phases).
> Companion to `rts-std-surface.md`
> (§rts:thread) and to the canonical engine design
> `rts-codegen-new-design.md`. No prior engine-threading doc existed — the
> only documentation was the mechanism table in
> `crates/rts-runtime/src/namespaces/thread/abi.rs` (now the `rts:thread`
> surface). This doc records the target model and why the current value
> model supports it.

## Thesis

RTS will have multithreading IN THE ENGINE (not just runtime threads
calling detached fns): JS objects crossing threads safely, regional
collection without a global stop-the-world, and parallelism in specific
engine areas. The target model is **per-thread regions + a shared heap
with promotion on publication** (a Java-G1 / Erlang middle ground),
because the PolyValue value model was built with exactly the properties
this requires.

## Why PolyValue supports it (the 3 properties)

1. **Payload = slot index, never a pointer.** The NaN-box word (STR/
   OBJECT/FUNCTION) carries a HandleTable slot (48 bits). Moving the
   OBJECT between regions/heaps invalidates no live word — only the slot
   is updated (the indirection the generational-GC doc already noted as
   "moving ≈ free"). TLABs, regions, compaction and promotion become slot
   updates, with no read barriers in generated code.
2. **Shards are already proto-regions.** The HandleTable has 32 lock-free
   shards with per-thread round-robin affinity (`alloc_entry`). Evolving
   to "the thread's region" = deterministic alloc→thread-shard affinity +
   local collection of that shard. `shard_for_handle` already decodes
   O(1).
3. **64-bit word = atomic load/store.** A shared PolyValue never tears;
   the tag check is valid cross-thread.

## The model (target)

```
┌───────────── Thread A ─────────────┐   ┌───────────── Thread B ────────────┐
│ region A (affine shards)           │   │ region B                          │
│  - local bump/slab allocation      │   │                                   │
│  - LOCAL GC: pauses only A         │   │                                   │
└─────────────┬──────────────────────┘   └───────────────┬───────────────────┘
              │ publication (write to shared global/      │
              │ channel/SharedArrayBuffer/shared object)  │
              ▼                                           ▼
        ┌──────────────────── SHARED heap ────────────────────────────┐
        │ promoted objects; rare global collection (today's parity)   │
        └──────────────────────────────────────────────────────────────┘
```

- **Local birth**: every object is born in the creating thread's region.
  Cheap local collection (the scanner already suspends per-thread via
  SuspendThread + stack maps; restrict the sweep to the region's shards).
- **Promotion on publication**: the FIRST time a region value escapes to
  another thread (write to a shared gcell/global, send over a channel,
  capture by `thread.spawn`, worker return), the subgraph is PROMOTED to
  the shared heap. Cheaply detectable: every write already goes through
  the slot paths (`obj_set`/`VEC_SET`/gcell) — it is a "destination region
  ≠ value region" check.
- **Promotion = moving slots** (property 1): re-home the entries into
  shared shards and update the slots; live words keep their meaning
  (the handle is stable if promotion reuses the same slot id in a shared
  shard — encoding decision: reserve shard bits OR a per-slot forwarding
  table).

## Prerequisites (mapped blockers; each becomes an issue)

| # | Blocker | Today | Fix |
|---|---|---|---|
| 1 | ~~**Thread-local GCELLS**~~ **DONE (T1, 2026-07-16)** | ~~module globals are per-thread~~ → module globals are now ONE shared store per program (`rts-std/src/collector/gcells.rs`); a worker's write to a module global is visible to every thread | done: `share()`/`attach()` replaced the value snapshot; lock-free chunked `AtomicU64` store (reads are cheaper than the thread_local it replaced). Program isolation kept: the store is per-program, not process-global |
| 2 | **Data ICs (`PropIcCell`)** | mutable cell without atomicity | mono→poly→mega states in an `AtomicU64` (shape+slot packed in one word) or per-thread ICs |
| 3 | **String pool / interning** | global pool with a lock | per-region interning + merge on promotion; immutable strings make this easy |
| 4 | **Shape registry** | global `Mutex` (fine for rare reads) | `RwLock`/lock-free snapshot reads; ids are append-only |
| 5 | **Event loop / microtasks** | single-thread queue (drained on main) | define: each region-owning thread has ITS microtask queue; global timers route to the callback's owner thread |
| 6 | **Codegen/JIT state** | global `reset_codegen_state`, 1 program per process | fine for multithreaded runtime; JIT stays single-compile |
| 7 | **NaN-box GC scanner** | conservative per-thread, already recognizes words (design §5.4) | per-region: mark only the thread's roots + a remembered set for shared→local refs (the promotion write barrier prevents shared→local: promoting closes the subgraph) |

Chosen key invariant: **a shared→local reference never exists.**
Promotion transitively closes the published subgraph. This eliminates
inter-region remembered sets; the cost is eager subgraph promotion
(acceptable: whoever publishes an object rarely publishes half of it).

## Engine-area parallelism (independent of regions)

Short-term targets that do NOT depend on the regional model:
- `parallel(arr).map/reduce` (rayon) — exists; surface in `rts:thread`.
- Parallel parse/HIR of independent modules at build time.
- AOT: per-module object emission in parallel (per-module ObjectModule is
  already the slicing design).
- GC: parallel shard sweep (shards are independent by design).

## User surface (summary; detail in rts-std-surface.md §rts:thread)

- `threadLocal(template)` → `Threaded<T>` — transparent per-thread
  instance (template + lazy structuredClone per thread; factory overload
  for non-clonables); the value that NEVER promotes.
- `shared(value)` → `Shared<T>` — transparent single instance promoted to
  the shared heap; per-method auto-synchronization + `lock(shared, cb)`
  for compound transactions.
- `channel<T>()` — mpsc; `send(v)` promotes `v`.
- `task(fn): Promise<T>` — pool thread integrated with await.
- `spawn(fn, arg)` — real thread; captures promote the captured subgraph.
- `Mutex`/`RwLock`/atomics — explicit low-level shared cells.
- `SharedArrayBuffer` + `Atomics` — raw shared memory (already
  primordial).
- Web-style workers (future): thread + region + isolated module +
  postMessage (= channel with structuredClone-or-promotion).

`Threaded`/`Shared` engine implementation: a new case on the dispatch
paths `Proxy` already traverses (`proxy_parts`-like) — obj_get/set/method
resolve to the per-thread/shared slot before normal dispatch; zero cost on
the non-exotic path.

## Phases

- **T0**: doc approved; issues for blockers 1–5. **DONE.**
- **T1**: shared gcells (#1) — also fixes the current setInterval/thread
  bug class. **DONE (2026-07-16)** — `rts-std/src/collector/gcells.rs`. A
  module-level binding is now ONE binding across a program's threads:
  `let n = 0` + `thread.spawn(() => n++)` accumulates (measured: two workers
  ×2 increments → `4`; before T1 it read `0`, each worker mutating a private
  copy). What it does NOT yet make sound is invoking a JS listener from a
  non-JS thread — the pending-error slot is still thread-local
  (`rts-runtime/src/adapters/value/errslot.rs`) and blocker #5 (per-region microtask
  queues) is open, so event DISPATCH stays on the JS thread (see
  `rts-node`'s `emitter`/`loop_sources` pumps). T1 is the first of those
  blockers, not the whole of multithreaded dispatch.
  Exposed (pre-existing, unrelated): `thread.spawn(fn, arg)` passes its arg
  through a raw-i64 bridge, so a `number` param reads the raw bits as an f64
  (#206/#242). T1 made the wrong value VISIBLE where it used to be silently
  discarded along with the write.
- **T2**: atomic ICs (#2) + audit of global runtime state.
- **T3**: deterministic thread→shard affinity + parallel sweep.
- **T4**: promotion on publication (region write barrier) + local GC.
- **T5**: workers/channels/threadLocal/shared on the surface.

Cross-dependency: the generational GC (copying nursery,
`gc-generational-design.md`, deferred until ~90% cross-runtime) COMPOSES
with this — a nursery is the "single-thread region" special case.
Implement T4 before or together with generational; never two distinct
moving models.
