# Node.js Globals — `process`

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `process` (ambient global) + `node:process` |
| Node.js version | 25.x |
| Stability | 2 - Stable (individual members carry their own Experimental/Deprecated/Legacy markers — noted inline) |
| Tier | P0 |
| Status | [ ] Not implemented — spec only |
| Import forms | ambient global `process` (available in every module with **no import**); `import process from "node:process"` (default export = the same singleton object); `import { argv, env, nextTick, ... } from "node:process"` (named re-exports of the same singleton's properties/functions, live-bound); `const process = require("node:process")` |
| Globals exposed | `process` (singleton, `EventEmitter` instance) |

## 1. Purpose

`process` is the single global object that represents the currently-running
RTS program and its host OS process. It is Node's (and therefore RTS's) most
heavily used built-in: command-line args (`argv`), environment variables
(`env`), lifecycle control (`exit`, signals, `nextTick`), resource metrics
(`memoryUsage`, `cpuUsage`, `hrtime`), and the standard I/O streams
(`stdout`/`stderr`/`stdin`) all live here. Unlike every other Node core module,
`process` requires **zero import** to use — it is injected into global scope
by the runtime before any user code executes, mirroring Node's own
`internal/bootstrap/node.js` behavior. This spec covers both: (a) the ambient
global singleton every RTS program already implicitly needs, and (b) the
`node:process` module form that lets code explicitly `import`/`require` the
same object (needed for ESM-strict codebases and for named-export
destructuring).

## 2. Exported API surface (COMPLETE)

### Classes

`process` has no user-constructible classes. The module *is* a single
pre-constructed singleton object, structured below as if it were a class
instance so every member has an unambiguous home.

#### `process` (the singleton itself)

**Extends:** `EventEmitter` (`node:events`). `process instanceof EventEmitter`
is `true` in real Node; `process` inherits the full `EventEmitter` prototype
surface (`on`, `once`, `off`, `emit`, `addListener`, `removeListener`,
`removeAllListeners`, `listeners`, `rawListeners`, `eventNames`,
`listenerCount`, `getMaxListeners`, `setMaxListeners`, `prependListener`,
`prependOnceListener`). These are **not reimplemented** by `node:process` —
see §5.1/§5.7 for how `process`'s event surface is backed by the same core
listener-registry primitive `node:events` provides.

Events fired specifically by `process` are listed under "Events" below.

#### `process.report` (namespace object, stable since v13.12.0)

Properties: `compact`, `directory`, `filename`, `reportOnFatalError`,
`reportOnSignal`, `reportOnUncaughtException`, `excludeEnv`, `signal` (all
listed in "Properties & constants"). Methods: `getReport([err])`,
`writeReport([filename][, err])` (listed in "Top-level functions").

#### `process.permission` (namespace object, since v20.0.0, Experimental)

Method: `has(scope[, reference])` (listed in "Top-level functions").

#### `process.channel` (namespace object, spawned/IPC process only)

Methods: `ref()`, `unref()` (listed in "Top-level functions"). `undefined`
when the process has no IPC channel (i.e. was not spawned with an `'ipc'`
stdio entry).

#### `process.finalization` (namespace object, since v22.5.0, Experimental)

Methods: `register(ref, callback)`, `registerBeforeExit(ref, callback)`,
`unregister(ref)` (listed in "Top-level functions").

#### `process.stdout` / `process.stderr` / `process.stdin`

Not classes of their own — instances of `stream.Writable`
(`stdout`/`stderr`) and `stream.Readable` (`stdin`), configured per §4. Each
carries an `.fd` property (`1`, `2`, `0` respectively). Full `stream.*` class
surface is out of scope for this document (see `node:stream`); this spec
covers only that `process` exposes these three fixed instances and their
`.fd`.

### Top-level functions

All are properties of the `process` singleton (`process.<name>(...)`), grouped
by area. "Variant" = sync | callback | promise, per the instructions.

| Function | Signature | Variant | Since |
|---|---|---|---|
| `process.nextTick` | `nextTick<Args extends any[]>(callback: (...args: Args) => void, ...args: Args): void` | callback (deferred, runs before I/O, after current op) | v0.1.26 |
| `process.exit` | `exit(code?: number \| string \| null): never` | sync | v0.1.13 |
| `process.abort` | `abort(): never` | sync | v0.7.0 |
| `process.cwd` | `cwd(): string` | sync | v0.1.8 |
| `process.chdir` | `chdir(directory: string): void` | sync, throws | v0.1.17 |
| `process.kill` | `kill(pid: number, signal?: string \| number): true` | sync, throws | v0.1.27 |
| `process.hrtime` | `hrtime(time?: [number, number]): [number, number]` | sync | v0.7.6 |
| `process.hrtime.bigint` | `hrtime.bigint(): bigint` | sync | v10.7.0 |
| `process.memoryUsage` | `memoryUsage(): MemoryUsage` | sync | v0.1.16 |
| `process.memoryUsage.rss` | `memoryUsage.rss(): number` | sync | v15.6.0 |
| `process.cpuUsage` | `cpuUsage(previousValue?: CpuUsage): CpuUsage` | sync | v6.1.0 |
| `process.resourceUsage` | `resourceUsage(): ResourceUsage` | sync | v12.6.0 |
| `process.threadCpuUsage` | `threadCpuUsage(previousValue?: CpuUsage): CpuUsage` | sync | v10.5.0 |
| `process.uptime` | `uptime(): number` | sync | v0.5.8 |
| `process.availableMemory` | `availableMemory(): number` | sync | v22.0.0 / v20.13.0, stable v24.0.0 |
| `process.constrainedMemory` | `constrainedMemory(): number` | sync | v19.6.0 / v18.15.0, stable v24.0.0 |
| `process.umask()` | `umask(): number` | sync, POSIX-like only (throws `ERR_UNSUPPORTED_OPERATION` on unsupported platforms — verify exact behavior on Windows, see §4) | v0.1.19 |
| `process.umask(mask)` | `umask(mask: number \| string): number` | sync, returns previous mask | v0.1.19 |
| `process.send` | `send(message: object, sendHandle?: net.Server \| net.Socket, options?: { keepOpen?: boolean }, callback?: (error: Error \| null) => void): boolean` | callback (IPC spawned-process only) | v0.5.9 |
| `process.disconnect` | `disconnect(): void` | sync (IPC spawned-process only) | v0.7.2 |
| `process.ref` | `ref(maybeRefable: object): void` | sync | v22.5.0 |
| `process.unref` | `unref(maybeRefable: object): void` | sync | v22.5.0 |
| `process.emitWarning` (options form) | `emitWarning(warning: string \| Error, options?: EmitWarningOptions): void` | sync | v8.0.0 |
| `process.emitWarning` (legacy form) | `emitWarning(warning: string \| Error, type?: string, code?: string, ctor?: Function): void` | sync | v6.0.0 |
| `process.setUncaughtExceptionCaptureCallback` | `setUncaughtExceptionCaptureCallback(fn: ((err: Error) => void) \| null): void` | sync, throws if a `domain` module handler is active | v2.3.0 |
| `process.addUncaughtExceptionCaptureCallback` | `addUncaughtExceptionCaptureCallback(fn: (err: Error) => boolean \| void): void` | sync (Experimental) | v25.9.0 |
| `process.hasUncaughtExceptionCaptureCallback` | `hasUncaughtExceptionCaptureCallback(): boolean` | sync | v9.3.0 |
| `process.dlopen` | `dlopen(module: object, filename: string, flags?: number): void` | sync | v0.1.16 |
| `process.getBuiltinModule` | `getBuiltinModule(id: string): object \| undefined` | sync | v22.3.0 |
| `process.loadEnvFile` | `loadEnvFile(path?: string): void` | sync (despite the name, this is a synchronous file read — verify against v20.10.0/v21.1.0 changelog wording) | v20.10.0 / v21.1.0 |
| `process.getActiveResourcesInfo` | `getActiveResourcesInfo(): string[]` | sync | v19.1.0 |
| `process.setSourceMapsEnabled` | `setSourceMapsEnabled(val: boolean): void` | sync | v12.12.0 / v13.7.0 |
| `process.execve` | `execve(file: string, args?: string[], env?: object): never` | sync (POSIX-like only; not on Windows/IBM i, Experimental) | v23.11.0 / v22.15.0 |
| `process.report.getReport` | `report.getReport(err?: Error): object` | sync | v11.8.0 |
| `process.report.writeReport` | `report.writeReport(filename?: string, err?: Error): string` | sync | v11.8.0 |
| `process.permission.has` | `permission.has(scope: string, reference?: string): boolean` | sync (Experimental) | v20.0.0 |
| `process.channel.ref` | `channel.ref(): void` | sync | (with `channel`) |
| `process.channel.unref` | `channel.unref(): void` | sync | (with `channel`) |
| `process.finalization.register` | `finalization.register(ref: object \| Function, callback: (ref: object, event: string) => void): void` | sync (Experimental) | v22.5.0 |
| `process.finalization.registerBeforeExit` | `finalization.registerBeforeExit(ref: object \| Function, callback: (ref: object, event: string) => void): void` | sync (Experimental) | v22.5.0 |
| `process.finalization.unregister` | `finalization.unregister(ref: object \| Function): void` | sync (Experimental) | v22.5.0 |

**POSIX-only user/group functions** (throw `ERR_UNSUPPORTED_OPERATION` — or an
equivalent, verify exact code — on Windows; not available in Worker threads):

