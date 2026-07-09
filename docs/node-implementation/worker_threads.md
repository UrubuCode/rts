# node:worker_threads

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:worker_threads` |
| Node.js version (25.x) | Documented against Node.js 25.x. `getEnvironmentData`/`setEnvironmentData` stable since v17.5.0/v16.15.0; `BroadcastChannel` stable since v18.0.0; `postMessageToThread` added v22.5.0 (Stability 1.1); `locks`/`LockManager`/`Lock` added v24.5.0 (Stability 1, Experimental); `worker.cpuUsage()` added v24.6.0/v22.19.0; `worker.startCpuProfile()` added v24.8.0; `worker.startHeapProfile()` added v24.9.0/v22.20.0; `worker[Symbol.asyncDispose]()` added v24.2.0/v22.18.0; `port.hasRef()` Stable since v24.0.0. |
| Stability | 2 - Stable (module core); 1.1 - Active development (`postMessageToThread`); 1 - Experimental (`locks`/`LockManager`/`Lock`) |
| Tier | P0 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import { Worker, isMainThread, parentPort, workerData, MessageChannel, MessagePort, BroadcastChannel, SHARE_ENV, threadId, resourceLimits, getEnvironmentData, setEnvironmentData, markAsUntransferable, markAsUncloneable, isMarkedAsUntransferable, moveMessagePortToContext, receiveMessageOnPort, postMessageToThread, locks } from "node:worker_threads"` (named ESM); `const { Worker, ... } = require("node:worker_threads")` (CJS); no default export. |
| Globals exposed | None from this module directly, but `Worker` is itself commonly used to construct the module's central primitive; RTS also exposes `MessageChannel`, `MessagePort`, `BroadcastChannel`, and `structuredClone` as WHATWG ambient globals elsewhere (see `docs/node-implementation/events.md` §5.6 and the web-globals surface) — `node:worker_threads` re-exports the *same* classes rather than defining new ones, exactly like `node:events`/`EventTarget`. |

## 1. Purpose

`node:worker_threads` lets a single Node.js process run JavaScript/TypeScript on multiple OS threads that share one process address space, one set of file descriptors, and (opt-in) `SharedArrayBuffer` memory — cheaper to spawn than `node:cluster`'s full OS processes, at the cost of losing process-level fault isolation. Each `Worker` runs a fully separate JS engine instance/environment (its own globals, its own event loop, its own module registry) but communicates with its parent and siblings only through structured-clone `postMessage` over `MessagePort`s (plus explicit `SharedArrayBuffer` transfer for true shared memory). This module is **central to the RTS multithread goal**: it is the concrete Node-API surface the engine's `docs/specs/rts-threading-model.md` (per-thread regions + shared heap with promotion-on-publication) is built to serve — a `Worker` is the user-facing embodiment of "a thread with its own region," and `MessagePort`/`postMessage` is the user-facing embodiment of the promotion-on-publication write barrier.

## 2. Exported API surface (COMPLETE)

### Classes

#### `class Worker extends EventEmitter`

Represents one live worker thread, each a completely separate JS engine/environment instance. Constructed directly by user code (unlike `cluster.Worker`, which is only ever obtained from `cluster.fork()`).

**Constructor**

```ts
new Worker(filename: string | URL, options?: WorkerOptions)
```

`filename` — path to the entry script/module (relative paths resolved relative to the *calling* module, not `process.cwd()`), a `file:` URL, or (when `options.eval === true`) a raw string of JavaScript/TypeScript source. When `filename` is not an absolute path and not prefixed with `./`/`../`, and `options.eval` is not set, it is resolved as a module specifier relative to the parent's module resolution (supports `data:` URLs with the `text/javascript` module-registry pattern for eval'd ESM). See `WorkerOptions` in §3.

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `worker.threadId` | `number` (integer) | Unique across the lifetime of the process; matches the value observed as `threadId` from *inside* the worker via `worker_threads.threadId`. |
| `worker.threadName` | `string \| null` | The `name` option echoed back, or `null` if not set/not yet running. |
| `worker.stdin` | `stream.Writable \| null` | Writable stream piping into the worker's `process.stdin`; `null` unless `options.stdin === true`. |
| `worker.stdout` | `stream.Readable` | Readable stream of the worker's `process.stdout`. By default this data is *also* auto-piped through to the parent's own `process.stdout`; passing `options.stdout: true` disables that auto-piping so the caller must consume this stream itself. |
| `worker.stderr` | `stream.Readable` | Same as `stdout`, mirrored for `process.stderr` / `options.stderr`. |
| `worker.resourceLimits` | `ResourceLimits` (see §3) | Empty object (`{}`) once the worker has stopped. |
| `worker.performance` | `WorkerPerformance` (see §3) | `eventLoopUtilization()` scoped to this worker; see below. |

**Instance methods**

##### `worker.postMessage(value, transferList?): void`

| name | type | optional | default |
|---|---|---|---|
| `value` | `any` (structured-clonable) | no | — |
| `transferList` | `Array<ArrayBuffer \| MessagePort \| FileHandle>` | yes | `[]` |

Returns: `void`. Throws: `DataCloneError` (a `DOMException`) synchronously if `value` contains an object marked via `markAsUncloneable`, or a non-transferable object present in `value` but omitted from `transferList` while required (`ERR_MISSING_MESSAGE_PORT_IN_TRANSFER_LIST` for MessagePort-shaped payload members). Variant: **sync call, async delivery** — the worker receives it as a `'message'` event on its `parentPort`.

##### `worker.terminate(): Promise<number>`

No parameters. Returns a `Promise<number>` that resolves with the exit code once the `'exit'` event has fired. Stops JS execution "as soon as possible" — may abort in the middle of an async operation. Throws: none synchronously.

##### `worker.ref(): void`

No parameters, no return. Undoes a prior `.unref()`; default state (an active `Worker` keeps the event loop alive).

##### `worker.unref(): void`

No parameters, no return. Lets the parent process/thread exit even if this `Worker` is still running.

##### `worker.getHeapSnapshot(options?): Promise<stream.Readable>`

| name | type | optional | default |
|---|---|---|---|
| `options.exposeInternals` | `boolean` | yes | `false` |
| `options.exposeNumericValues` | `boolean` | yes | `false` |

Returns a `Promise` resolving to a readable stream of a JSON V8 heap snapshot. Throws (rejects) `ERR_WORKER_NOT_RUNNING` if the worker has already exited.

##### `worker.getHeapStatistics(): Promise<HeapStatistics>`

No parameters. Same shape as `v8.getHeapStatistics()`. Rejects `ERR_WORKER_NOT_RUNNING` if not running.

##### `worker.cpuUsage(prev?): Promise<{ user: number, system: number }>`

| name | type | optional | default |
|---|---|---|---|
| `prev` | `{ user: number, system: number }` | yes | none |

*(Added v24.6.0, v22.19.0.)* Returns microsecond CPU-time deltas for the worker thread, mirroring `process.cpuUsage()`. Rejects `ERR_WORKER_NOT_RUNNING` if not running.

##### `worker.startCpuProfile(): Promise<CpuProfileHandle>`

*(Added v24.8.0.)* No parameters. Resolves to a disposable handle (supports `await using`) that stops CPU profiling and yields the profile when disposed — see §3 `CpuProfileHandle`.

##### `worker.startHeapProfile(): Promise<HeapProfileHandle>`

*(Added v24.9.0, v22.20.0.)* No parameters. Same disposable-handle shape as above, for heap sampling profiles — see §3 `HeapProfileHandle`.

##### `worker[Symbol.asyncDispose](): Promise<void>`

*(Added v24.2.0, v22.18.0.)* Equivalent to `.terminate()`, invoked automatically when the `Worker` is the subject of an `await using` declaration.

**Instance events**

| Event | Listener signature | Notes |
|---|---|---|
| `'online'` | `() => void` | Fires once the worker has started executing JS (its bootstrap is complete). Before this event, `worker.performance.eventLoopUtilization()` reports all-zero. |
| `'message'` | `(value: any) => void` | Fires when the worker's `parentPort.postMessage()` sends a structured-clone value. |
| `'messageerror'` | `(error: Error) => void` | Fires when deserializing an incoming message fails. |
| `'error'` | `(err: any) => void` | Fires on an uncaught exception inside the worker; the worker terminates immediately afterward (an `'exit'` follows). |
| `'exit'` | `(exitCode: number) => void` | Final event for this `Worker`; no further events fire afterward. `exitCode` is the value passed to `process.exit()` inside the worker, or `1` if the worker was terminated (`.terminate()` or an uncaught `'error'`). |

**`worker.performance` object**

##### `worker.performance.eventLoopUtilization(utilization1?, utilization2?): EventLoopUtilization`

| name | type | optional |
|---|---|---|
| `utilization1` | `EventLoopUtilization` | yes |
| `utilization2` | `EventLoopUtilization` | yes |

