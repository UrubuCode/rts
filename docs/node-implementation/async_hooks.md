# node:async_hooks

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:async_hooks` |
| Node.js version | 25.x |
| Stability | Mixed — `createHook`/`AsyncHook`/`executionAsyncResource`/`asyncWrapProviders`: **1 - Experimental**. `AsyncLocalStorage` (class, `run`, static `bind`, static `snapshot`): **2 - Stable**. `AsyncLocalStorage#exit`/`#enterWith`/`#disable`/`#withScope`: **1 - Experimental**. `AsyncResource` (class): **2 - Stable**. |
| Tier | P1 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import { createHook, executionAsyncId, triggerAsyncId, executionAsyncResource, asyncWrapProviders, AsyncHook, AsyncResource, AsyncLocalStorage } from 'node:async_hooks'` (CJS: `const async_hooks = require('node:async_hooks')`). Node's docs split `AsyncLocalStorage`/`AsyncResource` into a conceptual "async_context" page, but there is **no separate `node:async_context` importable specifier** — both classes are exported from `node:async_hooks` only. RTS mirrors that: one nodespace, one import specifier. |
| Globals exposed | None. `async_hooks` is a plain `node:` import; it does not add anything to `globalThis`. |

## 1. Purpose

`node:async_hooks` gives visibility into, and control over, the lifetime of
asynchronous resources in a Node.js process. It has two largely independent
halves: (1) `createHook`/`AsyncHook` — a low-level, experimental instrumentation
API that fires `init`/`before`/`after`/`destroy`/`promiseResolve` callbacks for
*every* async resource created anywhere in the process (timers, TCP sockets,
promises, fs requests, etc.), historically used by APM/tracing tools; and (2)
`AsyncResource`/`AsyncLocalStorage` — the stable, application-facing primitives
for propagating a "store" (arbitrary context data, e.g. a request id) across
async boundaries without manually threading a parameter through every callback.
`AsyncLocalStorage` is the modern, recommended surface (Node itself steers users
away from `createHook` for anything other than diagnostics) and is the priority
for RTS.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class AsyncHook`

Not constructed directly — instances come only from `createHook()`.

| Member | Signature | Description |
|---|---|---|
| `asyncHook.enable()` | `(): AsyncHook` | Enables the hook's callbacks. Returns `this` for chaining. Idempotent. |
| `asyncHook.disable()` | `(): AsyncHook` | Disables the hook's callbacks. Returns `this` for chaining. Idempotent. |

**Hook callbacks** (fields of the `options` object passed to `createHook`, not
methods on the class — see §2.4 for full signatures): `init`, `before`, `after`,
`destroy`, `promiseResolve`.

#### `class AsyncResource`

| Member | Signature | Description |
|---|---|---|
| constructor | `new AsyncResource(type: string, options?: number \| AsyncResourceOptions)` | Creates a resource and fires the `init` hook. `type` is a free-form user label (e.g. `"DBQuery"`). Legacy shorthand: passing a `number` for `options` is equivalent to `{ triggerAsyncId: options }`. |
| `asyncResource.runInAsyncScope(fn, thisArg?, ...args)` | `<This, Args extends any[], R>(fn: (this: This, ...args: Args) => R, thisArg?: This, ...args: Args): R` | Runs `fn` with the resource's async context as the active context (`executionAsyncId()` reports this resource's id inside `fn`); fires `before`, calls `fn`, fires `after`, restores prior context; returns/rethrows exactly what `fn` returns/throws. |
| `asyncResource.emitDestroy()` | `(): AsyncResource` | Manually fires `destroy`. Must be called **at most once**; a second call throws. Required when `requireManualDestroy: true` was set (GC-triggered auto-destroy is suppressed). Returns `this`. |
| `asyncResource.asyncId()` | `(): number` | This resource's unique id. |
| `asyncResource.triggerAsyncId()` | `(): number` | The id of the resource that caused this one to be created (captured at construction time). |
| `asyncResource.bind(fn, thisArg?)` | `<Func extends (...args: any[]) => any>(fn: Func, thisArg?: any): Func` | Returns a function that runs bound to *this* resource's scope. Since v17.8.0/v16.15.0, `thisArg` defaults to the caller's `this` if omitted. |
| `AsyncResource.bind(fn, type?, thisArg?)` *(static)* | `<Func extends (...args: any[]) => any>(fn: Func, type?: string, thisArg?: any): Func` | Convenience: creates a new `AsyncResource` (named `type`, default = `fn.name`) bound to the **current** execution context and returns `fn` bound to it. |

Instances have no public enumerable data properties beyond what a subclass
adds; `type`/`triggerAsyncId`/`asyncId` are accessed via the methods above, not
plain fields.

#### `class AsyncLocalStorage<T = any>`

