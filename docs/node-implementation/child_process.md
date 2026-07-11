# node:child_process

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:child_process` |
| Node.js version | 25.x |
| Stability | 2 - Stable |
| Tier | P1 |
| Status | [ ] Not implemented — spec only |
| Import forms | `import { spawn, exec, execFile, fork, spawnSync, execSync, execFileSync, ChildProcess } from "node:child_process"` · `import * as cp from "node:child_process"` · `const cp = require("node:child_process")` (CJS interop, if/when RTS supports it) |
| Globals exposed | None — all surface is import-only, no ambient globals |

## 1. Purpose

`node:child_process` lets JS/TS code create and control OS-level subprocesses:
run a shell command, exec a binary directly, or `fork()` another script with a
built-in IPC channel. It is the primitive underneath most build tooling,
test runners, git wrappers, and "shell out to a native tool" workloads — RTS
needs full parity here to run real-world npm-ecosystem scripts and to give
native RTS programs the same process-orchestration primitives Node and Bun
offer. The module exposes both non-blocking (`spawn`/`exec`/`execFile`/`fork`)
and blocking (`spawnSync`/`execSync`/`execFileSync`) variants, plus the
`ChildProcess` class (an `EventEmitter`) representing a live or exited child.

## 2. Exported API surface (COMPLETE)

### 2.1 Classes

#### `class ChildProcess extends EventEmitter`

Not constructed directly by user code — instances are returned by `spawn()`,
`exec()`, `execFile()`, and `fork()`. (Internally Node exposes the constructor
as `child_process.ChildProcess`, and RTS must export it too for `instanceof`
checks and subclass-typing compatibility, even though direct `new
ChildProcess()` has no documented public contract.)

**Constructor**

| Form | Notes |
|---|---|
| `new ChildProcess()` | Undocumented/internal in Node; RTS exports the symbol for `instanceof` parity but does not need to support meaningful manual construction. |

**Properties**

| Property | Type | Description |
|---|---|---|
| `subprocess.pid` | `number \| undefined` | PID of the child; `undefined` if the process failed to spawn. |
| `subprocess.connected` | `boolean` | `true` while an IPC channel to the child is open (only relevant to `fork()`-created children); `false` after `disconnect()`/`process.disconnect()` in child. |
| `subprocess.exitCode` | `number \| null` | Exit code once the process has exited; `null` while running or if terminated by signal. |
| `subprocess.killed` | `boolean` | `true` once `subprocess.kill()` has successfully delivered a signal (does **not** mean the process has actually exited yet). |
| `subprocess.signalCode` | `string \| null` | Signal name that terminated the process, else `null`. |
| `subprocess.spawnargs` | `string[]` | Full argument list used to spawn the process (including the resolved `argv[0]`). |
| `subprocess.spawnfile` | `string` | The executable file path that was actually spawned. |
| `subprocess.stdin` | `stream.Writable \| null` | Writable stream when `stdio[0] === 'pipe'`, else `null`. |
| `subprocess.stdout` | `stream.Readable \| null` | Readable stream when `stdio[1] === 'pipe'`, else `null`. |
| `subprocess.stderr` | `stream.Readable \| null` | Readable stream when `stdio[2] === 'pipe'`, else `null`. |
| `subprocess.stdio` | `Array<Stream \| null>` | All configured stdio streams/fds by index, including any beyond fd 2 (e.g. the `'ipc'` slot). |
| `subprocess.channel` | `{ ref(): void; unref(): void } \| undefined \| null` | Reference to the IPC channel object; `undefined`/`null` if no IPC channel exists (i.e. not created via `fork()` or `stdio` has no `'ipc'` entry). |

**Methods**

| Method | Signature | Returns |
|---|---|---|
| `subprocess.kill()` | `kill(signal?: number \| string): boolean` | `true` if the kernel signal delivery call succeeded (does not guarantee the process has died). |
| `subprocess.send()` | `send(message: any, callback?: (error: Error \| null) => void): boolean` · `send(message: any, sendHandle?: SendHandle, callback?: (error: Error \| null) => void): boolean` · `send(message: any, sendHandle?: SendHandle, options?: MessageOptions, callback?: (error: Error \| null) => void): boolean` | `boolean` — `false` if the channel is closed or send was throttled/queued and dropped; only valid for children spawned via `fork()`. |
| `subprocess.disconnect()` | `disconnect(): void` | `void` — closes the IPC channel; allows the child to exit gracefully once it has no other work. |
| `subprocess.ref()` | `ref(): void` | `void` — opposite of `unref()`; makes the child keep the parent's event loop alive (default state). |
| `subprocess.unref()` | `unref(): void` | `void` — lets the parent's event loop exit without waiting for this child. |
| `subprocess[Symbol.dispose]()` | `[Symbol.dispose](): void` | `void` — calls `kill('SIGTERM')`; enables `using subprocess = spawn(...)`. |

**`subprocess.channel` sub-object methods** (only present when `connected`):

| Method | Signature | Returns |
|---|---|---|
| `channel.ref()` | `ref(): void` | `void` — IPC channel keeps parent event loop alive. |
| `channel.unref()` | `unref(): void` | `void` — IPC channel does not keep parent event loop alive. |

**Events** (all via `EventEmitter.on/once/...`)

| Event | Listener signature | Emitted when |
|---|---|---|
| `'spawn'` | `() => void` | Process has spawned successfully; fires before any stdout/stderr data and before `'exit'`. If spawning fails, `'error'` fires instead and `'spawn'` never fires. |
| `'error'` | `(err: Error) => void` | The process could not be spawned, could not be killed, sending a message failed, or the child was aborted via the `signal` option. May or may not be followed by `'exit'`. |
| `'exit'` | `(code: number \| null, signal: NodeJS.Signals \| null) => void` | Process has ended. Exactly one of `code`/`signal` is non-`null` (or both `null` if killed via the `signal`/`timeout` option abort path in some edge cases). Stdio streams may still be open when this fires. |
| `'close'` | `(code: number \| null, signal: NodeJS.Signals \| null) => void` | Fires after `'exit'` (or after `'error'` if the process never spawned) once all stdio streams in the child have been closed. This is the "fully done" event. |
| `'disconnect'` | `() => void` | Fires after `subprocess.disconnect()` or `process.disconnect()` (child-side) is called; IPC channel is now closed. |
| `'message'` | `(message: any, sendHandle: net.Socket \| net.Server \| undefined) => void` | A message sent by the child (via `process.send()` in a `fork()`-created child) has arrived. |

### 2.2 Top-level functions

#### `child_process.spawn(command, args?, options?)`

**Variant:** async (event-driven)
**Returns:** `ChildProcess`
**Throws:** does not throw synchronously for spawn failures (those surface via the `'error'` event); may throw synchronously for invalid argument types/values.

| Param | Type | Optional | Default |
|---|---|---|---|
| `command` | `string` | no | — |
| `args` | `readonly string[]` | yes | `[]` |
| `options` | `SpawnOptions` | yes | `{}` |

#### `child_process.spawnSync(command, args?, options?)`

**Variant:** sync (blocking)
**Returns:** `SpawnSyncReturns<string \| Buffer>`
**Throws:** never throws for a failed spawn (error surfaces in `result.error`); throws `TypeError`/`RangeError` for invalid argument shapes.

| Param | Type | Optional | Default |
|---|---|---|---|
| `command` | `string` | no | — |
| `args` | `readonly string[]` | yes | `[]` |
| `options` | `SpawnSyncOptions` | yes | `{}` |

#### `child_process.exec(command, options?, callback?)`

**Variant:** async, callback-style (no native promise form; wrap with `util.promisify`)
**Returns:** `ChildProcess`
**Throws:** does not throw synchronously for command failures (reported via `callback`'s `error` and the `'error'`/`'exit'` events).

| Param | Type | Optional | Default |
|---|---|---|---|
| `command` | `string` | no | — |
| `options` | `ExecOptions` | yes | `{}` |
| `callback` | `(error: ExecException \| null, stdout: string \| Buffer, stderr: string \| Buffer) => void` | yes | — |

#### `child_process.execSync(command, options?)`

**Variant:** sync (blocking)
**Returns:** `string \| Buffer` — the captured stdout
**Throws:** `Error` (subtype `ExecException`) if the exit code is non-zero, the process is killed by a signal, or `maxBuffer` is exceeded. The thrown error carries `error.status`, `error.signal`, `error.stdout`, `error.stderr`.

| Param | Type | Optional | Default |
|---|---|---|---|
| `command` | `string` | no | — |
| `options` | `ExecSyncOptions` | yes | `{}` |

#### `child_process.execFile(file, args?, options?, callback?)`

**Variant:** async, callback-style (no native promise form; wrap with `util.promisify`)
**Returns:** `ChildProcess`
**Throws:** does not throw synchronously for command failures (reported via `callback`).

| Param | Type | Optional | Default |
|---|---|---|---|
| `file` | `string` | no | — |
| `args` | `readonly string[]` | yes | `[]` |
| `options` | `ExecFileOptions` | yes | `{}` |
| `callback` | `(error: ExecFileException \| null, stdout: string \| Buffer, stderr: string \| Buffer) => void` | yes | — |

#### `child_process.execFileSync(file, args?, options?)`

**Variant:** sync (blocking)
**Returns:** `string \| Buffer` — the captured stdout
**Throws:** `Error` (subtype `ExecFileException`) on non-zero exit / signal termination / `maxBuffer` exceeded, same shape as `execSync`'s throw.

| Param | Type | Optional | Default |
|---|---|---|---|
| `file` | `string` | no | — |
| `args` | `readonly string[]` | yes | `[]` |
| `options` | `ExecFileSyncOptions` | yes | `{}` |

#### `child_process.fork(modulePath, args?, options?)`

**Variant:** async (event-driven, always with an IPC channel available)
**Returns:** `ChildProcess`
**Throws:** does not throw synchronously for spawn failures (reported via `'error'`).

| Param | Type | Optional | Default |
|---|---|---|---|
| `modulePath` | `string \| URL` | no | — |
| `args` | `readonly string[]` | yes | `[]` |
| `options` | `ForkOptions` | yes | `{}` |

### 2.3 Properties & constants

`node:child_process` itself exports no numeric/string constant table (unlike
e.g. `os.constants`). Notable fixed values referenced by the API:

| Name | Value | Where used |
|---|---|---|
| Default `stdio` | `'pipe'` (⇒ `['pipe','pipe','pipe']`) | `spawn`/`exec`/`execFile`/`fork` |
| Default `killSignal` | `'SIGTERM'` | all kill/timeout paths |
| Default `maxBuffer` | `1024 * 1024` (1 MiB) | `exec`/`execFile`/`execSync`/`execFileSync`/`spawnSync` |
| Default `serialization` | `'json'` | `spawn`/`fork` IPC |
| Default shell (POSIX) | `'/bin/sh'` | `exec`/`execSync` |
| Default shell (Windows) | `process.env.ComSpec` (typically `cmd.exe`) | `exec`/`execSync` |
| Deprecation code | `DEP0190` | passing `args` together with `shell: true` to `execFile`/`spawn` |

### 2.4 Events

Covered fully under `ChildProcess` in §2.1 (`'spawn'`, `'error'`, `'exit'`,
`'close'`, `'disconnect'`, `'message'`). There are no module-level (non-instance)
events.

## 3. Types & option objects

```ts
type StdioNull = 'pipe' | 'overlapped' | 'ignore' | 'inherit';
type StdioPipeNamed = 'pipe' | 'overlapped';
type StdioPipe = StdioPipeNamed | undefined | null;
type IOType = StdioNull;

