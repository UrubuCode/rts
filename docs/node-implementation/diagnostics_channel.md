# node:diagnostics_channel

**RTS rts-node implementation spec — Node.js 25 parity.**

| | |
|---|---|
| Module | `node:diagnostics_channel` |
| Node.js version | 25.x (introduced v15.1.0 / v14.17.0; current API stabilized through v25.9.0) |
| Stability | 2 - Stable (declared stable since v19.2.0 / v18.13.0) |
| Tier | P2 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import * as diagnostics_channel from "node:diagnostics_channel"`; named `import { channel, hasSubscribers, subscribe, unsubscribe, tracingChannel, Channel, TracingChannel } from "node:diagnostics_channel"`. Bare specifier `"diagnostics_channel"` (no `node:` prefix) is legal in real Node but is **not** resolved by RTS's current `ns_prefix_for` (it only strips a literal `"node:"` prefix) — out of scope for this spec, tracked at the resolver level, not per-module. |
| Globals exposed | None. This module adds nothing to `globalThis`; it is reached only via explicit import. |

## 1. Purpose

`node:diagnostics_channel` is a general-purpose, in-process publish/subscribe bus for
diagnostics and observability data. Any code (core Node modules, userland libraries, or
the application itself) can `publish()` structured messages onto a named `Channel`
without knowing or caring whether anything is listening, and any code can `subscribe()`
to a named channel to receive those messages synchronously. Node's own core modules use
it internally to emit "built-in" instrumentation events (HTTP requests, module loads,
child-process spawns, worker creation, etc.) that APM/tracing libraries consume without
monkey-patching. `TracingChannel` is a higher-level helper built on top of five regular
channels (`start`/`end`/`asyncStart`/`asyncEnd`/`error`) that wraps a sync function,
a promise-returning function, or a callback-style function and automatically publishes
the right events around its execution.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Channel`

Not constructible directly by user code (`new Channel(...)` is not part of the public
API) — instances are obtained exclusively via `diagnostics_channel.channel(name)`.
Does **not** extend `EventEmitter`; it has its own minimal method set.

- **Instance properties**

  | Property | Type | Description |
  |---|---|---|
  | `name` | `string \| symbol` | (readonly, not formally documented as public but present on every instance) The name this channel was created with. |
  | `hasSubscribers` | `boolean` (getter) | `true` if this channel currently has one or more active `subscribe()` listeners. Checking this on the retrieved `Channel` object (rather than calling the module-level `diagnostics_channel.hasSubscribers(name)` function repeatedly) is the recommended fast-path pattern to avoid building an expensive message when nobody is listening. |

- **Instance methods**

  ##### `channel.publish(message)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `message` | `any` | no | — |

  Returns: `undefined`. Throws: never (subscriber exceptions are caught internally, see
  §4). Variant: **sync**.

  Synchronously invokes every current subscriber as `onMessage(message, this.name)`, in
  subscription order. A no-op (but never an error) when there are zero subscribers.

  ##### `channel.subscribe(onMessage)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `onMessage` | `(message: any, name: string \| symbol) => void` | no | — |

  Returns: `undefined`. Throws: `TypeError`/`ERR_INVALID_ARG_TYPE` (verify exact code) if
  `onMessage` is not a function. Variant: **sync**. Added in v18.7.0 / v16.17.0 (was
  briefly runtime-deprecated in the same releases, deprecation **revoked** in v24.8.0 /
  v22.20.0 — treat as a normal, non-deprecated, fully supported method).

  ##### `channel.unsubscribe(onMessage)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `onMessage` | `(message: any, name: string \| symbol) => void` | no | — |

  Returns: `boolean` — `true` if the exact handler reference was found and removed,
  `false` otherwise. Throws: none beyond arg-type validation. Variant: **sync**. Same
  version history as `subscribe()`.

  ##### `channel.bindStore(store, transform?)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `store` | `AsyncLocalStorage<any>` (from `node:async_hooks`) | no | — |
  | `transform` | `(message: any) => any` | yes | identity function |

  Returns: `undefined`. Variant: **sync** (sets up state consulted later, during
  `runStores`/publish). Added in v19.9.0 / v18.19.0. Binds `store` so that every
  `channel.runStores(context, fn, ...)` call also enters `store`'s context with
  `transform(context)` (or `context` itself if no transform given) as the store value for
  the duration of `fn`.

  ##### `channel.unbindStore(store)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `store` | `AsyncLocalStorage<any>` | no | — |

  Returns: `boolean` — `true` if `store` was bound and is now removed, `false`
  otherwise. Variant: **sync**. Added in v19.9.0 / v18.19.0.

  ##### `channel.runStores(context, fn, thisArg?, ...args)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `context` | `any` | no | — |
  | `fn` | `(...args: any[]) => any` | no | — |
  | `thisArg` | `any` | yes | `undefined` |
  | `...args` | `any[]` | yes | `[]` |

  Returns: `any` — whatever `fn` returns. Variant: **sync**. Added in v19.9.0 /
  v18.19.0. Publishes `context` to this channel's subscribers (exactly like `publish`)
  **and** enters every store bound via `bindStore` for the duration of the call to `fn`,
  then unwinds all store contexts (and re-throws) if `fn` throws.

#### `class TracingChannel`

Not constructible directly — obtained via `diagnostics_channel.tracingChannel(nameOrChannels)`.
Wraps 5 individual `Channel` instances, exposed as **own properties** of the same names:
`start`, `end`, `asyncStart`, `asyncEnd`, `error` (each a full `Channel`, individually
subscribable via `tracingChannel.start.subscribe(...)` etc., in addition to the bulk
`subscribe`/`unsubscribe` below).

