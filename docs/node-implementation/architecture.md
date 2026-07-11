# RTS Node.js Implementation — Architecture

> Canonical architecture for the RTS implementation of the Node.js 25 API.
> Read this before writing or reviewing any `rts-node` code or any
> `docs/node-implementation/<module>.md` spec. Companion docs:
> [`INDEX.md`](./INDEX.md) (module map + status) and
> [`rts-std-migration.md`](./rts-std-migration.md) (the rts-std surgery).

Status: **planning / spec phase** — no implementation code has landed yet. The
current `crates/rts-node/` is a dead scaffold (see §2) and is being rewritten
from scratch against this document.

---

## 1. Goal

Implement the **complete Node.js 25 API** natively in RTS — every stable core
module, the documented deprecated ones (`domain`, `punycode`, `sys`), and the
experimental ones (`sqlite`, `wasi`, `test`) — so that a program written for
Node runs on RTS unmodified. Node parity is measured against Node.js **25.x**
(`https://nodejs.org/docs/latest-v25.x/api/`).

This is **not** an embedding of Node or V8. RTS has its own compiler and runtime
engine. Node's API *surface* is reproduced; the *implementation* is RTS-native
Rust plus `.ts` ergonomic shims.

---

## 2. Core decisions (owner-approved)

1. **`rts-node` is independent of `rts-std`.** It owns its own **backend-specific**
   native Rust implementations (`std::fs`, `std::net`, `std::process`, `std::os`,
   and crates like `flate2`, `rustls`, `brotli`, …). It **does not depend on
   `rts-std`** and **must not mirror** rts-std externs. It **may** depend on
   `rts-engine` (which owns the async primitive) and `rts-primitives` — never
   `rts-std`.
   Duplication of *backend-specific* (OS/syscall) code is accepted; **cross-context
   pure logic is shared through `rts-primitives`, not duplicated** (see decision 7
   and [`layering.md`](./layering.md)).

2. **The Node-duplicate modules are removed from `rts-std`.** `rts-std` is
   reduced to RTS-unique surface (`audio`, `asio_audio`, future UI) plus the
   Web-standard global infra (`globals/*`, `io`, `http_server`) and the shared
   runtime infra. The filesystem / process / os / net / tls / crypto namespaces
   move to `rts-node`. Details and the exact remove-list live in
   [`rts-std-migration.md`](./rts-std-migration.md).

3. **No V8 in RTS — map to the RTS engine instead.** `node:v8` and every
   V8-coupled surface reproduce Node's API but back it with RTS-engine
   equivalents, for **all callers**. Never embed, emulate, or link V8. See §11.

4. **Full Node 25 coverage, tiered.** All ~43 modules are specced. Priority tiers
   P0/P1/P2 drive implementation order, not coverage — everything is in scope.

5. **The existing `rts-node` scaffold is dead code.** `node_lookup`,
   `ns_prefix_for`, and `NODE_SPECS` are never called by any crate; `rts-node`
   has zero reverse dependencies and contributes zero JIT symbols. Today a
   `node:fs` import is silently served by the `rts:fs` namespace (rts-std) because
   the front-end strips the `node:` prefix and reuses the `rts:` namespace key.
   The rewrite replaces all of this.

6. **Globals belong to the engine.** `rts-node` mirrors only the `node:` *module*
   API. The ambient global surface (`globalThis`, `Buffer`, `URL`, `fetch`,
   `console`, `process`, `structuredClone`, the `setTimeout` family, …) is
   **engine-surfaced** — `rts-node` adds no new globals and does not re-implement
   them. Where Node exposes a global *and* a `node:` module form of the same
   object, the module spec documents the surface but the object is the engine's
   (the module re-exports it). See [`layering.md`](./layering.md) §3.

7. **`rts-primitives` = cross-context logic.** An API correct in **both** the
   browser (`rts-shared` ↔ future `rts-browser`) **and** the backend
   (`rts-std`/`rts-node`) — a pure algorithm with no OS dependency — lives in
   `rts-primitives` (depends only on `rts-engine`, wasm-safe). Native logic first
   written in `rts-node` that turns out cross-context is **promoted** to
   `rts-primitives`, not copied. Backend-specific wrappers stay in `rts-node`. The
   per-module placement is in [`layering.md`](./layering.md) §6.

