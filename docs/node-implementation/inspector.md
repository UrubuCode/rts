# node:inspector

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:inspector` (+ `node:inspector/promises`) |
| Node.js version | 25.x |
| Stability | 2 - Stable (`node:inspector`, callback `Session.post()`); 1 - Experimental (`node:inspector/promises`, promise `Session.post()`); the `Network.*`/`NetworkResources.*`/`DOMStorage.*` integration methods are separately marked 1.1 - Active Development and are additionally gated behind CLI flags (see §4) |
| Tier | P2 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import * as inspector from "node:inspector"`; `import * as inspector from "node:inspector/promises"`; `const inspector = require("node:inspector")`; `const inspector = require("node:inspector/promises")` |
| Globals exposed | none — every member is reached through the module namespace object, no identifiers are added to the global scope |

## 1. Purpose

In real Node, `node:inspector` is a thin binding over the JS engine's built-in
debugger/profiler backend — the same backend that also implements the Chrome
DevTools Protocol (CDP) and backs `--inspect`. It lets a running process
open/close a debugging endpoint (`open`/`close`/`url`), block until a debugger
attaches (`waitForDebugger`), and create in-process `Session` objects that post
protocol-shaped commands (`session.post(method, params)`, e.g.
`"Runtime.evaluate"`, `"Profiler.start"`, `"HeapProfiler.takeHeapSnapshot"`)
and receive protocol events (`'inspectorNotification'` and per-method events).
`node:inspector/promises` is the identical `Session` API except `post()`
returns a `Promise` instead of taking a callback. The module also exposes a
handful of programmatic broadcast helpers (`Network.*`, `NetworkResources.put`,
`DOMStorage.*`) that let application code manually feed protocol-shaped events
to any connected DevTools-style frontend, and a minimal `console` object for
sending messages straight to the remote inspector console.

**RTS has no such engine to bind to, and does not acquire one.** Per the
project's binding no-V8 rule (`docs/node-implementation/architecture.md` §11):
RTS never embeds, emulates, or links the debugger/profiler engine Node's
binding wraps. `node:inspector`'s API *shape* is reproduced for source
compatibility; the backing implementation is RTS's own debug/introspection
surface, built from real `rts-engine`/`rts-node` primitives where an honest
equivalent exists, and an explicit "not implemented" elsewhere — never a
protocol emulation of an engine RTS does not have. Two ways to close that gap
exist, and §5 recommends starting with the narrower one:

- **(a) Full protocol-compatible endpoint (north star, not scheduled).** RTS
  grows a genuine wire-level endpoint — HTTP discovery responder + WebSocket +
  enough command/domain coverage, driven by real RTS-engine introspection
  (Cranelift stack maps, the mark+sweep collector, a future sampling
  profiler) — so an actual DevTools-class frontend can attach over the
  network and drive it for real. Large, multi-phase engine-coupling effort;
  tracked as a long-term direction, not committed to any near-term phase.
- **(b) Explicit, honest deferral (recommended initial scope).** RTS ships
  the full `node:inspector` API *shape* — `open`/`close`/`url`/
  `waitForDebugger`, `Session`, events, the broadcast helpers — backed
  entirely **in-process**: `session.post(method, params)` pattern-matches a
  small, honestly-scoped allowlist of `method` strings onto real RTS-engine
  primitives that already have a natural equivalent (see §5.1), and returns a
  genuine protocol-shaped "not implemented" error for every method outside
  that allowlist — never a crash, a silent no-op, or a hang. This already
  covers Node's own two canonical usage patterns (in-process CPU/heap
  profiling via `Session`, no attached frontend involved) without requiring a
  working network-facing protocol server at all.

§5 is explicit about which methods/domains are backed for real, which return
an honest "not implemented", and which are deferred outright (§7).

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Session extends EventEmitter`

Same class name and shape from both entry points; the two import specifiers
differ **only** in what `post()` returns (see the per-variant methods below).
Whether Node 25's actual `lib/inspector.js`/`lib/inspector/promises.js` share
one internal class with a flag or are truly two distinct class objects is
unconfirmed from the public docs page — **(verify)** against Node source
before assuming `inspector.Session !== require('node:inspector/promises').Session`
matters for any user-observable `instanceof` check; RTS's `.ts` shim can
implement it either way without changing behavior.

- Extends: `EventEmitter` (see `node:events`)
- No documented static members; `new Session()` is the only construction path
  (there is no `Session.create()`-style factory).

**Constructor**

##### `new Session()`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Added in: v8.0.0
- Throws: none
- Creates a new, unconnected `Session` instance. `TypedArray`/off-thread state
  is not allocated until `connect()`/`connectToMainThread()`.

**Instance methods (common to both variants)**

##### `session.connect(): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Added in: v8.0.0
- Returns: `void`
- Throws: `ERR_INSPECTOR_ALREADY_CONNECTED` if the session is already connected
  **(verify exact code/wording against Node 25 source — not returned by the
  fetched docs page; this is Node's documented error-code naming convention
  for the module, `lib/internal/errors.js`)**
- Variant: **sync**

Connects the session to the inspector back-end of the thread it was created
on.

##### `session.connectToMainThread(): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Added in: v12.11.0
- Returns: `void`
- Throws: if called outside a Worker thread — Node's documented error code for
  this condition is `ERR_INSPECTOR_NOT_WORKER` **(verify exact wording)**
- Variant: **sync**

Connects the session to the **main thread's** inspector back-end. Only valid
when called from a `node:worker_threads` Worker.

