# node:events

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:events` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import { EventEmitter } from "node:events"`; `import EventEmitter from "node:events"` (default export, same class); `const { EventEmitter } = require("node:events")`; `import { EventTarget, Event, CustomEvent, EventEmitterAsyncResource } from "node:events"` |
| Globals exposed | `EventTarget`, `Event`, `CustomEvent` are WHATWG globals available on `globalThis` with **no import** (Node exposes them since v15); `node:events` re-exports the *same* classes rather than defining new ones |

## 1. Purpose

`node:events` is the foundation of Node's asynchronous, callback/event-driven core API: it provides the classic `EventEmitter` (used pervasively by `fs.ReadStream`, `net.Socket`, `http.Server`, `child_process`, etc.) and re-exports the WHATWG `EventTarget`/`Event`/`CustomEvent` globals that RTS also exposes as ambient browser-compatible globals. It additionally provides module-level helpers (`events.on`, `events.once`, `events.getEventListeners`, …) that let any `EventEmitter` or `EventTarget` be consumed with `async`/`await` and iteration protocols instead of manual listener wiring. Almost every other Node core module (`fs`, `net`, `http`, `child_process`, `stream`, `dgram`, `readline`, `worker_threads`) depends on `EventEmitter` as a base class, so this module is a P0 prerequisite for the rest of the `node:*` surface.

## 2. Exported API surface (COMPLETE)

### Classes

#### `class EventEmitter`

Base class. No native syntax; a plain constructible class.

**Constructor**

```ts
new EventEmitter(options?: EventEmitterOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `EventEmitterOptions` | yes | `{}` |

Throws: none.

**Static properties** (attached to the `EventEmitter` class itself; `require("node:events")` returns this class, so `events.X` and `EventEmitter.X` are the same binding)

| Property | Type | Default | Notes |
|---|---|---|---|
| `EventEmitter.defaultMaxListeners` | `number` | `10` | Global default for `getMaxListeners()`/warning threshold on every emitter that hasn't called `setMaxListeners()`. Mutating it retroactively affects existing instances. Throws `RangeError` if set to a non-positive number. |
| `EventEmitter.errorMonitor` | `symbol` | `Symbol.for('nodejs.errorMonitor')` | Install a listener under this symbol to *monitor* `'error'` events without consuming them (does not prevent the crash-if-unhandled behavior). |
| `EventEmitter.captureRejections` | `boolean` | `false` | Global default for the `captureRejections` constructor option on all *newly created* `EventEmitter`/subclass instances. |
| `EventEmitter.captureRejectionSymbol` | `symbol` | `Symbol.for('nodejs.rejection')` | Symbol key an emitter can implement (`emitter[captureRejectionSymbol] = (err, eventName, ...args) => {}`) as a custom rejection handler. |
| `EventEmitter.EventEmitter` | `class` | — | Self-reference (`EventEmitter.EventEmitter === EventEmitter`). |
| `EventEmitter.EventEmitterAsyncResource` | `class` | — | Re-export, see below. |
| `EventEmitter.EventTarget` | `class` | — | Re-export of the WHATWG `EventTarget` global. |
| `EventEmitter.Event` | `class` | — | Re-export of the WHATWG `Event` global. |
| `EventEmitter.CustomEvent` | `class` | — | Re-export of the WHATWG `CustomEvent` global. |

**Static methods** — identical bindings under both `events.<fn>` (module-level) and `EventEmitter.<fn>`; documented once under "Top-level functions" below (`on`, `once`, `getEventListeners`, `getMaxListeners`, `listenerCount`, `setMaxListeners`, `addAbortListener`).

**Instance methods**

| Signature | Returns | Variant | Notes |
|---|---|---|---|
| `emitter.on(eventName: string \| symbol, listener: (...args: any[]) => void): this` | `EventEmitter` | sync | Appends listener. Same `(eventName, listener)` pair may be added more than once → invoked that many times. |
| `emitter.addListener(eventName, listener): this` | `EventEmitter` | sync | Alias of `.on()`. |
| `emitter.once(eventName: string \| symbol, listener: (...args: any[]) => void): this` | `EventEmitter` | sync | Listener auto-removed after first invocation, invoked before removal side effects are visible to `.listenerCount()`. |
| `emitter.off(eventName, listener): this` | `EventEmitter` | sync | Alias of `.removeListener()`. |
| `emitter.removeListener(eventName: string \| symbol, listener: Function): this` | `EventEmitter` | sync | Removes **at most one** instance of `listener`; call N times to remove N registrations. Emits `'removeListener'` (unless the emitter is mid-`removeAllListeners`). |
| `emitter.removeAllListeners(eventName?: string \| symbol): this` | `EventEmitter` | sync | Omit `eventName` → removes listeners for *every* event. Emits `'removeListener'` once per removed listener (best practice: avoid removing listeners added elsewhere). |
| `emitter.emit(eventName: string \| symbol, ...args: any[]): boolean` | `boolean` | sync | `true` iff the event had ≥1 listener. Listeners invoked synchronously, in registration order, with `this` bound to the emitter (for non-arrow listeners). |
| `emitter.listenerCount(eventName: string \| symbol, listener?: Function): number` | `number` | sync | With `listener`, counts how many times that exact function is registered. |
| `emitter.listeners(eventName: string \| symbol): Function[]` | `Function[]` | sync | Copy of the array; for `.once()` listeners returns the **original** function (not the internal wrapper). |
| `emitter.rawListeners(eventName: string \| symbol): Function[]` | `Function[]` | sync | Like `.listeners()` but returns the internal once-wrapper (has a `.listener` property pointing at the original). |
| `emitter.eventNames(): Array<string \| symbol>` | `(string\|symbol)[]` | sync | Names with ≥1 registered listener. |
| `emitter.getMaxListeners(): number` | `number` | sync | Effective cap: explicit `setMaxListeners()` value, else `EventEmitter.defaultMaxListeners`. |
| `emitter.setMaxListeners(n: number): this` | `EventEmitter` | sync | `n = 0` or `Infinity` disables the warning for this instance. Throws `RangeError` if `n` is negative or `NaN`. |
| `emitter.prependListener(eventName: string \| symbol, listener: Function): this` | `EventEmitter` | sync | Same as `.on()` but unshifts to the front of the listener array. |
| `emitter.prependOnceListener(eventName: string \| symbol, listener: Function): this` | `EventEmitter` | sync | `.once()` + prepend. |
| `emitter[Symbol.for('nodejs.rejection')](err: Error, eventName: string \| symbol, ...args: any[]): void` | `void` | sync (called from a promise-rejection microtask) | Optional override; called instead of emitting `'error'` when `captureRejections` is enabled and a listener's returned Promise rejects. |

**Events emitted by `EventEmitter` itself (meta-events)**

| Event | Args | Notes |
|---|---|---|
| `'newListener'` | `(eventName: string \| symbol, listener: Function)` | Emitted **before** the listener is actually appended — a listener registered for `'newListener'` inside a `'newListener'` handler will fire for its own registration too (careful with recursion). |
| `'removeListener'` | `(eventName: string \| symbol, listener: Function)` | Emitted **after** removal. For a removed `.once()` listener, `listener` is the original function, not the wrapper. |
| `'error'` | `(err: Error, ...)` | Special-cased: if there is no `'error'` listener when `'error'` is emitted, the error is thrown, printed, and the process exits. `EventEmitter.errorMonitor` listeners run first and do not suppress this. |

#### `class EventEmitterAsyncResource extends EventEmitter`

```ts
new EventEmitterAsyncResource(options?: EventEmitterAsyncResourceOptions)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `options` | `EventEmitterAsyncResourceOptions` | yes | `{}` |

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `eventEmitterAsyncResource.asyncId` | `number` | Unique id assigned to the underlying `AsyncResource`. |
| `eventEmitterAsyncResource.triggerAsyncId` | `number` | The `triggerAsyncId` passed (or defaulted) at construction. |
| `eventEmitterAsyncResource.asyncResource` | `AsyncResource` | The underlying resource; has an extra `.eventEmitter` back-reference. |