8. **Missing engine capabilities are added to the engine, the right way.** When a
   module needs something the motor lacks (a `vm` eval-context primitive, an
   `async_hooks` context stack, structured serialize, a Wasm engine), implement it
   in `rts-engine` (or the appropriate shared layer) following the RTS
   polymorphism ideals (PolyValue / Repr lattice / shapes + ICs; prove-monomorphic
   or fall to one tagged representation) — never a front-end special-case. The
   engine still names only primordials. See [`layering.md`](./layering.md) §4.

---

## 3. Crate layout

### 3.1 Current (before)

```
rts-engine        (heap GC + ABI + Registry + HandleTable; async-free by doctrine)
  ├─ rts-primitives   (primordials)
  ├─ rts-shared       (non-primordial universal: math/num/json/globals + path + fmt)
  ├─ rts-std          (fs, os, process, env, net, tls, crypto, audio, asio_audio,
  │                    globals/*, io, http_server, runtime/tokio, event_loop,
  │                    promise, collector, thread, sync, atomic, time, ffi, test)
  ├─ rts-node         (DEAD scaffold: name→symbol table, borrows rts-std externs)
  └─ rts-runtime      (facade: pub use of engine + primitives + shared + std + …)
```

### 3.2 Target (after)

```
rts-engine        (THE MOTOR: value model + primordials + GLOBAL surface + JIT/eval
                   + GC + HandleTable + Entry::Backend ext point + the ASYNC PRIMITIVE
                   (event loop, promise settle, microtask queue, timers). Missing caps
                   land here, polymorphism-correct. The native async DRIVER (tokio) is
                   feature-gated so a future wasm/browser build swaps in a host loop.)
  ├─ rts-primitives (primordials + CROSS-CONTEXT shared logic: path, querystring,
  │   │              url algo, util.format/inspect/types, codecs, EventEmitter,
  │   │              assert, punycode, string_decoder, hash algos, WHATWG-stream
  │   │              state machines. deps rts-engine only → wasm-safe → browser+backend)
  │   ├─ rts-shared  (browser-capable non-primordial → future rts-browser)
  │   ├─ rts-std     (BACKEND, RTS-native UNIQUE: audio, asio_audio, io, http_server —
  │   │               NO Node modules; the fs/os/net/… bodies moved to rts-node)
  │   └─ rts-node    (BACKEND Node module API. deps rts-engine + rts-primitives,
  │                   NEVER rts-std. Owns backend-specific (OS/syscall) logic; reuses
  │                   rts-primitives for cross-context logic; gets async from the
  │                   engine; references engine globals; adds no new globals.)
  └─ rts-runtime    (facade: pub use of all of the above; AOT staticlib superset)
```

Placement of *which logic goes in which layer* is governed by
[`layering.md`](./layering.md) (the decision tree + per-module matrix). The short
version: **global? → engine. Runs unchanged in a browser? → `rts-primitives`/
`rts-shared`. Needs the OS? → backend (`rts-node`/`rts-std`). Engine lacks it? →
add it to the engine, polymorphism-correct.**

Two **foundation prerequisites** gate independence (see §7, §6):

- **Move the async primitive into `rts-engine`** (owner decision — async is a
  primitive API). The event loop, promise settle, microtask queue, and the timers
  primitive move *down* from `rts-std` into the engine (the tokio-free `PromiseSlot`
  struct already lives there). The native tokio DRIVER is compiled behind a
  feature (`native-async`, on for the toolchain) so a future wasm/browser build
  omits it and supplies a host-driven loop. This dissolves the earlier
  `rts-async`-crate idea **and** the cyclic-dependency risk: async sits at the
  bottom, so `rts-std` and `rts-node` both consume it with no back-edge.
- **Land `Entry::Backend(Box<dyn Traceable>)`** in `rts-engine` so `rts-node` can
  register its own opaque handle payloads (Stats/Server/Socket/FileHandle/Worker)
  without editing the engine's closed `Entry` enum per type.

---

## 4. How `node:` imports resolve (doctrine-safe, data-driven)

Node modules are **non-primordial**. The engine must never hardcode a node module
name (`fs`, `http`, …) in codegen control flow — same PRIMORDIAL-vs-REGISTRY rule
that governs `Map`/`Date`/etc. Resolution is fully data-driven.