| Function | Signature | Since |
|---|---|---|
| `process.getuid` | `getuid(): number` | v0.1.28 |
| `process.geteuid` | `geteuid(): number` | v2.0.0 |
| `process.setuid` | `setuid(id: number \| string): void` | v0.1.28 |
| `process.seteuid` | `seteuid(id: number \| string): void` | v2.0.0 |
| `process.getgid` | `getgid(): number` | v0.1.31 |
| `process.getegid` | `getegid(): number` | v2.0.0 |
| `process.setgid` | `setgid(id: number \| string): void` | v0.1.31 |
| `process.setegid` | `setegid(id: number \| string): void` | v2.0.0 |
| `process.getgroups` | `getgroups(): number[]` | v0.9.4 |
| `process.setgroups` | `setgroups(groups: ReadonlyArray<string \| number>): void` | v0.9.4 |
| `process.initgroups` | `initgroups(user: string \| number, extraGroup: string \| number): void` | v0.9.4 |

**`EventEmitter`-inherited functions** (not reimplemented; see §5.1):
`on`, `once`, `off` (= `removeListener`), `addListener`, `emit`, `listeners`,
`rawListeners`, `removeAllListeners`, `eventNames`, `listenerCount`,
`getMaxListeners`, `setMaxListeners`, `prependListener`,
`prependOnceListener`. All `sync`.

### Properties & constants

| Property | Type | R/W | Notes |
|---|---|---|---|
| `process.argv` | `string[]` | read-only | `[execPath, scriptPath, ...userArgs]` |
| `process.argv0` | `string` | read-only | original, unprocessed `argv[0]` |
| `process.execPath` | `string` | read-only | absolute path to the running executable |
| `process.execArgv` | `string[]` | read-only | engine-specific CLI flags between executable and script |
| `process.version` | `string` | read-only | e.g. `"v25.0.0"` (RTS reports its own version string — see §4) |
| `process.versions` | `Record<string, string>` | read-only | dependency version map, see §3 `ProcessVersions` |
| `process.release` | `object` | read-only | `{ name, sourceUrl?, headersUrl?, libUrl? }`, see §3 |
| `process.platform` | `string` | read-only | one of `'aix' \| 'darwin' \| 'freebsd' \| 'linux' \| 'openbsd' \| 'sunos' \| 'win32'` |
| `process.arch` | `string` | read-only | one of `'arm' \| 'arm64' \| 'ia32' \| 'loong64' \| 'mips' \| 'mipsel' \| 'ppc64' \| 'riscv64' \| 's390' \| 's390x' \| 'x64'` |
| `process.pid` | `number` | read-only | |
| `process.ppid` | `number` | read-only | |
| `process.title` | `string` | read-write | process title as shown by `ps`/Task Manager |
| `process.debugPort` | `number` | read-write | |
| `process.exitCode` | `number \| string \| null \| undefined` | read-write | default `undefined`; a string must parse as an integer (since v20.0.0) |
| `process.env` | `Record<string, string \| undefined>` | read-write (per-key) | case-insensitive keys on Windows; per-process only, not propagated to the OS or to already-spawned Worker threads; assigning a non-string value is a deprecated implicit `String(value)` coercion |
| `process.config` | `object` | read-only, **frozen since v19.0.0** | build-time `./configure` options |
| `process.connected` | `boolean` | read-only | `true` while the IPC channel to the parent is open (spawned process only) |
| `process.noDeprecation` | `boolean` | read-write | suppress `DeprecationWarning` |
| `process.throwDeprecation` | `boolean` | read-write | throw instead of warn on deprecation |
| `process.traceDeprecation` | `boolean` | read-write | print a stack trace with each deprecation warning |
| `process.traceProcessWarnings` | `boolean` | read-write | print a stack trace with every `'warning'` event |
| `process.sourceMapsEnabled` | `boolean` | read-only | toggled only via `setSourceMapsEnabled()` |
| `process.allowedNodeEnvironmentFlags` | `Set<string>` (special subclass) | read-only | `.has(flag)` is overridden to normalize `--flag`/`flag`/`=value` forms; every other `Set` method behaves normally |
| `process.mainModule` | `object \| undefined` | read-only, **Deprecated** (verify exact `DEPxxxx` code) | legacy CJS-only; use `require.main` instead |
| `process.stdin.fd` | `number` | read-only | always `0` |
| `process.stdout.fd` | `number` | read-only | always `1` |
| `process.stderr.fd` | `number` | read-only | always `2` |
| `process.channel` | `object \| undefined` | read-only | present only when spawned with an `'ipc'` stdio slot |
| `process.report` | `object` | read-only container | sub-properties below |
| `process.report.compact` | `boolean` | read-write | default `false` |
| `process.report.directory` | `string` | read-write | default `""` (cwd) |
| `process.report.filename` | `string` | read-write | default `""` (auto-generated name) |
| `process.report.reportOnFatalError` | `boolean` | read-write | default `false` |
| `process.report.reportOnSignal` | `boolean` | read-write | default `false` |
| `process.report.reportOnUncaughtException` | `boolean` | read-write | default `false` |
| `process.report.excludeEnv` | `boolean` | read-write | default `false` |
| `process.report.signal` | `string` | read-write | default `'SIGUSR2'` |
| `process.permission` | `object` | read-only container | `.has()` method only |
| `process.features.cached_builtins` | `boolean` | read-only | since v12.0.0 |
| `process.features.debug` | `boolean` | read-only | |
| `process.features.inspector` | `boolean` | read-only | since v11.10.0 |
| `process.features.ipv6` | `boolean` | read-only, **Deprecated** v23.4.0/v22.13.0 | always `true` |
| `process.features.require_module` | `boolean` | read-only | since v23.0.0 |
| `process.features.tls` | `boolean` | read-only | |
| `process.features.tls_alpn` | `boolean` | read-only, **Deprecated** v23.4.0 | always `true` |
| `process.features.tls_ocsp` | `boolean` | read-only, **Deprecated** v23.4.0 | always `true` |
| `process.features.tls_sni` | `boolean` | read-only, **Deprecated** v23.4.0 | always `true` |
| `process.features.typescript` | `boolean \| "strip" \| "transform"` | read-only | since v23.0.0, stable v25.2.0 |
| `process.features.uv` | `boolean` | read-only, **Deprecated** v23.4.0 | always `true` |

### Events

Every listener is registered via the inherited `EventEmitter` surface
(`process.on(name, listener)`, etc.).

| Event | Listener signature | Since | Notes |
|---|---|---|---|
| `'exit'` | `(code: number) => void` | v0.1.7 | last event before the process image ends; **only synchronous work** is honored — any I/O/timer scheduled here is discarded |
| `'beforeExit'` | `(code: number) => void` | v0.11.12 | fires when the event loop has no further work; a listener may schedule new async work to keep the process alive; **not** fired for `process.exit()`, an uncaught exception, or a fatal error |
| `'uncaughtException'` | `(err: Error, origin: 'uncaughtException' \| 'unhandledRejection') => void` | v0.1.18 | default behavior with no listener: print to stderr + `process.exit(1)` |
| `'uncaughtExceptionMonitor'` | `(err: Error, origin: 'uncaughtException' \| 'unhandledRejection') => void` | v13.7.0 | observation-only, fires *before* `'uncaughtException'`, does not suppress default handling |
| `'unhandledRejection'` | `(reason: unknown, promise: Promise<unknown>) => void` | v1.4.1 | behavior with no listener governed by `--unhandled-rejections` (default: throw) |
| `'rejectionHandled'` | `(promise: Promise<unknown>) => void` | v1.4.1 | fires when a `.catch()` is attached to a promise *after* it already rejected unhandled |
| `'warning'` | `(warning: Error) => void` | v6.0.0 | `warning.name`, `.message`, `.stack` (and optionally `.code`, `.detail`) |
| `'message'` | `(message: unknown, sendHandle?: net.Server \| net.Socket) => void` | v0.5.10 | IPC spawned-process only |
| `'disconnect'` | `() => void` | v0.7.7 | IPC spawned-process only, fires once the channel closes |
| `'multipleResolves'` (Deprecated, removed — verify exact removal version) | `(type: 'resolve' \| 'reject', promise: Promise<unknown>, value: unknown) => void` | v10.12.0, removed later | flag with `(verify)` — confirm current status in v25 |
| `'worker'` | `(worker: Worker) => void` | v16.2.0 | fires on the main thread when a new `worker_threads.Worker` is created |
| `'workerMessage'` | `(value: unknown, source: number) => void` | v22.5.0 / v20.19.0 | delivered via `postMessageToThread()`; `source` is the sending thread id, or `0` for the main thread |
| Signal events (`'SIGINT'`, `'SIGTERM'`, `'SIGHUP'`, `'SIGUSR1'`, `'SIGUSR2'`, `'SIGBREAK'` (Windows), etc.) | `(signal: string) => void` | v0.9.0 | see §4 for the full per-platform signal table; not available inside Worker threads |

## 3. Types & option objects