- **Instance properties**

  | Property | Type | Description |
  |---|---|---|
  | `hasSubscribers` | `boolean` (getter) | `true` if **any** of the 5 underlying channels has at least one subscriber. Added in v22.0.0 / v20.13.0. |
  | `start` | `Channel` | Channel named `tracing:${name}:start`. |
  | `end` | `Channel` | Channel named `tracing:${name}:end`. |
  | `asyncStart` | `Channel` | Channel named `tracing:${name}:asyncStart`. |
  | `asyncEnd` | `Channel` | Channel named `tracing:${name}:asyncEnd`. |
  | `error` | `Channel` | Channel named `tracing:${name}:error`. |

- **Instance methods**

  ##### `tracingChannel.subscribe(subscribers)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `subscribers` | `TracingChannelSubscribers` (see §3) | no | — |

  Returns: `undefined`. Variant: **sync**. Subscribes each provided handler
  (`start`/`end`/`asyncStart`/`asyncEnd`/`error`) to its corresponding underlying
  channel; any key omitted from `subscribers` is simply not subscribed.

  ##### `tracingChannel.unsubscribe(subscribers)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `subscribers` | `TracingChannelSubscribers` | no | — |

  Returns: `boolean` — `true` only if **every** provided handler was found and removed
  from its channel; `false` if any one was not found (partial unsubscription may still
  occur even when the return is `false`). Variant: **sync**.

  ##### `tracingChannel.traceSync(fn, context?, thisArg?, ...args)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `fn` | `(...args: any[]) => any` | no | — |
  | `context` | `object` | yes | `{}` |
  | `thisArg` | `any` | yes | `undefined` |
  | `...args` | `any[]` | yes | `[]` |

  Returns: `any` — `fn`'s return value (rethrows if `fn` throws, after publishing to
  `error`). Throws: whatever `fn` throws, after the `error` channel fires. Variant:
  **sync**. Added in v19.9.0 / v18.19.0. Publishes `start` before calling `fn`, then
  either sets `context.result` and publishes `end` (success) or sets `context.error`,
  publishes `error`, publishes `end`, and rethrows (failure).

  ##### `tracingChannel.tracePromise(fn, context?, thisArg?, ...args)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `fn` | `(...args: any[]) => Promise<any>` | no | — |
  | `context` | `object` | yes | `{}` |
  | `thisArg` | `any` | yes | `undefined` |
  | `...args` | `any[]` | yes | `[]` |

  Returns: `Promise<any>` chained onto `fn`'s returned promise. Variant: **promise**.
  Added in v19.9.0 / v18.19.0. Publishes `start`, calls `fn`; once the returned promise
  settles, publishes `asyncStart` (result on the promise's own thread of execution) then
  `asyncEnd`, and `end` synchronously right after the call returns (before the promise
  settles) per the `start`/`end` vs `asyncStart`/`asyncEnd` split documented in §2.4. If
  `fn`'s return value is not thenable, a process warning is emitted and tracing exits
  early (still returns the value).

  ##### `tracingChannel.traceCallback(fn, position?, context?, thisArg?, ...args)`

  | Param | Type | Optional | Default |
  |---|---|---|---|
  | `fn` | `(...args: any[]) => any` | no | — |
  | `position` | `number` | yes | index of the last argument in `args` |
  | `context` | `object` | yes | `{}` |
  | `thisArg` | `any` | yes | `undefined` |
  | `...args` | `any[]` | no (must include the callback at `position`) | — |

  Returns: `any` — `fn`'s own return value (not the callback's). Variant: **callback**.
  Added in v19.9.0 / v18.19.0. Replaces `args[position]` with a wrapping callback before
  invoking `fn`; the wrapper publishes `asyncStart`/`asyncEnd` (and `error` if the
  callback's first, Node-convention `err` argument is set) around invocation of the
  original callback, and `start`/`end` around the synchronous call to `fn` itself.

### 2.2 Top-level functions

##### `diagnostics_channel.hasSubscribers(name)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `name` | `string \| symbol` | no | — |

Returns: `boolean`. Throws: arg-type validation error if `name` is neither `string` nor
`symbol`. Variant: **sync**. Looks the channel up **without** creating/registering it —
cheaper than `channel(name).hasSubscribers` when you don't otherwise need the `Channel`
object. Added in v15.1.0 / v14.17.0.

##### `diagnostics_channel.channel(name)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `name` | `string \| symbol` | no | — |

Returns: `Channel` — **always** a `Channel` object, even when it has zero subscribers
(the module deliberately makes construction unconditional so hot publish sites don't
need a defensive null check; see §4). Throws: arg-type validation error. Variant:
**sync**. Added in v15.1.0 / v14.17.0. Repeated calls with the same `name` return
channel objects that behave identically (same registry entry while it has subscribers
or bound stores; see the GC note in §4 for the no-subscriber case).

##### `diagnostics_channel.subscribe(name, onMessage)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `name` | `string \| symbol` | no | — |
| `onMessage` | `(message: any, name: string \| symbol) => void` | no | — |

Returns: `undefined`. Variant: **sync**. Equivalent to `channel(name).subscribe(onMessage)`
— convenience form that doesn't require holding onto the `Channel` object. Added in
v18.7.0 / v16.17.0 (deprecation revoked v24.8.0 / v22.20.0, see `Channel.subscribe`).

##### `diagnostics_channel.unsubscribe(name, onMessage)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `name` | `string \| symbol` | no | — |
| `onMessage` | `(message: any, name: string \| symbol) => void` | no | — |

Returns: `boolean`. Variant: **sync**. Equivalent to `channel(name).unsubscribe(onMessage)`.
Added in v18.7.0 / v16.17.0.

##### `diagnostics_channel.tracingChannel(nameOrChannels)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `nameOrChannels` | `string \| TracingChannelCollection` (see §3) | no | — |

Returns: `TracingChannel`. Variant: **sync** (constructs a synchronous wrapper object;
no I/O). Added in v19.9.0 / v18.19.0. When given a `string`, creates the 5 channels
named `tracing:${name}:{start,end,asyncStart,asyncEnd,error}`. When given an object of
pre-existing `Channel` instances (`{ start, end, asyncStart, asyncEnd, error }`), wraps
those instead — lets callers use custom channel names/objects while still getting the
`traceSync`/`tracePromise`/`traceCallback` helpers.

### 2.3 Properties & constants

No top-level constants are exported by this module. The only "constants" in the API
surface are the well-known **built-in channel name strings** Node's own core modules
publish to. These are not exported bindings — they are plain string literals passed to
`channel()`/`subscribe()`. Full catalog (all currently marked **Experimental** by Node):

| Category | Channel name(s) |
|---|---|
| Console | `console.log`, `console.info`, `console.debug`, `console.warn`, `console.error` |
| HTTP (client) | `http.client.request.created`, `http.client.request.start`, `http.client.request.error`, `http.client.response.finish` |
| HTTP (server) | `http.server.request.start`, `http.server.response.created`, `http.server.response.finish` |
| HTTP/2 (client) | `http2.client.stream.created`, `http2.client.stream.start`, `http2.client.stream.error`, `http2.client.stream.finish`, `http2.client.stream.bodyChunkSent`, `http2.client.stream.bodySent`, `http2.client.stream.close` |
| HTTP/2 (server) | `http2.server.stream.created`, `http2.server.stream.start`, `http2.server.stream.error`, `http2.server.stream.finish`, `http2.server.stream.close` |
| Module (CJS `require`) | `module.require.start`, `module.require.end`, `module.require.error` |
| Module (ESM `import()`) | `module.import.asyncStart`, `module.import.asyncEnd`, `module.import.error` |
| Net | `net.client.socket`, `net.server.socket`, `tracing:net.server.listen:asyncStart`, `tracing:net.server.listen:asyncEnd`, `tracing:net.server.listen:error` |
| UDP | `udp.socket` |
| Process / child_process | `child_process`, `tracing:child_process.spawn:start`, `tracing:child_process.spawn:end`, `tracing:child_process.spawn:error`, `execve` |
| Web Locks (v25.9.0+) | `locks.request.start`, `locks.request.grant`, `locks.request.miss`, `locks.request.end` |
| Worker threads | `worker_threads` |

### 2.4 Events

`Channel`/`TracingChannel` are not `EventEmitter`s and have no `on()`/`emit()`/`once()` —
"events" in this module means (a) the generic `onMessage(message, name)` subscriber
contract used by every `subscribe`/`bindStore`/`runStores` call, and (b) the concrete
message shapes published on the `TracingChannel` sub-channels and the built-in channels
above. Full per-channel payload shapes:

- **`TracingChannel` sub-channel events** (fired on `tracing:${name}:{start,end,asyncStart,asyncEnd,error}`):
  - `start(event)` — contains the traced function's arguments (or whatever the caller
    put in `context`) at call time.
  - `end(event)` — adds `result` (sync return value) or `error` (thrown value) to the
    same object passed to `start`.
  - `asyncStart(event)` — for `traceCallback`: `error` (callback's first arg, if
    truthy) / `result` (callback's second arg); for `tracePromise`: `result` (resolved
    value) or `error` (rejected value).
  - `asyncEnd(event)` — same shape as `asyncStart`; published immediately after (fields
    generally unchanged between the two for the current implementation).
  - `error(event)` — `error`: the error object/value produced by the traced function.

