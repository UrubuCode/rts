# node:wasi

**RTS rts-node implementation spec — Node.js 25 parity.**

| Field | Value |
|---|---|
| Module | `node:wasi` |
| Node.js version | 25.x (`https://nodejs.org/docs/latest-v25.x/api/wasi.html`) |
| Stability | 1 - Experimental (unchanged since introduction in v13.3.0/v12.16.0 — six-plus years without promotion to Stable; see §7 for the deferral recommendation this status supports) |
| Tier | P2 |
| Status | ⚠️ **Stale row — see [`node_completed.md`](./node_completed.md).** It claimed "not implemented" for modules that had already landed; the verified 100%-vs-partial state lives in the tracker, in ONE place. |
| Import forms | `import { WASI } from 'node:wasi'`; CJS `const { WASI } = require('node:wasi')`. **No default export** — `WASI` is the module's only member. No documented legacy bare specifier (`require('wasi')`) — this module has always lived exclusively under the `node:` prefix (verify against Node's module-alias table at implementation time, same caution `sqlite.md` flags for its own no-bare-alias claim). |
| Globals exposed | None. `node:wasi` adds nothing to `globalThis` — every use goes through the imported `WASI` class. (RTS's own `WebAssembly` global, needed to actually run a `.wasm` module wired to a `WASI` instance, is a **separate** ECMAScript-standard surface this module depends on — see §5.1/§5.7.) |

## 1. Purpose

`node:wasi` implements the [WebAssembly System Interface](https://wasi.dev/)
(WASI) — a capabilities-based, POSIX-like syscall table (`fd_read`/`fd_write`/
`path_open`/`clock_time_get`/`random_get`/`args_get`/`environ_get`/…) that a
compiled WebAssembly module can import and call to reach the host operating
system, exactly the way a natively-compiled executable calls into libc. The
module's entire public surface is one class, `WASI`: constructing an instance
fixes the command-line `args`, `env`, preopened directory mappings, and
standard-stream file descriptors a Wasm module will see; `wasi.start()`/
`wasi.initialize()` then wire that capability set into a real
`WebAssembly.Instance` (created through the ordinary, JS-standard
`WebAssembly.compile`/`instantiate` API — **not** part of `node:wasi` itself)
and run it. **Node's own docs are explicit that this is not a security
boundary**: "the current Node.js threat model does not provide secure
sandboxing as is present in some WASI runtimes… the file system sandboxing can
be escaped with various techniques." RTS must preserve this exact framing and
must not imply a stronger guarantee than Node itself claims. This is also the
**only** Node built-in module whose implementation genuinely requires RTS to
execute foreign, dynamically-supplied machine-independent bytecode (a `.wasm`
binary) rather than TS/JS source RTS's own compiler produced — see §5.1/§5.7
for why that makes this module's dependency shape unlike any other P0–P2
module specced so far.

## 2. Exported API surface (COMPLETE)

Node's own `node:wasi` reference page documents the `WASI` class at the
method/property level only — it does **not** itemize the ~40 individual WASI
preview1 syscalls (`fd_write`, `path_open`, `clock_time_get`, …) that populate
`wasi.wasiImport`; those are specified externally by the
[WASI preview1 spec](https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md),
not by Node. This section is complete with respect to what Node's own docs
define; §5.2 lists the syscall-level native externs RTS actually needs to
implement `wasiImport`'s contents.

### 2.1 Classes

#### `class WASI`

Not a subclass of `EventEmitter` or any stream base — a plain class, one
instance per "distinct environment" a Wasm module runs against.

**Constructor**

```typescript
new WASI(options?: WASIOptions)
```

- Added in: v13.3.0, v12.16.0.
- Description: creates a new WASI instance representing a distinct
  environment (its own `args`/`env`/`preopens`/std-stream fds/exit behavior).
- Throws: an error (verify exact shape/code — Node's page does not spell out
  a specific error code here) if `version` is omitted or is not one of
  `'unstable'`/`'preview1'` — mandatory with no default since v20.0.0.
- Variant: sync.

See §3 for the full `WASIOptions` field list with defaults.

**Instance properties**

| Property | Type | Notes |
|---|---|---|
| `wasi.wasiImport` | `Object` | Added v13.3.0/v12.16.0. Implements the WASI system-call API directly — this is the raw function table that must be passed as the `wasi_snapshot_preview1` (or `wasi_unstable`) import during `WebAssembly.Instance` instantiation. `getImportObject()` below is a convenience wrapper around this same object. |

**Instance methods**

| Method | Signature | Added |
|---|---|---|
| `wasi.start(instance)` | `(instance: WebAssembly.Instance): number \| void` | v13.3.0, v12.16.0 |
| `wasi.initialize(instance)` | `(instance: WebAssembly.Instance): void` | v14.6.0, v12.19.0 |
| `wasi.getImportObject()` | `(): { wasi_unstable: object } \| { wasi_snapshot_preview1: object }` | v19.8.0 |
| `wasi.finalizeBindings(instance[, options])` | `(instance: WebAssembly.Instance, options?: { memory?: WebAssembly.Memory }): void` | v24.4.0 |

**`wasi.start(instance)`**

- Attempts to begin execution of `instance` as a WASI **command** by invoking
  its `_start()` export.
- Throws if `instance` has no `_start()` export, **or** if `instance` has an
  `_initialize()` export (command and reactor shapes are mutually exclusive).
- Requires `instance` to export a `WebAssembly.Memory` named `memory`; throws
  if absent.
- Throws if called more than once on the same `WASI` instance.
- Internally calls `finalizeBindings()` first if not already done.
- Return/exit behavior is governed by the `returnOnExit` constructor option
  (see §3/§4): `true` (default since v20.1.0) → returns the exit code passed
  to `__wasi_proc_exit()` instead of terminating; `false` → the whole Node.js
  (RTS) process exits with that code.
- Variant: sync (the call blocks until the Wasm module's `_start` returns or
  calls `proc_exit`).

**`wasi.initialize(instance)`**

- Attempts to initialize `instance` as a WASI **reactor** by invoking its
  `_initialize()` export, if present.
- Throws if `instance` has a `_start()` export (mutually exclusive with
  `start()` — a module is either a command or a reactor, never both).
- Same `memory` export requirement and call-once-only rule as `start()`.
- Internally calls `finalizeBindings()` first if not already done.
- Difference from `start()`: a reactor's `_initialize()` sets up module state
  without handing control to a "main" entry point the way a command's
  `_start()` does — the module's actual API surface is invoked afterward via
  its other, ordinary exports (outside `node:wasi`'s own responsibility).
- Variant: sync.

**`wasi.getImportObject()`**

- Returns an import object directly usable as the second argument to
  `WebAssembly.instantiate(module, importObject)` when the Wasm module
  imports nothing beyond the WASI syscall table.
- If the constructor was given `version: 'unstable'`: returns
  `{ wasi_unstable: wasi.wasiImport }`.
- If given `version: 'preview1'` (or no version — pre-v20.0.0 default):
  returns `{ wasi_snapshot_preview1: wasi.wasiImport }`.
- Variant: sync.

**`wasi.finalizeBindings(instance[, options])`**

- Added v24.4.0. Sets up the WASI host bindings against `instance` **without**
  calling `initialize()` or `start()` — useful when the WASI module is
  instantiated in **child threads** so its linear memory can be shared across
  threads without re-running `_start()`/`_initialize()` on each one (direct
  tie-in to RTS's own threading model — see §5.4).
- Requires either `instance` to export a `memory`, or an explicit
  `options.memory` (`WebAssembly.Memory`); throws if neither yields a valid
  memory.
- `start()`/`initialize()` call this internally; calling it more than once
  (directly, or once directly plus once via `start()`/`initialize()`) throws.
- Variant: sync.

**Events:** none. `WASI` is not an `EventEmitter`.

### 2.2 Top-level functions

None. Every capability is reached through the `WASI` class.

### 2.3 Properties & constants

None documented at the `node:wasi` module level (contrast `node:sqlite`'s
`constants` object or `node:buffer`'s `INSPECT_MAX_BYTES`) — WASI's own
errno/rights/fd-flags constants are internal to the `wasiImport` syscall
table's wire protocol (the WASI preview1 spec), not exposed as a JS-visible
`wasi.constants` object by Node.

### 2.4 Events

None. No class in `node:wasi` extends `EventEmitter`.

---

## 3. Types & option objects

```typescript
type WASIVersion = 'unstable' | 'preview1';

interface WASIOptions {
  /**
   * Mandatory, no default. Added v19.8.0; made required (no fallback) in
   * v20.0.0. Only 'unstable' and 'preview1' are currently supported.
   */
  version: WASIVersion;

  /**
   * Command-line arguments the Wasm application sees. The first entry is
   * conventionally the virtual path to the WASI command itself.
   * Default: [].
   */
  args?: string[];

  /**
   * Environment variables the Wasm application sees, shaped like
   * `process.env`. Default: {}.
   */
  env?: Record<string, string>;

  /**
   * Maps the Wasm application's view of its own directory structure to real
   * host paths. Keys are virtual directories inside the sandbox; values are
   * the corresponding real paths on the host machine. No default — an
   * application given no preopens can access no host directory at all
   * through WASI's path_* syscalls.
   */
  preopens?: Record<string, string>;

  /** File descriptor used as stdin inside the Wasm application. Default: 0. */
  stdin?: number;

  /** File descriptor used as stdout inside the Wasm application. Default: 1. */
  stdout?: number;

  /** File descriptor used as stderr inside the Wasm application. Default: 2. */
  stderr?: number;

  /**
   * true (default, since v20.1.0 — earlier versions defaulted to actually
   * terminating the process): wasi.start() returns with the exit code passed
   * to __wasi_proc_exit() instead of ending the process.
   * false: the whole process exits with that code.
   */
  returnOnExit?: boolean;
}

interface FinalizeBindingsOptions {
  /** Default: instance.exports.memory. */
  memory?: WebAssembly.Memory;
}

/** wasi.getImportObject()'s return shape, keyed by the constructor's version. */
type WasiImportObject =
  | { wasi_unstable: Record<string, (...args: number[]) => number> }
  | { wasi_snapshot_preview1: Record<string, (...args: number[]) => number> };
```

Every WASI syscall in `wasiImport` follows the WASI preview1 C ABI convention:
all parameters and the return value are 32-bit integers (pointers/lengths
into the Wasm module's linear memory, or plain scalars); the return value is
a WASI errno code (`0` = success). Node's own docs do not restate this shape
per-function — it is defined by the external WASI spec, referenced rather
than reproduced here.

---

## 4. Node semantics & edge cases

- **Not a sandbox — verbatim from Node's own docs**: "The `node:wasi` module
  does not currently provide the comprehensive file system security
  properties provided by some WASI runtimes. Full support for secure file
  system sandboxing may or may not be implemented in future. In the mean
  time, do not rely on it to run untrusted code." And, on the capability
  model itself: "the current Node.js threat model does not provide secure
  sandboxing… the file system sandboxing can be escaped with various
  techniques." RTS must reproduce this disclaimer, not soften it.
- **`version` is mandatory** with no default since v20.0.0 (added as optional
  in v19.8.0). The only two accepted values gate the import-object namespace
  `getImportObject()` returns (`wasi_unstable` vs `wasi_snapshot_preview1`) —
  they do **not** otherwise change `wasiImport`'s function set in any way
  Node's docs describe beyond that namespace string.
- **Command vs reactor is a strict either/or.** A module with `_start()`
  must be run via `start()`; a module with `_initialize()` must be run via
  `initialize()`. Calling the wrong method throws — Node treats "has both" or
  "has neither of the expected export, given the method called" as caller
  error, not something to silently coerce.
- **The `memory` export is mandatory** for `start()`, `initialize()`, and
  `finalizeBindings()` (the last accepts an explicit `options.memory`
  override instead). Every WASI syscall marshals bytes through this memory —
  without it there is no way to pass strings/buffers across the host/Wasm
  boundary at all.
- **Idempotency/ordering**: `start()`, `initialize()`, and
  `finalizeBindings()` are each **call-once** per `WASI` instance (a second
  call to any of them throws); `start()`/`initialize()` transparently call
  `finalizeBindings()` first if it has not already run.
- **`returnOnExit` default flip (v20.1.0)**: earlier versions defaulted to
  actually terminating the host process when the Wasm module called
  `__wasi_proc_exit()`; the current (and this spec's target) default is
  `true` — `start()` returns the exit code as a normal value instead. RTS
  targets **only** the current (v20.1.0+) default; the pre-flip behavior is
  not part of this spec.
- **`preopens` has no default** — an application given none can perform no
  `path_open`/directory-relative syscall against the real filesystem at all;
  this is WASI's actual capability-restriction mechanism (as opposed to a
  genuine OS-level sandbox — see the non-sandboxing note above).
- **`getImportObject()` is a pure convenience** over `wasiImport` — a caller
  needing to combine WASI imports with other, non-WASI imports in the same
  `importObject` must build that merged object by hand, reading
  `wasi.wasiImport` directly instead of calling `getImportObject()`.
- **`finalizeBindings()`'s stated purpose is multithreading**: "useful when
  the WASI module is instantiated in child threads for sharing the memory
  across threads" — i.e. one thread runs `start()`/`initialize()` (which
  finalizes bindings once), while other threads that only need to *share*
  the same linear memory (not re-run `_start`/`_initialize`) call
  `finalizeBindings()` directly.
- **No `node:wasi`-specific Windows/POSIX divergence is documented** by Node
  itself; the underlying `preopens` real-path resolution necessarily inherits
  whatever host-path semantics the platform's filesystem has (drive
  letters/UNC on Windows vs POSIX absolute paths) — RTS should reuse its own
  `fs`/`path` platform-normalization logic here rather than inventing new
  rules (see §5.1).
- **Deprecations**: none within the current `WASI` class API — the class
  itself (v13.3.0+) already superseded an older, pre-release WASI experiment
  in Node that predates the stable API surface documented here; this spec
  targets only the class shape as it exists in Node 25.

---

## 5. RTS implementation notes

### 5.1 Native impl mapping

This module's dependency shape is unlike every other module specced so far —
including `node:vm`, which is the other "no equivalent engine capability
exists yet" P2 module. `node:vm`'s gap is a missing **scope-indirection**
capability inside RTS's own Cranelift lowering (a few new concepts bolted
onto a compiler RTS already owns end-to-end). `node:wasi`'s gap is a missing
**entire second execution engine**: to run a `.wasm` binary at all, something
has to decode WebAssembly bytecode and either interpret it or compile it to
native code — RTS's own compiler pipeline (TS/JS → HIR → Cranelift IR, per
`architecture.md`) has no path that starts from `.wasm` bytes. Concretely,
two separable pieces are needed, and it is important not to conflate them:

1. **A `WebAssembly` global (JS-standard, not Node-specific).** Real Node's
   `wasi.start(instance)` takes a `WebAssembly.Instance` — an object created
   through `WebAssembly.compile`/`instantiate`, which in real Node/browsers
   is implemented by the JS engine itself (V8), **not** by `node:wasi`. RTS
   has no `WebAssembly` global today (it is not mentioned anywhere in
   `architecture.md`'s primordial/ABI inventory). This is arguably not
   `node:wasi`-specific surface at all — it is a general ECMAScript global
   any RTS program could reach for even outside the Node-compat surface —
   but `node:wasi` is currently the **only** specced module whose usage
   pattern presupposes it exists. **This is a hard, blocking dependency**:
   nothing in this module can be implemented for real without it.
2. **The WASI syscall table itself (`wasiImport`)** — this part genuinely is
   `node:wasi`'s own job, and maps cleanly onto capability RTS already has:
   each WASI host function (`fd_write`, `fd_read`, `path_open`,
   `clock_time_get`, `random_get`, `args_get`, `environ_get`, …) is a thin
   Rust trampoline that (a) reads its integer/pointer arguments, (b)
   reads/writes raw bytes through the Wasm module's linear memory (via
   whatever `Memory`-data-pointer API the chosen Wasm engine exposes — see
   below), and (c) delegates the actual work to `rts-node`'s **own,
   already-designed-elsewhere** native primitives: file I/O through the same
   Rust code backing `node:fs`'s externs (see `fs.md`), process
   args/env through the same code backing `node:process`/`node:os`, wall
   clock and monotonic clock reads through the same primitives `node:perf_hooks`/
   `node:process.hrtime` use, and CSPRNG bytes through the same source
   `node:crypto`'s `randomBytes` uses. **No new syscall-level Rust logic is
   invented here** — this is a marshalling/adapter layer over primitives this
   spec assumes already exist per their own module docs.

**Recommended shape for piece 1 (owner decision, see §7):** embed the
[`wasmtime`](https://crates.io/crates/wasmtime) crate (or `wasmer`) inside
`rts-node` as a Wasm compile+execute engine, and implement `WebAssembly.compile`/
`instantiate`/`Module`/`Instance`/`Memory`/`Table` as a thin JS-facing layer
over it. A from-scratch RTS-native Wasm interpreter/compiler is the
alternative extreme — technically possible (RTS already owns a native-codegen
pipeline in a different domain) but a multi-month undertaking disproportionate
to a single Stability-1 module; not recommended as the first cut. Notably,
`wasmtime` ships its own `wasmtime-wasi` crate implementing the WASI host
functions directly against the *host's real* filesystem/clock/random — RTS
should **not** simply wire that crate in wholesale, since doing so would
bypass `rts-node`'s own `fs`/`os`/`crypto` primitives (and their preopen/
sandboxing semantics, error-code mapping, and platform-normalization logic)
in favor of a second, divergent implementation; `wasmtime` (or `wasmer`) is
recommended purely as **the piece that runs Wasm bytecode**, with the WASI
host-function table implemented natively in `rts-node` per point 2 above.

**Handle storage** (rts-node-local, mirroring the pattern established in
`sqlite.md`'s `SqliteConnEntry`/`SqliteStmtEntry`, since `Entry::Backend(Box<dyn
Traceable>)` — `architecture.md` §6 — is still a foundation prerequisite, not
yet landed):

- `WasiEnvEntry { args: Vec<String>, env: Vec<(String, String)>, preopens: Vec<(String, PathBuf)>, stdin_fd: i32, stdout_fd: i32, stderr_fd: i32, return_on_exit: bool, version: WasiVersion, bound_instance: Option<Handle>, finalized: bool }` — one per `new WASI(...)` call.
- `WasmInstanceEntry { engine_instance: <wasmtime::Instance or equivalent>, memory: <wasmtime::Memory>, owner_thread: ThreadId }` — the RTS-side wrapper around whatever the chosen Wasm engine's own instance/memory types are, backing the `WebAssembly.Instance`/`Memory` global (piece 1, not `node:wasi`-specific storage, but the type `wasi.start(instance)` receives).

### 5.2 ABI surface

Symbol convention: `__RTS_FN_NODE_WASI_<NAME>` for the `WASI` class itself
(`ns_prefix = "node_wasi"`); the `WebAssembly` global's own compile/instantiate
externs (piece 1 above) are **out of scope for this module's symbol table** —
they belong to whichever spec ends up owning the `WebAssembly` global (§5.7/§7)
and are only referenced here by the `Handle` type they hand back.

| Symbol | Args (`AbiType`) | Returns | Notes |
|---|---|---|---|
| `__RTS_FN_NODE_WASI_NEW` | `Handle args_vec, Handle env_pairs, Handle preopens_pairs, I32 stdin_fd, I32 stdout_fd, I32 stderr_fd, Bool return_on_exit, StrPtr version` | `Handle` (WasiEnvEntry) | Validates `version ∈ {'unstable','preview1'}`; sets the error slot otherwise. |
| `__RTS_FN_NODE_WASI_GET_IMPORT_OBJECT` | `Handle wasi_env` | `I64` *(opaque tagged Object value — see the "opaque `any`" convention `vm.md` §5.2 establishes for `rts-node` crossing arbitrary JS values)* | Builds `{wasi_unstable:…}` or `{wasi_snapshot_preview1:…}` wrapping the function table from the next row. |
| `__RTS_FN_NODE_WASI_GET_WASI_IMPORT` | `Handle wasi_env` | `I64` *(opaque tagged Object of native function pointers)* | Backs `wasi.wasiImport`; the object's own values are the syscall trampolines below, reified as callable `Function` handles (same "native fn as first-class JS function" mechanism the engine's `Entry::Function` already supports). |
| `__RTS_FN_NODE_WASI_START` | `Handle wasi_env, Handle wasm_instance` | `I64` (exit code) or throws | Validates `_start` xor `_initialize`, the `memory` export, and call-once; calls `FINALIZE_BINDINGS` internally first. |
| `__RTS_FN_NODE_WASI_INITIALIZE` | `Handle wasi_env, Handle wasm_instance` | `Void` or throws | Mirrors `START`'s validation with `_initialize`/`_start` swapped. |
| `__RTS_FN_NODE_WASI_FINALIZE_BINDINGS` | `Handle wasi_env, Handle wasm_instance, Handle memory_override_or_0` | `Void` | Wires the syscall trampolines against `wasm_instance`'s (or the override's) linear memory; call-once per `wasi_env`. |
| **WASI preview1 syscalls** (~46 total; representative rows below — full list follows the external [WASI preview1 spec](https://github.com/WebAssembly/WASI/blob/main/legacy/preview1/docs.md), which Node's own page does not itemize either, per §2) | | | |
| `__RTS_FN_NODE_WASI_SYS_ARGS_GET` / `_ARGS_SIZES_GET` | `Handle wasi_env, U64 mem_ptr_argv, U64 mem_ptr_argv_buf` (sizes variant: no mem ptrs, returns packed counts) | `I32` (WASI errno) | Copies `wasi_env.args` into linear memory in the WASI argv layout. |
| `__RTS_FN_NODE_WASI_SYS_ENVIRON_GET` / `_ENVIRON_SIZES_GET` | analogous to `args_*` | `I32` | Copies `wasi_env.env`. |
| `__RTS_FN_NODE_WASI_SYS_CLOCK_TIME_GET` | `Handle wasi_env, I32 clock_id, U64 mem_ptr_out` | `I32` | Delegates to the same wall/monotonic clock primitives `node:process.hrtime`/`node:perf_hooks` use (§5.1). |
| `__RTS_FN_NODE_WASI_SYS_RANDOM_GET` | `Handle wasi_env, U64 mem_ptr, U64 len` | `I32` | Delegates to the same CSPRNG source `node:crypto.randomBytes` uses. |
| `__RTS_FN_NODE_WASI_SYS_PROC_EXIT` | `Handle wasi_env, I32 exit_code` | `Void` (never returns to the Wasm module — unwinds the host call) | Sets the pending-exit state `START`'s `returnOnExit` branch reads. |
| `__RTS_FN_NODE_WASI_SYS_FD_WRITE` / `_FD_READ` | `Handle wasi_env, I32 fd, U64 mem_ptr_iovs, I32 iovs_len, U64 mem_ptr_nwritten_or_nread` | `I32` | Reads/writes the iovec array from linear memory; for `fd ∈ {stdin,stdout,stderr}` delegates to `node:process`'s stream primitives, for other `fd`s to the file-handle table below. |
| `__RTS_FN_NODE_WASI_SYS_PATH_OPEN` | `Handle wasi_env, I32 preopen_fd, StrPtr rel_path (via mem_ptr+len), I32 oflags, I64 rights_base, I64 rights_inheriting, I32 fdflags, U64 mem_ptr_out_fd` | `I32` | Resolves `rel_path` against the preopen mapping, then delegates to `node:fs`'s own open primitive (see `fs.md`) with the resolved host path. |
| `__RTS_FN_NODE_WASI_SYS_FD_CLOSE` / `_FD_SEEK` / `_FD_TELL` / `_FD_FDSTAT_GET` / `_FD_FILESTAT_GET` / `_PATH_FILESTAT_GET` / `_PATH_CREATE_DIRECTORY` / `_PATH_REMOVE_DIRECTORY` / `_PATH_UNLINK_FILE` / `_PATH_RENAME` / `_POLL_ONEOFF` / `_SCHED_YIELD` / … | per-syscall (`Handle wasi_env` + WASI's own documented integer/pointer args) | `I32` (WASI errno) | Each a thin adapter over the corresponding existing `node:fs`/`node:os` extern; no new filesystem logic. |
| `__RTS_FN_NODE_WASI_SYS_SOCK_ACCEPT` / `_SOCK_RECV` / `_SOCK_SEND` / `_SOCK_SHUTDOWN` | per WASI preview1 spec | `I32` | Preview1 defines these but most runtimes (including real Node's WASI implementation) leave them effectively unimplemented/`ENOSYS`-returning; RTS should match that stance unless a concrete use case appears (§7). |

Every syscall trampoline's `mem_ptr`/`len` pairs read/write the Wasm engine's
linear memory directly (piece 1's `Memory` type, §5.1) — **not** an RTS
`ArrayBuffer` handle; see §5.5.

### 5.3 Async model

- **The entire `WASI` class surface is synchronous**, matching Node exactly:
  `new WASI(...)`, `getImportObject()`, `start()`, `initialize()`, and
  `finalizeBindings()` all complete before returning; there is no
  Promise/callback-shaped member anywhere in this module.
- **Every WASI syscall trampoline is invoked synchronously and
  re-entrantly** from inside the Wasm engine's execution of `_start()`/
  `_initialize()`/any other export — the same "native code calls back into a
  stored callback, blocks until it returns" shape `sqlite.md` §5.3 documents
  for SQLite's user-defined-function callbacks, except here the "callback"
  side is always native Rust (the WASI host function), never JS — so there
  is no JS-callback-re-entrancy concern at all, only "does this Rust
  trampoline correctly read/write the engine's linear memory and call the
  right existing native primitive."
- **No tokio/event-loop involvement** for the syscall table itself — every
  WASI preview1 syscall is defined as synchronous in the spec (there is no
  async I/O story in WASI preview1; `poll_oneoff` is a synchronous multiplex
  primitive, not a promise-returning one). Listed explicitly here only to
  rule it out, per the pattern `buffer.md`/`sqlite.md` established for
  modules with no genuine async surface.
- **`wasi.start()`'s `returnOnExit` behavior is the one place "async" language
  could mislead**: it is not asynchronous — `__wasi_proc_exit()` unwinds the
  synchronous call stack back out to `start()`'s caller (or terminates the
  process), it does not defer anything to a later microtask/event-loop turn.

### 5.4 Multithread / worker interaction

- **`finalizeBindings(instance[, { memory }])` is this module's entire
  multithread story**, and Node's own docs state its purpose directly:
  sharing one Wasm module's linear memory across multiple threads. This maps
  onto RTS's threading model (`docs/specs/rts-threading-model.md`) as
  follows: the `WebAssembly.Memory` backing a Wasm instance is (or can be)
  `SharedArrayBuffer`-backed — already a primordial, already the shared-heap
  primitive `Atomics` operates on (`architecture.md` §8) — so one thread runs
  `wasi.start(instance)`/`initialize(instance)` (which finalizes bindings and
  begins/sets up execution), while other worker threads that only need to
  observe/mutate the same linear memory call
  `wasi.finalizeBindings(instance, { memory: sharedMemory })` directly,
  without re-invoking `_start`/`_initialize`. This is a direct, natural fit
  for RTS's existing `SharedArrayBuffer` + `worker_threads` mapping — no new
  primitive is needed beyond what §8/`worker_threads.md` already establishes.
- **A `WasiEnvEntry`/`WasmInstanceEntry` pair is owned by whichever thread
  constructed it** by default (per-thread region, matching `sqlite.md`'s
  `owner_thread` field on `SqliteConnEntry`) — genuine cross-thread sharing of
  the *host bindings* (not just the raw memory bytes) requires the explicit
  `finalizeBindings()` call on the receiving thread, mirroring how
  `SqliteConnEntry`/`SqliteStmtEntry` are documented as single-owning-thread
  by RTS-level policy even though the underlying native library is
  technically thread-safe.
- **Preopens/fd tables are per-`WASI`-instance state**, not global — multiple
  `WASI` instances (even in the same thread) are fully independent
  environments, exactly matching Node's "one instance = one distinct
  environment" framing in §2.1.

### 5.5 Buffer / TypedArray interop

- **The Wasm linear memory, not an RTS `ArrayBuffer` handle, is where every
  byte of WASI syscall interop actually happens.** A `WebAssembly.Memory`
  is itself `ArrayBuffer`-backed at the JS-visible level (piece 1, §5.1), but
  the **native** WASI syscall trampolines (§5.2) read/write it through
  whatever raw-pointer API the embedded Wasm engine exposes (e.g.
  `wasmtime::Memory::data_ptr`/`data_mut`), not through RTS's own
  `arraybuffer_data_ptr`/`arraybuffer_byte_len` engine functions — those exist
  for RTS-native `ArrayBuffer`s; the Wasm engine's memory is a **different**
  (if conceptually parallel) raw byte region owned by whichever Wasm-engine
  crate is chosen. Whether RTS should unify these into a single
  `ArrayBuffer`-handle representation (so `WebAssembly.Memory.buffer` really
  is a first-class RTS `ArrayBuffer`, letting `node:buffer`/`TypedArray` code
  read Wasm memory directly with zero glue) or keep them as two distinct
  memory universes bridged only at syscall trampoline sites is an
  implementation-time question for whoever lands piece 1 — flagged, not
  resolved, here.
- **String/path marshalling**: WASI syscalls like `path_open` pass a
  `(ptr, len)` pair into linear memory for the path string; the trampoline
  copies those bytes into a Rust `String` (UTF-8, same discipline
  `buffer.md` §5.1 documents for RTS's own string codecs) **before** calling
  `rts-node`'s existing `fs` externs — no new string-codec logic, pure reuse.
- **iovecs** (`fd_write`/`fd_read`'s `ciovec_array`/`iovec_array`) are arrays
  of `(ptr, len)` pairs, also read directly out of linear memory by the
  trampoline, gathered/scattered into a single buffer, then handed to the
  existing stream-write/read primitive one call at a time (or coalesced) —
  same pattern, no new abstraction.

### 5.6 Doctrine placement

- **Non-primordial, per the native-syntax dividing line.** `new WASI(...)`,
  `wasi.start(...)`, `wasi.getImportObject()` are all ordinary calls — no
  literal syntax — so `WASI` itself is unambiguously Registry/`.ts`-shim
  surface, not an engine primordial. The engine front-end must never
  hardcode `"WASI"`, `"wasi_snapshot_preview1"`, `"wasi_unstable"`, or any
  syscall name (`"fd_write"`, …) — resolution flows entirely through
  `rts-node`'s own `NodespaceSpec`/`NODE_SPECS` data table, identically to
  every other `node:*` module (`architecture.md` §4).
- **The harder doctrine question sits one layer down, on `WebAssembly`
  itself** (piece 1, §5.1) — not on `node:wasi`. `WebAssembly.Memory`/
  `Instance`/`Module` arguably "define or intercept what a value is" (a
  foreign linear-memory region, a foreign call boundary) in the sense the
  primordial rule of thumb cares about, the same way `ArrayBuffer`/
  `SharedArrayBuffer`/`Atomics` already do — **but** `WebAssembly` has no
  native literal syntax of its own (no `wasm\`...\`` template form, unlike
  regex's `/re/`), so the "dividing line is native syntax" rule places it on
  the non-primordial side despite the value-model-adjacent flavor of what it
  does. This tension is exactly the kind of case `CLAUDE.md`'s
  ANTI-HARDCODE section anticipates resolving via **shape/data, not a name**
  — but resolving it requires an actual design pass on the `WebAssembly`
  global that is out of scope for a `node:wasi`-specific spec. Flagged for
  owner sign-off in §7, not decided here.
- **Where the `.ts` lives**: `crates/rts-node/src/wasi/*.ts` (Node-specific
  surface, not JS/TS-universal — same placement rule `buffer.md` §5.6 and
  `sqlite.md`/`vm.md` already establish for their own modules). The `.ts`
  shim owns: constructing/validating `WASIOptions`, normalizing `preopens`
  path strings, building the `getImportObject()` wrapper object, and
  presenting `wasiImport`'s entries as ordinary JS functions backed by the
  native externs in §5.2.

### 5.7 Shared-infra dependencies (FLAG)

- **A Wasm execution engine (hard, blocking dependency).** Nothing in this
  module can run a real `.wasm` binary without one. Recommended:
  embed `wasmtime` (or `wasmer`) in `rts-node`, used **only** for
  compiling/running Wasm bytecode and exposing its linear memory — not for
  its own bundled WASI implementation (per §5.1, RTS re-implements the WASI
  host functions itself against `rts-node`'s own `fs`/`os`/`crypto`
  primitives, for consistent sandboxing/error-code/platform-normalization
  behavior with the rest of `node:*`). This is a genuinely new, large Cargo
  dependency and (if the "RTS-native interpreter" alternative is chosen
  instead) potentially a genuinely new execution-engine **subsystem** — this
  spec does not treat it as "just another extern," and it should not be
  estimated as such.
- **The `WebAssembly` global itself** — `compile`/`instantiate`/`Module`/
  `Instance`/`Memory`/`Table` — is presupposed by every example in Node's own
  `node:wasi` docs (`WebAssembly.compile(...)`, `WebAssembly.instantiate(wasm,
  wasi.getImportObject())`) but is **not part of `node:wasi`'s own spec** and
  has no owning crate/spec in this repository today. This must be designed
  and implemented (by whatever owner decision resolves the doctrine question
  in §5.6) **before** `node:wasi` can do anything beyond constructing a
  `WASI` object and reading its `wasiImport` table in isolation.
- **`rts-node`'s own `fs`/`os`/`process`/`crypto` native primitives** — this
  module reuses them wholesale for the WASI syscall trampolines (§5.1/§5.2);
  it adds no new filesystem/clock/random primitive of its own, only a
  WASI-shaped adapter layer. No new duplication-vs-shared-crate tradeoff
  applies here beyond what those modules' own docs already settle.
- **`worker_threads` / `SharedArrayBuffer`** — needed for `finalizeBindings()`'s
  documented multithread use case (§5.4) to be meaningful; the single-thread
  `start()`/`initialize()` path does not depend on it.
- **tokio / shared async runtime: not required.** Every member of this
  module is synchronous (§5.3) — listed here only to explicitly rule it out,
  per the pattern established in `buffer.md`/`sqlite.md`.
- **TLS/rustls, net sockets** — not used directly by this module; the
  `sock_*` WASI syscalls are a possible future exception (§7), not part of
  the initial scope.

### 5.8 Implementation phases

1. **(a) Owner decision + foundation: pick and embed a Wasm engine**
   (recommended: `wasmtime`) and land a minimal `WebAssembly` global
   (`compile`/`instantiate`/`Module`/`Instance`/`Memory`) sufficient to run a
   trivial, **import-free** `.wasm` command module end to end. This precedes
   everything else in this module and is a `node:wasi`-specific foundation
   prerequisite, layered on top of the general P-1 foundation work
   `architecture.md` §12 already lists (async-infra hoist, `Entry::Backend`,
   `node:` re-routing).
2. **(b)** Implement `new WASI(options)` construction + full `WASIOptions`
   normalization/validation (`args`/`env`/`preopens`/fd numbers/
   `returnOnExit`/`version`), with `wasi.wasiImport` initially populated by
   syscall stubs that all return a "not implemented" WASI errno — proves the
   object-shape/plumbing end to end before any real syscall exists.
3. **(c)** Implement `wasi.getImportObject()` (both `version` branches) and
   wire a `WASI` instance's `wasiImport` into `WebAssembly.instantiate` from
   step (a).
4. **(d)** Implement the syscalls with no filesystem/fd dependency:
   `args_get`/`args_sizes_get`, `environ_get`/`environ_sizes_get`,
   `clock_time_get`, `random_get`, `proc_exit`, `sched_yield`.
5. **(e)** Implement `fd_write`/`fd_read` for the three standard streams
   (stdin/stdout/stderr only) — this is enough to run the canonical
   "hello world" WASI example from Node's own docs end to end.
6. **(f)** Implement `wasi.start(instance)`/`wasi.initialize(instance)`'s
   full validation contract (`_start` xor `_initialize`, `memory` export
   required, call-once enforcement, `returnOnExit` branch).
7. **(g)** Implement `preopens` + `path_open` + the `fd_*`/`path_*`
   filesystem syscall family, routed through `rts-node`'s existing `fs`
   externs with preopen-relative path resolution.
8. **(h)** Implement `wasi.finalizeBindings(instance[, options])` and its
   shared-memory multithread story, once `worker_threads`/`SharedArrayBuffer`
   sharing (§8) is available to test against.
9. **(i)** Round out the remaining WASI preview1 syscalls (`poll_oneoff`,
   `fd_fdstat_*`, `fd_seek`/`fd_tell`, `path_rename`/`path_unlink_file`/
   `path_create_directory`/`path_remove_directory`, `fd_filestat_get`/
   `path_filestat_get`, …) to full preview1 coverage.
10. **(j)** `sock_*` syscalls — implement only if a concrete use case
    emerges; otherwise document as intentionally `ENOSYS`-equivalent,
    matching most real-world WASI runtimes (§7).
11. **(k)** `version: 'unstable'` variant — low priority; the only documented
    difference is the `getImportObject()` namespace string, so this is
    nearly free once `preview1` is complete.

---

## 6. Test plan

`tests/node/wasi/*.test.ts` (`rts:test` format; every test that runs a real
`.wasm` module needs a companion `.wasm`/`.wat` fixture file, compiled ahead
of time — e.g. via `wat2wasm`, matching Node's own docs example):

- **Constructor validation**: `new WASI({ version: 'preview1' })` succeeds
  with all-default `args`/`env`/`stdin`/`stdout`/`stderr`/`returnOnExit`;
  omitting `version` throws; `version: 'bogus'` throws; explicit `args`/`env`/
  `preopens`/fd overrides are reflected correctly once read back through the
  syscalls in step (d)/(e) below.
- **`getImportObject()` namespace selection**: `version: 'preview1'` →
  result has a `wasi_snapshot_preview1` key only; `version: 'unstable'` →
  `wasi_unstable` key only; both wrap the same `wasi.wasiImport` reference
  (`result.wasi_snapshot_preview1 === wasi.wasiImport`, or the `unstable`
  equivalent).
- **Hello-world end to end**: compile+instantiate the exact `fd_write`
  WAT example from Node's own docs (`(module ... (func $main (export
  "_start") ...))`), call `wasi.start(instance)`, assert captured stdout is
  exactly `"hello world\n"`.
- **Command/reactor validation**: a module exporting only `_start` → `start()`
  succeeds, `initialize()` throws; a module exporting only `_initialize` →
  `initialize()` succeeds, `start()` throws; a (contrived, invalid-by-spec)
  module exporting **both** → both `start()` and `initialize()` throw.
- **Missing `memory` export**: a module with `_start` but no `memory` export
  → `start()` throws.
- **Call-once enforcement**: calling `start()` twice on the same `WASI`
  instance throws on the second call; same for `initialize()`; same for
  `finalizeBindings()` called twice directly; `finalizeBindings()` called
  directly once, then `start()` called (which internally would otherwise
  call it again) does **not** throw — `start()` detects bindings are already
  finalized and skips re-finalizing.
- **`returnOnExit: true` (default)**: a module that calls
  `proc_exit(42)` from `_start` → `wasi.start(instance)` **returns** `42`;
  the RTS process itself keeps running afterward (assert subsequent test
  code in the same file still executes).
- **`returnOnExit: false`**: same module, `returnOnExit: false` → the whole
  process exits with code `42` — test this via a spawned child RTS process
  (`node:child_process`/`process.spawn`-equivalent) and assert its exit code,
  not in-process (an in-process exit would kill the test runner).
- **`args`/`env` round-trip**: a `.wasm` fixture that reads `args_get`/
  `environ_get` and writes their contents to stdout; construct `WASI` with
  known `args: ['prog', 'a', 'b']`/`env: { FOO: 'bar' }`, assert the captured
  output matches exactly.
- **`preopens` + filesystem round-trip**: a `.wasm` fixture that
  `path_open`s a preopened directory, writes a file, then reads it back;
  construct `WASI` with `preopens: { '/sandbox': <a temp host dir> }`, run
  the fixture, then assert (from the **host** side, via `node:fs`) that the
  file actually exists at the real temp path with the expected bytes —
  proves the preopen→host-path translation is correct end to end, not just
  self-consistent inside the Wasm sandbox.
- **`path_open` outside any preopen**: a fixture that attempts to open a
  path with no matching preopen prefix → the syscall returns a WASI
  permission-denied-shaped errno (verify exact code), not a crash, and no
  host-filesystem access actually occurs.
- **`random_get`/`clock_time_get` sanity**: a fixture that calls each and
  writes the raw bytes/value to stdout; assert the returned byte length/
  monotonicity properties (not exact values — these are inherently
  non-deterministic/environment-dependent).
- **`finalizeBindings()` standalone**: call it directly (no `start`/
  `initialize`) on a freshly instantiated module and assert **no** module
  code has run yet (e.g. a global side-effect the module's `_start` would
  otherwise perform has not happened); then call `start()` afterward and
  confirm it does not re-finalize (see call-once test above) and proceeds
  to actually run `_start`.
- **Multithread (once `worker_threads` lands)**: instantiate a `.wasm`
  module backed by a `SharedArrayBuffer`-based `WebAssembly.Memory` on the
  main thread; spawn a worker that calls
  `wasi.finalizeBindings(instance, { memory: sharedMemory })` and writes
  through a WASI syscall from the worker; assert the main thread observes
  the write in the same linear memory (proves shared-memory wiring, not
  merely that each side has an independent copy).

---

## 7. Open questions / deferrals

- **Wasm engine choice** (`wasmtime` vs `wasmer` vs an RTS-native
  interpreter/compiler) needs explicit owner sign-off before any
  implementation work starts — this is the single highest-leverage decision
  in this entire spec, since every other section depends on it existing.
  `wasmtime` is recommended for maturity and an actively maintained Rust API
  surface, used only for bytecode execution + linear memory (not its bundled
  WASI implementation — see §5.1/§5.7).
- **Where the `WebAssembly` global itself is designed and owned** — a new
  shared spec/crate, folded into `rts-node`, or a new engine-adjacent
  capability? This module cannot be implemented at all until that question
  has an answer, since `wasi.start(instance)`'s sole argument **is** a
  `WebAssembly.Instance`. Recommend a short, dedicated design pass (its own
  doc, analogous to how `docs/specs/rts-threading-model.md` got its own doc
  before `worker_threads.md` could be written against it) rather than
  deciding it inline inside this `node:wasi` spec.
- **Strong deferral candidate.** Given (a) the P2/experimental tier, (b) the
  hard, large, net-new Wasm-engine dependency this module requires before
  *anything* in it works (unlike every other P2 module, which needs at most
  new ABI plumbing over capability RTS already has), (c) Node itself still
  carrying this as Stability 1 - Experimental more than six years after
  introduction (v13.3.0, 2019) with no sign of promotion, and (d) no known
  current RTS program needing WASI, **this spec explicitly recommends
  deferring `node:wasi` past every other P0/P1/P2 module** — the honest
  read is that the ROI of implementing an entire second execution engine to
  support one Stability-1 Node module is low relative to finishing the P0/P1
  surface first. State this explicitly in any planning doc that schedules
  this module, per the project's honesty-over-inflated-scope discipline.
- **`sock_*` WASI syscalls** (`sock_accept`/`sock_recv`/`sock_send`/
  `sock_shutdown`) — preview1 defines placeholders, but most real-world WASI
  runtimes (reportedly including Node's own) leave these effectively
  unimplemented. Confirm RTS should match that stance (document as
  `ENOSYS`-equivalent) rather than attempt real socket passthrough, unless a
  concrete use case surfaces.
- **Exact WASI errno taxonomy** or Node-error-code mapping for syscall
  failures (e.g. what a `path_open` outside any preopen actually returns)
  was not exhaustively confirmed against Node's own C++ binding source in
  this pass — needs a differential check against `libc`/WASI's own
  `__wasi_errno_t` enum and, ideally, real Node's behavior, at
  implementation time.
- **Whether `WebAssembly.Memory` should unify with RTS's own `ArrayBuffer`
  handle representation** (§5.5) or remain a separate memory universe
  bridged only at WASI syscall trampoline sites — affects whether
  `node:buffer`/`TypedArray` code can read Wasm linear memory with zero glue;
  left as an implementation-time design choice for whoever lands piece 1.
- **`node:wasi`'s interaction with RTS's own AOT (`rts compile`) path** is
  unexplored here — real Node's WASI/`WebAssembly` story is JIT-only (V8);
  whether an RTS AOT binary can embed/link a compiled `.wasm` module's
  machine code produced by the chosen Wasm engine (or must always
  JIT-compile the `.wasm` at RTS-process startup even in an AOT build) is an
  open question for whoever lands piece 1, not resolved by this spec.