| Member | Signature | Description |
|---|---|---|
| constructor | `new AsyncLocalStorage<T>(options?: AsyncLocalStorageOptions<T>)` | `options.defaultValue` — value `getStore()` returns before any `run()`/`enterWith()` (default `undefined`). `options.name` — instance label (added v24.0.0), readable back via `.name`. |
| `asyncLocalStorage.run(store, callback, ...args)` | `<R, TArgs extends any[]>(store: T, callback: (...args: TArgs) => R, ...args: TArgs): R` | Runs `callback` synchronously with `store` as the active store for the callback and everything asynchronously spawned from it. Context is exited automatically when `callback` returns *or throws*. Return value / thrown error propagate through `run()` unchanged. |
| `asyncLocalStorage.exit(callback, ...args)` | `<R, TArgs extends any[]>(callback: (...args: TArgs) => R, ...args: TArgs): R` *(Experimental)* | Runs `callback` synchronously with the store temporarily cleared (`getStore()` → `undefined` inside it); restores the prior store afterward (even on throw). |
| `asyncLocalStorage.enterWith(store)` | `(store: T): void` *(Experimental)* | Mutates the *current* execution context in place to `store`, for the remainder of the current synchronous execution and everything asynchronous that follows from it. Unlike `run()`, this is not scoped — prefer `run()` unless there's a specific reason not to. |
| `asyncLocalStorage.getStore()` | `(): T \| undefined` | Returns the active store, or `undefined`/`defaultValue` outside any context. |
| `asyncLocalStorage.disable()` | `(): void` *(Experimental)* | Disables the instance: exits every context currently linked to it; `getStore()` returns `undefined` until `run()`/`enterWith()` is called again. Required before the instance can be garbage-collected. Does not affect the lifetime of previously stored values. |
| `asyncLocalStorage.withScope(store)` | `(store: T): RunScope` *(Experimental, added v25.9.0)* | Returns a disposable `RunScope` that has entered `store`; intended for `using` (explicit resource management). Restores the previous store when disposed (explicitly or via `using`), even on throw. **Caveat:** inside an `async function`, a scope opened before the first `await` leaks into the *caller's* context once control returns at that `await` — prefer `run()` for anything crossing an `await`. |
| `asyncLocalStorage.getStore()` returns before construction | — | N/A (documented above). |
| `asyncLocalStorage.name` | `readonly name: string` *(added v24.0.0)* | The `name` passed to the constructor, or `""`/`undefined` if none. |
| `AsyncLocalStorage.bind(fn)` *(static)* | `<Func extends (...args: any[]) => any>(fn: Func): Func` *(Stable since v23.11.0/v22.15.0)* | Returns a new function that captures the *current* execution context (across **all** `AsyncLocalStorage` instances, not just one) and re-enters it whenever the returned function is called. |
| `AsyncLocalStorage.snapshot()` *(static)* | `<R, TArgs extends any[]>(): (fn: (...args: TArgs) => R, ...args: TArgs) => R` *(Stable since v23.11.0/v22.15.0)* | Captures the current execution context and returns a function `(fn, ...args) => R` that invokes `fn` inside that captured context whenever called — a lighter-weight replacement for hand-rolling `AsyncResource` in simple cases. |

#### `class RunScope` *(added v25.9.0, Experimental)*

Returned only by `asyncLocalStorage.withScope(store)`.

| Member | Signature | Description |
|---|---|---|
| `scope.dispose()` | `(): void` | Restores the store that was active before `withScope()` was called. Idempotent — calling more than once has no additional effect. |
| `scope[Symbol.dispose]()` | `(): void` | Delegates to `dispose()`; enables `using scope = als.withScope(store)`. |

### 2.2 Top-level functions

#### `async_hooks.createHook(options)`
- **Params**: `options: { init?, before?, after?, destroy?, promiseResolve?, trackPromises?: boolean }` — see §2.4 for callback signatures; `trackPromises` defaults to `true`.
- **Returns**: `AsyncHook` (created disabled — call `.enable()`).
- **Throws**: `TypeError` if any provided hook field is not a function.
- **Variant**: sync (registration only; callbacks fire later, interleaved with the event loop).

#### `async_hooks.executionAsyncId()`
- **Params**: none.
- **Returns**: `number` — the id of the currently executing async resource (the "current" context).
- **Variant**: sync.

#### `async_hooks.triggerAsyncId()`
- **Params**: none.
- **Returns**: `number` — the id of the resource that triggered creation of the currently executing resource.
- **Variant**: sync.

#### `async_hooks.executionAsyncResource()`
- **Params**: none.
- **Returns**: `object` — the resource object associated with the current execution context (for the top-level context, an internal placeholder object).
- **Variant**: sync.

### 2.3 Properties & constants

| Name | Type | Description |
|---|---|---|
| `async_hooks.asyncWrapProviders` | `Map<string, number>` | Maps every internal provider-type name (`TCPWRAP`, `FSREQCALLBACK`, `PROMISE`, `Timeout`, …) Node's own `init` hook may report, to its stable numeric id. Node-internal-implementation surface, exposed read-only for tooling that wants to compare against numeric ids instead of strings. |

### 2.4 Events (hook callbacks)

`async_hooks` has no `EventEmitter`-style `.on(...)` events; the closest
equivalent is the five callback fields accepted by `createHook(options)`. Listed
here as "events" per the doc template, with exact signatures:

| Callback | Signature | Fires |
|---|---|---|
| `init` | `(asyncId: number, type: string, triggerAsyncId: number, resource: object) => void` | Synchronously, at resource construction time (including inside the resource's own constructor, before user code that constructed it continues). |
| `before` | `(asyncId: number) => void` | Immediately before the resource's callback is invoked. |
| `after` | `(asyncId: number) => void` | Immediately after the resource's callback returns (even if it threw — `after` still fires, then the exception propagates via `'uncaughtException'`). |
| `destroy` | `(asyncId: number) => void` | When the resource is destroyed (explicitly via `emitDestroy()` or GC-detected for wrap-style resources). May fire well after the resource stopped being used, or (rarely) not at all if the process exits first. |
| `promiseResolve` | `(asyncId: number) => void` | When the `resolve` function passed to the `Promise` executor is invoked (settling the promise, whether fulfilled or rejected). Only fires when promise tracking is enabled (`trackPromises`, default `true`, and at least one hook enabled). |

All five are optional; omitting one just means that lifecycle event is not
observed by this particular hook. If a hook callback throws, the process
crashes with the error (hook callbacks run outside normal try/catch reach).

## 3. Types & option objects

```ts
interface CreateHookCallbacks {
  init?(asyncId: number, type: string, triggerAsyncId: number, resource: object): void;
  before?(asyncId: number): void;
  after?(asyncId: number): void;
  destroy?(asyncId: number): void;
  promiseResolve?(asyncId: number): void;
  /** default: true */
  trackPromises?: boolean;
}

interface AsyncResourceOptions {
  /** default: executionAsyncId() at construction time */
  triggerAsyncId?: number;
  /**
   * If true, `emitDestroy()` is never called automatically on GC;
   * the caller must call it explicitly. Default: false — in that
   * case, GC-triggered auto-destroy only happens if at least one
   * `destroy` hook is currently active (perf optimization).
   */
  requireManualDestroy?: boolean;
}

interface AsyncLocalStorageOptions<T> {
  /** value returned by getStore() outside any run()/enterWith() scope */
  defaultValue?: T;
  /** instance label, readable back via .name (added v24.0.0) */
  name?: string;
}

/** Returned by AsyncLocalStorage.snapshot() */
type SnapshotRunner = <R, TArgs extends any[]>(fn: (...args: TArgs) => R, ...args: TArgs) => R;

/** Returned by asyncLocalStorage.withScope(store) */
interface RunScope {
  dispose(): void;
  [Symbol.dispose](): void;
}

/** The type-erased resource object seen by init() for Node's own internal
 *  resources ("TCPWRAP", "FSREQCALLBACK", "Timeout", "PROMISE", ...). RTS
 *  only ever passes user-visible resources (its own AsyncResource instances,
 *  and — where wired, see §5.8 — its own timer/promise internals); it does
 *  not attempt to model Node's exact internal wrap object shapes. */
type AsyncResourceLike = object;
```

## 4. Node semantics & edge cases

- **`AsyncLocalStorage` context propagation is explicit at every async
  boundary in the implementation**, not "free" — every place that later runs a
  callback (`Promise` continuation, `setTimeout`, `fs` callback, `net` event) is
  itself instrumented internally (via the same machinery `AsyncResource` exposes)
  to snapshot the context at scheduling time and restore it right before invoking
  the callback. A boundary that is *not* so instrumented (raw C++ addons via
  N-API without `napi_async_context`, custom thenables that don't extend
  `Promise`, hand-rolled callback queues) silently drops context — this is the
  single most common real-world bug category with this API ("context loss").
- **`run()` vs `enterWith()`**: `run()` is scoped to exactly the callback and
  its descendants; `enterWith()` mutates the ambient context for the rest of
  the *current* synchronous turn and beyond, with no automatic exit point — it
  is easy to leak a store into unrelated code that runs later on the same tick
  (e.g. a subsequent independent event-loop callback). Node's own docs recommend
  `run()` as the default and `enterWith()` only when a `run()`-shaped call site
  is not available (e.g. wrapping an existing `EventEmitter`'s `'request'`
  event where you cannot wrap the rest of the request's logic in one callback).
- **`exit()`** is the mirror of `run()` — briefly steps *outside* whatever
  context is active. Both `exit()` and `enterWith()` are marked Experimental
  (unlike the Stable `run()`), reflecting real usability/safety concerns in the
  upstream tracker.
- **`disable()` is mandatory before an `AsyncLocalStorage` instance can be
  GC'd** if any context ever entered it — otherwise the still-linked contexts
  keep it alive. This does not free/clear the store *values* — those are GC'd
  normally, only the instance's tracking machinery is released.
- **`withScope()` async-function gotcha (v25.9.0)**: opening a scope in an
  `async function` body before its first `await` changes the *caller's*
  context once the `async function` suspends and returns control — because the
  synchronous prefix of an `async function` runs on the caller's stack. Correct
  usage confines `withScope()`/`using` to fully synchronous code paths; anything
  crossing an `await` should use `run()` instead.
- **`emitDestroy()` idempotency**: calling it a second time on the same
  `AsyncResource` throws. `requireManualDestroy: false` (the default) still
  auto-emits `destroy` on GC collection of the resource — but *only* if at
  least one `destroy` hook is currently registered process-wide (a perf
  optimization: no hooks active ⇒ no need to track finalization at all).
- **Promise tracking is opt-in-by-hook**: promises get no `asyncId` machinery
  at all unless some `AsyncHook` is enabled (`createHook({...}).enable()`);
  `executionAsyncId()`/`triggerAsyncId()` inside a `.then()` chain only report
  meaningful values once tracking is active. `AsyncLocalStorage`, by contrast,
  does NOT require any hook to be enabled — its context propagation is
  independent, built directly into the promise/timer/callback machinery, not
  layered on top of `createHook`.
- **Ordering**: for a single resource, callback order is always `init` → zero
  or more `before`/`after` pairs (one pair per invocation of the resource's
  callback, e.g. once per `setInterval` tick) → `destroy` (at most once,
  possibly never if the process exits first or GC never runs before exit).
- **Errors inside hook callbacks are fatal** — thrown synchronously out of
  `init`/`before`/`after`/`destroy`/`promiseResolve` crash the process; there is
  no safe way to catch them from inside the hook itself.
- **Deprecations**: the old embedder API (`emitBefore`/`emitAfter` as directly
  callable methods, and the `asyncResource` property Node used to stash on
  functions returned by `resource.bind()`) is deprecated/removed upstream —
  RTS should not implement it.
- **Stability guidance from Node itself**: `createHook`/`AsyncHook`/
  `executionAsyncResource` are flagged Experimental with known "usability
  issues, safety risks, and performance implications"; Node recommends
  `AsyncLocalStorage` for context tracking, `process.getActiveResourcesInfo()`
  for resource introspection, and the separate Diagnostics Channel module for
  tracing — this directly informs the phase ordering in §5.8.
- **Platform differences**: none — this module has no OS-specific behavior;
  everything is pure userland/VM-level bookkeeping.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:async_hooks` has almost no OS-facing surface — it is *entirely* about
threading an id/context through RTS's own execution model. There is no
`std::fs`/`std::net`/etc. counterpart to back it with; the "native impl" is a
small Rust module owned by `rts-node` that maintains:

- a monotonically increasing `asyncId` counter (`AtomicU64`),
- a **context-frame stack**, conceptually `Vec<(AlsSlotId, PolyValue-handle)>`
  per execution context, used by `AsyncLocalStorage` (§5.2/§5.3),
- an `AsyncResource` record type: `{ async_id: u64, trigger_async_id: u64, type_label: String }`,
- an `AsyncHook` record type: `{ init, before, after, destroy, promiseResolve: Option<FunctionHandle>, enabled: bool, track_promises: bool }`
  in a process-wide registry of enabled hooks, walked (best-effort — see §5.8)
  at the few call sites `rts-node` itself controls.

Everything else (actually firing `before`/`after` around a `setTimeout` tick, a
`Promise` settle, an `fs` callback) is **cross-cutting**: it requires the
resource-owning subsystem (timers, promise, future fs/net callback APIs) to call
into this module's dispatcher at the right moments. That coupling is the
central architectural fact of this spec — see §5.3 and especially §5.7.

### 5.2 ABI surface

All symbols `__RTS_FN_NODE_ASYNC_HOOKS_<NAME>`, registered under nodespace
`async_hooks` (`ns_prefix = "node_async_hooks"`) in `rts-node`'s own
`NodespaceSpec`/`NODE_SPECS` table (mirrors the existing `fs`/`path`/`os`/
`process`/`util`/`crypto` pattern in `crates/rts-node/src/lib.rs`, just with the
module's *own* symbol prefix instead of borrowed `__RTS_FN_NS_*` ones).

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_ASYNC_HOOKS_EXECUTION_ASYNC_ID` | — | `I64` | Top of the current thread's resource-id stack, or `0` at top level. |
| `__RTS_FN_NODE_ASYNC_HOOKS_TRIGGER_ASYNC_ID` | — | `I64` | `trigger_async_id` of the current top-of-stack resource. |
| `__RTS_FN_NODE_ASYNC_HOOKS_EXECUTION_ASYNC_RESOURCE` | — | `Handle` | Handle to the current resource's boxed placeholder object (created lazily; top-level context gets a shared singleton handle). |
| `__RTS_FN_NODE_ASYNC_HOOKS_NEXT_ASYNC_ID` | — | `I64` | Internal: allocates and returns the next id from the global counter. Used by `AsyncResource`'s and `AsyncLocalStorage`'s `.ts` constructors; not part of the public JS surface. |
| `__RTS_FN_NODE_ASYNC_HOOKS_RESOURCE_ENTER` | `I64` (async_id), `I64` (trigger_async_id) | `Void` | Pushes a resource-scope frame onto the current thread's stack (used by `runInAsyncScope`, wraps `fn` call in `.ts`). |
| `__RTS_FN_NODE_ASYNC_HOOKS_RESOURCE_EXIT` | — | `Void` | Pops the most recent resource-scope frame (restores prior `executionAsyncId`/`triggerAsyncId`). Paired 1:1 with `RESOURCE_ENTER`; `.ts` wraps the pair in `try/finally`. |
| `__RTS_FN_NODE_ASYNC_HOOKS_ALS_NEW` | `Handle` (default value, may be the `undefined` singleton), `StrPtr` (name, may be empty) | `Handle` | Allocates a new ALS "slot id" record; returns an opaque `Handle` the `.ts` `AsyncLocalStorage` instance wraps. |
| `__RTS_FN_NODE_ASYNC_HOOKS_ALS_GET_STORE` | `Handle` (als handle) | `Handle` | Walks the current thread's context stack for this ALS slot id; returns its value, or the recorded default. |
| `__RTS_FN_NODE_ASYNC_HOOKS_ALS_PUSH` | `Handle` (als handle), `Handle` (store value) | `Void` | Pushes a new scoped frame for this ALS slot (backs `run()` and `withScope()`). |
| `__RTS_FN_NODE_ASYNC_HOOKS_ALS_POP` | `Handle` (als handle) | `Void` | Pops the most recent scoped frame for this ALS slot, restoring whatever was active before (backs `run()`'s `finally` and `RunScope.dispose()`). |
| `__RTS_FN_NODE_ASYNC_HOOKS_ALS_ENTER_WITH` | `Handle` (als handle), `Handle` (store value) | `Void` | Same push as `ALS_PUSH` but with **no** matching pop expected — models `enterWith`'s "for the rest of this execution" persistence. |
| `__RTS_FN_NODE_ASYNC_HOOKS_ALS_DISABLE` | `Handle` (als handle) | `Void` | Clears every frame tagged with this ALS slot id off every live context stack; marks the slot disabled (subsequent `ALS_GET_STORE` returns the default until re-entered). |
| `__RTS_FN_NODE_ASYNC_HOOKS_CONTEXT_SNAPSHOT` | — | `Handle` | Captures the **entire** current thread's context stack (all ALS slots + resource frames) into an opaque snapshot handle. Backs `AsyncLocalStorage.bind`/`.snapshot()` and the promise/timer integration boundary (§5.3). |
| `__RTS_FN_NODE_ASYNC_HOOKS_CONTEXT_RESTORE` | `Handle` (snapshot) | `Handle` (prior-current snapshot, for later un-restore) | Installs the given snapshot as the *current* thread's active stack; returns a handle to what was there before so the caller can restore it afterward (bind/snapshot wrap the user `fn` call in enter/restore-previous). |
| `__RTS_FN_NODE_ASYNC_HOOKS_HOOK_NEW` | `Handle` × 5 (`init`/`before`/`after`/`destroy`/`promiseResolve` `Function` handles, each nullable via the `undefined` singleton), `Bool` (`trackPromises`) | `Handle` | Allocates an `AsyncHook` record (disabled by default). |
| `__RTS_FN_NODE_ASYNC_HOOKS_HOOK_ENABLE` | `Handle` (hook) | `Void` | Marks the hook enabled in the process-wide registry. |
| `__RTS_FN_NODE_ASYNC_HOOKS_HOOK_DISABLE` | `Handle` (hook) | `Void` | Marks the hook disabled (removed from the dispatch set). |

Rich values (the ALS slot record, the `AsyncResource`'s Rust-side struct, the
`AsyncHook` record, snapshots) are all opaque `Handle`s into a `HandleTable`
entry owned by `rts-node`'s own async_hooks module — not `rts-engine`'s
`gc::Entry` enum (which stays primordial-only); see §5.6. `store`/`resource`
JS values themselves are ordinary `PolyValue` handles (Object/Function/String/
etc., or an inline int/bool/float) — the module never inspects or decodes
them, only stores and returns the handle bits.

### 5.3 Async model

- **`executionAsyncId`/`triggerAsyncId`/`executionAsyncResource`**: pure sync
  reads of the current thread's context-frame stack top. No event loop
  involvement.
- **`AsyncResource.runInAsyncScope`**: sync — push, call `fn` (propagating its
  return value/throw), pop in a `finally`.
- **`AsyncLocalStorage.run`/`exit`/`enterWith`/`withScope`**: sync at the call
  site — the *scoping* is synchronous. What makes the store visible later,
  inside code that runs *after* an `await` or inside a `setTimeout`/promise
  callback, is that RTS's async plumbing must itself snapshot+restore around
  every one of its own suspension points:
  - **`async function` / `await` / `Promise`**: per `docs/specs/async-promise-function.md`,
    `async function f(...)` lowers to `f = (args) => promise.create(__async_inner_f, args)`,
    and `promise.create` does `rt.spawn_blocking(move || invoke + settle)` on the
    shared tokio runtime — i.e. the "await" body actually runs on a **different
    OS thread** (a blocking-pool worker), not cooperatively on the calling
    thread. For `AsyncLocalStorage` to survive this hop, `promise.create` must
    call `CONTEXT_SNAPSHOT()` on the calling thread *before* spawning, and the
    spawned closure must call `CONTEXT_RESTORE(snapshot)` as its first action,
    before invoking `__async_inner_f`. This is a **required change to the
    promise subsystem**, not something `rts-node` can do unilaterally — flagged
    in §5.7.
  - **Timers (`setTimeout`/`setInterval`)**: same shape — snapshot at
    scheduling time, restore immediately before firing the JS callback.
  - **Callback-based `fs`/`net` APIs** (once `rts-node` reimplements them
    natively per the owner decision): same shape, at whatever native
    scheduling primitive backs their callbacks.
  - Node's own semantics: context does **not** automatically propagate into
    genuinely parallel work started via `thread.spawn`/a future
    `worker_threads.Worker` — those get a **fresh** top-level context unless a
    snapshot is explicitly captured and restored on the other side (mirrors
    real Node: `worker_threads` does not inherit the parent's `AsyncLocalStorage`
    store either).
- **`AsyncHook` `before`/`after`/`init`/`destroy`**: in real Node these fire
  around *every* resource in the process. In RTS, they can only fire around
  the handful of call sites something actually invokes the shared dispatcher —
  initially just `AsyncResource.runInAsyncScope` (`before`/`after`) and
  `AsyncResource` construction/`emitDestroy` (`init`/`destroy`). Full parity
  (timers, promises, future fs/net) needs each of those subsystems to also call
  the dispatcher; tracked as an explicit deferral (§5.8/§7), consistent with
  Node's own steer toward `AsyncLocalStorage` over `createHook` for anything
  beyond diagnostics.
- **`promiseResolve`**: needs the promise subsystem to call
  `HOOK_DISPATCH_PROMISE_RESOLVE(async_id)` at the exact point `resolve()`/
  `reject()` is invoked — another promise-subsystem touchpoint, same flag.

### 5.4 Multithread / worker interaction

- The context-frame stack is inherently **per-thread** state (`thread_local!`
  in Rust terms) — this matches Node's model, where context is per logical
  "execution context", not shared ambient global state. RTS's actual physical
  threads that need their own stack: the main thread, every tokio
  `spawn_blocking` worker used by `promise.create`, every `thread.spawn`'d OS
  thread, and (future) every `worker_threads.Worker`.
- Because `promise.create` genuinely hops OS threads (§5.3), the context stack
  cannot simply be "the same thread's TLS" the way it can be in real Node
  (single JS thread + libuv callback queue) — RTS must **explicitly** carry a
  snapshot across that hop. This is more explicit plumbing than upstream Node
  needs, but the same *observable* semantics (store survives `await`,
  independent of it being a real OS thread underneath) — it maps directly onto
  the "per-thread region + shared heap with promotion on publication" threading
  model (`docs/specs/rts-threading-model.md`): a store handle captured on
  thread A and restored on thread B must reference a heap object **visible**
  from B, i.e. **promoted to the shared heap** before/at the point of
  `CONTEXT_RESTORE`. A store that is a plain inline PolyValue (number/bool/
  small string) needs no promotion (it round-trips by value); a store that is
  an `Object`/`Array`/user class instance handle **does** need the promotion
  rule applied at snapshot-capture time.
- `worker_threads.Worker` (not yet implemented) should get a **fresh** context
  stack on its dedicated thread by default — matching real Node, where a
  `Worker`'s `AsyncLocalStorage` state is independent of its parent's unless
  the parent explicitly serializes context data through `workerData`/
  `postMessage` (which, being structured-clone/message-based, never carries a
  live `AsyncLocalStorage` store across — only plain data).
- **GC-safety consequence**: context-stack frames hold live `PolyValue` handles
  that are *not* necessarily reachable from any thread's machine stack (they
  live in a Rust `Vec` this module owns) — the conservative stack scanner
  (`mark_stack_roots`, `crates/rts-runtime/src/namespaces/gc/collector.rs`) will
  **not** see them. This module's context-stack storage must be registered as
  an explicit GC root source (a sibling to `thread_registry`), walked during
  `finish_cycle()`'s mark phase, or its held objects will be incorrectly
  collected out from under a suspended context. This is a cross-cutting GC
  requirement to flag alongside §5.7, not something `rts-node` can satisfy by
  itself (the GC lives in `rts-engine`).

### 5.5 Buffer / TypedArray interop

No dedicated byte-marshalling: the `store` value in `AsyncLocalStorage` and the
`resource` object seen by `init` are **arbitrary JS values**, including
`Buffer`/`TypedArray`/`ArrayBuffer` instances — this module only ever stores
and returns the `PolyValue` `Handle`/inline bits it is given, never inspects or
decodes them. Any byte-level interpretation is the caller's concern, identical
to how any other `any`-typed value crosses the ABI (tagged in, tagged out, no
special case here).

### 5.6 Doctrine placement

`node:async_hooks` is **entirely non-primordial** — no native literal syntax,
no interception of what a value *is* (no tag, no trap, no memory model). It
resolves exactly like every other `node:` module: the engine's front end never
names `async_hooks`, `AsyncHook`, `AsyncResource`, or `AsyncLocalStorage`
anywhere. A `node:async_hooks` import maps via `ns_prefix_for("node:async_hooks")`
(data lookup in `NODE_SPECS`, `crates/rts-node/src/lib.rs`) to the codegen
prefix `node_async_hooks`; calls like `node_async_hooks.executionAsyncId()`
resolve generically through `node_lookup`, the same one path `fs`/`path`/`os`/
`process`/`util`/`crypto` already use — zero special-case control flow added to
`crates/rts-codegen-new/`.

`AsyncHook`/`AsyncResource`/`AsyncLocalStorage`/`RunScope` are pure `.ts`
classes shipped from `rts-node`'s own `.ts` shim layer (mirrors the established
"non-primordial JS-shaped API is a `.ts` class over raw externs" pattern used
for `console`/`Map`/`Set`, except here it is namespaced under `node:` and lives
in `rts-node`, not `rts-shared`, per the owner decision that `rts-node` owns
its own independent surface). The classes hold only an opaque `Handle` field
internally and delegate every method to the `__RTS_FN_NODE_ASYNC_HOOKS_*`
externs in §5.2.

### 5.7 Shared-infra dependencies (FLAG)

- **Promise subsystem integration is mandatory, not optional.** For
  `AsyncLocalStorage`/`AsyncResource` to have any real effect across `await`
  boundaries, `promise.create`'s `spawn_blocking` call site (today in
  `rts-std`/the shared promise implementation) must call this module's
  `CONTEXT_SNAPSHOT`/`CONTEXT_RESTORE` primitives. Since `rts-node` cannot
  depend on `rts-std`, and `rts-std` must not gain a node-shaped duplicate of
  this module, **the context-frame-stack primitive itself needs a home both
  sides can reach without violating the dependency graph** — realistically
  hoisted down into `rts-engine` (the common ancestor of
  primitives/shared/std/runtime/node) as a small new leaf module (e.g.
  `rts-engine::async_context`), with `rts-node`'s `.ts` shim and the promise
  subsystem both calling into it directly.