- **Built-in channel events** — see §3 for full per-channel TypeScript payload shapes
  (`ConsoleLogEvent`, `HttpClientRequestCreatedEvent`, … one interface per channel name
  in the §2.3 catalog).

## 3. Types & option objects

```ts
/** Subscriber callback contract shared by Channel.subscribe / diagnostics_channel.subscribe. */
type ChannelMessageListener<TMessage = unknown> =
  (message: TMessage, name: string | symbol) => void;

/** Transform applied to the published message before it becomes a store's context. */
type StoreTransformer<TMessage = unknown, TContext = unknown> =
  (message: TMessage) => TContext;

/**
 * Minimal AsyncLocalStorage surface diagnostics_channel depends on.
 * Real type lives in node:async_hooks (not yet implemented in RTS — see §5.7/§7).
 */
interface AsyncLocalStorageLike<T = unknown> {
  getStore(): T | undefined;
  run<R>(store: T, callback: (...args: unknown[]) => R, ...args: unknown[]): R;
  enterWith(store: T): void;
  disable(): void;
}

/** Bulk subscriber set accepted by TracingChannel.subscribe/unsubscribe. */
interface TracingChannelSubscribers<TContext extends object = Record<string, unknown>> {
  start?: (event: TracingChannelStartEvent<TContext>) => void;
  end?: (event: TracingChannelEndEvent<TContext>) => void;
  asyncStart?: (event: TracingChannelAsyncEvent<TContext>) => void;
  asyncEnd?: (event: TracingChannelAsyncEvent<TContext>) => void;
  error?: (event: TracingChannelErrorEvent<TContext>) => void;
}

/** Pre-built channel set accepted by diagnostics_channel.tracingChannel(nameOrChannels). */
interface TracingChannelCollection {
  start: Channel;
  end: Channel;
  asyncStart: Channel;
  asyncEnd: Channel;
  error: Channel;
}

/** event object shapes for the 5 TracingChannel sub-channels. TContext is the caller-supplied `context` (or {}). */
type TracingChannelStartEvent<TContext extends object> = TContext;

interface TracingChannelEndEvent<TContext extends object> extends TContext {
  result?: unknown;
  error?: unknown;
}

type TracingChannelAsyncEvent<TContext extends object> = TracingChannelEndEvent<TContext>;

interface TracingChannelErrorEvent<TContext extends object> extends TContext {
  error: unknown;
}

// ---- Built-in channel payload shapes (§2.3 catalog) ----
// Referenced classes (http.ClientRequest, net.Socket, dgram.Socket, ChildProcess, Worker, …)
// belong to their OWN node:* modules, not yet implemented in RTS; typed here as forward
// references / `unknown` placeholders for signature completeness of THIS module's contract.

interface ConsoleChannelEvent { args: unknown[]; }
// 'console.log' | 'console.info' | 'console.debug' | 'console.warn' | 'console.error'

interface HttpClientRequestCreatedEvent { request: /* http.ClientRequest */ unknown; }
interface HttpClientRequestStartEvent { request: /* http.ClientRequest */ unknown; }
interface HttpClientRequestErrorEvent { request: /* http.ClientRequest */ unknown; error: Error; }
interface HttpClientResponseFinishEvent {
  request: /* http.ClientRequest */ unknown;
  response: /* http.IncomingMessage */ unknown;
}

interface HttpServerRequestStartEvent {
  request: /* http.IncomingMessage */ unknown;
  response: /* http.ServerResponse */ unknown;
  socket: /* net.Socket */ unknown;
  server: /* http.Server */ unknown;
}
interface HttpServerResponseCreatedEvent {
  request: /* http.IncomingMessage */ unknown;
  response: /* http.ServerResponse */ unknown;
}
interface HttpServerResponseFinishEvent {
  request: /* http.IncomingMessage */ unknown;
  response: /* http.ServerResponse */ unknown;
  socket: /* net.Socket */ unknown;
  server: /* http.Server */ unknown;
}

type Http2Headers = Record<string, string | string[]>;

interface Http2ClientStreamCreatedEvent { stream: /* http2.ClientHttp2Stream */ unknown; headers: Http2Headers; }
interface Http2ClientStreamStartEvent { stream: unknown; headers: Http2Headers; }
interface Http2ClientStreamErrorEvent { stream: unknown; error: Error; }
interface Http2ClientStreamFinishEvent { stream: unknown; headers: Http2Headers; flags: number; }
interface Http2ClientStreamBodyChunkSentEvent {
  stream: unknown;
  writev: boolean;
  data: Buffer | string | Buffer[] | Array<{ chunk: Buffer | string; encoding: string }>;
  encoding: string;
}
interface Http2ClientStreamBodySentEvent { stream: unknown; }
interface Http2ClientStreamCloseEvent { stream: unknown; }

interface Http2ServerStreamCreatedEvent { stream: /* http2.ServerHttp2Stream */ unknown; headers: Http2Headers; }
interface Http2ServerStreamStartEvent { stream: unknown; headers: Http2Headers; }
interface Http2ServerStreamErrorEvent { stream: unknown; error: Error; }
interface Http2ServerStreamFinishEvent { stream: unknown; headers: Http2Headers; flags: number; }
interface Http2ServerStreamCloseEvent { stream: unknown; }

interface ModuleRequireStartEvent { id: string; parentFilename: string; }
interface ModuleRequireEndEvent { id: string; parentFilename: string; }
interface ModuleRequireErrorEvent { id: string; parentFilename: string; error: Error; }
interface ModuleImportAsyncStartEvent { id: string; parentURL: /* URL */ unknown; }
interface ModuleImportAsyncEndEvent { id: string; parentURL: unknown; }
interface ModuleImportErrorEvent { id: string; parentURL: unknown; error: Error; }

interface NetClientSocketEvent { socket: /* net.Socket | tls.TLSSocket */ unknown; }
interface NetServerSocketEvent { socket: /* net.Socket */ unknown; }
interface NetServerListenAsyncStartEvent { server: /* net.Server */ unknown; options: Record<string, unknown>; }
interface NetServerListenAsyncEndEvent { server: unknown; }
interface NetServerListenErrorEvent { server: unknown; error: Error; }

interface UdpSocketEvent { socket: /* dgram.Socket */ unknown; }

interface ChildProcessEvent { process: /* child_process.ChildProcess */ unknown; }
interface ChildProcessSpawnStartEvent { process: unknown; options: Record<string, unknown>; }
interface ChildProcessSpawnEndEvent { process: unknown; }
interface ChildProcessSpawnErrorEvent { process: unknown; error: Error; }
interface ExecveEvent { execPath: string; args: string[]; env: string[]; }

interface LocksRequestStartEvent { name: string; mode: "exclusive" | "shared"; }
interface LocksRequestGrantEvent { name: string; mode: "exclusive" | "shared"; }
interface LocksRequestMissEvent { name: string; mode: "exclusive" | "shared"; }
interface LocksRequestEndEvent {
  name: string;
  mode: "exclusive" | "shared";
  steal: boolean;
  ifAvailable: boolean;
  error?: Error;
}

interface WorkerThreadsEvent { worker: /* worker_threads.Worker */ unknown; }
```

