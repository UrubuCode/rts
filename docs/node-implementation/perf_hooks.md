# node:perf_hooks

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:perf_hooks` |
| Node.js version | 25.x |
| Stability | **2 - Stable** — "provides an implementation of a subset of the W3C Web Performance APIs as well as additional APIs for Node.js-specific performance measurements." |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import { performance, PerformanceObserver, PerformanceEntry, PerformanceMark, PerformanceMeasure, PerformanceNodeEntry, PerformanceNodeTiming, PerformanceResourceTiming, PerformanceObserverEntryList, Histogram, IntervalHistogram, RecordableHistogram, createHistogram, monitorEventLoopDelay, timerify, eventLoopUtilization, constants } from 'node:perf_hooks'` (CJS: `const { performance, PerformanceObserver, ... } = require('node:perf_hooks')`). `timerify` and `eventLoopUtilization` are exported as **top-level functions since v25.2.0** — they are aliases of `performance.timerify`/`performance.eventLoopUtilization`, added at the module level for ergonomics; both forms must resolve to behaviorally identical implementations in RTS. |
| Globals exposed | `globalThis.performance` — the **same object identity** as `require('node:perf_hooks').performance` (Node.js guarantee, inherited from the Web Performance API where `performance` is a global). No other member of this module is a JS global — `PerformanceObserver`, `PerformanceEntry`, `Histogram`, etc. must be explicitly imported from `node:perf_hooks`, unlike in browsers where several of them are also globals. **RTS-specific note:** a minimal `globalThis.performance` (`now()`/`timeOrigin` only) already exists today as an always-on `.ts` prelude singleton (`crates/rts-shared/src/stdlib/performance.ts`, included unconditionally by every compiled program via `registry_build.rs`'s `PreludeTs` list) — this spec's job is to **extend that existing singleton in place**, not replace it. See §5.1/§5.6/§5.8. |

## 1. Purpose

`node:perf_hooks` exposes high-resolution timing instrumentation: a monotonic
clock (`performance.now()`), a timeline of named `mark`/`measure` entries (the
W3C User Timing API), a subscription mechanism for observing new timeline
entries as they appear (`PerformanceObserver`, covering Node-specific entry
types like `gc`/`http`/`net`/`dns`/`function` in addition to the Web-standard
`mark`/`measure`/`resource`), a function-call latency wrapper (`timerify`) that
records durations into a histogram, a standalone high-dynamic-range histogram
data structure (`createHistogram`/`Histogram`) reusable for any application
metric, event-loop lag sampling (`monitorEventLoopDelay`), and event-loop
utilization percentage (`eventLoopUtilization`). It is the primary
introspection/observability surface for both application code (custom timing
spans) and tooling (APM agents measuring GC pauses and event-loop health).

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class Performance extends EventEmitter` *(not exported directly — only its singleton instance `performance` is)*

| Member | Signature | Description |
|---|---|---|
| `performance.now()` | `(): number` | Milliseconds elapsed since `performance.timeOrigin`, monotonic, sub-millisecond float resolution. |
| `performance.timeOrigin` | `readonly timeOrigin: number` | Unix timestamp (ms) at which the process/timeline began (`now()` reads `0` at that instant). |
| `performance.mark(name[, options])` | `(name: string, options?: PerformanceMarkOptions): PerformanceMark` | Creates a `PerformanceMark` entry on the global timeline and returns it. `name` is **required** since v16.0.0. |
| `performance.measure(name[, startMarkOrOptions[, endMark]])` | `(name: string, startMarkOrOptions?: string \| PerformanceMeasureOptions, endMark?: string): PerformanceMeasure` | Creates a `PerformanceMeasure` entry spanning two points in time (by mark name, numeric timestamp, or `options`) and returns it. |
| `performance.clearMarks([name])` | `(name?: string): void` | Removes marks from the global buffer — all of them, or only the named one. No-op (never throws) if `name` does not match any existing mark. |
| `performance.clearMeasures([name])` | `(name?: string): void` | Same as `clearMarks` for measures. No-op on unmatched `name`. |
| `performance.clearResourceTimings([name])` | `(name?: string): void` | Removes `resource`-type entries from the global buffer — all of them, or only entries whose `name` (the resource URL) matches. |
| `performance.getEntries()` | `(): PerformanceEntry[]` | All buffered global-timeline entries, in chronological `startTime` order. |
| `performance.getEntriesByName(name[, type])` | `(name: string, type?: string): PerformanceEntry[]` | Filtered by `name`, optionally also by `entryType`. |
| `performance.getEntriesByType(type)` | `(type: string): PerformanceEntry[]` | Filtered by `entryType`. |
| `performance.eventLoopUtilization([utilization1[, utilization2]])` | `(utilization1?: EventLoopUtilization, utilization2?: EventLoopUtilization): EventLoopUtilization` | With no args: cumulative utilization since process start. With one prior snapshot: utilization *since* that snapshot. With two: the delta between them (`utilization2` earlier, `utilization1` later). |
| `performance.timerify(fn[, options])` | `<F extends (...args: any[]) => any>(fn: F, options?: TimerifyOptions): F` | Returns a wrapped version of `fn` that, on each call, records the call's wall-clock duration (nanoseconds) into `options.histogram` (if given) and emits a `'function'`-type `PerformanceEntry` on the global timeline. Preserves `fn`'s return value/throw behavior and arity. |
| `performance.markResourceTiming(timingInfo, requestedUrl, initiatorType, global, cacheMode, bodyInfo, responseStatus[, deliveryType])` | `(timingInfo: object, requestedUrl: string, initiatorType: string, global: object, cacheMode: '' \| 'local', bodyInfo: object, responseStatus: number, deliveryType?: string): PerformanceResourceTiming` | Internal-facing hook other subsystems (`fetch`, future `http`/`http2`/`net`/`dns`) call to append a `resource`-type entry. `deliveryType` defaults to `''`. |
| `performance.setResourceTimingBufferSize(maxSize)` | `(maxSize: number): void` | Caps the `resource`-entry buffer; default **250**. Once full, further resource entries fire `'resourcetimingbufferfull'` instead of being buffered until cleared. |
| `performance.toJSON()` | `(): object` | JSON-serializable snapshot of the `performance` object (own enumerable timing properties; exact shape not pinned by the Node docs — see §4). |
| `performance.nodeTiming` | `readonly nodeTiming: PerformanceNodeTiming` | The single `PerformanceNodeTiming` instance (Node-specific process/runtime milestones). |
| — (EventEmitter surface) | `.on/.once/.off('resourcetimingbufferfull', () => void)` | Inherited from `EventEmitter` — see §2.4. |

#### `class PerformanceEntry`

Base class of every timeline entry. **Constructor not exposed to user code** —
instances come only from `performance.mark/measure/markResourceTiming` or from
Node's own internal instrumentation (GC, `function`, `net`, `dns`, `http`,
`http2`).

| Member | Signature | Description |
|---|---|---|
| `entry.name` | `readonly name: string` | Entry label (mark/measure name, resource URL, function name, `"gc"`, etc., per entry type). |
| `entry.entryType` | `readonly entryType: string` | One of `'dns'`, `'function'`, `'gc'`, `'http2'`, `'http'`, `'mark'`, `'measure'`, `'net'`, `'node'`, `'resource'`. |
| `entry.startTime` | `readonly startTime: number` | High-resolution ms timestamp, relative to `performance.timeOrigin`. |
| `entry.duration` | `readonly duration: number` | Elapsed ms; `0` for a bare `mark` (a point-in-time event, not a span). |

#### `class PerformanceMark extends PerformanceEntry`

| Member | Signature | Description |
|---|---|---|
| `mark.detail` | `readonly detail: any` | The `options.detail` passed to `performance.mark()`, or `null`. |

`entryType` is always `'mark'`; `duration` is always `0`.

#### `class PerformanceMeasure extends PerformanceEntry`

| Member | Signature | Description |
|---|---|---|
| `measure.detail` | `readonly detail: any` | The `options.detail` passed to `performance.measure()`, or `null`. |

`entryType` is always `'measure'`.

#### `class PerformanceNodeEntry extends PerformanceEntry`

Base of every Node-specific (non-Web-standard) entry type (`gc`, `http`,
`http2`, `net`, `dns`, `function`). **Constructor not exposed.**

| Member | Signature | Description |
|---|---|---|
| `entry.detail` | `readonly detail: any` | Entry-type-specific structured data — see §3 for the shape per `entryType`. The stable, recommended access path (added v16.0.0). |
| `entry.flags` | `readonly flags: number` | **Deprecated since v19.0.0** — use `detail.flags` instead. Only meaningful when `entryType === 'gc'`. |
| `entry.kind` | `readonly kind: number` | **Deprecated since v19.0.0** — use `detail.kind` instead. Only meaningful when `entryType === 'gc'`. |

#### `class PerformanceNodeTiming extends PerformanceEntry`

Singleton, reached only via `performance.nodeTiming`. `name` is `"node"`,
`entryType` is `"node"`. **Constructor not exposed.**

| Member | Signature | Description |
|---|---|---|
| `nodeTiming.nodeStart` | `readonly nodeStart: number` | ms timestamp when the Node.js (RTS) process was initialized. |
| `nodeTiming.v8Start` | `readonly v8Start: number` | ms timestamp when the V8 platform was initialized. **RTS has no V8** — see §5.1 for the mapped meaning (RTS codegen-backend init). |
| `nodeTiming.environment` | `readonly environment: number` | ms timestamp when the Node.js (RTS) environment was initialized. |
| `nodeTiming.loopStart` | `readonly loopStart: number` | ms timestamp the event loop started, or `-1` if not yet started. |
| `nodeTiming.loopExit` | `readonly loopExit: number` | ms timestamp the event loop exited, or `-1` if not yet exited. |
| `nodeTiming.bootstrapComplete` | `readonly bootstrapComplete: number` | ms timestamp bootstrapping completed, or `-1` if not yet complete. |
| `nodeTiming.idleTime` | `readonly idleTime: number` | Cumulative ms the event loop has been idle within its event-provider (equivalent to the "idle" figure `eventLoopUtilization` derives from). |
| `nodeTiming.uvMetricsInfo` | `(): UvMetricsInfo` *(added v22.8.0/v20.18.0 — a method, despite the property-like doc heading)* | `{ loopCount, events, eventsWaiting }` — event-loop iteration/event counters. |

#### `class PerformanceResourceTiming extends PerformanceEntry`

Represents a completed network/resource fetch (`entryType: 'resource'`).
**Constructor not exposed** — created via `performance.markResourceTiming(...)`
(called internally by `fetch`/future `http`-family modules).

| Member | Signature | Description |
|---|---|---|
| `r.workerStart` | `readonly workerStart: number` | Timestamp immediately before dispatching the fetch request. |
| `r.redirectStart` | `readonly redirectStart: number` | Start time of the first redirect, or `0` if none. |
| `r.redirectEnd` | `readonly redirectEnd: number` | End time of the last redirect, or `0` if none. |
| `r.fetchStart` | `readonly fetchStart: number` | Timestamp immediately before the fetch starts. |
| `r.domainLookupStart` | `readonly domainLookupStart: number` | Timestamp before DNS lookup. |
| `r.domainLookupEnd` | `readonly domainLookupEnd: number` | Timestamp after DNS lookup completes. |
| `r.connectStart` | `readonly connectStart: number` | Timestamp before connection establishment. |
| `r.connectEnd` | `readonly connectEnd: number` | Timestamp after connection is established. |
| `r.secureConnectionStart` | `readonly secureConnectionStart: number` | Timestamp before the TLS handshake, or `0` if not secure. |
| `r.requestStart` | `readonly requestStart: number` | Timestamp before the first byte is requested from the server. |
| `r.responseEnd` | `readonly responseEnd: number` | Timestamp after the last byte of the response is received. |
| `r.transferSize` | `readonly transferSize: number` | Total octets transferred (headers + body + protocol overhead). |
| `r.encodedBodySize` | `readonly encodedBodySize: number` | Payload body size before decompression. |
| `r.decodedBodySize` | `readonly decodedBodySize: number` | Payload body size after decompression. |
| `r.toJSON()` | `(): object` | JSON-serializable snapshot of every property above plus the base `PerformanceEntry` fields. |

#### `class PerformanceObserverEntryList`

Passed as the first argument to a `PerformanceObserver` callback.
**Constructor not exposed.**

| Member | Signature | Description |
|---|---|---|
| `list.getEntries()` | `(): PerformanceEntry[]` | All entries this notification carries, chronologically ordered. |
| `list.getEntriesByName(name[, type])` | `(name: string, type?: string): PerformanceEntry[]` | Filtered subset. |
| `list.getEntriesByType(type)` | `(type: string): PerformanceEntry[]` | Filtered subset. |

#### `class PerformanceObserver`

| Member | Signature | Description |
|---|---|---|
| constructor | `new PerformanceObserver(callback: (list: PerformanceObserverEntryList, observer: PerformanceObserver) => void)` | Creates an observer, **not yet active** until `.observe(...)` is called. Throws `ERR_INVALID_ARG_TYPE` (since v18.0.0; was `ERR_INVALID_CALLBACK` before) if `callback` is not a function. |
| `PerformanceObserver.supportedEntryTypes` *(static)* | `readonly static supportedEntryTypes: string[]` | `['dns', 'function', 'gc', 'http2', 'http', 'mark', 'measure', 'net', 'node', 'resource']` (order not spec-guaranteed; RTS should ship the same set it actually supports — see §5.8 phasing for which are supported at each phase). |
| `observer.observe(options)` | `(options: PerformanceObserveOptions): void` | Subscribes. Exactly one of `options.type` / `options.entryTypes` must be given — passing both, or neither, throws (Node does not pin the exact error class in its prose; treat as `TypeError`, `(verify)` against Node source). `options.buffered` (default `false`) additionally delivers entries already on the global buffer at subscribe time. |
| `observer.disconnect()` | `(): void` | Unsubscribes; the observer stops receiving notifications. Idempotent. |
| `observer.takeRecords()` | `(): PerformanceEntry[]` | Returns and **empties** this observer's own pending-notification buffer (entries queued but not yet delivered to the callback). |

#### `class Histogram`

Base class — not directly constructible by user code (`createHistogram`
returns a `RecordableHistogram`; `monitorEventLoopDelay` returns an
`IntervalHistogram`; both extend `Histogram`).

| Member | Signature | Description |
|---|---|---|
| `histogram.count` | `readonly count: number` | Number of samples recorded. |
| `histogram.countBigInt` | `readonly countBigInt: bigint` | Same, as `bigint`. |
| `histogram.min` | `readonly min: number` | Minimum recorded value (nanoseconds, unless the histogram was fed application units via `record()`). |
| `histogram.minBigInt` | `readonly minBigInt: bigint` | Same, as `bigint`. |
| `histogram.max` | `readonly max: number` | Maximum recorded value. |
| `histogram.maxBigInt` | `readonly maxBigInt: bigint` | Same, as `bigint`. |
| `histogram.mean` | `readonly mean: number` | Mean of all recorded values. |
| `histogram.stddev` | `readonly stddev: number` | Standard deviation of all recorded values. |
| `histogram.exceeds` | `readonly exceeds: number` | Count of samples that exceeded the 1-hour-in-nanoseconds (`3_600_000_000_000`) maximum trackable threshold. |
| `histogram.exceedsBigInt` | `readonly exceedsBigInt: bigint` | Same, as `bigint`. |
| `histogram.percentiles` | `readonly percentiles: Map<number, number>` | Snapshot map of standard percentile → value. |
| `histogram.percentilesBigInt` | `readonly percentilesBigInt: Map<number, bigint>` | Same, as `bigint` values. |
| `histogram.percentile(p)` | `(p: number): number` | Value at percentile `p`, `p ∈ (0, 100]`. |
| `histogram.percentileBigInt(p)` | `(p: number): bigint` | Same, as `bigint`. |
| `histogram.reset()` | `(): void` | Clears all recorded data (count/min/max/mean/stddev/percentiles all reset to their empty-histogram defaults). |

#### `class IntervalHistogram extends Histogram`

Returned by `monitorEventLoopDelay()`. Samples event-loop delay on a
background timer at the configured `resolution`.

| Member | Signature | Description |
|---|---|---|
| `histogram.enable()` | `(): boolean` | Starts the sampling timer. Returns `true` if it was not already running, `false` if it was (idempotent no-op on the second call). |
| `histogram.disable()` | `(): boolean` | Stops the sampling timer. Returns `true` if it was running, `false` if already stopped. |
| `histogram[Symbol.dispose]()` | `(): void` | Calls `disable()`; supports `using histogram = monitorEventLoopDelay()`. |

#### `class RecordableHistogram extends Histogram`

Returned by `createHistogram()`. A general-purpose histogram user code (or
`timerify`) feeds manually — no background sampling.

| Member | Signature | Description |
|---|---|---|
| `histogram.record(val)` | `(val: number \| bigint): void` | Records `val`. Throws `RangeError` if `val` is not a positive safe integer / positive bigint. |
| `histogram.recordDelta()` | `(): void` | Records the nanosecond delta between now and the previous call to `recordDelta()` (the first call establishes the baseline and records nothing meaningful). |
| `histogram.add(other)` | `(other: RecordableHistogram): void` | Merges every recorded sample from `other` into `this`. |

### 2.2 Top-level functions

#### `perf_hooks.createHistogram([options])`
- **Params**: `options?: CreateHistogramOptions` — see §3.
- **Returns**: `RecordableHistogram`.
- **Throws**: invalid `lowest`/`highest`/`figures` — treated as `RangeError`/`ERR_OUT_OF_RANGE` per Node's general options-validation convention `(verify exact error class against Node source — docs prose does not pin it)`.
- **Variant**: sync (pure data-structure allocation).

#### `perf_hooks.monitorEventLoopDelay([options])`
- **Params**: `options?: MonitorEventLoopDelayOptions` — see §3.
- **Returns**: `IntervalHistogram` (created **disabled** — call `.enable()`).
- **Throws**: `resolution <= 0` — `(verify)`, treated as `RangeError`.
- **Variant**: sync (allocation); the actual sampling is asynchronous background work once `.enable()`'d.

#### `perf_hooks.timerify(fn[, options])` *(top-level alias since v25.2.0, mirrors `performance.timerify`)*
- **Params**: `fn: Function`, `options?: TimerifyOptions`.
- **Returns**: wrapped `Function` of the same arity/`this`-behavior as `fn`.
- **Throws**: `TypeError` if `fn` is not a function.
- **Variant**: sync wrapper; the wrapped function is sync/async depending on `fn` itself (an async `fn`'s returned Promise settling does not delay when the duration is recorded — the duration recorded is the synchronous call time, i.e. time to return the Promise, **not** time to Promise settlement — `(verify against Node source; documented example only covers wrapping an async function without clarifying which duration is captured)`).

#### `perf_hooks.eventLoopUtilization([utilization1[, utilization2]])` *(top-level alias since v25.2.0, mirrors `performance.eventLoopUtilization`)*
- **Params**: two optional prior `EventLoopUtilization` snapshots.
- **Returns**: `EventLoopUtilization` — `{ idle: number, active: number, utilization: number }` (ms, ms, ratio `0..1`).
- **Throws**: none documented (malformed snapshot args are tolerated/ignored per Node's general permissive style for this API — `(verify)`).
- **Variant**: sync.

### 2.3 Properties & constants

| Name | Type | Description |
|---|---|---|
| `perf_hooks.performance` | `Performance` | The singleton — identical object to `globalThis.performance`. |
| `perf_hooks.constants` | `PerfHooksConstants` | GC-related numeric constants (see §3) — kept for back-compat with the deprecated `flags`/`kind` properties; `detail.kind`/`detail.flags` on a `'gc'` entry hold values from this set. |

### 2.4 Events

#### `Performance` (the `performance` singleton) — `extends EventEmitter`

| Event | Signature | Fires |
|---|---|---|
| `'resourcetimingbufferfull'` | `(): void` | When the global `resource`-entry buffer (capacity set by `setResourceTimingBufferSize`, default 250) is full and a new resource entry would have been added. Typical handler calls `performance.clearResourceTimings()` to make room. |

#### `PerformanceObserver` callback (not a Node `'event'`, but the module's core notification mechanism)

| Callback | Signature | Fires |
|---|---|---|
| `callback` | `(list: PerformanceObserverEntryList, observer: PerformanceObserver) => void` | Asynchronously, after one or more new matching entries have been recorded, batched (not necessarily once per entry) `(verify exact scheduling granularity — Node's prose says only "the callback is invoked" without pinning micro/macrotask granularity — treat as a distinct queued notification task, not synchronous with `mark()`/`measure()`, in the RTS implementation)`. |

## 3. Types & option objects

```ts
interface PerformanceMarkOptions {
  /** Arbitrary data to attach; readable back via mark.detail. Default: null. */
  detail?: any;
  /** Custom timestamp (ms) instead of performance.now() at call time. */
  startTime?: number;
}

interface PerformanceMeasureOptions {
  detail?: any;
  /** ms; if given with only one of start/end, the other is derived. */
  duration?: number;
  /** Timestamp (ms, number) or a mark name (string) to end at. */
  end?: number | string;
  /** Timestamp (ms, number) or a mark name (string) to start at. */
  start?: number | string;
}

interface EventLoopUtilization {
  /** Cumulative ms the event loop was idle in the measured window. */
  idle: number;
  /** Cumulative ms the event loop was executing callbacks in the measured window. */
  active: number;
  /** active / (active + idle), in [0, 1]. */
  utilization: number;
}

interface TimerifyOptions {
  /** If given, timerify() records each call's duration (ns) here
   *  instead of / in addition to emitting a 'function' PerformanceEntry. */
  histogram?: RecordableHistogram;
}

interface CreateHistogramOptions {
  /** Lowest discernible value, integer > 0. Default: 1. */
  lowest?: number | bigint;
  /** Highest recordable value, integer >= 2 * lowest. Default: Number.MAX_SAFE_INTEGER. */
  highest?: number | bigint;
  /** Number of accuracy digits, 1-5. Default: 3. */
  figures?: number;
}

interface MonitorEventLoopDelayOptions {
  /** Sampling rate in ms, must be > 0. Default: 10. */
  resolution?: number;
}

interface PerformanceObserveOptions {
  /** Single entry type. Mutually exclusive with entryTypes. */
  type?: string;
  /** Multiple entry types. Mutually exclusive with type. */
  entryTypes?: string[];
  /** Also deliver entries already on the global buffer at subscribe time. Default: false. */
  buffered?: boolean;
}

type PerformanceObserverCallback =
  (list: PerformanceObserverEntryList, observer: PerformanceObserver) => void;

interface UvMetricsInfo {
  /** Number of event-loop iterations. */
  loopCount: number;
  /** Number of events processed by the event provider. */
  events: number;
  /** Number of events waiting to be processed when the provider was last checked. */
  eventsWaiting: number;
}

/** perf_hooks.constants — GC entry detail.kind / detail.flags values. */
interface PerfHooksConstants {
  NODE_PERFORMANCE_GC_MAJOR: number;
  NODE_PERFORMANCE_GC_MINOR: number;
  NODE_PERFORMANCE_GC_INCREMENTAL: number;
  NODE_PERFORMANCE_GC_WEAKCB: number;
  NODE_PERFORMANCE_GC_FLAGS_NO: number;
  NODE_PERFORMANCE_GC_FLAGS_CONSTRUCT_RETAINED: number;
  NODE_PERFORMANCE_GC_FLAGS_FORCED: number;
  NODE_PERFORMANCE_GC_FLAGS_SYNCHRONOUS_PHANTOM_PROCESSING: number;
  NODE_PERFORMANCE_GC_FLAGS_ALL_AVAILABLE_GARBAGE: number;
  NODE_PERFORMANCE_GC_FLAGS_ALL_EXTERNAL_MEMORY: number;
  NODE_PERFORMANCE_GC_FLAGS_SCHEDULE_IDLE: number;
}

/** entry.detail shape when entry.entryType === 'gc'. */
interface GcEntryDetail {
  kind: number;   // one of NODE_PERFORMANCE_GC_{MAJOR,MINOR,INCREMENTAL,WEAKCB}
  flags: number;  // bitmask of NODE_PERFORMANCE_GC_FLAGS_*
}

/** entry.detail shape when entry.entryType === 'http'. */
interface HttpEntryDetail {
  req: { method: string; url: string; headers: string[] };
  res: { statusCode: number; statusMessage: string; headers: string[] };
}

/** entry.detail shape when entry.entryType === 'http2' (Http2Stream). */
interface Http2StreamEntryDetail {
  bytesRead: number;
  bytesWritten: number;
  id: number;
  timeToFirstByte: number;
  timeToFirstByteSent: number;
  timeToFirstHeader: number;
}

/** entry.detail shape when entry.entryType === 'http2' (Http2Session). */
interface Http2SessionEntryDetail {
  bytesRead: number;
  bytesWritten: number;
  framesReceived: number;
  framesSent: number;
  maxConcurrentStreams: number;
  pingRTT: number;
  streamAverageDuration: number;
  streamCount: number;
  type: 'server' | 'client';
}

/** entry.detail shape when entry.entryType === 'function' (timerify). */
type FunctionEntryDetail = unknown[]; // the arguments the timed function was called with

/** entry.detail shape when entry.entryType === 'net' (connect). */
interface NetConnectEntryDetail {
  host: string;
  port: number;
}

/** entry.detail shape when entry.entryType === 'dns' (lookup). */
interface DnsLookupEntryDetail {
  hostname: string;
  family: number;
  hints: number;
  verbatim: boolean;
  addresses: string[];
}

/** entry.detail shape when entry.entryType === 'dns' (lookupService). */
interface DnsLookupServiceEntryDetail {
  host: string;
  port: number;
  hostname: string;
  service: string;
}

/** entry.detail shape when entry.entryType === 'dns' (queryXxx / getHostByAddr). */
interface DnsQueryEntryDetail {
  host: string;
  ttl: number;
  result: unknown;
}
```

## 4. Node semantics & edge cases

- **Units.** `performance.now()`, `.timeOrigin`, and every `PerformanceEntry.startTime`/`.duration` are **milliseconds** (sub-ms float). Every `Histogram` (from both `createHistogram` and `monitorEventLoopDelay`) records and reports in **nanoseconds**. This is a real, easy-to-miss unit mismatch inside one module — `timerify`'s recorded duration is nanoseconds even though the `'function'` `PerformanceEntry` it also emits has a millisecond `duration`.
- **`performance.measure()` errors.** Throws if the named `startMark` does not exist; throws if the named `endMark` does not exist. If `endMark` is omitted (2-arg form with a string `startMark`), it defaults to "now". Exact error class not pinned by Node's prose docs — treat as a generic `Error`/`SyntaxError`-shaped throw and confirm against Node's actual source before finalizing RTS's own error message/class.
- **`performance.mark()` name is required** (since v16.0.0 — earlier Node versions allowed an unnamed/`"default"` mark; RTS targets 25.x only, so implement the required-name form only). No reserved-name collision checking — a user can name a mark `"nodeStart"` and it simply coexists with (does not clobber) `nodeTiming.nodeStart`.
- **`clearMarks`/`clearMeasures`/`clearResourceTimings` are silent no-ops** when `name` matches nothing — never throw.
- **Resource timing buffer**: default capacity **250** entries (`setResourceTimingBufferSize`); once full, `'resourcetimingbufferfull'` fires instead of silently dropping or growing unbounded. `clearResourceTimings()` (with or without a name) is the standard way to make room again.
- **`PerformanceObserver.observe()` validation**: exactly one of `type`/`entryTypes` — passing both, or neither, is an error. `buffered: true` additionally replays already-buffered global entries matching the subscription into the *first* notification the observer receives after `observe()` is called.
- **Observer notification is asynchronous and can batch multiple entries** into one callback invocation — code must not assume one callback call per one `mark()`/`measure()`.
- **`GcEntryDetail`/`flags`/`kind` deprecation (v19.0.0)**: `performanceNodeEntry.flags`/`.kind` are the pre-v16 access path (now deprecated in favor of `.detail.kind`/`.detail.flags`); RTS should implement `.detail` as primary and mirror the same two numbers onto `.flags`/`.kind` for back-compat, not the reverse.
- **GC observation is opt-in-cost**: real Node/V8 only pays the GC-instrumentation overhead while at least one `'gc'`-type `PerformanceObserver` is actively subscribed (V8's GC prologue/epilogue callbacks are installed/removed dynamically) — this is the same "no active observer ⇒ no tracking cost" pattern as `async_hooks`'s destroy-hook optimization (see `docs/node-implementation/async_hooks.md` §4). RTS's collector hook (§5.7) must replicate this: gated on an active-observer count, not unconditionally recording every GC cycle.
- **Histogram value bounds**: `createHistogram({ highest })` must be `>= 2 * lowest`; `figures` in `[1, 5]`; a value that would exceed the 1-hour-in-nanoseconds (`3_600_000_000_000`) maximum trackable magnitude increments `.exceeds`/`.exceedsBigInt` instead of being recorded in the normal distribution. `percentile(p)`/`percentileBigInt(p)` require `p ∈ (0, 100]`.
- **`RecordableHistogram.record(val)`** requires `val` to be a positive safe integer (or positive `bigint`) — Node throws `RangeError` for zero/negative/non-integer input `(verify exact error class)`.
- **Cross-worker histogram cloning**: an `IntervalHistogram` sent across a `MessagePort` (`worker_threads`) arrives on the other side as a **plain `Histogram`** (read-only snapshot) — it does **not** implement `enable()`/`disable()`. RTS's structured-clone path for a `Histogram`-family handle must downgrade to the base-class shape on clone, matching this.
- **Platform differences**: no functional divergence between Windows and POSIX in the *documented* surface, but the **achievable precision of `monitorEventLoopDelay`'s `resolution`, and of `eventLoopUtilization`'s idle/active accounting, is platform-dependent** — Windows' default system timer granularity (~15.6 ms unless raised via `timeBeginPeriod`) can make a `resolution: 1`-ms sampling request report coarser real-world jitter than the same request on Linux/macOS. RTS should document this as an inherent platform characteristic, not a bug, exactly as the project's own benchmark notes already do for other timing-sensitive namespaces.
- **Security note**: unlike browser `performance.now()` (deliberately coarsened to mitigate timing side-channel attacks against cross-origin content), Node's server-side `performance.now()` is **not** artificially reduced in precision — RTS should not clamp it either (no analogous cross-origin threat model server-side).
- **`nodeStart`/`v8Start`/`environment`/`loopStart`/`loopExit`/`bootstrapComplete`** follow Node's own "`-1` sentinel if the milestone hasn't happened yet" convention — RTS should reuse that sentinel rather than `0`/`NaN`/`undefined`, since `0` is a legitimate real timestamp (process start).

## 5. RTS implementation notes

### 5.1 Native impl mapping

Most of this module's surface is **pure bookkeeping over one native primitive:
a monotonic nanosecond clock** — it needs almost no OS surface, similar to
`async_hooks` (§5.1 of that spec). Concretely:

- **`performance.now()`/`.timeOrigin`** already exist today (`std::time::Instant`
  + `std::time::SystemTime`, ms-granular) as the private `engine.now_ms()`/
  `engine.unix_ms()` bridges consumed by the always-on `.ts` prelude
  (`crates/rts-shared/src/stdlib/performance.ts`, wired in
  `crates/codegen-new/src/front/run/engineobj.rs`). This spec **extends that
  existing singleton class in place** — see §5.6/§5.8 for exactly how, and why
  it must not create a second, distinct `performance` object.
- **Marks, measures, `getEntries*`, `clearMarks/clearMeasures`,
  `PerformanceEntry`/`PerformanceMark`/`PerformanceMeasure`,
  `PerformanceObserver`/`PerformanceObserverEntryList`** need **no new native
  Rust surface at all** beyond a higher-resolution clock read (below) — they
  are ordinary JS objects (`{name, entryType, startTime, duration, detail}`)
  pushed into/filtered out of a plain array, exactly like `console`'s or
  `Map`'s `.ts`-only implementation. This is a `.ts`-shim-only slice of the
  module (owned by `rts-node`, not `rts-shared`, since it is `node:`-specific
  ergonomics — only the always-on Web-mandated `now()`/`timeOrigin` subset
  stays in the ambient prelude).
- **A nanosecond-precision monotonic clock** (`__RTS_FN_NODE_PERF_HOOKS_NOW_NS`,
  `std::time::Instant::now()` nanos since an origin — **not** the same
  ms-granular bridge `performance.now()` uses) is a genuine new native need,
  used internally by `timerify`'s duration recording and by the
  `IntervalHistogram` background sampler. Trivial, `rts-node`-local, no
  external crate.
- **`Histogram`/`IntervalHistogram`/`RecordableHistogram`** need a real
  log-linear/HDR-style histogram data structure — genuine new Rust code
  (either the `hdrhistogram` crate, matching Node's own use of the `hdr_histogram_c`
  library underneath, or a small hand-rolled bucketed histogram scoped to
  exactly this module's needs — see §5.8 for the recommendation). Lives
  entirely inside `rts-node`; no OS dependency beyond the clock above.
- **`IntervalHistogram.enable()`** starts a dedicated background `std::thread`
  that sleeps `resolution` ms per iteration and records the delta between
  expected and actual wake time (nanoseconds) into the histogram — see §5.3
  for why this is a *documented approximation* of "event loop delay" rather
  than a literal instrumentation of RTS's own callback-draining loop.
- **`'gc'`-type entries** need a callback hook fired by `rts-engine`'s
  mark+sweep collector (`crates/rts-runtime/src/namespaces/gc/collector.rs`'s
  `finish_cycle()`) — flagged as a cross-cutting prerequisite in §5.7 (mirrors
  the GC-root-registration flag `async_hooks.md` raised for the same
  collector).
- **`'http'`/`'http2'`/`'net'`/`'dns'`-type entries** need those modules
  (`node:http`, `node:http2`, `node:net`, `node:dns` — all P0/P1, not yet
  specced/built) to call this module's entry-append primitive at the right
  points; **`'resource'`-type entries** need whatever currently implements
  `fetch()` (today `rts-std`'s Web-global `fetch`) to call
  `performance.markResourceTiming(...)`. Both are deferred cross-module wiring
  — see §5.7/§5.8/§7.
- **`eventLoopUtilization()`** needs actual idle/active wall-clock time
  bookkeeping from whatever runs RTS's event loop (today `rts-std`'s
  `event_loop`, destined for the hoisted `rts-async` crate per
  `architecture.md` §3.2/§7) — flagged in §5.7.

### 5.2 ABI surface

All symbols `__RTS_FN_NODE_PERF_HOOKS_<NAME>`, registered under nodespace
`perf_hooks` (`ns_prefix = "node_perf_hooks"`) in `rts-node`'s own
`NodespaceSpec`/`NODE_SPECS` table (same pattern as `fs`/`path`/`os`/`process`/
`util`/`crypto` in `crates/rts-node/src/lib.rs`).

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_PERF_HOOKS_NOW_NS` | — | `U64` | Nanosecond monotonic clock, internal use only (`timerify`, histogram sampling) — **not** the public `performance.now()` (ms; already provided by the existing ambient prelude bridge, reused as-is, see §5.1/§5.6). |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_NEW` | `I64` (lowest), `I64` (highest), `I32` (figures) | `Handle` | Allocates a histogram record. Used by both `createHistogram` (`RecordableHistogram`, no background thread) and `monitorEventLoopDelay` (`IntervalHistogram`, background thread created but not started — see `HISTOGRAM_ENABLE`). |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_RECORD` | `Handle`, `U64` (value, ns) | `Void` | Throws-equivalent: value out of the configured `[lowest, highest]` bounds is clamped into `.exceeds` bookkeeping rather than a hard Rust panic — the `.ts` layer surfaces the `RangeError` for `val <= 0`/non-integer *before* calling this extern. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_RECORD_DELTA` | `Handle` | `Void` | Reads `NOW_NS`, computes delta since the histogram's internally stored `last_ns` (0 → no-op on first call), records it, updates `last_ns`. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_ADD` | `Handle` (dst), `Handle` (src) | `Void` | Merges `src`'s recorded distribution into `dst`. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_RESET` | `Handle` | `Void` | Clears all recorded data. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_COUNT` / `_MIN` / `_MAX` / `_MEAN` / `_STDDEV` / `_EXCEEDS` | `Handle` | `F64` | Statistics readers. `count`/`min`/`max`/`exceeds` are conceptually integers carried as `F64` (mirroring the `number`-typed JS getters); the BigInt-suffixed JS getters (`countBigInt`, etc.) reuse the same extern and box the result through the primordial `BigInt` tag in the `.ts` layer rather than needing a second symbol. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_PERCENTILE` | `Handle`, `F64` (p) | `F64` | `p ∈ (0,100]` validated in `.ts` before the call. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_PERCENTILES_DUMP` | `Handle`, `Handle` (pre-allocated output `Buffer`/`ArrayBuffer`) | `I32` (count written) | Writes `(percentile: f64, value: u64)` pairs into the caller-provided byte buffer; `.ts` turns the result into a `Map<number, number>` (or `Map<number, bigint>` for the BigInt getter). Kept as a bulk dump rather than N individual calls for the standard percentile set, for efficiency. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_ENABLE` | `Handle` | `Bool` | Starts (or no-ops if already running) the background sampling `std::thread` for an `IntervalHistogram`. Returns whether it actually transitioned stopped→running. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_DISABLE` | `Handle` | `Bool` | Stops the background thread (join or signal-and-detach — see §5.4). Returns whether it actually transitioned running→stopped. |
| `__RTS_FN_NODE_PERF_HOOKS_HISTOGRAM_FREE` | `Handle` | `Void` | Releases the histogram (and, for an `IntervalHistogram`, ensures its background thread is stopped first). Called from the `.ts` class's finalizer path / explicit `disable()`. |
| `__RTS_FN_NODE_PERF_HOOKS_GC_OBSERVE_BEGIN` | — | `Void` | Increments the process-wide active-`'gc'`-observer count that gates the collector hook (§5.7); called when the first `'gc'`-type `PerformanceObserver.observe()` subscribes. |
| `__RTS_FN_NODE_PERF_HOOKS_GC_OBSERVE_END` | — | `Void` | Decrements the same counter; called on `disconnect()`/last matching observer removed. |
| `__RTS_FN_NODE_PERF_HOOKS_GC_TAKE_PENDING` | `Handle` (pre-allocated output `Buffer`) | `I32` (count written) | Drains GC-cycle records the collector hook queued since the last drain (`kind`, `flags`, `startTime`, `duration` per record) — polled by the `.ts` `PerformanceObserver` dispatch loop (see §5.3) rather than the collector calling back into JS directly (keeps the collector's hook a cheap, allocation-free Rust-side append). |
| `__RTS_FN_NODE_PERF_HOOKS_ELU_SNAPSHOT` | — | via 2 out-params or a packed `U64`+`U64` return (`(idle_ns, active_ns)`) | Reads the cumulative idle/active counters described in §5.7. Placeholder/deferred implementation returns `(0, elapsed_since_process_start_ns)` until the real event-loop instrumentation lands — see §5.7/§7 (an honest, documented stand-in, not silently wrong data: `utilization` computes to a real, if approximate, `1.0` "always active" until real idle tracking exists — flagged, not hidden). |

Rich values (histogram records, the `IntervalHistogram`'s background-thread
handle) are opaque `Handle`s into a `HandleTable` entry owned by `rts-node`'s
own `perf_hooks` module — not `rts-engine`'s primordial `gc::Entry` enum (see
§5.6 and `architecture.md` §6 on `Entry::Backend`). Marks/measures/observer
entries themselves are **plain `.ts` objects**, not native handles at all —
see §5.1.

### 5.3 Async model

- **`performance.now/mark/measure/clearX/getEntries*`**: pure sync, no event
  loop involvement.
- **`PerformanceObserver` notification delivery**: Node's own docs do not pin
  the exact scheduling granularity (§4) — RTS should deliver notifications as a
  **microtask-queued callback** (drained at the same point the promise
  microtask queue drains, i.e. piggybacking the existing microtask-drain
  mechanism the promise subsystem already has, rather than inventing a new
  scheduling primitive) — batching all entries recorded since the observer's
  last notification into one `PerformanceObserverEntryList`. This needs the
  existing microtask-drain hook point (currently `rts-std`/promise subsystem,
  destined for `rts-async`) to also drain "pending perf observer
  notifications" each tick — a small, additive hook, not a new async primitive.
- **`'gc'`-type entries**: the collector hook (§5.2's `GC_TAKE_PENDING`) is
  polled, not pushed — the `.ts` `PerformanceObserver` dispatch loop calls
  `GC_TAKE_PENDING` at the same microtask-drain point as above, converts any
  drained records into `'gc'`-type `PerformanceEntry` objects, and delivers
  them to matching observers. This avoids the collector itself needing to call
  back into JS/the event loop mid-GC-cycle (which would be unsafe — GC cycles
  run with strict invariants about not allocating/reentering JS).
- **`IntervalHistogram` sampling (`monitorEventLoopDelay`)**: the background
  `std::thread` started by `enable()` samples **independently of the RTS event
  loop / JS execution** — it is a raw OS-thread sleep loop
  (`thread::sleep(resolution) ; record(now_ns() - expected_ns)`), which
  measures **OS scheduling jitter on a dedicated thread**, not literally "how
  long JS callbacks delay the next queued callback" the way libuv's mechanism
  does in real Node. This is a **documented approximation** — see §7. It
  requires no shared async infra (no tokio, no event loop) — just
  `std::thread`+`std::time`, entirely inside `rts-node`.
- **`timerify(fn)`**: synchronously wraps the call — reads `NOW_NS` before and
  after invoking `fn`, records the delta. If `fn` is itself `async`/returns a
  `Promise`, the recorded duration is the *synchronous* portion of the call
  (time to obtain the `Promise`, not time to its settlement) per the
  `(verify)` note in §2.2 — RTS should implement it this way (matching the
  literal reading of "wraps a function and records duration") and confirm
  against real Node's behavior with a dedicated test fixture (§6) before
  shipping, since this is a plausible-but-unconfirmed point.
- **`eventLoopUtilization()`**: sync read of cumulative counters — see §5.7 for
  what those counters need from the event loop itself.

### 5.4 Multithread / worker interaction

- **The global marks/measures/entries buffers, and the set of active
  `PerformanceObserver`s, are process-wide state**, not per-thread — matching
  real Node's single-JS-thread model where `performance` is one shared
  timeline. In RTS, where `promise.create`/timers may hop actual OS threads
  (tokio blocking-pool workers, per `docs/specs/async-promise-function.md`),
  this state must be a `Mutex`/lock-free-shard-guarded process-global (mirrors
  the `HandleTable`'s own 32-shard design), **not** `thread_local!` — unlike
  `async_hooks`'s context-frame stack (which is deliberately per-thread).
  `mark()`/`measure()` calls from any thread append to the same shared
  timeline.
- **`IntervalHistogram`'s background sampling thread** is one dedicated OS
  thread per histogram instance (not per-JS-thread) — created on `enable()`,
  stopped on `disable()`/finalization. It must be registered with
  `gc/thread_registry` (like every other RTS-spawned thread that might hold or
  produce GC-visible handles) even though the histogram's own recorded data is
  plain `u64` samples with no JS object references — required only if the
  sampling thread's `Handle` bookkeeping itself needs GC visibility (it does
  not hold `PolyValue`s, so this is a light registration, not a rooting
  concern, unlike `async_hooks`'s context stack).
- **Cross-worker histogram cloning**: when a `worker_threads` `MessagePort`
  structured-clones an `IntervalHistogram`, the receiving side must construct
  a **plain `Histogram`** (no `enable`/`disable`, no live background thread —
  the clone carries only the numeric distribution snapshot at clone time, not
  a live handle to the original's background thread). This maps directly onto
  RTS's `threadLocal`/`shared`/`channel` model
  (`docs/specs/rts-threading-model.md`): the clone promotes only the
  **recorded sample data** (plain numbers) to the shared heap / message
  payload, never the `Handle` to the source histogram's native background
  thread.
- **`node:worker_threads` interaction** (not yet specced): a `Worker`'s own
  `performance`/`PerformanceObserver` should observe entries from **that
  worker's own thread's activity** in real Node's actual model — but per the
  process-wide-buffer design above (chosen because RTS's own async model
  already hops OS threads for ordinary single-"JS-thread" work, unlike real
  Node), RTS's global timeline is naturally process-wide rather than
  per-worker. This divergence should be revisited once `node:worker_threads`
  has its own spec — see §7.

### 5.5 Buffer / TypedArray interop

- `PerformanceMark`/`PerformanceMeasure`/`PerformanceObserverEntryList`
  `.detail` accepts and returns **arbitrary JS values**, including
  `Buffer`/`TypedArray`/`ArrayBuffer` — this module never inspects or decodes
  `detail`, only stores and returns whatever `PolyValue` it was given (same
  pattern as `async_hooks`'s `store`/`resource`, §5.5 of that spec).
- `HISTOGRAM_PERCENTILES_DUMP` (§5.2) is the one place this module does its
  own byte-level marshalling: it writes raw `(f64, u64)` pairs into a
  caller-provided `ArrayBuffer`-backed scratch buffer (allocated by the `.ts`
  layer via the primordial `ArrayBuffer`/typed-array surface) rather than
  allocating N individual boxed numbers Rust-side — a deliberate efficiency
  choice for a method (`.percentiles`) that returns a full standard-percentile
  distribution on every read.

### 5.6 Doctrine placement

`node:perf_hooks` is **entirely non-primordial** — it defines no new value tag,
trap, or memory representation; it is bookkeeping over the existing primordial
`Function`/`Object`/`ArrayBuffer` surface plus one native clock/histogram data
structure. The engine's front end never names `perf_hooks`, `Performance`,
`PerformanceObserver`, `Histogram`, or any other member of this module anywhere
in `crates/rts-codegen-new/`. A `node:perf_hooks` import maps via
`ns_prefix_for("node:perf_hooks")` (data lookup in `NODE_SPECS`,
`crates/rts-node/src/lib.rs`) to the codegen prefix `node_perf_hooks`; calls
resolve generically through `node_lookup`, the same one path every other
`node:` module already uses — zero special-case control flow added to the
engine.

**The one doctrine-relevant subtlety this module has that most `node:` modules
don't**: it must extend an **already-existing always-on ambient global**
(`performance`, from `rts-shared/src/stdlib/performance.ts`, included in
*every* compiled program regardless of whether `node:perf_hooks` is ever
imported) rather than introduce a brand-new one. Two clean, doctrine-compatible
ways to do this were considered:

1. **(Chosen) Extend the same singleton in place.** `.ts` shim text is a
   codegen-time source-inclusion concern, not a Cargo/crate dependency — the
   `rts-node` `.ts` shim for `node:perf_hooks`, when actually imported, adds
   its methods (`mark`, `measure`, `clearMarks`, `getEntries*`,
   `eventLoopUtilization`, `timerify`, `markResourceTiming`,
   `setResourceTimingBufferSize`, `toJSON`, the `nodeTiming` getter) directly
   onto the **same** `Performance` prototype/instance the always-on prelude
   already defines (prototype augmentation, or the prelude class itself grows
   these methods gated so their native-extern calls are only reachable once
   `rts-node`'s symbols are linked in — exact mechanism is an implementation
   decision for whoever lands it, see §5.8). This preserves
   `require('node:perf_hooks').performance === globalThis.performance` for
   every program, matching real Node, **without** `rts-node` gaining a Cargo
   dependency on `rts-shared` (the coupling is at the generated-TS-source
   level, which the codegen's own module system already mediates, exactly like
   how multiple `.ts` preludes/shims are concatenated today).
2. **(Rejected) Two distinct objects.** A separate `rts-node`-owned
   `performance` that shadows the ambient one only inside files that import
   `node:perf_hooks`. Rejected because it breaks object identity the instant a
   program holds a reference to `globalThis.performance` before also importing
   `node:perf_hooks` — a real, non-hypothetical bug class (a shared utility
   module calling `performance.now()` without itself importing
   `node:perf_hooks`, while `main.ts` does import it, must still observe
   `main.ts`'s marks).

`PerformanceObserver`/`PerformanceEntry`/`PerformanceMark`/`PerformanceMeasure`/
`PerformanceNodeEntry`/`PerformanceNodeTiming`/`PerformanceResourceTiming`/
`PerformanceObserverEntryList`/`Histogram`/`IntervalHistogram`/
`RecordableHistogram` are pure `.ts` classes shipped from `rts-node`'s own
`.ts` shim layer (per `architecture.md` §10), delegating their irreducible
operations to the `__RTS_FN_NODE_PERF_HOOKS_*` externs in §5.2. None of them
are global — they exist only inside a file that actually
`import`s `node:perf_hooks`.

### 5.7 Shared-infra dependencies (FLAG)

- **GC collector hook (`'gc'`-type entries).** `rts-engine`'s mark+sweep
  collector (`crates/rts-runtime/src/namespaces/gc/collector.rs::finish_cycle`)
  needs a new, cheap extension point: an atomic "active `'gc'`-observer count"
  gate (incremented/decremented by `GC_OBSERVE_BEGIN`/`_END`, §5.2) and, when
  that count is `> 0`, an append of `{kind, flags, start_ns, duration_ns}` to a
  small lock-free ring buffer this module later drains via `GC_TAKE_PENDING`.
  This is infrastructure `rts-node` cannot add to itself since the collector
  lives in `rts-engine`, below it in the dependency graph — same category of
  ask as `async_hooks.md`'s GC-root-registration flag, coordinate with whoever
  owns the collector.
- **Event-loop idle/active time accounting (`eventLoopUtilization`).** RTS's
  current event-loop draining (`rts-std`'s `event_loop`, destined for the
  hoisted `rts-async` crate per `architecture.md` §3.2/§7) does not today track
  cumulative idle-vs-active wall-clock time at all — it is not a persistent
  reactor loop the way libuv is, but a drain invoked at specific points. This
  module needs the event-loop owner to expose two cumulative counters (`idle_ns`,
  `active_ns`, atomically updated) that `ELU_SNAPSHOT` (§5.2) reads. **Until
  this lands, `eventLoopUtilization()` is implementable only as the documented
  placeholder in §5.2** (reports "always active", i.e. `utilization ≈ 1.0`) —
  an honest, flagged approximation, not silently wrong data.
- **Microtask-drain hook for `PerformanceObserver` delivery.** §5.3's chosen
  delivery mechanism (piggyback the existing microtask-drain point) needs that
  drain loop (today in `rts-std`/the promise subsystem, destined for
  `rts-async`) to also call this module's "flush pending observer
  notifications" entry point once per drain — an additive hook, small, but
  still a cross-crate coordination point since `rts-node` cannot itself own or
  modify that drain loop.
- **`'resource'`-type entries / `markResourceTiming` integration.** Whatever
  currently implements the `fetch()` Web global (today `rts-std`) needs to call
  `performance.markResourceTiming(...)` on request completion. Because
  `performance` itself lives in the always-on ambient prelude (§5.6), this is a
  **plain JS-level call**, not a Cargo dependency — no crate-graph change
  needed, just a wiring task in whichever module owns `fetch()`.
- **`'http'`/`'http2'`/`'net'`/`'dns'`-type entries.** Depend on those `node:`
  modules (themselves not yet specced) calling this module's entry-append path
  at the right native call sites — pure intra-`rts-node` coupling once those
  modules exist (no cross-crate issue), deferred until they are built.
- If none of the above is wired, this module still works correctly for its
  **core surface**: `now`/`mark`/`measure`/`clearX`/`getEntries*`,
  `PerformanceObserver` for `'mark'`/`'measure'` entries, `createHistogram`/
  `RecordableHistogram`, `timerify`, and `monitorEventLoopDelay` (which needs
  only `std::thread`, no shared infra at all) — only `'gc'`/`'resource'`/
  `'http'`/`'http2'`/`'net'`/`'dns'` entry types and real (non-placeholder)
  `eventLoopUtilization()` are gated on the flags above.

### 5.8 Implementation phases

a. **Nanosecond clock + core timeline (`.ts`-only slice)**: `NOW_NS` extern;
   `.ts` `PerformanceEntry`/`PerformanceMark`/`PerformanceMeasure` classes;
   `mark`/`measure`/`clearMarks`/`clearMeasures`/`getEntries`/
   `getEntriesByName`/`getEntriesByType` added onto the **existing** ambient
   `performance` singleton in place (§5.6, decision 1) — ship and test this
   slice purely synchronously first, no observer yet.
b. **`PerformanceObserver` for `'mark'`/`'measure'`**: `.ts`
   `PerformanceObserver`/`PerformanceObserverEntryList` classes; wire the
   microtask-drain delivery point (§5.3) — if the shared drain hook (§5.7)
   isn't landed yet, a temporary `rts-node`-local polling loop (checked on
   every `mark()`/`measure()` call, not truly async-scheduled) is an acceptable
   interim, documented as such, upgraded once the real hook lands.
c. **`Histogram`/`RecordableHistogram`/`createHistogram`**: pick and land the
   histogram data structure (`hdrhistogram` crate vs. hand-rolled — decide
   before starting, see §7); `HISTOGRAM_NEW/_RECORD/_RECORD_DELTA/_ADD/_RESET/
   _COUNT/_MIN/_MAX/_MEAN/_STDDEV/_EXCEEDS/_PERCENTILE/_PERCENTILES_DUMP/_FREE`
   externs + `.ts` class.
d. **`timerify`** (both `performance.timerify` and the top-level
   `perf_hooks.timerify` alias): wraps a `Function`, records via `NOW_NS`,
   optionally into a `RecordableHistogram`, always emits a `'function'`
   `PerformanceEntry`. Confirm the async-duration-semantics `(verify)` note in
   §5.3 with a real-Node comparison fixture before finalizing.
e. **`IntervalHistogram`/`monitorEventLoopDelay`**: background `std::thread`
   sampler, `enable`/`disable`/`Symbol.dispose`; needs no shared infra (§5.7) —
   a good early, self-contained phase.
f. **`eventLoopUtilization`** (both `performance.eventLoopUtilization` and the
   top-level alias): ship the documented placeholder (§5.2/§5.7) first
   (function exists, returns real-shaped-but-approximate data, `utilization ≈
   1.0`), flip to the real idle/active counters once the event-loop owner
   lands them — track as a follow-up, not a blocker for shipping the rest of
   the module.
g. **`PerformanceNodeTiming`/`performance.nodeTiming`**: record
   `nodeStart`/`v8Start`(→ RTS codegen-backend init, see §4's note that RTS has
   no literal V8)/`environment`/`bootstrapComplete` timestamps at the
   corresponding points in RTS's own process-startup sequence (a new, small
   instrumentation task in whatever owns `main.rs`/engine bootstrap — flag if
   it turns out to need its own cross-crate coordination beyond what's already
   listed); `loopStart`/`loopExit`/`idleTime`/`uvMetricsInfo` depend on the
   same event-loop instrumentation as (f) — ship with `-1`/empty-stats
   sentinels until that lands, per §4's sentinel convention.
h. **GC collector hook + `'gc'`-type entries**: land the collector extension
   point (§5.7) — coordinate with the GC/collector owner; gate on active-
   observer count (§4's "opt-in cost" note) so the common case (no one
   observing `'gc'`) has zero added overhead.
i. **`PerformanceResourceTiming`/`markResourceTiming`/`'resourcetimingbufferfull'`
   /`setResourceTimingBufferSize`**: `.ts` class + buffer-capacity logic;
   coordinate with whoever owns `fetch()` to actually call
   `markResourceTiming(...)` on completion (§5.7) — until wired, the class and
   buffer machinery can still be tested by calling `markResourceTiming`
   directly in a test fixture.
j. **`'http'`/`'http2'`/`'net'`/`'dns'`-type entries**: deferred until those
   `node:` modules exist and are wired to call this module's entry-append path
   (§5.7/§7) — lowest priority slice, tracked but not blocking.
k. **Cross-worker histogram clone downgrade** (§5.4): implement once
   `node:worker_threads`/structured-clone-over-`MessagePort` exists — the
   "plain `Histogram`, no live thread" downgrade rule documented in §4/§5.4.
l. **Test fixtures + cross-runtime measurement** (§6).

## 6. Test plan

`tests/node_perf_hooks_*.test.ts` (`rts:test` format):

- **`now()` monotonic**: two consecutive `performance.now()` calls, second
  `>=` first; both are `number`s `>= 0`.
- **`timeOrigin` sanity**: `performance.timeOrigin` is a positive number close
  to the real wall-clock epoch at process start (within a generous tolerance).
- **Identity with `globalThis.performance`**: `performance === globalThis.performance`
  is `true`, and — critically — a module that reads `globalThis.performance`
  **before** any `import { performance } from 'node:perf_hooks'` executes
  elsewhere in the same program still observes marks added after that import
  runs (exercises §5.6's chosen "extend in place" design directly).
- **`mark`/`measure` basic**: `performance.mark('a')`; `performance.mark('b')`;
  `performance.measure('a-to-b', 'a', 'b')` returns a `PerformanceMeasure` with
  `duration >= 0` and `entryType === 'measure'`.
- **`measure` with options object**: `{ start: 'a', end: 'b', detail: {x: 1} }`
  form; `{ start: 0, duration: 10 }` numeric-timestamp form.
- **`measure` missing mark throws**: `performance.measure('x', 'doesNotExist')`
  throws.
- **`mark` name required**: calling `performance.mark()` with no argument is a
  compile-time/type error in TS — for the runtime-level test, call it via a
  dynamically-typed path and assert a throw.
- **`clearMarks`/`clearMeasures` no-op on missing name**: calling with a
  nonexistent name does not throw and does not affect other entries.
- **`getEntries`/`getEntriesByName`/`getEntriesByType`**: create a mix of
  marks/measures, assert each filter returns exactly the expected subset in
  chronological order.
- **`PerformanceObserver` basic**: `observe({ entryTypes: ['mark'] })`, then
  `performance.mark('x')`, assert the callback eventually receives an entry
  list containing exactly that mark (async — use the test framework's async
  support / a `Promise` the fixture awaits with a bounded timeout).
- **`PerformanceObserver` `buffered: true`**: mark **before** constructing the
  observer, then `observe({ type: 'mark', buffered: true })`, assert the first
  notification includes the pre-existing mark.
- **`PerformanceObserver.observe` validation**: both `type` and `entryTypes`
  given → throws; neither given → throws.
- **`observer.disconnect()`**: after disconnecting, further marks do not
  trigger the callback.
- **`observer.takeRecords()`**: records queued but not yet delivered are
  returned and the internal buffer is emptied (a second immediate call
  returns `[]`).
- **`createHistogram` + `record`**: record several values, assert
  `count`/`min`/`max`/`mean` are consistent; `reset()` zeroes them all.
- **`createHistogram` bigint getters**: `countBigInt`/`minBigInt`/`maxBigInt`
  match the `number` getters' values as `bigint`.
- **`createHistogram` invalid options**: `highest < 2 * lowest` throws;
  `figures` outside `[1,5]` throws.
- **`histogram.percentile` range validation**: `percentile(0)` and
  `percentile(101)` both throw; `percentile(50)`/`percentile(100)` succeed.
- **`histogram.record` invalid value**: `record(-1)`/`record(0)`/`record(1.5)`
  throw `RangeError`.
- **`histogram.recordDelta()`**: two calls with a known sleep between them
  record a value close to the sleep duration (nanoseconds), within tolerance.
- **`histogram.add`**: merge two histograms, assert the merged `count` is the
  sum and `max` is the max of the two.
- **`monitorEventLoopDelay` lifecycle**: `enable()` returns `true` then
  `false` on a second call; after some elapsed time, `count > 0`; `disable()`
  returns `true` then `false` on a second call; after `disable()`, `count`
  stops increasing.
- **`monitorEventLoopDelay` with `using`**: `using h = monitorEventLoopDelay();`
  — assert `h.enable()` was implicitly effective and disposal calls
  `disable()` (if the language surface supports `using`/`Symbol.dispose` —
  otherwise test the manual `enable()`/`disable()` pair only, per the §7
  language-feature caveat shared with `async_hooks`).
- **`timerify` basic**: wrap a plain sync function, call it several times,
  assert the wrapped function's return values are unchanged and a
  `RecordableHistogram` passed via `options.histogram` accumulates `count`
  matching the number of calls.
- **`timerify` emits `'function'` entries**: observe `entryTypes: ['function']`,
  call the timerified function, assert an entry arrives whose `name` matches
  the function and whose `detail` is the call's argument array.
- **`timerify` on an async function**: assert the recorded duration reflects
  the synchronous-call time per the §5.3 `(verify)` resolution, once
  confirmed against real Node.
- **`eventLoopUtilization` shape**: no-arg call returns
  `{idle, active, utilization}` with `utilization` in `[0, 1]`; two-snapshot
  delta form returns a smaller/different value than the cumulative form after
  some busy work runs in between (once the real counters land — mark as
  expected-approximate/skip-assert-on-exact-value until phase (f) of §5.8).
- **`PerformanceNodeTiming` sanity**: `performance.nodeTiming.nodeStart >= 0`;
  `loopStart`/`loopExit`/`bootstrapComplete` are either `-1` or a sane
  timestamp, never `NaN`/`undefined`.
- **`nodeTiming.uvMetricsInfo()`**: returns an object with numeric
  `loopCount`/`events`/`eventsWaiting`.
- **`resourcetimingbufferfull` + `setResourceTimingBufferSize`**: set a tiny
  buffer size, trigger enough `markResourceTiming` calls (directly, in the
  test, simulating what `fetch()` will eventually do) to exceed it, assert the
  event fires; `clearResourceTimings()` in the handler allows further entries.
- **`PerformanceResourceTiming.toJSON()`**: returns an object containing every
  documented property.
- **`constants` sanity**: `perf_hooks.constants.NODE_PERFORMANCE_GC_MAJOR`
  etc. are distinct numeric values.
- **GC entries** (once §5.8 phase h lands): `observe({ entryTypes: ['gc'] })`,
  force allocation pressure that triggers a collector cycle, assert a `'gc'`
  entry arrives with a valid `detail.kind`/`detail.flags`; assert **no**
  entries arrive when no `'gc'` observer is active (cost-gating check, best
  verified indirectly via a perf/counter assertion rather than timing).
- **Multithread**: a `RecordableHistogram` created on the main thread and
  passed (once cross-thread histogram cloning / `worker_threads` exists) into
  a worker via `MessagePort` arrives as a plain `Histogram` — assert
  `typeof clonedHistogram.enable` is `undefined` (or calling it throws), while
  `count`/`percentile(...)` still reflect the snapshot at clone time. A second
  fixture asserts marks/measures created from RTS's own OS-thread-hopping
  async machinery (a `promise`/`await` body actually executing on a tokio
  blocking-pool worker thread) still land on the **same shared** global
  timeline as marks created on the main thread (exercises §5.4's
  process-wide-not-per-thread buffer design).

## 7. Open questions / deferrals

- **Exact error classes for measure()/observe()/createHistogram()/record()
  validation failures.** Node's prose documentation does not pin `TypeError`
  vs `RangeError` vs a custom `ERR_*` code for several validation paths (flagged
  `(verify)` throughout §2/§4) — needs a quick check against Node's actual
  source (`lib/internal/perf/*.js`) before finalizing RTS's thrown-error
  classes/messages, since test fixtures comparing cross-runtime behavior will
  need to match exactly.
- **`timerify`'s recorded-duration semantics for an `async fn`** (§5.3): is it
  the synchronous call time (time to return the `Promise`) or the time to
  settlement? The docs' own example (timing a dynamic `import()`) reads as if
  it measures the *whole* async operation, which would contradict "duration is
  the synchronous call" — this needs empirical confirmation against real Node
  before phase (d) of §5.8 ships, since it changes whether `timerify`'s
  histogram hook needs a `.then()`-chained completion callback (making it
  genuinely async-aware) instead of a simple before/after wrap.
- **`eventLoopUtilization` real semantics without a libuv-shaped reactor**:
  RTS's event loop is not a persistent reactor the way Node's is; §5.7's
  proposed idle/active counters are a reasonable-but-not-yet-designed
  approximation. Needs sign-off from whoever owns the (currently `rts-std`,
  future `rts-async`) event-loop implementation on exactly what "idle" means
  in RTS's execution model before the real (non-placeholder) version ships.
- **`monitorEventLoopDelay`'s background-thread approach measures OS jitter on
  a dedicated thread, not literal JS-callback-queue delay** (§5.3) — this is
  the pragmatic RTS-shaped answer given no libuv-equivalent exists, but it is
  a documented **semantic divergence** from real Node worth flagging
  prominently in the module's own runtime docs/comments, not just this spec,
  so a future contributor does not "fix" it into something that silently
  breaks the approximation's honesty.
- **Histogram data-structure choice**: adopt the `hdrhistogram` crate
  (crates.io) or hand-roll a small bucketed histogram scoped to exactly this
  module's `(lowest, highest, figures)` contract? The former is
  well-tested and mirrors Node's own use of HDR histogram semantics; the
  latter avoids a new dependency. Needs an owner decision before phase (c) of
  §5.8.
- **Per-worker vs process-wide global timeline** (§5.4): this spec proposes a
  process-wide timeline (matching RTS's OS-thread-hopping async model more
  naturally than a strict per-"JS-thread" model would). Revisit once
  `node:worker_threads` has its own spec, in case its thread/region lifecycle
  mapping (`docs/specs/rts-threading-model.md`) argues for a different
  boundary (e.g. does a `Worker`'s `PerformanceObserver` see the main thread's
  marks too, or only its own — real Node's answer is "only its own", which the
  process-wide design here does not currently reproduce).
- **`using`/explicit resource management for `IntervalHistogram[Symbol.dispose]`**:
  same open prerequisite question `async_hooks.md` raised for `RunScope` —
  needs a yes/no on parser/HIR support for `using` before shipping the disposer
  form; the manual `enable()`/`disable()` pair works regardless and should ship
  first either way.
- **`PerformanceObserver.supportedEntryTypes` exact contents/order at each
  implementation phase**: should this static property reflect only the entry
  types RTS *actually* supports at the current phase (honest, but diverges
  from Node's always-full list), or the full Node-parity list regardless of
  what's wired yet (matches Node, but a program could `observe()` an
  `entryType` that will simply never fire)? Recommend the former
  (phase-accurate) with a tracking note, consistent with this project's
  "the parity number stays real" floor.
- **V8-specific fields with no RTS equivalent**: none identified in this
  module's surface beyond `nodeTiming.v8Start` (§4/§5.8g, mapped to RTS
  codegen-backend init time) — no other member of `perf_hooks` assumes V8
  internals, unlike `node:v8`/`node:vm`/`node:inspector` (see
  `architecture.md` §11).