- **Timer callback firing** (currently part of `rts-std`'s globals/timers) needs
  the same snapshot/restore wrapped around invoking a `setTimeout`/`setInterval`
  callback, for the same reason.
- **Shared multi-thread tokio runtime** (`rt()` in `rts-std`'s
  `runtime/async_rt.rs`) is exactly what crosses the OS-thread boundary that
  makes the promise-subsystem integration above necessary in the first place —
  flagged as the concrete reason a "just use thread-local storage" shortcut
  does not work for the `await` case.
- **GC root registration** (`mark_stack_roots`/`thread_registry`,
  `crates/rts-runtime/src/namespaces/gc/collector.rs`, owned by `rts-engine`)
  needs to learn about this module's context-stack storage as an additional
  root source — see §5.4. This is infrastructure `rts-node` cannot add to
  itself since the GC lives below it in the dependency graph.
- **Future `fs`/`net` callback-based APIs**, once `rts-node` reimplements them
  natively, will need the same snapshot/restore wrapping at their own
  callback-invocation points — noted here so that work item is not forgotten
  when those modules are built, even though it lives in their spec, not this
  one.
- If none of the above is wired, `AsyncLocalStorage`/`AsyncResource` still work
  correctly for **purely synchronous** use (nested `run()` calls, `bind`/
  `snapshot` used synchronously) — only the cross-thread-hop cases regress
  silently to "context lost", which is exactly the failure mode real Node docs
  warn about (§4), just triggered by an RTS-specific boundary instead of a
  missing `AsyncResource` wrap.

### 5.8 Implementation phases

a. **Hoist the context-frame-stack primitive** (push/pop/get-top/snapshot/
   restore, keyed by an opaque ALS-slot id) into the shared low crate reachable
   by both `rts-node` and the promise/timer subsystems (§5.7). This is a
   blocking prerequisite (CLAUDE.md Rule C: resolve the blocker before the
   main feature) — state the shift explicitly in the commit.
b. **`AsyncLocalStorage` core**: `ALS_NEW`/`ALS_GET_STORE`/`ALS_PUSH`/
   `ALS_POP`/`ALS_ENTER_WITH`/`ALS_DISABLE` externs + `.ts` class (constructor
   with `defaultValue`/`name`, `run`, `exit`, `enterWith`, `disable`,
   `getStore`, `.name` getter). Ship and test purely-synchronous behavior
   first (nested `run()`, `exit()`, `enterWith()` persistence) — this alone
   covers the majority of real-world single-call-frame use.
c. **`AsyncLocalStorage.bind`/`.snapshot()` (static)**: `CONTEXT_SNAPSHOT`/
   `CONTEXT_RESTORE` + `.ts` static methods, still exercised only
   synchronously at this phase.
d. **`AsyncResource`**: `NEXT_ASYNC_ID`/`RESOURCE_ENTER`/`RESOURCE_EXIT` +
   `HOOK`-independent `init`/`destroy` bookkeeping + `.ts` class (constructor,
   `runInAsyncScope`, `emitDestroy`, `asyncId`, `triggerAsyncId`, instance
   `bind`, static `bind`).
e. **`executionAsyncId`/`triggerAsyncId`/`executionAsyncResource`** top-level
   functions, backed by the same stack `AsyncResource`/`AsyncLocalStorage`
   already maintain.
f. **`withScope`/`RunScope`** (v25.9.0): depends on confirming the RTS
   parser/HIR supports `using` declarations / `Symbol.dispose` at all — verify
   that language-feature prerequisite before starting (see §7); if unsupported,
   ship `withScope()` returning a plain object with `.dispose()` and defer
   `using`-sugar support.
g. **Promise/timer integration** (the §5.3/§5.7 wiring): snapshot at
   `promise.create`'s `spawn_blocking` call and at timer scheduling, restore
   before invoking the user body/callback. This is the phase that makes
   `AsyncLocalStorage` actually survive `await` and `setTimeout` — coordinate
   with whoever owns the promise/timer subsystem; not solely an `rts-node`
   change.
h. **GC root registration** for the context-stack storage (§5.4) —
   coordinate with the GC/stack-map owner; needed before any workload that
   triggers a GC cycle while a cross-thread-restored store is the only live
   reference to a heap object.
i. **`createHook`/`AsyncHook`** best-effort: wire `init`/`destroy` around
   `AsyncResource` construction/`emitDestroy`, `before`/`after` around
   `runInAsyncScope`; ship `asyncWrapProviders` as a static compat constant
   (best-effort mapping, not 1:1 with Node's internal provider set). Document
   explicitly, in the module's own status notes, that this does **not** cover
   timers/fs/net/promise resources end-to-end until those subsystems also call
   the dispatcher (tracked per-module, not blocking this phase).
j. **Test fixtures + cross-runtime measurement** (§6).

## 6. Test plan

`tests/node_async_hooks_*.test.ts` (`rts:test` format):

- **ALS basic**: `new AsyncLocalStorage()`, `run(store, () => getStore())` returns
  `store`; `getStore()` outside any `run()` returns `undefined`.
- **ALS default value**: `new AsyncLocalStorage({ defaultValue: 'd' })`,
  `getStore()` outside `run()` returns `'d'`.
- **ALS nested run**: `run(a, () => run(b, () => getStore()))` returns `b`
  inside, `a` after the inner `run()` returns, `undefined` after the outer.
- **ALS run + throw**: store visible inside a `try` that throws inside `run()`,
  `getStore()` back to `undefined` in the surrounding `catch` (mirrors the
  Node docs example exactly).
- **ALS across `setTimeout`**: `run(store, () => setTimeout(() => assert
  getStore() === store))` — exercises the timer-integration phase (§5.8g);
  document as expected-fail until that phase lands, then flip to expected-pass.
- **ALS across `await`/`async function`**: same assertion but through an
  `async function` body awaited from inside `run()` — exercises the
  promise-integration phase (§5.8g).
- **`exit()`**: inside `run(store, () => { exit(() => assert getStore() ===
  undefined); assert getStore() === store; })`.
- **`enterWith()` persistence**: calling `enterWith(store)` mid-function, then
  asserting `getStore() === store` for the rest of that synchronous function
  **and** inside a subsequent independent `setTimeout` scheduled afterward
  (demonstrates the "leaks past the caller" character `run()` avoids).
- **`disable()`**: `run(store, () => { disable(); assert getStore() ===
  undefined; })`, then a fresh `run()` after works again.
- **`withScope`/`using`**: `{ using _ = als.withScope('x'); assert
  getStore() === 'x'; }` then `assert getStore() === undefined` after the
  block — plus the manual (`no `using``) variant calling `scope.dispose()`
  explicitly, and asserting a second `dispose()` call is a harmless no-op.
- **`AsyncLocalStorage.bind`**: capture a function inside `run(store, () =>
  AsyncLocalStorage.bind(fn))`, call the bound function later (outside any
  `run()`), assert it still observes `store`.
- **`AsyncLocalStorage.snapshot`**: reproduce the docs' `Foo` class pattern —
  a method captures `AsyncLocalStorage.snapshot()` at construction time inside
  one `run()`, and a later call to that method from inside a *different*
  `run()` still observes the store captured at construction.
- **`AsyncResource.runInAsyncScope`**: `executionAsyncId()` inside the callback
  differs from the id observed outside it; return value and thrown errors both
  propagate through `runInAsyncScope` unchanged.
- **`AsyncResource.emitDestroy` once**: second call throws.
- **`AsyncResource` static `bind`**: bind a plain function, call it, assert
  `executionAsyncId()` inside matches the resource's id, not the caller's.
- **`executionAsyncId`/`triggerAsyncId` basic sanity**: both are positive
  integers; a resource's `triggerAsyncId()` matches whatever
  `executionAsyncId()` was active at its construction.
- **`createHook` best-effort**: register `init`/`before`/`after`/`destroy` on
  an explicit `new AsyncResource(...)` + `runInAsyncScope`/`emitDestroy` call
  sequence and assert the four counts observed match 1/1/1/1 — scoped
  deliberately to what phase (i) actually wires, not full-runtime coverage.
- **Multithread**: a store set via `run()` on the main thread is visible
  inside a `.then()`/`await` continuation that (per RTS's async model)
  actually executes on a different tokio blocking-pool OS thread — asserts the
  cross-thread snapshot/restore plumbing (§5.3/§5.4) actually works, not just
  the synchronous case. A second fixture spawns an unrelated
  `thread.spawn`'d thread from inside a `run()` scope and asserts that thread's
  `getStore()` is `undefined` (no implicit inheritance across an unrelated OS
  thread, matching real Node's `worker_threads` behavior).

## 7. Open questions / deferrals

- **Full `createHook`/`AsyncHook` parity** (every resource type — timers,
  promises, future `fs`/`net`) requires each resource-owning subsystem to call
  this module's dispatcher; deferred until those subsystems are themselves
  rewritten natively in `rts-node`/their own specs. Node's own stance (steer
  users to `AsyncLocalStorage`, keep `createHook` Experimental) means this is
  intentionally the lowest-priority slice of this spec.
- **`using`/explicit resource management language support** is a prerequisite
  for a native `withScope()`/`RunScope` — needs a yes/no answer from whoever
  owns the parser/HIR before phase (f) starts; if `using` sugar is unsupported,
  ship the plain-object `RunScope` (still usable via manual `.dispose()`) and
  revisit the `using` ergonomics later.
- **`asyncWrapProviders` exact contents**: Node's provider-id set (`TCPWRAP`,
  `FSREQCALLBACK`, `PROMISE`, `Timeout`, …) is tied to Node's own internal C++
  resource taxonomy, which does not correspond 1:1 to RTS's actual resource
  types. Decide whether to ship a best-effort static compat map (covering the
  handful of names real-world code actually checks for, e.g. `"PROMISE"`,
  `"Timeout"`) or omit the property entirely until a concrete consumer needs it.
- **GC root registration for the context-stack** (§5.4/§5.7h) needs sign-off
  from whoever owns `rts-engine`'s GC/stack-map work — this spec proposes the
  shape (an explicit root source parallel to `thread_registry`) but does not
  itself decide the implementation.
- **`worker_threads` interaction** is written here on the assumption of "fresh
  context per Worker, no auto-inheritance" (matching real Node) — revisit once
  the `node:worker_threads` module itself has a spec, since the exact thread/
  region lifecycle mapping (`docs/specs/rts-threading-model.md`) may add
  nuance (e.g. whether a `Worker`'s dedicated thread pre-seeds anything from
  `workerData`).
- **Embedder API** (`emitBefore`/`emitAfter`/the deprecated
  `asyncResource`-on-bound-function property) is intentionally **not** planned
  — Node itself deprecated/removed it upstream.
- **`asyncId`/`triggerAsyncId` ABI width**: chosen as `I64` here (ids are a
  simple incrementing counter, i64 has no realistic overflow risk); the `.ts`
  surface still types them as JS `number`, so the usual float64 boundary
  coercion applies like any other numeric ABI return — flagged only in case a
  future review prefers `U64` for a nonnegative-only counter.
