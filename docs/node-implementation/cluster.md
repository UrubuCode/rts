# node:cluster

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:cluster` |
| Node.js version (25.x) | Documented against Node.js 25.x. Module itself dates to Node 0.x; `Primary`/`Master` terminology (`isPrimary`/`setupPrimary`) introduced in v16.0.0, `isMaster`/`setupMaster` kept as deprecated aliases. |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import cluster from "node:cluster"` (default export, primary form used in all Node examples); `const cluster = require("node:cluster")` (CJS); individual properties (`isPrimary`, `fork`, …) are also reachable as named imports since Node synthesizes named ESM exports for built-ins, but RTS should document/ship the default-import form as canonical. |
| Globals exposed | None. `node:cluster` exposes no globals — every member is reached through the module object returned by the import. |

## 1. Purpose

`node:cluster` lets a single Node.js program fork multiple copies of itself ("workers") that can share listening server ports, so a multi-core machine can be used for a single application without an external process supervisor. One "primary" process owns worker lifecycle and (optionally) connection scheduling; each worker is an independent, fully-isolated child process — a fresh OS process with its own address space and its own instance of the runtime — that communicates with the primary purely through an IPC channel. `node:cluster` is explicitly a *process*-isolation primitive: Node's own docs say to prefer `worker_threads` instead when process isolation is not required. In RTS this maps onto native OS process spawning (never raw `fork(2)`, which is unsafe to call from a multi-threaded async runtime) plus a byte-oriented IPC channel and, for socket sharing, OS-level file-descriptor/handle duplication.

## 2. Exported API surface (COMPLETE)

### Classes

#### `class Worker extends EventEmitter`

Represents one forked worker process. Never constructed directly by user code: obtained from `cluster.fork()` in the primary, or as `cluster.worker` inside the worker itself.

**Instance properties**

| Property | Type | Since | Notes |
|---|---|---|---|
| `worker.id` | `number` (integer, ≥ 1) | v0.8.0 | Unique, sequentially assigned id. While the worker is alive this is the key indexing it in `cluster.workers`. |
| `worker.process` | `ChildProcess` | v0.7.0 | In the primary: the `child_process.ChildProcess` created to spawn the worker. Inside the worker: this object IS the global `process`. RTS represents it as an opaque `Handle`. |
| `worker.exitedAfterDisconnect` | `boolean \| undefined` | v6.0.0 | `true` — the worker died as a direct result of `.disconnect()`; `false` — it died some other way; `undefined` — it has not exited yet. Lets the primary decide whether to auto-respawn (e.g. respawn on `false`/crash, not on `true`/graceful shutdown). |

**Instance methods**

##### `worker.send(message, sendHandle?, options?, callback?): boolean`

| name | type | optional | default |
|---|---|---|---|
| `message` | `object` (JSON-serializable, or structured-clonable under `'advanced'` serialization) | no | — |
| `sendHandle` | `net.Socket \| net.Server` | yes | none |
| `options` | `{ keepOpen?: boolean }` | yes | `{ keepOpen: false }` |
| `callback` | `(error: Error \| null) => void` | yes | none |

Return type: `boolean` — `false` indicates backpressure (the OS pipe/channel buffer is full), matching `ChildProcess.send()`/`process.send()` semantics.
Throws: none synchronously; delivery failures surface via `callback(error)` or the `'error'` event.
Variant: **sync call, async delivery** (fire-and-forget with an optional completion callback).
Direction: in the primary, sends to this worker; when called as `process.send()` inside a worker (an alias path, not on `Worker` itself), sends to the primary.

##### `worker.disconnect(): Worker`

No parameters. Returns: `this` `Worker` (reference, since v7.3.0 — earlier versions returned `undefined`).
Throws: none.
Variant: **async, event-driven** — completion is observed via the `'disconnect'` event, not a callback/return value.
Behavior: in the primary, sends an internal message telling the worker to disconnect itself; in the worker, closes all servers owned by that worker, waits for their `'close'` event, then closes the IPC channel. Existing client connections are **not** forcibly closed — the worker exits only once they drain naturally.

##### `worker.isConnected(): boolean`

No parameters. Returns `true` once the IPC channel is up (immediately after creation) until `'disconnect'` fires.
Variant: sync, non-blocking.

##### `worker.isDead(): boolean`

No parameters. Returns `true` once the worker's OS process has terminated (exited or killed by signal).
Variant: sync, non-blocking.

##### `worker.kill(signal?): void` (alias: `worker.destroy(signal?)`)

| name | type | optional | default |
|---|---|---|---|
| `signal` | `string` (POSIX signal name, e.g. `'SIGTERM'`) | yes | `'SIGTERM'` |

Returns: `void`. Throws: none directly (signal-delivery failures propagate through `'error'`).
Variant: sync call, async effect — forcefully terminates without waiting for graceful disconnect (unlike `.disconnect()`). Equivalent to `worker.process.kill(signal)`.
Windows note: POSIX signal names other than a termination request are not meaningful; Node emulates termination only (see §4).