**Instance methods**

| Signature | Returns | Notes |
|---|---|---|
| `eventEmitterAsyncResource.emitDestroy(): void` | `void` | Runs `destroy` hooks. Must be called at most once (throws if called twice); if never called and `requireManualDestroy` is `false` (default), GC finalization triggers it — but relying on GC timing means hooks may never run in practice. |

Inherits every `EventEmitter` instance method unchanged.

#### `class EventTarget`

WHATWG-compatible; **the same class** as the ambient global `EventTarget` (see 5.6).

**Constructor**: `new EventTarget()` — no arguments.

**Instance methods**

| Signature | Returns | Notes |
|---|---|---|
| `eventTarget.addEventListener(type: string, listener: EventListenerLike, options?: AddEventListenerOptions): void` | `void` | A given `(type, listener, capture)` triple is registered at most once (duplicate calls are no-ops). |
| `eventTarget.removeEventListener(type: string, listener: EventListenerLike, options?: RemoveEventListenerOptions): void` | `void` | Only removes if the `capture` option matches what was passed to `addEventListener`. |
| `eventTarget.dispatchEvent(event: Event): boolean` | `boolean` | `false` iff `event.cancelable` is `true` and some listener called `preventDefault()`; listeners run synchronously, in registration order. |

#### `class Event`

Ambient WHATWG global, re-exported.

```ts
new Event(type: string, options?: EventInit)
```

**Instance properties** (all read-only except where noted)

| Property | Type | Notes |
|---|---|---|
| `event.bubbles` | `boolean` | Always `false` in Node (no DOM tree). |
| `event.cancelBubble` | `boolean` (get/set) | Legacy alias: setting `true` calls `stopPropagation()`. |
| `event.cancelable` | `boolean` | From constructor `options.cancelable`, default `false`. |
| `event.composed` | `boolean` | Always `false`. |
| `event.currentTarget` | `EventTarget \| null` | Alias of `.target`. |
| `event.defaultPrevented` | `boolean` | `true` after a successful `preventDefault()` call. |
| `event.eventPhase` | `number` | `0` (not dispatching) or `2` (dispatching); Node has no capture/bubble phases. |
| `event.isTrusted` | `boolean` | `true` only for the engine-generated `AbortSignal` `'abort'` event; `false` otherwise. |
| `event.returnValue` | `boolean` (legacy) | Always the logical opposite of `defaultPrevented`. |
| `event.srcElement` | `EventTarget \| null` (legacy) | Alias of `.target`. |
| `event.target` | `EventTarget \| null` | The dispatching target. |
| `event.timeStamp` | `number` | Milliseconds since creation (`Date.now()`-based). |
| `event.type` | `string` | Event type string, coerced with `String(type)`. |

**Instance methods**

| Signature | Returns | Notes |
|---|---|---|
| `event.composedPath(): EventTarget[]` | array | `[target]` while dispatching, `[]` otherwise. |
| `event.initEvent(type: string, bubbles?: boolean, cancelable?: boolean): void` | `void` | Legacy, discouraged; cannot set `composed`. |
| `event.preventDefault(): void` | `void` | No-op unless `cancelable` is `true`. |
| `event.stopImmediatePropagation(): void` | `void` | Stops remaining listeners for this dispatch from running. |
| `event.stopPropagation(): void` | `void` | No-op (no propagation model in Node). |

#### `class CustomEvent extends Event`

Ambient WHATWG global, re-exported.

```ts
new CustomEvent<T = any>(type: string, options?: CustomEventInit<T>)
```

| Param | Type | Optional | Default |
|---|---|---|---|
| `type` | `string` | no | — |
| `options.detail` | `T` | yes | `null` |
| `options.bubbles`/`cancelable`/`composed` | `boolean` | yes | `false` (inherited from `EventInit`) |

**Instance property**

| Property | Type | Notes |
|---|---|---|
| `event.detail` | `T` | Arbitrary payload passed at construction; read-only. |

#### `class NodeEventTarget` (documented for completeness; Node-internal legacy shim, not a public export)