Returns `{ idle: number, active: number, utilization: number }` (see §3), queried from *outside* the worker (mirrors `perf_hooks.performance.eventLoopUtilization()` called from inside it). Zero in all fields before `'online'` and after `'exit'`.

---

#### `class MessageChannel`

**Constructor**: `new MessageChannel()` — no arguments.

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `channel.port1` | `MessagePort` | First endpoint of the connected pair. |
| `channel.port2` | `MessagePort` | Second endpoint. Messages posted to one arrive on the other. |

---

#### `class MessagePort extends EventTarget`

One endpoint of a `MessageChannel` pair, or the special `parentPort` connecting a `Worker` to its parent. **The same `MessagePort` class as the ambient WHATWG global** (RTS exposes `MessagePort`/`MessageChannel` globally per the web-globals surface); `node:worker_threads` re-exports it, it does not redefine it.

**Instance methods**

##### `port.postMessage(value, transferList?): void`

| name | type | optional | default |
|---|---|---|---|
| `value` | `any` (structured-clonable) | no | — |
| `transferList` | `Array<ArrayBuffer \| MessagePort \| FileHandle>` | yes | `[]` |

Same semantics/throws as `worker.postMessage` above.

##### `port.start(): void`

No parameters. Begins delivering queued `'message'` events. Automatically invoked the first time a `'message'` listener is attached (`.on('message', ...)`/`.addEventListener('message', ...)`); needed explicitly only when consuming via the `'close'`/async-iterator style without ever attaching a `'message'` listener directly.

##### `port.close(): void`

No parameters. Closes the port; both `port1` and `port2` (or a `MessagePort` and its `parentPort` counterpart) emit `'close'`. Further `postMessage()` calls after close are dropped silently (no throw).

##### `port.ref(): void`

No parameters. Default state — an active port keeps the owning thread's event loop alive.

##### `port.unref(): void`

No parameters. Opposite of `.ref()` — lets the thread exit even with this port still open.

##### `port.hasRef(): boolean`

*(Stability 2 — Stable since v24.0.0.)* No parameters. Returns whether this port currently counts toward keeping its thread alive.

**Instance events**

| Event | Listener signature | Notes |
|---|---|---|
| `'message'` | `(value: any) => void` | Structured-clone-decoded payload from the other end's `postMessage()`. |
| `'messageerror'` | `(error: Error) => void` | Deserialization failure. |
| `'close'` | `() => void` | Fires on both ends once either side closes (explicit `.close()`, GC of the other end, or the owning `Worker` exiting). |

---

#### `class BroadcastChannel extends EventTarget`

**Constructor**: `new BroadcastChannel(name: any)` — `name` is coerced to a string via template-literal semantics (`` `${name}` ``). Also available as an ambient WHATWG global (see `docs/node-implementation/events.md` §5.6-style globals); this module re-exports the same class.

**Instance methods**

##### `bc.postMessage(message): void`

| name | type | optional |
|---|---|---|
| `message` | `any` (structured-clonable) | no |

Broadcasts a structured-clone of `message` to every other open `BroadcastChannel` instance (in *any* thread of the process) constructed with the same `name`. The sender itself does not receive its own message.

##### `bc.close(): void`

No parameters. Disconnects this channel instance; it stops receiving and can no longer send.

##### `bc.ref(): void` / `bc.unref(): void`

Same semantics as `MessagePort.ref`/`.unref()`.

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `bc.name` | `string` (read-only) | The channel name passed to the constructor. |
| `bc.onmessage` | `((event: MessageEvent<any>) => void) \| null` | Convenience property-style handler, equivalent to `addEventListener('message', ...)` with at most one active callback. |
| `bc.onmessageerror` | `((event: MessageEvent<Error>) => void) \| null` | Property-style handler for `'messageerror'`. |

**Instance events**: `'message'` (`(event: MessageEvent<any>) => void`), `'messageerror'` (`(event: MessageEvent<Error>) => void`) — dispatched as `EventTarget` events (an `event.data` payload), not plain-value `EventEmitter` events like `MessagePort`'s.

---

#### `class Lock` *(Stability 1 — Experimental, added v24.5.0)*

Represents one held lock, passed into a `locks.request()` callback. No public constructor (never constructed directly by user code).

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `lock.name` | `string` (read-only) | The lock name this instance was requested under. |
| `lock.mode` | `'shared' \| 'exclusive'` (read-only) | The mode this lock was acquired in. |

---

#### `class LockManager` *(Stability 1 — Experimental, added v24.5.0)*

Not constructed by user code; reached via the module-level `locks` singleton.

##### `locks.request(name, options?, callback): Promise<any>`

| name | type | optional | default |
|---|---|---|---|
| `name` | `string` | no | — |
| `options.mode` | `'exclusive' \| 'shared'` | yes | `'exclusive'` |
| `options.ifAvailable` | `boolean` | yes | `false` |
| `options.steal` | `boolean` | yes | `false` |
| `options.signal` | `AbortSignal` | yes | none |
| `callback` | `(lock: Lock \| null) => any \| Promise<any>` | no | — |

Returns: `Promise<any>` resolving/rejecting with whatever `callback` returns/throws. `callback` receives `null` instead of a `Lock` only when `ifAvailable: true` and the lock could not be acquired immediately. The lock is released automatically once `callback`'s return value (or the promise it returns) settles — never released manually. Throws (rejects): `AbortError` if `signal` fires before acquisition. Variant: **async/promise**.

##### `locks.query(): Promise<LockManagerSnapshot>`

No parameters. Returns a `Promise` resolving to `{ held: Lock[], pending: { name: string, mode: string }[] }` — a point-in-time snapshot of every lock currently held/queued *anywhere in the process* (locks are process-wide, spanning all `Worker` threads, not per-thread).

### Top-level functions

##### `getEnvironmentData(key): any`

| name | type | optional |
|---|---|---|
| `key` | `any` (any cloneable value usable as a Map key) | no |

Returns: a structured-clone of the value set by an ancestor thread's `setEnvironmentData(key, value)` (searched up the worker-spawn chain), or `undefined` if never set. Variant: sync. Throws: none.

##### `setEnvironmentData(key, value?): void`

| name | type | optional | default |
|---|---|---|---|
| `key` | `any` | no | — |
| `value` | `any` | yes | `undefined` (deletes the entry for `key`) |

Sets data visible to `key`-lookups from `getEnvironmentData` in **every Worker spawned by this thread from this point forward** (not retroactive to already-running workers, and not visible back up to the parent). Variant: sync, void. Throws: none.

##### `markAsUntransferable(object): void`

| name | type | optional |
|---|---|---|
| `object` | `any` | no |

No-op for primitives. Marks `object` so any future attempt to include it in a `postMessage()` `transferList` throws `DataCloneError` instead of transferring — the object remains cloneable (deep-copied) if not otherwise excluded. **Irreversible.** Variant: sync, void.

##### `isMarkedAsUntransferable(object): boolean`

| name | type | optional |
|---|---|---|
| `object` | `any` | no |

Returns whether a prior `markAsUntransferable()` call applies to `object`. Variant: sync.

##### `markAsUncloneable(object): void`

| name | type | optional |
|---|---|---|
| `object` | `any` | no |

Marks `object` so using it as (or nested inside) the `value` argument of any `postMessage()` throws `DataCloneError`. No effect on `ArrayBuffer` or `Buffer`-like objects (they follow their own transfer/clone rules regardless). **Irreversible.** Variant: sync, void.

##### `isMarkedAsUncloneable(object): boolean` *(verify — presence/name inferred from the `markAsUncloneable` pairing pattern; confirm exact export name against the v25.x doc build before implementing)*

Returns whether `object` was previously marked via `markAsUncloneable()`. Variant: sync.

##### `moveMessagePortToContext(port, contextifiedSandbox): MessagePort`

| name | type | optional |
|---|---|---|
| `port` | `MessagePort` | no |
| `contextifiedSandbox` | `object` (a context created via `vm.createContext()`) | no |

Returns a new `MessagePort` bound to the given `vm` context; `port` becomes unusable afterward (equivalent to a transfer). Variant: sync. RTS note: depends on a `node:vm` contextify implementation existing first — see §7.

##### `receiveMessageOnPort(port): { message: any } | undefined`

| name | type | optional |
|---|---|---|
| `port` | `MessagePort \| BroadcastChannel` | no |

Synchronously dequeues and returns exactly one pending message (wrapped as `{ message }`) without going through the normal `'message'` event/async delivery path, or `undefined` if none is queued. Used for polling a port from synchronous code (e.g. inside a callback where `await` is unavailable). Variant: **sync**, does not start async event delivery for this port.

##### `postMessageToThread(threadId, value, transferList?, timeout?): Promise<void>`

