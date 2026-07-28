# node:test

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:test` (+ `node:test/reporters` entry point) |
| Node.js version | 25.x |
| Stability | 2 - Stable (core runner: test/describe/it/hooks/mock/run); 1.1 - Active development (snapshot testing, watch mode, `mock.module`); 1 - Experimental (`--experimental-test-coverage` code coverage collection) — sub-feature stability differs, see per-item notes in §2/§4 |
| Tier | P1 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import { test, describe, it, before, after, beforeEach, afterEach, run, mock, snapshot, assert as testAssert } from "node:test"` · `import test from "node:test"` (default export is the `test` function itself, with `.skip`/`.todo`/`.only`/`.expectFailure` attached) · `const test = require("node:test")` · `import { spec, tap, dot, junit, lcov } from "node:test/reporters"` · CLI: `node --test [glob...]` (RTS: `rts --test` / `rts node --test`, see §5.8) |
| Globals exposed | **Only** when the process is launched via the `--test` CLI flag: `test`, `describe`, `it`, `before`, `after`, `beforeEach`, `afterEach` become ambient globals (no import needed) inside every file the runner loads. Outside `--test` mode, `node:test` exposes **no** globals — everything must be explicitly imported. |

## 1. Purpose

`node:test` is Node's built-in test runner: a small BDD-ish suite/test
declaration API (`test`/`it`/`describe`/`suite` + lifecycle hooks), a
first-class mocking layer (`mock.fn`/`mock.method`/`mock.module`/`mock.timers`),
a programmatic `run()` entry point that streams structured test events, a set
of built-in reporters (spec/tap/dot/junit/lcov), and auxiliary features
(snapshot testing, code coverage, watch mode). It is the module a TS/JS
project reaches for to write and execute its own test suite without a third
party framework (Jest/Mocha/Vitest). For RTS this is a large, mostly
**orchestration-level** module — almost the entire surface is bookkeeping
(who is `only`, what is the full hierarchical name, did the plan match) over
values/functions the engine already understands, with a handful of narrow
native needs (spawning per-file child processes for isolation, reading child
stdout, filesystem access for snapshots/watch, a monotonic clock for
timeouts/durations).

## 2. Exported API surface (COMPLETE)

### Classes

#### `TestContext`

Not constructed directly by user code — passed as the first argument to every
test function (`test(name, (t) => {...})`) and to hook callbacks. Not an
`EventEmitter`, not a stream.

- **Methods:**
  - `before(fn?, options?): void` — register a hook that runs once before the
    first subtest of *this* test/suite; `fn: (ctx: TestContext) => void |
    Promise<void>`, `options: HookOptions`.
  - `beforeEach(fn?, options?): void` — runs before each subtest of this
    test/suite; same param shapes.
  - `after(fn?, options?): void` — runs once after all subtests complete
    (guaranteed to run even if a subtest throws).
  - `afterEach(fn?, options?): void` — runs after each subtest completes.
  - `diagnostic(message: string): void` — emits a `test:diagnostic` event /
    TAP `# message` comment line; purely informational, never affects
    pass/fail.
  - `plan(count: number, options?: PlanOptions): void` — declares the exact
    number of assertions **and** subtests expected to run under this test;
    if the actual count differs when the test function returns/settles, the
    test fails with a plan-mismatch `ERR_TEST_FAILURE`.
  - `runOnly(shouldRunOnlyTests: boolean): void` — toggles, at the subtest
    level, whether only `{ only: true }`-marked subtests run for the
    remainder of this test/suite's subtest declarations.
  - `skip(message?: string): void` — marks the *currently running* test as
    skipped (called from inside the test body, as opposed to the `skip`
    option passed at declaration time).
  - `todo(message?: string): void` — marks the currently running test as
    TODO (same in-body vs. declaration-time distinction as `skip`).
  - `test(name?, options?, fn?): Promise<void>` — declares and runs a
    **subtest**; identical signature family to the top-level `test()`.
    Callers **must** `await` (or otherwise chain) the returned promise —
    an un-awaited subtest still still running when the parent test function
    returns is canceled and reported as a failure of the parent.
  - `waitFor(condition: () => boolean | Promise<boolean>, options?:
    WaitForOptions): Promise<void>` — polls `condition` at `options.interval`
    until it returns/resolves truthy or `options.timeout` elapses (rejects on
    timeout). *(exact interval/timeout defaults: verify against Node source
    at implementation time — not fully specified in the fetched documentation
    excerpt.)*
- **Properties:**
  - `filePath: string` — absolute path of the file this test belongs to.
  - `fullName: string` — hierarchical name (`"parent > child > this test"`).
  - `name: string` — this test's own declared name.
  - `passed: boolean` — whether the test ultimately passed (readable inside
    `after`/from a reporter-facing wrapper; not meaningful mid-execution).
  - `error: Error | undefined` — the failure reason, if any.
  - `attempt: number` — current retry attempt number (relevant once retry
    support is exercised; `0`/`1`-based — verify exact indexing against Node
    source).
  - `workerId: number` — id of the isolated worker/child-process running this
    test file (meaningful only under `isolation: 'process'`).
  - `signal: AbortSignal` — fires when the test's own timeout elapses or an
    ancestor/`run()`-level `signal` aborts; test code should pass this into
    any cancelable operation it starts.
  - `mock: MockTracker` — a `MockTracker` instance **scoped to this test**;
    every mock created through it is automatically restored when the test
    finishes (unlike the module-level `mock` singleton, which persists until
    explicitly reset/restored).
  - `assert: { snapshot(value, options?): void; fileSnapshot(value, path,
    options?): void }` — snapshot-testing entry points bound to this test
    (see §2 Top-level functions / `snapshot` and §3 `SnapshotOptions`); also
    the target of any custom assertion registered via `assert.register(name,
    fn)` (appears as `context.assert[name]`).
- **Events:** none (plain object, not an `EventEmitter`).

#### `SuiteContext`

Passed as the first argument to `describe`/`suite` callback functions.
Deliberately a much smaller surface than `TestContext` — a suite body is
expected to only *declare* tests/hooks (via the top-level `test`/`it`/hook
functions, which auto-attach to the nearest enclosing suite), not assert.

- **Properties:**
  - `filePath: string`
  - `fullName: string`
  - `name: string`
  - `signal: AbortSignal`
- **Methods:** none documented beyond the properties above.
- **Events:** none.

#### `MockTracker`

The interface behind both the module-level `mock` export and every
`TestContext.mock`/hook-context `.mock` property. Tracks every mock it
creates so `reset()`/`restoreAll()` can undo them in bulk.

- **Methods:**
  - `fn(original?: Function, implementation?: Function, options?:
    MockFunctionOptions): Function & { mock: MockFunctionContext }` — wraps
    `original` (default: a no-op returning `undefined`) so it can be swapped
    out for `implementation` after `options.times` calls (default:
    unlimited — verify exact numeric sentinel used internally, conceptually
    `Infinity`).
  - `getter(object: object, methodName: string | symbol, implementation?:
    Function, options?: MockFunctionOptions): Function & { mock:
    MockFunctionContext }` — mocks the getter of an existing accessor
    property.
  - `method(object: object, methodName: string | symbol, implementation?:
    Function, options?: MockMethodOptions): Function & { mock:
    MockFunctionContext }` — mocks a regular method, or (via
    `options.getter`/`options.setter`) an accessor's getter/setter.
  - `module(specifier: string, options?: MockModuleOptions):
    MockModuleContext` — replaces the module resolved by `specifier` for
    subsequent `import`/`require` of that specifier within the current
    isolation scope. **Requires the `--experimental-test-module-mocks`
    flag** (or RTS equivalent, see §5.8/§7) and depends on module-loader hook
    support.
  - `property(object: object, propertyName: string | symbol, value?: any):
    MockPropertyContext` — replaces a data property's value; restorable.
  - `reset(): void` — resets (clears call history / re-points to original)
    every mock created via this tracker, without fully un-wrapping them.
  - `restoreAll(): void` — restores every mock created via this tracker to
    its original, pre-mocked implementation/value, and stops tracking them.
  - `setter(object: object, methodName: string | symbol, implementation?:
    Function, options?: MockFunctionOptions): Function & { mock:
    MockFunctionContext }` — mocks an accessor's setter.