```ts
interface MemoryUsage {
  rss: number;
  heapTotal: number;
  heapUsed: number;
  external: number;
  arrayBuffers: number; // since v15.9.0
}

interface CpuUsage {
  user: number;   // microseconds
  system: number; // microseconds
}

interface ResourceUsage {
  userCPUTime: number;
  systemCPUTime: number;
  maxRSS: number;
  sharedMemorySize: number;
  unsharedDataSize: number;
  unsharedStackSize: number;
  minorPageFault: number;
  majorPageFault: number;
  swapCount: number;
  fsRead: number;
  fsWrite: number;
  ipcSent: number;
  ipcReceived: number;
  signalsCount: number;
  voluntaryContextSwitches: number;
  involuntaryContextSwitches: number;
}

interface EmitWarningOptions {
  type?: string;    // default 'Warning'
  code?: string;
  ctor?: Function;
  detail?: string;
}

interface ProcessVersions {
  node: string;
  v8: string;
  uv: string;
  zlib: string;
  brotli: string;
  ares: string;
  modules: string;
  nghttp2: string;
  napi: string;
  llhttp: string;
  undici: string;
  acorn: string;
  cjs_module_lexer: string;
  simdjson: string;
  icu?: string;      // only present in an ICU-enabled build
  openssl?: string;  // only present when TLS/crypto support is compiled in
  unicode?: string;
  // RTS note: this map is regenerated to describe RTS's OWN dependency set
  // (its Rust crates), not literally copied from Node's V8/libuv stack —
  // see §4 for exactly which keys RTS reports and why.
}

interface ProcessRelease {
  name: string;          // 'node' (RTS reports its own product name — verify)
  sourceUrl?: string;
  headersUrl?: string;
  libUrl?: string;        // Windows only
}

interface ProcessConfig {
  target_defaults: Record<string, unknown>;
  variables: Record<string, unknown>;
}

// process.report.getReport() / writeReport() return/produce an object with
// (at minimum, exact shape is implementation-defined and version-dependent —
// mark contents "(verify)" against the real Node v25 report schema):
interface DiagnosticReport {
  header: Record<string, unknown>;
  javascriptStack?: Record<string, unknown>;
  nativeStack?: unknown[];
  javascriptHeap?: Record<string, unknown>;
  resourceUsage?: ResourceUsage;
  libuv?: unknown[];
  environmentVariables?: Record<string, string>;
  sharedObjects?: string[];
  [key: string]: unknown;
}

// process.getActiveResourcesInfo() element examples: 'TTYWrap', 'Timeout',
// 'TCPWrap', 'PipeWrap' — RTS reports its own resource-kind strings drawn
// from its HandleTable Entry variants (see §5.1), not V8/libuv's names.

type Signal =
  | 'SIGABRT' | 'SIGALRM' | 'SIGBUS' | 'SIGCHLD' | 'SIGCONT' | 'SIGFPE'
  | 'SIGHUP' | 'SIGILL' | 'SIGINT' | 'SIGIO' | 'SIGIOT' | 'SIGKILL'
  | 'SIGPIPE' | 'SIGPOLL' | 'SIGPROF' | 'SIGPWR' | 'SIGQUIT' | 'SIGSEGV'
  | 'SIGSTKFLT' | 'SIGSTOP' | 'SIGSYS' | 'SIGTERM' | 'SIGTRAP' | 'SIGTSTP'
  | 'SIGTTIN' | 'SIGTTOU' | 'SIGUNUSED' | 'SIGURG' | 'SIGUSR1' | 'SIGUSR2'
  | 'SIGVTALRM' | 'SIGWINCH' | 'SIGXCPU' | 'SIGXFSZ'
  | 'SIGBREAK' /* Windows only */ | 'SIGLOST' /* Windows only, deprecated */;
```

## 4. Node semantics & edge cases

- **`process.env` string coercion.** Every value read from `process.env` is a
  `string`; assigning a non-string implicitly stringifies it (Node emits a
  `DeprecationWarning` for this — RTS should mirror that warning, not silently
  coerce). Deleting a key (`delete process.env.FOO`) removes it. On Windows,
  key lookups are case-insensitive (`process.env.Path` and
  `process.env.PATH` alias the same variable); on POSIX they are
  case-sensitive.
- **`process.env` mutation scope.** Writes affect only the current process's
  in-memory copy; they are **not** propagated to the real OS environment
  block seen by unrelated processes, nor to `worker_threads` Workers already
  running (a new Worker inherits a **copy** taken at spawn time, same as
  Node).
- **`process.exit(code)` truncation risk.** Forces immediate termination
  without waiting for the event loop to drain — any pending
  `stdout`/`stderr` writes, unresolved Promises, or scheduled `setTimeout`
  callbacks may be lost. Node's own docs discourage calling it directly in
  favor of setting `process.exitCode` and letting the process exit naturally.
  Prefer the latter in RTS-authored code; `process.exit()` must still be
  supported bit-for-bit for compatibility.
- **Exit code rules.** `process.exitCode` accepts `number | string | null |
  undefined`; a string value must parse as an integer (as of v20.0.0) or
  Node throws. If both `process.exit(code)` and `process.exitCode` are used,
  the explicit `exit(code)` argument wins.
- **`'exit'` listener constraints.** The event loop is not running when
  `'exit'` fires — only synchronous code executes; anything async
  (a `setTimeout`, an unresolved Promise `.then`) scheduled inside the
  handler is silently dropped, never runs.
- **`'beforeExit'` vs `'exit'`.** `'beforeExit'` fires only when Node would
  otherwise exit due to an empty event loop (not for `process.exit()`,
  signals, or uncaught exceptions); a handler *can* keep the process alive
  by scheduling new work. `'exit'` always fires exactly once as the true
  final step, no matter how termination was triggered (except `SIGKILL`
  and `process.abort()`, which bypass all JS entirely).
- **Windows signal support is a strict subset of POSIX.** `SIGINT`,
  `SIGTERM`, and `SIGKILL` **always terminate the process unconditionally**
  on Windows regardless of listeners (no graceful override possible).
  `SIGBREAK` (Ctrl+Break) is Windows-only. `SIGHUP` is delivered only when
  the console window is closed, and only in narrow circumstances. Sending an
  arbitrary POSIX signal number via `process.kill(pid, n)` on Windows is not
  meaningful the way it is on POSIX. `SIGKILL`/`SIGSTOP` can never have a
  listener on POSIX either — attempting `process.on('SIGKILL', ...)` is a
  silent no-op there (the OS never delivers them to a running handler).
- **`process.kill(pid, signal)` semantics.** Despite the name, this **sends a
  signal**, it does not necessarily terminate — the default `signal` is
  `'SIGTERM'`. Sending signal `0` is the standard POSIX idiom for "test that
  the process exists and I have permission to signal it" without actually
  signaling; throws `ESRCH` if no such process, `EPERM` if permission is
  denied. Returns `true` on success (does not guarantee the *target* process
  actually reacted, only that the signal was delivered by the OS).
  `process.kill(process.pid, ...)` can signal the current process itself.
- **`process.abort()`** immediately terminates and (platform-permitting)
  produces a core dump; not available for Worker threads.
- **`process.chdir()` and `process.umask()` without a Worker.** Neither is
  available inside a `worker_threads.Worker` (process-global OS state, not
  per-thread); calling them there throws.
- **`process.hrtime()` has no relationship to wall-clock/`Date.now()`** — it
  measures elapsed time since an arbitrary, unspecified reference point
  (typically process/system start), monotonic and never adjusted by NTP/
  clock changes. Only *differences* between two calls are meaningful.
  `process.hrtime.bigint()` returns the same monotonic clock as a single
  `bigint` nanosecond count instead of a `[seconds, nanoseconds]` tuple —
  prefer it for new code (`process.hrtime()` itself is legacy-shaped but not
  deprecated).
- **`process.memoryUsage()` cost.** On some platforms gathering
  `arrayBuffers` may be more expensive than the other fields; Node does not
  document a cheap partial-fetch flag other than the standalone
  `memoryUsage.rss()` fast path for just RSS.
- **`process.cpuUsage(previousValue)` diffing.** When given a prior
  `CpuUsage` snapshot, subtracts field-by-field and returns the delta;
  passing a malformed object throws (`ERR_INVALID_ARG_TYPE` — verify exact
  code).