*(Stability 1.1 — Active development, added v22.5.0.)*

| name | type | optional | default |
|---|---|---|---|
| `threadId` | `number` | no | — |
| `value` | `any` (structured-clonable) | no | — |
| `transferList` | `Array<ArrayBuffer \| MessagePort \| FileHandle>` | yes | `[]` |
| `timeout` | `number` (milliseconds) | yes | `undefined` (wait forever) |

Returns: `Promise<void>`, resolving once the target thread's `'workerMessage'` process-event listener has processed the message. Throws (rejects): `ERR_WORKER_MESSAGING_FAILED` (target has no `'workerMessage'` listener), `ERR_WORKER_MESSAGING_ERRORED` (the listener threw), `ERR_WORKER_MESSAGING_TIMEOUT` (exceeded `timeout`), `ERR_WORKER_MESSAGING_SAME_THREAD` (`threadId` is the caller's own). Variant: **async/promise** — a direct thread-to-thread send that does *not* require a pre-established `MessageChannel`/`MessagePort`.

### Properties & constants

| Name | Type | Notes |
|---|---|---|
| `isMainThread` | `boolean` (read-only) | `true` iff current code is not running inside a `Worker`. |
| `isInternalThread` | `boolean` (read-only) | `true` iff running inside a Node-internal worker (e.g. a loader thread) — practically always `false` for RTS user code; document as always `false` unless/until RTS grows internal worker threads of its own. |
| `threadId` | `number` (read-only, integer) | Unique, process-lifetime identifier for the current thread; `0` on the main thread. |
| `threadName` | `string \| null` (read-only) | The current thread's `name` (from `WorkerOptions.name`), or `null`. |
| `workerData` | `any` (read-only) | Structured-clone of the value passed as `WorkerOptions.workerData`; `undefined` on the main thread or if not passed. |
| `parentPort` | `MessagePort \| null` (read-only) | The port back to the parent thread; `null` on the main thread. |
| `resourceLimits` | `ResourceLimits` (read-only, see §3) | Empty object (`{}`) on the main thread; the *effective* engine resource constraints inside a worker. |
| `SHARE_ENV` | `symbol` (constant) | Sentinel passed as `WorkerOptions.env` to request write-shared `process.env` between parent and worker. |
| `locks` | `LockManager` (singleton instance) | *(Stability 1 — Experimental.)* |

### Events