## 4. Node semantics & edge cases

- **`name` may be `string` or `symbol`.** Symbol-named channels let a module keep a
  private, guaranteed-collision-free channel (nobody else can `subscribe` to it without
  holding a reference to the same `Symbol`), unlike string names which are a shared
  global namespace.
- **`channel(name)` never returns `undefined`/`null`.** This is a deliberate design
  choice so hot code paths can do `const ch = channel("x"); ... ch.publish(...)`
  unconditionally with no null-check; the cost of an unsubscribed channel is meant to be
  "one property read on an object", not "an allocation + registry write" — see the
  `hasSubscribers` fast-path guidance in §2.1.
- **`diagnostics_channel.hasSubscribers(name)` does a registry lookup without forcing
  channel creation** — cheaper for a one-off check than `channel(name).hasSubscribers`
  when the channel isn't otherwise needed.
- **The channel registry is process-global and shared across every `require`/`import`**
  of the module (module caching makes `diagnostics_channel` itself a singleton); in
  real Node, unreferenced `Channel` objects with zero subscribers **and** zero bound
  stores are eligible for garbage collection via an internal `WeakReference` +
  `FinalizationRegistry` pair, so `channel(name) === channel(name)` is **not**
  guaranteed to hold across two calls separated by a GC cycle in which the channel had
  no subscribers in between — this is purely a memory-management optimization and must
  never be observable as a correctness difference (a freshly re-created channel for the
  same name behaves identically).