- **`process.title` write has OS-dependent effective length.** On some
  POSIX systems the settable title length is bounded by `argv`/`environ`
  memory layout; excess characters are silently truncated (verify exact RTS
  behavior — will differ from V8/libuv's specific truncation length since
  RTS's process image layout is different).
- **`process.config` is frozen since v19.0.0** — `Object.freeze()`'d; write
  attempts are no-ops in sloppy mode, throw in strict mode/ESM.
- **`process.allowedNodeEnvironmentFlags.has(flag)` normalizes input** —
  accepts with or without a leading `--`, and with or without a
  `=value` suffix, e.g. `.has('--max-old-space-size')` and
  `.has('max-old-space-size=100')` both resolve consistently; every other
  `Set` method (`.add`, `.forEach`, iteration, `.size`) behaves like a
  normal (read-effectively-only in practice) `Set<string>`.
- **`process.report.reportOnFatalError`/`reportOnSignal`/
  `reportOnUncaughtException`** default to `false`; when enabled, Node
  writes a JSON diagnostic report file on the corresponding trigger.
  `process.report.signal` (default `'SIGUSR2'`) is the signal that, when
  `reportOnSignal` is `true`, triggers report generation — has no meaning on
  Windows the way it does on POSIX (verify RTS's approach given Windows has
  no `SIGUSR2`).
- **`process.dlopen`** is the low-level primitive `require()` uses internally
  to load native `.node` addons; almost never called directly by user code.
  RTS's equivalent native-addon loading is the existing `rts-napi` crate
  (`__RTS_FN_NS_NAPI_LOAD_ADDON`) — `process.dlopen` in `node:process` should
  be a thin compatibility wrapper over that existing mechanism, not a new
  implementation (cross-reference, not part of this module's own build).
- **`process.getBuiltinModule(id)`** returns the built-in module object
  *without* triggering `require()`'s module-registration side effects (no
  entry added to `require.cache`); returns `undefined` for a non-existent or
  disallowed id. Useful for user land ESM loaders that need to detect "is
  this specifier a Node built-in" without fully loading it.
- **`process.loadEnvFile(path)`** parses a dotenv-style file directly into
  `process.env` (no template/expansion beyond basic `KEY=VALUE`/quoting
  rules — verify exact quoting/escaping/comment syntax supported); throws
  `ENOENT`-shaped error if `path` (or the default `.env`) does not exist.
- **`process.execve` fully replaces the process image** (POSIX `execve(2)`
  semantics) — nothing after the call ever runs; on failure it throws
  instead (image not replaced). **Not available on Windows or IBM i.**
  Marked Experimental.
- **`process.mainModule` is deprecated** in favor of `require.main`; only
  meaningful for CJS entry points (`undefined` in an ESM entry point).
- **Deprecated `process.features.*` flags** (`ipv6`, `tls_alpn`, `tls_ocsp`,
  `tls_sni`, `uv`) are permanently frozen to a fixed value (mostly `true`)
  for compatibility — RTS should hardcode the same fixed values rather than
  attempt to derive them, since upstream Node itself no longer varies them.
- **`process.send`/`disconnect`/`channel`/`'message'`/`'disconnect'` event**
  only exist when the process was spawned by a parent via
  `child_process.fork()`/`spawn(..., {stdio: [...,'ipc']})`; on a normally
  launched (non-child) process these are all `undefined`/no-ops — RTS must
  detect "was I spawned with an IPC slot" at process bootstrap (cross-module
  coordination with `node:child_process`, see §5.7).
- **`'uncaughtException'` handler safety.** Node's own docs warn: resuming
  normal operation after an uncaught exception is unsafe (the process may be
  in an inconsistent/leaked state) — the *only* recommended safe action
  inside the handler is synchronous cleanup followed by `process.exit()`.
  RTS's spec/impl should carry this same warning to users, not attempt to
  make "safe resume" actually safe.
- **`'unhandledRejection'` default behavior is configurable** via the
  `--unhandled-rejections` CLI flag (`throw`/`strict`/`warn`/`none`); default
  is effectively `throw` (unhandled rejections become uncaught exceptions)
  as of modern Node — confirm and document RTS's chosen default explicitly
  since RTS has no CLI-flag-parity guarantee for every V8 flag.

## 5. RTS implementation notes

### 5.1 Native impl mapping

- **`argv`/`argv0`/`execPath`/`execArgv`/`pid`/`ppid`/`cwd`/`chdir`/`title`** —
  `std::env::args()`, `std::env::current_exe()`, `std::process::id()`,
  `std::env::current_dir()`/`set_current_dir()`. Parent PID (`ppid`) needs a
  platform primitive not in `std`: `sysinfo` crate or a raw syscall
  (`getppid()` on POSIX via `libc::getppid()`; `NtQueryInformationProcess`
  or `CreateToolhelp32Snapshot` parent-walk on Windows) — a small,
  independent, rts-node-owned dependency (never borrowed from `rts-std`).
  Process **title** has no direct `std` equivalent either: POSIX uses
  `prctl(PR_SET_NAME)`/overwriting the `argv[0]` memory; Windows has no true
  process-title concept for a console app the way `ps` shows one — RTS
  should set the console window title via `SetConsoleTitleW` as the closest
  analog and document the platform gap.
- **`env`** — `std::env::vars()`/`var()`/`set_var()`/`remove_var()`, wrapped
  in a `.ts` `Proxy`-like object (or a real engine `Proxy` per the
  primordial doctrine) so `process.env.FOO = 123` triggers the
  deprecated-coercion warning path and `delete process.env.FOO` calls
  `remove_var`. Case-insensitive key matching on Windows needs an
  rts-node-owned case-folding wrapper (native `std::env` is
  case-*sensitive* even on Windows at the Rust layer, since it talks to the
  raw environment block — RTS must normalize itself, matching Node's own
  documented Windows behavior).
- **`platform`/`arch`** — `std::env::consts::OS`/`ARCH`, translated to
  Node's exact string vocabulary (Rust's `"macos"` → Node's `"darwin"`,
  Rust's `"windows"` → Node's `"win32"`, etc. — a small static lookup table,
  not a 1:1 passthrough).
- **`version`/`versions`/`release`** — static data baked in at RTS build
  time (via `build.rs` reading `Cargo.toml`/crate versions of RTS's own
  dependency set: Cranelift, the RTS engine version, tokio, rustls, etc.);
  RTS reports **its own** version identity, not a spoofed Node version
  string — exact key set for `versions` needs an owner decision (§7).
- **`memoryUsage`/`cpuUsage`/`resourceUsage`/`threadCpuUsage`** — POSIX:
  `getrusage(2)` via `libc::getrusage`; RSS specifically also via
  `/proc/self/statm` (Linux) or `task_info` (macOS, `mach2`/`libc` crate).
  Windows: `GetProcessMemoryInfo`/`K32GetProcessMemoryInfo`
  (`PROCESS_MEMORY_COUNTERS_EX`) for RSS/working-set, `GetProcessTimes` for
  user/kernel CPU time. `arrayBuffers`/`heapUsed`/`heapTotal` map onto the
  **RTS engine's own GC heap accounting** (`rts-engine::gc` already tracks
  handle-table/allocation stats) rather than any V8-specific concept — needs
  a small new accessor exposed from `rts-engine` (not `rts-std`) since
  `rts-node` cannot reach `rts-std`'s GC internals and shouldn't need to;
  the engine's GC module is the natural, dependency-clean home for
  "how much heap memory is live right now" (flagged in §5.7 only if it does
  not already have a public accessor).