- **Properties:**
  - `timers: MockTimers` — the timer/date-mocking sub-interface.
- **Events:** none.

#### `MockFunctionContext`

The `.mock` property attached to every function/getter/setter returned by
`MockTracker.fn`/`.method`/`.getter`/`.setter`.

- **Methods:**
  - `callCount(): number` — number of times the mock has been invoked since
    creation or the last `resetCalls()`.
  - `mockImplementation(implementation: Function): void` — permanently
    replaces the implementation used for all subsequent calls.
  - `mockImplementationOnce(implementation: Function, onCall?: number): void`
    — replaces the implementation for exactly one future call (the `onCall`
    -th call, 0-based, if given; otherwise the next call).
  - `resetCalls(): void` — clears `.calls` without restoring the original
    implementation.
  - `restore(): void` — restores the pre-mock original implementation/value
    (equivalent to what `MockTracker.restoreAll()` does for this one mock).
- **Properties:**
  - `calls: MockCall[]` — see §3 `MockCall` for the exact per-entry shape.

#### `MockModuleContext`

Returned by `MockTracker.module()`.

- **Methods:**
  - `restore(): void` — un-mocks the module specifier, reverting subsequent
    resolutions to the real module.
- **Properties:** none documented.
- **Events:** none.

#### `MockPropertyContext`

Returned by `MockTracker.property()`.

- **Methods:**
  - `mockImplementation(value: any): void` — permanently sets the property's
    mocked value.
  - `mockImplementationOnce(value: any, onAccess?: number): void` — sets the
    value returned for exactly one future access.
  - `resetAccesses(): void` — clears `.accesses` without restoring.
  - `restore(): void` — restores the original value.
  - `accessCount(): number` — number of times the property has been read
    since creation or the last `resetAccesses()`.
- **Properties:**
  - `accesses: PropertyAccess[]` — see §3.

#### `MockTimers`

`MockTracker.timers` / `TestContext.mock.timers`. Fakes the passage of time
for timer callbacks and, optionally, `Date`.

- **Methods:**
  - `enable(enableOptions?: MockTimersEnableOptions): void` — begins
    intercepting the APIs listed in `enableOptions.apis` (default: all of
    `setTimeout`/`setInterval`/`setImmediate`/`clearTimeout`/
    `clearInterval`/`clearImmediate`; `Date` only when explicitly listed),
    seeded at `enableOptions.now` (default: `0`, i.e. Unix epoch).
  - `reset(): void` — un-mocks every intercepted API and clears pending fake
    timers.
  - `tick(milliseconds?: number): void` — advances the fake clock by
    `milliseconds` (default `0`), synchronously firing every timer callback
    whose fire time has now elapsed (in fire-time order).
  - `runAll(): void` — fires every currently pending fake timer regardless
    of its scheduled delay, advancing the fake clock to match.
  - `setTime(milliseconds: number): void` — jumps the fake clock to an
    absolute value **without** executing timers that would have fired along
    the way (use `tick`/`runAll` for that).
  - `[Symbol.dispose](): void` — equivalent to `reset()`; enables `using
    t.mock.timers.enable(...)` explicit-resource-management syntax.
- **Properties:** none beyond the methods above.
- **Events:** none.

#### `TestsStream`

Returned by `run()`. A `Readable` stream (object mode) that is also an
`EventEmitter`; consumers may either listen for the granular events below or
`.pipe()`/`.compose()` it directly into a reporter (which itself is a
`Transform`/async-generator over the same event objects — see the
`node:test/reporters` exports).

- **Extends:** `stream.Readable` (which itself is an `EventEmitter`).
- **Events** (event name → `event.data` shape, all delivered as `(event:
  TestEvent) => void` where `event.type` mirrors the event name and
  `event.data` is the discriminated payload — see §3 `TestEvent`):
  - `'test:coverage'` — coverage summary for a completed run (only when
    coverage collection was enabled).
  - `'test:complete'` — a single test/suite finished (pass, fail, skip, or
    todo) with full timing/result detail.
  - `'test:dequeue'` — a test/suite has been dequeued for execution.
  - `'test:diagnostic'` — a diagnostic message (`context.diagnostic(...)` or
    internal runner diagnostics).
  - `'test:enqueue'` — a test/suite has been enqueued.
  - `'test:fail'` — a test/suite failed.
  - `'test:interrupted'` — a hook/test was interrupted (e.g. by
    `forceExit`/an abort).
  - `'test:pass'` — a test/suite passed.
  - `'test:plan'` — the number of subtests/assertions planned at a given
    nesting level.
  - `'test:start'` — a test/suite began execution.
  - `'test:stderr'` — a line of stderr output produced by an isolated child
    process running a test file.
  - `'test:stdout'` — a line of stdout output produced by an isolated child
    process.
  - `'test:summary'` — the final aggregate summary (counts of pass/fail/skip/
    todo/cancelled/duration) once the whole run completes.
  - `'test:watch:drained'` — (watch mode) the rerun queue emptied.
  - `'test:watch:restarted'` — (watch mode) a rerun cycle began after a file
    change.

### Top-level functions

#### Test/suite declaration (18)

| # | Signature | Variant |
|---|---|---|
| 1 | `test(name?: string, options?: TestOptions, fn?: (t: TestContext, done?: (err?: Error) => void) => void \| Promise<void>): Promise<void>` | sync / callback / promise |
| 2 | `test.skip(name?, options?, fn?): Promise<void>` | same as `test`, forces `skip: true` |
| 3 | `test.todo(name?, options?, fn?): Promise<void>` | same as `test`, forces `todo: true` |
| 4 | `test.only(name?, options?, fn?): Promise<void>` | same as `test`, forces `only: true` |
| 5 | `test.expectFailure(name?, options?, fn?): Promise<void>` | same as `test`, forces `expectFailure: true` *(shorthand added in recent Node 25.x; verify exact subversion at implementation time)* |
| 6 | `it(name?, options?, fn?): Promise<void>` | alias of `test` |
| 7 | `it.skip(name?, options?, fn?): Promise<void>` | alias of `test.skip` |
| 8 | `it.todo(name?, options?, fn?): Promise<void>` | alias of `test.todo` |
| 9 | `it.only(name?, options?, fn?): Promise<void>` | alias of `test.only` |
| 10 | `it.expectFailure(name?, options?, fn?): Promise<void>` | alias of `test.expectFailure` |
| 11 | `describe(name?: string, options?: TestOptions, fn?: (s: SuiteContext) => void \| Promise<void>): Promise<void>` | alias of `suite` |
| 12 | `describe.skip(name?, options?, fn?): Promise<void>` | forces `skip: true` |
| 13 | `describe.todo(name?, options?, fn?): Promise<void>` | forces `todo: true` |
| 14 | `describe.only(name?, options?, fn?): Promise<void>` | forces `only: true` |
| 15 | `suite(name?, options?, fn?): Promise<void>` | sync (body runs synchronously to *declare* subtests; those subtests then execute per the normal scheduler) |
| 16 | `suite.skip(name?, options?, fn?): Promise<void>` | forces `skip: true` |
| 17 | `suite.todo(name?, options?, fn?): Promise<void>` | forces `todo: true` |
| 18 | `suite.only(name?, options?, fn?): Promise<void>` | forces `only: true` |

Detail:

- **`test`/`it`** — `name` defaults to `fn.name` or `"<anonymous>"`. When the
  test function's signature includes a second parameter (`done`), the test is
  **callback-style**: it does not complete until `done()` is invoked (with an
  optional `Error` first argument to fail it); mixing callback style *and*
  returning a `Promise` from the same function is an error (`ERR_TEST_FAILURE`
  — "callback and promise both used"). Otherwise, if the function is `async`
  or returns a thenable, the test is **promise-style** and completes when that
  promise settles. A plain synchronous function that throws fails
  synchronously; one that returns normally passes.
- **`test.skip`/`.todo`/`.only`/`.expectFailure`** are pure sugar over the
  `options` object — `test(name, { skip: true, ...options }, fn)` etc. — not
  separate execution paths.
- **`suite`/`describe`** — the callback runs **synchronously to completion**
  purely to *register* nested `test`/`it`/hook calls against this suite (the
  callback itself may still be `async`, but Node does not wait on registration
  order across an `await` inside a suite body in the way it does for a test
  body — nested test declarations happening after an `await` inside a suite
  function are a known footgun, not a feature to design around). `suite()`'s
  own returned `Promise` resolves once every declared child test/suite has
  finished executing.

#### Hooks (4)

| # | Signature | Variant |
|---|---|---|
| 19 | `before(fn?: (ctx: TestContext) => void \| Promise<void>, options?: HookOptions): void` | sync / promise |
| 20 | `after(fn?: (ctx: TestContext) => void \| Promise<void>, options?: HookOptions): void` | sync / promise |
| 21 | `beforeEach(fn?: (ctx: TestContext) => void \| Promise<void>, options?: HookOptions): void` | sync / promise |
| 22 | `afterEach(fn?: (ctx: TestContext) => void \| Promise<void>, options?: HookOptions): void` | sync / promise |

- All four attach to the **nearest enclosing suite** (or the whole file, if
  called at top level outside any `describe`). `fn` defaults to a no-op.
  `after` is guaranteed to run even if a `beforeEach`/test/`afterEach` in the
  same scope throws. A hook that throws or times out (`options.timeout`)
  fails every test that would otherwise have run under it (for `before`) or
  fails just the current test (for `beforeEach`/`afterEach`).

#### Execution (1)

| # | Signature | Variant |
|---|---|---|
| 23 | `run(options?: RunOptions): TestsStream` | callback (event-stream) |

- Synchronously returns a `TestsStream`; all actual test execution and event
  emission happens asynchronously afterward. Throws synchronously only for
  malformed `options` (e.g. both `files` and `globPatterns` given together, an
  invalid `isolation` value). See §3 `RunOptions`.

#### `assert` namespace (1)

| # | Signature | Variant |
|---|---|---|
| 24 | `assert.register(name: string, fn: (this: TestContext, ...args: any[]) => void): void` | sync |

- Registers a custom assertion under `context.assert[name]` for every
  `TestContext` created afterward, so test bodies can call
  `t.assert.myCustomThing(...)`. This is `node:test`'s own `assert` export
  (a small extensibility surface distinct from, and not a replacement for,
  the general-purpose `node:assert` module — most user code still imports
  `node:assert` separately for `strictEqual`/`deepStrictEqual`/etc. inside a
  test body).

#### `snapshot` namespace (2)

| # | Signature | Variant |
|---|---|---|
| 25 | `snapshot.setDefaultSnapshotSerializers(serializers: Array<(value: any) => string>): void` | sync |
| 26 | `snapshot.setResolveSnapshotPath(fn: (testFilePath: string) => string): void` | sync |

- `setDefaultSnapshotSerializers` installs the serializer chain applied (in
  order, first match wins — verify exact matching rule against Node source)
  before a value is written to/compared against a snapshot, when the call
  site doesn't supply its own `options.serializers`.
- `setResolveSnapshotPath` overrides where `context.assert.fileSnapshot`
  writes/reads snapshot files for a given test file (default: `<test file
  name>.snapshot` alongside the test file).

#### `mock` namespace, top-level (8)

Identical signatures/semantics to the `MockTracker` instance methods
documented above — `mock` (the module export) **is** a `MockTracker`
instance shared for the whole process/isolation-scope (as opposed to
`TestContext.mock`, a fresh instance auto-restored per test).

| # | Signature |
|---|---|
| 27 | `mock.fn(original?, implementation?, options?): Function & { mock: MockFunctionContext }` |
| 28 | `mock.getter(object, methodName, implementation?, options?): Function & { mock: MockFunctionContext }` |
| 29 | `mock.method(object, methodName, implementation?, options?): Function & { mock: MockFunctionContext }` |
| 30 | `mock.module(specifier, options?): MockModuleContext` |
| 31 | `mock.property(object, propertyName, value?): MockPropertyContext` |
| 32 | `mock.reset(): void` |
| 33 | `mock.restoreAll(): void` |
| 34 | `mock.setter(object, methodName, implementation?, options?): Function & { mock: MockFunctionContext }` |

#### `node:test/reporters` exports (5)

| # | Signature | Variant |
|---|---|---|
| 35 | `spec: TransformLike` | consumer of `TestsStream` events |
| 36 | `tap: TransformLike` | consumer of `TestsStream` events |
| 37 | `dot: TransformLike` | consumer of `TestsStream` events |
| 38 | `junit: TransformLike` | consumer of `TestsStream` events |
| 39 | `lcov: TransformLike` | consumer of `TestsStream` events; **only produces meaningful output when coverage was collected** (`--experimental-test-coverage` / `run({ coverage: true })`) |

- `TransformLike` = either a `stream.Transform` instance in object-read /
  string-or-buffer-write mode, or an async generator function `(source:
  AsyncIterable<TestEvent>) => AsyncGenerator<string \| Buffer>` — Node's own
  custom-reporter contract accepts both shapes (see the "Custom reporter"
  authoring styles below) and the five built-ins are implemented using one or
  the other internally; *(exact per-reporter implementation shape — class
  instance vs. bare async-generator function — should be verified against
  Node source at implementation time; either is achievable from the RTS
  `.ts` shim and is an implementation detail invisible to callers who just
  `import { spec } from "node:test/reporters"` and hand it to
  `--test-reporter=spec` or `.compose(spec)`)*.
