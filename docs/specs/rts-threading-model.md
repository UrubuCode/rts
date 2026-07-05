# RTS Threading Model — engine-level multithreading + regional heap (v0, proposal)

> **Status: PROPOSAL** (2026-07-05). Companion to `rts-std-surface.md`
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
| 1 | **Thread-local GCELLS** | module globals are per-thread (the setInterval hack; memory `project_test_100_grind`) | promote gcells to shared cells with synchronized writes; this is what turns "write to a global" into a promotion point instead of a bug |
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

- **T0** (now): doc approved; issues for blockers 1–5.
- **T1**: shared gcells (#1) — also fixes the current setInterval/thread
  bug class.
- **T2**: atomic ICs (#2) + audit of global runtime state.
- **T3**: deterministic thread→shard affinity + parallel sweep.
- **T4**: promotion on publication (region write barrier) + local GC.
- **T5**: workers/channels/threadLocal/shared on the surface.

Cross-dependency: the generational GC (copying nursery,
`gc-generational-design.md`, deferred until ~90% cross-runtime) COMPOSES
with this — a nursery is the "single-thread region" special case.
Implement T4 before or together with generational; never two distinct
moving models.
