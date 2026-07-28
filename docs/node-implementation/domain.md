# node:domain

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:domain` |
| Node.js version | 25.x |
| Stability | 0 - Deprecated (pending deprecation; no replacement API finalized yet) |
| Tier | P2 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import domain from "node:domain"`; `import { create, Domain } from "node:domain"`; CJS-style `require("node:domain")` / `require("domain")` (legacy specifier, both map to the same module) |
| Globals exposed | `process.domain` (only populated once `node:domain` has been imported at least once; otherwise `undefined`) |

## 1. Purpose

`node:domain` groups a set of I/O and event-emitting operations so that any error
they raise — whether thrown synchronously, emitted as an `'error'` event, or
thrown inside a bound callback — is routed to a single `'error'` handler instead
of crashing the process via an unhandled exception. It predates
`async_hooks`/`AsyncLocalStorage` and is officially deprecated: Node's own docs
warn it cannot reliably resume normal operation after an error and recommend
process-restart strategies (e.g. via `node:cluster`) instead of "catch and
ignore". RTS implements it for source compatibility with legacy npm packages
that still `require('domain')`, not as a recommended pattern for new RTS code.

## 2. Exported API surface (COMPLETE)

### Classes

#### `class Domain extends EventEmitter`

No public constructor is exposed on the module surface — instances are created
exclusively through `domain.create()`. Internally the class has a normal
constructor (`new Domain()`) which `create()` calls; whether RTS also exports the
class itself as `domain.Domain` is an implementation detail of Node's own
`lib/domain.js` (`exports.Domain = Domain`) and is **not** documented on the
public docs page — **(verify)** against Node 25 source before treating
`domain.Domain` as required surface. If present, it is included below for
completeness.

- Extends: `EventEmitter` (see `node:events`)
- Events: `'error'` (see Events section below) — plus everything inherited from
  `EventEmitter` (`'newListener'`, `'removeListener'`, etc.), since `Domain`
  does not override emitter mechanics beyond adding the `'error'`-routing
  behavior described in this document.

**Instance properties**

| Property | Type | Description |
|---|---|---|
| `members` | `Array<EventEmitter>` | Emitters explicitly bound via `domain.add()`. Since Node v9.3.0 timers are no longer accepted/stored here — only `EventEmitter`-derived objects. Mutating this array directly is not part of the public contract; use `add()`/`remove()`. |

**Instance methods**

##### `domain.run<T extends unknown[]>(fn: (...args: T) => void, ...args: T): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `fn` | `(...args: T) => void` | no | — |
| `...args` | `T` | yes | `[]` |

- Returns: `void`
- Throws: never itself — any synchronous throw from `fn` (or from anything it
  schedules that is implicitly/explicitly bound to this domain) is routed to
  this domain's `'error'` event instead of propagating.
- Variant: **sync** (the call to `run` itself is synchronous; async work
  scheduled inside `fn` is bound per the Async model, §5.3).

Runs `fn` with this domain set as the active domain (`enter()` before, `exit()`
after/on throw). Every `EventEmitter` (and, in real Node, every timer/native
async handle) created during the synchronous extent of `fn` is implicitly bound
to the domain.

##### `domain.add(emitter: EventEmitter): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `emitter` | `EventEmitter` | no | — |

- Returns: `void`
- Throws: none
- Variant: **sync**
- History: Node v9.3.0 — no longer accepts timer objects (EventEmitter only).

Explicitly binds `emitter` to the domain: an `'error'` event it emits, or a
throw from inside one of its listeners, is routed to the domain's `'error'`
event. If `emitter` was already a member of another domain, it is first
removed from that domain (an emitter belongs to at most one domain).

##### `domain.remove(emitter: EventEmitter): void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `emitter` | `EventEmitter` | no | — |

- Returns: `void`
- Throws: none
- Variant: **sync**

The inverse of `add()` — removes domain error-routing from `emitter`.

##### `domain.bind<F extends (...args: any[]) => any>(callback: F): F`

| Param | Type | Optional | Default |
|---|---|---|---|
| `callback` | `F` | no | — |

