# node:timers

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:timers` (+ `node:timers/promises`) |
| Node.js version | 25.x |
| Stability | 2 - Stable (`node:timers/promises` overall: 2 - Stable; its `scheduler.wait`/`scheduler.yield` pair: 1 - Experimental) |
| Tier | P0 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import timers from 'node:timers'`; `import { setTimeout, clearTimeout, setInterval, clearInterval, setImmediate, clearImmediate } from 'node:timers'`; `const timers = require('node:timers')`; `import { setTimeout, setInterval, setImmediate, scheduler } from 'node:timers/promises'`; `const timersPromises = require('node:timers/promises')`; ambient global (no import at all) `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`/`setImmediate`/`clearImmediate` |
| Globals exposed | `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`, `setImmediate`, `clearImmediate` — these are **already** ambient globals in Node (and in RTS today, via the `timers` global namespace). `node:timers` does not introduce *new* globals; it re-exports the identical function objects as named/default exports of an explicit module (`require('node:timers').setTimeout === globalThis.setTimeout` in real Node — same identity, not a copy) |

## 1. Purpose

`node:timers` is the explicit-import form of JavaScript's ambient timer scheduling primitives: `setTimeout`/`setInterval`/`setImmediate` and their `clear*` counterparts, plus the `Timeout`/`Immediate` handle objects they return. It exists so code can `import` timers the same way it imports any other Node builtin (useful for tooling, explicit dependency graphs, and non-browser-global environments), while remaining byte-for-byte the same objects as the ambient globals. `node:timers/promises` is the promise-returning sibling: `await setTimeout(ms, value)` instead of a callback, an async-iterator form of `setInterval`, `AbortSignal`-based cancellation via an `options.signal`, and the experimental WHATWG Scheduling-APIs-flavored `scheduler.wait`/`scheduler.yield`. This spec covers both the `node:timers` module surface and the `node:timers/promises` submodule; it does **not** cover `process.nextTick` (a distinct, higher-priority microtask-like queue documented under `node:process`) or `queueMicrotask` (a WHATWG global, not part of the `timers` module in Node's own docs).

## 2. Exported API surface (COMPLETE)

### Classes

#### `class Timeout`

- **Returned by:** `setTimeout()` and `setInterval()` (both the `node:timers` module form and the ambient global form — same class).
- **Base class:** none (not an `EventEmitter`; a plain opaque handle-like object).
- **Events:** none.
- **Instance methods:**

  | Method | Added | Returns | Notes |
  |---|---|---|---|
  | `timeout.close()` | v0.9.1 | `Timeout` (`this`) | **Stability: 3 - Legacy** — use `clearTimeout()` instead. Cancels the timeout. |
  | `timeout.hasRef()` | v11.0.0 | `boolean` | `true` if this `Timeout` keeps the event loop active. |
  | `timeout.ref()` | v0.9.1 | `Timeout` (`this`) | Requests the event loop not exit while this `Timeout` is active. Idempotent; default state for a freshly created `Timeout`. |
  | `timeout.refresh()` | v10.2.0 | `Timeout` (`this`) | Resets the timer's start time to now and reschedules it to fire after the **original** delay, measured from now. No-op-safe to call on an already-fired one-shot timeout (re-arms it) or on an already-cleared one (no observable effect per Node; RTS should match — see §4). |
  | `timeout.unref()` | v0.9.1 | `Timeout` (`this`) | The active `Timeout` will **not** require the event loop to remain active — if it is the only thing pending, the process may exit before it fires. |
  | `timeout[Symbol.toPrimitive]()` | v14.9.0, v12.19.0 | `integer` | Coerces the `Timeout` to a primitive numeric id that can be used to reference this timeout — notably **across `worker_threads`**, after transferring the primitive to the correct thread and calling `clearTimeout(thatNumber)` there. |
  | `timeout[Symbol.dispose]()` | v20.5.0/v18.18.0 (stable since v24.2.0) | `void` | Cancels the timeout. Enables `using t = setTimeout(...)` (explicit resource management, TC39 stage-4 as of ES2026). Equivalent to `clearTimeout()`. |

#### `class Immediate`

- **Returned by:** `setImmediate()` (module form and ambient global form — same class).
- **Base class:** none.
- **Events:** none.
- **Instance methods:**

  | Method | Added | Returns | Notes |
  |---|---|---|---|
  | `immediate.hasRef()` | v11.0.0 | `boolean` | `true` if this `Immediate` keeps the event loop active. |
  | `immediate.ref()` | v9.7.0 | `Immediate` (`this`) | Requests the event loop not exit while this `Immediate` is active. Default state. |
  | `immediate.unref()` | v9.7.0 | `Immediate` (`this`) | The active `Immediate` will not require the event loop to remain active. |
  | `immediate[Symbol.dispose]()` | v20.5.0/v18.18.0 (stable since v24.2.0) | `void` | Cancels the immediate. Equivalent to `clearImmediate()`. |

  `Immediate` has **no** `.refresh()` (immediates are inherently one-shot, "as soon as possible", so there is nothing meaningful to re-arm) and **no** `Symbol.toPrimitive` (Node does not document cross-thread `Immediate` id coercion the way it does for `Timeout`).

### Top-level functions

#### `node:timers` module (and identical ambient globals)

| Function | Variant |
|---|---|
| `setTimeout(callback[, delay[, ...args]])` | callback |
| `clearTimeout(timeout)` | sync |
| `setInterval(callback[, delay[, ...args]])` | callback |
| `clearInterval(timeout)` | sync |
| `setImmediate(callback[, ...args])` | callback |
| `clearImmediate(immediate)` | sync |

##### `setTimeout(callback[, delay[, ...args]])`

- **Added:** v0.0.1. **History:** v18.0.0 — an invalid `callback` now throws `ERR_INVALID_ARG_TYPE` (previously `ERR_INVALID_CALLBACK`).
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `callback` | `(...args: any[]) => void` | no | — |
  | `delay` | `number` | yes | `1` |
  | `...args` | `any[]` | yes | `[]` |

- **Returns:** `Timeout` — pass to `clearTimeout()`.
- **Throws:** `TypeError` (`ERR_INVALID_ARG_TYPE`) if `callback` is not a function.
- **Variant:** callback (one-shot, fires once after `delay` ms).

##### `clearTimeout(timeout)`

