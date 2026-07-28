# node:console

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:console` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P0 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import console from "node:console"` (default export = the module's own pre-built `Console` instance, distinct object identity from the ambient global `console`); `import { Console } from "node:console"`; `const { Console } = require("node:console")`; the bare identifier `console` is also an **ambient global** available in every module with no import at all |
| Globals exposed | `console` (global `Console` instance backed by `process.stdout`/`process.stderr`) |

## 1. Purpose

`node:console` provides the `Console` class used to build custom loggers bound
to arbitrary writable streams (e.g. a log file, an in-memory buffer, a socket),
plus the well-known global `console` object every JS/TS program already uses
for `console.log`/`console.error`/etc. In RTS this module is the **native,
stream-configurable** sibling of the ambient `console` global that every RTS
program already gets via the `.ts` prelude — this spec covers making `Console`
a real, constructible, stream-parametric class (today the ambient global is a
fixed stdout/stderr singleton) and formalizing its full method surface
(`table`, `group*`, `time*`, `count*`, `assert`, `dir*`) with real behavior
instead of best-effort stubs.

## 2. Exported API surface (COMPLETE)

### Classes

#### `class Console`

No base class (does **not** extend `EventEmitter`; no events — see
"Events" below).

**Constructors (2 overloads):**

```ts
new Console(stdout: Writable, stderr?: Writable, ignoreErrors?: boolean)
new Console(options: ConsoleConstructorOptions)
```

| Param | Type | Optional | Default | Notes |
|---|---|---|---|---|
| `stdout` | `stream.Writable` | no (form 1) | — | sink for `log`/`info`/`debug`/`dir`/`table`/`group*`/`time*`/`count*` |
| `stderr` | `stream.Writable` | yes | `stdout` | sink for `error`/`warn`/`trace`/`assert` |
| `ignoreErrors` | `boolean` | yes | `true` | swallow write errors to the underlying stream instead of throwing |
| `options` | `ConsoleConstructorOptions` | no (form 2) | — | see §3 |

Throws: `TypeError` (`ERR_CONSOLE_WRITABLE_STREAM`, verify exact code) if
`stdout` (or `stderr` when given) is not a writable stream.

**Instance methods** (all variant: **sync call**, output write may itself be
sync or async per stream — see §4):

| Method | Signature | Sink |
|---|---|---|
| `log` | `log(...args: any[]): void` | stdout |
| `info` | `info(...args: any[]): void` (alias of `log`) | stdout |
| `debug` | `debug(...args: any[]): void` (alias of `log`) | stdout |
| `dir` | `dir(obj: any, options?: ConsoleDirOptions): void` | stdout |
| `dirxml` | `dirxml(...data: any[]): void` (delegates to `log`) | stdout |
| `table` | `table(tabularData: any, properties?: string[]): void` | stdout |
| `group` | `group(...label: any[]): void` | stdout |
| `groupCollapsed` | `groupCollapsed(...label: any[]): void` (alias of `group`) | stdout |
| `groupEnd` | `groupEnd(): void` | — (no output; pops indent) |
| `count` | `count(label?: string): void` | stdout |
| `countReset` | `countReset(label?: string): void` | — (no output) |
| `time` | `time(label?: string): void` | — (no output; starts timer) |
| `timeEnd` | `timeEnd(label?: string): void` | stdout |
| `timeLog` | `timeLog(label?: string, ...data: any[]): void` | stdout |
| `error` | `error(...args: any[]): void` | stderr |
| `warn` | `warn(...args: any[]): void` (alias of `error`) | stderr |
| `trace` | `trace(...args: any[]): void` | stderr |
| `assert` | `assert(value: any, ...message: any[]): void` | stderr (only when `value` is falsy) |
| `clear` | `clear(): void` | stdout (TTY-only; no-op otherwise) |
| `profile` | `profile(label?: string): void` | inspector only, no stdout/stderr output |
| `profileEnd` | `profileEnd(label?: string): void` | inspector only |
| `timeStamp` | `timeStamp(label?: string): void` | inspector only |

Per-method detail:

- **`log(...args)` / `info(...args)` / `debug(...args)`** — `util.format()`-style:
  if `args[0]` is a string containing format specifiers (`%s %d %i %f %j %o %O
  %c %%`), it is used as the format template against `args[1..]`; otherwise
  every arg is `util.inspect()`-ed independently and joined with a single
  space, one line per call, terminated by `\n`.
- **`error(...args)` / `warn(...args)`** — identical formatting to `log`, sink
  is `stderr`. `warn` is a strict alias of `error` (same function object in
  Node).
- **`assert(value, ...message)`** — if `value` is truthy, no-op. If falsy
  (including omitted), writes `"Assertion failed"` optionally followed by
  `: ` + `util.format(...message)` to stderr. **Never throws** — this is not
  Node's internal `assert` module.
- **`dir(obj, options?)`** — `util.inspect(obj, options)` written as one line.
  Bypasses any custom `[util.inspect.custom]` on `obj` only when
  `options.customInspect === false`; unlike `log`, `dir` accepts
  inspect-specific options (`depth`, `colors`, `showHidden`) directly.
- **`dirxml(...data)`** — no XML rendering exists in Node's non-browser
  implementation; behaves as `log(...data)`.
- **`table(tabularData, properties?)`** — if `tabularData` is not an
  array/object (or is empty), falls back to `log(tabularData)`. Otherwise
  builds box-drawing-character grid: `(index)` column + one column per
  observed property key (or per entry of `properties` if given, in that
  order) + a `Values` column for non-object array elements. Nested
  object/array cells are rendered via `util.inspect()` (truncated).
- **`group(...label)`** — if `label` given, first calls `log(...label)` at the
  *current* indent, then increases the indent used by every subsequent call
  (`log/info/debug/warn/error/dir/table/trace/assert/group/time*/count*`) by
  `groupIndentation` spaces (default 2). Nestable; each call pushes one level.
- **`groupCollapsed(...label)`** — alias of `group`; Node's terminal output is
  identical to `group` (the collapsed/expanded distinction only exists in
  browser DevTools).
- **`groupEnd()`** — pops one indent level (floor 0; extra calls are
  no-ops). No output of its own.
- **`count(label = "default")`** — per-instance `Map<string, number>` keyed by
  `label`; increments (creating at 1 if absent) and writes `"<label>: <n>"`.
- **`countReset(label = "default")`** — resets the counter for `label` to 0
  (no output); a subsequent `count(label)` prints `"<label>: 1"`.
- **`time(label = "default")`** — records `label -> high-res start time` in a
  per-instance map. Calling `time()` again with a label already running emits
  a **process warning** (`Label '<label>' already exists for console.time()`)
  and resets the start.
- **`timeEnd(label = "default")`** — computes elapsed since `time(label)`,
  deletes the entry, writes `"<label>: <elapsed><unit>"` (unit auto-scaled:
  `ms` under 1000 ms, else formatted seconds/etc., 13.0.0+ behavior). Warns
  (`Label '<label>' does not exist for console.timeEnd()`) and no-ops if
  `label` was never started.
- **`timeLog(label = "default", ...data)`** — like `timeEnd` but does **not**
  delete the timer (it keeps running) and appends `util.format(...data)` after
  the elapsed-time string. Warns/no-ops if not started.
- **`trace(...args)`** — writes `"Trace: " + util.format(...args)` to stderr
  followed by the current JS call stack (same rendering as an `Error` stack,
  minus the "Error" header), each frame indented `"    at ..."`.
- **`clear()`** — only has effect when the bound `stdout` is a TTY
  (`isTTY === true`): POSIX clears scrollback similar to shell `clear`;
  Windows clears only the current viewport. No-op when `stdout` is a file,
  pipe, or socket.
- **`profile`/`profileEnd`/`timeStamp`** — no-ops unless the process is
  running under `--inspect` and a debugger/profiler client is attached; RTS
  has no inspector protocol, so these are **permanent intentional no-ops**
  (documented, not a bug) — see §7.

### Top-level functions

None. `node:console`'s module namespace exposes only the `Console` class and
the pre-built global-equivalent instance (see below) — there are no free
functions.

### Properties & constants

| Name | Type | Notes |
|---|---|---|
| `console.Console` | `typeof Console` | every `Console` instance (including the ambient global `console`) carries a `.Console` property pointing back at the class, so `require("node:console").Console === Console` and `global.console.Console === Console` |
| module default export | `Console` instance | `node:console`'s own module-level export is a **freshly-constructed** `Console` bound to `process.stdout`/`process.stderr` — semantically equivalent to, but **not the same object identity as**, the ambient global `console` (Node constructs each independently; both wrap the same two streams) |

No other constants (no `Symbol.for('nodejs.util.inspect.custom')` re-export
here — that symbol lives on `util`, not `console`).

### Events

`Console` does not extend `EventEmitter` and emits no events.

## 3. Types & option objects

```ts
/** Constructor options (single-object overload). */
interface ConsoleConstructorOptions {
  stdout: WritableLike;
  stderr?: WritableLike;
  /** Swallow errors writing to the underlying stream. Default: true. */
  ignoreErrors?: boolean;
  /** 'auto' resolves from stdout.isTTY + color depth. Default: 'auto'. */
  colorMode?: boolean | "auto";
  /** Forwarded to util.inspect() for every non-string logged value.
   *  May be a single options object (both streams) or a Map<Writable,
   *  InspectOptionsLike> to set per-stream options (colorMode and per-stream
   *  Map both landed to let stdout/stderr diverge, e.g. color on a TTY
   *  stdout but not a piped stderr). */
  inspectOptions?: InspectOptionsLike | Map<WritableLike, InspectOptionsLike>;
  /** Spaces per console.group() nesting level. Default: 2. */
  groupIndentation?: number;
}

/** Minimal writable-stream shape Console needs (RTS has no generic
 *  stream.Writable yet — see §5.1/§5.7). */
interface WritableLike {
  write(chunk: string | Uint8Array): boolean;
  readonly isTTY?: boolean;
}

/** Subset of util.inspect options Console cares about. */
interface InspectOptionsLike {
  showHidden?: boolean;
  depth?: number | null;
  colors?: boolean;
  customInspect?: boolean;
  [key: string]: unknown;
}

/** console.dir() second argument. */
interface ConsoleDirOptions {
  showHidden?: boolean; // default false
  depth?: number | null; // default 2, null = infinite
  colors?: boolean; // default false
  customInspect?: boolean; // default true (false bypasses [util.inspect.custom])
}

/** Format-specifier reference consumed by log/info/debug/warn/error/trace/assert
 *  when args[0] is a string. Anything not matched by a specifier falls back to
 *  util.inspect() on the corresponding remaining arg; extra args beyond the
 *  specifier count are appended space-separated, inspected individually. */
type FormatSpecifier =
  | "%s"  // String(arg) (or util.inspect for non-primitives, one level)
  | "%d"  // Number(arg) truncated to integer; NaN if not coercible
  | "%i"  // parseInt(arg, 10); NaN if not coercible
  | "%f"  // parseFloat(arg)
  | "%j"  // JSON.stringify(arg); "[Circular]" text on cycles instead of throwing
  | "%o"  // util.inspect(arg, { showHidden: true, showProxy: true })
  | "%O"  // util.inspect(arg) (no showHidden)
  | "%c"  // CSS styling directive — consumes the arg, emits NO text (Node/RTS
          //   ignore terminal styling; browser-only feature)
  | "%%"; // literal "%", consumes no arg
```

## 4. Node semantics & edge cases

- **Global vs module instance identity.** `console === require("node:console")`
  is **false** in real Node — the module's default export and the global are
  two independently-constructed `Console` instances (both wrapping the same
  `process.stdout`/`process.stderr`). RTS must preserve this: importing
  `node:console` must not alias the ambient global object.
- **stdout/stderr write timing is not uniformly sync or async.** Per Node's
  "A note on process I/O" (referenced directly from the console docs): files
  are synchronous on both POSIX and Windows; TTYs are synchronous on POSIX but
  asynchronous on Windows; pipes/sockets are asynchronous on POSIX but
  synchronous on Windows (verify exact wording against the live doc — table
  content could not be fetched verbatim in this pass, but this OS-pairing is
  the long-standing, stable Node behavior). Practical effect: a process that
  exits immediately after a large `console.log` to a piped stdout can drop
  output on POSIX (backpressure not awaited) — Node explicitly does not
  guarantee delivery in that case, and RTS must not pretend otherwise.
- **`%d`/`%i`/`%f` on non-numeric args** produce `NaN` text, not a thrown
  error; `%s` on an object calls `String()` (not deep inspect) for primitives
  but a shallow `util.inspect` for the object case per Node's real behavior.
- **`%j` never throws** on a circular structure; it substitutes
  `'[Circular]'` (via a replacer), unlike bare `JSON.stringify`.
  Extra/missing arguments: extra args beyond the number of specifiers are
  appended, space-joined, each independently inspected; missing args for a
  specifier leave the literal specifier text (`%s`) in the output.
  - **`%c` always consumes one argument and emits nothing** — a common bug
    source when porting browser-authored template strings; RTS must consume
    but not print it (not silently ignore the whole call).
- **`console.table` truncation** — deeply nested cell values are rendered
  through `util.inspect` and can be truncated/ellipsized exactly like normal
  inspect output; column order follows first-seen key order across rows,
  not alphabetical.
- **`time()`/`timeEnd()`/`timeLog()` label collisions emit `process.emitWarning`**,
  not an exception — calling `timeEnd("x")` twice in a row is a no-op with a
  warning on the second call, never a crash.
- **`console.clear()` is a pure no-op on non-TTY streams** — piping to a file
  or another process must never emit clear-screen escape codes.
- **`assert` never throws**, unlike the `node:assert` module of a similar
  name — this is a frequent porting mistake to guard against in tests.
- **Deprecations/removals:** none currently deprecated in the documented
  surface as of Node 25; `profile`/`profileEnd`/`timeStamp` remain
  inspector-only no-ops outside `--inspect` in real Node too, so RTS matching
  that with a permanent no-op is spec-compliant, not a shortcut.
- **No documented Worker-thread-specific behavior** — each Node worker gets
  its own `process.stdout`/`stderr` proxies that pipe to the parent; RTS's
  mapping is in §5.4.
- **Windows vs POSIX visual differences**: `console.table`'s box-drawing
  characters and `clear()`'s escape sequences are the same source text on
  both platforms; only the underlying terminal's rendering/capability differs
  (color depth detection via `getColorDepth()`/`isTTY`, not console's own
  logic).

## 5. RTS implementation notes

### 5.1 Native impl mapping

`rts-node` owns this module fully; no dependency on `rts-std`'s existing
`globals/console` primitives (those stay as the *ambient* JS-global's backing
today — see the migration note in §5.6). Native surface needed, all
implemented directly in `rts-node::console`:

- **Formatting** (`util.format`-equivalent: `%s %d %i %f %j %o %O %c %%`
  parsing) — pure Rust string logic, no external crate; reuses the same
  routine `node:util`'s `format()` needs (shared internal fn inside
  `rts-node`, not a cross-crate dependency).