- **Subscriber exceptions never break `publish()`.** If an `onMessage` handler throws,
  Node catches it and defers it to `process.nextTick()` as an `'uncaughtException'`
  (see `process.html#event-uncaughtexception`) — critically, **this does not stop the
  remaining subscribers of the same `publish()` call** from running; each subscriber
  invocation is independent.
- **Zero-cost-when-unused is the design intent**, not a hard runtime guarantee: even a
  channel with subscribers only pays for the (small) publish loop; the point of
  `hasSubscribers` is to let the *caller* skip building an expensive message object
  when nobody is listening, not that the module magically elides the check itself.
- **`bindStore`/`unbindStore`/`runStores` are pure `AsyncLocalStorage` integration
  sugar** — `runStores(context, fn, ...)` both publishes `context` to plain subscribers
  *and* enters every bound store's context (transformed if a `transform` was given to
  `bindStore`) for the duration of `fn`, unwinding correctly (including on throw).
- **No explicit "unregister a channel" API** — the only lifecycle is
  subscribe/unsubscribe (observable) and GC-based cleanup (not observable, see above).
- **Deprecations: none currently active.** `Channel.prototype.subscribe`/`unsubscribe`
  had a brief *documentation-only* deprecation (v18.7.0/v16.17.0) that was **revoked**
  in v24.8.0/v22.20.0 — do not implement any deprecation warning for these.
- **No OS-specific behavior.** Pure in-process JS feature; no Windows/POSIX divergence,
  no errno/OS error codes. The only "errors" are argument-validation `TypeError`s
  (Node convention: `ERR_INVALID_ARG_TYPE` for a non-function `onMessage` or a
  non-string/symbol `name` — verify exact code against Node source at implementation
  time) and whatever `Error` the traced function itself throws/rejects with.
- **Not cross-thread.** Each JS realm (in Node: the main thread and each
  `worker_threads.Worker`) has its own independent module cache and therefore its own
  independent channel registry — subscribing on the main thread does **not** see
  messages published inside a Worker, and vice versa. The single documented exception is
  the `'worker_threads'` built-in channel itself, which fires **on the parent thread**
  (the one that called `new Worker(...)`), not inside the new worker.
- **Built-in channels are all marked Experimental** in the Node docs (console/http/
  http2/module/net/udp/process/locks/worker_threads) — the core pub/sub API
  (`channel`/`subscribe`/`unsubscribe`/`hasSubscribers`/`Channel`/`TracingChannel`) is
  Stable; only the specific instrumentation points core modules choose to publish are
  Experimental and may change/gain/lose fields across minors.

## 5. RTS implementation notes

### 5.1 Native impl mapping

This module needs **almost no native Rust code**. `Channel`/`TracingChannel` are a pure
in-memory pub/sub data structure over primitives the RTS engine already provides to any
TS code (closures/`Function` values, `Map`, `Symbol`, `try`/`catch`) — there is no
filesystem, network, or OS call anywhere in the module's own logic. The entire surface
in §2.1/§2.2 (registry `Map<string | symbol, ChannelImpl>`, `Channel` class, the 5
module-level functions, `TracingChannel` composition, `traceSync`/`tracePromise`/
`traceCallback`) ships as a **`.ts` shim** under `crates/rts-node/ts/diagnostics_channel.ts`,
analogous to how non-primordial engine-adjacent globals (console, Map/Set) ship as
ambient `.ts` per the CLAUDE.md anti-hardcode doctrine — except here it lives under
rts-node's own sources (rts-node is independent, does not reach into `rts-shared`).

The one piece of genuine native surface is a **bridge for OTHER rts-node modules
written in Rust** (once `node:net`, `node:http`, `node:child_process`, etc. exist) to
publish into the *same* TS-side registry without round-tripping a full callback
invocation for the common "is anybody even listening" check. See §5.2.

### 5.2 ABI surface