- **Added:** v0.0.1.
- **Params:** `timeout: Timeout | string | number` — a `Timeout` object, or the primitive obtained from `timeout[Symbol.toPrimitive]()` (as a `string` or `number`).
- **Returns:** `void`.
- **Throws:** never (clearing an unknown/already-cleared/foreign-value timeout is a silent no-op, matching `clearTimeout(undefined)`/garbage-argument tolerance in real Node).
- **Variant:** sync.

##### `setInterval(callback[, delay[, ...args]])`

- **Added:** v0.0.1. **History:** v18.0.0 — same `ERR_INVALID_ARG_TYPE` change as `setTimeout`.
- **Params:** identical shape to `setTimeout` (`callback`, `delay` default `1`, `...args`).
- **Returns:** `Timeout` — pass to `clearInterval()`.
- **Throws:** `TypeError` (`ERR_INVALID_ARG_TYPE`) if `callback` is not a function.
- **Variant:** callback (repeating, fires every `delay` ms until cleared).

##### `clearInterval(timeout)`

- **Added:** v0.0.1.
- **Params:** `timeout: Timeout | string | number`.
- **Returns:** `void`.
- **Throws:** never.
- **Variant:** sync.

##### `setImmediate(callback[, ...args])`

- **Added:** v0.9.1. **History:** v18.0.0 — same `ERR_INVALID_ARG_TYPE` change.
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `callback` | `(...args: any[]) => void` | no | — |
  | `...args` | `any[]` | yes | `[]` |

- **Returns:** `Immediate` — pass to `clearImmediate()`.
- **Throws:** `TypeError` (`ERR_INVALID_ARG_TYPE`) if `callback` is not a function.
- **Variant:** callback (fires once, at the end of the current event-loop turn, after I/O callbacks and before the next timer poll).

##### `clearImmediate(immediate)`

- **Added:** v0.9.1.
- **Params:** `immediate: Immediate`.
- **Returns:** `void`.
- **Throws:** never.
- **Variant:** sync.

#### `node:timers/promises` submodule

| Function | Variant |
|---|---|
| `timersPromises.setTimeout([delay[, value[, options]]])` | promise |
| `timersPromises.setImmediate([value[, options]])` | promise |
| `timersPromises.setInterval([delay[, value[, options]]])` | promise (async iterator) |
| `timersPromises.scheduler.wait(delay[, options])` | promise |
| `timersPromises.scheduler.yield()` | promise |

##### `timersPromises.setTimeout([delay[, value[, options]]])`

- **Added:** v15.0.0.
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `delay` | `number` | yes | `1` |
  | `value` | `any` | yes | `undefined` |
  | `options` | `TimerPromiseOptions` | yes | `{}` |

- **Returns:** `Promise<any>` — fulfills with `value` after `delay` ms.
- **Throws (rejects with):** `AbortError` (`DOMException`, `name === 'AbortError'`) if `options.signal` fires before the delay elapses; `TypeError` (`ERR_INVALID_ARG_TYPE`) for a non-function/non-`AbortSignal` bad-shaped `options.signal`.
- **Variant:** promise.

##### `timersPromises.setImmediate([value[, options]])`

- **Added:** v15.0.0.
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `value` | `any` | yes | `undefined` |
  | `options` | `TimerPromiseOptions` | yes | `{}` |

- **Returns:** `Promise<any>` — fulfills with `value` at the end of the current event-loop iteration.
- **Throws (rejects with):** `AbortError` if `options.signal` fires before the immediate runs.
- **Variant:** promise.

##### `timersPromises.setInterval([delay[, value[, options]]])`

- **Added:** v15.9.0.
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `delay` | `number` | yes | `1` |
  | `value` | `any` | yes | `undefined` |
  | `options` | `TimerPromiseOptions` | yes | `{}` |

- **Returns:** `AsyncIterable<any>` — a `for await` loop yields `value` every `delay` ms, indefinitely, until the loop body `break`s/`return`s/throws or `options.signal` aborts (which makes the iterator throw `AbortError`).
- **Throws:** `AbortError` from the iterator when `options.signal` aborts mid-iteration.
- **Variant:** promise (async iterator, not a single promise).

##### `timersPromises.scheduler.wait(delay[, options])`

- **Added:** v17.3.0, v16.14.0. **Stability:** 1 - Experimental.
- **Params:**

  | Name | Type | Optional | Default |
  |---|---|---|---|
  | `delay` | `number` | no | — |
  | `options` | `TimerPromiseOptions` | yes | `{}` |

- **Returns:** `Promise<void>`.
- **Throws (rejects with):** `AbortError` on `options.signal` abort.
- **Variant:** promise. Defined by the WHATWG Scheduling APIs draft; equivalent to `timersPromises.setTimeout(delay, undefined, options)`.

##### `timersPromises.scheduler.yield()`

- **Added:** v17.3.0, v16.14.0. **Stability:** 1 - Experimental.
- **Params:** none.
- **Returns:** `Promise<void>`.
- **Throws:** never (no cancellation option — no `options` parameter at all).
- **Variant:** promise. Equivalent to `timersPromises.setImmediate()` called with no arguments.

### Properties & constants