##### `session.disconnect(): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Added in: v8.0.0
- Returns: `void`
- Throws: none (idempotent — safe to call on an already-disconnected session
  per Node's docs, though calling before any `connect()` is a degenerate case)
- Variant: **sync**

Immediately closes the session. All pending `post()` callbacks/promises are
resolved with an error (documented error code `ERR_INSPECTOR_CLOSED`
**(verify exact wording)**). A session reconnected after `disconnect()` loses
all prior inspector-side state (e.g. previously-set breakpoints).

**`session.post()` — variant-specific signature**

##### `node:inspector` — `session.post(method: string, params?: object, callback?: (err: Error | null, result: object) => void): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `method` | `string` | no | — |
| `params` | `object` | yes | `undefined` |
| `callback` | `(err: Error \| null, result: object) => void` | yes | `undefined` |

- Added in: v8.0.0
- History: v18.0.0 — passing an invalid `callback` throws `ERR_INVALID_ARG_TYPE`
  (previously `ERR_INVALID_CALLBACK`)
- Returns: `void`
- Throws: `ERR_INVALID_ARG_TYPE` synchronously for a malformed `callback`
  argument; all *protocol-level* errors (unknown method, malformed params,
  session not connected) are delivered asynchronously as the callback's first
  argument, never thrown
- Variant: **callback**

##### `node:inspector/promises` — `session.post(method: string, params?: object): Promise<object>`

| Param | Type | Optional | Default |
|---|---|---|---|
| `method` | `string` | no | — |
| `params` | `object` | yes | `undefined` |

- Added in: v19.0.0
- Returns: `Promise<object>` — resolves with the method-specific result object
- Throws: never synchronously; protocol-level errors reject the returned
  promise
- Variant: **promise**

Posts `method` (a fully-qualified CDP method name, e.g. `"Runtime.evaluate"`,
`"Debugger.enable"`) with `params` to the inspector back-end.

```javascript
// callback (node:inspector)
const session = new inspector.Session();
session.connect();
session.post('Runtime.evaluate', { expression: '2 + 2' },
             (error, { result }) => console.log(result));
// { type: 'number', value: 4, description: '4' }

// promise (node:inspector/promises)
const { Session } = require('node:inspector/promises');
const session = new Session();
session.connect();
const result = await session.post('Runtime.evaluate', { expression: '2 + 2' });
```

**Events (both variants, emitted on a `Session` instance)**

| Event | Listener signature | Added in |
|---|---|---|
| `'inspectorNotification'` | `(message: { method: string, params: object }) => void` | v8.0.0 |
| `'<inspector-protocol-method>'` (e.g. `'Debugger.paused'`, `'HeapProfiler.addHeapSnapshotChunk'`) | `(message: { method: string, params: object }) => void` | v8.0.0 |

`'inspectorNotification'` fires for **every** notification the engine's
debugger/profiler backend sends, regardless of method. Additionally, a
same-named event fires for the
specific method (e.g. `session.on('Debugger.paused', ...)`), receiving the
identical message object.

```javascript
session.on('Debugger.paused', ({ params }) => {
  console.log(params.hitBreakpoints);
});
session.on('HeapProfiler.addHeapSnapshotChunk', (m) => {
  fs.writeSync(fd, m.params.chunk);
});
```

### 2.2 Top-level functions

##### `inspector.open(port?: number, host?: string, wait?: boolean): Disposable`

| Param | Type | Optional | Default |
|---|---|---|---|
| `port` | `number` | yes | whatever was specified on the CLI (`--inspect-port`) |
| `host` | `string` | yes | whatever was specified on the CLI |
| `wait` | `boolean` | yes | `false` |

- History: v20.6.0 — return value changed to a `Disposable`
- Returns: `Disposable` — an object implementing the
  [explicit resource management](https://tc39.es/proposal-explicit-resource-management/#sec-disposable-interface)
  protocol (`[Symbol.dispose]()`), which calls `inspector.close()` when
  disposed (e.g. via a `using` declaration). No usage example is given on the
  public docs page beyond the interface link — **(verify)** the exact
  `Symbol.dispose` wiring against Node source if pixel-exact behavior matters.
- Throws: if an inspector is already active — Node's documented error code is
  `ERR_INSPECTOR_ALREADY_ACTIVATED` **(verify exact wording)**
- Variant: **sync**

Activates the inspector on `host:port`. Equivalent to
`node --inspect=[[host:]port]` but callable at runtime. If `wait` is `true`,
blocks the event loop until a client has connected.

##### `inspector.close(): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Added in: v9.0.0
- History: v18.10.0 — now exposed inside Worker threads too
- Returns: `void`
- Throws: none (safe to call with no active inspector — a no-op)
- Variant: **sync**

Attempts to close all remaining inspector connections, **blocking the event
loop** until every one is closed, then deactivates the inspector.

##### `inspector.url(): string | undefined`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Returns: `string | undefined` — the active inspector's WebSocket URL (e.g.
  `ws://127.0.0.1:9229/166e272e-7a30-4d09-97ce-f1c012b43c34`), or `undefined`
  if there is no active inspector
- Throws: none
- Variant: **sync**

##### `inspector.waitForDebugger(): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Added in: v12.7.0
- Returns: `void`
- Throws: if there is no active inspector — Node's documented error code is
  `ERR_INSPECTOR_NOT_AVAILABLE` **(verify exact wording)**
- Variant: **sync** (blocks synchronously until the `Runtime.runIfWaitingForDebugger`
  command is received)

Blocks execution until a client has sent the
`Runtime.runIfWaitingForDebugger` command.

##### `inspector.Network.dataReceived(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v24.2.0, v22.17.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync** (fire-and-forget broadcast)

Broadcasts the `Network.dataReceived` CDP event; buffers data until
`Network.streamResourceContent` is invoked by the frontend; enables the
`Network.getResponseBody` command.

##### `inspector.Network.dataSent(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v24.3.0, v22.18.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

Broadcasts `Network.dataSent`; enables `Network.getRequestPostData`.

##### `inspector.Network.requestWillBeSent(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v22.6.0, v20.18.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.Network.responseReceived(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v22.6.0, v20.18.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.Network.loadingFinished(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v22.6.0, v20.18.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.Network.loadingFailed(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v22.7.0, v20.18.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.Network.webSocketCreated(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v24.7.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.Network.webSocketHandshakeResponseReceived(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v24.7.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.Network.webSocketClosed(params?: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `object` | yes | `undefined` |

- Added in: v24.7.0
- Requires: `--experimental-network-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.NetworkResources.put(url: string, content: string): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `url` | `string` | no | — |
| `content` | `string` | no | — |

- Added in: v24.5.0, v22.19.0
- Requires: `--experimental-inspector-network-resource`
- Stability: 1.1 - Active Development
- Returns: `void`
- Variant: **sync**

Provides response content for `loadNetworkResource` CDP requests (e.g. so a
connected DevTools frontend can resolve a source map that isn't otherwise
network-fetchable by it).

```javascript
const mapUrl = 'http://localhost:3000/dist/app.js.map';
const distAppJsMap = await fetch(mapUrl).then((res) => res.text());
inspector.NetworkResources.put(mapUrl, distAppJsMap);
```

##### `inspector.DOMStorage.domStorageItemAdded(params: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `{ storageId: DOMStorageId, key: string, newValue: string }` | no | — |

- Added in: v25.5.0
- Requires: `--experimental-storage-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.DOMStorage.domStorageItemRemoved(params: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `{ storageId: DOMStorageId, key: string }` | no | — |

- Added in: v25.5.0
- Requires: `--experimental-storage-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.DOMStorage.domStorageItemUpdated(params: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `{ storageId: DOMStorageId, key: string, oldValue: string, newValue: string }` | no | — |

- Added in: v25.5.0
- Requires: `--experimental-storage-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.DOMStorage.domStorageItemsCleared(params: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `{ storageId: DOMStorageId }` | no | — |

- Added in: v25.5.0
- Requires: `--experimental-storage-inspection`
- Returns: `void`
- Variant: **sync**

##### `inspector.DOMStorage.registerStorage(params: object): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `params` | `{ isLocalStorage: boolean, storageMap: object }` | no | — |

- Added in: v25.5.0
- Requires: `--experimental-storage-inspection`
- Returns: `void`
- Variant: **sync**

### 2.3 Properties & constants

| Name | Type | Description |
|---|---|---|
| `inspector.console` | `object` | An object that sends messages straight to the remote inspector console. Documented explicitly as **not** having API parity with the real `node:console`/global `console` — only some subset of methods exist. The docs example uses `.log()`; the complete method list is undocumented on the public page — **(verify)** against `lib/internal/inspector_console.js` before committing to any method beyond `log` as required surface. |

There are no other documented module-level constants (no version numbers,
no protocol-revision string) exposed directly on the `inspector`/
`inspector/promises` namespace object.

### 2.4 Events

Only `Session` instances emit events (`'inspectorNotification'` and the
dynamic per-protocol-method events, both documented above under `Session`).
The `inspector`/`inspector/promises` module namespace objects themselves emit
nothing.

## 3. Types & option objects

```ts
/** Listener/result shape shared by both post() variants and by every
 *  Session event. */
interface InspectorMessage<Params = Record<string, unknown>> {
  method: string;
  params: Params;
}

/** node:inspector (callback variant). */
type SessionPostCallback = (err: Error | null, result: Record<string, unknown>) => void;

/** node:inspector/promises (promise variant). */
type SessionPostPromise = Promise<Record<string, unknown>>;

/** The DOMStorage.* helpers' shared storage identifier shape. */
interface DOMStorageId {
  securityOrigin: string;
  storageKey: string;
  isLocalStorage: boolean;
}

interface DOMStorageItemAddedParams {
  storageId: DOMStorageId;
  key: string;
  newValue: string;
}

interface DOMStorageItemRemovedParams {
  storageId: DOMStorageId;
  key: string;
}

interface DOMStorageItemUpdatedParams {
  storageId: DOMStorageId;
  key: string;
  oldValue: string;
  newValue: string;
}

interface DOMStorageItemsClearedParams {
  storageId: DOMStorageId;
}

interface DOMStorageRegisterStorageParams {
  isLocalStorage: boolean;
  storageMap: Record<string, unknown>;
}

/** Network.* params are intentionally untyped `object` in Node's own docs —
 *  they mirror the Chrome DevTools Protocol "Network" domain's event payload
 *  shapes 1:1 and are not independently re-specified on the Node docs page.
 *  (verify exact per-event field shape against the CDP spec if strict
 *  validation is desired; RTS's native side should treat these as opaque
 *  JSON passthrough, not a validated schema — see §5.1.) */
type NetworkEventParams = Record<string, unknown>;

/** Disposable returned by inspector.open() (TC39 explicit resource
 *  management proposal shape). */
interface Disposable {
  [Symbol.dispose](): void;
}

/** class Session extends EventEmitter — see §2 for full member docs; shown
 *  here as a single TS shape covering both import-specifier variants (the
 *  `post` overload differs by which module the type is pulled from). */
declare class Session extends EventEmitter {
  constructor();
  connect(): void;
  connectToMainThread(): void;
  disconnect(): void;
  // node:inspector
  post(method: string, params?: object, callback?: SessionPostCallback): void;
  post(method: string, callback?: SessionPostCallback): void;
  // node:inspector/promises
  post(method: string, params?: object): SessionPostPromise;
}
```

## 4. Node semantics & edge cases

- **Everything here is, in real Node, a binding over the engine's own
  built-in debugger/profiler.** The same backend implements `--inspect` and
  the Chrome DevTools Protocol; there is no separate/alternate implementation
  inside Node itself to fall back to. RTS's Cranelift-based runtime has no
  analogous built-in debugger/profiler to bind to — see §5 for how RTS maps
  this without acquiring or emulating one.
- **Same-thread breakpoints are explicitly discouraged.** Node's own docs warn
  that setting breakpoints via a same-thread `Session` (`session.connect()`)
  is not recommended, because a paused/blocked V8 isolate cannot service its
  own debugger commands; `session.connectToMainThread()` from a Worker, or a
  separate out-of-process debugger client over the raw WebSocket protocol, is
  the supported pattern for real breakpoint debugging.
- **`HeapProfiler.takeHeapSnapshot` / `HeapProfiler.stopTrackingHeapObjects`
  `reportProgress` warning.** Node's docs explicitly warn against setting
  `reportProgress: true` on these two commands.
- **Console objects are not released automatically.** Objects that reach the
  remote inspector console via the console API are held alive until
  `Runtime.discardConsoleEntries` is sent explicitly — a potential memory-hold
  gotcha for long sessions. **(verify exact method-name casing** —
  `Runtime.discardConsoleEntries` vs `Runtime.DiscardConsoleEntries` —
  **against the CDP spec/Node source**; not load-bearing for RTS today since
  phase (b), §5.1, does not implement this command at all.)
- **Protocol version.** The docs point to the "latest" Chrome DevTools
  Protocol spec at https://chromedevtools.github.io/devtools-protocol/v8/
  (versioned upstream against the engine Node's binding wraps) rather than
  pinning an exact protocol revision number — there is no single fixed
  "inspector protocol version" constant exposed by the module.
- **CLI flags.** `inspector.open()` is the programmatic equivalent of
  `--inspect[=[host:]port]`. The broader CLI flag family (from general Node
  knowledge — **(verify)** the exact current set/spelling against Node 25's
  CLI docs, not returned by the fetched page): `--inspect[=[host:]port]`,
  `--inspect-brk[=[host:]port]` (break on first line), `--inspect-port=[host:]port`
  / `--debug-port` alias, `--inspect-wait[=[host:]port]` (wait for a client
  before running any code), `--inspect-publish-uid=stderr,http` (where the
  listening address is announced).
- **Security.** Binding the inspector to a non-loopback host
  (`inspector.open(port, '0.0.0.0')` or the CLI equivalent) exposes a
  WebSocket endpoint with **no authentication** that grants arbitrary code
  execution in the process (`Runtime.evaluate`) to anyone who can reach it —
  Node's docs and security advisories are explicit that this must never be
  done on an untrusted network. RTS's implementation must preserve
  loopback-only defaults and must not silently widen the bind address.
- **No platform differences documented** beyond ordinary TCP/WebSocket
  bind-address semantics (Windows vs POSIX loopback resolution, which is
  already handled generically by whatever TCP listener backs `open()`).
- **Deprecations.** No deprecated members are documented on the current page;
  `inspector.open()`'s return type changed (string/undefined pre-v20.6.0 in
  some Node internals discussions → `Disposable` since v20.6.0) but this is an
  addition, not a removal.
- **Worker-thread exposure of `close()`** — only became available inside
  Worker threads as of v18.10.0; before that it was main-thread only.

## 5. RTS implementation notes

### 5.1 Native impl mapping

Per the project's binding no-V8 rule (`docs/node-implementation/architecture.md`
§11): RTS never embeds, emulates, or links the engine Node's `node:inspector`
binding wraps. There is no equivalent built-in debugger/profiler inside
`rts-codegen-new`/Cranelift to bind to, so this module cannot be a
transliteration of Node's binding layer. RTS instead reproduces the API shape
and backs each member with either a real RTS-engine primitive or an explicit,
non-crashing deferral. Recommended initial scope is **(b)** from §1; **(a)**
is listed separately below as the unscheduled long-term direction.

**(b) — recommended initial scope, all in-process:**

- **`open()`/`close()`/`url()`/`waitForDebugger()` → a real RTS debug-server
  lifecycle, not a stub.** `rts-node` owns a process-wide singleton
  (`OnceLock<Mutex<InspectorState>>`, not shared with `rts-std`) that, on
  `open()`, binds a real `std::net::TcpListener` (loopback by default,
  matching Node's own default and security posture) and serves a minimal HTTP
  discovery responder (`/json`, `/json/list`, `/json/version` — the same
  endpoints any DevTools-style frontend probes first) so `url()` returns a
  genuinely reachable address and `waitForDebugger()` can block on a real
  accepted TCP connection. `url()`'s string keeps the `ws://host:port/uuid`
  shape real Node returns (source-compat for code that only parses/logs the
  string), but this is RTS's **own** endpoint and **own** minimal discovery
  response — it does not claim wire-level protocol compatibility (no
  WebSocket upgrade, no JSON-RPC command loop) in this phase; a client that
  actually tries to open a WebSocket to that URL will fail to attach until
  (a) lands. A connecting TCP client today learns only that an RTS process is
  listening, nothing more. Documented explicitly as a deferral in §7, not a
  silent gap.
- **`Session`/`session.post()` never need the network endpoint at all.** In
  real Node, `Session` talks to the local engine backend in-process — an
  attached network client is a separate, optional consumer of the same
  backend, not a requirement for `Session` to function (Node's own two
  canonical examples, CPU and heap profiling via `Session`, never attach an
  external frontend). RTS implements `session.post(method, params, cb)` as a
  **pure in-process dispatch**: a small, honestly-scoped allowlist of
  `method` strings mapped onto real primitives:
  - `Runtime.enable`/`Runtime.disable`/`Debugger.enable`/`Debugger.disable` —
    state no-ops with a real ack, so Node code that unconditionally sends
    these before using a domain doesn't hard-fail.
  - `Runtime.evaluate` — bridged to RTS's own compile-and-run seam, the same
    primitive `node:vm`'s `compileFunction` wraps
    (`docs/node-implementation/vm.md` §5.1) — reused, not reimplemented.
  - Heap-stats-shaped commands (any command whose result is heap totals) —
    reuse the exact `rts-engine` `HandleTable`/collector primitive
    `node:v8`'s `getHeapStatistics` already exposes
    (`docs/node-implementation/v8.md` §5.1/§5.2,
    `__RTS_FN_NODE_V8_GET_HEAP_STATISTICS_JSON`) — no new
    heap-introspection primitive is built for this module.
  - `Profiler.start`/`Profiler.stop` (CPU profiling) — **not backed**; RTS
    has no statistical/sampling profiler today (the existing `trace/`
    namespace is a manual push/pop frame stack, not a sampler — the
    identical gap `node:v8`'s `startCpuProfile` documents in
    `docs/node-implementation/v8.md` §5.7/§7). Returns the honest "not
    implemented" error, not a fabricated empty profile.
  - `Schema.getDomains` — returns the (small) set of domains actually backed
    above, not Node's full domain list.
  - Every method outside this allowlist returns a real protocol-shaped error
    (mirroring `ERR_INSPECTOR_COMMAND` semantics) — never a crash, a silent
    no-op, or a hang.
- **Notification fan-out (`Network.*`/`NetworkResources.put`/`DOMStorage.*`).**
  These are documented as purely *programmatic* broadcast helpers — no
  automatic instrumentation is implied. Under (b), with no wire-attachable
  frontend to receive them, they are implemented as real natives that
  validate/serialize `params` and enqueue them on the same process-wide
  fan-out list a future frontend could drain — a safe, honest no-op in
  observable effect, consistent with how Node itself behaves when the
  corresponding `--experimental-*` flag is off.
- **`inspector.console`.** Maps its (small, undocumented-exact) method set to
  the same string-formatting logic `node:console`/global `console` already
  uses inside `rts-node`, broadcast as a notification the same way any other
  in-process event is (no new formatter).

**(a) — north star, unscheduled:** grow the phase-(b) listener into a real
wire-level protocol server (WebSocket upgrade + JSON-RPC command loop) with
enough domain coverage that an actual DevTools-class frontend can attach and
drive it — `Runtime`/`Debugger` backed by live Cranelift frame/stack-map
introspection (real breakpoints would need code-patching integration with the
JIT), `Profiler`/`HeapProfiler` backed by a real sampling profiler once one
exists. This is substantial, multi-phase engine work; it is not part of the
phased plan in §5.8 and must not be silently implied by any "minimal subset"
language elsewhere in this doc.

### 5.2 ABI surface

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_INSPECTOR_OPEN` | `port: I32, host: StrPtr, wait: Bool` | `Void` | binds the real loopback `TcpListener` + discovery responder (§5.1); error slot set if already active |
| `__RTS_FN_NODE_INSPECTOR_CLOSE` | *(none)* | `Void` | closes the listener + any accepted connections; no-op if inactive |
| `__RTS_FN_NODE_INSPECTOR_URL` | *(none)* | `StrPtr` | `ptr=0,len=0` sentinel maps to `undefined` in the `.ts` shim |
| `__RTS_FN_NODE_INSPECTOR_WAIT_FOR_DEBUGGER` | *(none)* | `Void` | blocks calling thread on a real accepted TCP connection; error slot set if inactive |
| `__RTS_FN_NODE_INSPECTOR_SESSION_CREATE` | *(none)* | `Handle` | allocates session state in rts-node's own handle table |
| `__RTS_FN_NODE_INSPECTOR_SESSION_CONNECT` | `session: Handle` | `Void` | marks the session live; no network involvement |
| `__RTS_FN_NODE_INSPECTOR_SESSION_CONNECT_MAIN` | `session: Handle` | `Void` | error slot set if not called from a Worker region |
| `__RTS_FN_NODE_INSPECTOR_SESSION_DISCONNECT` | `session: Handle` | `Void` | rejects/error-backs all pending requests on this session |
| `__RTS_FN_NODE_INSPECTOR_SESSION_POST` | `session: Handle, method: StrPtr, params_json: StrPtr, request_id: U64` | `Void` | in-process dispatch against the §5.1 allowlist; result delivered via the thunk below (never a network round trip) |
| `__RTS_FN_NODE_INSPECTOR_SESSION_SET_THUNK` | `session: Handle, thunk_fn_ptr: U64` | `Void` | registers the JS-side callback/notification dispatcher (see §5.3) — same `Entry::Function` pointer-passing convention already used for timers |
| `__RTS_FN_NODE_INSPECTOR_CONSOLE_CALL` | `method: StrPtr, args_json: StrPtr` | `Void` | backs `inspector.console.*` |
| `__RTS_FN_NODE_INSPECTOR_NETWORK_EMIT` | `method: StrPtr, params_json: StrPtr` | `Void` | validates/enqueues; safe no-op in observable effect until (a) lands (§5.1) |
| `__RTS_FN_NODE_INSPECTOR_NETWORK_RESOURCE_PUT` | `url: StrPtr, content: StrPtr` | `Void` | backs `NetworkResources.put`; same no-op-until-(a) status |
| `__RTS_FN_NODE_INSPECTOR_DOMSTORAGE_EMIT` | `method: StrPtr, params_json: StrPtr` | `Void` | backs every `DOMStorage.*` broadcast helper; same no-op-until-(a) status |

- **Opaque handles:** `Session` is the only rich object — an
  `Entry::InspectorSession { pending: HashMap<u64, PendingRequest>, thunk: Option<fn_ptr>, connected: bool, main_thread: bool }`
  in rts-node's own handle table, following the same shard-aware allocation
  pattern as every other handle-based namespace (mirroring `node:v8`'s and
  `node:crypto`'s own private handle tables, not `rts-engine`'s shared
  `Entry` enum).
- **JSON passthrough, not a validated schema:** `params`/results cross the ABI
  as UTF-8 JSON text (`StrPtr`), parsed/serialized on the RTS side using
  whatever JSON implementation `rts-node` already owns for `node:util`/
  console formatting — not a hand-marshalled struct per protocol method
  (there are hundreds of documented method names; only the small allowlisted
  subset in §5.1 is ever actually interpreted, everything else produces the
  honest "not implemented" error).
- **`.ts` shim vs native extern split:** `Session`/`EventEmitter` wiring,
  `open()`/`close()`/`url()`/`waitForDebugger()` argument normalization, and
  the promise-wrapping for `node:inspector/promises` are all `.ts`. The
  in-process command dispatch (`Runtime.evaluate` bridging, protocol-error
  shaping) and the discovery listener (§5.1) are native Rust in `rts-node`
  itself (not a `.ts` shim) because they inherently need runtime internals
  (the eval/compile pipeline, a real socket) that no `.ts`-visible primitive
  exposes.

### 5.3 Async model

| Source | Binding | Sync / callback / promise |
|---|---|---|
| `open`/`close`/`url`/`waitForDebugger` | direct native call (blocking where documented) against the real listener (§5.1) | sync |
| `session.connect`/`connectToMainThread`/`disconnect` | direct native call, mutates session Handle state | sync |
| `session.post()` (`node:inspector`) | native in-process dispatch against the §5.1 allowlist; result delivered via the same thunk-into-`INVOKE_AUTO` pattern already used for timers | callback |
| `session.post()` (`node:inspector/promises`) | pure `.ts` wrapper: `new Promise((resolve, reject) => nativePost(method, params, (err, res) => err ? reject(err) : resolve(res)))` — **no separate native promise path required** | promise |
| `'inspectorNotification'` / per-method events | native side calls the registered thunk with `(method, params_json)` whenever an in-process event is generated (an allowlisted command's side effect, or a broadcast-helper call); `.ts` shim does `session.emit('inspectorNotification', msg); session.emit(msg.method, msg)` | callback (event) |
| `Network.*`/`NetworkResources.put`/`DOMStorage.*` | direct native call, synchronous validate+enqueue (§5.1/§5.2) | sync |

The `Runtime.evaluate` bridge (§5.1) is the one allowlisted command that may
take non-trivial time (it compiles and runs arbitrary TS/JS). The simplest
correct phase-(b) implementation runs it synchronously on the calling thread,
inline with `post()`, before invoking the callback — acceptable because there
is no attached network client whose request/response cycle it could stall
under this phase's scope. If a slow/hanging evaluated expression turns out to
be a real problem in practice, moving it onto a dedicated `std::thread`
`rts-node` owns itself (not necessarily the shared tokio runtime) is a
straightforward, non-blocking follow-up — see §5.7.

### 5.4 Multithread / worker interaction

- The inspector **transport/back-end state is process-wide**, matching real
  Node (there is exactly one listening socket / one `url()` for the whole
  process, not one per thread) — implemented as a single native singleton in
  `rts-node`, guarded by a mutex, independent of `rts-std`.
- **Each `Session` is per-creation-site**, not process-global: any RTS thread
  may `new Session()` and `connect()` it to *that thread's own* execution
  context. Mapped onto `docs/specs/rts-threading-model.md`: a Worker
  (`node:worker_threads`) is an RTS thread/region, and `connectToMainThread()`
  from inside that region must reach across to the main thread's inspector
  back-end — modeled as a **channel** (per the threading model's
  `channel`/`shared` surface) carrying serialized JSON command/response pairs
  between the worker's `Session` handle and the main-thread dispatcher, rather
  than a raw shared-memory struct.
- **Not** safe to `connectToMainThread()` outside a Worker region — enforced
  natively (native call checks "is this thread the main thread's region")
  and surfaces as the `ERR_INSPECTOR_NOT_WORKER`-equivalent error rather than
  a crash.
- The `Network.*`/`DOMStorage.*` broadcast helpers and `NetworkResources.put`
  are safe to call from any thread — they only serialize+enqueue onto the
  single process-wide fan-out list, guarded by the same mutex as the rest of
  the inspector state.

### 5.5 Buffer / TypedArray interop

Not applicable in any meaningful way — every message this module sends or
receives (`post()` params/results, notification `params`, the `HeapProfiler`
snapshot-chunk example in the upstream docs) is JSON **text**, crossing the
ABI as `StrPtr`. There is no `Buffer`/`TypedArray` parameter or return value
anywhere in the documented public surface.

### 5.6 Doctrine placement

- `node:inspector` is **non-primordial** — no native literal/syntactic form;
  it is an import/require-only debugging utility, squarely in the
  "Registry / `.ts` stdlib, node-module data table" bucket. The engine never
  names `Session`, `inspector`, `open`, `post`, or any other member directly.
- Resolves exactly like every other `node:*` module: `rts-node` registers
  `NodespaceSpec { node_module: "inspector", ns_prefix: "node_inspector", members }`
  (and a second entry, or the same entry under an alternate lookup key, for
  `"inspector/promises"`) in `NODE_SPECS`; `ns_prefix_for("node:inspector")` /
  `ns_prefix_for("node:inspector/promises")` resolve via the same generic
  `node:` import machinery already used for `fs`/`path`/`os`/`process` — zero
  new codegen surface, zero hardcoded module name in the engine itself.
- Native-extern vs `.ts`-shim split (restated from §5.2): the `Session`/
  `EventEmitter`/promise-wrapping ergonomics are `.ts`; the discovery listener
  (§5.1), the in-process command dispatch, and process-wide state are native
  Rust owned entirely by `rts-node` (no `rts-std` dependency, per the
  architecture decision — `rts-node` may add its own crate dependencies the
  same way it is expected to own `flate2`/`rustls`-equivalent choices for
  other modules).

### 5.7 Shared-infra dependencies (FLAG)

- **`Runtime.evaluate` reuses `node:vm`'s compile-and-run seam, not a second
  one.** Both modules bridge to the same "compile TS/JS source at runtime and
  execute it" primitive (`docs/node-implementation/vm.md` §5.1). That doc
  notes the primitive lives in `rts-std` today
  (`crates/rts-std/src/runtime/mod.rs`) with an open question about making it
  reachable without an `rts-std` dependency for `rts-node`'s independence —
  the same coordination point applies here; do not build a second eval seam
  for this module once that question is resolved.
- **Heap-stats-shaped commands reuse `node:v8`'s primitive, not a new one.**
  `node:v8`'s `getHeapStatistics` (`docs/node-implementation/v8.md` §5.1/§5.2,
  `__RTS_FN_NODE_V8_GET_HEAP_STATISTICS_JSON`) is already `rts-node`-owned —
  this is a same-crate function call, not a cross-crate dependency.
- **CPU profiling (`Profiler.*`) shares `node:v8`'s missing-sampler gap.**
  Neither `node:v8`'s `startCpuProfile` nor this module's `Profiler.start`/
  `Profiler.stop` can be implemented for real until a genuine statistical
  sampling profiler exists somewhere in the runtime
  (`docs/node-implementation/v8.md` §5.7/§7) — new native work, not currently
  a hoist candidate from any existing crate. Track as one shared
  prerequisite, not two.
- **Background thread for `Runtime.evaluate`.** Not required in phase (b)
  (§5.3) — flagged only as a straightforward, non-blocking follow-up if a
  slow evaluated expression proves to be a real problem in practice.
- **Callback/notification delivery queue.** `session.post()`'s callback
  variant and the `'inspectorNotification'`/per-method events need to enqueue
  their delivery onto the *calling thread's* callback queue so ordering
  relative to other pending timers/microtasks matches Node's single-queue
  model. Today that queue lives in `rts-std`'s `event_loop`. Since `rts-node`
  cannot depend on `rts-std`, this either needs (a) its own lightweight
  thread-local callback queue duplicated inside `rts-node` (acceptable — the
  ordering guarantee only needs to hold *within* this module's own callbacks,
  not globally interleaved with unrelated timers, since Node itself does not
  document strict cross-module callback ordering here), or (b) a hoist of the
  minimal "enqueue a callback on thread X" primitive into a shared low crate
  (`rts-engine` or a new shared crate) both `rts-std` and `rts-node` can call.
  Recommendation: start with (a) — a private queue — and revisit only if a
  concrete ordering bug surfaces against real npm packages.
- **Promise subsystem (`promise.create`/`.then`/settle).** **Not required** —
  §5.1/§5.3 deliberately implement `node:inspector/promises`'s `post()` as a
  pure `.ts`-level `new Promise(...)` wrapper around the native callback path,
  so this module never needs to call into whatever crate backs
  `promise.create` today.
- **TLS/crypto.** None — the discovery listener is plain TCP/HTTP, never TLS,
  in stock Node either.
- **Net/TCP.** Handled entirely inside `rts-node` via `std::net` (own
  implementation, per the architecture decision) — not a shared-infra
  dependency on `rts-std`'s `net`/`tls` modules.

If (b) above is ever chosen for the callback queue, that is the one item that
would need explicit hoisting; everything else in this module is achievable
fully inside `rts-node` with `rts-node`-owned dependencies, reusing the
`node:vm`/`node:v8` primitives noted above rather than duplicating them.

### 5.8 Implementation phases

All of the below is phase-(b) scope (§1/§5.1) — the full protocol-compatible
endpoint (a) is listed separately at the end as unscheduled, not a phase to
sequence against.

1. **(a)** `rts-node/src/inspector/mod.rs` — `NodespaceSpec` registration for
   both `"inspector"` and `"inspector/promises"` (unblocks import resolution,
   §5.6), `MEMBERS` initially empty.
2. **(b)** Native process-wide `InspectorState` singleton +
   `__RTS_FN_NODE_INSPECTOR_OPEN/CLOSE/URL/WAIT_FOR_DEBUGGER` backed by a real
   `std::net::TcpListener` and the HTTP `/json*` discovery responder (§5.1) —
   enough for `open()`/`close()`/`url()` to be genuinely truthful and for
   `waitForDebugger()` to block on an actual accepted connection. No
   WebSocket upgrade, no wire-level command framing.
3. **(c)** `.ts` shim: `Session` class extending `node:events`'s
   `EventEmitter`; `connect`/`connectToMainThread`/`disconnect` wired to the
   native `Handle` lifecycle from §5.2 (no `post()` yet).
4. **(d)** `session.post()` in-process dispatch for the allowlist in §5.1
   (`Runtime.enable/disable/evaluate`, `Debugger.enable/disable`,
   `Schema.getDomains`, heap-stats-shaped commands reusing `node:v8`'s
   primitive); every other method returns a real protocol-shaped error, never
   crashes or hangs.
5. **(e)** Thunk-based callback delivery for the `node:inspector` callback
   variant; `.ts`-only `Promise` wrapper for `node:inspector/promises` on top
   of the same native call (§5.3).
6. **(f)** `'inspectorNotification'` + dynamic per-method events wired
   through the same thunk mechanism, for internally-generated events (there
   is no externally-attached frontend to originate events from in this
   phase).
7. **(g)** `inspector.console` — map its (small, to-be-confirmed-exact per
   §7) method set to notification-shaped broadcasts.
8. **(h)** `Network.*`/`NetworkResources.put`/`DOMStorage.*` broadcast
   helpers, gated behind their respective `--experimental-*` flags for
   source-compat with feature-detection code that checks flag state before
   calling them; implemented as the validate+enqueue no-op described in
   §5.1.
9. **(i)** `connectToMainThread()` cross-region channel plumbing (§5.4),
   contingent on `node:worker_threads`' own RTS-thread/region mapping
   already existing.
10. **(j)** Test suite (§6); document the deferred items (§7) explicitly,
    including that `Profiler.*` and any method outside the §5.1 allowlist are
    out of scope for this phase by design, not silently unsupported.

**North star (a) — unscheduled, tracked as a future epic, not part of the
above sequence:**

- WebSocket upgrade + protocol-shaped JSON-RPC framing over the phase-(b)
  listener, sufficient for a real DevTools-class frontend to attach.
- A genuine statistical sampling profiler backing `Profiler.start`/`stop`
  (shared prerequisite with `node:v8`'s `startCpuProfile`, §5.7).
- Live `Runtime`/`Debugger` domain coverage backed by real Cranelift
  frame/stack-map introspection; real breakpoints/stepping would
  additionally need code-patching integration with the JIT — a
  disproportionate effort for this module's tier, flagged explicitly rather
  than implied (§7).

## 6. Test plan

- `inspector_open_close_url.test.ts` — `inspector.open(0)`; assert
  `inspector.url()` starts with `"ws://"` (string shape only — phase (b) does
  not implement a real WebSocket upgrade, §5.1/§7); `inspector.close()`;
  assert `inspector.url() === undefined`.
- `inspector_open_disposable.test.ts` — `using session = inspector.open(0)`
  (or manual `disposable[Symbol.dispose]()`); assert leaving scope closes the
  inspector exactly as `inspector.close()` would.
- `inspector_double_open_throws.test.ts` — call `open()` twice without an
  intervening `close()`; assert the second call throws rather than silently
  replacing the listener or hanging.
- `inspector_wait_for_debugger_throws_when_inactive.test.ts` — call
  `waitForDebugger()` with no active inspector; assert it throws immediately
  rather than blocking forever.
- `inspector_session_connect_disconnect.test.ts` — `new Session()`,
  `connect()`, `disconnect()`; assert no throw and that a `post()` call after
  `disconnect()` errors (callback receives an `Error`) instead of hanging.
- `inspector_post_runtime_evaluate_callback.test.ts` (`node:inspector`) —
  `session.post('Runtime.evaluate', { expression: '2 + 2' }, cb)`; assert `cb`
  receives `(null, { result: { value: 4, ... } })`.
- `inspector_post_runtime_evaluate_promise.test.ts` (`node:inspector/promises`)
  — `await session.post('Runtime.evaluate', { expression: '2 + 2' })`; assert
  the resolved value's `result.value === 4`.
- `inspector_post_unsupported_method.test.ts` — post a method outside the
  allowlist (e.g. `'Profiler.start'`, deferred per §5.1/§7); assert the
  callback/promise receives a real error object (not a crash, not a silent
  no-op, not a hang).
- `inspector_heap_stats_shares_v8_primitive.test.ts` — post an allowlisted
  heap-stats-shaped command via `Session`; assert the numeric fields match
  `require('node:v8').getHeapStatistics()` called separately in the same
  process — proves the §5.1/§5.7 reuse (one primitive, not two divergent
  implementations).
- `inspector_inspector_notification_fires.test.ts` — trigger any allowlisted
  command that produces a notification side effect (or a connected-frontend
  round trip in-process); assert `'inspectorNotification'` fires with the
  expected `{ method, params }` shape.
- `inspector_per_method_event_fires.test.ts` — assert
  `session.on('Debugger.enable', ...)`-style per-method listener receives the
  same message object as the generic `'inspectorNotification'` listener for
  the same event.
- `inspector_console_object_no_parity_but_no_throw.test.ts` —
  `inspector.console.log('a message')`; assert it does not throw, explicitly
  without asserting output-format parity with global `console` (per §4, no
  parity is guaranteed/expected).
- `inspector_connect_to_main_thread_outside_worker_throws.test.ts` — call
  `session.connectToMainThread()` from the main thread; assert it throws.
- `inspector_connect_to_main_thread_from_worker.test.ts` (multithread) —
  spawn a `node:worker_threads` Worker, inside it create a `Session` and call
  `connectToMainThread()`, `post()` an allowlisted command, assert the result
  round-trips from the main thread's back-end. Mark expected-fail/skip until
  `node:worker_threads`' own region mapping lands if it isn't ready yet.
- `inspector_disconnect_rejects_pending.test.ts` — `post()` a command, then
  `disconnect()` before any reply is delivered; assert the pending
  callback/promise settles with an error rather than the process hanging.
- `inspector_network_experimental_flag_gated.test.ts` — call
  `inspector.Network.requestWillBeSent({...})` without
  `--experimental-network-inspection` set; assert it is a safe no-op (per
  Node's own documented flag-gating, and independently a no-op under phase
  (b) since there is no attached frontend to receive it, §5.1) rather than
  throwing or crashing.
- `inspector_domstorage_broadcast_shape.test.ts` — call each `DOMStorage.*`
  helper with a minimal valid `params` object; assert no throw and that the
  native call validates/serializes the exact field names from §3 (no
  connected-frontend round trip expected under phase (b), §5.1).
- `inspector_module_load_no_crash.test.ts` — bare
  `import * as inspector from "node:inspector"` (and the `/promises` variant)
  with no further usage; assert the program runs to completion without
  opening a listener or throwing.

## 7. Open questions / deferrals

- **Full protocol-compatible endpoint (north star (a)).** Whether/when RTS
  grows the phase-(b) discovery listener into a genuine WebSocket + JSON-RPC
  wire server that a real DevTools-class frontend can attach to is open and
  **unscheduled** — §5.1/§5.8 list it separately and explicitly as a future
  epic, not a silent "later" implied by "minimal subset" language elsewhere.
  Until it lands, `url()`'s `ws://`-shaped string is not actually
  connectable (§5.1/§6).
- **CPU profiling (`Profiler.start`/`Profiler.stop`) is deferred outright.**
  RTS has no statistical/sampling profiler anywhere in the runtime today —
  the identical gap `node:v8`'s `startCpuProfile` documents
  (`docs/node-implementation/v8.md` §5.7/§7). Not part of phase (b); ships
  returning an honest "not implemented" error, never a fabricated profile.
  One shared prerequisite (§5.7) unblocks both modules at once.
- **Real breakpoint/stepping debugging is explicitly out of scope for the
  foreseeable future, not just deferred quietly.** Full parity
  (`Debugger.setBreakpoint`, `Debugger.stepOver`, live call-stack inspection)
  would require deep coupling with the Cranelift JIT — patching generated
  code for breakpoints, integrating with the existing
  `UserStackMap`/conservative-stack-scanner machinery for live-frame
  inspection, and (potentially) source-level stepping over generated IR. This
  is disproportionate engineering for a P2-tier module; it is **not** part of
  the phased plan in §5.8, and is **not** implied to be part of north star
  (a) either unless a concrete need emerges — flagged here rather than
  silently assumed.
- **Security posture of the `Runtime.evaluate` bridge.** Bridging
  `Runtime.evaluate` to RTS's own eval/compile pipeline means that **once
  north star (a) exists** and the endpoint is reachable over the network,
  it is arbitrary-code-execution by design (matching real Node's own
  documented risk) — the implementation must default to loopback-only
  binding and must never widen that default silently. Under phase (b) there
  is no network reachability into `session.post()` at all (§5.1), so the
  practical exposure today is limited to same-process callers; still worth a
  dedicated security-review pass before any work toward (a) begins.
- **Exact `inspector.console` method list.** Node's docs state "no API
  parity" with only a `.log()` example shown; the complete method set needs a
  source-level check (`lib/internal/inspector_console.js` in Node's own
  source) before phase (g) commits to a specific list.
- **Exact `ERR_INSPECTOR_*` codes/wording.** Neither the fetched docs page nor
  a source-level check has confirmed exact error message text for
  `ERR_INSPECTOR_ALREADY_ACTIVATED`, `ERR_INSPECTOR_ALREADY_CONNECTED`,
  `ERR_INSPECTOR_CLOSED`, `ERR_INSPECTOR_COMMAND`,
  `ERR_INSPECTOR_NOT_AVAILABLE`, `ERR_INSPECTOR_NOT_WORKER` — used from the
  general, well-known Node error-code naming convention. Matching on error
  *type*/condition is safe; exact string interpolation is not yet confirmed.
- **Exact current CLI flag set/spelling** (`--inspect`, `--inspect-brk`,
  `--inspect-port`, `--inspect-wait`, `--inspect-publish-uid`) — not returned
  by the fetched docs page for this module; filled from general knowledge,
  needs a check against Node 25's CLI options doc. Separately open: whether
  RTS should ever expose an actual `--inspect`-equivalent CLI flag given
  phase (b) has no network-attachable endpoint worth auto-opening on launch.
- **`Runtime.evaluate` / heap-stats / CPU-profiler prerequisite hoists
  (§5.7).** Whether the compile-and-run seam and the future sampling
  profiler end up living in `rts-node` directly or get hoisted into a lower
  shared crate is decided by `node:vm`'s and `node:v8`'s own specs, not this
  one — this module only consumes whatever they land on, and must not
  duplicate either primitive in the meantime.
- **Shared callback-queue hoist (§5.7).** Deferred decision between a private
  `rts-node`-local callback queue (default plan) versus hoisting a minimal
  "enqueue on thread X" primitive into shared low-level infra — revisit if a
  concrete cross-module callback-ordering bug is found against real usage.
- **`Session` identity across the two import specifiers.** Whether
  `require('node:inspector').Session === require('node:inspector/promises').Session`
  in real Node (same class, `post()` behavior switched by an internal flag)
  or two distinct class objects — affects whether RTS's `.ts` shim can share
  one class definition (simpler) or must produce two distinct constructors;
  not blocking (either implementation satisfies the documented per-variant
  behavior) but worth confirming for `instanceof`-sensitive npm packages.