A reduced `EventEmitter`-shaped API over the DOM-style listener storage, used internally by a few legacy Node core objects. Not exported as `require('node:events').NodeEventTarget`. Key differences from `EventEmitter`: a given `(type, listener)` pair is registered at most once; no `prependListener`/`prependOnceListener`/`rawListeners`/`errorMonitor`; no `'newListener'`/`'removeListener'` meta-events; no special `'error'` handling; listeners may be `EventListener` objects (`{handleEvent(evt)}`) as well as plain functions.

| Signature | Returns |
|---|---|
| `nodeEventTarget.addListener(type: string, listener: Function): NodeEventTarget` | `NodeEventTarget` |
| `nodeEventTarget.emit(type: string, arg: any): boolean` | `boolean` |
| `nodeEventTarget.eventNames(): string[]` | `string[]` |
| `nodeEventTarget.listenerCount(type: string): number` | `number` |
| `nodeEventTarget.setMaxListeners(n: number): NodeEventTarget` | `NodeEventTarget` |
| `nodeEventTarget.getMaxListeners(): number` | `number` |
| `nodeEventTarget.off(type, listener, options?): NodeEventTarget` | `NodeEventTarget` |
| `nodeEventTarget.on(type, listener): NodeEventTarget` | `NodeEventTarget` |
| `nodeEventTarget.once(type, listener): NodeEventTarget` | `NodeEventTarget` |
| `nodeEventTarget.removeAllListeners(type?: string): NodeEventTarget` | `NodeEventTarget` |
| `nodeEventTarget.removeListener(type, listener, options?): NodeEventTarget` | `NodeEventTarget` |

RTS treats `NodeEventTarget` as **out of scope / deferred** (see §7) — no Node core module RTS targets in P0/P1 requires it directly; user code cannot `import` it.

### Top-level functions

Every function below is exported both as `events.<name>` (module-level, ESM named export / CJS `require("node:events").<name>`) **and** as the identical `EventEmitter.<name>` static.

#### `events.on(emitter, eventName, options?)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `emitter` | `EventEmitter` | no | — |
| `eventName` | `string \| symbol` | no | — |
| `options.signal` | `AbortSignal` | yes | — |
| `options.close` | `string[]` | yes | `[]` |
| `options.highWaterMark` | `number` | yes | `Number.MAX_SAFE_INTEGER` |
| `options.lowWaterMark` | `number` | yes | `1` |

Returns: `AsyncIterableIterator<any[]>`. Variant: **async iterator** (consumed via `for await`).
Throws: if `emitter` emits `'error'`, the iterator's next `Promise` rejects with that error and all listeners installed by `on()` are removed; if `options.signal` aborts, iteration ends by throwing the abort reason (`AbortError`).

#### `events.once(emitter, name, options?)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `emitter` | `EventEmitter \| EventTarget` | no | — |
| `name` | `string \| symbol` | no | — |
| `options.signal` | `AbortSignal` | yes | — |