Two thin `extern "C"` symbols, declared as `NodespaceMember`s in a new
`diagnostics_channel::SPEC` (module `"diagnostics_channel"`, `ns_prefix`
`"node_diagnostics_channel"`), following the existing `NodespaceSpec`/`NodespaceMember`
shape in `crates/rts-node/src/lib.rs`:

| Symbol | Args (`AbiType`) | Returns | Purpose |
|---|---|---|---|
| `__RTS_FN_NODE_DIAGNOSTICS_CHANNEL_HAS_SUBSCRIBERS_NATIVE` | `StrPtr` (channel name) | `Bool` | Lets a Rust-implemented node module (net/http/…) check `hasSubscribers` for a **string**-named built-in channel before constructing an event payload, without calling back into TS. Mirrors a native side counter kept in sync by the `.ts` registry's `subscribe`/`unsubscribe` (each calls this one-way sync extern below on transition 0↔1 subscriber). |
| `__RTS_FN_NODE_DIAGNOSTICS_CHANNEL_PUBLISH_FROM_NATIVE` | `StrPtr` (channel name), `Handle` (message object) | `Void` | Lets native Rust node-module code publish a message (already constructed as an RTS object/`Handle`) into the TS-side registry, invoking the real subscriber `Function` values exactly as `channel.publish` would. |

Everything else in §2 (the `Channel`/`TracingChannel` object shape, `bindStore`/
`runStores`, `traceSync`/`tracePromise`/`traceCallback`) is **not** on the ABI boundary
at all — it is plain TS talking to plain TS `Function` values in the same heap, so no
marshalling occurs. `Channel` objects are **not** opaque native `Handle`s; they are
ordinary shape-based TS objects (the engine's own shapes + data ICs already give them
fast property access — no bespoke Rust-side table needed).

Symbol-named channels never cross the ABI: the two bridge externs only take `StrPtr`
because they exist solely to let *native* code publish to the **string-named built-in**
channels documented in §2.3 — user/library code creating `Symbol`-named channels only
ever talks to them from TS/JS, so no native path is required for that case.

### 5.3 Async model

- `hasSubscribers` / `channel` / `subscribe` / `unsubscribe` / `Channel.publish` /
  `Channel.subscribe` / `Channel.unsubscribe`: fully **synchronous**, no event loop or
  tokio involvement of any kind.
- `TracingChannel.traceSync`: synchronous wrapper (try/catch around a direct call);
  no async infra needed.
- `TracingChannel.tracePromise`: composes with the **existing** RTS Promise surface
  (`promise.then`/await, already primordial/engine-level per the async/Promise/Function
  design — see `.claude/rules/03-features.md`) purely from the `.ts` shim, i.e. it is
  written as ordinary `.then()`/`.catch()` JS, not a new native async primitive.
- `TracingChannel.traceCallback`: wraps the target callback argument with a JS wrapper
  function; uses the existing `Function`-value call machinery (already how
  `.call`/`.apply`/`.bind` work) — no new native trampoline required.
- `Channel.bindStore` / `Channel.runStores`: these are the one area that depends on
  **context propagation across async boundaries** (`AsyncLocalStorage`), which does not
  exist in RTS today (see §5.7 and §7) — blocked until `node:async_hooks` lands.

### 5.4 Multithread / worker interaction

Per Node semantics (§4) and the RTS threading model
(`docs/specs/rts-threading-model.md`), the channel registry is **per-realm, not
shared**: it must live in **thread-local** storage (the RTS `threadLocal` region), one
fresh `Map` per OS/engine thread. Concretely:

- Main thread and each `worker_threads.Worker` (once that module exists, mapped onto an
  RTS thread/region per the threading model) each get their own independent
  `diagnostics_channel` registry — no promotion-on-publication to the shared heap is
  ever needed for this module, because nothing about it is meant to be visible
  cross-thread.
- The `'worker_threads'` built-in channel event fires on the **parent** thread's own
  registry when it creates a new Worker — this is a same-thread publish from the
  Worker-constructor's native code (via the native bridge in §5.2), not a cross-thread
  message.
- If a user wants cross-thread diagnostics correlation, they must forward messages
  themselves via a `MessagePort`/`SharedArrayBuffer` — entirely out of scope for this
  module, which is intentionally single-realm.
- The two native bridge externs (§5.2) may be called from whichever thread the owning
  native module (net/http/child_process) itself runs on (including a tokio worker for
  an async server) — this module adds **no new** thread-registration requirement beyond
  what that calling module already must satisfy (i.e. the calling thread must already
  be registered in `thread_registry` for the GC scanner, same as every other native
  caller of a `Function` value).

### 5.5 Buffer / TypedArray interop

Not applicable to the module's own logic — `message`/`context` are always passed as
opaque `any` object references that stay in the same JS heap for the whole
subscribe→publish→handler round trip; there is no byte-level ABI crossing for typical
usage. The only place raw bytes can appear is **incidentally**, inside a
payload field of a built-in channel event from another module — e.g.
`Http2ClientStreamBodyChunkSentEvent.data` may be a `Buffer`. Such fields are built by
the *owning* native module using RTS's standard TypedArray/`Uint8Array`-backed `Buffer`
representation (already primordial `ArrayBuffer`/ `TypedArray` per the engine's memory
model) exactly as that module already does elsewhere (e.g. `fs.readFileSync`'s return
value) — `diagnostics_channel` itself defines no new buffer-crossing mechanism.

### 5.6 Doctrine placement