- Returns: a wrapper function with the same call signature as `callback`
  (`F`, forwarding all arguments and its return value).
- Throws: never itself; wraps `callback` so that a throw *inside* it is routed
  to the domain's `'error'` event rather than propagating to the caller.
- Variant: **sync** wrapper (usable as a callback/promise handler; the wrapper
  itself does not create async work).

Returns a new function that, when called, runs `callback` with this domain
active. Any exception `callback` throws is emitted on the domain instead of
being thrown from the wrapper.

##### `domain.intercept<F extends (...args: any[]) => any>(callback: F): (err: Error | null | undefined, ...rest: Parameters<F>) => ReturnType<F> | void`

| Param | Type | Optional | Default |
|---|---|---|---|
| `callback` | `F` | no | — |

- Returns: a wrapper function whose **first parameter is treated as an error**.
- Throws: never itself; both an `err`-shaped first argument and a throw inside
  `callback` are routed to the domain's `'error'` event.
- Variant: **sync** wrapper, intended for classic Node `(err, data)` callbacks.

Like `bind()`, but if the wrapped function's first argument is a non-null/
non-undefined value it is treated as an `Error` and emitted on the domain
directly — `callback` is *not* invoked in that case. This removes the need for
the caller to write `if (err) return cb(err);` before calling `callback` with
the remaining arguments.

##### `domain.enter(): void`

- Returns: `void`
- Throws: none
- Variant: **sync**

Low-level primitive: pushes this domain onto the internal domain stack and sets
it as `domain.active` / `process.domain`. Idempotent-callable (may be called
multiple times on the same domain, nesting it again). Prefer `run()`/`bind()`/
`intercept()` in application code — `enter()`/`exit()` exist for library authors
implementing their own binding helpers.

##### `domain.exit(): void`

- Returns: `void`
- Throws: none
- Variant: **sync**

Pops this domain (and any domain nested above it that was entered after it) off
the internal domain stack, restoring whichever domain was active before. Must be
paired with a corresponding `enter()` before switching to a different execution
context, or the stack becomes unbalanced.

##### `domain.dispose()` — **REMOVED, do not implement**