Current front-end path (to be re-pointed):

1. `front/modules/resolve.rs::is_builtin()` — any `node:*` specifier is a builtin
   (no disk touch), like `rts:*`.
2. `front/modules/flatten.rs::builtin_ns()` — **today** strips `node:` → bare ns
   key, colliding `node:fs` onto `rts:fs`. **The rewrite changes this**: `node:*`
   must map to node-owned namespace keys (e.g. `node_fs`) resolved against
   `rts-node`'s own registry rows, not the `rts:` namespaces.
3. `front/run/module_entry.rs` records `local → (ns, member)`; the call lowers via
   `registry::namespace_member(ns, member)` to the member's extern symbol.

The only literals codegen may name are the **scheme prefixes** `"node:"` / `"rts:"`
(and the `"rts:test"` framework special-case) — never a specific module. The
`rts-node` `NodespaceSpec`/`NODE_SPECS` table **is** the "Registry for node": a
`node:fs` → `node_fs` namespace → member → symbol lookup that is pure data.

**Wiring work the rewrite must do** (Scout findings): add `rts-node` as a
dependency of `rts-runtime` (so its symbols link and its registry rows populate),
harvest its externs into the JIT symbol table via the existing
`registry::all_jit_symbols()` / `adapter_symbols` path, and move the node
namespace `register` rows out of the `rts:` rows in `registry_build.rs`.

---

## 5. ABI & value marshalling

All native surface goes through `rts-engine::abi` typed `extern "C"` symbols. No
`JsValue`, no boxing at the boundary.

| TS type  | `AbiType`   | Cranelift repr                | Note                         |
|----------|-------------|-------------------------------|------------------------------|
| `number` | `I64`/`F64` | `i64`/`f64`                   | native bits                  |
| `bool`   | `Bool`      | `i8`/`i64` (0/1)              |                              |
| `string` | `StrPtr`    | 2 slots `(ptr, len)`, UTF-8   | static ptr or GC handle      |
| bytes    | `Handle`    | `u64` → Buffer/ArrayBuffer     | see §9                       |
| object   | `Handle`    | `u64` HandleTable slot        | Stats/Server/Socket/… (§6)   |
| void     | `Void`      | —                             |                              |

**Symbol convention for rts-node:** `__RTS_FN_NODE_<MODULE>_<NAME>`, e.g.
`__RTS_FN_NODE_FS_READ_FILE_SYNC`, `__RTS_FN_NODE_NET_CONNECT`. This is distinct
from rts-std's `__RTS_FN_NS_<NS>_<NAME>` — rts-node owns its own symbol space and
does not reuse rts-std's, even where an implementation is byte-for-byte identical.

Each module doc's **§5.2** enumerates the native externs; ergonomic wrappers
(classes, option normalization, event wiring) live in `.ts` shims (§10).

---

## 6. Opaque handles (rich objects)

Node exposes many stateful objects: `fs.Stats`, `FileHandle`, `net.Server`,
`net.Socket`, `http.Server`, `Cipher`/`Hash`, `zlib` streams, `Worker`,
`MessagePort`, `Database` (sqlite). These cross the ABI as **opaque `u64`
handles** into the engine `HandleTable` (`rts-engine/src/heap/handles.rs`,
gen16/slot43/shard5), with a `.ts` class wrapper holding the handle.

Constraint: `Entry` is a **closed enum** in rts-engine with no generic backend
variant yet (only a Phase-2 TODO). rts-node either (a) adds variants to the engine
enum per type (layering smell, avoid), or (b) **lands `Entry::Backend(Box<dyn
Traceable>)`** and registers its own payload types through it. (b) is the chosen
path and a foundation prerequisite (§3.2). Existing generic variants (`Buffer`,
`Vec`, `Map`, `String`, `ProcessChild`, `Hasher`) may be reused directly.

GC: an opaque handle keeps its slot reachable while a live `PolyValue` references
it. Backend payloads implement `Traceable` so the collector can mark any nested
handles they hold.

---

## 7. Async model

Node's async APIs come in three forms; RTS maps each:

- **Sync (`*Sync`)** → call the blocking native primitive directly. No runtime
  needed. These are implemented first (P-a of every module).
- **Callback (`fn(err, result)`)** → schedule the work on the shared tokio runtime
  via `spawn_blocking`, then enqueue the callback invocation on the event loop.