`node:worker_threads` itself emits no module-level events (unlike `node:cluster`). All events are instance events on `Worker`/`MessagePort`/`BroadcastChannel` (listed above), plus the `process`-level `'workerMessage'` event (on the *receiving* thread's global `process` object) that `postMessageToThread()` targets — documented under `node:process`, referenced here for completeness since `postMessageToThread` is meaningless without it.

## 3. Types & option objects

```ts
interface WorkerOptions {
  /** Arguments appended to `process.argv` inside the worker (as strings). Not usable with `eval: true`. */
  argv?: any[];
  /**
   * Initial `process.env` inside the worker. Default: a copy of the current thread's
   * `process.env` at construction time. Pass the `SHARE_ENV` symbol to make the worker's
   * `process.env` the SAME live object as the parent's (read AND write shared).
   */
  env?: Record<string, string> | typeof SHARE_ENV;
  /** If true, `filename` is treated as JavaScript source text rather than a path/URL. Default: false. */
  eval?: boolean;
  /** Node CLI flags for the worker's engine instance. Default: inherited from the parent (a fixed allow-listed subset in real Node; RTS may not need this restriction — see §4). */
  execArgv?: string[];
  /** If true, `worker.stdin` is a writable stream feeding the worker's `process.stdin`. Default: false (worker's stdin is immediately EOF). */
  stdin?: boolean;
  /** If true, `worker.stdout` does NOT auto-pipe to the parent's `process.stdout` (caller must consume it). Default: false (auto-piped). */
  stdout?: boolean;
  /** If true, `worker.stderr` does NOT auto-pipe to the parent's `process.stderr`. Default: false (auto-piped). */
  stderr?: boolean;
  /** Structured-clone value made available inside the worker as `workerData`. */
  workerData?: any;
  /** Track/clean up file descriptors the worker leaves open on unclean exit. Default: true. */
  trackUnmanagedFds?: boolean;
  /** Objects to transfer (rather than clone) as part of `workerData`. */
  transferList?: Array<ArrayBuffer | MessagePort | import('node:fs/promises').FileHandle>;
  /** JS-engine resource constraints for this worker's own heap; does not bound external/native allocations. */
  resourceLimits?: ResourceLimits;
  /**
   * Debugging label for this worker (surfaced in thread dumps/inspector).
   * Truncated at 32767 chars (Windows), 64 (macOS), 16 (Linux) — platform-specific OS thread-name limits.
   */
  name?: string;
}

interface ResourceLimits {
  /** Default: 4. Larger stacks cost more memory per worker; also raises native-stack-overflow risk if set too small for deep recursion. */
  stackSizeMb?: number;
  maxYoungGenerationSizeMb?: number;
  maxOldGenerationSizeMb?: number;
  codeRangeSizeMb?: number;
}

/** Returned (read-only) shape of `worker.resourceLimits` / module-level `resourceLimits`. */
interface EffectiveResourceLimits {
  maxYoungGenerationSizeMb: number;
  maxOldGenerationSizeMb: number;
  codeRangeSizeMb: number;
  stackSizeMb: number;
}

interface WorkerPerformance {
  eventLoopUtilization(
    utilization1?: EventLoopUtilization,
    utilization2?: EventLoopUtilization,
  ): EventLoopUtilization;
}

interface EventLoopUtilization {
  idle: number;
  active: number;
  utilization: number; // active / (active + idle), in [0, 1]
}

interface HeapSnapshotOptions {
  exposeInternals?: boolean; // default false
  exposeNumericValues?: boolean; // default false
}

/** Same shape as v8.getHeapStatistics(); listed here for completeness (see node:v8 spec for the full field list). */
interface HeapStatistics {
  total_heap_size: number;
  total_heap_size_executable: number;
  total_physical_size: number;
  total_available_size: number;
  used_heap_size: number;
  heap_size_limit: number;
  malloced_memory: number;
  peak_malloced_memory: number;
  does_zap_garbage: number;
  number_of_native_contexts: number;
  number_of_detached_contexts: number;
  [key: string]: number; // additional fields per V8 version
}

interface CpuUsage {
  user: number; // microseconds
  system: number; // microseconds
}

/** Disposable handle returned by worker.startCpuProfile(); supports `await using`. */
interface CpuProfileHandle {
  /** Stops profiling and returns the collected profile (JSON, Chrome DevTools format). */
  stop(): Promise<object>;
  [Symbol.asyncDispose](): Promise<void>; // calls stop() and discards the result if not already stopped
}

/** Disposable handle returned by worker.startHeapProfile(); supports `await using`. */
interface HeapProfileHandle {
  stop(): Promise<object>;
  [Symbol.asyncDispose](): Promise<void>;
}

/** Callback shape for MessagePort/Worker 'message'/'messageerror' events (EventEmitter-style, plain value). */
type NodeMessageListener = (value: any) => void;
type NodeMessageErrorListener = (error: Error) => void;

/** Callback shape for BroadcastChannel 'message'/'messageerror' events (EventTarget-style, wrapped). */
interface MessageEventLike<T> {
  data: T;
  // other MessageEvent fields (origin, lastEventId, source, ports) are present for
  // WHATWG-global compatibility but not meaningfully populated by worker_threads.
}
type BroadcastMessageListener = (event: MessageEventLike<any>) => void;
type BroadcastMessageErrorListener = (event: MessageEventLike<Error>) => void;

interface LockRequestOptions {
  mode?: 'exclusive' | 'shared'; // default 'exclusive'
  ifAvailable?: boolean; // default false
  steal?: boolean; // default false
  signal?: AbortSignal;
}

type LockCallback = (lock: Lock | null) => any | Promise<any>;

interface LockManagerSnapshot {
  held: Lock[];
  pending: Array<{ name: string; mode: 'shared' | 'exclusive' }>;
}

/** Structured-clone-supported object categories (informative, not a literal TS type). */
type CloneableExtra =
  | RegExp | BigInt | Map<any, any> | Set<any>
  | ArrayBuffer | SharedArrayBuffer
  | /* every TypedArray ctor */ Int8Array | Uint8Array | Uint8ClampedArray
  | Int16Array | Uint16Array | Int32Array | Uint32Array
  | Float32Array | Float64Array | BigInt64Array | BigUint64Array
  | WebAssembly.Module
  | import('node:crypto').KeyObject | import('node:crypto').X509Certificate
  | import('node:perf_hooks').Histogram
  | import('node:fs/promises').FileHandle
  | MessagePort
  | import('node:net').BlockList | import('node:net').SocketAddress;

/** Transferable (moved, not copied) object categories. */
type Transferable = ArrayBuffer | MessagePort | import('node:fs/promises').FileHandle;
```

## 4. Node semantics & edge cases

- **`process.env` isolation by default.** Each worker gets an independent *copy* of `process.env` at construction time; writes on either side are invisible to the other and invisible to native add-ons in either thread. Passing `env: SHARE_ENV` makes both threads read/write the *same* underlying environment-variable store. **Windows note:** environment-variable name lookups are normally case-insensitive on Windows, but inside a worker (unlike the main thread) they behave case-sensitively — an intentional Node quirk to flag/replicate, not a bug to "fix."
- **`process.exit()` inside a worker only stops that worker's thread**, not the whole process — a materially different meaning than `process.exit()` on the main thread. `process.abort()` is unavailable inside a worker. `process.chdir()`, process UID/GID setters, and `process.title =` assignment are all unavailable/no-ops inside a worker (a worker does not own the process-wide CWD/identity).
- **stdio piping can create backpressure-driven ordering surprises.** Because worker stdio rides on `MessagePort` message-passing under the hood, output from a worker can be delayed/blocked if the *receiving* thread's event loop is itself busy running synchronous code — `console.log` inside a worker is not a guaranteed-immediate side effect from the parent's perspective.
- **`'online'` vs. execution start timing.** Unlike the main thread (whose bootstrap happens before the loop starts), a worker's bootstrap runs *inside* its own event loop, so `eventLoopUtilization()` is meaningfully non-zero essentially from the first tick — but querying it from *outside* the worker (`worker.performance.eventLoopUtilization()`) reads all-zero until `'online'` fires, and again all-zero forever after `'exit'`.
- **Signals are not delivered via `process.on('SIGxxx', ...)` inside a worker** — signal delivery is a whole-process concept; workers do not each get their own signal-handling story.
- **`resourceLimits` bounds only the JS engine's own managed heap** (young/old generation + code range + stack) — it does **not** bound external allocations (native add-ons, `ArrayBuffer`/`Buffer` backing stores are explicitly called out as unaffected). Exceeding a limit terminates the worker; a *global* OS out-of-memory condition can still abort the whole process regardless of any per-worker limit.
- **Structured clone loses non-value-shaped state.** Class instances clone into plain objects (own enumerable properties only) — no prototype, no accessors, no private fields (`#field`) survive; getters are read once at clone time and become that value's *data* property on the far side, not a live getter.
- **Transfer is a move, not a copy.** After `transferList`-ing an `ArrayBuffer`/`MessagePort`/`FileHandle`, it becomes unusable on the sending side — including any other reference to the *same* object elsewhere in `value` that was not itself transferred (the whole object, wherever referenced, is detached).
- **`markAsUntransferable`/`markAsUncloneable` are irreversible** for the lifetime of the object; both raise a `DataCloneError` (a `DOMException`, not a plain `Error`) when violated. `markAsUncloneable` explicitly has no effect on `ArrayBuffer`/`Buffer`-like objects (those follow the transfer/detach rules, not the cloneable/uncloneable ones).
- **`trace_events` is unsupported inside workers.** `async_hooks` IS supported and is the documented mechanism (via `AsyncResource`) for correlating diagnostic/stack-trace info across a worker-pool's tasks.
- **No IPC channel to parent processes.** A worker cannot see any IPC channel its *process* might have inherited from `node:child_process`/`node:cluster` — that channel belongs to the process, not to any one thread within it.
- **`Worker.name` OS thread-name length limits differ by platform**: 32767 characters (Windows), 64 (macOS), 16 (Linux) — silently truncated, not an error.
- **Exit code semantics differ from process exit codes.** `'exit'`'s `exitCode` is whatever value was passed to `process.exit()` *inside* the worker, or exactly `1` if the worker was force-terminated (`.terminate()` or an uncaught error) — there is no analogue of POSIX signal-based exit-code encoding (`128 + signal`) since a worker thread cannot itself receive a POSIX signal.
- **`postMessageToThread` requires a `'workerMessage'` listener already registered on the target thread's `process`** — sending to a thread with none throws `ERR_WORKER_MESSAGING_FAILED` (the analogue of "nobody's listening", detected at send time from the *target's* current listener-registration state, not the sender's).
- **No built-in worker pool.** Node deliberately does not ship a pool/queue abstraction over `Worker` — every "worker pool" library (e.g. `piscina`) is userland; RTS's own ergonomic layer (if any) belongs in the `.ts` shim, not native code.
- **`locks`/`LockManager` is process-wide, not per-thread.** A lock requested in one worker is visible to `.query()` calls and blocks conflicting `.request()` calls from every other thread in the same process, main thread included.

## 5. RTS implementation notes

### 5.1 Native impl mapping

- **Thread spawn.** `new Worker(...)` maps to a real OS thread (`std::thread::Builder::new().stack_size(...).spawn(...)`, NOT `std::process::Command` — a worker is a thread in the SAME process, sharing the address space, unlike `node:cluster`'s full-process fork). `resourceLimits.stackSizeMb` maps directly to `Builder::stack_size` (bytes = MB × 1024 × 1024); the young/old-generation and code-range limits have no 1:1 Rust-std equivalent (no separate "JS heap" segmentation the way V8 has) — enforce them, if at all, as a soft allocation-tracking cap inside RTS's own GC/heap accounting rather than an OS mechanism, and document any gap explicitly (§7).
- **Per-worker environment/module/event-loop instance.** Each worker thread gets its OWN: JIT/codegen module instance (a fresh compile of the same script, since `crates/rts-codegen-new`'s per-process codegen state is documented as "1 program per process" today — see the threading-model spec's blocker table, item 6), its own module registry/import cache, its own microtask queue, its own `gcell` set. This is the direct concrete instantiation of `rts-threading-model.md`'s **"per-thread region"** — a `Worker` IS a region-owning thread in that model, not an approximation of one.
- **`filename`/`eval` script loading.** Reuse whatever module-resolution + compile pipeline the main thread uses (parser → HIR → Cranelift), invoked fresh on the worker's own thread; `eval: true` skips path resolution and compiles `filename` directly as source text (mirrors `runtime.eval`/`runtime_eval_src_jit` already in `rts-runtime`, but must run *on* the worker thread, not dispatched back to the spawning thread).
- **`stdin`/`stdout`/`stderr` piping.** Model as an RTS-internal duplex byte channel (not OS pipes — no new process, so no OS-level stdio inheritance question) between the worker's thread-local stdio surface and the parent: by default the worker's writes to its `process.stdout`/`stderr` are forwarded straight through to the real process-wide stdout/stderr (shared OS file descriptor — no piping needed structurally, just serialize writes from multiple threads); `stdout: true`/`stderr: true` redirect those writes into an in-process byte queue instead, exposed to the parent as `worker.stdout`/`worker.stderr` readable streams. `stdin: true` is the reverse: `worker.stdin` writes feed a queue the worker's `process.stdin` reads reads from instead of the real OS stdin.
- **`env`/`SHARE_ENV`.** Default: snapshot-copy `std::env::vars()` into a per-thread env-map owned by the worker (RTS's own `process.env` implementation must already be a per-thread-overridable map, not a direct pass-through to `std::env::var`, precisely to support this). `SHARE_ENV`: both threads hold an `Arc<RwLock<HashMap<String,String>>>` to the SAME env-map instead of separate copies.
- **`workerData`/`transferList`.** Structured-clone `workerData` at construction time using RTS's shared structured-clone serializer (§5.7) before handing it to the new thread; entries in `transferList` are detached/moved (their underlying `ArrayBuffer`/handle is re-homed rather than copied) using the exact same transfer machinery `postMessage` uses (§5.2/5.4) — `workerData` transfer is not a special case, just an initial `postMessage`-shaped payload delivered before the worker's user script starts running.
- **`MessageChannel`/`MessagePort`.** An MPSC (really SPSC-pair, bidirectional = two SPSC queues) byte-and-value channel between two owning threads; `parentPort` is exactly a `MessagePort` whose "other end" is implicitly the spawning thread rather than a user-visible `MessageChannel` object. Implement `MessagePort` as one shared struct (a lock-free ring buffer or a `crossbeam-channel`/`std::sync::mpsc`-style queue) referenced by opposite-direction `Handle`s on each side.
- **`BroadcastChannel`.** A named, process-wide fan-out registry (`Arc<RwLock<HashMap<String, Vec<Weak<PortLike>>>>>` keyed by channel name) — every live `BroadcastChannel` instance with the same name across every thread in the process is a subscriber; `postMessage` clones the value once per *other* subscriber and pushes it into each one's queue.
- **`locks`/`LockManager`.** A single process-wide `Arc<Mutex<LockRegistry>>` (name → current holder(s)/mode + a FIFO/steal-aware wait queue), independent of the region/worker model — locks coordinate access to *application-level named resources*, not engine memory, so they need no region-awareness at all.
- **`getEnvironmentData`/`setEnvironmentData`.** A process-wide `Arc<RwLock<HashMap<ClonedKey, ClonedValue>>>` (keys/values are structured-clone snapshots, per Node's "any cloneable value usable as a Map key" wording) that every newly-spawned `Worker` inherits a *reference into* at spawn time (so subsequent `setEnvironmentData` calls on an ancestor thread are visible to workers spawned *afterward*, but Node's own docs make no promise of live-updating an *already-running* worker's already-resolved lookups — implement as "copy-on-spawn from the current live table," which satisfies the documented behavior without over-promising live propagation).
- **`getHeapSnapshot`/`getHeapStatistics`/`cpuUsage`/`startCpuProfile`/`startHeapProfile`.** RTS has no V8 to introspect; these need RTS-native equivalents (GC heap dump + shard/handle-table statistics for the snapshot/statistics pair; OS-level per-thread CPU-time accounting — `GetThreadTimes` on Windows, `clock_gettime(CLOCK_THREAD_CPUTIME_ID)`/`getrusage` on POSIX — for `cpuUsage`; profiling requires a sampling/instrumentation hook into the Cranelift-JIT'd code that does not exist yet). Treat as a deferred/best-effort area (§7) — implement the ABI shape now, wire real data collection incrementally.

### 5.2 ABI surface

All rich objects (`Worker`, `MessagePort`, `BroadcastChannel`, `Lock`) are opaque `Handle`s (u64) into `rts-node`'s own handle table; the `Worker`/`MessageChannel`/`MessagePort`/`BroadcastChannel`/`LockManager` classes, `WorkerOptions` normalization, and all `EventEmitter`/`EventTarget` wiring are `.ts`-shim constructs over these externs, per the "no high-level API in Rust" rule. `MessagePort`/`EventTarget`-shaped dispatch (`.onmessage`, `addEventListener`) reuses whatever native `EventTarget` primitive already backs the ambient global (see `docs/node-implementation/events.md`) rather than re-implementing listener storage.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_WORKER_THREADS_IS_MAIN_THREAD` | — | `Bool` | |
| `__RTS_FN_NODE_WORKER_THREADS_THREAD_ID` | — | `I64` | Current thread's id; `0` on main. |
| `__RTS_FN_NODE_WORKER_THREADS_THREAD_NAME` | — | `StrPtr` | Empty string sentinel = `null`. |
| `__RTS_FN_NODE_WORKER_THREADS_SPAWN` | `filename_or_src: StrPtr, is_eval: Bool, options_json: StrPtr, worker_data_clone: Handle (StructuredClone), transfer_handles_ptr: U64, transfer_handles_len: I32` | `Handle` (Worker) | Spawns the OS thread; `options_json` carries the non-clone/non-handle `WorkerOptions` fields (argv/env-mode/execArgv/stdin·stdout·stderr flags/resourceLimits/name/trackUnmanagedFds). |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_POST_MESSAGE` | `worker: Handle, value_clone: Handle (StructuredClone), transfer_handles_ptr: U64, transfer_handles_len: I32` | `Void` | Throws (via the shared error-slot convention) on `DataCloneError`. |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_TERMINATE` | `worker: Handle` | `Handle` (a future/poll-token resolved to the exit code) | |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_REF` / `_UNREF` | `worker: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_RESOURCE_LIMITS` | `worker: Handle` | `StrPtr` (JSON) | Empty object JSON once stopped. |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_CPU_USAGE` | `worker: Handle, prev_json: StrPtr` | `StrPtr` (JSON `{user,system}`) | Rejects (error-slot) `ERR_WORKER_NOT_RUNNING` if stopped. |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_GET_HEAP_STATISTICS` | `worker: Handle` | `StrPtr` (JSON) | |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_GET_HEAP_SNAPSHOT` | `worker: Handle, options_json: StrPtr` | `Handle` (readable-stream handle) | |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_EVENT_LOOP_UTILIZATION` | `worker: Handle, u1_json: StrPtr, u2_json: StrPtr` | `StrPtr` (JSON `{idle,active,utilization}`) | All-zero before `'online'`/after `'exit'`. |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_POLL_EVENT` | `worker: Handle, timeout_ms: I64` | `StrPtr` (JSON event envelope; empty = none) | Drives `'online'/'message'/'messageerror'/'error'/'exit'`. |
| `__RTS_FN_NODE_WORKER_THREADS_CHANNEL_NEW` | — | `U64, U64` (two Handles: port1, port2 — via 2-return-slot convention, or two calls returning a packed value) | |
| `__RTS_FN_NODE_WORKER_THREADS_PORT_POST_MESSAGE` | `port: Handle, value_clone: Handle, transfer_handles_ptr: U64, transfer_handles_len: I32` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_PORT_START` | `port: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_PORT_CLOSE` | `port: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_PORT_REF` / `_UNREF` | `port: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_PORT_HAS_REF` | `port: Handle` | `Bool` | |
| `__RTS_FN_NODE_WORKER_THREADS_PORT_POLL_EVENT` | `port: Handle, timeout_ms: I64` | `StrPtr` (JSON event envelope) | Drives `'message'/'messageerror'/'close'`. |
| `__RTS_FN_NODE_WORKER_THREADS_PARENT_PORT` | — | `Handle` | `0` sentinel = main thread (no parent port). |
| `__RTS_FN_NODE_WORKER_THREADS_WORKER_DATA` | — | `Handle` (StructuredClone) | `0`/hole sentinel = `undefined`. |
| `__RTS_FN_NODE_WORKER_THREADS_RESOURCE_LIMITS` | — | `StrPtr` (JSON) | Module-level (current thread's effective limits). |
| `__RTS_FN_NODE_WORKER_THREADS_BROADCAST_NEW` | `name: StrPtr` | `Handle` | |
| `__RTS_FN_NODE_WORKER_THREADS_BROADCAST_POST_MESSAGE` | `bc: Handle, value_clone: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_BROADCAST_CLOSE` | `bc: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_BROADCAST_REF` / `_UNREF` | `bc: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_BROADCAST_POLL_EVENT` | `bc: Handle, timeout_ms: I64` | `StrPtr` (JSON event envelope) | Drives `'message'/'messageerror'`. |
| `__RTS_FN_NODE_WORKER_THREADS_GET_ENV_DATA` | `key_clone: Handle` | `Handle` (StructuredClone) | Hole sentinel = not set. |
| `__RTS_FN_NODE_WORKER_THREADS_SET_ENV_DATA` | `key_clone: Handle, has_value: Bool, value_clone: Handle` | `Void` | `has_value=false` deletes the entry. |
| `__RTS_FN_NODE_WORKER_THREADS_MARK_UNTRANSFERABLE` | `value_clone: Handle` | `Void` | Marks by identity in a side WeakSet-equivalent (native handle table entry flag when the value is already a handle-backed object; a small identity registry otherwise). |
| `__RTS_FN_NODE_WORKER_THREADS_IS_MARKED_UNTRANSFERABLE` | `value_clone: Handle` | `Bool` | |
| `__RTS_FN_NODE_WORKER_THREADS_MARK_UNCLONEABLE` | `value_clone: Handle` | `Void` | |
| `__RTS_FN_NODE_WORKER_THREADS_IS_MARKED_UNCLONEABLE` | `value_clone: Handle` | `Bool` | |
| `__RTS_FN_NODE_WORKER_THREADS_RECEIVE_MESSAGE_ON_PORT` | `port: Handle` | `StrPtr` (JSON `{has:bool, message:...}` envelope, or reuse the StructuredClone Handle convention) | Synchronous dequeue; no event emitted. |
| `__RTS_FN_NODE_WORKER_THREADS_POST_MESSAGE_TO_THREAD` | `thread_id: I64, value_clone: Handle, transfer_handles_ptr: U64, transfer_handles_len: I32, timeout_ms: I64, has_timeout: Bool` | `Handle` (future/poll-token) | Rejects with the `ERR_WORKER_MESSAGING_*` family. |
| `__RTS_FN_NODE_WORKER_THREADS_LOCKS_REQUEST` | `name: StrPtr, mode_is_shared: Bool, if_available: Bool, steal: Bool, abort_signal: Handle` | `Handle` (Lock, or `0` if `ifAvailable` and unavailable) | The `.ts` shim wraps this + a release call around invoking `callback`. |
| `__RTS_FN_NODE_WORKER_THREADS_LOCKS_RELEASE` | `lock: Handle` | `Void` | Called by the `.ts` shim once `callback` settles — not exposed to user code directly (Node itself has no manual release). |
| `__RTS_FN_NODE_WORKER_THREADS_LOCKS_QUERY` | — | `StrPtr` (JSON `LockManagerSnapshot`) | |
| `__RTS_FN_NODE_WORKER_THREADS_LOCK_NAME` / `_MODE` | `lock: Handle` | `StrPtr` | |

Handle-typed objects: `Worker` (thread handle + join-state + resource-limit snapshot + stdio queue refs), `MessagePort` (queue-pair endpoint ref), `BroadcastChannel` (registry-subscription ref), `Lock` (registry-entry ref). `StructuredClone` here denotes whichever Handle-kind the shared structured-clone serializer (§5.7) produces — worker_threads does not invent its own clone representation. JSON-shaped payloads (`options_json`, event envelopes, `resourceLimits`) follow the same convention already used by other `rts-node` specs (e.g. `node:cluster`'s `settings_json`).

### 5.3 Async model

- **`new Worker(...)`**: sync call, returns the `Worker` handle immediately (thread spawn is not awaited); `'online'` fires asynchronously once the new thread's bootstrap completes, delivered through `__RTS_FN_NODE_WORKER_THREADS_WORKER_POLL_EVENT` drained each event-loop tick — same poll/drain pattern as `node:cluster`'s `'fork'`/`'online'`.
- **`worker.postMessage()`/`port.postMessage()`**: sync, non-blocking enqueue on the sending side; delivery is asynchronous, observed as a `'message'` event on the receiving side's own event-loop tick (never synchronously re-entrant into the sender).
- **`worker.terminate()`**: returns a `Promise<number>` — needs the shared promise-settle machinery; resolves once the corresponding `'exit'` poll-event has been observed and translated.
- **`postMessageToThread()`**: promise-returning; native side needs a completion callback bridged through the promise-settle machinery once the *target* thread's `'workerMessage'` handling completes (success, throw, or timeout) — genuinely cross-thread completion signaling, not just local queue delivery.
- **`locks.request()`**: promise-returning; the lock-acquire step can block (queue behind another holder) — must not block the calling thread's actual OS thread/event loop; queue the wait as an async task woken on release, settling the returned promise once the lock is granted (or immediately-rejects a `null`-callback invocation under `ifAvailable`).
- **`getHeapSnapshot`/`getHeapStatistics`/`cpuUsage`/`startCpuProfile`/`startHeapProfile`**: all promise-returning; each requires a genuine cross-thread request/response (query the *target* worker thread's live state, not the calling thread's) — needs a request-and-await bridge into that thread's own control-message queue (reuse the same poll/queue plumbing `postMessage` uses, tagged as an internal control message rather than a user-visible `'message'` event).
- All of the above (`'online'`/`'message'`/`'error'`/`'exit'` delivery, `terminate()`, `postMessageToThread()`, `locks.request()`) need the **shared** event loop + promise-settle infrastructure so ordering interleaves correctly with the rest of a thread's scheduled async work — see §5.7.

### 5.4 Multithread / worker interaction

This module IS the direct user-facing surface of `docs/specs/rts-threading-model.md`; every subsection there applies concretely here:

- **A `Worker` = a region-owning thread.** Per the threading-model doc's target ("per-thread regions + shared heap with promotion on publication"), spawning a `Worker` should map to spawning a thread that owns its own affine HandleTable shard(s) (property 2 in that doc: "shards are already proto-regions" — deterministic alloc→thread-shard affinity), its own local GC pass, and (once T4 lands) its own local-only collection cycle that never stops the world for sibling workers.
- **`postMessage`/`MessagePort` = the promotion write barrier, made a public API.** Structured-clone `postMessage` is *already* a full deep-copy across the language-level clone boundary — semantically compatible with "eager subgraph promotion" (the threading-model doc's chosen invariant: "a shared→local reference never exists" because publishing transitively closes the subgraph) even before the underlying GC/region machinery exists. In the near term (pre-T4), `postMessage` can be implemented as literal clone-by-value with no engine-level region concept at all — it becomes progressively cheaper (real move/slot-update semantics for already-not-referenced-elsewhere subgraphs) as T4 (promotion on publication) lands, with **no observable JS-level behavior change** required by that migration.
- **`transferList`/`ArrayBuffer` transfer is genuine ownership move**, independent of the region model's GC-object promotion — it maps directly onto Rust ownership transfer of the backing byte buffer between threads (a `Handle`'s underlying allocation is simply re-homed to the receiving thread's shard/region, the sending thread's handle-table entry invalidated). This is the cleanest, already-fully-supported case (property 1 in the threading-model doc: payload = slot index, so moving is a slot update, not a pointer fixup).
- **`SharedArrayBuffer` (used alongside `worker_threads`, not part of this module) is the "raw shared memory" escape hatch** the threading-model doc lists as already primordial — genuinely shared, no promotion/copy semantics at all, backed by real cross-thread-visible memory with `Atomics` for synchronization. `node:worker_threads` itself does not allocate `SharedArrayBuffer`s; it merely allows transferring/referencing them in `postMessage`/`workerData`.
- **`SHARE_ENV`/env-map sharing and the `locks`/`getEnvironmentData`-`setEnvironmentData` process-wide tables are exactly the "shared cell" pattern** the threading-model doc's `Shared<T>` surface targets (per-method auto-synchronization) — implement each as a `shared()`-flavored primitive internally (an `Arc<RwLock<...>>`) even before `rts:thread`'s public `shared()` API exists, so the two converge rather than diverge later.
- **Per-worker isolated engine state.** Each worker needs its own copy of every item in the threading-model doc's blocker table (gcells, data ICs, string-pool/interning, shape registry, microtask queue, codegen/JIT state) — worker_threads is the feature that makes items 1, 3, 5 in that table non-optional prerequisites rather than nice-to-haves; a correct `Worker` implementation cannot ship before **T1 (shared gcells)** and a per-thread microtask queue (blocker 5) are real, or every worker will silently corrupt/share the main thread's globals (the exact `setInterval`-thread bug class already on record in project memory).
- **`BroadcastChannel`/`locks` are process-wide by design** (not per-region) — they are the two places in this module that intentionally do NOT follow the region-affinity model; they are analogous to the threading-model doc's shared heap itself, always-promoted, never locally-owned.

### 5.5 Buffer / TypedArray interop

- `ArrayBuffer`/typed-array payloads inside `postMessage`/`workerData` either (a) clone — deep byte-copy into a fresh `ArrayBuffer` handle in the receiving thread's region, or (b) transfer — the backing allocation is moved (ownership re-homed), never duplicated, and the sender's view is detached (`byteLength` becomes 0, any read throws). RTS should implement both paths atop primordial `ArrayBuffer`/`TypedArray` machinery already owned by the engine — worker_threads does not need its own byte-buffer representation, only the move-vs-copy decision at the `postMessage` boundary.
- `SharedArrayBuffer` referenced in a message is **never cloned nor transferred** — its identity (the same underlying shared memory) is preserved across the message boundary; both threads end up with a `SharedArrayBuffer` `Handle` pointing at the identical backing allocation. This is the one payload type where "crossing the ABI" means "share a pointer/allocation across threads," not "copy bytes" — must be modeled distinctly from the ArrayBuffer clone/transfer paths.
- `Buffer` (Node's `Uint8Array` subclass) clones as a **plain `Uint8Array`** on the far side — the `Buffer`-specific prototype/methods do not survive structured clone (an explicit Node semantics point, §4) — the `.ts` shim must not attempt to re-wrap it as `Buffer` automatically; that would be a parity deviation from real Node.
- `FileHandle` (from `node:fs/promises`) is both cloneable and transferable per the Node docs' lists — RTS should treat it as an opaque native-resource `Handle` re-homed exactly like a `MessagePort` (ownership move to the target thread's handle table) when transferred, or an error/no-op when the underlying resource genuinely cannot be duplicated for the clone (non-transfer) case — flag as an open question if RTS's `fs` FileHandle backing cannot support true duplication (§7).

### 5.6 Doctrine placement

- `node:worker_threads` is confirmed **non-primordial**: no native literal/syntax form, reachable only via `import ... from "node:worker_threads"`. Per the primordial-vs-registry doctrine, the engine (`crates/rts-codegen-new/`) must never hardcode `"worker_threads"` (or `"Worker"`/`"MessagePort"`/etc.) anywhere, including in an allow-list.
- Resolution is data-driven: a `NodespaceSpec { node_module: "worker_threads", ns_prefix: "node_worker_threads", members: MEMBERS }` entry registered in `rts-node`'s own module table, resolved at import time via `node_lookup("worker_threads")`/`ns_prefix_for("node:worker_threads")` — never a `match module_name { "worker_threads" => ... }` arm in codegen.
- Split: every native operation is `__RTS_FN_NODE_WORKER_THREADS_<NAME>` (rich objects as opaque `Handle`s); the `Worker`/`MessageChannel`/`MessagePort`/`BroadcastChannel`/`LockManager`/`Lock` classes, `WorkerOptions` defaulting/validation, and all `EventEmitter`/`EventTarget` event wiring live in a `.ts` shim shipped by `rts-node` that calls these externs — no JS-shaped object graph assembled natively. `MessagePort`/`BroadcastChannel` reuse the ambient `EventTarget` primitive (already spec'd for `node:events`/web globals) rather than a second listener-storage implementation.

### 5.7 Shared-infra dependencies (FLAG)

- **Event loop / microtask pump** — needed on *every* worker thread (not just the main thread) to deliver `'online'/'message'/'messageerror'/'error'/'exit'`/`'close'` without blocking that thread's own JS. Currently a single-instance `event_loop` living in `rts-std`; worker_threads requires this to become **per-thread-instantiable**, which is also exactly threading-model blocker #5 — this module cannot ship correctly until that generalization exists (not merely "hoisted," but re-architected for multiple concurrent instances).
- **Promise subsystem** (`promise.create`/settle) — needed for `worker.terminate()`, `postMessageToThread()`, `locks.request()`, and the profiling/heap-statistics methods, all of which are promise-returning. Currently in `rts-std` (`promise`); each worker thread needs its own promise-settle context wired to its own event loop (same per-thread generalization as above).
- **Shared tokio runtime** (`async_rt`) — useful for the cross-thread request/response plumbing behind `getHeapSnapshot`/`getHeapStatistics`/`cpuUsage`/profiling (querying a *different* thread's state without blocking the caller). Currently in `rts-std` (`runtime/async_rt.rs`).
- **HandleTable** (`gc` shard/slab) — needed for every opaque handle this module produces (`Worker`/`MessagePort`/`BroadcastChannel`/`Lock`/`StructuredClone`). Already reachable via `rts-engine` (not `rts-std`) — listed for completeness; this module's per-thread-region ambitions (§5.4) additionally require the shard/thread-affinity work the threading-model doc tracks as its own item (#3), which is architecture work inside `rts-engine`, not merely an `rts-node` wiring task.
- **Structured-clone serializer** for `ArrayBuffer`/TypedArrays/`Map`/`Set`/`RegExp`/`BigInt`/etc. — the single most load-bearing shared dependency in this entire module: `postMessage`, `workerData`, `BroadcastChannel.postMessage`, `getEnvironmentData`/`setEnvironmentData`, and `receiveMessageOnPort` are ALL built directly on it. Wherever RTS implements this for `structuredClone()`/other `postMessage`-shaped APIs, it must be exposed to `rts-node` without an `rts-std` dependency — this is the same shared-serializer flag raised in `docs/node-implementation/cluster.md` §5.7, and the two modules should consume the identical implementation rather than each growing its own.
- **Per-thread gcells** (threading-model blocker #1) — every `Worker` needs its own independent copy of module-level `let`/`const`/`var` globals; without this, spawning a `Worker` risks silently sharing/corrupting the main thread's globals (the exact bug class already on record — see the `setInterval`-thread memory note). This is a hard prerequisite, not a nice-to-have optimization, for any `Worker` implementation beyond a toy demo.
- Since `rts-node` cannot depend on `rts-std`, the event loop, promise subsystem, and shared tokio runtime must be **hoisted** into a shared low crate (e.g. `rts-engine` or a new shared crate both `rts-std` and `rts-node` depend on) — and, uniquely for this module, that hoisted event loop/promise machinery must ALSO become genuinely multi-instance (one live instance per worker thread) rather than the current process-wide singleton, before `worker_threads` can be implemented without violating either the independence rule or basic per-thread isolation correctness.

### 5.8 Implementation phases

a. **Data-table stub + main-thread-only constants.** Register the `worker_threads` `NodespaceSpec` with `isMainThread` (always `true` until real workers exist), `threadId` (always `0`), `SHARE_ENV` (an opaque symbol/sentinel value), `parentPort`/`workerData`/`resourceLimits` (always `null`/`undefined`/`{}` pre-Worker) — unblocks code that only branches on `isMainThread` without spawning anything.
b. **`Worker` MVP: bare thread spawn, no messaging.** `std::thread::Builder::spawn` re-running the compile pipeline on the new thread with `argv`/`workerData` ignored; wire `'online'`/`'exit'`/`worker.terminate()`/`.ref()`/`.unref()`. Confirms per-thread codegen-state instantiation works at all (the real unknown, since today's codegen state is documented single-instance).
c. **`MessageChannel`/`MessagePort`/`parentPort` with `'json'`-only clone.** Byte-queue-pair implementation; `postMessage`/`'message'`/`'messageerror'`/`.close()`/`'close'`; JSON-only structured-clone subset (objects/arrays/strings/numbers/booleans/null) as an interim, matching the same "start JSON-only, add real structured-clone later" path `node:cluster` takes for `serialization: 'advanced'`.
d. **Real structured-clone serializer** (shared dependency, §5.7) — once available, upgrade `postMessage`/`workerData`/`BroadcastChannel` to full fidelity (RegExp/Map/Set/TypedArrays/BigInt/circular refs) and add `transferList`/`ArrayBuffer` move semantics + `SharedArrayBuffer` identity-preservation.
e. **Per-thread gcells + per-thread event loop/microtask queue** (the two hard threading-model prerequisites, §5.7) — required before `Worker` can safely run arbitrary user scripts with module-level state without corrupting the main thread.
f. **`WorkerOptions` full surface.** `env`/`SHARE_ENV`, `stdin`/`stdout`/`stderr` piping, `execArgv`, `resourceLimits.stackSizeMb` (real), `name`, `trackUnmanagedFds`.
g. **`BroadcastChannel`.** Process-wide named fan-out registry, built once the structured-clone serializer (step d) exists.
h. **`markAsUntransferable`/`isMarkedAsUntransferable`/`markAsUncloneable`/`isMarkedAsUncloneable`.** Small identity-registry additions layered onto the transfer/clone paths from step d.
i. **`getEnvironmentData`/`setEnvironmentData`.** Process-wide table + copy-on-spawn semantics.
j. **`receiveMessageOnPort`/`postMessageToThread`.** The former is a small synchronous-dequeue addition to the MessagePort queue (step c); the latter needs the cross-thread promise-settle bridge (§5.3) plus a `process`-level `'workerMessage'` event (shared work item with a future `node:process` spec).
k. **`locks`/`LockManager`/`Lock`.** Process-wide named-lock registry, independent of everything else — can land any time after step a, but low-priority relative to the core messaging path given it is still Node Stability 1 (Experimental).
l. **Profiling/diagnostics tail: `getHeapSnapshot`, `getHeapStatistics`, `cpuUsage`, `startCpuProfile`, `startHeapProfile`, `worker.performance.eventLoopUtilization`.** Each needs its own RTS-native data source (§5.1) — implement the ABI/ts-shim shape early (stub data) so calling code does not hard-fail, backfill real collection per-item as the underlying GC/profiling infra matures.
m. **`resourceLimits` real enforcement** (beyond `stackSizeMb`) — young/old-generation and code-range caps require RTS's own heap-accounting to expose comparable knobs; likely lands alongside/after the generational GC work (`gc-generational-design.md`), not before.

## 6. Test plan

- `worker_basic_spawn_exit.test.ts` — `isMainThread === true` in main; spawn a `Worker` whose script sets `isMainThread === false` and reports its own `threadId`/`isMainThread` back via `postMessage`; assert `'online'` then `'exit'` fire in order with `exitCode === 0`.
- `worker_message_roundtrip.test.ts` — main posts a nested object/array to the worker via `parentPort`, worker echoes it back; assert deep equality (JSON-subset fidelity at minimum; full structured-clone fidelity once phase d lands).
- `worker_message_types.test.ts` — round-trip `RegExp`, `Map`, `Set`, `BigInt`, a circular object reference, and a `TypedArray`; assert each survives clone with correct `instanceof`/contents (deferred to phase d — mark pending until then).
- `worker_data_passed.test.ts` — `new Worker(file, { workerData: { a: 1, b: [2, 3] } })`; worker reads `workerData` and posts it back; assert deep equality and that mutating `workerData` inside the worker does NOT affect a second worker spawned with the same literal.
- `worker_transfer_arraybuffer.test.ts` — main creates an `ArrayBuffer`, writes a marker byte, transfers it via `postMessage(buf, [buf])`; assert `buf.byteLength === 0` in the sender immediately after the call, and the worker observes the marker byte on its copy.
- `worker_shared_array_buffer.test.ts` — main creates a `SharedArrayBuffer`, sends it (not transferred) to a worker; worker writes via `Atomics.store`, main observes the write via `Atomics.load` after a `postMessage` round-trip signal — proves true shared-memory identity, not clone.
- `worker_env_isolated_default.test.ts` — main sets `process.env.FOO = 'main'`, spawns a worker; worker reads `process.env.FOO` (should be the value at spawn time, or `undefined` if unset) then sets `process.env.FOO = 'worker'`; assert main's `process.env.FOO` is unaffected after the worker exits.
- `worker_share_env.test.ts` — spawn with `{ env: SHARE_ENV, eval: true }` running `process.env.SET_IN_WORKER = 'foo'`; after `'exit'`, assert `process.env.SET_IN_WORKER === 'foo'` on the main thread (mirrors the Node docs' canonical example).
- `worker_stdio_capture.test.ts` — spawn with `{ stdout: true, stderr: true }`; worker `console.log`/`console.error`s known strings; assert `worker.stdout`/`worker.stderr` streams (not the process's real stdout) receive them, and the parent's real stdout does NOT show the worker's output.
- `worker_stdio_default_pipe.test.ts` — spawn without stdio options; worker `console.log`s a known string; assert it appears on the *parent's* real captured stdout (default auto-pipe behavior).
- `worker_terminate.test.ts` — spawn a worker running an infinite loop/long sleep; call `worker.terminate()`; assert the returned promise resolves with `1` and `'exit'` fires with `exitCode === 1`.
- `worker_uncaught_error.test.ts` — worker script throws synchronously at top level; assert `'error'` fires on the `Worker` with the right message/name, followed by `'exit'` with `exitCode === 1`.
- `worker_ref_unref.test.ts` — spawn a worker, call `.unref()`; assert the process/main-thread does not wait on it to exit naturally (a short-lived script test harness assertion, environment-dependent — document exact mechanics per RTS's process-exit model).
- `message_channel_basic.test.ts` — `new MessageChannel()`; post on `port1`, receive on `port2` and vice versa; `.close()` on one side emits `'close'` on both.
- `message_port_start_lazy.test.ts` — construct a channel, attach a listener via `on('message', ...)` (should auto-`start()`); separately verify a port with no listener attached does not lose/drop messages sent to it before a listener is later added (buffers until start).
- `message_port_transfer_between_workers.test.ts` **(multithread)** — main creates a `MessageChannel`, transfers `port2` into a second worker's `postMessage(..., [port2])`, keeps `port1` in the first worker; assert the two workers can message each other directly without round-tripping through main.
- `broadcast_channel_multi_listener.test.ts` **(multithread)** — main and 2 workers each open a `BroadcastChannel('topic')`; one posts, assert the OTHER two receive it (not the sender itself), across both worker threads and the main thread.
- `mark_untransferable.test.ts` — `markAsUntransferable(buf)`; attempt `port.postMessage(buf, [buf])`; assert it throws with `error.name === 'DataCloneError'`; a plain (unmarked) `postMessage(buf)` (clone, not transfer) still succeeds.
- `mark_uncloneable.test.ts` — `markAsUncloneable(obj)`; attempt `port.postMessage(obj)`; assert `DataCloneError`; assert an `ArrayBuffer`/`Buffer` marked uncloneable is UNAFFECTED (per the documented no-op exception) and still clones/transfers normally.
- `get_set_environment_data.test.ts` — main `setEnvironmentData('k', 'v')`, spawns a worker AFTER the call; worker's `getEnvironmentData('k') === 'v'`; a second `setEnvironmentData('k', undefined)` then a fresh worker sees `getEnvironmentData('k') === undefined`.
- `receive_message_on_port_sync.test.ts` — post a message to a port with no listener attached; synchronously call `receiveMessageOnPort(port)` and assert it returns `{ message }` without ever emitting a `'message'` event; a second call with nothing queued returns `undefined`.
- `post_message_to_thread_basic.test.ts` **(multithread)** — worker registers a `process.on('workerMessage', ...)` handler; main calls `postMessageToThread(worker.threadId, value)`; assert the promise resolves and the handler observed `value`.
- `post_message_to_thread_no_listener.test.ts` — target a thread with no `'workerMessage'` listener; assert the promise rejects with `ERR_WORKER_MESSAGING_FAILED`.
- `post_message_to_thread_timeout.test.ts` — target a listener that never returns/blocks; call with a short `timeout`; assert rejection with `ERR_WORKER_MESSAGING_TIMEOUT`.
- `post_message_to_thread_same_thread.test.ts` — call with the caller's own `threadId`; assert immediate rejection `ERR_WORKER_MESSAGING_SAME_THREAD`.
- `resource_limits_stack_size.test.ts` — spawn with a small `resourceLimits.stackSizeMb`; run a script that recurses deeply enough to overflow that stack specifically (not the default); assert the worker terminates (via `'error'`/`'exit'`) rather than corrupting the process.
- `locks_exclusive_serializes.test.ts` — two `locks.request('r', async lock => { ...sleep...; order.push(id); })` calls from different workers on the same lock name; assert they run strictly serialized (never overlapping), verified via a shared timestamp/order array observed back on main.
- `locks_shared_mode_concurrent.test.ts` — several `locks.request('r', { mode: 'shared' }, cb)` calls; assert they ARE allowed to run concurrently (overlapping active windows), unlike the exclusive-mode test above.
- `locks_if_available.test.ts` — hold an exclusive lock, then request the same name with `{ ifAvailable: true }` from elsewhere; assert the callback receives `null` immediately rather than waiting.
- `locks_query_snapshot.test.ts` — hold one lock, queue a second pending request behind it; call `locks.query()`; assert `held`/`pending` reflect both accurately.
- `worker_thread_cpu_usage.test.ts` — spawn a worker doing CPU-bound work for a known duration; `await worker.cpuUsage()`; assert `user`/`system` are plausible non-zero microsecond values (loose bound assertion, not exact).
- `worker_performance_event_loop_utilization.test.ts` — assert `worker.performance.eventLoopUtilization()` is all-zero immediately after construction (before `'online'`), non-zero after the worker has done some work, and all-zero again after `'exit'`.

## 7. Open questions / deferrals

- **Per-thread codegen/JIT state instantiation** is the single biggest unknown: today's `crates/rts-codegen-new` state is documented as "1 program per process" (threading-model blocker #6, marked "fine for multithreaded runtime; JIT stays single-compile" in that doc — but that note is about the *compile step*, not about whether the *generated code's runtime data* — gcells, shape registry, data ICs — can safely be duplicated per-thread). This spec assumes it can be made per-thread; confirm during phase b/e implementation rather than assuming.
- **`resourceLimits`' young/old-generation and code-range caps** have no natural RTS equivalent (no V8-style generational heap yet — see `gc-generational-design.md`, deferred until ~90% cross-runtime). Decide whether to accept-and-ignore these three fields (documenting a parity gap) or approximate them against RTS's own allocator statistics; `stackSizeMb` is the one field with a direct, immediate Rust mapping (`std::thread::Builder::stack_size`).
- **`execArgv`.** Real Node restricts this to a fixed allow-list of V8/Node CLI flags; RTS has no equivalent CLI-flag-per-worker concept today. Decide whether to accept-and-ignore, or define an RTS-specific allow-list (e.g. `--jit-opt-level`, if such flags come to exist) — not urgent, low usage in practice.
- **`moveMessagePortToContext`** depends on a `node:vm` contextify implementation that does not yet have its own spec — block implementation on that spec existing, or de-scope to "not implemented, throws" until then.
- **`isMarkedAsUncloneable`'s exact export name** could not be double-confirmed against the fetched v25.x doc excerpt (only `markAsUncloneable` was directly quoted in the source used for this spec) — verify the precise symbol name/signature against the live Node 25 doc build before finalizing the ABI surface (flagged inline in §2).
- **`FileHandle` duplication semantics on transfer** — confirm whether RTS's own `fs` `FileHandle` backing (once specced) supports genuine cross-thread duplication of the underlying OS file descriptor (POSIX `dup`, Windows `DuplicateHandle`) for the transfer case, or whether this needs its own capability flag/fallback.
- **Worker pool ergonomics.** Node ships no pool abstraction; decide whether RTS's `.ts` layer should optionally ship one (outside strict Node parity) as a value-add, or leave it strictly userland to match Node's minimalism exactly.
- **`locks` API stability.** Node itself marks this Stability 1 (Experimental) as of v24.5.0 — track upstream changes; do not over-invest relative to the core messaging surface (§5.8k is intentionally low-priority).
- **Interaction with `node:cluster`.** A `cluster` worker *process* may itself spawn `worker_threads` internally — purely additive/orthogonal (a thread inside that one process), but worth an explicit combined test once both specs are implemented, to confirm no cross-contamination of thread-local state between the two subsystems.
- **`async_hooks` correlation across worker boundaries.** Node explicitly recommends `AsyncResource` for worker-pool diagnostics; RTS's own `async_hooks` spec (not yet written) will need to define whether/how an async-id space is shared or kept fully separate per worker thread — flag for reconciliation once that spec exists.