**Events** (inherited from `EventEmitter`; identical payloads to the module-level events below but scoped to this one worker)

| Event | Signature | Since |
|---|---|---|
| `'message'` | `(message: any, handle?: net.Socket \| net.Server) => void` | v0.7.0 |
| `'online'` | `() => void` | v0.7.0 |
| `'listening'` | `(address: ClusterAddress) => void` | v0.7.0 |
| `'disconnect'` | `() => void` | v0.7.7 |
| `'exit'` | `(code: number, signal: string) => void` | v0.11.2 |
| `'error'` | `(error: Error) => void` | v0.7.3 |

### Top-level functions

##### `cluster.fork(env?): Worker`

| name | type | optional | default |
|---|---|---|---|
| `env` | `Record<string, string>` | yes | `{}` — merged into (not replacing) the worker's inherited `process.env` |

Return type: `Worker` — a live handle, usable immediately to attach listeners.
Throws: none directly; spawn failures surface as an `'error'` event on the returned `Worker` and/or a non-zero `'exit'`.
Variant: **sync call with async continuation** — `'fork'` fires synchronously before `fork()` returns; `'online'` fires later, once the child process signals readiness.
Availability: primary process only (a no-op/incorrect call in a worker process — RTS should decide whether to throw or silently no-op; Node itself does not document a hard guard here, so mirror Node's permissive behavior unless testing reveals otherwise).

##### `cluster.disconnect(callback?): void`

| name | type | optional | default |
|---|---|---|---|
| `callback` | `() => void` | yes | none |

Return type: `void`.
Throws: none.
Variant: **async/callback** — calls `.disconnect()` on every entry in `cluster.workers`; `callback` fires exactly once, after every worker has disconnected and its handles have closed (allows the primary process to exit gracefully once all workers are gone).
Availability: primary process only.

##### `cluster.setupPrimary(settings?): void`

| name | type | optional | default |
|---|---|---|---|
| `settings` | `ClusterSettings` (partial; see §3) | yes | merged into the existing `cluster.settings` |

Return type: `void`. Emits `'setup'` on the `cluster` module with the resulting settings snapshot (advisory — multiple calls can happen within one tick, so `'setup'`'s payload is not authoritative; read `cluster.settings` directly for the current value).
Throws: none directly.
Variant: sync.
Semantics: changes apply only to workers `fork()`'d **after** this call — already-running workers are unaffected. `env` cannot be set through `setupPrimary` (only per-call via `fork(env)`). `schedulingPolicy` is frozen the moment the first worker is forked or the first `setupPrimary()` call is made — later attempts to change it are ineffective.
Availability: primary process only.

##### `cluster.setupMaster(settings?): void`

**Deprecated since v16.0.0.** Exact alias of `cluster.setupPrimary` — identical signature, return type, semantics, and the `'setup'` event. RTS should implement it as a direct re-export/alias, optionally surfacing a deprecation warning to match Node's `--pending-deprecation`/runtime deprecation behavior (`DEP0141`).

### Properties & constants

| Name | Type | Since | Notes |
|---|---|---|---|
| `cluster.isPrimary` | `boolean` | v16.0.0 | `true` iff `process.env.NODE_UNIQUE_ID` is unset (i.e., this process was not itself spawned by `cluster.fork()`). |
| `cluster.isMaster` | `boolean` | v0.8.1 (**deprecated v16.0.0**, `DEP0141`) | Alias of `isPrimary`. |
| `cluster.isWorker` | `boolean` | v0.6.0 | `!isPrimary`. |
| `cluster.schedulingPolicy` | `number` | v0.11.2 | One of the two constants below. Default: `SCHED_RR` (`2`) on every platform except Windows, which defaults to `SCHED_NONE` (`1`) because libuv cannot yet distribute IOCP handles efficiently. Overridable via the `NODE_CLUSTER_SCHED_POLICY` env var (`'rr'` / `'none'`), read once before the first fork; the value is frozen thereafter. |
| `cluster.SCHED_NONE` | `number` — constant `1` | v0.11.2 | Primary creates the listening socket and hands the raw handle to each interested worker; workers `accept()` directly. OS-scheduled, can be badly unbalanced in practice. |
| `cluster.SCHED_RR` | `number` — constant `2` | v0.11.2 | Primary itself accepts every connection and round-robins it to a worker, with light heuristics to avoid overloading one worker. |
| `cluster.worker` | `Worker \| undefined` | v0.7.0 | Set only inside a worker process — the worker's own `Worker` object (`self`). `undefined` in the primary. |
| `cluster.workers` | `Record<number, Worker>` | v0.7.0 | Primary-only. Keyed by `worker.id`. A worker is guaranteed removed from this table before its own final `'disconnect'` or `'exit'` event fires (i.e. observers never see a `cluster.workers` entry for an already-fully-gone worker). |
| `cluster.settings` | `ClusterSettings` | v0.7.1 | The effective settings snapshot after the last `setupPrimary()`/implicit defaulting from the first `fork()`. Treated as read-only — mutate only via `setupPrimary()`. |

### Events (emitted on the `cluster` module object, itself an `EventEmitter`)

| Event | Signature | Since | Notes |
|---|---|---|---|
| `'fork'` | `(worker: Worker) => void` | v0.7.0 | Fires synchronously from inside `cluster.fork()`, before the child process is confirmed alive. Useful for logging or setting up a custom "did this worker come up in time" timeout. |
| `'online'` | `(worker: Worker) => void` | v0.7.0 | Fires once the worker process has actually started and signaled readiness. Distinct from `'fork'` (schedule-time vs. running-time). |
| `'listening'` | `(worker: Worker, address: ClusterAddress) => void` | v0.7.0 | Bubbled from the worker's own `server.listen()` `'listening'` event. A worker listening on several addresses fires this once per address. |
| `'disconnect'` | `(worker: Worker) => void` | v0.7.9 | Fires once the worker's IPC channel has closed (graceful exit, forced kill, or manual `.disconnect()`). There may be a delay before the matching `'exit'`; use both together to detect a worker stuck alive after disconnecting. |
| `'exit'` | `(worker: Worker, code: number, signal: string) => void` | v0.7.9 | Fires once the worker's underlying OS process has actually terminated. A common place to call `cluster.fork()` again to auto-respawn (guarded by `worker.exitedAfterDisconnect` to avoid respawn-looping a deliberate shutdown). |
| `'message'` | `(worker: Worker, message: any, handle?: net.Socket \| net.Server) => void` | v2.5.0 (`worker` param added v6.0.0) | Fires when the primary receives any message from any worker via `process.send()`. |
| `'setup'` | `(settings: ClusterSettings) => void` | v0.7.1 | Fires every time `.setupPrimary()`/`.setupMaster()` is called. Advisory only (see `setupPrimary` semantics above). |

---

## 3. Types & option objects

```ts
interface ClusterSettings {
  /** Arguments passed to the Node.js executable. Default: process.execArgv. */
  execArgv?: string[];
  /** File path to the worker file. Default: process.argv[1]. */
  exec?: string;
  /** String arguments passed to the worker. Default: process.argv.slice(2). */
  args?: string[];
  /** Current working directory of the worker process. Default: undefined (inherit from primary). */
  cwd?: string;
  /**
   * Serialization used for inter-process messages.
   * 'advanced' enables structured-clone-style payloads (Buffer/TypedArray/Map/Set/etc);
   * 'json' is plain JSON.stringify/parse. Default: 'json' (documented default value is
   * the boolean-looking `false`, which historically meant "json").
   */
  serialization?: "json" | "advanced";
  /** Whether worker output is piped to the primary (false) or discarded/inherited per `stdio`. Default: false. */
  silent?: boolean;
  /**
   * Configures the stdio of forked processes. Overrides `silent` when present.
   * MUST contain an 'ipc' entry for the cluster/IPC channel to exist.
   */
  stdio?: Array<"pipe" | "ipc" | "ignore" | "inherit" | number>;
  /** POSIX only. Sets the worker process's user id. See setuid(2). */
  uid?: number;
  /** POSIX only. Sets the worker process's group id. See setgid(2). */
  gid?: number;
  /**
   * Sets the worker's inspector port. A function receives the worker index
   * and must return a port number (lets each worker get a distinct port,
   * e.g. `process.debugPort + n`).
   */
  inspectPort?: number | ((workerIndex: number) => number);
  /** Windows only. Hides the forked process's console window. Default: false. */
  windowsHide?: boolean;
}

interface ClusterAddress {
  address: string;
  port: number;
  /** 4 = TCPv4, 6 = TCPv6, -1 = Unix domain socket, 'udp4' | 'udp6' = UDP. */
  addressType: 4 | 6 | -1 | "udp4" | "udp6";
}

/** Opaque in RTS: a Handle wrapping a native socket/server resource. */
type SendHandle = unknown; // net.Socket | net.Server in Node

interface SendOptions {
  /** Keep the passed socket open in the sending process after sending it. Default: false. */
  keepOpen?: boolean;
}

type SendCallback = (error: Error | null) => void;

/** Tri-state, mirrors worker.exitedAfterDisconnect. */
type ExitedAfterDisconnect = true | false | undefined;
```

## 4. Node semantics & edge cases

- **Round-robin vs. OS scheduling.** `SCHED_RR` (default everywhere but Windows): "the primary process listens on a port, accepts new connections and distributes them across the workers in a round-robin fashion, with some built-in smarts to avoid overloading a worker process." `SCHED_NONE`: "the primary process creates the listen socket and sends it to interested workers. The workers then accept incoming connections directly." Node's own docs warn `SCHED_NONE` "should, in theory, give the best performance" but in practice distribution can be badly unbalanced — "over 70% of all connections ended up in just two processes, out of a total of eight" was observed.
- **Windows default.** `SCHED_RR` is default on every OS except Windows; Windows defaults to `SCHED_NONE` "until libuv is able to effectively distribute IOCP handles without incurring a large performance hit."
- **`schedulingPolicy` is a global, one-shot setting** — frozen after the first `fork()` or the first `setupPrimary()` call; later writes are ignored.
- **Workers are spawned via `child_process.fork()`** under the hood, so they inherit that mechanism's IPC channel and its two serialization modes (`'json'` / `'advanced'`), documented under "Advanced serialization for child_process".
- **`server.listen()` behaves differently inside a worker in exactly three cases** (because listen calls are transparently handed to the primary):
  1. `server.listen({ fd: 7 })` — file descriptor `7` **in the primary**, not the worker, is what gets listened on; the resulting handle is then passed to the worker.
  2. `server.listen(handle)` — an explicit handle bypasses the primary entirely; the worker uses it directly.
  3. `server.listen(0)` — normally picks a random port; in a cluster, **every worker gets the same "random" port** on repeated `listen(0)` calls (random only the first time, deterministic afterward). To get distinct ports per worker, derive one from `cluster.worker.id`.
- **No built-in request routing.** Node explicitly does not provide any routing/session-affinity logic — applications must not rely on in-memory session/login state without external coordination (e.g. a shared store), since a client's connections can land on different workers across requests.
- **Worker removal ordering guarantee.** A worker is guaranteed removed from `cluster.workers` **before** its own final `'disconnect'` or `'exit'` event fires — observers never race a stale entry.
- **Graceful vs. forceful shutdown.** `.disconnect()` is graceful: closes owned servers, drains existing connections, then tears down IPC — existing client connections are *not* forcibly closed and disconnect does not wait on them. `.kill(signal)`/`.destroy(signal)` is immediate and forceful (no graceful drain).
- **Auto-exit-on-disconnect protection.** A worker calls `process.exit(0)` automatically if a `'disconnect'` event happens on its own `process` object and `.exitedAfterDisconnect` is not `true` — guards against silent, accidental disconnection.
- **Non-networking use is valid.** "Although a primary use case for the `node:cluster` module is networking, it can also be used for other use cases requiring worker processes" — i.e. cluster is a general primary/worker-process pattern, not exclusively an HTTP load-balancing tool.
- **Deprecations.** `cluster.isMaster` → use `cluster.isPrimary` (deprecated v16.0.0, `DEP0141`); `cluster.setupMaster()` → use `cluster.setupPrimary()` (same deprecation).
- **Windows uid/gid.** `uid`/`gid` map to POSIX `setuid(2)`/`setgid(2)`; Node has no meaningful equivalent on Windows (should be a no-op there, not an error).
- **`windowsHide`** only affects the console window on Windows; irrelevant/no-op elsewhere.
- **Fault isolation.** Because every worker is a separate OS process (not a thread), a worker crash does not corrupt the primary's or siblings' memory — this is cluster's core value proposition over `worker_threads`, which shares the process's fate more tightly.

## 5. RTS implementation notes

### 5.1 Native impl mapping

- **Process spawn.** `cluster.fork()` must NOT call raw `fork(2)` (unsafe from a multi-threaded async runtime — forked child inherits a half-initialized tokio/thread state). Use `std::process::Command` re-invoking `std::env::current_exe()` with the same script/args, plus `NODE_UNIQUE_ID` (or an RTS-internal equivalent env var) set so the child process knows it is a worker. `Command::spawn()` is portable (POSIX `posix_spawn`/`fork+exec`, Windows `CreateProcess`) and does not require unsafe fork semantics.
- **IPC channel.** Node's cluster IPC rides on the `'ipc'` stdio slot of `child_process.fork()` — a duplex pipe. RTS should implement an equivalent dedicated duplex byte channel between primary and worker (a named pipe / Unix domain socket / anonymous pipe pair layered with a length-prefixed frame protocol), carrying JSON (`'json'` mode) or the RTS structured-clone-style encoding (`'advanced'` mode, once available — see §5.7).
- **`env` merging.** Build the child's environment as `current process env ∪ fork(env)` (the passed `env` argument only adds/overrides, never replaces).
- **`uid`/`gid`.** POSIX only — apply via `libc::setuid`/`setgid` (or the `nix` crate) either pre-exec (`CommandExt::pre_exec` on Unix) or immediately after the child starts; no-op on Windows.
- **`windowsHide`.** `std::os::windows::process::CommandExt::creation_flags(CREATE_NO_WINDOW)`.
- **`silent`/`stdio`.** Map to `Stdio::piped()` vs `Stdio::inherit()`/`Stdio::null()` per slot on the `Command`.
- **Round-robin (`SCHED_RR`) accept-and-forward.** The primary itself must own the listening socket and, per accepted connection, hand the *already-accepted* connection's OS socket handle to a chosen worker — this still requires real cross-process socket-handle transfer (same mechanism as `SCHED_NONE`, just decided by the primary instead of the OS).
- **OS-level socket handoff (`SCHED_NONE`/`SCHED_RR`).** Requires passing a live socket file descriptor (POSIX, via `SCM_RIGHTS` ancillary data over a Unix domain socket) or a duplicated socket handle (Windows, via `WSADuplicateSocket`/`WSASocket` targeting the worker's process id) across the process boundary. This is genuine unsafe, OS-specific systems code with no portable `std` API — treat as its own implementation phase (§5.8g), not a side effect of the IPC channel.
- **Worker lifecycle watching.** Background wait on the child's exit status (`std::process::Child::wait`/a non-blocking poll loop) to drive `'exit'`; watch IPC-channel closure to drive `'disconnect'`.
- **`inspectPort`.** No RTS inspector protocol exists yet — accept the option but no-op it (§7).

### 5.2 ABI surface

All rich objects (`Worker`, the IPC channel, the underlying child process) are opaque `Handle`s (u64) into `rts-node`'s own handle table; `cluster.workers`/`cluster.settings`/the `Worker` class/EventEmitter wiring are `.ts`-shim constructs built on top of these externs, per the "no high-level API in Rust" rule.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_CLUSTER_IS_PRIMARY` | — | `Bool` | Reads whether the internal "unique id" env var is set. |
| `__RTS_FN_NODE_CLUSTER_FORK` | `env_json: StrPtr` | `Handle` (Worker) | Spawns the worker process; `env_json` is a JSON object of extra env entries. |
| `__RTS_FN_NODE_CLUSTER_SETUP_PRIMARY` | `settings_json: StrPtr` | `Void` | Merges into the primary's stored `ClusterSettings`. |
| `__RTS_FN_NODE_CLUSTER_DISCONNECT_ALL` | — | `Handle` (a future/poll-token; drained by the `.ts` event loop bridge) | Triggers `.disconnect()` on every live worker. |
| `__RTS_FN_NODE_CLUSTER_WORKER_SEND` | `worker: Handle, message_json: StrPtr, send_handle: Handle, keep_open: Bool` | `Bool` | `send_handle == 0` means "no handle passed." Returns `false` on backpressure. |
| `__RTS_FN_NODE_CLUSTER_WORKER_DISCONNECT` | `worker: Handle` | `Void` | Begins graceful shutdown. |
| `__RTS_FN_NODE_CLUSTER_WORKER_IS_CONNECTED` | `worker: Handle` | `Bool` | |
| `__RTS_FN_NODE_CLUSTER_WORKER_IS_DEAD` | `worker: Handle` | `Bool` | |
| `__RTS_FN_NODE_CLUSTER_WORKER_KILL` | `worker: Handle, signal: StrPtr` | `Void` | |
| `__RTS_FN_NODE_CLUSTER_WORKER_ID` | `worker: Handle` | `I64` | |
| `__RTS_FN_NODE_CLUSTER_WORKER_EXITED_AFTER_DISCONNECT` | `worker: Handle` | `I32` | Tri-state: `-1` = undefined, `0` = false, `1` = true. |
| `__RTS_FN_NODE_CLUSTER_POLL_EVENT` | `timeout_ms: I64` | `StrPtr` (JSON event envelope, empty string = none pending) | Drained once per event-loop tick by the `.ts` shim; feeds `'fork'/'online'/'listening'/'disconnect'/'exit'/'message'/'setup'/'error'` back into the `EventEmitter`-shaped surface. |
| `__RTS_FN_NODE_CLUSTER_SCHED_POLICY_GET` | — | `I32` | `1` = `SCHED_NONE`, `2` = `SCHED_RR`. |
| `__RTS_FN_NODE_CLUSTER_SCHED_POLICY_SET` | `policy: I32` | `Void` | No-op once frozen (first fork/setup already happened) — native side enforces the freeze, not the `.ts` shim. |

Handle-typed objects: `Worker` (wraps: child-process handle, IPC channel handle, id, `exitedAfterDisconnect` tri-state). The `cluster.workers` hash and `ClusterSettings` object are synthesized/maintained entirely in the `.ts` shim (a `Map<number, Worker>` updated on fork/exit events) — never natively stored as a JS object graph, per the doctrine that data-shaping is a `.ts` concern, not an engine/native one.

### 5.3 Async model

- **`fork()`**: sync call, returns the `Worker` handle immediately; `'online'` fires later once the child's readiness handshake completes — delivered through the shared event loop's poll/drain mechanism (`__RTS_FN_NODE_CLUSTER_POLL_EVENT`), not a raw callback parameter.
- **`disconnect(callback)`**: callback-style; native side tracks an internal countdown across every worker's disconnect-then-IPC-close and resolves once it reaches zero, surfaced to `.ts` as a promise the shim then adapts to the Node-shaped `callback` API (or invoked directly if RTS decides to expose callback-native).
- **`worker.send()`**: sync, non-blocking write (returns a backpressure boolean immediately); the optional `callback` fires asynchronously via the poll/event mechanism once the OS write completes (or fails).
- **Inbound `'message'`/`'listening'`/`'disconnect'`/`'exit'`**: all delivered by a background reader (one per worker IPC channel + one process-wait per worker) pushing decoded event envelopes into a queue; the main thread's event-loop tick drains the queue and re-emits the corresponding `.ts` `EventEmitter` event. This requires a background thread or async task per worker — the shared tokio runtime is the natural fit (avoids spinning one raw OS thread per worker for what is fundamentally I/O-bound waiting).
- All of the above needs the **shared** event loop + promise-settle machinery so cluster's events interleave correctly with timers/microtasks/other async work already scheduled on the same JS thread — a private per-module loop would desync ordering guarantees users rely on (e.g. `'exit'` firing before a subsequent microtask).

### 5.4 Multithread / worker interaction

- `node:cluster`'s unit of concurrency is a full **OS process**, not a thread and not an RTS-threading-model "region." Each worker has its own address space, its own GC heap, its own instance of the RTS runtime — there is no shared-heap/region-promotion story to design here at all. The `docs/specs/rts-threading-model.md` primitives (`threadLocal`/`shared`/`channel`/region promotion) are about threads **inside one process**; they simply do not apply across `cluster.fork()` boundaries.
- The only cross-worker communication channel is the OS-level IPC byte pipe — conceptually a `postMessage` that is **always** copy-serialized (JSON or structured-clone-style bytes), **never** `SharedArrayBuffer` (there is no shared memory to speak of between separate OS processes; Node itself does not support this either).
- `cluster.workers`, `cluster.settings`, `cluster.isPrimary`, `cluster.schedulingPolicy` are inherently **primary-process-only** module-level state; a worker process naturally has none of it (it is a distinct process with its own module globals) — so cluster does not need to solve the "thread-local gcell" problem `worker_threads`/the regional-heap redesign has to solve; process isolation gives it for free.
- The one place a real OS-level shared resource crosses the boundary is the `SCHED_RR`/`SCHED_NONE` listening-socket handoff (§5.1) — this is raw OS file-descriptor/handle duplication (`SCM_RIGHTS`/`WSADuplicateSocket`), entirely outside the engine's GC/threading model.
- A cluster worker process may independently use `worker_threads`/`rts:thread` internally — that is a nested, orthogonal concern (threads inside that one worker's process) that `node:cluster` itself does not need to know about.

### 5.5 Buffer / TypedArray interop

- IPC messages default to `serialization: 'json'` — plain `JSON.stringify`/`parse` over the wire. A `Buffer`/`Uint8Array` payload round-trips the same way Node's own JSON mode does (as a `{ type: 'Buffer', data: [...] }`-shaped array of numbers, or base64 — decide to match Node's exact JSON-mode Buffer shape for `JSON.parse`/`JSON.stringify` compatibility with hand-written Node code).
- `serialization: 'advanced'` should reuse whatever structured-clone-equivalent serializer RTS builds for TypedArrays/`ArrayBuffer`/`Map`/`Set` (primordial types) elsewhere (e.g. for `structuredClone()`/`postMessage`), letting a `Uint8Array`/`Buffer` cross cluster IPC as raw bytes instead of an inflated JSON number array — a meaningful throughput win for byte-heavy messages. This is a hard dependency: `'advanced'` mode cannot be implemented before that shared serializer exists (§5.7/§7).
- `sendHandle` (passing a `net.Socket`/`net.Server` via `worker.send()`) is **not** byte-payload interop — it is the OS socket-descriptor handoff described in §5.1/§5.2, a distinct mechanism.
- No cluster IPC path uses shared memory; separate processes mean copy-serialize-deserialize always, by construction — `SharedArrayBuffer` is a non-goal here (matches Node parity: Node does not support it over cluster IPC either).

### 5.6 Doctrine placement

- `node:cluster` is confirmed **non-primordial**: it has no native literal/syntax form — reachable only via `import ... from "node:cluster"` / `require("node:cluster")` — so per the primordial-vs-registry doctrine the engine (`crates/rts-codegen-new/`) must never hardcode `"cluster"` anywhere, not even in an allow-list.
- Resolution is purely data-driven: a `NodespaceSpec { node_module: "cluster", ns_prefix: "node_cluster", members: MEMBERS }` entry registered in rts-node's own module table (the "registry for node" analogue), resolved at import time via `node_lookup("cluster")`/`ns_prefix_for("node:cluster")` — never a `match module_name { "cluster" => ... }` arm anywhere in codegen.
- Split: every native operation is `__RTS_FN_NODE_CLUSTER_<NAME>` (rich objects as opaque `Handle`s); the `Worker` class, the `cluster.workers` hash, `ClusterSettings` merge/defaulting, and all `EventEmitter` wiring/sequencing (`'fork'` → `'online'` → `'listening'` → …) live entirely in a `.ts` shim shipped by `rts-node` that calls these externs and re-exposes the Node-shaped tree — no JS-shaped logic in Rust.

### 5.7 Shared-infra dependencies (FLAG)

- **Event loop / microtask pump** — needed to deliver `'online'/'listening'/'disconnect'/'exit'/'message'` asynchronously without blocking the primary's JS thread. Currently lives in `rts-std` (`event_loop`).
- **Promise subsystem** (`promise.create`/settle) — needed for `cluster.disconnect(callback)`'s join-then-callback semantics and any promise-flavored sugar. Currently in `rts-std` (`promise`).
- **Shared tokio runtime** (`async_rt`) — needed to run background IPC-pipe readers / child-process `wait()` cheaply (one task per worker rather than one raw OS thread per worker). Currently in `rts-std` (`runtime/async_rt.rs`).
- **HandleTable** (`gc` shard/slab) — needed for the opaque `Worker`/child-process/IPC-channel handles. This one is already reachable (`rts-engine`, not `rts-std`) — listed only for completeness since rts-node needs it too.
- **Structured-clone-style serializer** for TypedArrays/`Buffer`/`Map`/`Set` — needed for `serialization: 'advanced'` fidelity (§5.5). Wherever RTS implements this for `structuredClone()`/`postMessage`, `node:cluster` needs the same primitive exposed to `rts-node` without an `rts-std` dependency.
- Since `rts-node` cannot depend on `rts-std`, items 1–3 and the structured-clone serializer must be **hoisted** into a shared low crate (e.g. promoted into `rts-engine` or a new shared crate both `rts-std` and `rts-node` depend on) before `node:cluster`'s async event delivery and `'advanced'` serialization can be implemented without violating the independence rule.
- Socket/fd-passing for `SCHED_RR`/`SCHED_NONE` and the round-robin accept loop should reuse whatever native socket/listener model `node:net` settles on (avoid a second, parallel socket implementation) — this is a dependency on the `node:net` spec's own native-impl decisions, not new shared infra by itself.

### 5.8 Implementation phases

a. **Data-table stub.** Register the `cluster` `NodespaceSpec` with `isPrimary`/`isWorker`/`isMaster`/`schedulingPolicy`/`SCHED_RR`(`2`)/`SCHED_NONE`(`1`) as pure, no-fork constants — unblocks user code that only branches on `cluster.isPrimary` without ever calling `fork()`.
b. **`cluster.fork()` MVP.** Spawn via `std::process::Command` re-invoking `current_exe()` with the original script + unique-id env var; no IPC yet (worker runs standalone). Confirms process-spawn plumbing, the `Worker` handle, `worker.process`, `.kill()`, `.isDead()`.
c. **IPC channel.** Add a duplex byte pipe between primary and each worker; implement `worker.send()`/`'message'` with `'json'` serialization only.
d. **Worker lifecycle events.** `'online'`/`'disconnect'`/`'exit'`/`'error'` wired through background process-wait + IPC-close detection, bridged via the shared event loop.
e. **`setupPrimary()`/`cluster.settings`.** Merge logic + `execArgv`/`args`/`cwd`/`env`/`silent`/`stdio`/`uid`/`gid`/`windowsHide` plumbed into the spawn call; `setupMaster()` as a direct alias.
f. **`'advanced'` serialization.** TypedArray/`Buffer`/`Map`/`Set` passthrough, once the shared structured-clone serializer is available to `rts-node` (§5.7).
g. **Real socket handoff.** `SCM_RIGHTS` (POSIX) / `WSADuplicateSocket` (Windows) for `SCHED_NONE` and the primary-side accept-and-forward loop for `SCHED_RR` — the hard OS-specific part. Gate behind a capability check; fall back to an `SO_REUSEPORT`-based interim (flagged Node-semantics deviation, §7) where fd-passing is not yet implemented.
h. **`cluster.disconnect(callback)`.** Graceful all-workers shutdown join.
i. **`inspectPort`.** Deferred — accept-but-ignore until RTS has an inspector protocol (§7).

## 6. Test plan

- `cluster_basic_fork.test.ts` — `isPrimary`/`isWorker` branch; `fork()` one worker; `worker.id === 1`; `cluster.workers` gains an entry; entry removed on `'exit'`.
- `cluster_multi_fork.test.ts` — fork N (e.g. 4) workers; unique sequential ids `1..N`; all present in `cluster.workers`; `disconnect()` all; verify the map is empty once every `'exit'` has fired.
- `cluster_message_roundtrip.test.ts` — primary `worker.send()`s an object, worker echoes it back via `process.send()`, primary asserts deep-equality of nested arrays/objects.
- `cluster_message_json_types.test.ts` — round-trip string/number/bool/null/nested array/object under `'json'` mode; verify `undefined`/function fields are dropped exactly as Node's own `JSON.stringify` would.
- `cluster_worker_exit_code.test.ts` — worker calls `process.exit(3)`; primary's `'exit'` fires with `code === 3, signal === null`.
- `cluster_worker_signal_kill.test.ts` — primary calls `worker.kill('SIGTERM')`; `'exit'` fires with `signal === 'SIGTERM'`, `code === null` (POSIX only; adjust/skip assertions on Windows per §4).
- `cluster_exited_after_disconnect.test.ts` — `exitedAfterDisconnect` is `undefined` while alive, `true` after a graceful `worker.disconnect()`, `false` after an abrupt `worker.kill()` without a prior disconnect.
- `cluster_disconnect_all.test.ts` — fork 3 workers; `cluster.disconnect(callback)`; callback fires exactly once, only after all 3 have exited; `cluster.workers` is empty at that point.
- `cluster_setup_primary_settings.test.ts` — `setupPrimary({ exec, args, silent: true })` then `fork()`; verify the child observed the right `args` (echoed back over IPC) and that its stdout was **not** inherited (`silent`).
- `cluster_env_per_fork.test.ts` — `fork({ FOO: 'bar' })` twice with different values; each worker echoes back its own `process.env.FOO`; verify no leakage between sibling workers.
- `cluster_scheduling_policy.test.ts` — default `cluster.schedulingPolicy` matches platform (`2`/`SCHED_RR` on POSIX, `1`/`SCHED_NONE` on Windows); `NODE_CLUSTER_SCHED_POLICY` env override honored before the first fork, ignored after.
- `cluster_listening_multiworker.test.ts` **(multi-process)** — N workers each open a TCP listener on the same port; primary observes one `'listening'` event per worker with correct `address`/`port`/`addressType`; fire M connections and assert they land across more than one worker (allow slack under `SCHED_NONE`'s OS-scheduled imbalance).
- `cluster_deprecated_aliases.test.ts` — `cluster.isMaster === cluster.isPrimary`; `cluster.setupMaster()` behaves identically to `setupPrimary()` (and, if RTS emits a deprecation warning, assert it fires once).
- `cluster_worker_self_reference.test.ts` — inside a worker: `cluster.worker.id` matches the id the primary assigned; `cluster.isWorker === true`; `cluster.workers` is empty/absent (primary-only state).
- `cluster_error_event.test.ts` — force a spawn failure (bad `exec` path via `setupPrimary`); assert an `'error'` event surfaces on the resulting `Worker`/`cluster` rather than a thrown exception.
- `cluster_send_handle.test.ts` **(deferred to phase g)** — `worker.send(msg, socketHandle, { keepOpen: true })`; verify the receiving worker gets a working socket it can read/write, and the sender's copy stays open per `keepOpen`. Mark skipped/deferred until real fd-passing lands.

## 7. Open questions / deferrals

- **Real fd/handle passing** (`SCM_RIGHTS`/`WSADuplicateSocket`) is nontrivial OS-specific unsafe code. Until phase g lands, `SCHED_RR`/`SCHED_NONE` may need an interim `SO_REUSEPORT`-based approximation — an explicit Node-semantics deviation (OS-balanced instead of primary-round-robined connections) that must be documented if it is kept as a long-lived fallback on any platform lacking fd-passing support.
- **`serialization: 'advanced'` fidelity** depends on RTS's own structured-clone-equivalent serializer for TypedArrays/`Map`/`Set` being available to `rts-node` (§5.7); until then, only `'json'` mode should be advertised/documented.
- **`inspectPort`** has no RTS-side inspector protocol to hook into yet — accept and ignore the option until an inspector spec exists.
- **Windows `uid`/`gid`** — Node itself no-ops/ignores these on Windows; confirm RTS matches (no error) during implementation.
- **Nested forking** — Node structurally allows a worker to itself call `cluster.fork()` or spawn `worker_threads` (each worker is a fresh, fully capable Node process). RTS should explicitly decide whether nested `cluster.fork()` is supported at parity or deferred, rather than leaving it accidentally broken.
- **Windows round-robin note.** Node's docs say Windows will switch its default to `SCHED_RR` "once libuv is able to effectively distribute IOCP handles without incurring a large performance hit" — implying libuv does not do so today. RTS's own IOCP-vs-round-robin tradeoff on Windows should be re-benchmarked independently rather than assumed identical to libuv's historical result.
- **Relationship to a future `node:child_process` spec.** `cluster.fork()` is explicitly "spawned using `child_process.fork()`" in Node. When RTS writes a `node:child_process` spec, `node:cluster`'s process-spawn plumbing (§5.1/§5.8b) should be **unified** with it rather than duplicated — flag for reconciliation once that spec exists.
- **Relationship to `node:net`.** Real fd-passing (phase g) and the listening-socket handoff assume whatever Handle/socket model `node:net` settles on — must be reconciled to avoid two competing socket-handle representations.
