# RTS Standard Surface — `rts:*` surface redesign (v1)

> **Status: DIRECTION-APPROVED PROPOSAL** (hard cutover; per-module
> `rts:<ns>` imports; bytes absorbed by TypedArrays; execution in phases
> after this map is approved). Owner decisions of 2026-07-05. This doc is
> the canonical member-by-member map; execution follows the phases at the
> end.
>
> Thesis: **RTS exports the Rust std into the JS/TS environment** — a rich,
> unrestricted environment — and removes everything that duplicates what
> the JS language already provides. The surface has three rings: GLOBALS
> (JS/Web spec, zero imports), `rts:*` MODULES (the native platform, the
> differentiator), and INTERNAL (engine plumbing, invisible in `rts.d.ts`).

## Conventions (binding)

1. **camelCase for every member** (`read_file` → `readFile`). The rename is
   ONLY on the TS/Registry surface (`Member.name`/`ts_signature`); the
   `__RTS_FN_*` ABI/linker symbols do NOT change.
2. **Per-module imports**: `import { readFile } from "rts:fs"`. The single
   `"rts"` specifier dies (hard cutover — tests/fixtures rewritten in the
   same PR of each phase). Mirrors `node:*`; enables per-module AOT slicing.
3. **Zero JS duplication**: if the language covers it (Promise, Map, JSON,
   Date, Math, RegExp…), there is no namespace for it. The externs become
   internal symbols consumed by the engine/preludes, outside `rts.d.ts`.
4. **Bytes = `Uint8Array`/`ArrayBuffer`** (primordials). The `buffer`
   namespace dies; every native API that speaks bytes takes/returns a
   TypedArray.
5. **User code never allocates RAM manually.** GC + TypedArrays cover all
   normal use; raw memory exists only behind `rts:ffi`'s explicit `unsafe`
   sub-surface, for C interop.
6. No new snake_case member; CI lints the committed `rts.d.ts` against the
   generator.

---

## Ring 1 — GLOBALS (zero imports, JS/Web spec)

Already exist; they stay (only moving from "namespaces" to declared globals
in the d.ts): `console`, `JSON`, `JSON5`, `Math`, `Date`, `Promise`,
`RegExp`, `Map`/`Set`/`WeakMap`/`WeakSet`, `WeakRef`/`FinalizationRegistry`,
`Symbol`, `BigInt`, `Proxy`/`Reflect`, `Error`+family, `fetch`/`Headers`/
`FormData`/`Request`/`Response`/`Blob`/`File`, `URL`/`URLSearchParams`,
`TextEncoder/Decoder(+Streams)`, `ReadableStream`/`WritableStream`/
`TransformStream`/`CompressionStream`, `Event`/`EventTarget`/
`AbortController`/`AbortSignal`, `MessageChannel`/`MessagePort`,
`setTimeout`/`setInterval`/`setImmediate`+clears, `queueMicrotask`,
`structuredClone`, `atob`/`btoa`, `performance`, `crypto` (see rts:crypto),
`ArrayBuffer`/`SharedArrayBuffer`/`DataView`/TypedArrays, `Atomics`, `Intl`,
`DOMException`, `EventEmitter` (node compat), `globalThis`.

---

## Ring 2 — `rts:*` MODULES (the Rust std)

### `rts:fs`
| New | Comes from |
|---|---|
| `readBytes(path): Uint8Array` | fs.read_all (Buffer→TypedArray) |
| `readText(path): string` | fs.read_text |
| `writeText(path, s)` / `writeBytes(path, b: Uint8Array)` | fs.write / fs.write_bytes |
| `appendText(path, s)` | fs.append |
| `exists`, `isFile`, `isDir` | exists / is_file / is_dir |
| `size(path): number` | size |
| `modifiedMs(path): number` | modified_ms |
| `createDir`, `createDirAll`, `removeDir`, `removeDirAll`, `removeFile`, `rename`, `copy` | same, snake |
| `readDir(path): string[]` | readdir |
| **NEW** `watch(path, cb)` | notify crate — file watching |
| **NEW** `mmap(path): Uint8Array` | zero-copy memory-mapped view |
| **NEW** `lock(path)` / `unlock(path)` | file locks |
| **COMPTIME** `includeBytes(path): Uint8Array` | embeds the file into the binary at BUILD time (build error if missing) |
| **COMPTIME** `includeString(path): string` | same, as string (name settled with the owner) |

Node-style aliases (`readFileSync` etc.) leave this module — Node compat
lives only in `node:fs` (rts-node).

### `rts:io`
`print`, `eprint`, `stdout.write/flush`, `stderr.write/flush`,
`stdin.read(): Uint8Array`, `stdin.readLine(): string`.