// Per-fd stdio entry: a mode string, an inherited fd number, an existing
// Stream to share, or null/undefined for the default.
type StdioOption =
  | IOType
  | 'ipc'
  | Stream
  | number
  | null
  | undefined;

// Whole-`options.stdio` shape.
type StdioOptions = IOType | Array<StdioOption>;

type SerializationType = 'json' | 'advanced';

interface MessageOptions {
  keepOpen?: boolean; // default: false
}

type SendHandle = unknown /* net.Socket | net.Server | dgram.Socket */;

interface CommonSpawnOptions {
  cwd?: string | URL;                 // default: process.cwd()
  env?: NodeJS.ProcessEnv;            // default: process.env
  argv0?: string;                     // overrides argv[0] sent to the child
  stdio?: StdioOptions;               // default: 'pipe'
  detached?: boolean;                 // default: false
  uid?: number;                       // POSIX only; ignored on Windows
  gid?: number;                       // POSIX only; ignored on Windows
  shell?: boolean | string;           // default: false
  windowsVerbatimArguments?: boolean; // default: false
  windowsHide?: boolean;              // default: false
  signal?: AbortSignal;
  timeout?: number;                   // ms; default: undefined (no timeout)
  killSignal?: string | number;       // default: 'SIGTERM'
}

interface SpawnOptions extends CommonSpawnOptions {
  serialization?: SerializationType; // default: 'json' (only meaningful with an 'ipc' stdio slot)
}