`node:diagnostics_channel` is **non-primordial** — it has no native syntax, is reached
only through an explicit `node:` import, and the engine must never hardcode its name.
Resolution is 100% data-driven through rts-node's existing tables:
`ns_prefix_for("node:diagnostics_channel")` → `"node_diagnostics_channel"` via a new
`diagnostics_channel::SPEC: NodespaceSpec` entry added to `NODE_SPECS` in
`crates/rts-node/src/lib.rs`, and `node_lookup("node_diagnostics_channel.<name>")` for
the two native members in §5.2. The bulk of the surface (`Channel`, `TracingChannel`,
the 5 top-level functions) is a **`.ts` shim** rts-node ships and the module resolver
maps the `node:diagnostics_channel` specifier onto directly — the native member table
for this module is deliberately almost empty (2 members) because the feature itself
needs almost no native primitive, matching the "no builtins in the engine, minimal
native surface, JS-shaped ergonomics in `.ts`" design rule.

### 5.7 Shared-infra dependencies (FLAG)

- **`node:async_hooks` / `AsyncLocalStorage` — does not exist anywhere in RTS yet.**
  `Channel.bindStore`/`unbindStore`/`runStores` are entirely unimplementable without
  it. This is not "hoist an existing rts-std module" — it is a **new capability** that
  must be designed and land first (continuation-local storage across the RTS async
  model, which per `.claude/rules/03-features.md` is currently *interim synchronous*).
  Core pub/sub (everything except these 3 methods) can ship and be fully useful without
  it.