- **`hrtime`/`hrtime.bigint`/`uptime`** — `std::time::Instant` (monotonic,
  matches Node's guarantee); `uptime()` is elapsed time since an `Instant`
  captured once at process bootstrap.
- **`availableMemory`/`constrainedMemory`** — POSIX: `sysinfo(2)`/
  `/proc/meminfo` (Linux) or `sysctl` (macOS/BSD) for available; cgroups v1/
  v2 limit files (`/sys/fs/cgroup/memory.max` etc.) for constrained. Windows:
  `GlobalMemoryStatusEx` for available; Job Object memory limits
  (`QueryInformationJobObject`) for constrained, or `0` if unconstrained.
  A small, rts-node-owned `sysinfo`-style dependency (or hand-rolled
  per-platform reads) — independent of any equivalent already in `rts-std`.
- **`umask`** — POSIX `libc::umask()` (note: genuinely process-global and
  not thread-safe to read without a transient set-then-restore, since POSIX
  `umask(2)` has no pure "peek" mode — RTS's getter form must do the classic
  set-new-then-restore-old dance, guarded by a `Mutex` for
  thread-safety, see §5.4). **No real concept on Windows** — Node throws or
  no-ops there (verify exact behavior; RTS should match).
- **`kill`/signals** — POSIX: `libc::kill(pid, sig)`, signal name↔number
  table hand-rolled (mirrors `signal(7)`). Windows: `TerminateProcess` for
  the terminate-unconditionally signals (`SIGINT`/`SIGTERM`/`SIGKILL`),
  `GenerateConsoleCtrlEvent` for `SIGINT`/`SIGBREAK` delivery to a console
  process group. Registering a JS listener for a POSIX signal needs a
  real OS-level signal handler installed once per signal name on first
  `.on(signal, ...)` call, forwarding into the `process` EventEmitter — the
  `signal-hook` crate (or hand-rolled `sigaction`) is a reasonable
  rts-node-owned dependency for this on POSIX; Windows uses
  `SetConsoleCtrlHandler`.
- **`getuid`/`geteuid`/`setuid`/…/`getgroups`/`setgroups`/`initgroups`** —
  thin wrappers over `libc::getuid`/`geteuid`/`setuid`/`seteuid`/`getgid`/
  `getegid`/`setgid`/`setegid`/`getgroups`/`setgroups`/`initgroups`
  (POSIX-only; compiled out / return an unsupported-operation error on
  Windows).
- **`execve`** — `libc::execve` directly (POSIX-like only; compiled out on
  Windows/IBM i, matching Node's own platform restriction).
- **`stdout`/`stderr`/`stdin`** — `std::io::stdout()`/`stderr()`/`stdin()`
  wrapped as the `stream.Writable`/`Readable` shapes `node:stream` defines;
  `node:process` itself only needs to *bind* the three global instances and
  expose `.fd`, not reimplement stream machinery (cross-module dependency on
  `node:stream`, both are rts-node-owned so no crate-boundary issue, only an
  implementation-order one — see §5.8).
- **`dlopen`** — delegates to the existing `rts-napi` addon-loading pipeline
  (`__RTS_FN_NS_NAPI_LOAD_ADDON`); `node:process`'s own native surface for
  this is a thin forwarding shim, not a reimplementation.
- **`report.getReport`/`writeReport`** — assembles a JSON object from the
  above native accessors (memory/cpu/resource usage, `env` unless
  `excludeEnv`, loaded-module list, a Rust-native backtrace via the
  `backtrace` crate or RTS's existing crash-handler infra in `src/crash.rs`
  for `nativeStack`) plus RTS/engine-specific fields (Cranelift IR unit
  counts, GC stats) in place of V8/libuv-specific ones.
- **`getActiveResourcesInfo`** — enumerates live entries across the RTS
  engine's `HandleTable` shards (already shard-aware, see `02-runtime.md`)
  and maps each `Entry` variant to a Node-shaped resource-kind string
  (`Entry::TcpStream` → `'TCPWrap'`-equivalent, etc.) — needs a small new
  read-only iteration accessor on `rts-engine::heap::handles` if one does
  not already exist (flag in §5.7 only if missing).
- **`finalization.*`** — a small rts-node-owned registry
  `Vec<(WeakRef-like ref, callback, phase)>` consulted at the `'exit'`/
  `'beforeExit'` dispatch points; "was `ref` GC'd" needs the engine's
  existing `WeakRef` GC-coupling (primordial, `rts-engine`) — reachable
  without any `rts-std` dependency.
- **`setUncaughtExceptionCaptureCallback`/
  `addUncaughtExceptionCaptureCallback`/
  `hasUncaughtExceptionCaptureCallback`** — a simple rts-node-owned
  `thread_local!`/`Mutex`-guarded callback slot (or LIFO `Vec` for the
  `add*` form) consulted by the engine's top-level uncaught-exception
  dispatch path before falling back to the default print+exit behavior.
- **`emitWarning`** — formats and routes to the same `'warning'` event
  dispatch as any internal RTS-emitted deprecation warning; honors
  `noDeprecation`/`throwDeprecation`/`traceDeprecation` for the
  `'DeprecationWarning'` type specifically.
- **`setSourceMapsEnabled`** — toggles whether RTS's own stack-trace
  formatting consults a source-map (relevant once RTS ships source maps for
  compiled/bundled output — currently a no-op flag store if source-map
  support does not exist yet; document as a known gap if so).

### 5.2 ABI surface

Proposed symbol convention: `__RTS_FN_NODE_PROCESS_<NAME>`. Scalars cross
directly (`I32`/`I64`/`U64`/`F64`/`Bool`); strings cross as `StrPtr`
(`ptr:i64, len:i64`, UTF-8); the few object-shaped return values
(`MemoryUsage`, `CpuUsage`, `ResourceUsage`, `ProcessVersions`, `ProcessRelease`,
diagnostic report) are assembled by a `.ts` shim from multiple granular
scalar-returning externs (mirrors the `node:buffer` doc's pattern for
`Blob`/`File` metadata) rather than one extern building a whole JS object,
so the ABI stays flat and typed.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_PROCESS_ARGV_COUNT` | — | `I64` | |
| `__RTS_FN_NODE_PROCESS_ARGV_AT` | `I64 index` | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_ARGV0` | — | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_EXEC_PATH` | — | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_EXEC_ARGV_COUNT` / `_AT` | — / `I64 index` | `I64` / `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_VERSION` | — | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_VERSIONS_KEY` | `StrPtr key` | `StrPtr` (empty if absent) | one call per `ProcessVersions` field |
| `__RTS_FN_NODE_PROCESS_RELEASE_NAME` / `_SOURCE_URL` / `_HEADERS_URL` / `_LIB_URL` | — | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_PLATFORM` | — | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_ARCH` | — | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_PID` | — | `I64` | |
| `__RTS_FN_NODE_PROCESS_PPID` | — | `I64` | |
| `__RTS_FN_NODE_PROCESS_GET_TITLE` / `_SET_TITLE` | — / `StrPtr title` | `StrPtr` / `Void` | |
| `__RTS_FN_NODE_PROCESS_GET_DEBUG_PORT` / `_SET_DEBUG_PORT` | — / `I64 port` | `I64` / `Void` | |
| `__RTS_FN_NODE_PROCESS_CWD` | — | `StrPtr` | |
| `__RTS_FN_NODE_PROCESS_CHDIR` | `StrPtr dir` | `Void`, traps to `ENOENT`/`ENOTDIR` | |
| `__RTS_FN_NODE_PROCESS_ENV_GET` | `StrPtr key` | `StrPtr` (sentinel-length for "unset") | case-fold on Windows internally |
| `__RTS_FN_NODE_PROCESS_ENV_SET` | `StrPtr key, StrPtr value` | `Void` | |
| `__RTS_FN_NODE_PROCESS_ENV_DELETE` | `StrPtr key` | `Void` | |
| `__RTS_FN_NODE_PROCESS_ENV_KEYS` | — | `Handle` (array of strings) | for `Object.keys(process.env)`/iteration |
| `__RTS_FN_NODE_PROCESS_EXIT` | `I32 code` | `Void` (never returns) | |
| `__RTS_FN_NODE_PROCESS_ABORT` | — | `Void` (never returns) | |
| `__RTS_FN_NODE_PROCESS_UPTIME` | — | `F64` | seconds |
| `__RTS_FN_NODE_PROCESS_HRTIME_SEC` / `_NSEC` | — | `I64` / `I64` | split tuple; `.ts` combines |
| `__RTS_FN_NODE_PROCESS_HRTIME_BIGINT` | — | `I64` (reinterpreted as `bigint` primordial) | nanoseconds |
| `__RTS_FN_NODE_PROCESS_MEM_RSS` / `_HEAP_TOTAL` / `_HEAP_USED` / `_EXTERNAL` / `_ARRAY_BUFFERS` | — | `U64` each | assembled into `MemoryUsage` by `.ts` |
| `__RTS_FN_NODE_PROCESS_CPU_USER` / `_SYSTEM` | — | `U64` each | assembled into `CpuUsage` |
| `__RTS_FN_NODE_PROCESS_THREAD_CPU_USER` / `_SYSTEM` | — | `U64` each | per calling OS thread |
| `__RTS_FN_NODE_PROCESS_RESOURCE_USAGE_FIELD` | `I32 field_id` | `U64` | one dispatch symbol + an id enum instead of 16 separate symbols (implementer's choice; either is acceptable, this spec picks the compact form) |
| `__RTS_FN_NODE_PROCESS_AVAILABLE_MEMORY` | — | `U64` | |
| `__RTS_FN_NODE_PROCESS_CONSTRAINED_MEMORY` | — | `U64` (0 = unconstrained) | |
| `__RTS_FN_NODE_PROCESS_UMASK_GET` | — | `I32` | POSIX only |
| `__RTS_FN_NODE_PROCESS_UMASK_SET` | `I32 mask` | `I32` (previous mask) | POSIX only |
| `__RTS_FN_NODE_PROCESS_KILL` | `I64 pid, StrPtr signal` | `Bool`, traps `ESRCH`/`EPERM` | |
| `__RTS_FN_NODE_PROCESS_SIGNAL_ON` | `StrPtr signal, Handle callback_fn` | `Void` | installs the OS handler on first registration for that name |
| `__RTS_FN_NODE_PROCESS_SEND` | `Handle message_obj, Handle send_handle_or_zero, Bool keep_open` | `Bool` | IPC only |
| `__RTS_FN_NODE_PROCESS_DISCONNECT` | — | `Void` | IPC only |
| `__RTS_FN_NODE_PROCESS_CONNECTED` | — | `Bool` | |
| `__RTS_FN_NODE_PROCESS_CHANNEL_REF` / `_UNREF` | — | `Void` | IPC only |
| `__RTS_FN_NODE_PROCESS_REF` / `_UNREF` | `Handle refable` | `Void` | timers/sockets |
| `__RTS_FN_NODE_PROCESS_EMIT_WARNING` | `StrPtr message, StrPtr type, StrPtr code, StrPtr detail` | `Void` | empty `StrPtr` = absent field |
| `__RTS_FN_NODE_PROCESS_SET_UNCAUGHT_CAPTURE` | `Handle fn_or_zero` | `Void` | |
| `__RTS_FN_NODE_PROCESS_ADD_UNCAUGHT_CAPTURE` | `Handle fn` | `Void` | Experimental, LIFO list |
| `__RTS_FN_NODE_PROCESS_HAS_UNCAUGHT_CAPTURE` | — | `Bool` | |
| `__RTS_FN_NODE_PROCESS_DLOPEN` | `Handle module_obj, StrPtr filename, I32 flags` | `Void` | forwards to `rts-napi` |
| `__RTS_FN_NODE_PROCESS_GET_BUILTIN_MODULE` | `StrPtr id` | `Handle` (0 = `undefined`) | |
| `__RTS_FN_NODE_PROCESS_LOAD_ENV_FILE` | `StrPtr path` | `Void`, traps `ENOENT` | empty `StrPtr` = default `.env` |
| `__RTS_FN_NODE_PROCESS_GET_ACTIVE_RESOURCES_INFO` | — | `Handle` (array of strings) | |
| `__RTS_FN_NODE_PROCESS_SET_SOURCE_MAPS_ENABLED` | `Bool val` | `Void` | |
| `__RTS_FN_NODE_PROCESS_SOURCE_MAPS_ENABLED` | — | `Bool` | |
| `__RTS_FN_NODE_PROCESS_EXECVE` | `StrPtr file, Handle args_vec, Handle env_obj_or_zero` | `Void` (never returns on success), traps `ENOENT`/`EACCES` | POSIX-like only |
| `__RTS_FN_NODE_PROCESS_REPORT_GET` | `Handle err_or_zero` | `Handle` (report object) | |
| `__RTS_FN_NODE_PROCESS_REPORT_WRITE` | `StrPtr filename, Handle err_or_zero` | `StrPtr` (written filename) | |
| `__RTS_FN_NODE_PROCESS_PERMISSION_HAS` | `StrPtr scope, StrPtr reference` | `Bool` | empty `reference` = omitted |
| `__RTS_FN_NODE_PROCESS_GETUID` / `_GETEUID` / `_GETGID` / `_GETEGID` | — | `I64` | POSIX only |
| `__RTS_FN_NODE_PROCESS_SETUID` / `_SETEUID` / `_SETGID` / `_SETEGID` | `I64 id` | `Void`, traps `EPERM`/`EINVAL` | POSIX only |
| `__RTS_FN_NODE_PROCESS_GETGROUPS` | — | `Handle` (array of ints) | POSIX only |
| `__RTS_FN_NODE_PROCESS_SETGROUPS` | `Handle groups_vec` | `Void` | POSIX only |
| `__RTS_FN_NODE_PROCESS_INITGROUPS` | `StrPtr user, I64 extra_group` | `Void` | POSIX only |
| `__RTS_FN_NODE_PROCESS_FINALIZATION_REGISTER` | `Handle ref, Handle callback, I32 phase` | `Void` | `phase`: 0=exit, 1=beforeExit |
| `__RTS_FN_NODE_PROCESS_FINALIZATION_UNREGISTER` | `Handle ref` | `Void` | |

`.ts` shim vs native extern split: **the `process` singleton object, its
`EventEmitter` prototype wiring, `report`/`permission`/`channel`/
`finalization` namespace objects, and every multi-field return-shape
assembly** (`memoryUsage()`, `cpuUsage()`, `resourceUsage()`, `versions`,
`release`) live in an rts-node `.ts` prelude; **every OS-level read/mutation**
is a native extern per the table above. Rich diagnostic/report objects and
string arrays (`env` keys, `execArgv`, `getActiveResourcesInfo`) cross as
`Handle` into engine-owned Array/Object shapes, populated field-by-field from
Rust.

### 5.3 Async model

- **Overwhelming majority: fully synchronous.** Every property getter,
  `cwd`/`chdir`/`kill`/`hrtime`/`memoryUsage`/`cpuUsage`/`resourceUsage`/
  `uptime`/`umask`/uid-gid family/`execve`/`report.*`/`permission.has`/
  `dlopen`/`getBuiltinModule`/`getActiveResourcesInfo`/
  `setSourceMapsEnabled` complete against already-resident process/OS state
  with no I/O wait.
- **`process.nextTick(callback, ...args)`** is **callback-based but not a
  Promise/microtask** in the ECMAScript sense — it is Node's own
  higher-priority queue, drained fully before the event loop proceeds to the
  next phase (and before Promise microtasks, in real Node's documented
  ordering — verify RTS's event-loop phase ordering matches, since RTS's
  event loop is its own implementation, not libuv). Needs a dedicated
  "next-tick queue" distinct from RTS's existing Promise microtask queue
  (see §5.7 — currently that queue lives in `rts-std`).
- **`process.send(message, ...)`** is callback-style (`callback(error)`) and
  requires the IPC channel machinery (a `net.Socket`-shaped pipe to the
  parent process) — this is genuinely async I/O, needing the shared runtime
  (§5.7).
- **`process.loadEnvFile(path)`** is a plain synchronous file read despite
  living alongside otherwise-async-flavored Node APIs — no Promise/
  callback is involved per the fetched docs; confirm against real Node
  behavior before finalizing (§7).
- **Signal delivery (`process.on('SIGINT', ...)`)** is inherently
  asynchronous from the JS program's point of view (an OS-level interrupt
  arriving at an arbitrary point), but the RTS-side dispatch — once the
  native OS handler forwards into the engine — is a synchronous emit onto
  the `process` EventEmitter from whatever thread/mechanism the event loop
  uses to safely marshal an async-signal-context event into JS-land (a
  signal handler itself must not run arbitrary JS directly; needs a
  pipe-to-self or an atomic-flag-plus-poll bridge, a well-known pattern for
  self-pipe signal handling — implementation detail for §5.8).
- **`'exit'`/`'beforeExit'`/`'uncaughtException'`/`'unhandledRejection'`/
  `'rejectionHandled'`/`'warning'`** are all synchronous emits triggered by
  engine-internal lifecycle points (some of which — unhandled
  rejection/rejection-handled — depend on the Promise subsystem's reject/
  settle bookkeeping, currently in `rts-std::promise` — flagged in §5.7).

### 5.4 Multithread / worker interaction

- **`process.env`, `cwd`, `title`, `umask`, uid/gid state are genuinely
  OS-process-global**, not per-thread — but per the RTS threading model
  (`docs/specs/rts-threading-model.md`), each `worker_threads.Worker` gets
  its own **region** with its own *view* onto these. Node's own documented
  behavior: a Worker's `process.env` is a **copy** taken at spawn time (
  mutations in one thread do not propagate to another); `process.chdir()`/
  `process.umask()` are **not available at all** inside a Worker (they
  would race a genuinely-global OS resource across threads with no
  coordination — Node disallows this outright, and RTS should too). RTS's
  own env-copy-per-region should be implemented as a per-thread snapshot
  taken at Worker-region creation, mirroring "promotion on publication" only
  in the sense that no promotion happens here at all — it is a one-shot
  fork-copy, deliberately *not* shared/promoted state.
- **`process.pid`/`ppid`/`platform`/`arch`/`version`/`versions`/`release`/
  `argv`/`execPath`** are process-wide immutable facts — safe to read from
  any thread/region with no synchronization (`OnceLock`/static, computed
  once at bootstrap).
- **`process.exitCode`/`process.title`/`process.debugPort`** are mutable
  process-global scalars — need a `Mutex`/`AtomicI64`-guarded
  `OnceLock<...>` per the `02-runtime.md` shared-state pattern so every
  thread observes the same value (unlike `env`, these are **not**
  per-Worker copies in real Node — a Worker actually shares the *reporting*
  of exit code with the main thread's own exit sequence per Node's
  documented Worker lifecycle; verify exact RTS semantics wanted here, §7).
- **The `process` EventEmitter's listener registry itself is per-isolate in
  real Node** (each Worker has an entirely separate `process` object/global,
  not a shared one) — RTS's per-thread-region model maps naturally: each
  RTS thread/region gets its **own** `process` singleton instance (its own
  listener list, own signal-handler registration set), never a
  cross-thread-shared one. Only the main thread's `process` receives real
  OS signals; a Worker's synthetic `process.on('SIGINT', ...)` (if RTS
  chooses to support it at all — Node itself does **not** deliver real OS
  signals to Workers) should be a documented no-op/gap, matching Node.