interface SpawnSyncOptions extends CommonSpawnOptions {
  input?: string | NodeJS.ArrayBufferView;
  encoding?: BufferEncoding | 'buffer'; // default: 'buffer'
  maxBuffer?: number;                  // default: 1024 * 1024
}

interface SpawnSyncReturns<T> {
  pid: number;
  output: Array<T | null>;   // [null, stdout, stderr, ...extra fds]
  stdout: T;
  stderr: T;
  status: number | null;     // exit code, or null if terminated by signal
  signal: string | null;     // signal name, or null if exited normally
  error?: Error;             // set if spawn failed or process timed out
}

interface ExecOptions {
  cwd?: string | URL;
  env?: NodeJS.ProcessEnv;
  encoding?: BufferEncoding | 'buffer'; // default: 'utf8'
  shell?: string;                       // default: '/bin/sh' (POSIX) / ComSpec (Windows)
  signal?: AbortSignal;
  timeout?: number;                     // default: 0 (no timeout)
  maxBuffer?: number;                   // default: 1024 * 1024
  killSignal?: string | number;         // default: 'SIGTERM'
  uid?: number;
  gid?: number;
  windowsHide?: boolean;                // default: false
}

interface ExecSyncOptions {
  cwd?: string | URL;
  input?: string | NodeJS.ArrayBufferView;
  stdio?: StdioOptions;                 // default: 'pipe'
  env?: NodeJS.ProcessEnv;
  shell?: string;
  uid?: number;
  gid?: number;
  timeout?: number;
  killSignal?: string | number;         // default: 'SIGTERM'
  maxBuffer?: number;                   // default: 1024 * 1024
  encoding?: BufferEncoding | 'buffer';  // default: 'buffer'
  windowsHide?: boolean;
}

interface ExecFileOptions extends CommonSpawnOptions {
  encoding?: BufferEncoding | 'buffer'; // default: 'utf8'
  timeout?: number;                     // default: 0
  maxBuffer?: number;                   // default: 1024 * 1024
}

interface ExecFileSyncOptions extends CommonSpawnOptions {
  input?: string | NodeJS.ArrayBufferView;
  encoding?: BufferEncoding | 'buffer';  // default: 'buffer'
  maxBuffer?: number;                    // default: 1024 * 1024
}

interface ForkOptions {
  cwd?: string | URL;
  detached?: boolean;                    // default: false
  env?: NodeJS.ProcessEnv;
  execPath?: string;
  execArgv?: string[];                   // default: process.execArgv
  gid?: number;
  serialization?: SerializationType;     // default: 'json'
  signal?: AbortSignal;
  killSignal?: string | number;          // default: 'SIGTERM'
  silent?: boolean;                      // default: false
  stdio?: StdioOptions;                  // overrides `silent` if given
  uid?: number;
  windowsVerbatimArguments?: boolean;
  timeout?: number;
}

// Error shapes passed to exec/execFile callbacks and thrown by the sync forms.
interface ExecException extends Error {
  cmd?: string;
  killed?: boolean;
  code?: number;
  signal?: NodeJS.Signals;
}

interface ExecFileException extends Error {
  cmd?: string;
  killed?: boolean;
  code?: number | string; // string for spawn-level errors, e.g. 'ENOENT'
  signal?: NodeJS.Signals;
  stdout?: string | Buffer;
  stderr?: string | Buffer;
}
```

## 4. Node semantics & edge cases

- **Shell injection is the #1 security foot-gun.** `exec()` *always* spawns a
  shell and interpolates `command` verbatim — never pass unsanitized user
  input to it. `execFile()`/`spawn()` are safe by default (no shell) but become
  exactly as dangerous as `exec()` once `shell: true`/`shell: '<path>'` is set.
- **Sync vs async blocking.** `spawnSync`/`execSync`/`execFileSync` block the
  calling thread/event loop for the entire child lifetime — acceptable for
  build-script/CLI use, never for server request-handling paths.
- **Windows batch files.** `.bat`/`.cmd` files cannot be executed directly via
  `CreateProcess` without a shell; `execFile('script.bat', ...)` without
  `shell: true` fails on Windows. Node historically special-cased detection of
  `.bat`/`.cmd` extensions to route through `cmd.exe`; `exec()` always works
  because it already shells out.
- **PATH resolution differs by platform.** POSIX: honors `options.env.PATH` (or
  `process.env.PATH`), and falls back to `/usr/bin:/bin` if an `env` object is
  supplied without a `PATH` key at all. Windows: environment variable names are
  case-insensitive; if multiple casings of `PATH` are present in `options.env`
  (`PATH`, `Path`, `path`), the lexicographically first key wins — outcome is
  effectively undefined from the caller's point of view if they collide.
- **`detached` semantics differ by platform.** POSIX: the child becomes the
  leader of a new process group and will keep running after the parent exits
  regardless of `detached`'s interaction with `stdio`; typical pattern is
  `detached: true` + `stdio: 'ignore'` + `subprocess.unref()`. Windows: the
  child gets its own console window and is independent of the parent's
  console/process group; `unref()` is still required to let the parent exit
  without waiting.
- **stdio `'pipe'` creates real OS pipes/streams**, not references the child can
  use as `/dev/stdout`-style special files across all platforms; do not treat a
  readable stream as writable or vice versa (undefined behavior).
- **`maxBuffer` truncates and kills.** For `exec`/`execFile`/`execSync`/
  `execFileSync`/`spawnSync`, once combined stdout (or stderr) output exceeds
  `maxBuffer` bytes, the child is killed and the error carries
  `code: 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER'`. Output is silently truncated —
  a multi-byte UTF-8 sequence straddling the truncation point may be corrupted.
  `spawn()`'s raw streams are unaffected (no buffering ceiling — the caller owns
  backpressure).
- **Event ordering:** `'spawn'` fires first (if the process started). Then, at
  process end, `'exit'` fires before `'close'`; `'close'` additionally
  guarantees every stdio stream has been fully closed/drained. `'error'` may
  fire instead of `'spawn'`/`'exit'` (failed to spawn) or in addition to them
  (kill failed, send failed, `signal` aborted) — listeners must not assume
  exactly-once semantics between `'error'` and `'exit'`.
- **Windows signal delivery is restricted.** POSIX signals do not exist on
  Windows; `subprocess.kill(signal)` only recognizes `'SIGKILL'`, `'SIGTERM'`,
  `'SIGINT'`, and `'SIGQUIT'` there, and regardless of which of those is
  requested the process is always terminated forcefully (equivalent to
  `TerminateProcess`, i.e. like `SIGKILL`). Any other signal name is ignored.
- **PID reuse race.** Between reading a child's `pid` and later calling
  `kill()`, if the OS has already reaped and reused that PID for an unrelated
  process, the signal can hit the wrong target. Prefer the built-in `timeout`
  option over manual delayed `kill()` calls when possible.