Returns: `Promise<any[]>`. Variant: **promise**.
Throws (rejects): if `emitter` emits `'error'` before `name`; if `options.signal` is already aborted or aborts while waiting (rejects with the signal's abort reason).

#### `events.getEventListeners(emitterOrTarget, eventName)`

| Param | Type | Optional |
|---|---|---|
| `emitterOrTarget` | `EventEmitter \| EventTarget` | no |
| `eventName` | `string \| symbol` | no |

Returns: `Function[]`. Variant: **sync**. For `EventTarget` this is the *only* way to inspect its listeners (no `.listeners()` method on `EventTarget`).

#### `events.getMaxListeners(emitterOrTarget)`

| Param | Type | Optional |
|---|---|---|
| `emitterOrTarget` | `EventEmitter \| EventTarget` | no |

Returns: `number`. Variant: **sync**.

#### `events.listenerCount(emitterOrTarget, eventName)`

| Param | Type | Optional |
|---|---|---|
| `emitterOrTarget` | `EventEmitter \| EventTarget` | no |
| `eventName` | `string \| symbol` | no |

Returns: `number`. Variant: **sync**. (History: accepting `EventTarget` was added/un-deprecated in v25.4.0 — RTS targets that current behavior directly, no deprecated-path needed.)

#### `events.setMaxListeners(n?, ...eventTargets)`

| Param | Type | Optional | Default |
|---|---|---|---|
| `n` | `number` (non-negative) | yes | `EventEmitter.defaultMaxListeners` (10) |
| `...eventTargets` | `Array<EventEmitter \| EventTarget>` | yes | `[]` — if empty, sets the **global** default for all newly created emitters/targets |

Returns: `void`. Variant: **sync**. Throws `RangeError` if `n` is negative.

#### `events.addAbortListener(signal, listener)`

| Param | Type | Optional |
|---|---|---|
| `signal` | `AbortSignal` | no |
| `listener` | `(event: Event) => void` | no |

Returns: `Disposable` (an object with `[Symbol.dispose]()` that removes the listener). Variant: **sync** (registers a one-shot listener that fires when/if `signal` aborts). No-ops immediately (returns a no-op disposable) if `signal.aborted` is already `true`? — **No**: per spec it still fires synchronously via the normal abort-listener path if already aborted; the disposable exists purely for early unsubscription convenience.

### Properties & constants

| Name | Type | Default | Scope |
|---|---|---|---|
| `events.defaultMaxListeners` / `EventEmitter.defaultMaxListeners` | `number` | `10` | module + class static (same binding) |
| `events.errorMonitor` / `EventEmitter.errorMonitor` | `symbol` | `Symbol.for('nodejs.errorMonitor')` | module + class static |
| `events.captureRejections` / `EventEmitter.captureRejections` | `boolean` | `false` | module + class static, mutable global toggle |
| `events.captureRejectionSymbol` / `EventEmitter.captureRejectionSymbol` | `symbol` | `Symbol.for('nodejs.rejection')` | module + class static |

**`MaxListenersExceededWarning`** (not a class instantiated by user code — an internally constructed `Error`-like object passed to `process.emitWarning`, observable via `process.on('warning', ...)`)

| Field | Type |
|---|---|
| `name` | `'MaxListenersExceededWarning'` |
| `emitter` | `EventEmitter` |
| `type` | `string \| symbol` |
| `count` | `number` |

### Events

| Event name | Emitted by | Args | Meaning |
|---|---|---|---|
| `'newListener'` | any `EventEmitter` | `(eventName, listener)` | Fired just before a listener is appended. |
| `'removeListener'` | any `EventEmitter` | `(eventName, listener)` | Fired just after a listener is removed. |
| `'error'` | any `EventEmitter` | `(err, ...)` | Special-cased crash-if-unhandled semantics (see §4). |
| user-defined | any `EventEmitter`/`EventTarget` | whatever `emit()`/`dispatchEvent()` passed | Ordinary application events. |
| `'abort'` | `AbortSignal` (an `EventTarget`) | `(event: Event)` with `isTrusted: true` | Fired once when the signal is aborted; also invokes `signal.onabort`. |

## 3. Types & option objects

```ts
interface EventEmitterOptions {
  /** Enable automatic Promise-rejection capture for listeners that return a Promise. */
  captureRejections?: boolean; // default: false (or EventEmitter.captureRejections if set)
}

interface EventEmitterAsyncResourceOptions extends EventEmitterOptions {
  /** Type name of the async event. Default: constructor's name (`new.target.name`). */
  name?: string;
  /** id of the execution context that created this async event. Default: current executionAsyncId(). */
  triggerAsyncId?: number;
  /** If true, disables automatic emitDestroy-on-GC; caller must call emitDestroy() manually. Default: false. */
  requireManualDestroy?: boolean;
}

interface StaticEventEmitterOptions {
  /** Cancels the async iterator early; its next() rejects with the abort reason. */
  signal?: AbortSignal;
  /** Event names that end iteration cleanly (iterator returns {done: true}) instead of erroring. Default: []. */
  close?: string[];
  /** Max buffered-but-unconsumed events before the emitter is paused. Default: Number.MAX_SAFE_INTEGER. */
  highWaterMark?: number;
  /** Buffered-event count at which the emitter resumes after having been paused. Default: 1. */
  lowWaterMark?: number;
}

interface StaticEventEmitterOnceOptions {
  signal?: AbortSignal;
}

interface AddEventListenerOptions {
  /** Auto-remove after first invocation. Default: false. */
  once?: boolean;
  /** Hint only, not enforced by Node. Default: false. */
  passive?: boolean;
  /** Distinguishes registrations for removeEventListener matching; not otherwise meaningful (no capture phase). Default: false. */
  capture?: boolean;
  /** Removes the listener when signal.abort() is called. */
  signal?: AbortSignal;
}

interface RemoveEventListenerOptions {
  capture?: boolean; // default: false
}

interface EventInit {
  bubbles?: boolean;    // default: false (unused in Node, kept for API completeness)
  cancelable?: boolean; // default: false
  composed?: boolean;   // default: false (unused in Node)
}

interface CustomEventInit<T = any> extends EventInit {
  detail?: T; // default: null
}

/** A listener may be a plain function or an object implementing this. */
interface EventListenerObject {
  handleEvent(evt: Event): void;
}
type EventListenerLike = ((evt: Event) => void) | EventListenerObject;

/** Return type of events.addAbortListener() and any [Symbol.dispose]-bearing disposable. */
interface Disposable {
  [Symbol.dispose](): void;
}

/** Shape passed to process.emitWarning for the max-listeners case. */
interface MaxListenersExceededWarning extends Error {
  name: 'MaxListenersExceededWarning';
  emitter: EventEmitter;
  type: string | symbol;
  count: number;
}
```

## 4. Node semantics & edge cases

- **Registration order & multiplicity.** `on(name, fn)` may be called with the same `(name, fn)` pair repeatedly; each call adds a distinct entry and `fn` is invoked that many times per `emit()`. `removeListener` removes exactly one matching entry per call — removing all duplicates requires calling it in a loop or using `removeAllListeners(name)`.
- **`once()` identity.** `.listeners(name)` unwraps `.once()` registrations back to the original function reference (so `listeners(name).includes(originalFn)` is `true`); `.rawListeners(name)` instead exposes the internal wrapper, which itself carries a `.listener` property pointing at the original — this is what `removeListener`'s `'removeListener'` event argument uses to report the *original* function, not the wrapper.
- **`'error'` is special.** If `emit('error', ...)` runs against an emitter with zero listeners for `'error'` (and zero listeners for `EventEmitter.errorMonitor`, which never counts as satisfying the requirement), the error value is thrown synchronously from `emit()`, Node prints it and the process exits non-zero. `errorMonitor` listeners run *before* the real `'error'` listeners (or before the crash) purely for observability — installing one does not prevent the crash if no real `'error'` listener exists.
- **`MaxListenersExceededWarning`.** Adding more than `getMaxListeners()` (default 10) listeners for one event name emits a process warning (not a thrown error) with `name/emitter/type/count` fields, once per (emitter, event) pair that crosses the threshold. `setMaxListeners(0)` or `setMaxListeners(Infinity)` disables the warning for that instance; the module-level `events.defaultMaxListeners` changes the default retroactively for every emitter that never called `setMaxListeners()` itself. `AbortSignal` instances are exempt from `defaultMaxListeners` but can still have `setMaxListeners(n)` called on them individually via `events.setMaxListeners(n, signal)`.
- **`captureRejections`.** When enabled (per-instance constructor option, or globally via `events.captureRejections = true` / `EventEmitter.captureRejections = true`), every listener invocation whose return value is a thenable gets `.then(undefined, handler)` attached; on rejection, `handler` calls `emitter[captureRejectionSymbol]` if the emitter implements it, else emits `'error'` with the rejection reason. **The resulting `'error'` event itself gets no further rejection-capture wrapping** (this would otherwise infinite-loop) — so a `captureRejections`-triggered `'error'` handler must not be `async` (an async `'error'` handler that itself rejects has nowhere further to go and becomes an unhandled rejection).
- **`this` binding.** A non-arrow listener function's `this` is set to the emitter instance for the duration of that invocation; an arrow-function listener ignores this (its `this` is lexical, from the enclosing scope at definition time). RTS classes/instance methods passed as bound listeners (`obj.method.bind(obj)`) behave like any other function value — `this` inside them is whatever `.bind()` fixed, `emit()`'s attempted `this`-binding on a bound function is a no-op per normal JS bound-function semantics.
- **`events.once()` "missed event" footgun.** Because awaiting a `Promise` yields to the microtask queue, multiple events can be emitted between creating one `events.once()` promise and `await`-ing the next one, silently dropping events. Node's own docs recommend constructing every needed `events.once()`/`events.on()` promise *before* awaiting any of them (`await Promise.all([once(ee,'a'), once(ee,'b')])`) — RTS's docs/tests should reproduce this exact pattern to avoid the same trap in generated code / examples.
- **`events.on()` backpressure.** The async iterator buffers emitted event-arg-arrays internally; once the buffer reaches `highWaterMark`, the emitter is effectively paused (internally, by not exceeding the buffer — Node's real implementation removes/reinstalls the listener at the water marks) until the consumer drains it back down to `lowWaterMark`. An `'error'` during iteration rejects the iterator's current/next `.next()` call and discards any still-buffered events; the `close` option's named events make the iterator end cleanly (`{done: true}`) instead of erroring.
- **Node `EventTarget` vs DOM `EventTarget`.** No event hierarchy/tree — `dispatchEvent` never bubbles or captures across objects; `bubbles`/`composed`/`eventPhase`/`stopPropagation()` exist only for API-shape completeness and are inert. If a listener throws, or is `async` and its returned Promise rejects, the failure is *not* swallowed silently: it is (currently) forwarded to `process.on('error')` before `process.on('uncaughtException')` — Node's own docs flag this exact routing as **deprecated, subject to change** in a future release; RTS should track upstream's eventual behavior change rather than hard-code today's routing as permanent.
- **No platform (OS) differences.** This module is pure ECMAScript-level state machinery; there is no Windows-vs-POSIX distinction, no file descriptors, no errno codes. The only "platform" dependency is the RTS engine's own event loop / microtask draining (relevant to `events.on`/`events.once`/`captureRejections`), not the OS.
- **Security notes.** None specific to this module beyond the general advice that unbounded listener registration is a memory-leak vector (hence the `MaxListenersExceededWarning`); `node:events` performs no I/O, no deserialization of untrusted data, and is not subject to the Node permission model (`--permission`) since it touches no filesystem/network/process resource.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:events` is almost entirely representable as pure `.ts` state machines over already-primordial engine values — arrays, plain objects, `Function` values (regular PolyValue callables, called directly with `cb(...args)`, never a raw code-pointer transmute), `Promise`, `Symbol`, and `Date.now()`. **No Rust std module backs this module's core logic** — there is no filesystem, socket, or OS surface for `EventEmitter`/`EventTarget`/`Event`/`CustomEvent` to wrap.

Concretely:

- `EventEmitter` — a `.ts` class holding parallel arrays keyed by event name (`Map<string|symbol, Array<{fn, once}>>` implemented over the already-available `Map`/`Set` `.ts` stdlib primitives, or plain arrays as `rts-shared/src/stdlib/events.ts`'s existing `EventTarget` does). `emit()` snapshots the listener array before iterating (same "snapshot-before-dispatch" discipline the legacy `rts-std` native emitter already uses, so a listener calling `off()` mid-dispatch cannot corrupt the in-flight iteration) and calls each listener directly as a JS value.
- `EventTarget`/`Event`/`CustomEvent` — **do not reimplement**: RTS already ships a spec-shaped `EventTarget`/`Event` pair as part of the ambient Web-IDL globals in `rts-shared/src/stdlib/events.ts` (consumed today by `AbortSignal`/`AbortController`/`MessagePort`). `CustomEvent` is currently **missing** from that file (flagged in §5.7) and must be added there — `node:events`' shim re-exports the ambient globals rather than defining parallel classes, exactly matching real Node's behavior of vending the *same* `EventTarget`/`Event` for both `globalThis` and `require('node:events')`.
- `EventEmitterAsyncResource` — a `.ts` subclass of the `.ts` `EventEmitter` that additionally tracks a monotonically increasing numeric `asyncId`/`triggerAsyncId` pair (no real `async_hooks` propagation exists in RTS yet — see §5.3/§5.7) and exposes `emitDestroy()` as a plain method with a "called-twice throws" guard.
- `captureRejections` — implemented purely at the `.ts` level: after invoking a listener, `typeof result?.then === "function"` duck-types a thenable and attaches `.then(undefined, handler)` (all primordial `Promise` operations, no native call).
- `events.on()`/`events.once()` — implemented in `.ts` using `new Promise((resolve, reject) => {...})` plus `emitter.once(...)`/`emitter.on(...)` internally; `events.on()`'s async-iterator shape is a hand-written object implementing `[Symbol.asyncIterator]()` that resolves/queues via an internal pending-resolvers array (the same pattern already used for other RTS async generator desugaring), not a native primitive.
- **Contrast with the legacy `rts-std/src/events/mod.rs` namespace.** That module is a raw-function-pointer, fixed-arity (`emit0`/`emit1`) handle-based `EventEmitter` built for the *old* engine's callback model (`func_addr` transmuted to `extern "C" fn()`); its own doc comment already notes it is incompatible with the new engine's boxed `TAG_FUNCTION` `PolyValue` callables. `node:events` must **not** port or wrap that namespace — it is a clean-room `.ts` implementation using ordinary JS function calls, consistent with the `rts-shared/src/stdlib/*.ts` "no native syntax ⇒ .ts stdlib" doctrine bucket that `Map`/`Set`/`EventTarget` already occupy. That legacy namespace is a drain/deletion target independent of this work (tracked in the `rts-std` de-duplication effort, not part of this spec).

### 5.2 ABI surface

**Zero new `extern "C"` symbols and zero new `Handle` variants are required** for correctness — every operation reduces to JS-level array/object/`Function`/`Promise` manipulation the engine already lowers natively (shapes + data ICs for property/method access, the existing `Promise`/microtask primordial for async).

- `NodespaceSpec` entry (mirrors the existing `fs`/`os`/`process` pattern in `crates/rts-node/src/lib.rs`):

  ```rust
  pub const SPEC: NodespaceSpec = NodespaceSpec {
      node_module: "events",
      ns_prefix: "node_events",
      members: &[], // no native members — see §5.1
  };
  ```

  An empty `members` slice is intentional and sufficient: `node_lookup("node_events.X")` will simply never resolve (correct — there is no native `X`), while `ns_prefix_for("node:events")` still returns `Some("node_events")`, which is all the module resolver needs to mount the bundled `.ts` shim (`crates/rts-node/src/events/events.ts`) under the `"node:events"` specifier instead of 404ing on an unrecognized import. This is analogous to how `console`/`json`/`map_set` mount today via `e.include` of a `.ts` prelude, except keyed off an explicit `node:` specifier rather than an always-on ambient global.
- **Handles:** none. `EventEmitter`/`EventTarget`/`Event`/`CustomEvent` instances are ordinary GC'd JS objects with a shape (per the engine's hidden-class model) — they never need to leave the JS value world to reach a `HandleTable` slot, because there is no Rust-side resource (file descriptor, socket, cipher context) backing them.
- **`.ts` shim vs native extern split:** 100% `.ts` shim (`crates/rts-node/src/events/events.ts`, bundled/compiled the same way other `rts-node` `.ts` sources are), 0% native extern. This is a deliberate, load-bearing example that not every `node:*` P0 module needs new ABI surface — the module's entire job is JS-shape ergonomics over primitives (`Function`, `Promise`, `Array`) the engine already owns.
- **Future optimization (non-blocking, backlog only):** if a profiling pass later shows `emit()` is hot in a tight loop (e.g. a high-frequency `net`/`stream` internal `emit('data', chunk)`), the engine's existing shape-keyed data-IC method dispatch already gives `.ts`-implemented `emit()` a fast monomorphic call path for free — no new intrinsic is proposed here since the generic mechanism already covers it.

### 5.3 Async model

- `emitter.emit()` is always **fully synchronous** — no tokio, no event-loop hop. This matches Node's own guarantee that listeners run in-order, synchronously, on the calling stack.
- `captureRejections` only *reacts* to an already-existing `Promise` a listener happened to return; it adds a `.then(undefined, handler)` at the JS level. The actual scheduling of that `.then` callback is the engine's existing primordial microtask queue — no tokio/`spawn_blocking` involved.
- `events.once(emitter, name, options)` → **promise** variant; `events.on(emitter, name, options)` → **async iterator** variant. Both are implemented as ordinary `.ts` `Promise`/async-generator machinery (single-threaded, cooperative), consistent with `docs/specs/async-promise-function.md`'s Promise-centric model — neither needs the shared tokio runtime, since there is no blocking I/O anywhere in this module.
- `events.addAbortListener`/`AbortSignal` integration reuses the already-implemented ambient `AbortSignal`/`AbortController` (`rts-shared/src/stdlib/events.ts`) — an abort is just another synchronous `EventTarget` dispatch, not a cross-thread signal.
- `EventEmitterAsyncResource`'s `asyncId`/`triggerAsyncId` bookkeeping is **not** wired to a real async-context-propagation mechanism (RTS has no `async_hooks` implementation yet); see §5.7/§7 — ships as an inert numeric counter for API-shape compatibility only.

### 5.4 Multithread / worker interaction

- An `EventEmitter`/`EventTarget`/`Event`/`CustomEvent` instance is **ordinary per-thread-region heap data** under `docs/specs/rts-threading-model.md` — nothing in this module needs `shared`/promotion-on-publication semantics, because Node itself never shares a live `EventEmitter` object across `worker_threads` (there is no such API — you cross a real OS-thread boundary via `MessageChannel`/`MessagePort`, transferring/copying data, never by handing a listener array to another thread). RTS should preserve exactly this restriction: an `EventEmitter` created in one thread's region stays `threadLocal` by construction; nothing in `node:events` requires the shared heap.
- `MessageChannel`/`MessagePort` (already implemented as ambient `.ts` in `rts-shared/src/stdlib/events.ts`, using `queueMicrotask` today for same-thread delivery) is the correct future integration point for real cross-`worker_threads` delivery once `worker_threads` maps a `Worker` onto a real RTS thread/region (per the threading model doc) — `MessagePort.postMessage` would then need to enqueue onto the *target* thread's channel rather than `queueMicrotask` on the current thread. That upgrade belongs to the `node:worker_threads` module's spec, not this one; `node:events` itself has no direct multithread surface.
- `events.addAbortListener`/`AbortSignal` are likewise per-thread; RTS does not need this module to support a signal aborted on one thread waking listeners registered on another.

### 5.5 Buffer / TypedArray interop

None. `node:events` carries no byte-level payloads — `emit()`/`dispatchEvent()` arguments and `CustomEvent.detail` are ordinary JS values (`any`), passed by reference like any other object/array/primitive. If a caller happens to `emit('data', someBuffer)`, the `Buffer`/`TypedArray` instance flows through as an ordinary object argument — no ABI marshalling is needed since nothing in this module crosses the `extern "C"` boundary at all (see §5.2).

### 5.6 Doctrine placement

- `node:events` is **entirely non-primordial**: `EventEmitter`, `EventTarget`, `Event`, `CustomEvent`, `EventEmitterAsyncResource` have no native literal/syntactic form (no `emitter` literal, no `on foo { }` syntax) — the engine must never hardcode any of these class names. This is Tier P0 by *ecosystem importance* (nearly every other Node core module extends `EventEmitter`), not by doctrine tier (doctrine-wise it sits squarely in the Registry/`.ts`-shim bucket, same as `Map`/`Set`/console).
- **Resolution path:** `import { EventEmitter } from "node:events"` → specifier stripped of the `"node:"` prefix by the module loader → looked up in `rts-node`'s `NODE_SPECS` table via `ns_prefix_for("node:events")` → `Some("node_events")`, exactly like the existing `fs`/`os`/`process`/`path`/`util`/`crypto` entries in `crates/rts-node/src/lib.rs`. Because `SPEC.members` is empty, the *only* job this resolution does is let the module loader mount `crates/rts-node/src/events/events.ts` under the `"node:events"` specifier (bundled into the program like any other `rts-node`-shipped `.ts` source) — no `node_lookup()` call ever needs to succeed for this module, since there is no native member to look up.
- **Native-extern vs `.ts`-shim split:** 100% `.ts` shim, 0% native extern (§5.1/§5.2). `EventTarget`/`Event`/`CustomEvent` are literal re-exports of the *same ambient global classes* `rts-shared`'s stdlib already vends (owned there per doctrine — "Web-IDL globals with no native syntax" live in `rts-shared/src/stdlib/*.ts`, not `rts-node`). Because this is a **source-level** TypeScript re-export (`export { EventTarget, Event, CustomEvent };` referencing the ambient identifiers already in scope from the injected prelude), it creates **no Rust-level crate dependency** — `rts-node`'s Cargo.toml still depends on nothing but `rts-engine`, honoring the "rts-node cannot depend on rts-shared/rts-std" constraint at the *crate* level, while still sharing the *class identity* at the *JS-value* level (an `instanceof EventTarget` check works identically whether the object came via `node:events` or the bare global, because it is the same underlying class value).

### 5.7 Shared-infra dependencies (FLAG)

- **Promise/microtask draining loop.** `captureRejections`'s internal `.then(undefined, handler)`, and the `Promise`/async-iterator machinery behind `events.once()`/`events.on()`, need the engine's microtask queue to actually drain (i.e., a pending `.then()` callback must eventually run even with no further application code). Today the async/Promise settle plumbing (`promise.create`, `rt.spawn_blocking`, the event-loop drain) lives under `rts-std` (`runtime/async_rt.rs`, `promise` namespace per `docs/specs/async-promise-function.md`). Since `rts-node` cannot depend on `rts-std`, **the microtask/event-loop drain primitive itself must be reachable from a crate `rts-node` (and every other future `node:*` module needing async) can depend on** — i.e., hoisted into `rts-engine` or a new shared low crate beneath the `rts-std` cut line. This module does not need `tokio` (nothing here is blocking I/O), only the microtask *scheduling and draining* mechanism.
- **Ambient global `CustomEvent`.** `rts-shared/src/stdlib/events.ts` currently defines `Event`, `EventTarget`, `AbortSignal`, `AbortController`, `MessagePort`, `MessageChannel` — but **not** `CustomEvent`. It must be added there (WHATWG globals are owned by `rts-shared` per doctrine) before `node:events`' `.ts` shim can re-export it. This is a small, one-class, cross-cutting prerequisite outside `rts-node`'s own tree; flagged here so it isn't lost.
- **`process.emitWarning` (soft dependency on `node:process`).** Delivering `MaxListenersExceededWarning` the way real Node does means routing it to `process.on('warning', ...)`. `node:process` is a sibling `rts-node` module (not `rts-std`), so this is an in-family dependency, not a doctrine violation — but it should be a **soft/lazy** reference (e.g. call through a small indirection that no-ops to `console.warn`/the engine's `io.eprint` if `node:process`'s warning-emitter isn't wired up yet), so `node:events` doesn't hard-block on `node:process`'s implementation order.
- **`async_hooks`-style `triggerAsyncId`/`executionAsyncId` sequencing.** Needed for a fully faithful `EventEmitterAsyncResource`. RTS has no `async_hooks` implementation anywhere yet. Not blocking for P0 (ship the inert numeric-counter stub per §5.3), but flagged as a future shared-infra need if/when `node:async_hooks` becomes its own module.
- **No dependency** on `fs`, `net`, `tls`, `crypto`, or the shared `tokio` runtime for this module — explicitly noting "none" per the task's request so a later implementer doesn't go looking for one.

### 5.8 Implementation phases

(a) Add `CustomEvent` to `rts-shared/src/stdlib/events.ts` (ambient global prerequisite, small standalone patch: `class CustomEvent extends Event { detail: any; constructor(type, opts) { super(type, opts); this.detail = opts?.detail ?? null; } }`).

(b) Scaffold `crates/rts-node/src/events/mod.rs` with an empty `MEMBERS: &[NodespaceMember] = &[]` and `SPEC` (`node_module: "events"`, `ns_prefix: "node_events"`), registered in `NODE_SPECS` in `crates/rts-node/src/lib.rs` — makes `import ... from "node:events"` resolve at all.

(c) Write `crates/rts-node/src/events/events.ts`: the `.ts` `EventEmitter` class (on/once/off/addListener/removeListener/removeAllListeners/emit/listenerCount/listeners/rawListeners/eventNames/getMaxListeners/setMaxListeners/prependListener/prependOnceListener), plus the `'newListener'`/`'removeListener'` meta-events and the special-cased `'error'`-with-no-listener throw.

(d) Wire the module loader to mount `events.ts`'s exports under the `"node:events"` specifier (same mechanism already used for other `rts-node` `.ts`-shipped modules), and re-export `EventTarget`, `Event`, `CustomEvent` from the ambient globals.

(e) Add `EventEmitter.defaultMaxListeners`/`errorMonitor`/`captureRejections`/`captureRejectionSymbol` statics + the `MaxListenersExceededWarning` construction-and-delivery path (soft-wired to `node:process`'s warning emitter per §5.7).

(f) Add `captureRejections` (constructor option + global toggle) with the thenable-duck-type `.then(undefined, handler)` wrapper.

(g) Add the module-level/static helpers: `getEventListeners`, `getMaxListeners`, `listenerCount`, `setMaxListeners` (covering both `EventEmitter` and `EventTarget` receivers).

(h) Add `events.once()` (promise) and `events.on()` (async iterator, with `signal`/`close`/`highWaterMark`/`lowWaterMark` support).

(i) Add `events.addAbortListener()` (disposable wrapper around a one-shot `AbortSignal` listener).

(j) Add `EventEmitterAsyncResource` (subclass + `asyncId`/`triggerAsyncId` stub counter + `emitDestroy()` double-call guard).

(k) Cross-runtime fixtures (see §6) run against Node/Bun/Deno + RTS; wire into the existing cross-runtime harness.

## 6. Test plan

`tests/node-events/*.test.ts` (using the standard `rts:test` `describe`/`test`/`expect` template):

1. **Basic on/emit** — register two listeners for the same event, `emit()` with 0/1/N args, assert both called in registration order with correct args; assert `emit()` return value (`true`/`false` with/without listeners).
2. **`once()` semantics** — listener fires exactly once across multiple `emit()` calls; `listeners(name)` returns the original function (not a wrapper) both before and after removal via `removeListener`.
3. **`off`/`removeListener` aliasing and multiplicity** — add the same `(name, fn)` pair 3×, `emit()` → fn called 3×; `removeListener` once → called 2×; `removeAllListeners(name)` → called 0×.
4. **`prependListener`/`prependOnceListener`** — assert prepended listener runs before previously-registered ones.
5. **`newListener`/`removeListener` meta-events** — assert `'newListener'` fires before the listener is actually invokable (a `'newListener'` handler that itself calls `emit(name)` synchronously should NOT see the not-yet-added listener); assert `'removeListener'` fires after removal with the original (unwrapped) function for a `.once()` registration.
6. **`'error'` with no listener throws / crashes** — nested in a try/catch at the test-harness boundary (avoid actually crashing the test process): assert emitting `'error'` with zero `'error'` listeners throws synchronously from `emit()`. Separate case: an `errorMonitor` listener observes the error but the throw still happens.
7. **`MaxListenersExceededWarning`** — attach 11 listeners for one event with default max (10); assert a warning fires (via a stubbed/observable warning channel) with `count === 11`; assert `setMaxListeners(0)` suppresses it; assert `setMaxListeners(-1)` throws `RangeError`.
8. **`captureRejections`** — construct `new EventEmitter({ captureRejections: true })`; a listener returns a rejecting Promise; assert `'error'` fires (asynchronously, after a microtask turn) with the rejection reason, and that a synchronous throw-in-listener path (no captureRejections) still crashes-if-unhandled as in test 6.
9. **`events.once(emitter, name)`** — resolves with the emitted args array on the *next* matching emit; rejects if `'error'` fires first; the `Promise.all([once(a,'x'), once(a,'y')])` pattern correctly captures both events even when both fire synchronously back-to-back before any `await` resumes (regression test for the "missed event" footgun in §4).
10. **`events.once` with `AbortSignal`** — pass an already-aborted signal → immediate rejection; pass a signal that aborts mid-wait → rejection with the abort reason.
11. **`events.on(emitter, name)` async iterator** — `for await` consumes 3 emitted events in order; breaking out of the loop removes the internal listener (assert `listenerCount(emitter, name) === 0` after the loop); an `'error'` emitted mid-iteration causes the `for await` to throw.
12. **`events.getEventListeners`/`getMaxListeners`/`listenerCount` on both `EventEmitter` and `EventTarget`** — same assertions against both receiver types, confirming the dual-receiver contract.
13. **`EventTarget`/`Event` basics** — `addEventListener`/`removeEventListener`/`dispatchEvent`; a `cancelable: true` event whose listener calls `preventDefault()` makes `dispatchEvent()` return `false`; duplicate `addEventListener(type, sameFn)` calls are no-ops (listener invoked once, not twice); `{ once: true }` option auto-removes.
14. **`CustomEvent`** — `new CustomEvent('x', { detail: {a:1} })` dispatched through an `EventTarget`; listener reads `event.detail` correctly; `event instanceof Event` is `true`.
15. **Ambient-global identity** — `require("node:events").EventTarget === globalThis.EventTarget` (and same for `Event`/`CustomEvent`) — regression test for the doctrine re-export requirement in §5.6 (catches an accidental parallel reimplementation).
16. **`this` binding** — a plain-function listener sees `this === emitter`; an arrow-function listener sees the lexical outer `this` (undefined/module-scope in a test file), not the emitter.
17. **`EventEmitterAsyncResource`** — construct with/without explicit `name`/`triggerAsyncId`; assert `asyncId`/`triggerAsyncId` are numbers; `emitDestroy()` a second time throws.
18. **Multithread smoke test (per §5.4)** — spawn a worker/thread, create an `EventEmitter` inside it, confirm it is NOT visible/usable from the parent thread (no accidental cross-thread sharing); separately, confirm a `MessageChannel`'s `port1`/`port2` DO deliver `postMessage` payloads across the two ends when driven from the same thread (existing `MessagePort` behavior) as the baseline this module intentionally does not change.
19. **Class inheritance** — a user `class MyEmitter extends EventEmitter { ... }` works with `super()` and all inherited methods; combine with a `for...of`-driven bulk `emit()` loop and a `try/catch` around a throwing listener, per the project's "testing creativity" mandate (adjacent-feature combinations, not just happy path).

## 7. Open questions / deferrals

- **`NodeEventTarget`** — documented by Node but not a public export of `node:events`; no RTS core module currently needs it. Deferred indefinitely unless a future `node:*` module RTS implements turns out to depend on its exact reduced-API semantics (Node itself only uses it internally in legacy contexts).
- **`EventEmitterAsyncResource` real `async_hooks` fidelity** — ships as an inert `asyncId`/`triggerAsyncId` counter (§5.3) until/unless RTS gets a real `async_hooks` implementation; `executionAsyncId()`-based defaulting is not implementable without it. Tracked as a follow-up once `node:async_hooks` is scoped.
- **`events.on()` real backpressure (`highWaterMark`/`lowWaterMark`) vs a simplified always-buffer-everything approximation** — the exact "pause by temporarily removing the internal listener" mechanics are an internal Node implementation detail rather than observable API contract in most programs; first pass may implement an unbounded buffer (approximation) and only add true pause/resume if a real workload needs the memory bound. Flag in the PR if the first landed version is the approximation.
- **Deprecated `EventTarget` error-routing behavior (`process.on('error')` before `uncaughtException`)** — Node's own docs mark this "subject to change." RTS should decide whether to match today's routing or jump straight to the eventual (undocumented-timeline) fixed behavior; recommend matching current Node until upstream actually ships the change, then follow.
- **`events.addAbortListener`'s already-aborted-signal behavior** — the fetched documentation does not fully pin down whether the listener fires synchronously-immediately or is skipped when `signal.aborted` is already `true` at call time; verify against Node's actual source/test262-style behavior before finalizing (marked "(verify)" in §2).
- **Performance** — no native fast path is proposed for P0 (see §5.2); revisit only if profiling of a real `node:stream`/`node:net`-heavy workload shows `EventEmitter.emit()` as a hot spot once those modules land on top of this one.