- **`process.umask()`'s get-via-set-then-restore dance** (§5.1) must be
  `Mutex`-guarded at the OS-process level regardless of the RTS threading
  model, since the underlying POSIX `umask(2)` call has zero built-in
  thread-safety and affects the whole OS process, not a per-RTS-thread
  region — an RTS-side `Mutex<()>` guard specific to this one call is
  required no matter how the rest of the module is threaded.
- **`process.getActiveResourcesInfo()`** must enumerate across **all**
  shards of the engine's `HandleTable` (already 32-way shard-aware, see
  `02-runtime.md`), not just the calling thread's own allocations — matches
  Node's "whole process" semantics for this diagnostic.
- **`worker_threads.Worker` itself, `MessagePort`/channels, and
  `SharedArrayBuffer` promotion** are out of scope for `node:process`
  proper (they belong to `node:worker_threads`), but this module's design
  (per-region `process` singleton, one-shot env copy, main-thread-only real
  signal delivery) is written to compose cleanly with that future module
  rather than needing rework once it lands.

### 5.5 Buffer / TypedArray interop

- `node:process` itself has **no direct byte-buffer surface** — no method
  here accepts or returns a `Buffer`/`TypedArray`/`ArrayBuffer` (contrast
  with `node:fs`/`node:buffer`). The only adjacent surface is `process.send`
  accepting an arbitrary structured-clone-able `message` (which *may*
  contain a `Buffer`/`TypedArray` payload as a plain field) — that payload
  crosses the IPC boundary via the engine's existing structured-clone/
  serialization path (out of this module's own scope to reimplement) rather
  than a new byte-marshalling primitive specific to `process`.
- `process.report.getReport()`'s output object is a plain JS object tree
  (numbers/strings/nested objects), never raw bytes — no TypedArray
  involvement.

### 5.6 Doctrine placement

- **Non-primordial, confirmed.** `process` (and the `node:process` module
  form) has no native literal syntax — it is an ordinary object reached by
  ambient-global lookup or `import`/`require`, so per the "dividing line is
  native syntax" doctrine it is **not** an engine primordial. The engine
  front-end must never hardcode the identifier `"process"` or special-case
  `import ... from "node:process"`. Resolution flows entirely through
  rts-node's own data table (`NodespaceSpec` / `NODE_SPECS` / `node_lookup` /
  `ns_prefix_for`, today in `crates/rts-node/src/lib.rs`, per the existing
  scaffold's `SPEC { node_module: "process", ns_prefix: "node_process",
  members }` shape) — `"node:process"` maps to an `ns_prefix` exactly like
  every other `node:*` module; no new engine-side special case is
  introduced by this spec.
- **Ambient-global injection without hardcoding.** `process` (unlike every
  other `node:*` module) must also be available with **zero import** in
  every RTS program, not only in explicit Node-target programs. This is the
  same generic mechanism `CLAUDE.md`'s ANTI-HARDCODE §3 describes for
  whole-global-object injection ("write it as a `.ts` PRELUDE and
  `e.include` it"): rts-node ships a `process_globals.ts` prelude,
  unconditionally included (or included whenever the Node-compat surface is
  enabled — an existing project-level decision point, not new to this
  spec), binding the singleton object into global scope. The front-end's
  inclusion mechanism stays name-agnostic; no `if name == "process"` arm is
  ever written in the engine.
- **`process` as an `EventEmitter` instance** is itself an instance of a
  non-primordial backend class (`node:events`' `EventEmitter`) — this
  module must resolve that inheritance via the same generic
  class-composition mechanism other non-primordial classes use (shape +
  prototype chain), never by the engine special-casing "process is an
  EventEmitter."
- **Where the `.ts` lives:** `crates/rts-node/src/process/*.ts` (rts-node
  owns all of `node:process` — Node-specific surface, not JS/TS-universal,
  so it does **not** belong in `rts-primitives` or `rts-shared/src/stdlib/`).
  The existing `crates/rts-node/src/process/mod.rs` (a thin table borrowing
  `__RTS_FN_NS_ENV_CWD`/`__RTS_FN_NS_OS_PLATFORM`/etc. — old-style symbols
  reusing `rts-std` namespaces) is the scaffold this spec **replaces**: per
  the owner decision, the new implementation owns its **own** native
  externs (`__RTS_FN_NODE_PROCESS_*`, §5.2) and must not continue borrowing
  `rts-std`'s `env`/`os` namespace symbols.

### 5.7 Shared-infra dependencies (FLAG)

- **Next-tick queue.** `process.nextTick` needs a priority queue distinct
  from (and drained before) the ECMAScript Promise microtask queue. Today
  RTS's only queue-shaped async primitive is the Promise/event-loop
  machinery in `rts-std::promise`/`async_rt`. Since `rts-node` cannot depend
  on `rts-std`, either (a) a minimal next-tick queue is implemented
  natively and independently inside `rts-node` (simplest, some duplication
  of "a queue drained at a fixed point in the loop"), or (b) the queue
  primitive is hoisted into a shared low crate (`rts-engine` is the natural
  home, since it already owns other "run before returning to JS" concerns).
  This spec assumes (a) unless the owner decides to hoist.
- **Promise reject/settle bookkeeping** for `'unhandledRejection'`/
  `'rejectionHandled'` — real Node ties these events directly into the
  engine's Promise implementation's internal reject-tracking. RTS's Promise
  create/settle machinery is documented (`00-meta.md`/`03-features.md`) as
  currently living in `rts-std::promise` + `async_rt`. Since `rts-node`
  cannot depend on `rts-std`, firing these two events correctly requires
  either a hook exposed from wherever Promise settlement ultimately lives
  once hoisted (the `Promise` **class** itself is primordial/
  `rts-primitives`-owned; only the concrete settle-tracking machinery is
  the open question — same flag already raised in the `node:buffer` spec
  for `Blob`'s async methods) — **this module cannot ship
  `'unhandledRejection'`/`'rejectionHandled'` correctly until that hook
  exists**, independent of `rts-std`.
- **IPC channel (`process.send`/`disconnect`/`channel`/`'message'`/
  `'disconnect'` event)** needs a duplex pipe to a parent process — the same
  transport `node:child_process`'s `fork()` needs on the child side. This is
  new shared infra neither `rts-std` nor `rts-node` currently has in a
  cross-module-reachable form; must be designed once alongside
  `node:child_process` (a sibling rts-node module), not solved purely
  within `node:process`.
- **tokio / shared async runtime:** needed only for `process.send`'s
  genuine I/O (the IPC pipe) — everything else in this module is
  synchronous OS-primitive access needing no async runtime at all. Since
  `rts-node` is independent of `rts-std` (which currently owns the shared
  tokio `OnceLock<Runtime>` in `async_rt.rs`), `rts-node` needs **its own**
  minimal async I/O story for the IPC pipe specifically — either its own
  lightweight `OnceLock<tokio::runtime::Runtime>` (accepted duplication per
  the owner's independence decision) or a hoisted shared runtime handle.
  Flagged as needed, scoped narrowly to IPC.
- **Signal-to-JS bridge (self-pipe or equivalent)** — needs a way to safely
  marshal an OS asynchronous-signal-context interrupt into a normal
  JS-callable event dispatch on the "main" RTS thread. Not currently
  provided by any crate; must be built as new rts-node-owned infra (a
  classic self-pipe-trick or an atomic flag polled by the event loop's own
  tick — whichever the RTS event loop's existing poll structure makes
  cheapest, another reason this benefits from the event loop itself living
  somewhere `rts-node` can reach — see next bullet).
- **RTS event loop tick hook** — `process.nextTick` draining,
  `'beforeExit'` detection ("event loop has no further work"), and the
  signal self-pipe poll all need a hook into the RTS event loop's own tick/
  phase structure. The event loop is documented as living in `rts-std`
  today. `rts-node` cannot depend on `rts-std`, so `node:process` (and
  every other rts-node module with lifecycle/async needs) requires the
  event loop's core tick-scheduling primitive to be reachable from a crate
  `rts-node` **can** depend on — most likely hoisted into `rts-engine`
  alongside the GC/ABI contract it already owns, or a new minimal
  `rts-async-core` crate beneath both `rts-std` and `rts-node`. This is the
  single biggest structural dependency this module surfaces; it blocks
  `nextTick`, `'beforeExit'`, `'unhandledRejection'`/`'rejectionHandled'`,
  and signal delivery all at once, and should be resolved once, centrally,
  rather than per-module.
- **GC heap-accounting accessor** for `memoryUsage()`'s `heapUsed`/
  `heapTotal`/`arrayBuffers` fields — needs a small public read accessor on
  `rts-engine::gc` reporting live-allocation/heap-capacity numbers. Likely
  already partially available (the GC already tracks handle-table
  occupancy for its own mark/sweep cycles); flagged only in case no public
  accessor exists yet.
- **`rts-napi` addon loader** for `process.dlopen` — already exists
  (`__RTS_FN_NS_NAPI_LOAD_ADDON` in the `rts-napi` crate) and is
  cross-module-reachable without a new dependency; listed here only as an
  explicit cross-reference, not a genuine gap.
- **`node:child_process` fork/IPC transport** — see the IPC bullet above;
  a sibling rts-node module dependency, not a `rts-std` one.
- **TLS/rustls, net sockets, crypto (SHA/CSPRNG):** **none** — not used by
  `node:process`.

### 5.8 Implementation phases

1. **(a)** Land the purely synchronous, no-cross-module-dependency core:
   `argv`/`argv0`/`execPath`/`execArgv`/`platform`/`arch`/`pid`/`ppid`/
   `version`/`versions`/`release`/`cwd`/`chdir`/`title`/`debugPort`/
   `exitCode`/`exit`/`abort`/`uptime`/`hrtime`(+`.bigint`) — proves the
   basic ABI + `.ts` singleton-object shape end to end with zero shared-infra
   blockers.
2. **(b)** Add `env` (get/set/delete/keys + Windows case-fold + the
   deprecated-coercion warning) as its own `.ts` `Proxy`-backed wrapper over
   the native externs.
3. **(c)** Add `memoryUsage`/`cpuUsage`/`resourceUsage`/`threadCpuUsage`/
   `availableMemory`/`constrainedMemory` (needs the GC heap-accounting
   accessor from §5.7 for a subset of `memoryUsage` fields; everything else
   is direct OS reads).
4. **(d)** Add `umask`, the POSIX uid/gid family, `execve` — all
   platform-gated (compiled out / unsupported-operation error on Windows
   where documented).
5. **(e)** Add `kill` + the signal name/number table + basic
   `process.on(signalName, ...)` registration, **without** yet building the
   full self-pipe bridge — land it as a minimal "deliver on next event-loop
   tick" stub if the tick-hook infra (§5.7) is not ready yet, documented as
   an interim gap.
6. **(f)** Implement the ambient-global `process_globals.ts` prelude +
   `EventEmitter` base wiring (depends on `node:events`'s core listener
   registry landing first or alongside).
7. **(g)** Implement `nextTick` once the shared next-tick-queue decision
   (§5.7) is made (native rts-node-owned queue vs hoisted primitive).
8. **(h)** Implement `'exit'`/`'beforeExit'` dispatch, wired to the engine's
   real program-termination and empty-event-loop detection points.
9. **(i)** Implement `'uncaughtException'`/`'uncaughtExceptionMonitor'`/
   `setUncaughtExceptionCaptureCallback`/`addUncaughtExceptionCaptureCallback`/
   `hasUncaughtExceptionCaptureCallback`, wired to the engine's top-level
   exception-dispatch point.
10. **(j)** Implement `'unhandledRejection'`/`'rejectionHandled'` — blocked
    on the Promise settle-tracking hook (§5.7) becoming reachable from
    `rts-node`.
11. **(k)** Implement `emitWarning`/`'warning'` +
    `noDeprecation`/`throwDeprecation`/`traceDeprecation`/
    `traceProcessWarnings`.
12. **(l)** Implement `report.*` (assembles from (c)'s accessors + a
    Rust-native backtrace) and `getActiveResourcesInfo` (needs the
    HandleTable enumeration accessor, §5.7 note).
13. **(m)** Implement `send`/`disconnect`/`channel`/`'message'`/
    `'disconnect'` — blocked on the IPC transport (§5.7), best scheduled
    alongside `node:child_process`'s `fork()` work.
14. **(n)** Implement `permission.has` (Experimental — thin stub reporting
    "no permission model enforced" is an acceptable interim if RTS has not
    built a permission-model equivalent yet; document as a gap).
15. **(o)** Implement `finalization.*` (needs the engine's `WeakRef`
    GC-coupling, already primordial).
16. **(p)** Implement `dlopen` (thin forward to `rts-napi`),
    `getBuiltinModule`, `loadEnvFile`, `setSourceMapsEnabled`,
    `allowedNodeEnvironmentFlags`, `mainModule` (deprecated stub),
    `features.*` (static table) — the remaining long tail, each independent
    and low-risk once the core singleton/event wiring exists.

## 6. Test plan

`tests/node/process/*.test.ts` (`rts:test` format):

- **Ambient global, no import:** a `.test.ts` file that reads `process.pid`/
  `process.argv` with **zero** import statement, proving the global-injection
  prelude works; a second file using `import process from "node:process"`
  and a third using `import { argv, env } from "node:process"` all observe
  the same live singleton (mutate `process.title` via one form, read via
  another).
- **`argv`/`argv0`/`execPath`/`execArgv`:** shape/type checks
  (`Array.isArray(process.argv)`, `argv[0] === execPath`-style relationship
  per Node's documented layout).
- **`platform`/`arch`:** value is one of the documented enum members;
  cross-check against `os.platform()`/`os.arch()` agreement (once
  `node:os` exists) for consistency.
- **`cwd`/`chdir`:** `chdir(dir)` then `cwd() === dir` (resolved/absolute
  form); `chdir('/nonexistent/path')` throws with `ENOENT`; `chdir` inside a
  simulated Worker context throws (once `worker_threads` exists) or is
  documented as deferred.
- **`env`:** read an existing var; `process.env.FOO = 'bar'` then re-read;
  `delete process.env.FOO` then `process.env.FOO === undefined`; Windows-only
  case-insensitivity test (`process.env.PATH` vs `process.env.Path`, gated
  by `process.platform === 'win32'`); assigning a number and asserting the
  deprecated-coercion warning fires (or is suppressed by `noDeprecation`).
- **`exitCode`/`exit`:** set `process.exitCode = 1` and let the process end
  naturally vs `process.exit(2)` immediately — assert (via a child-process
  harness) the OS-level exit code observed matches in both cases; `exit('7')`
  (string form) accepted; `exit('abc')` throws.
- **`'exit'` listener constraints:** register an `'exit'` listener that
  tries to schedule a `setTimeout`/`Promise.then` and assert it never runs
  (matches Node's synchronous-only guarantee).
- **`'beforeExit'` vs `'exit'` ordering:** a listener on `'beforeExit'` that
  schedules a `setTimeout(0)` keeps the process alive one more tick, and
  `'exit'` still fires exactly once after that extra tick resolves.
- **`hrtime`/`hrtime.bigint`:** monotonic non-decreasing across two calls;
  the diff-form `hrtime(prev)` returns a sane, small, non-negative delta for
  a short sleep; `hrtime.bigint()` value converts consistently with
  `hrtime()`'s two-part tuple (same order of magnitude).
- **`memoryUsage`/`cpuUsage`/`resourceUsage`:** all numeric fields present
  and non-negative; `cpuUsage(prevSnapshot)` delta is non-negative after
  doing CPU-bound work between snapshots; `memoryUsage.rss()` agrees in
  order of magnitude with `memoryUsage().rss`.
- **`umask` (POSIX only, gated by `process.platform !== 'win32'`):**
  `umask(0o022)` returns the previous mask; a subsequent `umask()`
  (no-arg getter) reports `0o022` back.
- **`kill`:** `process.kill(process.pid, 0)` returns `true` (existence
  check, no actual signal effect); `process.kill(999999999, 'SIGTERM')`
  throws `ESRCH`; register `process.on('SIGUSR2', ...)` (POSIX only) and
  self-signal via `process.kill(process.pid, 'SIGUSR2')`, assert the
  listener fires.
- **Windows-specific signal behavior (gated by `win32`):**
  `process.kill(pid, 'SIGINT')` unconditionally terminates a spawned child
  regardless of listeners; `'SIGBREAK'` listener registration does not
  throw.
- **`uncaughtException`:** spawn a child RTS process (via a test harness)
  that throws synchronously with no handler, assert default stderr-print +
  non-zero exit; a second child registers a handler that calls
  `process.exit(0)`, assert clean exit.
- **`unhandledRejection`/`rejectionHandled`:** a Promise that rejects with
  no `.catch` fires `'unhandledRejection'` with the correct `reason`/
  `promise`; attaching a late `.catch()` afterward fires
  `'rejectionHandled'` with the same promise reference.
- **`emitWarning`:** both call forms (options object and legacy
  type/code/ctor) produce an equivalent `'warning'` event payload
  (`name`/`message`/`code`); `noDeprecation = true` suppresses a
  `'DeprecationWarning'`-typed `emitWarning` call; `throwDeprecation = true`
  makes the same call throw instead of warn.
- **`report.getReport`/`writeReport`:** `getReport()` returns an object with
  the documented top-level keys; `writeReport()` produces a file whose
  contents `JSON.parse` successfully; `report.excludeEnv = true` omits
  `environmentVariables` (or produces an empty map) from the output.
- **`allowedNodeEnvironmentFlags`:** `.has('--max-old-space-size')` and
  `.has('max-old-space-size=100')` both resolve the same normalized way (or
  both return a documented default if RTS does not support the flag,
  consistently).
- **`getActiveResourcesInfo`:** opening a resource (e.g. a timer or a TCP
  listener once available) increases the returned array's length; closing
  it decreases it again.
- **`finalization.register`/`registerBeforeExit`/`unregister`:** register a
  callback on an object, drop all references, force a GC cycle, assert the
  callback fires on `'exit'` vs `'beforeExit'` per which registration form
  was used; `unregister` before the object is collected prevents the
  callback from firing.
- **`dlopen`/`getBuiltinModule`:** `getBuiltinModule('node:process') !==
  undefined`; `getBuiltinModule('not-a-real-module')` returns `undefined`
  and does not throw.
- **`loadEnvFile`:** load a temp `.env` file with `KEY=value` lines, assert
  `process.env.KEY === 'value'` afterward; a missing file path throws.
- **Multithread (`worker_threads`, once that module lands):** a Worker's
  `process.env` mutation does not affect the main thread's `process.env`
  (one-shot copy proof); calling `process.chdir()`/`process.umask()` inside
  a Worker throws (matches Node's documented restriction); the main
  thread's real `SIGINT` delivery does not also fire inside a Worker's
  `process` (each thread's `process` singleton is independent); `pid`
  inside a Worker equals the same OS pid as the main thread (single OS
  process, multiple RTS-engine threads).

## 7. Open questions / deferrals

- **Event-loop / next-tick / Promise-settle hoisting decision (§5.7).**
  This is the single largest open structural question this module surfaces:
  where does the shared "tick scheduler" primitive that `nextTick`,
  `'beforeExit'`, `'unhandledRejection'`/`'rejectionHandled'`, and the
  signal self-pipe all need actually live, given `rts-node` cannot depend on
  `rts-std`? Needs an owner decision (hoist into `rts-engine`, a new
  `rts-async-core` crate, or accept independent duplication in `rts-node`)
  before phases (g)/(j)/(e)'s full form can be implemented.
- **Exact `process.version`/`versions`/`release` content for RTS.** Should
  RTS report a Node-compatible `version` string for maximum ecosystem
  compatibility (some npm packages `semver`-gate on `process.version`), or
  its own distinct product version? Which dependency keys belong in
  `versions` (Cranelift version? tokio? rustls?) needs an owner call — see
  the `ProcessVersions` interface note in §3.
- **`process.config`/`process.release` exact contents** for an RTS build —
  what, if anything, is meaningful to report here given RTS has no
  `./configure`-equivalent build system today.
- **IPC transport design (`send`/`disconnect`/`channel`, `node:child_process`
  `fork()`)** is fully deferred to whenever `node:child_process` is
  implemented; this spec only documents the `process`-side surface shape.
- **Permission model (`process.permission`)** — RTS has no existing
  permission-model equivalent to Node's `--permission` flag machinery;
  ship as an always-`true`/no-op stub or defer entirely, pending an owner
  decision on whether RTS will ever implement process-level permission
  gating.
- **`'multipleResolves'` event current status in Node 25** — the fetched
  docs did not clearly confirm whether this event still exists or was fully
  removed; marked `(verify)` in §2's Events table. Confirm against the raw
  `doc/api/process.md` changelog before implementing.
- **Exact error codes** for several throwing paths (`umask` on Windows,
  `cpuUsage` with a malformed previous-value argument, `ERR_CONSOLE_...`-
  style codes for invalid `report.*` inputs) are marked `(verify)` inline in
  §2/§4 and need a differential-test pass against real Node before this
  spec is considered final.
- **`process.title` truncation length on each platform** is implementation-
  defined even in real Node (varies with `argv`/`environ` memory layout);
  RTS's own limit will necessarily differ and should be measured/documented
  once implemented, not guessed in advance.
- **Whether Workers receive any signal delivery at all** — this spec
  assumes "no, matching Node," but should be reconfirmed once
  `node:worker_threads` is actually specced/implemented, in case RTS's own
  threading model make a different choice preferable.
- **`getActiveResourcesInfo`'s exact resource-kind string vocabulary** — RTS
  will necessarily invent its own strings (its `HandleTable` `Entry` variants
  don't map 1:1 to libuv's handle-kind names); needs a small dedicated
  naming pass, not a literal copy of Node's strings.