- **Shell grandchildren survive `kill()`.** On POSIX, `spawn('sh', ['-c',
  'sleep 1000'])` then `.kill()` terminates the shell process but not
  `sleep`, which becomes an orphan — this is expected shell-process-tree
  behavior, not an RTS/Node bug; use `detached` + process-group kill
  (`kill(-pid)`) to take out the whole tree.
- **IPC serialization.** `'json'` (default) round-trips only JSON-serializable
  values. `'advanced'` (uses V8's serialization API in real Node) additionally
  supports `Map`, `Set`, `Date`, `Buffer`/`TypedArray`, circular references,
  etc. `fork()`'s child receives an `NODE_CHANNEL_FD` environment variable
  identifying its IPC file descriptor.
- **`DEP0190`** — passing `args` while `shell: true` (or a shell path string) to
  `execFile()`/`spawn()` is deprecated since v23.11.0/v22.15.0: the shell can
  reinterpret/merge the array in surprising ways; prefer building the full
  command string yourself or using `exec()`.
- **`uid`/`gid` are POSIX-only** — silently ignored (no error) when passed on
  Windows.
- **Common spawn error codes** surfacing on `'error'`/synchronous throw:
  `ENOENT` (command not found / bad cwd), `EACCES` (no execute permission),
  `EPERM`, `ETIMEDOUT` is not itself a raised code — a `timeout` firing instead
  kills the child and surfaces as a normal signal-terminated exit
  (`signal: 'SIGTERM'` by default) plus (for `exec`/`execFile`) `killed: true`
  on the error object.
- **`AbortSignal` support** (`options.signal`) aborts the process the same way
  `timeout` does (kills it), and the callback/promise-wrapped error's `name` is
  `'AbortError'`.
- **Default encodings differ by API.** `spawnSync`/`execFileSync` default to
  `'buffer'` (raw `Buffer` outputs); `exec`/`execFile` (and their sync
  `execSync` cousin for its return value) default to `'utf8'` strings.
- **No native promise API.** There is no `child_process.promises`; the
  ecosystem convention is `util.promisify(exec)`/`util.promisify(execFile)`.

## 5. RTS implementation notes

### 5.1 Native impl mapping

- **Process spawn/kill/wait** — `std::process::{Command, Child, Stdio}`
  (cross-platform baseline for both sync and async paths).
- **POSIX-only options** (`uid`, `gid`, process-group/`detached`) —
  `std::os::unix::process::CommandExt` (`uid()`, `gid()`, `process_group()`)
  under `#[cfg(unix)]`; no-op (matching Node's silent-ignore behavior) under
  `#[cfg(windows)]`.
  built-in `pre_exec`/session APIs are only available in nightly/unstable
  form for some needs — a `setsid()`-equivalent via `libc::setsid` inside a
  `pre_exec` closure is the concrete mechanism for POSIX `detached`.
- **Windows-only options** (`windowsHide`, `detached`'s "own console" +
  process-group semantics, `windowsVerbatimArguments`) —
  `std::os::windows::process::CommandExt::{creation_flags, raw_arg}` under
  `#[cfg(windows)]`, using `CREATE_NO_WINDOW` (hide), `DETACHED_PROCESS` /
  `CREATE_NEW_PROCESS_GROUP` (detached), and `raw_arg` to bypass Rust's default
  argument quoting when `windowsVerbatimArguments: true`.
- **Signal delivery** — POSIX: `libc::kill(pid, signum)` via a small
  rts-node-owned signal-name→number table (`SIGTERM`, `SIGKILL`, `SIGINT`,
  `SIGHUP`, `SIGQUIT`, `SIGUSR1`, `SIGUSR2`, …) — this table is NOT borrowed
  from `rts-std` (independence requirement); rts-node vendors its own minimal
  copy. Windows: `TerminateProcess` for the recognized subset
  (`SIGKILL`/`SIGTERM`/`SIGINT`/`SIGQUIT`), silently ignore anything else, via
  the `windows-sys` crate (direct rts-node dependency, not shared with any
  other crate's FFI bindings).
- **Piped stdio I/O** — `Stdio::piped()` + `std::io::{Read, Write}` on the
  `ChildStdin`/`ChildStdout`/`ChildStderr` handles; concurrent read of two
  output pipes plus writing stdin requires either dedicated OS threads per
  pipe (simple, no extra crate) or `tokio::process` (see §5.3 for the
  tradeoff and the §5.7 flag it depends on).
- **`spawnSync`/`execSync`/`execFileSync`** — `std::process::Command::output()`
  (captures both stdout/stderr to completion) or `.status()` (when stdio is
  `'inherit'`), run directly on the calling thread; `input` is written by first
  spawning with `Stdio::piped()` for stdin, writing the bytes, then dropping
  the stdin handle to send EOF, before reading stdout/stderr to completion.
- **`fork()`** — implemented as re-invoking the current RTS executable
  (`std::env::current_exe()`) with an internal, undocumented subcommand (e.g.
  `rts --node-fork <modulePath> [args...]`) that boots the interpreter/JIT
  directly on `modulePath`, analogous to how Node's `fork()` re-invokes `node`
  itself. `execPath`/`execArgv` (`ForkOptions`) override which binary/pre-args
  are used, mirroring Node's knobs for spawning a different Node build.
- **IPC channel wire format** — rts-node defines its **own** protocol (it does
  not need bit-for-bit compatibility with real Node's internal IPC framing,
  since both ends are always RTS processes): newline-delimited JSON messages
  over a dedicated OS pipe (POSIX: anonymous pipe via `os_pipe`-equivalent
  hand-rolled with `libc::pipe`; Windows: an anonymous pipe via
  `CreatePipe`), with the child discovering its channel fd/handle through an
  `RTS_NODE_CHANNEL_FD` environment variable (naming mirrors Node's
  `NODE_CHANNEL_FD` for conceptual parity, not byte-compatibility).
- **Argv/env marshalling across the ABI** — see §5.2's handle-based
  list-builder pattern (an argv array and an env map do not fit any single
  scalar `AbiType`).

### 5.2 ABI surface

New `Entry` variants are added to the **shared** `HandleTable` enum owned by
`rts-engine` (`crates/rts-engine/src/heap/handles.rs`) — this is the same
mechanism `rts-std` already used to add `ProcessChild`/`Map`/`Vec`/`Regex`/
`CString` variants to that one flat enum, so it does not create any
`rts-node → rts-std` coupling (only the already-allowed `rts-node → rts-engine`
edge):

- `Entry::NodeArgv(Vec<String>)` — an argv-array builder handle.
- `Entry::NodeEnv(Vec<(String, String)>)` — an env-map builder handle.
- `Entry::NodeChildProcess(NodeChild)` — wraps `std::process::Child` +
  optional piped stdin/stdout/stderr handles + optional IPC channel fd.
- `Entry::NodeSpawnSyncResult(NodeSpawnSyncResult)` — bundles
  status/signal/stdout/stderr/error for the synchronous combined-call API.
- `Entry::NodeChannel(NodeChannel)` — the IPC channel side used by `fork()`.

**Externs** (all `#[unsafe(no_mangle)] pub extern "C" fn`, symbol convention
`__RTS_FN_NODE_CHILD_PROCESS_<NAME>`):

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `ARGV_NEW` | — | `Handle` | New empty argv-list handle. |
| `ARGV_PUSH` | `Handle, StrPtr` | `Void` | Appends one argument. |
| `ARGV_FREE` | `Handle` | `Void` | Frees a builder not consumed by `SPAWN`. |
| `ENV_NEW` | — | `Handle` | New empty env-map handle. |
| `ENV_SET` | `Handle, StrPtr, StrPtr` | `Void` | Sets one `key=value` pair. |
| `ENV_FREE` | `Handle` | `Void` | Frees a builder not consumed by `SPAWN`. |
| `SPAWN` | `StrPtr(file), Handle(argv, 0=none), Handle(env, 0=inherit), StrPtr(cwd, empty=inherit), I32(stdio_mode bitpacked 2 bits × 3 fds), Bool(detached), I64(uid, -1=unset), I64(gid, -1=unset), Bool(windows_hide), Bool(windows_verbatim), I32(shell_mode: 0=none/1=default/2=custom), StrPtr(shell_path), Bool(want_ipc)` | `Handle` | `0` = spawn failed; check `LAST_ERROR_MSG`/`LAST_ERROR_CODE`. Consumes the argv/env handles. |
| `SPAWN_SYNC` | (same shape as `SPAWN`, plus) `StrPtr(input_ptr)` | `Handle` | Returns a `NodeSpawnSyncResult` handle (blocking; captures stdout/stderr to completion). |
| `LAST_ERROR_MSG` | — | `Handle` (string) | Thread-local: message of the last failed `SPAWN`/`KILL`/`SEND` on this thread. |
| `LAST_ERROR_CODE` | — | `Handle` (string, e.g. `"ENOENT"`) | Thread-local companion to `LAST_ERROR_MSG`. |
| `PID` | `Handle` | `I64` | `-1` if never spawned. |
| `KILL` | `Handle, StrPtr(signal)` | `Bool` | Delivers the named signal. |
| `WAIT_SYNC` | `Handle` | `I32` | Blocking join; exit code (or last status if already reaped). Sets `LAST_SIGNAL`. |
| `LAST_SIGNAL` | — | `Handle` (string, empty = exited normally) | Thread-local companion to `WAIT_SYNC`/async exit delivery. |
| `WAIT_ASYNC` | `Handle, U64(callback_token)` | `Void` | Arms a background waiter that posts `'exit'`/`'close'` through the event-loop bridge (§5.3). |
| `STDIN_WRITE` | `Handle, StrPtr` | `I64` | Bytes written, or `-1` on a closed pipe. |
| `STDIN_END` | `Handle` | `Void` | Closes stdin (sends EOF to the child). |
| `STDOUT_ON_DATA` / `STDERR_ON_DATA` | `Handle, U64(callback_token)` | `Void` | Arms a background reader thread pushing `'data'`/`'end'`/`'error'` events through the bridge. |
| `DISCONNECT` | `Handle` | `Void` | Closes the IPC channel (if any). |
| `SEND` | `Handle, StrPtr(json), Handle(send_handle, 0=none), Bool(keep_open)` | `Bool` | `'json'` mode only for v1 (§5.5/§7). |
| `CHANNEL_REF` / `CHANNEL_UNREF` | `Handle` | `Void` | ref/unref the IPC channel's keep-alive. |
| `FREE` | `Handle` | `Void` | Releases the `ChildProcess` table entry (GC-hooked once the `.ts` object is unreachable). |
| `SPAWN_SYNC_STATUS` | `Handle` | `I32` | On a `NodeSpawnSyncResult` handle. |
| `SPAWN_SYNC_SIGNAL` | `Handle` | `Handle` (string, empty=none) | " |
| `SPAWN_SYNC_STDOUT` / `SPAWN_SYNC_STDERR` | `Handle` | `Handle` (Buffer) | " |
| `SPAWN_SYNC_ERROR_MSG` / `SPAWN_SYNC_ERROR_CODE` | `Handle` | `Handle` (string) | " |
| `SPAWN_SYNC_FREE` | `Handle` | `Void` | Frees the result bundle. |

**`.ts` shim vs native extern split:** the entire `ChildProcess` JS-shaped
class (EventEmitter subclassing, property getters mapping native state,
`kill()`/`send()`/`disconnect()`/`ref()`/`unref()`/`[Symbol.dispose]()` thin
wrappers around the externs above), all 7 top-level functions (option
normalization/defaulting, POSIX/Windows shell command-line assembly and
quoting, `stdio` array→bitpacked-mode conversion, encoding conversion between
`Buffer` and `string`, `maxBuffer` accumulation + truncation, `timeout`/
`AbortSignal` wiring to `kill()`, Windows `.bat`/`.cmd` auto-shell-routing),
and the minimal `.stdout`/`.stderr`/`.stdin` stream-like facades all live in a
`.ts` shim shipped by `rts-node` — the Rust side never speaks a JS-shaped API.

### 5.3 Async model

- **`spawn()`** returns synchronously (just allocates the handle + starts the
  OS process); all of its events (`'spawn'`, `'exit'`, `'close'`, `'error'`,
  `'message'`, stdio `'data'`) are delivered by handing a completed native
  event to the RTS event loop's callback queue from a background thread/task —
  the exact same generic "native async op → JS callback" bridge every other
  async RTS feature uses (timers, promise settlement).
- **`exec()`/`execFile()`** (callback form) are a thin composition over
  `spawn()`: internal stdout/stderr accumulation into a growing `Buffer` with
  `maxBuffer` enforcement, then a single `callback(error, stdout, stderr)`
  once `'close'` fires. No new async primitive beyond `spawn()`'s.
- **`exec()`/`execFile()` promise form** does not exist natively in Node and
  RTS does not add one either — parity is achieved by making `node:util`'s
  `promisify()` work generically over any callback-shaped function (already
  that module's concern, not this one's).
- **`fork()`** layers a second async I/O source (the IPC channel pipe) on top
  of `spawn()`'s model: inbound bytes are parsed as newline-delimited JSON and
  pushed as `'message'` events through the same bridge; outbound `send()`
  writes are a buffered, non-blocking pipe write (v1 has no stream-style
  `'drain'`/backpressure signal — deferred, see §7).
- **`spawnSync`/`execSync`/`execFileSync`** are fully synchronous: direct
  `std::process::Command::output()`/`.status()` calls on the calling OS
  thread, with zero event-loop involvement — this correctly reproduces Node's
  documented "blocks the entire event loop" semantics because in RTS's model
  that calling thread simply cannot service other JS callbacks while blocked.
- **`timeout`/`signal` (`AbortSignal`)** for the async variants are implemented
  as a watcher that calls the native `KILL` extern after the timeout elapses or
  when the `AbortSignal` fires externally.
- **Which need tokio:** only the background wait/read work benefits from an
  async runtime (`tokio::process::Child` avoids a full OS thread per pipe per
  child). Recommended v1 approach: **do not** take a `tokio` dependency in
  rts-node at all — use plain `std::thread` (one waiter + up to 3 reader/writer
  threads per child with piped stdio). This keeps rts-node's independence
  clean (no shared-runtime problem to solve) at the cost of one-thread-per-pipe
  overhead for very high child-process concurrency; revisit only if profiling
  shows that matters (see §5.7/§7).

### 5.4 Multithread / worker interaction

- A `ChildProcess` handle refers to process-external OS state (the actual
  child process + its pipe fds); the Rust-side `Entry::NodeChildProcess` is
  stored once in the already-thread-safe sharded `HandleTable`, so any RTS
  thread holding the `u64` handle value may call `kill()`/`send()`/etc. on it —
  internal mutation is guarded by the entry's own lock, following the existing
  `with_entry_mut` pattern used by every other handle-backed namespace.
- Mapped onto `docs/specs/rts-threading-model.md`: a `ChildProcess` handle is a
  **shared-heap** resource (like a `Buffer` or a socket) — safe to hand across
  a `channel` to another RTS thread/region because it is already
  handle-indirected (no raw pointer or `Child`/pipe object crosses region
  boundaries, only the opaque `u64`). The underlying OS `Child`/pipe fds are
  not RTS-managed memory, so no per-thread-region promotion logic applies to
  them — only the entry's own internal `Mutex`/lock.
- Every background waiter/reader/writer thread this module spawns per child
  **must** register itself in `gc::thread_registry` (the same registration
  every other RTS-spawned OS thread uses), so the conservative GC stack
  scanner correctly walks its stack instead of missing live handle values it
  might be holding.
- `fork()`'s child is a full separate OS process — its own address space, its
  own GC heap, its own `thread_registry` — no RTS memory sharing occurs at all
  beyond the IPC byte channel. This is the simplest threading case: full
  process isolation, not RTS in-process thread/region isolation.
- No direct relation to `worker_threads` (a distinct future rts-node module)
  beyond sharing the same "spawn background OS thread + bridge completion
  events through the loop" plumbing — keep that bridge implementation generic
  enough to be reused there later (forward-compat note, not a requirement now).

### 5.5 Buffer / TypedArray interop

- `stdout`/`stderr` default to raw bytes: `Buffer` (a `Uint8Array` subclass,
  itself backed by the primordial `TypedArray`/`ArrayBuffer` model). The
  native reader thread pushes each chunk of read bytes as a fresh
  `ArrayBuffer`-backed handle, and the `.ts` shim wraps it with
  `Buffer.from(...)` before emitting `'data'`; when `options.encoding` is set,
  the shim decodes to `string` using the shared UTF-8 (or other) decode path
  `node:buffer` already owns (no separate decoder implemented here).
- `input` (for `spawnSync`/`execSync`/`execFileSync`) accepts
  `string | Buffer | TypedArray | DataView` — the native `SPAWN_SYNC` extern
  only needs a raw `(ptr, len)` byte pair regardless of the JS-side source
  type, so the `.ts` shim normalizes any of those inputs down to a byte
  pointer via the existing typed-array raw-pointer accessor every `Buffer`
  method already uses; no new ABI shape is required.
- IPC `send()` in `'json'` mode serializes with `JSON.stringify` (the existing
  native `JSON`/global path) to a UTF-8 string sent as one line over the
  channel pipe; `'advanced'` mode (Map/Set/Date/Buffer/TypedArray-capable,
  circular-reference-safe) needs a structured-clone-style byte encoder that
  does not exist yet anywhere in RTS — v1 ships `'json'` only and throws/warns
  if `'advanced'` is explicitly requested (see §7).
- `sendHandle` (passing an open `net.Socket`/`net.Server` fd across the IPC
  channel) requires OS-level fd-passing (`SCM_RIGHTS` over a Unix domain
  socket on POSIX; `WSADuplicateSocketW`/`DuplicateHandle` on Windows) — a
  substantial standalone feature, deferred for v1 (accepting only `undefined`
  and throwing/erroring otherwise; see §7).

### 5.6 Doctrine placement

- `child_process` is **non-primordial**: it has no native literal/syntax form
  (reached only via named function calls / a class you never construct
  directly with `new` in user code paths) — the engine (`crates/rts-codegen-new/`)
  MUST NOT name `ChildProcess`, `spawn`, `exec`, `fork`, or any other member of
  this module anywhere in its control flow.
- **Resolution path:** `import { spawn } from "node:child_process"` is handled
  by the engine's already-generic `node:`-specifier import resolution (which
  is not specific to this module) calling `rts_node::ns_prefix_for("node:child_process")`
  → a data hit on `NodespaceSpec { node_module: "child_process", ns_prefix:
  "node_child_process", members: CHILD_PROCESS_MEMBERS }` → codegen-qualified
  calls such as `"node_child_process.spawn"` resolve via
  `rts_node::node_lookup` to a `NodespaceMember` carrying the
  `__RTS_FN_NODE_CHILD_PROCESS_SPAWN` symbol + its `AbiType` signature — the
  exact same generic, data-driven path every other `node:*` module already
  uses (`fs`, `path`, `os`, `process`, `util`, `crypto` in the current
  `NODE_SPECS` table). Zero new codegen code is required for this module.
- **Native-extern vs `.ts`-shim split** — see the closing paragraph of §5.2:
  raw process/pipe/signal/IPC primitives are native externs; the entire
  JS-shaped `ChildProcess` class and all 7 top-level functions' ergonomics
  (option normalization, shell quoting, encoding, event wiring) live in a
  `.ts` shim shipped by `rts-node`, mirroring the existing rule applied to
  `console`/`Map`/`Set`/etc.
- **No `rts-shared`/`rts-std` dependency** — `rts-node`'s `Cargo.toml` stays
  `rts-engine`-only (already verified true today); the module does **not**
  borrow any `__RTS_FN_NS_PROCESS_*` symbol from the old thin-wrapper
  `crates/rts-node/src/process/mod.rs` (that whole old rts-node — a thin table
  over `rts-std`'s namespaces — is being deleted and rewritten per the owner
  decision, and this module's implementation must not resurrect the coupling).

### 5.7 Shared-infra dependencies (FLAG)

- **Event-loop callback dispatch.** The mechanism that lets a background
  native thread hand a completed I/O event (`'exit'`, `'data'`, `'message'`)
  back into the JS callback/microtask queue currently lives as
  `rts-std::event_loop` (`crates/rts-std/src/event_loop.rs`) plus
  `rts-std::promise_slot` (`crates/rts-std/src/promise_slot.rs`). rts-node
  needs the same capability but cannot depend on `rts-std` — **this must be
  hoisted** to a shared low-level location (e.g. into `rts-engine`, or a new
  tiny crate beneath both `rts-std` and `rts-node`) before any of this
  module's async paths (`spawn`/`exec`/`execFile`/`fork`, non-blocking stdio,
  `'exit'`/`'close'`/`'message'` events) can be implemented for real.
- **Shared tokio runtime.** `rts-std::runtime::async_rt::rt()`
  (`crates/rts-std/src/runtime/async_rt.rs`) is the process-wide
  `OnceLock<tokio::Runtime>` every current async feature reuses, with
  `on_thread_start`/`stop` hooks registering workers into
  `gc::thread_registry`. This module's v1 plan (§5.3) deliberately avoids
  needing it by using plain `std::thread` instead of `tokio::process` — so
  **no hoist is required for v1** here specifically, but flag it: if a future
  revision switches to `tokio::process` for scalability, it will need this
  runtime hoisted/shared rather than spinning up a second independent tokio
  runtime (which would double the worker pool and split GC thread
  registration).
- **Thread registration helper.** `gc::thread_registry`'s registration
  function (used by every RTS-spawned background OS thread so the GC stack
  scanner can walk it) must be reachable from `rts-node` without going through
  `rts-std` — verify it already lives in (or is re-exported cleanly from)
  `rts-engine`; if not, it needs the same kind of hoist as the two items
  above.
- **Thread-local error-slot pattern.** Async/await's existing convention for
  propagating failure via a thread-local error slot is documented as living in
  the async/promise/function subsystem (`rts-std`-adjacent). rts-node's
  `LAST_ERROR_MSG`/`LAST_ERROR_CODE`/`LAST_SIGNAL` externs (§5.2) are a
  **private, independent** copy of the same *pattern* (not shared code) — no
  hoist strictly required, but flagged so a future unification effort knows
  two independent thread-local-error-slot implementations exist by design.
- **AbortSignal/AbortController wiring.** `timeout`/`signal` options need to
  observe an `AbortSignal` firing and call the native `KILL` extern.
  `AbortSignal` is not in `CLAUDE.md`'s primordial list, so its plumbing likely
  lives in `rts-std` today (used by `fetch`'s abortable requests) — **flag for
  confirmation at implementation time**: either (a) hoist that plumbing
  alongside the event-loop bridge above, or (b) if `AbortSignal` turns out to
  already be engine/primordial-owned, rts-node observes it directly with no
  `rts-std` dependency.
- If, at implementation time, the team decides v1 ships with plain OS threads
  and a minimal private event-loop hook rts-node owns end-to-end, these flags
  become moot for this module — but that decision (hoist vs. private
  duplicate vs. defer-the-feature) must be made explicitly before phases (b)
  onward in §5.8, not silently assumed.

### 5.8 Implementation phases

a. Argv/env handle builders + `spawnSync`/`execFileSync` MVP: no piping, no
   async, no IPC. `std::process::Command::output()`, the
   `Entry::NodeSpawnSyncResult` bundle + its getter externs, `.ts` shim for
   `spawnSync`/`execFileSync` (and `execSync` building on it plus shell
   command-line assembly). Proves the ABI shape end to end with zero
   event-loop dependency.
b. `spawn()` MVP: async `ChildProcess` with `stdio: 'inherit'`/`'ignore'` only
   (no piping yet) + `pid`/`kill()`/`spawnargs`/`spawnfile` +
   `'spawn'`/`'exit'`/`'error'` events. **Resolve the §5.7 event-loop-bridge
   flag first** — this phase cannot complete without it.
c. Piped stdio (`'pipe'`): stdin `write()`/`end()` + stdout/stderr
   `'data'`/`'end'` push events via a minimal Readable/Writable-like
   `EventEmitter` facade; `'close'` once all pipes are drained and the child
   has exited; wires up `exec()`/`execFile()`/`execSync`/`execFileSync`/
   `spawnSync`'s buffer accumulation + `maxBuffer` enforcement +
   `ERR_CHILD_PROCESS_STDIO_MAXBUFFER`.
d. `exec()`/`execFile()` callback API completing on top of (c); POSIX
   (`/bin/sh -c`) and Windows (`cmd.exe /d /s /c`, correct quoting) shell
   command-line building; `execSync` completing on top of (a)+(c)'s buffer
   logic.
e. `detached`/`ref()`/`unref()`, `uid`/`gid` (POSIX), `windowsHide`/
   `windowsVerbatimArguments`, Windows `.bat`/`.cmd` auto-shell-routing,
   full POSIX signal-name table + the restricted Windows subset for `kill()`.
f. `timeout`/`killSignal`/`AbortSignal` support across all async variants
   (needs the §5.7 AbortSignal flag resolved first).
g. `fork()`: RTS self-re-exec plumbing (`rts --node-fork <modulePath>`), the
   newline-delimited-JSON IPC wire protocol (`'json'` serialization only),
   `send()`/`'message'`/`disconnect()`/`connected`/`channel.ref()`/
   `channel.unref()`.
h. `'advanced'` IPC serialization (defer until a shared structured-clone
   routine exists — see §5.5/§7); `sendHandle` fd-passing (defer, likely out
   of v1 scope entirely — see §5.5/§7).
i. Edge-case hardening: PID-reuse-safe kill (check exit status before
   signaling), an opt-in process-group kill helper for the "shell
   grandchildren survive kill()" gotcha (documented as intentional parity
   with Node unless the team wants an RTS-only ergonomic extra),
   `[Symbol.dispose]()`, and `Symbol.dispose` scoping via `using`.

## 6. Test plan

- `spawn_basic.test.ts` — spawn a trivial command, assert exit code 0, `'spawn'`
  then `'exit'` then `'close'` fire in order, `pid` is a positive integer.
- `spawn_nonzero_exit.test.ts` — child exits with a non-zero code; `exitCode`
  set correctly, `signalCode === null`.
- `spawn_args_roundtrip.test.ts` — args containing spaces, quotes, and shell
  metacharacters (`;`, `|`, `$()`, `&&`) are passed through byte-for-byte and
  are **not** interpreted (proves `spawn()`/`execFile()` do not shell out by
  default).
- `spawn_stdio_pipe.test.ts` — write multiple chunks to `stdin`, read multiple
  `'data'` chunks from `stdout`, assert full accumulated content matches, then
  `'end'` and `'close'` fire.
- `spawn_stdio_ignore_and_inherit.test.ts` — `stdio: 'ignore'` yields
  `stdin/stdout/stderr === null`; `stdio: 'inherit'` shares the parent's
  streams (smoke-tested via a child that must succeed, not via output
  capture).
- `exec_shell_features.test.ts` — `exec()` correctly runs a command using
  shell features (`&&`, a pipe, a glob) that would not work via `execFile()`.
- `exec_maxbuffer_truncates.test.ts` — child produces > `maxBuffer` bytes;
  callback error has `code === 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER'`, output
  truncated, child was killed.
- `execFile_no_shell_is_safe.test.ts` — an argument containing shell
  metacharacters passed via `execFile()` is treated as a literal argument, not
  executed (security-property test).
- `execFile_shell_true_args_deprecated.test.ts` — `shell: true` + an `args`
  array surfaces the `DEP0190` deprecation path.
- `spawnSync_basic.test.ts` / `execSync_basic.test.ts` /
  `execFileSync_basic.test.ts` — exit status/signal/stdout/stderr shape for a
  successful and a failing command each.
- `spawnSync_timeout_kills_child.test.ts` — `timeout` option kills a
  long-running child; `status === null`, `signal === 'SIGTERM'`
  (or the configured `killSignal`).
- `fork_message_roundtrip.test.ts` — parent and `fork()`-spawned child
  exchange `send()`/`'message'` events both directions; `send()`'s optional
  callback fires without error.
- `fork_disconnect.test.ts` — `disconnect()` flips `connected` to `false` and
  fires `'disconnect'` on both ends.
- `kill_signals_posix.test.ts` (POSIX-gated) — `SIGTERM` vs `SIGKILL` produce
  the expected `signalCode`.
- `kill_signals_windows.test.ts` (Windows-gated) — only the recognized subset
  (`SIGKILL`/`SIGTERM`/`SIGINT`/`SIGQUIT`) has any effect; anything else is a
  silent no-op, and all of them terminate forcefully.
- `detached_unref_lets_parent_exit.test.ts` — `detached: true` +
  `stdio: 'ignore'` + `unref()` lets the parent process finish while a
  short-lived detached child keeps running (run on both POSIX and Windows).
- `error_event_enoent.test.ts` — spawning a nonexistent command surfaces
  `'error'` with `err.code === 'ENOENT'`, and `'exit'`/`'spawn'` never fire.
- `close_fires_after_exit.test.ts` — asserts strict ordering: `'exit'` before
  `'close'`, and all stdio streams are guaranteed closed by the time `'close'`
  runs.
- `abortsignal_aborts_child.test.ts` — an external `AbortController.abort()`
  kills the running child; the surfaced error's `name === 'AbortError'`.
- `env_replaces_not_merges.test.ts` — passing an explicit `env` object fully
  replaces (does not merge with) the parent's environment inside the child.
- `windows_batch_file_spawn.test.ts` (Windows-gated) — spawning a `.bat`/`.cmd`
  file without `shell: true` behaves per the chosen §7 policy (fails cleanly
  or auto-routes through `cmd.exe`, whichever RTS decides to implement).
- `concurrent_spawn_multithread.test.ts` — spawn N children concurrently from
  multiple RTS threads/regions; assert `HandleTable` thread-safety, every
  waiter/reader thread completes and deregisters, and the GC does not collect
  a live `ChildProcess` handle mid-flight (regression test for the
  conservative stack scanner + `thread_registry`).
- `symbol_dispose_kills_on_scope_exit.test.ts` — `using child = spawn(...)`
  (or an explicit `child[Symbol.dispose]()` call) sends `SIGTERM` on scope
  exit.

## 7. Open questions / deferrals

- **`'advanced'` IPC serialization fidelity** (Map/Set/Date/Buffer/TypedArray,
  circular references) needs a shared structured-clone-style byte encoder that
  does not exist anywhere in RTS yet; v1 ships `'json'` serialization only.
- **`sendHandle`** (passing an open `net.Socket`/`net.Server` fd across the IPC
  channel) requires OS-level fd-passing (`SCM_RIGHTS` / `WSADuplicateSocketW`)
  — a substantial standalone feature; deferred, likely out of v1 scope
  entirely.
- **Full `node:stream.Readable`/`Writable` parity** for `.stdout`/`.stderr`/
  `.stdin` — v1 ships a minimal `EventEmitter`-based facade (`'data'`/`'end'`/
  `'error'` events, a plain `write()`/`end()` with no backpressure signal);
  true streams (backpressure, `.pipe()`, async iteration) wait on a dedicated
  `node:stream` module landing.
- **Windows `.bat`/`.cmd` auto-shell-routing exact algorithm** — decide whether
  to replicate Node's historical CVE-driven auto-detection-and-route-through-
  `cmd.exe` behavior precisely, or take the simpler "require `shell: true`
  explicitly, else throw a clear error" path for v1.
- **AbortController/AbortSignal ownership location** (primordial/engine-owned
  vs. `rts-std`-hosted) must be confirmed before implementing §5.8 phase (f) —
  see the §5.7 flag.
- **Scope of `fork()`'s re-exec target** — whether `rts --node-fork` should
  support forking an arbitrary Node-style script/module or is restricted to
  RTS-compiled/interpretable modules only; affects how faithfully
  `execPath`/`execArgv` need to behave.
- **Process-group "kill the whole shell subtree" convenience helper** — Node
  does not auto-fix the "shell grandchildren survive `kill()`" gotcha either;
  decide whether RTS offers an extra ergonomic escape hatch (e.g. an
  RTS-specific `killTree()`) or intentionally matches Node's rough edge
  exactly for strict parity.
- **`overlapped` stdio mode** (Windows `FILE_FLAG_OVERLAPPED`) — likely low
  priority; confirm whether any real target workload needs it before
  implementing, since even Node's own docs flag it as an advanced/rare case.