- **Inspection** (`util.inspect`-equivalent rendering of arrays/objects/
  classes/circular refs) — pure Rust, walks the engine's `PolyValue`/shape
  representation via the same `engine.display`-style bridge the current `.ts`
  ambient console already leans on (see §5.2); this is the single largest
  chunk of native logic in the module.
- **Table rendering** (`console.table`) — pure Rust grid layout + Unicode
  box-drawing characters, no crate needed.
- **Timers** (`console.time`/`timeEnd`/`timeLog`) — `std::time::Instant`,
  per-`Console`-instance `HashMap<String, Instant>` guarded the same way
  other rts-node per-module state is guarded (`Arc<Mutex<T>>` /
  `thread_local!`, per `02-runtime.md` "State" convention — no dependency on
  any shared state system).
- **Counters** (`console.count`/`countReset`) — per-instance
  `HashMap<String, u64>`, same guarding pattern.
- **TTY / color-depth detection** (`colorMode: 'auto'`, `clear()`) — Rust
  `std::io::IsTerminal` (stable since Rust 1.70) for `isTTY`; a small
  ANSI-capability probe (env `TERM`/`COLORTERM`/Windows console mode via
  `winapi`/`windows-sys` `GetConsoleMode`) for color depth — **not** shared
  with `rts-std`; `rts-node` vendors its own minimal probe (a few dozen
  lines), matching the "fully independent crate" decision.