- **Promise (`node:fs/promises`, `util.promisify`)** → allocate a pending
  `PromiseSlot`, settle it from the worker, drained by the event loop.

**Async is a primitive and lives in `rts-engine`** (owner decision). The machinery
— event loop (`run_event_loop` draining microtasks/timers/macrotasks/pending
promises), the microtask queue, the `PromiseSlot` settle/resolve/reject/wait
functions, and the timers primitive — moves *down* from `rts-std` into the engine,
alongside the tokio-free `PromiseSlot` *struct* already there. The native tokio
DRIVER (the shared `rt()` runtime + real OS timers) is compiled behind a
`native-async` feature so the engine still builds for wasm/browser (which supplies
a host loop). Both `rts-std` and `rts-node` then consume async from the engine — no
separate `rts-async` crate, no cyclic dependency. The sync (`*Sync`) surface needs
no driver and is implemented first (P-a) regardless.

Each module doc's **§5.7** flags exactly which async infra it needs.

---

## 8. Threading / `worker_threads`

RTS has its own threading model (`docs/specs/rts-threading-model.md`): per-thread
memory regions + a shared heap with promotion-on-publication; surface
`threadLocal` / `shared` / `channel`. Node's `worker_threads` maps onto it:

- `Worker` → an RTS thread with its own region (a fresh JIT/eval context).
- `MessagePort` / `MessageChannel` → an RTS channel; `postMessage` = structured
  clone across the channel (promote to shared heap on publication).
- `SharedArrayBuffer` → shared-heap memory (already primordial); `Atomics`
  operate on it.
- `parentPort` / `workerData` / `threadId` / `isMainThread` → thread-local
  bootstrap state.

Per-module thread-safety (module-global state, watchers, servers bound to a
thread) is documented in each doc's **§5.4**. `cluster` layers over
`worker_threads` + `child_process`.

---

## 9. Buffer / TypedArray interop

`Buffer` **extends `Uint8Array`**. TypedArrays / `ArrayBuffer` / `DataView` are
primordial (engine-owned memory model). Node byte APIs map onto them:

- A `Buffer`/`Uint8Array` argument crosses the ABI as a `Handle` to the backing
  `ArrayBuffer` (+ offset/length) or as `(ptr, len)` for a borrowed view.
- `node:buffer` adds `Blob`, `File`, `atob`/`btoa`, `transcode`, and constants on
  top of the primordial `Uint8Array`.
- The `Buffer` class itself is a `.ts` shim subclassing `Uint8Array`, calling
  native externs for the operations `std`/crates provide (base64, hex, utf-8
  transcode, etc.).

---

## 10. `.ts` shim vs native extern split

Two layers per module:

- **Native externs (Rust, in `rts-node`)** — the irreducible operations that need
  `std`/OS/crates: raw fd read/write, socket connect, spawn, hash update, deflate.
  Typed `extern "C"`, handle-based, minimal.
- **`.ts` shim (shipped by `rts-node`)** — the JS-shaped ergonomics: the classes
  (`Stats`, `Server`, `Socket`, `Buffer`, `URL`), option-object normalization,
  event wiring (EventEmitter), promise/callback adaptation, default arguments.
  Pure TS/JS over the externs and the primordials. No engine hooks — the shim only
  names primordials and calls `rts-node` externs.

This mirrors the existing engine doctrine: the engine names no non-primordial; the
ergonomic surface is `.ts` over native primitives.

---

## 11. `node:v8`, `vm`, `inspector` → RTS engine (NO V8)

**Binding rule: RTS never embeds, emulates, or links V8. Any Node surface that in
Node is backed by V8 internals is backed by the RTS engine instead, for all
callers.** The API surface is preserved for parity; the implementation is RTS.

- **`node:v8`**
  - `serialize` / `deserialize` → RTS structured-serialize (reuse the
    `structuredClone` infra); RTS's own wire format (not V8's), documented.
  - `getHeapStatistics` / `getHeapSpaceStatistics` → RTS mark+sweep collector +
    HandleTable statistics.
  - `writeHeapSnapshot` / `getHeapSnapshot` / `getHeapSnapshotStream` → RTS heap
    snapshot in RTS's own format.
  - `setFlagsFromString` / `--v8-options` → mapped to RTS engine flags where an
    equivalent exists, otherwise a documented no-op.
  - `GCProfiler`, `takeCoverage`, `stopCoverage`, `promiseHooks`,
    `startupSnapshot`, `cachedDataVersionTag` → RTS-engine hooks or documented
    deferrals.