None. Neither `node:timers` nor `node:timers/promises` exports any constant or data property (contrast e.g. `node:os`'s `constants` object) — the entire surface is the two classes and the eleven functions above.

### Events

None. Nothing in this module or submodule is an `EventEmitter`; there are no `'error'`/lifecycle events anywhere in the timers surface itself (compare `node:child_process`, `node:net`, etc.).

## 3. Types & option objects

```typescript
/** Shared shape of the `options` parameter accepted by every `node:timers/promises`
 *  function except `scheduler.yield()` (which takes no options at all). */
interface TimerPromiseOptions {
  /** When `false`, the scheduled underlying `Timeout`/`Immediate` does not
   *  require the event loop to remain active (equivalent to calling
   *  `.unref()` on it). Default `true`. */
  ref?: boolean;
  /** An `AbortSignal` that cancels the scheduled operation; the returned
   *  promise/iterator rejects/throws with an `AbortError` when it fires. */
  signal?: AbortSignal;
}

/** The callback shape accepted by setTimeout/setInterval/setImmediate. */
type TimerCallback = (...args: any[]) => void;

/** What `clearTimeout`/`clearInterval` accept beyond a `Timeout` instance —
 *  the coerced primitive from `timeout[Symbol.toPrimitive]()`, typically used
 *  to reference a timer created on (and to be cleared from) another thread. */
type TimeoutRef = Timeout | string | number;

/** Node's actual rejection value on timer-promise cancellation is a
 *  `DOMException` (not a plain `Error`), with this shape: */
interface AbortErrorLike extends Error {
  name: 'AbortError';
  code?: undefined; // DOMException's `.code` is 0 for a named "AbortError"; Node's own docs refer to it simply by `.name`
}

/** The value produced per-iteration by `timersPromises.setInterval`; `T` is
 *  whatever `value` was passed in (defaults to `undefined`). */
type IntervalAsyncIterable<T = undefined> = AsyncIterable<T>;
```

No callback-invocation-result shapes are involved anywhere in this module — every `TimerCallback` is `void`-returning and Node ignores any return value.

## 4. Node semantics & edge cases

- **Delay clamping.** For both `setTimeout` and `setInterval`, when `delay` is `> 2147483647` (max signed 32-bit int, ms — about 24.8 days), `< 1`, or `NaN`, Node silently clamps it to `1` — it never throws for an out-of-range delay. Non-integer delays are truncated toward zero (`1.9` → `1`).
- **`setImmediate` has no `delay` parameter at all** — it always fires at the end of the current turn, in the "check" phase of the libuv event loop, which runs after I/O callbacks and before the next timer-poll phase. This makes `setImmediate(cb)` and `setTimeout(cb, 0)` observably different in ordering under I/O-bound code (immediates fire first if the current tick is inside an I/O callback) though from a *fresh* top-level script tick the relative order between them is technically unspecified by the language and historically flips between the first tick and subsequent ticks in real Node — user code should not depend on which of the two wins on the very first tick.
- **Ordering relative to microtasks and `process.nextTick`.** Within one macrotask, `process.nextTick` queue drains first, then the promise microtask queue, and only then does control return to the event loop for the next timer/immediate/I/O phase — `node:timers`/`node:timers/promises` callbacks are always macrotasks and therefore always run *after* any pending `nextTick`/microtask backlog from the current tick.
- **`refresh()` on a fired one-shot `Timeout`.** Calling `.refresh()` after a `setTimeout` callback has already run re-arms it for another single fire (from now, using the original delay) — this is the documented mechanism for building a manual "debounce"-with-a-single-timer pattern without allocating a new `Timeout` each time. Calling `.refresh()` on an interval `Timeout` (from `setInterval`) resets its next-fire time the same way, without affecting the on-going period.
- **`ref()`/`unref()` default and effect.** A freshly created `Timeout`/`Immediate` is `ref()`'d by default (`hasRef()` returns `true`) — it keeps the process's event loop alive on its own. `unref()` flips this so the timer/immediate can still fire (if nothing else keeps the loop alive) but does not by itself prevent process exit; if the process would otherwise have nothing left to do, it exits and the unref'd timer simply never fires. This is the mechanism behind "background heartbeat that shouldn't keep the process running" patterns.
- **`Timeout[Symbol.toPrimitive]()` cross-thread use.** The coerced integer is explicitly documented as safe to pass to a different `worker_threads` thread (e.g. via `postMessage`) and used there with `clearTimeout(thatNumber)` — this only makes sense if the *identity* the primitive encodes is resolvable independent of which JS realm/thread calls `clearTimeout` with it (see §5.4 for how this constrains the RTS handle design).
- **`node:timers/promises` `AbortSignal` semantics.** If `options.signal` is already `aborted` at call time, the returned promise rejects immediately (no timer is even scheduled) with `signal.reason` if set, else a generic `AbortError` `DOMException`. If it aborts later, the underlying `Timeout`/`Immediate` is cleared and the promise rejects the same way. For `setInterval`'s async iterator form, an abort mid-loop causes the next `for await` step to throw (propagating out of the loop) rather than just silently ending iteration.
- **Deprecations.** No timer API was deprecated or removed for Node 25. The only "Legacy" (stability 3, distinct from "Deprecated") member is `timeout.close()`, which has carried that annotation since `clearTimeout()` was introduced and is not scheduled for removal — it remains fully functional, just not the recommended spelling.
- **Windows vs POSIX.** Node's own timer implementation (via libuv) has historically differed in *coalescing granularity* on Windows (older Windows kernels had ~15.6 ms default timer-tick resolution, meaning very small delays could fire up to that much late) versus sub-millisecond POSIX `clock_gettime`-backed timers; this is a libuv/OS scheduler-resolution artifact, not a documented Node API difference — no code-visible behavior difference exists in the public API surface itself, only in observed real-world firing jitter. RTS's own timer engine (§5.1) uses `std::time::Instant`, whose monotonic-clock resolution/coalescing behavior inherits the same platform characteristics.
- **`setInterval` drift.** Node does not drift-correct: each tick's next deadline is computed as "now (at the moment the previous callback returned) + period", so a slow callback body pushes every subsequent tick later by the same amount, rather than the interval schedule being anchored to the original start time. This is a documented (if implicit) behavioral contract long-running interval-based code relies on.
- **Security note.** Nothing in `timers`/`timers/promises` performs privileged operations or crosses a trust boundary; the only "abuse" vector documented anywhere near timers is unrelated event-loop starvation from a runaway zero-delay `setImmediate`/`setTimeout(fn,0)` recursion, which is a general Node event-loop-fairness concern, not a `timers`-specific security issue.

## 5. RTS implementation notes

### 5.1 Native impl mapping

RTS **already has** a working timer/event-loop core — it currently backs the *ambient global* `setTimeout`/`setInterval`/`setImmediate`/`clear*` family, implemented in `crates/rts-std/src/globals/timers/` (spec: `mod.rs`; native impl: `instance.rs`) and driven by `crates/rts-std/src/event_loop.rs`. `node:timers` must **reuse this exact machinery**, not reimplement it — in real Node, `require('node:timers').setTimeout` and `globalThis.setTimeout` are the identical function object; a second independent timer/macrotask queue for the module form would silently break cross-form ordering guarantees (a `node:timers` timeout and an ambient-global timeout racing against each other must be ordered by one shared `(deadline, seq)` timeline, not two).

Today's implementation, in Rust-standard-library terms:

- **Macrotask queue** (`instance.rs`'s `MACROTASK_QUEUE`): a `thread_local! RefCell<Vec<Macrotask>>` of `{ fp, flag, handle, deadline: std::time::Instant, seq: u64, period_ms: Option<u64> }` entries. Both `setTimeout` (`period_ms: None`) and `setInterval` (`period_ms: Some(ms)`) share this queue, ordered by `(deadline, seq)` for deterministic firing order — this replaced an earlier "one OS thread per timer" design that fired concurrent timers out of registration order.
- **Immediate queue** (`IMMEDIATE_QUEUE`, process-global `Mutex<Vec<(fp, cancelled, ran)>>`): drained separately, before macrotasks, matching the "check phase before timer/poll phase" ordering from §4.
- **Cancellation** (`TIMERS`, process-global `Mutex<HashMap<u64, Arc<AtomicBool>>>`): `clearTimeout`/`clearInterval`/`clearImmediate` set an `AtomicBool` flag consulted by the pump loops; this map is **not** thread-local, so a `clearTimeout` call from a different thread than the one that created the timer already works today (relevant to the `Symbol.toPrimitive` cross-thread use case in §4/§5.4).
- **Callback invocation** (`invoke_timer_cb`): detects whether the stored `fp` is a raw function pointer or a `Function` `Handle` (e.g. from `.bind()`) and dispatches through `__RTS_FN_RT_INVOKE_AUTO` accordingly — the same generic invocation bridge every other callback-taking API (`EventEmitter`, `Promise`, `AbortSignal` listeners) uses.
- **Pump/drain entry points**: `pump_due_macrotasks()` (fires everything already due, re-enqueuing intervals), `drain_immediates()`, `drain_macrotasks()` (post-main: fires due macrotasks and sleeps until the next deadline, capped at 5s), `pump_until(target)` (time-driven pump used by `time.sleep_ms` so a short `setTimeout` fires *during* a longer synchronous sleep, preserving order), `drain_pending_timers()` (blocks up to 5s waiting for pending one-shot timers so their callbacks run before process exit — intervals are excluded, since waiting for an infinite repeat would hang forever).
- **Event loop epilogue** (`event_loop.rs::run_event_loop`, exposed as the extern `__RTS_FN_RT_RUN_EVENT_LOOP`): drains, in JS-spec order, microtasks → immediates → macrotasks (which themselves drain their own microtasks per callback) → pending one-shot timers → fire-and-forget promises → microtasks again → unhandled-rejection reporting. Called host-side after `__rts_startup` on the JIT path, and from the AOT shim `main` before it returns (this was the fix that made `await`/`.then`/`setTimeout` work in `rts compile` binaries at all).

**Known gaps this spec's phases must close** (see §5.8), not present in today's implementation:
1. **Trailing-args forwarding.** `setTimeout(callback, delay, ...args)`'s `args` are **not currently forwarded** — `invoke_timer_cb` always invokes the callback with a fixed single `0` argument. Node passes every trailing argument through to the callback.
2. **No upper delay clamp or `NaN` handling.** The native path special-cases `delay_ms <= 0` to `0`/`1` but does not clamp `> 2147483647`, and depends on whatever a JS `NaN`-typed `delay` coerces to before it reaches the `i64` extern parameter — this coercion must happen at the `.ts`/lowering boundary (ToNumber-then-clamp), not be left to fall through as an arbitrary bit pattern.
3. **No `ref`/`unref`/`hasRef`/`refresh` primitives.** Today's "ref-ness" is an *aggregate* policy baked into `drain_pending_timers` (wait for all pending one-shot timeouts, never wait for intervals) rather than a per-timer flag — this does not match Node's actual per-instance `ref()`/`unref()` semantics (e.g. an explicitly-`unref()`'d one-shot timeout should **not** be waited for at process-exit, and an explicitly-`ref()`'d interval arguably should keep the process alive, neither of which today's blanket rule expresses).
4. **`Timeout`/`Immediate` are not reified as JS objects today** — `setTimeout` et al. return a raw `Handle` (`u64`), not an object with `.ref()/.unref()/.refresh()/.hasRef()/[Symbol.toPrimitive]/[Symbol.dispose]` methods. These need a thin `.ts`-side wrapper class (§5.2/§5.8).

### 5.2 ABI surface

Symbol convention for the **shared, hoisted** timer core (see §5.7 — this is not `rts-node`-owned code, it is code both the ambient-global registration and `rts-node`'s `node:timers` module point at): `__RTS_FN_RT_TIMER_<NAME>`, following the existing `__RTS_FN_RT_*` convention already used for engine-wide bridges like `__RTS_FN_RT_INVOKE_AUTO`. `rts-node`'s `NODE_SPECS` entry for `node_timers` declares `NodespaceMember`s whose `symbol` field points at these **same** extern names — no new/duplicate native queue, just a second data-table entry resolving to identical symbols (mirroring how, in real Node, the module export and the global are the same function object).

No `Timeout`/`Immediate` field is ever decomposed across the ABI boundary — both are opaque `Handle`s (u64) end to end; every method on them (`.ref()`, `.refresh()`, etc.) is a single-`Handle`-argument extern.

| Symbol | Args (AbiType) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_RT_TIMER_SET_TIMEOUT` | `U64 fp, I64 delay, Handle args_vec` | `Handle` | `args_vec` is a `Handle` to an `Entry::Vec` of the trailing `...args` (empty vec if none) — closes gap 1 above; `fp` may be a raw fn pointer or a `Function` `Handle` per existing `invoke_timer_cb` detection |
| `__RTS_FN_RT_TIMER_CLEAR_TIMEOUT` | `Handle` | `Void` | no-op on unknown/already-cleared handle |
| `__RTS_FN_RT_TIMER_SET_INTERVAL` | `U64 fp, I64 delay, Handle args_vec` | `Handle` | same args-forwarding extension |
| `__RTS_FN_RT_TIMER_CLEAR_INTERVAL` | `Handle` | `Void` | |
| `__RTS_FN_RT_TIMER_SET_IMMEDIATE` | `U64 fp, Handle args_vec` | `Handle` | |
| `__RTS_FN_RT_TIMER_CLEAR_IMMEDIATE` | `Handle` | `Void` | |
| `__RTS_FN_RT_TIMER_REF` | `Handle` | `Void` | sets the per-timer ref flag `true` (new — closes gap 3) |
| `__RTS_FN_RT_TIMER_UNREF` | `Handle` | `Void` | sets the per-timer ref flag `false` |
| `__RTS_FN_RT_TIMER_HAS_REF` | `Handle` | `Bool` | reads the per-timer ref flag |
| `__RTS_FN_RT_TIMER_REFRESH` | `Handle` | `Bool` | `Timeout` only; recomputes `deadline = now + original_delay` in place, re-enqueuing if the timer already fired (one-shot) or had been removed; returns `false` if the handle refers to an already-`clear()`-ed/unknown timer (nothing to refresh) |
| `__RTS_FN_RT_TIMER_TO_PRIMITIVE` | `Handle` | `I64` | backs `Timeout[Symbol.toPrimitive]`; returns the numeric id usable cross-thread with `clearTimeout` (see §5.4 for the precision constraint this implies) |

`rts-node`'s own crate contributes **no new native symbols for the base six operations** — only the `NodespaceSpec` metadata rows in `crates/rts-node/src/timers/mod.rs` (`ns_prefix: "node_timers"`, `node_module: "timers"`) pointing at the table above, mirroring exactly what the (also-hoisted) ambient-global `timers` namespace spec points at. `rts-node`'s `.ts` shim (§5.8) is where all JS-shape ergonomics live: the `Timeout`/`Immediate` wrapper classes, argument-count-based overload resolution (`delay` omitted vs `value` omitted in the promises API), and the entire `node:timers/promises` submodule (built as pure `.ts` over the table above plus the ambient `Promise`/`AbortSignal` — no native symbols of its own at all).

### 5.3 Async model

- **`setTimeout`/`setInterval`/`setImmediate` (module + global) are callback-style**, not promise-based — the native `__RTS_FN_RT_TIMER_SET_*` externs enqueue into the thread-local macrotask/immediate queue and return immediately; the callback fires later, off the call stack, when the owning thread's event-loop epilogue (`run_event_loop`/`pump_until`/`drain_macrotasks`) reaches it. No tokio task is spawned for the common case — firing is driven by the same-thread pump loop, not a background thread, which is what gives the `(deadline, seq)` ordering its determinism (no cross-thread race to reorder two timers registered microseconds apart).
- **`node:timers/promises`' `setTimeout`/`setImmediate`/`scheduler.wait`/`scheduler.yield` are thin promise wrappers** over the callback primitives: `new Promise((resolve, reject) => { const t = <nativeSetTimeout>(() => resolve(value), delay, []); ...abort wiring... })`. This needs the engine's `Promise` create/settle machinery (constructing a `Promise`, resolving it from inside a plain synchronous timer callback — no `spawn_blocking`/tokio task required, since the "work" being awaited is purely the passage of time already handled by the existing pump loop).
- **`node:timers/promises`' `setInterval`** is an async generator (`.ts`-level `async function* `) that internally does `await <promisified setTimeout>(delay, value, {signal})` in a loop, `yield`ing the resolved value each iteration, and clearing the underlying interval-analog timer if the loop is exited early (`return`/`break`/`throw` triggers the generator's `finally`) or `options.signal` aborts.
- **None of this needs the shared tokio runtime for the base timer-firing mechanism** — timers are pumped from the same thread's synchronous event-loop epilogue, not asynchronously scheduled on a thread pool. Tokio *is* needed transitively for `node:timers/promises`, only because that submodule builds on the engine's general `Promise` subsystem, which (for other, unrelated features such as `promise.create` around a Rust `Future`) already depends on the shared runtime being initialized — `node:timers/promises` itself issues no `rt().spawn_blocking(...)` call.
- **AOT vs JIT**: identical — both call the same `__RTS_FN_RT_RUN_EVENT_LOOP` epilogue (JIT: host-side after `__rts_startup`; AOT: from the generated `main` shim before it returns), so a `node:timers` timeout/interval/immediate registered during either compiled form's execution is drained the same way.

### 5.4 Multithread / worker interaction

- **The macrotask/immediate queues are `thread_local!` today, and that is the *correct* mapping onto `docs/specs/rts-threading-model.md`**, not an oversight to "fix" into a shared queue: each Node `worker_threads` `Worker` runs its own independent event loop with its own independent set of pending timers — a `setTimeout` registered on a worker fires relative to *that worker's* event-loop turns, never observed or drained by the main thread (or any other worker) directly. RTS's existing per-thread `MACROTASK_QUEUE`/`IMMEDIATE_QUEUE` already model this as `threadLocal` state; the pump/drain functions (`run_event_loop` et al.) must simply be called from **each** worker's own execution epilogue, exactly like the main thread's, once `worker_threads` is implemented.
- **The cancellation flag map (`TIMERS`) is process-global on purpose**, and this is what makes the documented `Timeout[Symbol.toPrimitive]` cross-thread pattern (§2/§4) already work today in principle: a numeric id obtained on thread A and sent to thread B via a `channel`/`MessagePort` can be used in `clearTimeout(id)` called from thread B, because the flag it flips is visible process-wide — only the *enqueue* side (which thread's pump loop will notice the flag and skip firing) is thread-local, which is fine, since the check happens on whichever thread owns the timer regardless of which thread requested cancellation.
- **Constraint this places on `__RTS_FN_RT_TIMER_TO_PRIMITIVE` (§5.2):** the coerced value must remain valid and *the same* number when read back on a different OS thread and passed into `clearTimeout`/`__RTS_FN_RT_TIMER_CLEAR_TIMEOUT` there — since the current `Handle` is a 64-bit `gen:16 + slot:48` value and JS numbers are IEEE-754 doubles (53-bit safe-integer range), the full 64-bit handle **cannot** always round-trip through a JS `number` losslessly. This must either (a) be verified as in-range for realistic slot counts (48-bit slot alone already exceeds 53 bits combined with a 16-bit generation, so verification is required, not assumed), or (b) use a narrower dedicated numeric-id scheme just for this coercion (e.g. a separate small monotonic counter keyed to the handle in a side table), analogous to how `rts-std`'s per-shard `HandleTable` already partitions differently-sized fields. Flagged as a concrete open question in §7.
- **Worker lifecycle edge case:** if a worker terminates (or the process exits) while its `thread_local!` macrotask/immediate queue still has entries, those timers never fire and their `Handle`s (and any `Entry::Vec` args they reference) are never freed via the normal fire-then-`free_handle` path — RTS needs an explicit "drain or discard this thread's timer queue" hook on worker teardown (mirroring `drain_pending_timers`'s existing "wait up to 5s at main-thread exit" policy, but bounded per-worker-teardown rather than only at process end) once `worker_threads` lands; until then this is latent (single-thread-correct) dead-handle-on-abrupt-exit behavior, not a regression this spec introduces.
- **`SharedArrayBuffer`/`MessagePort` transfer:** neither `Timeout` nor `Immediate` is a transferable object in Node (only the coerced `Symbol.toPrimitive` *number*, for `Timeout`, is meant to cross threads) — RTS must not attempt to make the `Handle` itself directly usable from a thread that did not create it, only the coerced primitive id.

### 5.5 Buffer / TypedArray interop

None directly. No timer function accepts or returns binary data. The only way bytes reach a timer callback at all is indirectly, through the ordinary `...args` forwarding path (§5.1 gap 1/§5.2's `args_vec` `Handle`): if user code does `setTimeout((buf) => ..., 100, someBuffer)`, `someBuffer` (a `Uint8Array`-derived `Buffer`, primordial per the engine's TypedArray model) rides inside the generic `Entry::Vec` args handle like any other `PolyValue` argument — no timers-specific marshalling is needed beyond what the generic function-invocation bridge (`__RTS_FN_RT_INVOKE_AUTO`) already does for any callback's arguments.

### 5.6 Doctrine placement

`node:timers` is **non-primordial** — the engine (`rts-codegen-new`) must never hardcode `"timers"` (or `"setTimeout"`/`"setInterval"`/`"setImmediate"`/etc. as *module-import* names) anywhere in codegen control flow. Resolution follows the existing `NodespaceSpec` mechanism (`crates/rts-node/src/lib.rs`): `import { setTimeout } from 'node:timers'` maps through `rts_node::ns_prefix_for("node:timers")` → `"node_timers"` (pure data lookup against `NODE_SPECS`), and `node_timers.setTimeout(...)` resolves via `rts_node::node_lookup("node_timers.setTimeout")` to a `NodespaceMember` — the same generic path every other `node:*` module uses, zero hardcoded arm in codegen.

The **ambient global** `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`/`setImmediate`/`clearImmediate` (no import required) are separately classified as **web-global infra** per the owner decision that "rts-std keeps only RTS-unique surface + the web-global infra" — they stay registered as a global namespace (today: `timers` in `rts-engine`'s `Engine::ns`, wherever that registration ends up living post-migration) exactly as they are today, unrelated to whether `rts-node` exists. **The one thing that must not happen** is `rts-node` growing its *own*, second copy of the macrotask/immediate queue and pump loop just to answer `node:timers` calls — unlike `fs`/`os`/`crypto` (where `rts-node` legitimately owns an independent native implementation because the underlying OS resource genuinely has nothing to do with any other engine subsystem), the timer queue **is** the engine's own macrotask timeline, shared with `await`/`Promise` ordering and the event-loop epilogue itself. Two independent queues would silently desynchronize `node:timers`-created timers from ambient-global-created ones (and from `Promise` microtask ordering), which is exactly the class of bug this doctrine section exists to prevent for infra that must stay singular. See §5.7 for the concrete hoist this implies.

### 5.7 Shared-infra dependencies (FLAG)

`rts-node` cannot depend on `rts-std`, but `node:timers`/`node:timers/promises` parity needs infrastructure that **today lives entirely inside `rts-std`**. Unlike most other `node:*` P0 modules, this is not "give `rts-node` its own independent native implementation" (that would be actively wrong here per §5.6) — it is "hoist the one true timer/event-loop core to somewhere both the ambient-global registration and `rts-node` can reach it":

- **The macrotask + immediate queues and pump/drain functions** — currently `rts-std/src/globals/timers/instance.rs` (`MACROTASK_QUEUE`, `IMMEDIATE_QUEUE`, `TIMERS`, `pump_due_macrotasks`/`drain_macrotasks`/`drain_immediates`/`pump_until`/`drain_pending_timers`). This is the centerpiece of the whole module — it must move to a crate below both `rts-std` and `rts-node` (or directly into `rts-engine`, alongside the `HandleTable`/GC it already leans on for `Entry::Env`/`Entry::Vec` allocation), with **both** the ambient-global `timers` namespace and `rts-node`'s `node_timers` `NodespaceSpec` pointing at the identical hoisted symbols — never two copies.
- **The event-loop epilogue** — currently `rts-std/src/event_loop.rs` (`run_event_loop`/`__RTS_FN_RT_RUN_EVENT_LOOP`). This already calls into the timers module directly (`drain_immediates`/`drain_macrotasks`/`drain_pending_timers`) and also drains microtasks and pending promises — it needs to move (or be split) alongside the timer core so the ordering contract (microtasks → immediates → macrotasks → pending timers → fire-and-forget promises → microtasks again) stays a single, one-owner pipeline reachable from wherever `rts-node`'s `node:timers` externs and the ambient globals both live.
- **`Promise` create/settle subsystem** — currently `rts-std/src/promise/`. Needed only for `node:timers/promises` (wrapping a native timer callback in `new Promise(...)`/resolving it) — the base `node:timers` callback surface does not need this.
- **Generic native-thread → JS-callback invocation bridge (`__RTS_FN_RT_INVOKE_AUTO`)** — already used by the existing timers implementation itself (and by `EventEmitter`/`AbortSignal` listeners); needs to be reachable from wherever the hoisted timer core ends up, same as today.
- **GC thread-registry hook** — any worker thread that owns a `thread_local!` timer queue (§5.4) must be registered the same way tokio worker threads are today (`rts-std/src/runtime/async_rt.rs`'s `on_thread_start` hook into `gc/thread_registry`) so the GC's conservative stack scanner can see live `Handle`s referenced from that thread's pending `Macrotask`/immediate entries.
- **Not flagged (already reachable, no hoist needed):** `AbortController`/`AbortSignal` — these are ambient **global classes** resolved through the engine's ordinary `global_class_lookup` mechanism (`crates/rts-std/src/globals/abort/`), so `rts-node`'s `.ts` shim for `node:timers/promises` can simply write plain TS (`signal.addEventListener('abort', ...)`, `signal.aborted`, `AbortSignal.timeout(...)`) exactly like any user script does — no direct crate coupling to `rts-std` internals is needed for this part, the same way the `console`/`Map`/`Set` `.ts` shims call ambient globals without a Rust-level dependency.

If no hoist happens first, `node:timers` cannot be implemented at all without violating §5.6's "must not duplicate the queue" constraint — this is the single blocking dependency for this entire module (contrast `node:os`, which needed zero hoist prerequisites).

### 5.8 Implementation phases

1. **(a)** Hoist the macrotask/immediate queues, cancellation-flag map, and pump/drain functions (today's `rts-std/src/globals/timers/instance.rs`) plus the event-loop epilogue (`rts-std/src/event_loop.rs`) to a crate reachable by both the ambient-global registration and `rts-node`, renaming the externs to the `__RTS_FN_RT_TIMER_*` convention from §5.2 (update the ambient-global `timers` namespace spec's `symbol` fields to match — pure rename, no behavior change). This is the prerequisite everything else depends on (§5.7).
2. **(b)** Extend `__RTS_FN_RT_TIMER_SET_TIMEOUT`/`SET_INTERVAL`/`SET_IMMEDIATE` to accept the `args_vec: Handle` parameter and thread it through `invoke_timer_cb` (extending the existing empty-`Entry::Vec` call into the real forwarded args) — closes the trailing-args gap (§5.1 gap 1). Update the `.ts`-visible signature the ambient globals already expose so existing global-`setTimeout` callers gain arg-forwarding too (this is a real behavior fix, not `node:timers`-exclusive, and should be called out as such in the PR per the "explicit regression/behavior-change" project rule).
3. **(c)** Add the `REF`/`UNREF`/`HAS_REF`/`REFRESH` externs: extend `Macrotask`/the immediate-queue entry with a `ref_flag: Arc<AtomicBool>` (default `true`) alongside the existing `cancelled` flag; teach `drain_pending_timers`'s process-exit wait to consult per-timer `ref_flag` instead of the current blanket "wait for all one-shot timeouts, never wait for intervals" rule (§5.1 gap 3) — an unref'd one-shot timeout must not be waited on; a ref'd interval arguably should keep the loop alive (needs a decision on how `drain_pending_timers`'s current "never wait for intervals" policy composes with an explicitly-`ref()`'d interval — flagged in §7).
4. **(d)** Add the delay-clamping/`NaN` handling at the `.ts`/lowering coercion boundary (ToNumber → clamp to `[1, 2147483647]`, non-integer truncation) so it is applied uniformly to both the ambient globals and `node:timers` (§5.1 gap 2) — this is a coercion-layer fix, not a new native symbol.
5. **(e)** Build the `Timeout`/`Immediate` `.ts` wrapper classes (`crates/rts-node/src/timers/timers.ts`) around the opaque `Handle`s: `.ref()/.unref()/.hasRef()/.refresh()/.close()` delegating to the (c) externs, `[Symbol.toPrimitive]` delegating to `__RTS_FN_RT_TIMER_TO_PRIMITIVE`, `[Symbol.dispose]` delegating to the appropriate `clear*`. Register `crates/rts-node/src/timers/mod.rs`'s `NodespaceSpec` (`node_module: "timers"`, `ns_prefix: "node_timers"`) in `NODE_SPECS`, with `setTimeout`/`clearTimeout`/`setInterval`/`clearInterval`/`setImmediate`/`clearImmediate` all resolving to the shared symbols from (a)-(d) — no new native queue.
6. **(f)** Build `node:timers/promises` entirely as a `.ts` shim (`crates/rts-node/src/timers/promises.ts`) over (e)'s primitives plus ambient `Promise`/`AbortController`/`AbortSignal`: `setTimeout`/`setImmediate` as direct promise wrappers with abort wiring; `setInterval` as an async generator looping a promisified single-shot delay; `scheduler.wait`/`scheduler.yield` as one-line delegations to the two above. No new native externs.
7. **(g)** Verify/extend `Symbol.dispose` dispatch end to end for `using t = setTimeout(...)` — confirm the engine's well-known-`Symbol` method resolution reaches a `.ts`-shim-defined class method the same way it does for other well-known symbols (`Symbol.iterator`), since this may not yet be exercised by any existing feature (flagged in §7 if it needs its own follow-up).
8. **(h)** `worker_threads` interaction pass (can land after `worker_threads` itself exists): call the hoisted pump/drain epilogue from each worker's own execution teardown, and resolve the `Symbol.toPrimitive` cross-thread precision question from §5.4/§7 before shipping that specific method as "done".
9. **(i)** Full test-plan pass (§6) across JIT and AOT, including the worker-threads leg once (h) lands.

## 6. Test plan

```
tests/node/timers/timers_basic.test.ts
  - setTimeout(cb, 10) fires cb exactly once, after clearTimeout on a DIFFERENT
    unrelated handle does not affect it
  - clearTimeout(handle) before the delay elapses prevents cb from firing at all
  - setTimeout(cb) with delay omitted defaults to 1ms (fires, does not hang)
  - setTimeout(cb, -5) / setTimeout(cb, NaN) both clamp to the minimum delay and
    still fire (do not throw, do not hang)
  - setTimeout(cb, 2147483648) (over the int32 max) clamps to minimum delay
    behavior rather than silently never firing or overflowing
  - setTimeout((a, b, c) => ..., 5, 1, 'two', [3]) receives (1, 'two', [3]) as
    the callback's arguments (closes the args-forwarding gap)
  - setTimeout(callback) with a non-function callback throws synchronously with
    a TypeError (ERR_INVALID_ARG_TYPE-shaped)
  - two setTimeout(cb, 10) calls registered back-to-back fire in registration
    order (deterministic (deadline, seq) ordering)
  - clearTimeout(timeout[Symbol.toPrimitive]()) (the coerced number form)
    cancels the same timer as clearTimeout(timeout) (the object form)

tests/node/timers/timers_interval.test.ts
  - setInterval(cb, 10) fires cb repeatedly; clearInterval after N fires stops
    further invocations
  - a closure-capturing counter incremented inside the interval callback
    reflects every tick (regression guard for the historical
    thread-per-interval bug where a captured counter was invisible)
  - setInterval(cb, 0) / setInterval(cb, -1) clamp to a minimum positive period
    rather than firing in a tight/hanging loop
  - clearInterval called from inside the interval's own callback (self-cancel
    on the Nth tick) stops further ticks cleanly

tests/node/timers/timers_immediate.test.ts
  - setImmediate(cb) fires cb exactly once, with no delay parameter
  - setImmediate(cb, 'x', 'y') forwards trailing args
  - clearImmediate(handle) before the current turn ends prevents cb from firing
  - ordering: queueMicrotask/.then() callbacks scheduled before a setImmediate
    in the same turn run before the immediate fires
  - ordering: a setImmediate registered inside a setTimeout(cb,0) callback
    fires before the *next* macrotask-queue entry (check phase semantics)

tests/node/timers/timers_ref_unref.test.ts
  - timeout.hasRef() is true immediately after setTimeout(); false after
    .unref(); true again after a subsequent .ref()
  - a process whose only pending work is an unref'd setTimeout exits without
    waiting for it to fire (spawn as a subprocess fixture, assert fast exit
    and that the callback's side effect never happened)
  - a process with only a ref'd setTimeout (default state) waits for it to
    fire before exiting
  - immediate.hasRef()/.ref()/.unref() mirror the same contract for setImmediate

tests/node/timers/timers_refresh.test.ts
  - timeout.refresh() on an unfired timeout resets its firing time (measure:
    refreshing repeatedly before the delay elapses postpones firing indefinitely,
    like a manual debounce)
  - timeout.refresh() called AFTER the callback has already fired re-arms it
    for exactly one more fire
  - timeout.refresh() returns the same Timeout instance (chainable)

tests/node/timers/timers_module_identity.test.ts
  - const { setTimeout: modST } = await import('node:timers'); modST === globalThis.setTimeout
  - clearing a timer created via the module form with the ambient global
    clearTimeout (and vice versa) works identically (proves shared queue,
    not two independent implementations)

tests/node/timers/timers_promises_basic.test.ts
  - await setTimeout(20, 'done') resolves to 'done' after roughly the delay
  - await setImmediate('x') resolves to 'x' on the next turn
  - for await (const v of setInterval(10, 'tick')) { ... break after 3 iterations }
    yields 'tick' three times then stops (and the underlying interval-analog
    timer is confirmed cleared afterward — no further ticks observed)
  - await scheduler.wait(15) resolves with undefined
  - await scheduler.yield() resolves promptly (same-turn-ish, no meaningful delay)

tests/node/timers/timers_promises_abort.test.ts
  - setTimeout(1000, 'x', { signal: AbortSignal.timeout(10) }) rejects with an
    error whose .name === 'AbortError' well before the 1000ms delay
  - setTimeout(20, 'x', { signal: alreadyAbortedSignal }) rejects immediately
    without ever scheduling a real timer (observable via a fast rejection time)
  - an AbortController.abort() invoked mid-delay rejects the pending
    setTimeout/setImmediate promise
  - for-await setInterval(10, 'tick', { signal }) : aborting mid-loop causes the
    for-await to throw AbortError, and no further 'tick' values are yielded
  - setTimeout(20, 'x', { signal }).catch(e => e.name) distinguishes AbortError
    from a plain programming-error rejection

tests/node/timers/timers_symbol_dispose.test.ts
  - `{ using t = setTimeout(() => { flag = true }, 1000); }` — leaving the
    block synchronously (before 1000ms) cancels the timeout (flag stays false)
  - `t[Symbol.dispose]()` called manually behaves identically to clearTimeout(t)
  - same pattern for `using i = setImmediate(cb)` / `Immediate[Symbol.dispose]`

tests/node/timers/timers_worker_threads.test.ts (multithread)
  - a setTimeout registered inside a Worker fires and is observed only by that
    worker (a listener on the main thread never sees it; the worker's own
    event-loop epilogue drains it, not the main thread's)
  - a Timeout's coerced Symbol.toPrimitive() value, sent from the creating
    worker to the main thread via postMessage, successfully cancels the timer
    when clearTimeout(receivedNumber) is called from the main thread
  - terminating a Worker with pending (never-fired) setTimeout/setInterval
    entries does not crash the process and does not fire those callbacks after
    termination
  - two workers and the main thread each running their own independent
    setInterval(cb, 5) loops concurrently do not interfere with each other's
    firing order or counts (stress: 3 threads x 50 ticks each)
```

## 7. Open questions / deferrals

- **`Timeout[Symbol.toPrimitive]()` cross-thread precision.** The current `Handle` encoding (`gen:16 + slot:48`) may not fit losslessly in a JS `number`'s 53-bit safe-integer range when both fields are combined into one coerced value — needs verification against realistic slot/generation ranges, or a dedicated narrower id scheme just for this one coercion, before `Symbol.toPrimitive`/cross-thread `clearTimeout` can be marked done (§5.4).
- **Interaction between `ref()`/`unref()` and today's `drain_pending_timers`/interval-exclusion policy.** An explicitly `.ref()`'d `setInterval` should arguably keep the process alive indefinitely (matching Node), which conflicts with the current blanket "never wait for intervals at process exit" rule designed to avoid an infinite hang — needs an owner decision on whether RTS actually reproduces "a ref'd interval keeps the process running forever" (real Node behavior) or intentionally deviates for practical script-exit ergonomics, documented explicitly either way (§5.8 phase c).
- **`Symbol.dispose`/`using` end-to-end support.** Whether TC39 explicit resource management (`using`/`await using` declarations) is implemented anywhere in the RTS parser/engine yet is unverified independent of timers — if it is not, `timeout[Symbol.dispose]()`/`immediate[Symbol.dispose]()` can still be implemented as plain callable methods (usable via manual `t[Symbol.dispose]()`), but the `using t = setTimeout(...)` sugar itself is blocked on that unrelated, broader engine feature landing first.
- **Windows timer-resolution/coalescing fidelity.** No code-visible API difference exists to implement, but real firing jitter under `std::time::Instant` on Windows vs POSIX has not been measured for RTS's own pump loop — flagged as a "verify it's within Node's own observed tolerance" item, not a design gap.
- **Whether the hoisted timer/event-loop core (§5.7) lands directly in `rts-engine` or a new shared low crate below both `rts-std` and `rts-node`.** Either satisfies the "reachable by both, not duplicated" constraint; the choice affects only where `crates/.../timers/` physically lives, not the ABI/spec surface in this document. Left to whoever picks up §5.8 phase (a), consistent with the broader `docs/specs/rts-std-surface.md` restructuring already in flight.
- **`node:timers/promises` `scheduler.wait`/`scheduler.yield` stability.** These remain Node-side "1 - Experimental" as of Node 25 (unlike the rest of the module, which is Stable) — worth flagging in user-facing RTS docs/`rts.d.ts` comments the same way Node's own docs do, rather than presenting the whole submodule as uniformly Stable.