- **Promise subsystem** (`promise.create`/`.then`/`.wait`, currently living under
  `rts-std::runtime::async_rt`/`promise` per the project's crate partition) — needed
  conceptually by `tracePromise`, but since rts-node cannot depend on rts-std, the
  actual implementation must go through the **engine-level** Promise primordial
  surface (plain `.then()` JS syntax in the `.ts` shim), not a direct Rust dependency on
  `rts-std`'s promise module. No new hoist is required *if* Promise's `.then` is already
  reachable as ordinary TS syntax from any `.ts` shim (it is, per the primordial
  doctrine) — flagged here only so a future native (non-`.ts`) fast path for
  `tracePromise` is known to need this infra shared/hoisted if ever attempted.
  Instrumentation note: it is exactly this Promise surface that a native
  `node:http`/`node:net` server implementation would use if it wanted to publish
  `http.server.request.start` etc. from a tokio task — see §5.4.
- **Shared tokio runtime** (`rts-std::runtime::async_rt::rt()`) — not needed by this
  module's own logic. Only indirectly relevant through §5.4: whichever *other* native
  rts-node module eventually calls the `PUBLISH_FROM_NATIVE` bridge from a tokio worker
  thread inherits whatever thread-registration requirement it already has to solve for
  its own correctness; this module adds no new requirement on top of that.
- No fs/net/crypto primitives are needed by `diagnostics_channel` itself.

### 5.8 Implementation phases

a. **Core `.ts` shim, zero native surface.** `crates/rts-node/ts/diagnostics_channel.ts`
   with a module-scope `Map<string | symbol, ChannelImpl>` registry, the `Channel`
   class (`name`, `hasSubscribers` getter, `publish`, `subscribe`, `unsubscribe`), and
   the 4 top-level functions `channel`/`hasSubscribers`/`subscribe`/`unsubscribe`.
   Register `"node:diagnostics_channel"` in `NODE_SPECS` with an **empty** native
   member table to start (pure `.ts`, no externs yet).
b. **`TracingChannel` + `tracingChannel()`.** 5 sub-channels, `subscribe`/
   `unsubscribe` bulk methods, `traceSync` (try/catch composition), `tracePromise`
   (`.then`/`.catch` composition), `traceCallback` (callback-wrapping composition) —
   all still pure `.ts`, no native change.
c. **Native bridge externs.** Add
   `__RTS_FN_NODE_DIAGNOSTICS_CHANNEL_HAS_SUBSCRIBERS_NATIVE` and
   `__RTS_FN_NODE_DIAGNOSTICS_CHANNEL_PUBLISH_FROM_NATIVE` (§5.2), wire them to the same
   TS-side registry via the existing "invoke a kept-alive `Function`/`Entry::Function`"
   trampoline pattern already used for timers/promise settlement. This unblocks other
   native rts-node modules from publishing built-in instrumentation events later — it
   is not required for `diagnostics_channel` to be considered feature-complete on its
   own.
d. **First built-in-channel producer, end to end.** Once `node:net` exists, wire
   `net.client.socket`/`net.server.socket` (simplest 1-field payloads) as a concrete
   proof the native bridge round-trips correctly.
e. **`bindStore`/`unbindStore`/`runStores`.** Gated entirely on `node:async_hooks`
   landing with a real `AsyncLocalStorage`; implement against it once available.
f. **Backfill remaining built-in channels** (console, http, http2, module, process/
   child_process, udp, worker_threads, Web Locks) opportunistically as each *owning*
   node module (`node:console` integration point, `node:http`, `node:http2`,
   `node:module`, `node:child_process`, `node:dgram`, `node:worker_threads`) is itself
   implemented — tracked as follow-up work per owning module, not a blocker for this
   module's completeness.

## 6. Test plan

- **Basic roundtrip.** `subscribe("x", fn)`; `channel("x").publish({a:1})`; assert `fn`
  received `({a:1}, "x")`.
- **Multiple subscribers.** Subscribe 3 handlers to the same channel; publish once;
  assert all 3 ran, in subscription order.
- **Unsubscribe correctness.** `unsubscribe("x", fn)` returns `true` once, `false` on a
  second call with the same (now-removed) handler; remaining subscribers still fire.
- **Symbol channel isolation.** A `Symbol("x")`-named channel and a `"x"`-string-named
  channel are independent — subscribing to one never receives publishes on the other.
- **`hasSubscribers` transitions.** Assert `false` before any `subscribe`, `true`
  immediately after, `false` again immediately after the matching `unsubscribe` — check
  both the module-level `diagnostics_channel.hasSubscribers("x")` and the
  `channel("x").hasSubscribers` getter agree at each step.
- **`channel()` never null / publish with zero subscribers is a no-op.** Call
  `channel("never-subscribed").publish("msg")` and assert no throw, no observable
  effect.
- **Throwing subscriber does not block siblings.** Subscribe `fnA` (throws), `fnB`
  (records a call); publish once; assert `fnB` still ran. (Exact RTS error-surfacing
  behavior — mirror Node's deferred-`uncaughtException` or something simpler — is an
  open question, see §7; the "siblings still run" invariant is not.)
- **`TracingChannel.traceSync` happy path.** `tc.traceSync(() => 42, {})` → returns
  `42`; `start`/`end` both fire; `end` event has `result === 42`.
- **`TracingChannel.traceSync` throwing fn.** `tc.traceSync(() => { throw new Error("e") })`
  → rethrows; `error` fires with `.error` set; `end` still fires after `error`.
- **`TracingChannel.tracePromise` happy path.** Resolves; `asyncStart`/`asyncEnd` fire
  with `.result`; `start`/`end` fire around the synchronous call.
- **`TracingChannel.tracePromise` rejection.** Rejects; `asyncEnd`/`error` fire with
  `.error`; the returned promise itself still rejects with the original reason.
- **`TracingChannel.traceCallback` happy + error-first convention.** Callback invoked
  as `(err, result)`; assert `asyncStart`/`asyncEnd` see `result` when `err` is
  falsy, and `error` fires when `err` is truthy. Also test a non-default `position`
  argument (callback not in the last argument slot).
- **`tracingChannel(nameOrChannels)` with a pre-built `TracingChannelCollection`.**
  Pass 5 manually-created `Channel`s; assert `traceSync` publishes on exactly those,
  not on any auto-derived `tracing:${name}:*` name.
- **Multithread isolation (guarded until `node:worker_threads` lands).** Subscribe on
  the main thread; spawn a Worker that publishes on a same-named channel inside itself;
  assert the main-thread subscriber does **not** receive it (separate registries per
  §5.4). Conversely, a `'worker_threads'` subscription on the main thread **does** fire
  when the Worker is created.
- **`bindStore`/`runStores` correlation (deferred, guarded until `node:async_hooks`
  lands).** Bind an `AsyncLocalStorage`; run `runStores(ctx, fn)`; inside `fn`, assert
  `store.getStore()` reflects `ctx` (or the transformed value if a `transform` was
  passed to `bindStore`).
- **Perf-sanity fast path.** `if (channel.hasSubscribers) channel.publish(buildExpensiveMessage())`
  pattern: assert `buildExpensiveMessage` is never called when there are zero
  subscribers (side-effect counter, since real allocation-avoidance isn't directly
  assertable from a `.test.ts` fixture).

## 7. Open questions / deferrals

- **`node:async_hooks` / `AsyncLocalStorage` does not exist in RTS at all.**
  `bindStore`/`unbindStore`/`runStores` are fully blocked on it; core pub/sub is not.
  Needs its own design spec before this module can claim 100% parity.
- **Exact subscriber-throw error-surfacing strategy.** Real Node defers to
  `process.nextTick()` + `'uncaughtException'`. RTS's `process`-level uncaught-exception
  story is not settled in this doc's scope — owner decision needed on whether to mirror
  Node exactly (defer + still crash the process by default) or something simpler (e.g.
  synchronous catch-and-log) for the interim; either way the "other subscribers still
  run" invariant must hold.
- **`WeakReference`/`FinalizationRegistry`-based automatic channel GC** is a pure memory
  optimization in real Node, not user-observable correctness. RTS's `WeakRef` is
  currently strong-ref interim (per `.claude/rules` — real weak semantics land with the
  GC weak phase, tracked issue #217). Recommendation: implement the registry with plain
  strong references for now (channel count is inherently small/bounded in practice);
  revisit once #217 lands if channel churn ever becomes a measured leak.
- **Wiring the ~30 concrete built-in instrumentation channels** (console/http/http2/
  net/module/process/udp/worker_threads/Web Locks) is gated on each *owning* node
  module existing first (`node:http`, `node:net`, `node:http2`, `node:child_process`,
  `node:dgram`, `node:worker_threads`, `node:module`) — tracked as N separate per-module
  follow-ups, not a blocker for `diagnostics_channel` itself being "done".
- **Web Locks channels (`locks.request.*`, added v25.9.0)** additionally depend on a Web
  Locks API (`navigator.locks`) that does not exist in RTS at all (browser-oriented
  API, no current RTS equivalent or plan) — very low priority, likely deferred
  indefinitely.
- **Cross-realm / `vm.Context` sharing.** Real Node's behavior here is subtle and not
  fully explored in this spec; RTS has no `node:vm` module yet, so this is moot until
  that module exists.
- **Bare specifier `"diagnostics_channel"` (no `node:` prefix).** Valid in real Node,
  not currently resolved by RTS's `ns_prefix_for` (module-resolver-level gap, not
  specific to this module) — noted, not solved, here.