- **`node:vm`** (V8 contexts + `Script` compilation) → RTS JIT + RTS's own
  eval/context (RTS already has runtime `eval`/`eval_file`). `Script`,
  `compileFunction`, `createContext`, `runInContext/NewContext/ThisContext` map to
  RTS compile+run in a fresh context; `SourceTextModule` maps to the RTS module
  loader; `measureMemory` → RTS heap stats.
- **`node:inspector`** (V8 inspector / CDP protocol) → RTS's own debug surface, or
  an explicit deferral documented in `inspector.md`. Do not implement the V8
  inspector protocol.
- **Heap fields elsewhere** — `process.memoryUsage()`, `process.report`, worker
  heap info → RTS heap stats, same source as `node:v8`.

The affected module docs' §5 state this mapping explicitly (they must not say
"embed V8").

---

## 12. Tiers & phasing

Tiers set implementation order (all modules are in scope):

- **P0** (core, first): `fs`, `path`, `os`, `process`, `events`, `buffer`,
  `stream`, `util`, `timers`, `console`, `url`, `assert`, `string_decoder`,
  `querystring`, `worker_threads`, `http`, `globals`.
- **P1**: `crypto`, `net`, `https`, `dns`, `child_process`, `zlib`, `tls`,
  `readline`, `dgram`, `cluster`, `module`, `perf_hooks`, `async_hooks`, `tty`,
  `test`.
- **P2 / experimental / deprecated**: `http2`, `inspector`, `repl`,
  `trace_events`, `diagnostics_channel`, `v8`, `vm`, `wasi`, `sqlite`,
  `punycode`*, `domain`*  (*deprecated).

Cross-cutting **P-1 foundation** (before per-module impl): moving the async
primitive into the engine (§7), the `Entry::Backend` extension point (§6), the
codegen `node:` re-routing (§4), and wiring `rts-node` into the dependency graph +
JIT harvest.

**Prioritization within a tier (owner decision):** do what has a **mature pure-Rust
path first**; push anything whose only pure-Rust option is **immature/experimental**
(node:sqlite via `turso_core` BETA, node:wasi via `wasmi`, and full TLS-provider
hardening) to the **end**. If a capability genuinely has no pure-Rust path, it is
deferred last — but the crate research found a pure-Rust path for every domain, so
"last" here means immature, not impossible.

Every module doc ends with an ordered **§5.8 implementation phases** list (P-a
sync surface → P-b callback/promise → P-c classes/events → …).

---

## 13. Open decisions (need owner sign-off)

1. **Fate of the `rts:*` OS-ish surface.** `rts:fs`/`rts:os`/`rts:net`/… are
   defined by the approved `docs/specs/rts-std-surface.md`. Moving the bodies to
   `rts-node` means either (a) the `rts:*` node-overlapping namespaces re-resolve
   from `rts-node`, or (b) they are dropped in favor of `node:*` + the web-global
   surface. The owner instruction ("keep the modules only in rts-node") points at
   (b), but that contradicts `rts-std-surface.md`. **Recommendation:** (b) — drop
   the `rts:` OS-overlap namespaces; keep `rts:*` only for RTS-unique surface
   (audio, etc.) and Web globals; Node programs use `node:*`. Confirm before
   executing the rts-std removal.
2. **~~`rts-async` crate vs tokio-in-engine~~ — RESOLVED (owner):** async is a
   primitive → it lives in **`rts-engine`**, native tokio driver behind a
   `native-async` feature. No `rts-async` crate. (Kept here for history.)
3. **crypto duplication vs shared primitive.** node:crypto in rts-node gets its
   own hash/CSPRNG (duplication accepted); rts-std keeps its web-crypto primitive
   for the `crypto` global. Confirm the duplication over a shared low-level crate.
4. **`http_server` (actix) vs `node:http` / `node:net`.** rts-node implements
   `node:http` on its own `node:net` stack. The existing actix `http_server` stays
   as an RTS-native high-performance server, separate from `node:http`. Confirm.