- **Custom reporter contract** (for user-authored reporters, not a `node:test`
  export, but part of this module's documented surface since `--test-reporter
  <path>` loads a user module implementing it): a default export that is
  either (a) a `stream.Transform` with `writableObjectMode: true` whose
  `transform(event, encoding, callback)` receives each `TestEvent` and calls
  `callback(null, outputChunk)`, or (b) an async generator function
  `async function* (source) { for await (const event of source) { yield
  ...; } }`.

### Properties & constants

- **`mock`** — the module-level `MockTracker` singleton (methods 27-34
  above).
- **`snapshot`** — the module-level snapshot-configuration namespace
  (methods 25-26 above).
- **`assert`** — the module-level custom-assertion-registration namespace
  (method 24 above) — **not** to be confused with the separate `node:assert`
  module.

### Events

`node:test` itself defines no ambient/global events. All eventing is scoped
to a `TestsStream` instance returned by `run()` (see the `TestsStream` class
above for the full event list).

## 3. Types & option objects

```ts
/** Options accepted by test()/it()/test.skip()/.todo()/.only()/.expectFailure(). */
interface TestOptions {
  /** Async subtest/assertion parallelism within this test. Default: false (serial). */
  concurrency?: number | boolean;
  /** Mark this test as expected to fail; see ExpectFailureSpec. Default: false. */
  expectFailure?: ExpectFailureSpec;
  /** Run only this test (requires --test-only or isolation:'none', see §4). Default: false. */
  only?: boolean;
  /** Abort this test when the signal fires. Default: undefined. */
  signal?: AbortSignal;
  /** Skip this test; string form is a documented reason. Default: false. */
  skip?: boolean | string;
  /** Execute but never fail the run on this test's outcome; string form is a reason. Default: false. */
  todo?: boolean | string;
  /** Per-test timeout in ms after which the test fails/aborts. Default: Infinity. */
  timeout?: number;
  /** Exact number of assertions + subtests expected; mismatch fails the test. Default: undefined (unchecked). */
  plan?: number;
}

/** Options accepted by describe()/suite() and their .skip/.todo/.only variants — same shape as TestOptions. */
type SuiteOptions = TestOptions;

/** Full form of the `expectFailure` option. */
type ExpectFailureSpec =
  | boolean
  | string                                   // reason label only
  | RegExp                                   // failure message/error must match
  | ((error: unknown) => boolean)            // validator
  | Error                                    // failure must match this Error's shape
  | { label?: string; match?: RegExp | ((error: unknown) => boolean) };

/** Options accepted by before()/after()/beforeEach()/afterEach() and their TestContext method equivalents. */
interface HookOptions {
  signal?: AbortSignal;
  /** Default: Infinity. */
  timeout?: number;
}

/** Options accepted by context.plan(count, options). */
interface PlanOptions {
  /**
   * Whether/how long to wait for the planned count to be reached before
   * timing out the test. (verify exact default and unit against Node source
   * — documentation excerpt did not fully specify this option's shape.)
   */
  wait?: boolean | number;
}

/** Options accepted by context.waitFor(condition, options). */
interface WaitForOptions {
  /** Polling interval in ms. (verify default) */
  interval?: number;
  /** Overall timeout in ms before rejecting. (verify default) */
  timeout?: number;
}

/** Options accepted by run(). */
interface RunOptions {
  concurrency?: number | boolean;             // default: false
  cwd?: string;                                // default: process.cwd()
  files?: string[];                            // default: auto-discovery, mutually exclusive with globPatterns
  forceExit?: boolean;                         // default: false
  globPatterns?: string[];                     // default: standard patterns (see §4)
  inspectPort?: number | (() => number);       // default: undefined
  isolation?: 'process' | 'none';              // default: 'process'
  only?: boolean;                              // default: false
  setup?: (stream: TestsStream) => void;       // default: undefined
  execArgv?: string[];                         // default: []
  argv?: string[];                             // default: []
  signal?: AbortSignal;                        // default: undefined
  testNamePatterns?: string | RegExp | Array<string | RegExp>;  // default: undefined
  testSkipPatterns?: string | RegExp | Array<string | RegExp>;  // default: undefined
  timeout?: number;                            // default: Infinity
  watch?: boolean;                             // default: false
  shard?: ShardConfig;                         // default: undefined
  rerunFailuresFilePath?: string;              // default: undefined
  coverage?: boolean;                          // default: false
  coverageExcludeGlobs?: string | string[];    // default: undefined
  coverageIncludeGlobs?: string | string[];    // default: undefined
  lineCoverage?: number;                       // default: 0 (percent threshold)
  branchCoverage?: number;                     // default: 0
  functionCoverage?: number;                   // default: 0
  env?: Record<string, string>;                // default: process.env (not merged: replaces)
}

interface ShardConfig {
  index: number;   // 1-based shard index
  total: number;   // total number of shards
}

/** Options accepted by mock.fn()/mock.getter()/mock.setter(). */
interface MockFunctionOptions {
  /** Number of calls before automatically reverting to `original`. Default: Infinity (unlimited). */
  times?: number;
}

/** Options accepted by mock.method(). */
interface MockMethodOptions extends MockFunctionOptions {
  /** Mock the property's getter instead of a plain method. Default: false. */
  getter?: boolean;
  /** Mock the property's setter instead of a plain method. Default: false. */
  setter?: boolean;
}

/** Options accepted by mock.module(). Requires --experimental-test-module-mocks. */
interface MockModuleOptions {
  /** Whether repeated mock.module() calls for the same specifier reuse a cached mock. Default: false. */
  cache?: boolean;
  /** Named exports to substitute; keys not listed fall through to the real module. Default: {}. */
  namedExports?: Record<string, unknown>;
  /** Default export to substitute. Default: undefined (real module's default export, if any). */
  defaultExport?: unknown;
}

type TimerAPI =
  | 'setTimeout' | 'setInterval' | 'setImmediate'
  | 'clearTimeout' | 'clearInterval' | 'clearImmediate'
  | 'Date';

interface MockTimersEnableOptions {
  /** Which globals to intercept. Default: all timer APIs; 'Date' only if explicitly listed. */
  apis?: TimerAPI[];
  /** Initial fake clock value. Default: 0 (Unix epoch). */
  now?: number | Date;
}

/** Options accepted by context.assert.snapshot()/fileSnapshot(). */
interface SnapshotOptions {
  /** Overrides the globally configured serializer chain for this call only. */
  serializers?: Array<(value: any) => string>;
}

/** One entry of MockFunctionContext.calls. */
interface MockCall {
  arguments: unknown[];
  /** Present when the call returned normally (mutually exclusive with `error`). */
  result?: unknown;
  /** Present when the call threw/rejected (mutually exclusive with `result`). */
  error?: Error;
  /** `this` value the mock was invoked with. */
  this?: unknown;
  /** Set only when the mock was invoked as a constructor (`new mockedCtor(...)`). */
  target?: unknown;
  /** Captured call-site stack trace. (verify exact string format against Node source) */
  stack?: string;
}

/** One entry of MockPropertyContext.accesses. */
interface PropertyAccess {
  /** Whether this access was a read that produced a value (vs. a write). */
  hasValue: boolean;
  /** The value read, when hasValue is true. */
  value?: unknown;
}

/** Discriminated event delivered on TestsStream / to a reporter's event source. */
interface TestEvent<T extends string = string, D = unknown> {
  type: T;
  data: D;
}

/** Common fields present on most 'test:*' event.data payloads. */
interface TestEventCommon {
  name: string;
  nesting: number;      // depth in the suite hierarchy
  file?: string;
  line?: number;
  column?: number;
}

interface TestPassFailData extends TestEventCommon {
  details: {
    duration_ms: number;
    type?: 'suite' | 'test';
    passed: boolean;
    error?: { message: string; stack?: string; cause?: unknown; code?: string };
  };
  todo?: boolean | string;
  skip?: boolean | string;
}

interface TestPlanData extends TestEventCommon {
  count: number;
}

interface TestDiagnosticData extends TestEventCommon {
  message: string;
}

interface TestCoverageData {
  summary: CoverageSummary;
}

interface CoverageSummary {
  files: Array<{
    path: string;
    totalLineCount: number;
    coveredLineCount: number;
    totalBranchCount: number;
    coveredBranchCount: number;
    totalFunctionCount: number;
    coveredFunctionCount: number;
    // per-line/branch/function detail maps omitted here — see Node's own
    // coverage-map shape; only needed once §5.8 phase (k) is implemented.
  }>;
  totals: {
    totalLineCount: number;
    coveredLineCount: number;
    totalBranchCount: number;
    coveredBranchCount: number;
    totalFunctionCount: number;
    coveredFunctionCount: number;
  };
  thresholds?: { line: number; branch: number; function: number };
}
```

## 4. Node semantics & edge cases

- **Encodings.** All test-runner-internal I/O (TAP/event serialization
  between an isolated child process and its parent, snapshot files) is UTF-8
  text. No binary-encoding surface of its own.
- **Platform differences.** Process-isolation mode spawns one child process
  per test file via the same primitives `node:child_process` uses; on
  Windows this goes through `CreateProcess`, on POSIX through `fork`/`exec` —
  no `node:test`-specific platform branching beyond whatever
  `node:child_process` already handles. Watch mode's file-change detection
  inherits `node:fs.watch`'s well-known platform quirks (inotify coalescing
  on Linux, `ReadDirectoryChangesW` on Windows, kqueue on macOS).
- **Error/errno codes.** The runner's own failures surface as
  `ERR_TEST_FAILURE` (generic test/hook failure, timeout, plan mismatch,
  callback-and-promise-both-used, uncaught exception during a test). Module
  mocking without the required flag throws an `ERR_INVALID_STATE`-style
  usage error (Node treats it as programmer error, not a runtime data error).
  A `mock.module()` call for a specifier with no loader-hook support throws a
  resolution-time error.
- **Ordering guarantees.** Within one file/suite, sibling tests run in
  declaration order unless `concurrency` allows overlap. `before` runs once,
  strictly before the first test/subtest at its scope; `after` runs once,
  strictly after the last one (including on failure). `beforeEach`/
  `afterEach` bracket **every** test at their scope, in declaration order
  relative to other same-scope hooks. Un-awaited subtests are **not**
  guaranteed to finish before the parent test resolves — this is the single
  most common correctness footgun in test authoring and must be preserved
  (the parent fails, it does not silently drop the subtest).
- **Backpressure.** `TestsStream` is a real `Readable`; if a reporter/consumer
  reads slowly, event production upstream **should** respect the stream's
  internal buffering the same way any other Node `Readable` producer would —
  in RTS terms this means the same generic stream backpressure contract
  `node:stream`/`stream.Readable` already implements, not a bespoke one.
- **Deprecations.** None specific to `node:test` itself as of the v25
  documentation snapshot; the watch-mode and coverage-collection features are
  still marked below "Stable" (see the metadata table).
- **`only` gating rule (important, easy to get wrong).** `{ only: true }` is
  honored **only** when the process was started with `--test-only`, **or**
  `run({ only: true })` was passed programmatically, **or** test isolation is
  `'none'`. Outside those conditions, an `only`-marked test runs like any
  other (Node does **not** silently make it exclusive by default — this
  prevents an accidentally-committed `.only` from silently hiding the rest of
  a suite in default CI runs that don't pass `--test-only`). When `only` mode
  *is* active: any suite marked `only` runs all its descendants **unless**
  some descendant is itself marked `only`, in which case only those (and
  their required ancestor chain) run; `context.runOnly(bool)` toggles this
  behavior for subtests declared after the call, at any nesting depth.
- **`skip`/`todo`/`expectFailure` precedence.** If more than one is set:
  `skip` always wins over both `todo` and `expectFailure`; `todo` wins over
  `expectFailure`. A `skip`ped test is never executed; a `todo` test **is**
  executed but its outcome never fails the run; `expectFailure` inverts
  pass/fail (the run fails if the test *unexpectedly* passes).
- **Name-pattern filtering (`--test-name-pattern` / `testNamePatterns`).**
  Matches against a test's `fullName` at any nesting level (a bare substring
  or `/regex/flags` form); multiple patterns are AND-ed; `--test-skip-pattern`
  can be combined and both must be satisfied. Filtering only decides which
  discovered tests **execute** — it never changes which **files** are loaded,
  and non-matching tests' `before`/`beforeEach`/`afterEach` hooks at their
  scope are also skipped (not just the test body).
- **Security notes.** Test files are executed as arbitrary code with the same
  privileges as the invoking process — `node:test` provides no sandboxing.
  Combining this with any future RTS permission-model work is out of scope
  for this module's spec.
- **Distinct from RTS's own existing `test`/`bundle.ts` harness.** The RTS
  project already ships an **internal** test harness under the `rts:test`
  specifier (`crates/rts-runtime/src/namespaces/test/` — `test_core` +
  `bundle.ts`, described/used throughout the project's own `.claude/rules` +
  `tests/*.test.ts` suite). That harness is unrelated to this spec: different
  import specifier (`rts:test` vs. `node:test`), different owning crate
  (`rts-runtime`/`rts-std` vs. `rts-node`), and per the "rts-node is fully
  independent" decision (see the architecture facts below), `node:test`
  **must not** depend on or reuse the `rts:test` harness's native pieces —
  even though the two are conceptually similar (both are TAP-ish event-driven
  test runners). Flagged again in §7 to prevent an implementer from reaching
  for the existing `rts:test` internals as a shortcut.

## 5. RTS implementation notes

### 5.1 Native impl mapping

The overwhelming majority of `node:test`'s surface is **pure orchestration
logic** — suite/test tree bookkeeping, name-pattern matching, `only`/`skip`/
`todo` precedence, mock call-history tracking, event object construction —
all of which operates on values/functions already first-class in the RTS
value model (functions, closures, `Promise`, arrays/objects for call
records). None of that needs a Rust std module. The narrow native needs are:

| Area | Rust std / crate |
|---|---|
| Process isolation (`isolation: 'process'`, default) — spawn one child `rts` process per test file, capture stdout/stderr | `std::process::Command` (own to `rts-node`, shared internally with `node:child_process`'s own implementation — **not** a dependency on the old `rts-std` process namespace, which is being removed) |
| Reading a child's stdout/stderr as it streams (for `test:stdout`/`test:stderr` events) | `std::io::Read` on the child's piped handles, read on a dedicated OS thread (`std::thread::spawn`) |
| Snapshot file read/write (`context.assert.fileSnapshot`, `--test-update-snapshots`) | `std::fs` (own to `rts-node` via `node:fs`, not `rts-std`) |
| Watch mode file-change detection | `std::fs` + a native filesystem-notification mechanism (own to `rts-node` via `node:fs.watch`, not `rts-std`) |
| Monotonic clock for `duration_ms`, `timeout`, `signal` deadlines | `std::time::Instant` (own to `rts-node`, or reused via `node:perf_hooks`/`node:process.hrtime` if those land first in the same crate) |
| Test-name-pattern matching (`testNamePatterns`) | plain string/regex work — RTS's already-primordial `RegExp`, no new native fn |
| Code coverage instrumentation (`--experimental-test-coverage`) | **not** a Rust std module — this needs cooperation from the RTS *compiler/codegen* itself (line/branch/function hit counters emitted into compiled code), an order of magnitude harder than everything else in this module; see §5.8(k) and §7 |

Everything else — the suite tree, hook scheduling, `TestContext`/
`SuiteContext` objects, `MockTracker`/`MockFunctionContext`/
`MockPropertyContext`/`MockTimers`, the five built-in reporters, snapshot
serialization, `TestsStream` event construction — is implemented as `.ts`
shipped by `rts-node`, calling only the narrow native externs above plus
whatever `node:child_process`/`node:fs`/`node:util` (same crate) already
expose.

### 5.2 ABI surface

No rich object in this module's surface needs to become a `Handle` in the
`HandleTable` sense **except** the isolated child-process reference (already
owned by `node:child_process`'s own `Handle`, reused here rather than
duplicated). Everything else (`TestContext`, `MockTracker`, `TestsStream`,
etc.) is an ordinary `.ts`-constructed object.

| Symbol | Args (AbiType) | Return (AbiType) | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_TEST_SPAWN_ISOLATED` | `file: StrPtr`, `argv: StrPtr` (newline/serialized-array encoded, matching the convention `node:process`/`node:child_process` already use), `env: StrPtr` (serialized) | `Handle` (child-process handle) | Prefer routing through `node:child_process`'s own spawn extern directly rather than adding a duplicate — this row exists only if a `node:test`-specific convenience wrapper proves necessary; default assumption is **reuse**, not a new symbol. |
| `__RTS_FN_NODE_TEST_READ_CHILD_LINE` | `handle: Handle`, `stream: I32` (0=stdout,1=stderr) | `StrPtr` (empty when the stream has closed) | Same reuse caveat as above — likely just `node:child_process`'s existing stdout/stderr line-read primitive. |
| `__RTS_FN_NODE_TEST_MONOTONIC_NOW_MS` | — | `F64` | Only needed if no monotonic-clock extern already exists in `rts-node` by the time this module is implemented (likely already provided by `node:perf_hooks`/`node:process.hrtime`, in which case skip this row entirely). |
| `__RTS_FN_NODE_TEST_COVERAGE_SNAPSHOT` | — | `Handle` (opaque coverage-counter blob) | Deferred — depends on engine/codegen instrumentation existing at all (see §5.8(k), §7). Not part of the initial implementation. |

**This module's ABI footprint is close to zero** by design — it is
overwhelmingly a consumer of other `rts-node` modules' externs
(`node:child_process`, `node:fs`, `node:util`) plus the engine's own
primordial `Promise`/`Function`/`Error`/`Array`/`Object` operations. Treat
any new symbol above as "add only if the equivalent doesn't already exist
elsewhere in `rts-node`".

### 5.3 Async model

| Area | Mapping |
|---|---|
| Promise-style test/hook functions (`async (t) => {...}`) | Directly `await`ed by the `.ts` scheduler — uses the engine's primordial `Promise`/`await` machinery, no native involvement. |
| Callback-style test functions (`(t, done) => {...}`) | The `.ts` shim wraps the call in `new Promise((resolve, reject) => { ... invoke fn(t, (err) => err ? reject(err) : resolve()) ... })`. **This requires the RTS engine's `Promise` to support the general executor form** (`new Promise((resolve, reject) => ...)`), not only the `async-function → promise.create(fn, args)` rewrite path described in `docs/specs/async-promise-function.md`. **Flag:** confirm executor-style `new Promise(...)` is (or will be) supported before/while implementing this — if only the rewrite-based path exists today, callback-style test functions are the one place in this module that cannot be built without that prerequisite landing first. |
| `run()`'s streamed event emission | Each completed test/hook enqueues an event object onto the `TestsStream`'s internal `Readable` buffer; consuming it via `.pipe()`/`for await` is ordinary `stream.Readable` consumption (`node:stream`, same crate) — no bespoke scheduling. |
| Process-isolation mode | The parent `.ts` orchestrator spawns N child processes (up to `concurrency`), reads each child's piped stdout as an event/TAP stream on a dedicated Rust-side reader thread, and re-emits parsed events into the parent's own `TestsStream` — a real multiplexer over multiple OS-level pipes. This can be built with plain blocking `std::thread`-per-child-process I/O (no tokio requirement), or with the shared tokio runtime if async pipe reads are preferred for lower thread count; either is viable, see §5.7. |
| Watch mode | Needs a **long-lived** process that both watches the filesystem and re-runs affected test files — this is the one area of the module that meaningfully wants a real async event loop (the project's own open epic **#207**) rather than a one-shot cooperative Promise chain; until #207 lands, watch mode should be treated as its own deferred sub-feature (see §5.8(l), §7), not blocking the rest of the module. |
| `mock.timers` | Purely synchronous fake-clock bookkeeping in `.ts` (no real timer/tokio involvement at all while mocked — that is the entire point of mocking timers: real scheduling is bypassed). |
| `waitFor(condition, options)` | A `.ts` polling loop using the mockable-or-real `setTimeout`/`Promise` combo already available; no new native primitive. |

### 5.4 Multithread / worker interaction

- **Default `isolation: 'process'`** spawns a genuinely separate **OS
  process** per test file — this is coarser and simpler than the RTS
  `worker_threads`/threading-model surface (`docs/specs/rts-threading-model.md`):
  it is not a same-process thread/region at all, so there is zero shared
  mutable state to reason about between test files by construction (matches
  Node's own model exactly, and is in fact *easier* to guarantee correct in
  RTS than isolation:'none', since each child process gets its own fresh RTS
  runtime instance, own GC heap, own everything).
- **`isolation: 'none'`** runs every discovered test file in the **same**
  process/thread, sequentially at the top level (subtests may still run with
  `concurrency`). This means the module-level bookkeeping — the "current
  suite/test" stack the top-level `test`/`describe`/hook functions register
  against, the module-level `mock` singleton, `snapshot`'s serializer
  config — is a **single-thread-local** structure for the thread that calls
  `run()`/executes `--test`. Per the RTS threading model, this bookkeeping
  **must** live in thread-local (region-local) state, never a process-wide
  `Mutex`-guarded static — because if `node:test` is ever invoked from
  *inside* a `worker_threads` Worker (its own OS thread/region under the RTS
  model), that worker's suite-declaration stack must not interleave with the
  main thread's (or another worker's) suite stack. This mirrors the existing
  project gotcha already noted in memory ("GCELLS thread-local!" for
  `rts:test`'s own timer/global state) — the same discipline applies here.
  Flag as the module's single most important thread-safety rule.
  Concretely: any RTS thread/region running its **own** `node:test` suite
  gets its own private root suite/queue, mock singleton, and snapshot config
  — never a shared one.
- **`mock.timers.enable()`** patches `globalThis.setTimeout`/`Date`/etc. for
  the **calling thread/region's view of `globalThis`** only — it must never
  leak into or affect another worker's timers. This depends on however the
  RTS threading model resolves "is `globalThis` per-region or truly
  process-shared" for ordinary mutable global rebinding; flag as an open
  dependency on that broader design decision (not specific to this module,
  but this module is a concrete consumer that needs the answer).
- **`concurrency`** (test-level option or `--test-concurrency` CLI flag)
  drives either more parallel child **processes** (isolation:'process') or
  more concurrently-interleaved **async** test functions within one thread
  (isolation:'none') — the latter is cooperative `Promise` concurrency, not
  additional OS threads, and needs no `worker_threads`/region involvement at
  all.
- **`shard`** (`{ index, total }`) partitions the discovered file list before
  any process/thread is spawned — pure `.ts` list-slicing logic, no
  threading-model interaction.

### 5.5 Buffer / TypedArray interop

`node:test` moves almost no binary data across the ABI:

- Child-process stdout/stderr for the isolated-execution TAP/event protocol
  is UTF-8 **text** (`StrPtr`), read line-by-line — not raw byte buffers in
  the TypedArray sense.
- Snapshot files are UTF-8 text (serialized values via `util.inspect`-style
  formatting or a custom serializer function returning a `string`).
- Coverage data (once implemented) is numeric counters (line/branch/function
  hit counts), not byte buffers.
- If a test author's own code under test happens to use `Buffer`/TypedArrays
  and passes them into `assert`/`mock`/snapshot comparisons, that data flows
  through as ordinary already-primordial engine values (per the primordial
  TypedArray doctrine) with no `node:test`-specific marshalling step — the
  same as it would for `node:assert`.

### 5.6 Doctrine placement

`node:test` is **non-primordial** — no native literal/syntactic form, reached
only via `import ... from "node:test"` (or `"node:test/reporters"`). Per the
primordial-vs-registry doctrine, the engine must never hardcode the strings
`"test"`, `"describe"`, `"mock"`, etc. anywhere in `crates/rts-codegen-new/`.
Resolution flow:

1. `import { test, ... } from "node:test"` / `"node:test/reporters"` resolves
   through the **generic** node-module-import mechanism shared by every
   `node:*` specifier — a data row in `rts-node`'s `NODE_SPECS` /
   `node_lookup` / `ns_prefix_for` table mapping `"test"` (and the
   `"test/reporters"` sub-path) to its `.ts` shim file(s) + the small native
   extern namespace prefix from §5.2 (e.g. `node_test`), exactly like every
   other `node:*` module.
2. `--test` CLI-injected globals (`test`/`describe`/`it`/`before`/`after`/
   `beforeEach`/`afterEach` available with no import) are **not** an
   engine-level global-injection mechanism — they are produced by a small
   bootstrap `.ts` the `rts-cli` dispatch path prepends/imports when invoked
   with `--test`, itself doing nothing more than `import { test, describe,
   it, before, after, beforeEach, afterEach } from "node:test"` and assigning
   each onto `globalThis`. Zero engine hardcoding either way.
3. The `.ts` shim(s) (shipped by `rts-node`, not `rts-primitives`/
   `rts-shared`, since this is Node-specific, not a universal JS global)
   implement essentially 100% of the classes/functions in §2, using only
   primordial engine operations plus same-crate calls into
   `node:child_process` (process isolation), `node:fs` (snapshots, watch),
   `node:util` (inspect-based serialization), and `node:assert`-shaped
   helpers reused conceptually (not a hard dependency — `node:test`'s own
   `assert.register` is a separate, much smaller mechanism).
4. No `.ts` prelude injects a `node:test`-specific `__rts_wk_*` engine hook;
   the module needs no engine-side cooperation beyond what already exists for
   `Promise`/`Function`/`Error`/`Array`/`Object`/`RegExp`.

### 5.7 Shared-infra dependencies (FLAG)

- **event loop** — needed for watch mode specifically (a long-lived
  filesystem-watching + rerun process) and, more softly, for scheduling many
  concurrent async test functions efficiently under `concurrency`; the
  non-watch, non-highly-concurrent core of the module can get by on
  cooperative `Promise` chaining alone, but watch mode is a hard dependency
  on the project's open **#207** real-event-loop epic.
- **tokio** — potentially needed for the process-isolation multiplexer (many
  simultaneous child-process pipe reads); a `std::thread`-per-child
  alternative is also viable and avoids the dependency, so flag this as
  "needed only if the tokio-based multiplexer approach is chosen over the
  plain-OS-thread approach" rather than an unconditional requirement.
- **promise settle** — needed for callback-style test functions (`(t, done)
  => {...}`), which must be wrapped in a genuine executor-style `new
  Promise((resolve, reject) => ...)`; confirm this general Promise-executor
  form is reachable from `rts-node` without a `rts-std` dependency (i.e.
  hoisted to wherever the Promise subsystem itself lives once `rts-node`'s
  independence is fully realized), since today's documented async model
  (`docs/specs/async-promise-function.md`) primarily describes the
  `async-function-rewrite` path, not raw executor construction.
- **tls/rustls** — not needed.
- **crypto primitives** — not needed.
- **net sockets** — not needed.

If none of the above prove to already be reachable without a `rts-std`
dependency by the time this module is implemented, the concrete ask is:
hoist (a) a minimal Promise-executor constructor and (b) (only if the
tokio-based multiplexer path is chosen) an accessor to the shared multi-
thread tokio runtime, into a layer both `rts-std` (for its own remaining
audio/UI surface) and `rts-node` can depend on — per the project's own
architecture facts, `rts-node` must not gain a direct `rts-std` dependency to
get these.

### 5.8 Implementation phases

a. Scaffold the `NODE_SPECS` row for `"test"` (+ `"test/reporters"` as a
   sub-path/alias) pointing at new `.ts` shim file(s); stub every export in
   §2 with `throw new Error("not implemented")` bodies so imports resolve
   and the named-export surface type-checks end-to-end.
b. Implement the core single-process, `isolation: 'none'`-shaped suite tree:
   `test`/`it`/`describe`/`suite` (+ `.skip`/`.todo`/`.only`), `before`/
   `after`/`beforeEach`/`afterEach`, `TestContext`/`SuiteContext`, sequential
   execution, plain pass/fail bookkeeping and a minimal stdout summary (no
   reporters, no isolation, no mocking yet) — get "a single `.test.ts` file
   runs and reports pass/fail" working end-to-end. (Given RTS's simpler
   single-process CLI model today, treat `isolation: 'none'` as the natural
   **first** milestone even though it is not Node's own default — sequence
   process-isolation later, in step (h).)
c. Add `only`/`skip`/`todo`/`expectFailure` precedence, name/skip-pattern
   filtering (`testNamePatterns`/`testSkipPatterns`), `context.plan`/
   `context.waitFor`, per-test/hook `timeout`/`signal`.
d. Add `TestsStream` + the programmatic `run()` entry point + full event
   emission (`test:start`/`pass`/`fail`/`plan`/`diagnostic`/`enqueue`/
   `dequeue`/`complete`/`summary`).
e. Add the five built-in reporters (`spec` first as the CLI default, then
   `tap`, `dot`, `junit`, `lcov`) as `.ts` consumers of the `TestsStream`
   event contract; wire `--test-reporter`/`--test-reporter-destination`.
f. Add `mock.fn`/`.method`/`.getter`/`.setter`/`.property` +
   `MockFunctionContext`/`MockPropertyContext`/`MockTracker` (pure `.ts`
   monkey-patching of plain object properties — no native surface).
g. Add `mock.timers` (fake `setTimeout`/`setInterval`/`setImmediate`/
   `clearTimeout`/`clearInterval`/`clearImmediate` + optional `Date`
   faking).
h. Add `isolation: 'process'` mode: spawn one child `rts` process per test
   file (reusing `node:child_process`'s spawn/pipe-read primitives per
   §5.2), stream each child's events back into the parent `TestsStream`,
   respect `concurrency`/`shard`.
i. Add `mock.module()` — gated behind an RTS-equivalent
   `--experimental-test-module-mocks` flag; depends on however RTS's module
   resolution/loader-hook system works (flag as an open question in §7 if no
   loader-hook mechanism exists yet at implementation time).
j. Add snapshot testing (`context.assert.snapshot`/`.fileSnapshot`,
   `snapshot.setDefaultSnapshotSerializers`/`.setResolveSnapshotPath`,
   `--test-update-snapshots`).
k. Add code coverage collection (`--experimental-test-coverage` +
   `lineCoverage`/`branchCoverage`/`functionCoverage` thresholds + the
   `/* node:coverage disable/enable/ignore */` comment directives + wiring
   the `lcov` reporter to real data) — the heaviest lift, dependent on
   engine/codegen-level instrumentation; treat as its own sub-epic, likely
   sequenced last or deferred to a follow-up (see §7).
l. Add watch mode (`--watch` / `run({ watch: true })`) — depends on
   `node:fs.watch` plus the project's real-event-loop epic (**#207**); treat
   as deferred/blocked rather than part of the initial landing (see §7).
m. Wire CLI integration in `rts-cli` (`--test`, `--test-name-pattern`,
   `--test-only`, `--test-reporter[-destination]`, `--test-timeout`,
   `--test-concurrency`, `--test-isolation`, `--test-update-snapshots`,
   `--test-shard`, `--experimental-test-coverage` +
   `--test-coverage-include`/`-exclude`), taking care not to collide with or
   reuse the project's own pre-existing, unrelated `rts:test` CLI/harness
   (see §4's explicit note).
n. Land the test fixtures in §6.

## 6. Test plan

- `tests/node/test/basic_test_it_describe.test.ts` — a plain `test()`, an
  `it()` alias, nested `describe`/`suite` blocks with multiple `it`s;
  confirms pass/fail counts and `fullName` hierarchy.
- `tests/node/test/hooks_before_after_each.test.ts` — `before`/`after`/
  `beforeEach`/`afterEach` at both file scope and nested `describe` scope;
  confirms ordering and that `after` still runs when a test throws.
- `tests/node/test/nested_subtests_await.test.ts` — awaited subtests all
  complete and roll up into the parent's pass/fail; a **deliberately
  un-awaited** subtest fixture confirms the parent is marked failed/
  interrupted rather than silently dropping the subtest.
- `tests/node/test/skip_todo_only_precedence.test.ts` — `skip` beats `todo`
  beats `expectFailure` when combined; a `skip`-ped test never executes its
  body (side-effect counter stays at 0); a `todo` test that throws does not
  fail the run.
- `tests/node/test/only_mode_hierarchy.test.ts` — with `only` mode active
  (`run({ only: true })` in the harness): a suite marked `only` runs all
  descendants; a descendant additionally marked `only` narrows execution to
  just that descendant; `context.runOnly(false)` re-widens for later
  subtests. A companion case confirms `only`-marked tests run as **normal**
  tests (not exclusively) when `only` mode is *not* active.
- `tests/node/test/name_pattern_filtering.test.ts` — `testNamePatterns`
  (string substring + `/regex/i` form) selects the right subset across
  nesting levels; combined `testSkipPatterns` narrows further; a
  non-matching test's `beforeEach` side effect does not run.
- `tests/node/test/plan_and_waitfor.test.ts` — `context.plan(n)` passes when
  exactly `n` assertions/subtests run and fails when the count mismatches;
  `context.waitFor(() => flag)` resolves once an async callback flips `flag`
  and rejects on timeout when it never does.
- `tests/node/test/expect_failure.test.ts` — `expectFailure: true` on a test
  that throws → run passes; on a test that does **not** throw → run fails
  (unexpected success); `expectFailure: /msg/` matches/mismatches the thrown
  message.
- `tests/node/test/callback_style_done.test.ts` — `(t, done) => {...}` test
  passes when `done()` is called with no error, fails when called with an
  `Error`; a fixture that both calls `done()` **and** returns a `Promise`
  confirms the documented `ERR_TEST_FAILURE` ("callback and promise both
  used").
- `tests/node/test/timeout_and_abort_signal.test.ts` — a test with
  `timeout: 10` that never resolves fails via `ERR_TEST_FAILURE`; `t.signal`
  fires and is observed by the test body's own abort-aware operation.
- `tests/node/test/mock_fn_basic.test.ts` — `mock.fn()` call-count, default
  no-op return value; `mockImplementation`/`mockImplementationOnce(fn,
  onCall)`; `resetCalls()` vs `restore()` difference.
- `tests/node/test/mock_method_getter_setter.test.ts` — `mock.method(obj,
  'name')` intercepts calls on an existing method and `.mock.calls[i].this`
  is the receiver; `mock.method(obj, 'prop', impl, { getter: true })` and
  `{ setter: true }` variants.
- `tests/node/test/mock_property.test.ts` — `mock.property(obj, 'x', 42)`
  swaps the value; `accesses` records reads; `restore()` reverts.
- `tests/node/test/mock_timers_settimeout_date.test.ts` — `mock.timers.
  enable({ apis: ['setTimeout'] })`; `setTimeout(fn, 1000)` does not fire
  until `tick(1000)`; `runAll()` fires all pending; `enable({ apis: ['Date']
  })` + `setTime(...)` freezes `Date.now()`.
- `tests/node/test/mock_module.test.ts` *(experimental — guard/skip if the
  flag/loader-hook prerequisite from §5.8(i) is not yet implemented)* —
  `mock.module('./dep.ts', { namedExports: { fn: () => 42 } })` substitutes
  the import in code under test; `.restore()` reverts.
- `tests/node/test/snapshot_inline_and_file.test.ts` — `t.assert.snapshot(v)`
  on first run creates a baseline (under `--test-update-snapshots`) and
  matches on subsequent runs; a deliberately changed value fails the
  comparison; `t.assert.fileSnapshot(v, path)` writes/reads the given path.
- `tests/node/test/run_programmatic_teststream_events.test.ts` — calling
  `run({ files: [...] })` and asserting the exact sequence/shape of
  `test:start`/`test:pass`/`test:fail`/`test:plan`/`test:summary` events via
  direct `.on(...)` listeners (not a reporter).
- `tests/node/test/reporters_spec_tap_dot_junit_lcov.test.ts` — pipes a known
  fixture run through each of the five built-in reporters and checks
  reporter-specific output shape (TAP header/`ok`/`not ok` lines; JUnit XML
  well-formedness; dot's compact `.`/`F` characters; lcov's `.info`-shaped
  output once coverage data exists).
- `tests/node/test/process_isolation_multi_file.test.ts` *(multiprocess)* —
  `run({ files: [a, b], isolation: 'process', concurrency: 2 })` spawns two
  child processes; confirms `test:stdout`/`test:stderr` events surface
  correctly interleaved and `workerId` differs between the two files'
  `TestContext`s.
- `tests/node/test/isolation_none_shared_state.test.ts` — with `isolation:
  'none'`, two test files loaded into the same process do **not** leak
  `mock`-singleton state or suite-registration state into each other beyond
  what Node itself documents (module-level `mock` persists across files
  under `'none'` unless explicitly reset — confirm RTS matches this rather
  than incorrectly resetting between files).
- `tests/node/test/worker_thread_node_test.test.ts` *(multithread)* — spawns
  a `worker_threads` Worker that itself imports `node:test` and runs its own
  small suite; confirms the worker's suite-registration/mock-singleton
  bookkeeping never interleaves with or corrupts the main thread's own
  concurrently-running `node:test` suite (validates §5.4's thread-local
  bookkeeping rule under real concurrent use).
- `tests/node/test/coverage_thresholds.test.ts` *(deferred/experimental —
  only meaningful once §5.8(k) lands)* — `run({ coverage: true, lineCoverage:
  80 })` against a fixture with known partial coverage fails the run when
  below threshold and passes when a fully-covered fixture is used.

## 7. Open questions / deferrals

- **Code coverage collection (`--experimental-test-coverage`)** is by far the
  heaviest lift in this spec — it requires the RTS compiler/codegen itself to
  emit line/branch/function hit counters, an engine-level feature with no
  precedent elsewhere in the current codebase. Recommend scoping it as its
  own follow-up design (possibly its own spec doc) rather than blocking the
  rest of `node:test` on it; §5.8 sequences it last (phase k) for this
  reason.
- **Watch mode (`--watch`)** depends on the project's own open **#207**
  real-async-event-loop epic. Until that lands, watch mode should be treated
  as explicitly deferred (phase l), not silently stubbed as "pass" per the
  project's honesty floor.
- **`mock.module()`** depends on RTS having (or building) a module
  resolution/loader-hook system comparable to Node's `module.register()`
  hooks. If no such mechanism exists in `rts-node`'s module loader by the
  time this module is implemented, `mock.module()` should be marked
  explicitly unimplemented (throwing a clear "not supported yet" error)
  rather than approximated with something unsound (e.g. globally mutating
  the target module's exports object in place, which would break isolation
  guarantees other tests rely on).
- **Relationship to RTS's own pre-existing `rts:test` harness** — flagged
  prominently in §4 and §5.8(m): the project already has an internal,
  conceptually similar TAP-like test runner (`rts:test` / `bundle.ts` /
  `test_core`) used for RTS's *own* regression suite. Per the "rts-node is
  fully independent" architecture decision, `node:test` must be built from
  scratch in `rts-node` without depending on that existing
  `rts-runtime`/`rts-std`-hosted harness, even though a naive implementer
  might reach for it as a shortcut given the surface-level similarity.
- **Exact numeric defaults** for `context.plan`'s `wait` option and
  `context.waitFor`'s `interval`/`timeout` were not fully resolved by the
  fetched Node 25 documentation excerpts (marked "(verify)" in §2/§3) —
  re-check against Node's own source (`lib/internal/test_runner/test.js`)
  before finalizing the `.ts` shim's defaults.
- **Exact built-in-reporter implementation shape** (`spec`/`tap`/`dot`/
  `junit`/`lcov` as `Transform` instances vs. bare async-generator functions)
  was not conclusively determined from the fetched documentation — marked
  "(verify)" in §2. Low risk either way since both shapes are achievable from
  the `.ts` shim and both are accepted by the documented custom-reporter
  contract.
- **`MockCall.stack`'s exact string format** and **`context.attempt`'s
  indexing base** (0- vs 1-based) were referenced in the docs but not fully
  specified in the fetched excerpts — marked "(verify)" in §2/§3.
- **`test.expectFailure`/`it.expectFailure` as a distinct shorthand method**
  (as opposed to only the `{ expectFailure: ... }` option form) appears to be
  a recent Node 25.x addition per the fetched documentation excerpt, but the
  exact sub-version was not confirmed — verify at implementation time before
  committing to shipping it as a top-level export.
- **Async executor-style `new Promise((resolve, reject) => ...)` support** —
  §5.3/§5.7 flag this as a hard prerequisite for callback-style
  (`(t, done) => {...}`) test functions; confirm it is (or soon will be)
  supported by the engine's Promise subsystem before starting phase (b)/(c)
  of §5.8, since a meaningful fraction of real-world Node test suites use the
  callback form.