Node deprecated `dispose()` in v0.11.7 (marked "DEPRECATED. Please recover from
failed I/O actions explicitly via error event handlers set on the domain
instead.") and it was **removed entirely** by Node v8 — it does not exist in
Node 25 and is absent from the current docs page. Listed here only so a future
implementer does not accidentally resurrect it.

### Top-level functions

##### `domain.create(): Domain`

| Param | Type | Optional | Default |
|---|---|---|---|
| *(none)* | — | — | — |

- Returns: `Domain` — a new, empty (`members = []`) domain instance, not yet
  entered/active.
- Throws: none
- Variant: **sync**

### Properties & constants

| Name | Type | Description |
|---|---|---|
| `domain.active` | `Domain \| undefined` | The currently active domain (top of the internal domain stack), or `undefined` if none is active. Set by `enter()`/`run()`/`bind()`/`intercept()`-invocation and cleared by `exit()`. Read-only from the module's perspective (mutated only via the enter/exit machinery). Mirrored as `process.domain`. |

### Events

#### `'error'` (emitted on a `Domain` instance)

| Listener signature | `(err: Error) => void` |
|---|---|

Emitted when any of the following happens while the domain is active or the
emitter/callback is bound to it:

- a callback wrapped by `bind()`/`intercept()` throws;
- an `EventEmitter` added via `add()` (or implicitly bound during `run()`)
  itself emits `'error'`;
- a listener attached to a bound/added emitter throws while handling any event.

If a domain has **no** `'error'` listener, standard `EventEmitter` semantics
apply: the error is thrown (synchronously, from wherever the emit occurred),
which is equivalent to not having a domain at all for that case — see §4.

## 3. Types & option objects

```ts
/** Extra properties Node (and RTS) attach to an Error once it has been routed
 *  through a domain's error path. Present in addition to the error's own
 *  fields (message, stack, etc). */
interface DomainTaggedError extends Error {
  /** The domain that first handled this error. */
  domain: Domain;
  /** The event emitter that emitted the `'error'` event that led here, if any
   *  (undefined when the error came from a thrown exception inside a bound
   *  callback rather than an `'error'` emit). */
  domainEmitter?: EventEmitter;
  /** The bound/intercepted callback function that was executing when the
   *  error occurred, if any. */
  domainBound?: (...args: unknown[]) => unknown;
  /** true if the error reached the domain via a JS `throw`; false if it
   *  arrived via an `'error'` event emit or a callback's error-first arg. */
  domainThrown: boolean;
}

/** Shape of the wrapper returned by domain.bind(). Generic over the wrapped
 *  function's own signature. */
type DomainBoundFunction<F extends (...args: any[]) => any> = F;

/** Shape of the wrapper returned by domain.intercept(). The wrapped function
 *  F is assumed to have signature (err, ...rest) => R; intercept() strips the
 *  err param from the returned wrapper's perspective (it consumes it). */
type DomainInterceptedFunction<F extends (err: any, ...rest: any[]) => any> =
  (err: Error | null | undefined, ...rest: Parameters<F> extends [any, ...infer R] ? R : never[]) => ReturnType<F> | void;

/** The Domain class itself, extending EventEmitter, as summarized in §2. */
declare class Domain extends EventEmitter {
  members: EventEmitter[];
  run<T extends unknown[]>(fn: (...args: T) => void, ...args: T): void;
  add(emitter: EventEmitter): void;
  remove(emitter: EventEmitter): void;
  bind<F extends (...args: any[]) => any>(callback: F): DomainBoundFunction<F>;
  intercept<F extends (err: any, ...rest: any[]) => any>(callback: F): DomainInterceptedFunction<F>;
  enter(): void;
  exit(): void;
}
```

## 4. Node semantics & edge cases

- **No platform differences.** `node:domain` is pure JS-level control flow; it
  performs no OS/filesystem/network calls and behaves identically on Windows
  and POSIX.
- **Stack discipline.** `enter()`/`exit()` manipulate a genuine stack; calling
  `exit()` without a matching prior `enter()` on that domain is a no-op /
  pops whatever is on top depending on Node's internal bookkeeping — do not
  rely on unbalanced enter/exit sequences. `run()` always balances its own
  enter/exit pair (via try/finally), including when `fn` throws.
- **An emitter belongs to at most one domain.** `add()` transparently removes
  the emitter from any domain it was previously a member of (§2, `add`).
- **Unhandled `'error'` still throws.** A domain with zero `'error'` listeners
  does not swallow anything — standard `EventEmitter` "no listener for
  'error'" behavior applies and the error is (re)thrown, which without further
  handling terminates the process exactly like the no-domain case. Domains are
  not a silent catch-all.
- **Promises are only partly covered.**
  - Since Node v8.0.0, a Promise's `.then()`/`.catch()` handler runs in the
    domain that was active when `.then()`/`.catch()` was *called* (registration
    time), not the domain active when the promise settles.
  - `domain.bind(fn)` can still be used to force a handler to run in a
    *different* domain than the one active at registration time.
  - Since Node v8.8.0, Promises created inside a `vm` context do not carry a
    `.domain` property (main-context promises still do); handler dispatch still
    works correctly regardless.
  - **Domains never emit `'error'` for unhandled promise rejections** — that
    is exclusively `process.on('unhandledRejection')`'s job; domains are
    orthogonal to it.
- **Deprecation status.** Stability 0, "Pending Deprecation" since v1.4.2 —
  Node has not shipped a hard-removal or a finalized replacement; the
  practical recommendation (from Node's own docs and from RTS's own no-legacy
  stance) is `AsyncLocalStorage`/`async_hooks` for new code. RTS treats this
  module as legacy-compat surface, not a pattern to promote.
- **Security / robustness note (verbatim intent of the upstream docs).**
  Using a domain's `'error'` handler purely to swallow errors and keep serving
  requests is called out explicitly as an anti-pattern: after a thrown error,
  the process may be holding leaked resources / half-updated state, and
  "picking up where you left off" is close to impossible in JS. The documented
  recommendation is: log the error, attempt a graceful in-flight response,
  stop accepting new work, and let the process exit (restarting workers via
  `node:cluster` or an external supervisor) rather than trying to keep the
  same process alive indefinitely.
- **Re-entrancy.** `run()` can be called again inside an already-active
  domain's `run()` (nested domains); `enter()` similarly nests. The active
  stack always reflects the innermost still-entered domain.

## 5. RTS implementation notes

### 5.1 Native impl mapping

`node:domain` requires essentially **no new native Rust code**. It is almost
entirely a `.ts` shim (`rts-node`'s `domain/domain.ts`) layered over
capabilities that already exist elsewhere in `rts-node`/the engine:

- `Domain extends EventEmitter` — reuses the `node:events` `EventEmitter`
  implementation (itself either a native-backed primitive namespace already in
  the runtime layer, or the `rts-node` port of `node:events` — whichever lands
  first; `node:domain` only needs it to already exist and be subclassable from
  `.ts`).
- The "current active domain" stack is plain **thread-local module state** in
  the `.ts` shim (`let domainStack: Domain[] = []` at module scope). RTS's
  engine already gives top-level mutable module bindings (gcells)
  thread-local semantics (see project memory: timer/global state is
  thread-local per RTS thread), which happens to be exactly the isolation
  domains need — no dedicated native stack/counter is required.
  the domain stack lives in JS-visible state, not in Rust.
- Error-object tagging (`error.domain` / `error.domainEmitter` /
  `error.domainBound` / `error.domainThrown`) is plain dynamic property
  assignment on an `Error` instance (`Error` is primordial; shapes/ICs already
  support adding arbitrary extra own properties to any object, including
  errors) — no native helper needed.
- `bind()`/`intercept()`/`run()` are ordinary `.ts` closures using `try/catch`
  (primordial control flow) around the wrapped call.
- `process.domain` is a getter `rts-node`'s `node:process` shim exposes,
  delegating to the domain module's active-domain accessor (an
  intra-`rts-node` `.ts`↔`.ts` coupling, not a native/ABI concern).

Net result: `domain/mod.rs` in `rts-node` registers the module in the
nodespace data table (§5.6) with an **empty or near-empty `MEMBERS` list** —
the module exists as a resolvable `node:domain` import target, but the actual
behavior is implemented entirely above the ABI line.

### 5.2 ABI surface

No dedicated `__RTS_FN_NODE_DOMAIN_*` externs are required for the happy path
described in §2 — everything routes through already-primordial mechanisms
(`Error`, closures, `try/catch`, `EventEmitter`). This is a deliberate,
minimal-surface design decision, not an oversight:

| Concern | Mechanism | Native symbol needed? |
|---|---|---|
| Domain instance / `members` array | `.ts` class + `Array` (primordial) | No |
| Active-domain stack | thread-local module `let` (gcell) | No |
| `'error'` routing | `EventEmitter` (`node:events`) `.emit('error', ...)` | No (reuses `node:events`'s own externs, if any) |
| Error tagging | dynamic property set on `Error` instance | No |
| `enter`/`exit`/`run`/`bind`/`intercept` | `.ts` closures + `try/catch` | No |

If a future phase needs a native fast-path (e.g. to avoid the overhead of a
`.ts`-level `Promise.prototype.then` patch for the domain/Promise interaction
in §5.3), the natural addition would be a single opaque `Handle` for "captured
domain reference at Promise-creation time" (`AbiType::Handle`, stored inline in
the Promise's own object shape) plus one accessor:

- `__RTS_FN_NODE_DOMAIN_CAPTURE_ACTIVE() -> Handle` — snapshot the currently
  active domain (or a sentinel "no domain" handle) for later restoration.
- `__RTS_FN_NODE_DOMAIN_RESTORE(handle: Handle) -> Void` — push the captured
  domain back onto the active stack for the duration of a callback invocation.

These two are **not required for the initial implementation** (§5.8 phase (a)/
(b)); they are only listed so the design is not blocked if the `.ts`-only
Promise patch in §5.3 turns out to be infeasible.

### 5.3 Async model

| Source of async work | Binding strategy | Sync / callback / promise |
|---|---|---|
| Synchronous throw inside `run(fn)` | `try/catch` around the call, `exit()` in `finally`, catch routes to `'error'` | sync |
| `domain.bind(cb)` / `domain.intercept(cb)` used explicitly by the calling code (the documented, supported pattern) | wrapper closure enters the domain, invokes `cb`, catches/exits | callback |
| Emitter added via `domain.add(e)`, or created during the synchronous extent of `run()` (implicit binding) | `EventEmitter`'s internal emit path checks for an owning domain and enters it before invoking listeners | callback (via `EventEmitter.emit`) |
| `setTimeout`/`setInterval`/`setImmediate`/`process.nextTick` scheduled inside `run()` | requires the timer/microtask `.ts` shims to capture `domain.active` at scheduling time and wrap the stored callback with `domain.bind()` before handing it to the native timer/microtask queue | callback |
| `Promise.prototype.then`/`.catch`/`.finally` handlers | requires capturing `domain.active` **at registration time** (when `.then()` is called) rather than at settle time, per §4; implemented as a `.ts`-level patch of `Promise.prototype.then` that is installed lazily, only once `node:domain` is first imported (mirrors Node's own lazy-patch approach) | promise |
| Work crossing the shared tokio worker pool (`promise.create`'s `spawn_blocking`, per `docs/specs/async-promise-function.md`) | the domain active at the point `promise.create` is called must be captured and re-entered on the tokio worker thread before invoking the user function, because the active-domain gcell is thread-local and the worker thread is a **different OS thread** — otherwise the callback silently runs with no domain active | promise |

The last row is the one genuine gap relative to real Node (which is
single-threaded end-to-end, so a captured "current domain" trivially survives
across any `setTimeout`/I/O callback without crossing an OS thread boundary).
RTS's promise machinery hops to a tokio worker thread for `spawn_blocking`
work, so **explicit** capture/restore around that hop is required for the
domain to "follow" the async continuation the way Node users expect. See
§5.7 for why this needs shared infra rather than being purely `rts-node`-local,
and §5.8/§7 for phasing.

### 5.4 Multithread / worker interaction

- The active-domain stack is thread-local `.ts` module state (a gcell), which
  matches Node's *effective* semantics: each real Node.js Worker thread
  (`node:worker_threads`) is an independent realm with its own domain state,
  and RTS's per-thread-region model gives the same isolation for free — a
  `Worker` (mapped to an RTS thread/region per `docs/specs/rts-threading-model.md`)
  starts with an empty domain stack, never inheriting the parent thread's
  active domain.
- **Not** thread-safe/shared on purpose: two domains active concurrently on
  two different RTS threads are simply independent; `Domain.members`/its
  event listeners are plain heap objects that could in principle be shared
  across a `SharedArrayBuffer`-style shared region, but nothing in this module
  requires that, and doing so is out of scope.
- The one place isolation actively *hurts* correctness (rather than just being
  irrelevant, as in the Worker case) is the tokio-`spawn_blocking` hop
  used by `promise.create` for `async function` bodies (§5.3, last row) — that
  hop is an RTS **implementation detail** of how a single logical async
  continuation is executed, not a user-visible `Worker`, so losing domain
  context there is a bug relative to Node's model, not a semantic difference
  to document away. It is the one spot requiring explicit capture/restore
  plumbing rather than "just let thread-local isolation do the right thing".

### 5.5 Buffer / TypedArray interop

Not applicable — `node:domain` never touches byte data, buffers, or typed
arrays. No ABI marshalling beyond ordinary object/handle references is needed.

### 5.6 Doctrine placement

- `node:domain` is **non-primordial** (no native literal/syntactic form; it is
  a plain `require`/`import`-only utility class, squarely in the "Registry /
  `.ts` stdlib" bucket per the primordial-vs-registry doctrine). The engine
  never names `Domain`, `domain`, or any of its methods directly.
- It resolves exactly like every other `node:*` module: `rts-node`'s
  `NodespaceSpec { node_module: "domain", ns_prefix: "node_domain", members }`
  is added to `NODE_SPECS`; `ns_prefix_for("node:domain")` yields `"node_domain"`
  and `node_lookup("node_domain.<name>")` resolves any (rare — see §5.2) native
  member. The engine's only involvement is the generic `node:` import
  resolution machinery already used for `fs`/`path`/`os`/`process`/etc. —
  zero new codegen surface.
- Native-extern vs `.ts`-shim split: **the entire public surface in §2 is a
  `.ts` shim** (`rts-node/src/domain/domain.ts`, shipped and `e.include`-d like
  other `.ts` stdlib modules). The Rust side (`rts-node/src/domain/mod.rs`)
  contributes only the `NodespaceSpec` registration entry, with `MEMBERS`
  either empty or holding the two optional fast-path externs from §5.2 if a
  later phase adds them.

### 5.7 Shared-infra dependencies (FLAG)

- **`node:events` (`EventEmitter`)** — `Domain` extends it; must already be
  available as a subclassable `.ts`/native-backed class inside `rts-node`
  (either ported from the current `events` runtime namespace or freshly
  implemented there). Not itself a cross-crate dependency once ported, but
  domain work should not start before `node:events` is in place.
- **Promise subsystem (`promise.create`/`.then`/`.wait`)** — currently lives in
  `rts-std` per the architecture notes (`async_rt`/`event_loop`/`promise`).
  `rts-node` must not depend on `rts-std`. The `.then`/`.catch` registration
  hook needed for domain-aware Promise dispatch (§5.3) can likely be
  implemented purely at the `.ts` level (patching `Promise.prototype.then`,
  which is JS-visible regardless of which crate backs the native `promise.*`
  calls underneath) — **if** Promise method dispatch in the current engine is
  a genuine dynamic-prototype call and not a baked-in intrinsic that bypasses
  `.prototype`. This needs verification (§7); if it is not patchable at the
  `.ts` level, the domain-capture/restore mechanism must instead be hoisted
  into a shared low crate (e.g. `rts-engine` or a new shared crate) that both
  `rts-node` and the Promise implementation can call into.
- **Tokio `spawn_blocking` boundary** — the async-function continuation hop
  described in §5.3/§5.4 lives in the Promise/async subsystem (`rts-std`
  today). Domain capture-before-spawn / restore-on-worker-thread needs a hook
  at that exact boundary; same hoisting caveat as above applies.
- **`process.domain`** — needs a small, in-crate (not cross-crate) coupling
  between `rts-node`'s `node:process` shim and `node:domain`'s active-domain
  accessor. Both live in `rts-node`, so this is not a shared-infra concern,
  just an inter-module ordering note (`node:domain` must be importable/
  initializable before `process.domain` is read, matching Node's own
  lazy-population behavior — `process.domain` is `undefined` until
  `node:domain` has been loaded at least once).

If the `.ts`-level Promise patch in §5.3 turns out to work without any new
native hook, the only genuinely *required* shared-infra dependency is
**`node:events`**, and the Promise/tokio items above become "nice to have,
needed only for full spec parity of the async-following behavior" rather than
a hard blocker.

### 5.8 Implementation phases

1. **(a)** Land `rts-node/src/domain/mod.rs` — `NodespaceSpec` registration
   with an empty `MEMBERS` (unblocks `import ... from "node:domain"` resolving
   at all, per §5.6).
2. **(b)** `.ts` shim: `Domain` class extending `node:events`'s `EventEmitter`,
   `members: []`, thread-local `domainStack` gcell, `domain.create()`,
   `domain.active` getter.
3. **(c)** `enter()`/`exit()` (stack push/pop) + `run(fn, ...args)` (try/
   finally around enter/exit, catch routes to `this.emit('error', err)` with
   `err.domain = this; err.domainThrown = true`).
4. **(d)** `bind(cb)` / `intercept(cb)` wrappers (same enter/exit/tag pattern,
   `intercept` additionally short-circuiting on a truthy first arg).
5. **(e)** `add(emitter)` / `remove(emitter)` — patch the target emitter's
   internal emit path (or wrap its `.emit`) to route `'error'` through the
   owning domain; enforce "remove from previous domain first" on `add`.
6. **(f)** `process.domain` getter wired into `rts-node`'s `node:process`
   shim, reading `domain.active`.
7. **(g)** Implicit binding for `.ts`-shimmed timers (`setTimeout`/
   `setInterval`/`setImmediate`) and `process.nextTick`, when scheduled
   inside an active `run()` — capture-and-wrap at scheduling time.
8. **(h)** Promise `.then()`/`.catch()`/`.finally()` domain-registration-time
   capture (lazy `Promise.prototype` patch installed on first `node:domain`
   import) — contingent on the §5.7/§7 verification of whether `.prototype`
   patching is observable by the engine's Promise dispatch.
9. **(i)** Domain-follows-continuation across the tokio `spawn_blocking` hop
   for `async function` bodies (§5.3 last row / §5.4) — the one item that may
   require the shared-infra hoist from §5.7; can ship as an explicit
   known-limitation if deferred.
10. **(j)** Test suite (§6); confirm no-listener-throws semantics and the
    `add`/`remove` domain-reassignment edge case explicitly.

## 6. Test plan

- `domain_run_catches_sync_throw.test.ts` — `d.run(() => { throw new Error("x") })`
  with a `d.on('error', ...)` listener; assert the listener fires once with the
  thrown error, and that the error carries `domainThrown === true` and
  `domain === d`.
- `domain_run_no_listener_rethrows.test.ts` — same as above but with **no**
  `'error'` listener registered; assert the error propagates/crashes rather
  than being silently swallowed (§4 "unhandled error still throws").
- `domain_add_emit_error.test.ts` — `d.add(emitter)`; `emitter.emit('error', new Error("e"))`;
  assert it surfaces on `d`'s `'error'` event with `domainEmitter === emitter`
  and `domainThrown === false`.
- `domain_add_listener_throws.test.ts` — `d.add(emitter)`; a listener on
  `emitter.on('data', () => { throw ... })`; `emitter.emit('data')`; assert
  routed to `d`'s `'error'`.
- `domain_remove_unbinds.test.ts` — `d.add(emitter)` then `d.remove(emitter)`;
  a subsequent `emitter.emit('error', ...)` must **not** reach `d` (and, with
  no other listener, should throw/propagate normally).
- `domain_add_reassigns_from_other_domain.test.ts` — `d1.add(emitter)`;
  `d2.add(emitter)`; assert `emitter` is no longer a member of `d1.members`
  and errors now route to `d2`, not `d1`.
- `domain_bind_wraps_callback.test.ts` — `const wrapped = d.bind(cb)`; calling
  `wrapped(...)` where `cb` throws routes to `d`'s `'error'`; calling it where
  `cb` returns normally forwards the return value and arguments correctly.
- `domain_intercept_error_first_arg.test.ts` — `d.intercept(cb)` invoked with
  `(someError, data)` — assert `cb` is **not** called and `someError` reaches
  `d`'s `'error'` instead; invoked with `(null, data)` — assert `cb(data)` runs
  normally.
- `domain_enter_exit_nesting.test.ts` — manually `d1.enter()`, assert
  `domain.active === d1`; `d2.enter()`, assert `domain.active === d2`; `d2.exit()`,
  assert `domain.active === d1`; `d1.exit()`, assert `domain.active === undefined`.
- `domain_process_domain_mirror.test.ts` — while `d.run(...)` is executing
  synchronously, assert `process.domain === d` from inside the callback.
- `domain_nested_run.test.ts` — `d1.run(() => { d2.run(() => { throw ... }) })`
  — assert the error routes to `d2` (the innermost active domain), not `d1`.
- `domain_promise_then_registration_domain.test.ts` — mirrors the upstream
  docs example: create a promise inside `d1.run()`, call `.then()` inside
  `d2.run()`, assert the handler observes `domain.active === d2` (registration
  domain), matching documented Node semantics from §4. Mark as expected-fail /
  skip until phase (h) in §5.8 lands, to lock in the target behavior up front.
- `domain_promise_then_bind_override.test.ts` — same setup but the handler is
  wrapped with `d1.bind(...)`; assert it runs under `d1` instead.
- `domain_timer_implicit_binding.test.ts` — `d.run(() => setTimeout(() => { throw ... }, 0))`;
  assert the timer callback's throw is routed to `d`'s `'error'` (phase (g)).
- `domain_async_continuation_follows.test.ts` (multithread-relevant) —
  `d.run(async () => { await Promise.resolve(); throw new Error("late") })`;
  assert the throw after the `await` (which, per §5.3/§5.4, may execute on a
  different tokio worker OS thread than the one that called `run()`) still
  routes to `d`'s `'error'`, proving the capture/restore plumbing survives the
  thread hop. Mark as expected-fail/skip until phase (i) lands if deferred.
- `domain_worker_thread_isolation.test.ts` (multithread) — start a domain,
  `run()` code that spawns a real RTS worker thread (`node:worker_threads`,
  when available); assert the spawned thread's `domain.active` is
  `undefined` (fresh, isolated realm) rather than inheriting the parent's
  active domain, per §5.4.
- `domain_module_load_no_crash.test.ts` — bare `import domain from "node:domain"`
  with no further usage; assert the program runs to completion (module import
  alone must not throw or hang), and that `process.domain` reads as
  `undefined` before any domain has been entered.

## 7. Open questions / deferrals

- **Full "implicit binding" parity.** Real Node auto-binds essentially every
  async primitive created during `run()`'s synchronous extent (timers, native
  handles, fs requests) via deep engine hooks. RTS's initial scope (§5.8
  phases a–h) covers explicit `add`/`bind`/`intercept`, plus `.ts`-shimmed
  timers and Promise `.then` registration-time capture. Native fs/net/other
  `rts-node` module callbacks scheduled inside `run()` without explicit
  `bind()`/`intercept()` are **not** auto-bound in the initial implementation
  — this is a documented, honest gap (the module is deprecated upstream
  specifically because this kind of implicit magic is unreliable; RTS should
  not over-invest in perfecting it).
- **`Promise.prototype.then` patchability.** Needs a concrete check against the
  current engine: is `Promise`'s `.then`/`.catch`/`.finally` dispatch a real
  dynamic prototype-method call that a `.ts`-level monkeypatch can observe, or
  a baked-in intrinsic invoked directly by codegen (bypassing `.prototype`
  lookup)? This determines whether phase (h)/(i) in §5.8 are pure `.ts` work
  or require the native hook sketched in §5.2.
- **`domain.Domain` export.** Whether Node 25's `lib/domain.js` still exports
  the raw `Domain` class (undocumented on the public docs page) needs a
  source-level check before deciding whether `domain.Domain` is part of RTS's
  committed surface or an intentionally-omitted implementation detail.
- **Cross-thread continuation (§5.3/§5.4/§5.7 last row).** Whether the
  capture/restore hook for the tokio `spawn_blocking` hop lands as pure
  `rts-node`-side `.ts` wrapping around the function handed to
  `promise.create`, or requires hoisting a small piece of the Promise/async
  runtime out of `rts-std` into shared infra, is undecided pending a look at
  how `promise.create`'s `fn` argument is actually invoked today (whether it
  is guaranteed to go through a JS-level closure RTS can pre-wrap, vs. calling
  a raw generated `__async_inner_*` function pointer directly).
- **Priority vs. `AsyncLocalStorage`/`async_hooks`.** Given the module's own
  "pending deprecation" status and Node's explicit recommendation to use
  `async_hooks`/`AsyncLocalStorage` instead, RTS may choose to deprioritize
  the harder parts of §5.8 (phases g–i) in favor of implementing
  `AsyncLocalStorage` well first, shipping `node:domain` with only the
  synchronous/explicit-binding subset (phases a–f) plus documented
  limitations for the rest. This is a product-priority call, not an
  engineering blocker.