### `rts:net`
TCP: `tcpListen`, `tcpAccept`, `tcpConnect`, `tcpSend(h, b: Uint8Array)`,
`tcpRecv(h): Uint8Array`, `tcpLocalAddr`, `tcpClose`. UDP: `udpBind`,
`udpSendTo`, `udpRecvFrom`, `udpLastPeer`, `udpLocalAddr`, `udpClose`.
DNS: `resolve`. TLS (absorbs the `tls` ns): `tlsConnect`, `tlsSend`,
`tlsRecv`, `tlsClose`. **NEW**: `unixListen`/`unixConnect` (Unix domain
sockets; named pipes on Windows) — real IPC. **TENTATIVE**:
`serveTcp(addr, onConn)` — callback-style accept loop sugar (owner:
"maybe"; decide when implementing the module).

### `rts:http`
`serve(addr, handler)`, `request.method/path/body`, `respond` (absorbs
`http_server`, camelCase).

### `rts:process`
`exit`, `abort`, `pid`, `args(): string[]`, `cwd`, `setCwd`, `spawn`,
`wait`, `kill`, `env.get/set/remove` (absorbs the `env` ns). **NEW**:
`exec(cmd): Promise<{stdout, stderr, code}>` — one-shot await-able run;
`onSignal("SIGINT", cb)` — real signal handling.

### `rts:os`
`platform`, `arch`, `family`, `eol`, `homeDir`, `tempDir`, `configDir`,
`cacheDir`. **COMPTIME** versions (`target.os/arch/family` folded consts →
per-platform dead-code elimination) live in `rts:build`.

### `rts:path`
`join`, `parent`, `fileName`, `stem`, `ext`, `isAbsolute`, `normalize`,
`withExt`.

### `rts:time`
`nowMs`, `nowNs` (monotonic), `unixMs`, `unixNs`, `sleepMs`, `sleepNs`.

### `rts:thread`
Absorbs `thread` + `sync` + `atomic` + `parallel` into one cohesive
surface. The high-level triangle — **isolate / coordinate / communicate**:

- **`threadLocal(template: T): Threaded<T>`** — one instance PER thread.
  Fully TRANSPARENT: every property/method access forwards to the current
  thread's instance (`cache.set(k, v)` hits this thread's Map directly —
  no `.get().value`). The wrapper has NO methods of its own (avoids
  colliding with T's methods like `Map.get`); the escape hatch is the
  imported `raw(threaded)` helper. Argument semantics: the value is a
  TEMPLATE — each other thread receives a lazy `structuredClone` of it on
  first touch; non-clonable values (closures, IO handles) use the factory
  overload `threadLocal(() => T)`. Never contended → no locks. In the
  regional model this is the value that NEVER promotes (stays in the
  thread's region); it formalizes as an opt-in API what the GCELLS
  thread-local hack does today by accident.
  ```ts
  import { threadLocal, raw } from "rts:thread";
  const cache = threadLocal(new Map<string, number>());
  cache.set("k", 1);   // this thread's Map, direct use
  raw(cache);          // the raw per-thread instance
  ```
- **`shared(value: T): Shared<T>`** — ONE instance, promoted to the shared
  heap. Transparent like `Threaded`, but each individual method call is
  auto-synchronized; compound transactions use the imported
  `lock(shared, cb)` (holds the internal mutex across the callback).
  ```ts
  import { shared, lock } from "rts:thread";
  const registry = shared(new Map<string, number>());
  registry.set("k", 1);              // auto-synchronized
  lock(registry, (m) => {            // compound transaction
    if (!m.has("k")) m.set("k", m.size);
  });
  ```
- **`channel<T>(): [Sender<T>, Receiver<T>]`** — mpsc; `send(v)` promotes
  `v` (regional model).
- **`task(fn): Promise<T>`** — run on a pool thread, integrates with
  `await` (replaces manual spawn_async/join).
- `spawn(fn, arg?): Thread` (+ `join`/`detach`), `scope(cb)`, `sleepMs`,
  `id` — real threads.
- **`parallel(arr).map(fn)` / `.filter` / `.reduce`** — chainable rayon
  data-parallelism (replaces parallel_map/for_each/reduce), `numThreads`.
- **NEW** `Semaphore(n)`, `Barrier(n)` — classic missing primitives.
- Low-level layer (kept, for those who need it): `Mutex`, `RwLock`, `Once`,
  `AtomicI64`/`AtomicBool`/`AtomicF64` + `fence*` (the `Atomics` global
  covers SharedArrayBuffer; these cover standalone cells).

Engine-side implementation of `Threaded`/`Shared`: a new case on the
dispatch paths that `Proxy` already traverses (`proxy_parts`-like) —
obj_get/set/method on them resolves to the per-thread/shared slot before
normal dispatch. Zero cost on the non-exotic path. Execution/region model:
`docs/specs/rts-threading-model.md`.

### `rts:crypto`
`sha256(b: Uint8Array|string): Uint8Array`, one-shot
`hash("sha256", data)`, streaming `Hash` (`createHash("sha256")`),
`randomBytes(n): Uint8Array`, `randomUuid`, `hexEncode/Decode`,
`base64Encode/Decode`, sip: `hashStr`, `hashBytes`, `hashI64`,
`hashCombine` (absorbs the `hash` ns). The Web `crypto` global
(`getRandomValues`, `randomUUID`, `subtle.digest`) delegates here.

### `rts:decimal`
Renames `bigfloat`: `Decimal.from(x)`, `add/sub/mul/div/neg/sqrt`,
`toString`, `toNumber` — registry class, no manual `free` (GC).

### `rts:ffi`
The C-interop surface (absorbs `ffi` + `ptr` + `mem` + `alloc` + `hint`).
Design rule: **normal user code never touches raw memory** — the common
FFI case uses TypedArrays; raw is quarantined behind `unsafe`.

- `open(lib)`, `symbol`, CString/OsString helpers.
- **`pin(b: Uint8Array): Ptr`** — the pointer of an existing TypedArray
  (pinned for the duration of the call scope). The common path: GC
  allocates, C reads/writes.
- **`unsafe.*` sub-surface** (the name screams the contract):
  `unsafe.alloc/allocZeroed/realloc/dealloc`, `unsafe.Ptr`
  (`readI64/I32/U8/F64`, `write*`, `copy`, `offset`, `null`, `isNull`),
  `sizeOf`/`alignOf` consts.
- hints: `blackBox`, `spinLoop`, `unreachable`, `assertUnchecked`.
- **NEW — reverse-FFI exports, `native(fn)`** (name settled with the
  owner; decorators on `const` don't exist in TS, so the form is a
  comptime wrapper):
  ```ts
  import { native } from "rts:ffi";

  // C symbol = the const's name ("add") — no loose string in the common case
  export const add = native((a: i32, b: i32): i32 => a + b);

  // override when the C name differs
  export const soma = native("extend_c_example",
    (a: i32, b: f64): i32 => { ... });
  ```
  COMPTIME marker (shape-based, like `getPointer`): enforces a
  monomorphic ABI (i32/i64/f64/bool/ptr+len; `any` = compile error),
  declares the symbol with `Linkage::Export` on the ObjectModule. The
  export prologue registers the thread with the GC (`thread_registry`)
  and installs the error slot (JS panic → error code, never unwinding
  across C). New build target `rts compile --lib` → `.dll`/`.so`/`.a` +
  generated `.h` header from the signatures. Explicit `rts_init()` or
  lazy init on first call.
- **Ambient type aliases**: `i32`, `i64`, `u64`, `f64`, `f32`, `bool`
  (subtypes of `number` meaningful to the engine's checker) — required in
  `native()` signatures, globally available.

### `rts:runtime`
`eval`, `evalFile`, `importModule`, `gc.collect`, `gc.liveCount`,
**NEW** `memoryUsage()`, hot-reload. (Dev/advanced; `trace_*` becomes
crash-handler internal, outside the d.ts.)

### `rts:test`
Current framework (describe/test/expect via prelude) + internal
`test_core`.

### `rts:build` (**NEW — comptime**)
`includeBytes`/`includeString` (aliases of the rts:fs ones), `env(name)`
(BUILD-time env, constant), `target.os/arch/family` (folded consts),
`buildId()`, `version()`, `compileTimestamp()`. All resolved in the
front-end; they never exist at runtime.

### `rts:simd` (**NEW — later phase**)
Cranelift vectors exposed: `f64x2`, `i32x4`, `f32x4` + lanes/shuffle/fma.
Depends on HIR type design; gated behind its own doc.

### `rts:compress` (**NEW**)
`gzip`/`gunzip`/`deflate`/`inflate` over `Uint8Array` (impl already exists
internally in the CompressionStreams — just expose).

### `rts:tty` (**NEW**)
`isTTY(fd)`, `size(): {cols, rows}`, `setRawMode(bool)`, color detection —
essential for the AOT CLI audience.

### Domain (out of this doc's scope, own frozen plan)
`rts:dom`, `rts:render`, `rts:input`, `rts:egui`, `rts:audio` — follow
`docs/specs/html-engine/*`. They only inherit the conventions (camelCase
already ok).

---

## What DIES from the public surface (becomes internal symbols)

| Namespace | Replacement | Note |
|---|---|---|
| `promise` | `Promise` global | externs become engine plumbing |
| `collections` | `Map`/`Set`/`Array` | vec_*/map_* = internal representation |
| `json` | `JSON` global | |
| `date` | `Date` global | |
| `math`, `num`, `fmt`, `util` | `Math`/`Number` globals | checked/wrapping/bits → `rts:ffi` (machine numerics) |
| `string` | `String.prototype` | |
| `regex` | `RegExp` | |
| `events` | `EventEmitter` global / `node:events` | |
| `timers`, `text_encoding`, `performance`, `fetch`, `url`, `console`, `JSON5`, `globalThis` | already globals | registration changes from "ns" to global |
| `buffer` | `Uint8Array`/`ArrayBuffer`/`DataView` | decision: TypedArrays are THE bytes representation |
| `gc` | `rts:runtime.gc` | |
| `trace` | internal (crash handler) | |
| `env` | `rts:process.env` | |
| `tls` | `rts:net` | |
| `sync`, `atomic`, `parallel` | `rts:thread` | |
| `hash` | `rts:crypto` | |
| `alloc`, `mem`, `ptr`, `ffi`, `hint` | `rts:ffi` (raw under `unsafe`) | |
| `test_core` | internal to `rts:test` | |
| `engine` | internal (prelude bridges) | NEVER public |
| `bigfloat` | `rts:decimal` | |
| `asio_audio` | `rts:audio` (feature-gated) | |

---

## Primitive relocation (rts-primitives)

Rule (existing doctrine): **a primordial's impl lives in `rts-primitives`**
(pure Rust, wasm-safe, no tokio/io). Current leaks to fix:

1. `rts-std/src/collector/string_pool.rs` — typeof/toString/inspection of
   PRIMITIVES (String/Number/Boolean) mixed with the pool/GC. Split: the
   value logic (coercions, numeric formatting, the `engine` ns `str_*`
   string ops) → `rts-primitives`; the pool/handles stay in the collector.
2. `rts-shared/src/globals/symbol` → `rts-primitives` (Symbol is
   PRIMORDIAL since 2026-06-26; CLAUDE.md already mandates it; move it).
3. `rts-shared` BigInt/Proxy/Reflect (primordial since 2026-07-03) →
   `rts-primitives`.
4. String/Boolean/Number externs in `rts-primitives` declaring
   `__RTS_FN_NS_GC_STRING_NEW` with divergent signatures (current
   `clashing_extern_declarations` warnings) → ONE canonical declaration in
   each crate's `gc_surface.rs`, internal `pub use`.
5. Goal: the `String`/`Boolean`/`Number`/`Array` primordials running 100%
   in Rust inside `rts-primitives`, depending on nothing above
   `rts-engine`.

---

## Execution phases (each = green PR + suite)

- **F0 — Generator/Registry**: builder gains `module("rts:fs")` + an
  `internal` flag (outside the d.ts); the `rts.d.ts` generator emits
  `declare module "rts:fs"` per module; the resolver accepts `rts:<ns>`.
- **F1 — Hide internals**: mark `engine/trace/test_core/gc/collections/
  promise/json/date/math/num/fmt/util/string/regex/events` as internal;
  globals registered as globals. Nothing renamed yet; the whole suite must
  stay green (it only uses the old surface where it is public).
- **F2 — camelCase + new modules**: rename members (map above), regroup
  (`tls`→net, `sync/atomic/parallel`→thread, `hash`→crypto,
  `ffi+ptr+mem+alloc+hint`→ffi, `env`→process, `bigfloat`→decimal);
  **hard cutover**: tests/fixtures/examples rewritten in the same PR (per
  module, small PRs: F2a fs/io, F2b net/http, F2c thread, F2d ffi, …).
- **F3 — buffer→TypedArrays**: byte APIs migrate to Uint8Array; the
  buffer namespace dies.
- **F4 — Primitives**: rts-primitives relocation (map above).
- **F5 — Comptime**: `rts:build` + `includeBytes/includeString` + `env!`
  in the front-end.
- **F6 — New systems**: watch/mmap/locks, signals, tty, compress, unix
  sockets, channels, threadLocal/shared/lock, task, Semaphore/Barrier,
  exec, memoryUsage. Each gets its own issue.
- **F7 — `native()` + `rts compile --lib`** (detail doc if needed).
- **F8 — simd** (own doc first).

The honesty floor holds in every phase: real parity, green build, no
silent regression; cross-runtime fixtures using the old surface are
UPDATED (intentional change, documented per PR).