- **Stream write** (`stdout.write`/`stderr.write` on the bound streams) —
  `std::io::{Stdout, Stderr, Write}` for the process-level defaults; a custom
  `Console(options)` binds to *any* writable, which in RTS terms means a
  `node:fs` file handle, an in-memory buffer, or a `node:net` socket handle
  (each already an opaque native handle owned by its respective rts-node
  module) — `console`'s native layer only needs a `dyn Write`-shaped call,
  not stream internals.

### 5.2 ABI surface

Proposed symbols (`__RTS_FN_NODE_CONSOLE_<NAME>`), all `extern "C"`:

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_CONSOLE_NEW` | `Handle(stdout_sink), Handle(stderr_sink), Bool(ignore_errors), I32(group_indentation), I32(color_mode)` | `Handle` | allocates a `ConsoleState` in the rts-node handle table; `color_mode`: 0=false/1=true/2=auto |
| `__RTS_FN_NODE_CONSOLE_FORMAT` | `Handle(console), StrPtr(fmt_or_first_arg_json), Handle(args_array)` | `Handle` (formatted string) | runs the `%s/%d/%i/%f/%j/%o/%O/%c/%%` + inspect pipeline; returns a GC/handle string ready to hand to the string primitive |
| `__RTS_FN_NODE_CONSOLE_WRITE_STDOUT` | `Handle(console), StrPtr(line)` | `Void` | applies current indent, writes `line + "\n"` to the bound stdout sink; honors `ignoreErrors` |
| `__RTS_FN_NODE_CONSOLE_WRITE_STDERR` | `Handle(console), StrPtr(line)` | `Void` | same, stderr sink |
| `__RTS_FN_NODE_CONSOLE_INSPECT` | `Handle(console), Handle(value), I32(depth), Bool(show_hidden), Bool(colors)` | `Handle` (string) | `util.inspect`-equivalent single-value render, used by `dir`/`%o`/`%O` |
| `__RTS_FN_NODE_CONSOLE_TABLE` | `Handle(console), Handle(value), Handle(properties_array_or_null)` | `Handle` (string, full multi-line table text) | builds the grid; caller still routes the result through `WRITE_STDOUT` |
| `__RTS_FN_NODE_CONSOLE_GROUP_PUSH` | `Handle(console)` | `Void` | increments indent by `groupIndentation` |
| `__RTS_FN_NODE_CONSOLE_GROUP_POP` | `Handle(console)` | `Void` | decrements indent, floored at 0 |
| `__RTS_FN_NODE_CONSOLE_COUNT` | `Handle(console), StrPtr(label)` | `U64` (new count) | increments+returns; caller formats `"<label>: <n>"` |
| `__RTS_FN_NODE_CONSOLE_COUNT_RESET` | `Handle(console), StrPtr(label)` | `Void` | |
| `__RTS_FN_NODE_CONSOLE_TIME_START` | `Handle(console), StrPtr(label)` | `Bool` (false if already running → caller emits the "already exists" warning) | |
| `__RTS_FN_NODE_CONSOLE_TIME_END` | `Handle(console), StrPtr(label)` | `F64` (elapsed ms, or `-1.0` if not started → caller emits the "does not exist" warning) | consumes (removes) the timer |
| `__RTS_FN_NODE_CONSOLE_TIME_LOG` | `Handle(console), StrPtr(label)` | `F64` (elapsed ms, or `-1.0`) | does **not** remove the timer |
| `__RTS_FN_NODE_CONSOLE_IS_TTY` | `Handle(console), Bool(is_stderr)` | `Bool` | backs `clear()`'s no-op-unless-TTY gate and `colorMode: 'auto'` resolution |
| `__RTS_FN_NODE_CONSOLE_CLEAR` | `Handle(console)` | `Void` | emits the platform clear-sequence only if `IS_TTY` |
| `__RTS_FN_NODE_CONSOLE_TRACE_STACK` | — | `Handle` (string) | captures the current JS call stack via the engine's existing `trace/` frame-stack facility (native, engine-owned — `node:console` calls it, does not reimplement it) |
| `__RTS_FN_NODE_CONSOLE_FREE` | `Handle(console)` | `Void` | drops a custom (non-global) `Console` instance's native state |

Objects becoming opaque `Handle`s: the `Console` native state (`ConsoleState`:
bound stdout/stderr sink refs, timers map, counters map, indent level,
inspect/color options) — one handle per `Console` instance, including the
ambient global's own handle created once at process start. `profile`/
`profileEnd`/`timeStamp` get **no ABI symbols** — they compile to a `.ts`-level
no-op directly (no native call at all; nothing to inline-cache or dispatch).

Native-extern vs `.ts`-shim split:
- **Native externs**: formatting/inspection/table-layout/timer-and-counter
  bookkeeping/TTY probing/stack capture/raw stream writes (everything above).
- **`.ts` shim** (ships in `rts-node`'s TS surface, mirrors the
  `.ts`-over-primitives convention already used for the ambient console):
  the `Console` class shape itself (constructor overload resolution, default
  parameter handling, method names/arity/aliasing `info→log`, `debug→log`,
  `warn→error`, `groupCollapsed→group`), argument marshalling into the
  `args_array` handle the `FORMAT`/`TABLE` externs expect, and the
  `console.Console` back-reference property.

### 5.3 Async model

Every `Console` method is **synchronous from the caller's point of view** —
Node's console API has no callback or Promise-returning form. The only
async-shaped concern is the *underlying stream write* (see §4): when the bound
sink is a `node:net` socket or a non-blocking pipe, the write may not be
flushed by the time `console.log()` returns. RTS's plan:
- **Default global console** (`process.stdout`/`stderr`) — direct
  `std::io::{Stdout,Stderr}::write_all`, blocking, no tokio involvement; this
  matches the common/fast path and needs nothing from the event loop.
- **Custom `Console` bound to a `node:fs` file / `node:net` socket handle** —
  delegates to that handle's own existing native write primitive (already
  implemented by `rts-node::fs` / a future `rts-node::net`); `console` itself
  adds no new async machinery, it is a pure client of whatever the target
  module already exposes. If the target module's write is
  async-under-the-hood (e.g. a socket using the shared tokio runtime), that is
  the target module's concern per §5.7, not something `console` needs to await
  — matching Node's own fire-and-forget semantics (`ignoreErrors` swallows the
  failure, it never awaits completion).
- **No Promise subsystem involvement.** No method here returns a `Promise`
  in Node, so none needs the RTS promise-settle path.

### 5.4 Multithread / worker interaction

Per module instance (i.e. per `ConsoleState` handle) state — timers map,
counters map, indent level — is **not** shared across RTS threads/workers by
default, mirroring Node: each `worker_threads.Worker` gets its own global
`console` bound to worker-local `process.stdout`/`stderr` proxies that pipe
output to the parent. Mapping onto `docs/specs/rts-threading-model.md`:

- The ambient global `console`'s `ConsoleState` is a **per-thread-region**
  value: when an RTS worker thread/region is spawned, it gets its own fresh
  `ConsoleState` handle (own timers/counters/indent), not a handle shared with
  the parent's region — this matches Node's per-Worker `console` exactly and
  avoids any need for cross-thread locking on the hot logging path.
  - **Note on interim scaffolding:** the *ambient* global `console` today
    (`rts-shared/src/stdlib/console.ts`) has been referenced under `#[project
    memory]` as leaning on `GCELLS` thread-local storage for other globals in
    this codebase — the new `rts-node`-owned `Console` must **not** reuse that
    mechanism; it gets its own explicit per-region handle allocation (see
    below), not an incidental thread-local reuse.
- **Underlying OS stdout/stderr file descriptors ARE process-wide** (shared
  across every RTS thread/worker, same as in Node/any OS process) — writes
  from different threads interleave at the OS level exactly as they would in
  Node; RTS does not need to add cross-thread serialization beyond what
  `std::io::Stdout`'s internal lock already provides (line-level atomicity,
  no smearing of a single `write_all` call, matching Node's own guarantees —
  or lack thereof for multi-write sequences).
- **A custom `Console` instance explicitly shared across threads** (the user
  constructs one `Console` and passes its handle into a spawned worker) needs
  its `ConsoleState` handle to live in **shared heap** (per the threading
  model's promotion-on-publication rule) rather than a thread-local region,
  since the handle crossed a thread boundary — the timers/counters maps then
  need a lock (`Mutex`), same pattern as any other cross-thread-published
  native state in `02-runtime.md`.
- No `SharedArrayBuffer`/channel involvement — `console` carries no bytes that
  need the shared-memory/`MessagePort` path; only the handle itself
  (a `u64`) crosses a thread boundary, same as any other handle passed to
  `Worker` args.

### 5.5 Buffer / TypedArray interop

`console.log`/`error`/etc. accept `Uint8Array`/`Buffer` arguments like any
other value — they get `util.inspect`-style rendered as
`Uint8Array(N) [ b0, b1, ... ]` / `<Buffer xx xx ...>` (Buffer's own
`[util.inspect.custom]`), not raw-byte-dumped. No ABI-level byte transfer is
needed beyond what the engine's existing TypedArray/ArrayBuffer primordial
representation already provides — `INSPECT`/`FORMAT` read the typed array's
existing memory view (ptr+len+element-kind) the same way any other primordial
consumer does; `console` adds no new byte-marshalling path. `console.table`
similarly treats a `Uint8Array` row/cell value as an inspected scalar cell,
not specially chunked.

### 5.6 Doctrine placement

`console` (as a `node:console` **module**) is **non-primordial** — the engine
must never hardcode the name `"console"` or `"Console"` in
`crates/rts-codegen-new/`. `import { Console } from "node:console"` resolves
exactly like every other `node:X` import: `ns_prefix_for("node:console")` →
`"node_console"` via the `NODE_SPECS` data table (`NodespaceSpec { node_module:
"console", ns_prefix: "node_console", members: [...] }`), then
`node_lookup("node_console.<method>")` resolves each qualified call to a
`NodespaceMember { symbol: "__RTS_FN_NODE_CONSOLE_<NAME>", .. }` — a pure
data lookup, zero `match "console" => ...` arms anywhere in codegen. This is
the "registry for node" mechanism described in the architecture facts, applied
identically to how `fs`/`path`/`os`/`process`/`util`/`crypto` already resolve
in the current `rts-node::NODE_SPECS`.

The **ambient global `console`** (no import needed) is a separate but related
concern: it is not itself a primordial either (per the Primordial doctrine,
only String/Object/Array/Function/Promise/Boolean/Number/Error+subclasses/
Symbol/BigInt/Proxy/Reflect/TypedArrays are primordial) — today it is realized
as an ambient `.ts` class (`rts-shared/src/stdlib/console.ts`) instantiated
once and injected into every module's scope, per the "singleton globals via
`gcell_classes`" pattern already used for exactly this kind of case (see
`CLAUDE.md`'s ANTI-HARDCODE section, point 1: `const console = new Console()`
referenced from a function is the textbook `funcval::singleton_instance_globals`
case, not a per-name special case). Once `node:console` lands in `rts-node`,
the ambient global's `.ts` class should be **re-pointed to call the same
native externs** (`__RTS_FN_NODE_CONSOLE_*`) that the explicit `node:console`
import uses, so there is exactly one native implementation and two `.ts`
entry points (ambient prelude class + `node:console`'s own class), not two
parallel implementations. Native-extern / `.ts`-shim split is exactly the
split laid out in §5.2.

### 5.7 Shared-infra dependencies (FLAG)

- **None required for the core surface.** Every native operation this module
  needs (`Instant`-based timers, `HashMap`-based counters, `std::io` stream
  writes, `IsTerminal`/console-mode TTY probing, string formatting/inspection,
  the existing engine `trace/` frame-stack facility for `trace()`) is either
  pure Rust std or already engine-owned (the frame stack) — nothing here pulls
  in the shared tokio runtime, the promise-settle path, TLS/rustls, or crypto
  primitives.
- **Soft dependency, not a blocker:** if a *custom* `Console` is bound to a
  `node:net` socket handle whose writes are implemented asynchronously (e.g.
  the socket module chooses to route through the shared tokio runtime for
  backpressure), then `console`'s `WRITE_STDOUT`/`WRITE_STDERR` externs must
  be able to call into that socket module's existing write primitive without
  themselves adding a new tokio dependency — the async infra, if any, belongs
  to `node:net`/`node:fs`, not to `console`. This is noted here only so a
  later implementer of `node:net`/`node:fs` in `rts-node` (which **will** need
  to flag the shared-tokio/event-loop hoist per those modules' own specs)
  does not assume `console` needs the same hoist — it does not.

If none: **none** (beyond the soft, indirect dependency on whatever the target
stream module needs, called out above for completeness).

### 5.8 Implementation phases

1. **(a) Native `ConsoleState` + handle table entry** in `rts-node::console`:
   struct with bound stdout/stderr sink refs (an enum: `Stdio | RawFd(Handle)`
   to support both the process-default fast path and a custom-stream handle),
   timers `HashMap<String, Instant>`, counters `HashMap<String, u64>`, indent
   `u32`, `ignore_errors: bool`, `color_mode`, `group_indentation: u32`. Wire
   `NEW`/`FREE`.
2. **(b) Formatting core**: implement the `%s/%d/%i/%f/%j/%o/%O/%c/%%` parser
   + fallback-to-inspect-each-arg path as a standalone Rust fn (unit-testable
   without the engine); wire `FORMAT`.
3. **(c) Inspection**: single-value `util.inspect`-equivalent renderer
   (primitives, arrays, plain objects, class instances via shape metadata,
   circular-reference guard, `depth`/`showHidden`/`colors` options); wire
   `INSPECT`, hook it into step (b)'s `%o`/`%O` handling and `dir()`.
4. **(d) Basic sinks**: `WRITE_STDOUT`/`WRITE_STDERR` over `std::io`
   for the process-default case; `log/info/debug/error/warn` `.ts` shim
   methods wired end-to-end against the global console first (smallest
   viable vertical slice, testable via the existing test harness's stdout
   capture).
5. **(e) Indentation**: `GROUP_PUSH`/`GROUP_POP`, thread the current indent
   into every `WRITE_STDOUT`/`WRITE_STDERR` call as a line prefix; wire
   `group`/`groupCollapsed`/`groupEnd`.
6. **(f) Counters and timers**: `COUNT`/`COUNT_RESET`/`TIME_START`/
   `TIME_END`/`TIME_LOG`, plus the `.ts`-level unit auto-scaling
   (`ms` vs `s`) and the "already exists"/"does not exist"
   `process.emitWarning`-equivalent (falls back to a plain stderr line if
   `process.emitWarning` itself isn't implemented yet — track as a dependency
   note, not a blocker, since `process` is a separate P0 module).
7. **(g) `assert`**: truthiness gate + `FORMAT` + `WRITE_STDERR`, prefixed
   `"Assertion failed"` / `"Assertion failed: "`.
8. **(h) `table`**: grid layout (`TABLE` extern) — column discovery,
   box-drawing render, `properties` filter, non-tabular fallback to `log`.
9. **(i) `trace`**: wire the engine's existing frame-stack `trace/` capture
   into `TRACE_STACK`, format as `"Trace: " + message + stack`.
10. **(j) `clear`**: `IS_TTY` probe + platform clear-sequence write, no-op
    gate.
11. **(k) Custom-stream constructor**: the 2-overload `new Console(...)`
    resolution in the `.ts` shim, binding to a `node:fs` file handle or an
    in-memory buffer sink as the first concrete non-default-stream targets;
    defer full `node:net` socket binding until that module exists.
12. **(l) `node:console` module wiring**: register in `NODE_SPECS`, add
    `"console"` to the `node_module`/`ns_prefix` table, re-point the ambient
    `.ts` prelude class to call the same natives (§5.6), delete the old
    `engine.display`/`engine.print_line`/`engine.eprint_line` private bridges
    once nothing else depends on them.
13. **(m) `profile`/`profileEnd`/`timeStamp`**: `.ts`-level permanent no-ops,
    no native symbols (documented deferral, not a TODO).

## 6. Test plan

`tests/node_console_basic.test.ts`:
- `console.log("a", 1, true)` → space-joined line on stdout.
- `console.log("count: %d", 5)` and `console.log("count:", 5)` → both produce
  `"count: 5"`.
- `console.error(...)` / `console.warn(...)` land on stderr, not stdout
  (assert via two separate capture buffers).
- `console.info`/`console.debug` behave identically to `console.log`.

`tests/node_console_format_specifiers.test.ts`:
- Each of `%s %d %i %f %j %o %O %c %%` individually, plus a template mixing
  several, plus more args than specifiers (extra args appended), plus fewer
  args than specifiers (literal specifier left in output).
- `%d`/`%i`/`%f` given a non-numeric string/object → `NaN` in output, no throw.
- `%j` on a self-referential object → `"[Circular]"` substring, no throw.
- `%c` consumes its argument and prints nothing extra.

`tests/node_console_assert.test.ts`:
- `console.assert(true, "unreachable")` → no stderr output.
- `console.assert(false, "msg %s", "x")` → stderr line starting with
  `"Assertion failed: msg x"`.
- `console.assert(0)` (no message) → stderr `"Assertion failed"` only.
- Confirm no exception is thrown in any case.

`tests/node_console_group.test.ts`:
- Nested `group("a"); group("b"); log("x"); groupEnd(); log("y"); groupEnd();
  log("z")` → indent increases by `groupIndentation` per level and resets
  correctly; `z` back at indent 0.
- Extra `groupEnd()` beyond depth 0 is a silent no-op (does not throw, does
  not go negative).
- Custom `groupIndentation` via constructor option changes the step size.

`tests/node_console_count_time.test.ts`:
- `count()`/`count("x")`/`count()` sequence → `default: 1`, `x: 1`,
  `default: 2`.
- `countReset("x")` then `count("x")` → back to `x: 1`.
- `time("t")` ... `timeEnd("t")` → one stdout line matching
  `/^t: [\d.]+m?s$/`.
- `timeLog("t2")` after `time("t2")` reports elapsed **and** leaves the timer
  running (a following `timeEnd("t2")` still succeeds).
- `timeEnd("never-started")` → warning path, no throw, no stdout line with a
  bogus elapsed value.

`tests/node_console_dir_table.test.ts`:
- `dir({a: 1, nested: {b: 2}}, { depth: 0 })` truncates nested content
  (compare against `depth: null`/infinite showing the nested object fully).
- `table([{a: 1, b: "y"}, {a: "z", b: 2}])` → header row `(index)`, `a`, `b`
  and two data rows, box-drawing borders present.
- `table(42)` (non-tabular) → falls back to the same output as `log(42)`.
- `table([{a: 1}], ["a"])` → `properties` filters/orders columns.

`tests/node_console_custom_stream.test.ts`:
- `new Console({ stdout: fileSink, stderr: fileSink })` writes both log and
  error lines into the same target, confirming stream binding is per-instance
  and does not affect the global `console`'s own output.
- `ignoreErrors: false` on a sink that throws on write surfaces the error
  instead of swallowing it (contrast with `ignoreErrors: true`, the default).

`tests/node_console_module_identity.test.ts`:
- `import console2 from "node:console"` then `console2 !== console` (module
  export and ambient global are distinct instances) while both still write to
  the real process stdout/stderr.
- `Console === require("node:console").Console` and
  `console.Console === Console`.

`tests/node_console_worker_isolation.test.ts` (multithread):
- Spawn an RTS worker/thread; inside it call `console.count("w")` several
  times and `console.time("wt")`/`timeEnd("wt")`; assert the parent thread's
  own counters/timers for the same labels are unaffected (per-region
  `ConsoleState`, §5.4).
- Construct one `Console` bound to a custom sink in the parent, pass its
  handle into a worker, write from both threads; assert no interleaved/
  corrupted single lines (each `write_all` call remains atomic) and no crash
  under the promoted shared-heap `ConsoleState`.

## 7. Open questions / deferrals

- **`profile`/`profileEnd`/`timeStamp`** are permanent no-ops until/unless RTS
  ever grows an inspector-protocol story; not tracked as a gap against Node
  parity (Node itself no-ops these outside `--inspect`).
- **Exact `ERR_CONSOLE_WRITABLE_STREAM`-style error code/message** for an
  invalid constructor stream argument is marked "(verify)" above — could not
  fetch the literal string from the live docs in this pass; confirm against
  Node source (`lib/internal/console/constructor.js`) before implementing the
  throw path.
- **Exact POSIX/Windows sync-vs-async table wording** for the "note on
  process I/O" cross-reference in §4 is marked "(verify)" — the OS pairing
  described is the long-stable Node behavior from memory/prior knowledge, not
  a verbatim quote; re-confirm the literal text against
  `doc/api/process.md`'s "A note on process I/O" section before citing it in
  user-facing docs.
- **`util.inspect` fidelity** — `console`'s `INSPECT`/`FORMAT` externs need a
  real `util.inspect`-equivalent renderer; this spec assumes that renderer is
  either shared with (or built alongside) `node:util`'s own `format`/`inspect`
  implementation. Whether that shared formatting core lives as a small
  internal module inside `rts-node` (duplicated minimally between `console`
  and `util`) or is factored into one `rts-node`-internal helper both modules
  call is an implementation detail left to whichever module lands first —
  recommend building it once, under `rts-node::util`, and having
  `rts-node::console` call it, to avoid drift between `console.log("%o", x)`
  and `util.format("%o", x)` output.
- **Default constructor form's exact custom-stream target types** for phase
  (k) — this spec only commits to a `node:fs` file handle and an in-memory
  buffer as the first concrete sinks; full `node:net` socket binding is
  explicitly deferred until `node:net` itself exists in `rts-node`.
- **Ambient-global migration timing** (§5.6: re-pointing
  `rts-shared/src/stdlib/console.ts` to the new natives) is a cross-cutting
  change touching a file outside `rts-node` — sequence it as a follow-up PR
  once `node:console`'s native surface is stable, not bundled into the first
  landing, to keep the "regress explicitly" discipline clean (the ambient
  global currently has zero test regressions to protect and should keep it
  that way through the transition).
