# RTS Node.js Implementation — Layer Placement Doctrine

> Where each piece of logic lives. Read after [`architecture.md`](./architecture.md).
> This refines the "rts-node duplicates everything" stance: duplicate only
> **backend-specific** (OS/syscall) logic; **share cross-context** logic through
> `rts-primitives`; **globals belong to the engine**.

Owner decisions this doc encodes (2026-07-09):

1. **Missing engine capabilities are implemented in the engine** — following the
   RTS polymorphism ideals (PolyValue / Repr lattice / shapes + ICs;
   prove-monomorphic-or-fall-to-tagged), never a front-end special-case hack.
2. **Globals belong to the engine.** `rts-node` mirrors only the `node:` *module*
   API. The ambient global surface (`globalThis`, `Buffer`, `URL`, `fetch`,
   `console`, `process`, `structuredClone`, …) is **engine-surfaced**; rts-node
   does not re-own or re-implement it.
3. **`rts-primitives` = cross-context logic** — an API that is correct in **both**
   the browser (`rts-shared` ↔ future `rts-browser`) **and** the backend
   (`rts-std` / `rts-node`). Native logic implemented in rts-node that turns out
   cross-context is **promoted** to `rts-primitives`.
4. **Analyze placement per concern and organize** — the matrix in §6 does this for
   every Node module.

---

## 1. The layers (bottom-up)

```
rts-engine        THE MOTOR. Value model (PolyValue/Repr/shapes/ICs), primordials,
                  JIT + eval/context, GC + HandleTable, and the GLOBAL surface
                  registration. Globals are surfaced here; missing capabilities are
                  added here (polymorphism-correct). Async-free by doctrine.
  │
  ├─ rts-primitives   PRIMORDIAL classes + CROSS-CONTEXT shared logic. Depends only
  │   │               on rts-engine → wasm-safe → usable by browser AND backend.
  │   │               (path, querystring, url algorithm, util.format/inspect/types,
  │   │                base64/hex/utf-8 codecs, EventEmitter, assert comparison,
  │   │                punycode, string_decoder, hash algorithms, WHATWG-stream
  │   │                state machines, compression algorithms.)
  │   │
  │   ├─ rts-shared    Browser-capable non-primordial universal (→ future rts-browser).
  │   │                Web-standard surface that isn't a pure primitive.
  │   │
  │   ├─ rts-std       BACKEND, RTS-native UNIQUE: audio, asio_audio, future UI, and
  │   │                the backend side of Web-global infra (io, http_server). NO Node
  │   │                modules (fs/os/net/… moved out — see rts-std-migration.md).
  │   │
  │   └─ rts-node      BACKEND, Node module API. Independent of rts-std. Depends on
  │                    rts-engine + rts-primitives. Owns backend-specific (OS/syscall)
  │                    logic; reuses rts-primitives for cross-context logic; gets
  │                    async from the engine; references engine globals; never rts-std.
  │
  └─ rts-runtime   Facade (pub use of all of the above); AOT staticlib superset.

  NB: the ASYNC PRIMITIVE (event loop, promise settle, microtask queue, timers)
  lives IN rts-engine (owner decision — async is primitive); the native tokio
  driver is behind a `native-async` feature so the engine still builds for wasm.
  Both rts-std and rts-node consume async from the engine — no separate crate.
```

**The browser/backend axis** is the discriminator for the primitive idea: a
"primitive" is logic that would run **unchanged in a browser** (`rts-browser`,
future) and on the **backend** (`rts-std`/`rts-node`). If it needs the OS, it is
backend-only. If it is a global, it is the engine's.

`rts-node`'s dependency set is `rts-engine + rts-primitives` (async included in the
engine) — **never `rts-std`**. This keeps the independence rule from
`architecture.md` §2 while
allowing cross-context logic to be shared (via `rts-primitives`, which is not
`rts-std`) instead of duplicated.

---

## 2. The placement decision tree

For any piece of Node functionality, ask in order:

1. **Is it a global** (reachable with no `import`, part of `globalThis`)?
   → **engine-surfaced.** Its implementation is sourced from the layer that owns
   the *type* (a primordial → engine/primitives; a Web class → shared; a backend
   singleton like `process` → node/std, but the *global binding* is the engine's).
   rts-node does **not** re-implement it; `node:<module>` re-exports the same
   engine object where Node also exposes a module form.
2. **Could it run unchanged in a browser** (pure algorithm, no OS, no native
   crate)? → **`rts-primitives`** (or `rts-shared` if it's non-primordial
   Web-standard but not a pure primitive).
3. **Does it need the OS / a native crate / a syscall** (files, sockets,
   processes, real ciphers, terminals)? → **backend**: `rts-node` (Node API) or
   `rts-std` (RTS-native unique).
4. **Is it a capability the engine lacks** (a new coercion, a value-model hook, a
   context primitive)? → **implement it in `rts-engine`**, polymorphism-correct
   (design-doc PolyValue/Repr/shapes), then consume it from the layer above.

**Promotion rule:** if a function first written in `rts-node` is later needed by
the browser side too, **move it down to `rts-primitives`** (or `rts-shared`) —
do not copy it. Backend-specific wrappers stay in `rts-node`; the shared core
lives once.

---

## 3. Globals belong to the engine (detail)

Node injects ~60 identifiers into every realm (`globalThis`, `Buffer`, `URL`,
`fetch`, `console`, `process`, `structuredClone`, `queueMicrotask`, the
`setTimeout` family, `AbortController`, `TextEncoder`/`Decoder`, the WHATWG stream
classes, `crypto`, `performance`, …). See [`globals.md`](./globals.md) §2.0 for the
full classification.

Ownership:

- **Primordials** (`Object`/`Array`/`Function`/`Promise`/`Symbol`/`BigInt`/
  `TypedArray`s/`Proxy`/`Reflect`/`Error`…) — the **engine**, already.
- **`Buffer`** — a primitive (`Uint8Array` subclass) in `rts-primitives`; the
  engine surfaces it as a global.
- **`URL`/`URLSearchParams`, `TextEncoder`/`Decoder`, `Blob`/`File`, `Event`/
  `EventTarget`, WHATWG streams, `AbortController`, `structuredClone`,
  `queueMicrotask`** — Web-standard classes: the algorithm is a primitive/shared,
  the **global binding** is the engine's.
- **`fetch`/`Request`/`Response`/`Headers`/`FormData`** — Web-standard; header/URL
  parsing is primitive/shared, the network transport is backend (`rts-node` /
  `rts-std`), the global binding is the engine's.
- **`console`, `process`, `performance`** — backend-backed singletons; the global
  binding and the shape are the engine's, the backend implementation is
  `rts-std`/`rts-node`.
- **`setTimeout`/`setInterval`/`setImmediate` + clears** — the engine surfaces the
  globals; the primitive lives in `rts-engine` (async primitive).

`rts-node` therefore adds **no new globals**. Where Node exposes a global *and* a
`node:` module form of the same object (`node:buffer`→`Buffer`,
`node:console`→`console`, `node:timers`→`setTimeout`, `node:events`→`EventTarget`,
`node:perf_hooks`→`performance`, `node:crypto`→`crypto`), the module spec
documents the module surface but the object is the engine's — the module just
re-exports it.

---

## 4. Engine gaps — fill them in the engine

When a Node module needs something the motor does not yet provide (a new eval
context primitive for `vm`, a context-frame stack for `async_hooks`, structured
serialize for `v8.serialize`/`structuredClone`, a Wasm engine for `wasi`), the fix
is to **add the capability to `rts-engine`** (or the appropriate shared layer),
**following the RTS ideals**:

- Values stay in the PolyValue / Repr model; prove monomorphic and unbox where the
  type system can, fall to one honest tagged representation where it can't.
- No front-end name-hardcode or AST-shape guessing; new capability = new
  metadata/primitive resolved through the generic paths.
- The engine still names only primordials; the new capability is a primitive or a
  Registry-resolved entry, not a hardcoded non-primordial.

This is explicitly allowed and expected — the engine is meant to grow to cover the
semantics Node needs, the *right way*, rather than being worked around in
`rts-node`.

---

## 5. Consequences for the crate moves

- `path`, `querystring`, `punycode`, `string_decoder`, `url` algorithm,
  `util.format`/`inspect`/`types`, base64/hex/utf-8 codecs, `EventEmitter`,
  `assert` comparison, WHATWG-stream state machines, compression/hash algorithms
  → **`rts-primitives`** (some are already in `rts-shared`; reclassify pure ones
  down to primitives). `node:path`/`node:querystring`/… are thin `.ts`
  re-exports over the primitive.
- `fs`, `net`, `dgram`, `tls`, `child_process`, `os`, `process` (backend),
  `dns`, `http`/`http2`/`https` transport, `readline`, `tty`, `wasi`, `sqlite`,
  real `crypto` ciphers/keys, `zlib` streams → **`rts-node`** (backend-specific).
- `v8`, `vm`, `inspector`, the module resolver, `worker_threads` threading, the
  context stack for `async_hooks`, `structuredClone`/serialize → **`rts-engine`**
  (or the `rts-engine` async primitive for the runtime parts).
- Globals → **engine-surfaced**, sourced per §3.

This means the earlier "duplicate fs/net/os in rts-node" holds only for the
**backend-specific** parts; the **pure** parts of those modules (e.g. `fs`
constants, path handling inside `fs`, header parsing inside `http`) reuse the
primitives rather than being duplicated.

---

## 6. Placement matrix (per module)

Layer codes: **E** engine · **PR** rts-primitives (cross-context) · **SH**
rts-shared / rts-browser · **AS** async primitive (in rts-engine) · **ST** rts-std (backend native) ·
**ND** rts-node (backend Node API). "Global?" = does it contribute an engine
global.

| Module | Primary layer(s) | Global? | Placement notes |
|---|---|---|---|
| assert | **PR** | — | Pure value comparison; diff rendering reuses `util.inspect` (PR). |
| async_hooks | **E** + AS + ND | — | Context-frame stack is an **engine** primitive (GC-rooted); AsyncLocalStorage wraps it; hook wiring around promise/timer settle in AS. |
| buffer | **PR** (+ E global) | `Buffer` | `Buffer` = `Uint8Array` subclass + codecs in PR; engine surfaces the global. `Blob`/`File` Web classes (SH). |
| child_process | **ND** | — | OS processes; backend only. |
| cluster | **ND** | — | Layers over child_process + net; backend. |
| console | **E** global + PR + ST/SH | `console` | Engine surfaces `console`; formatting = PR; output via ST `io` (backend) / browser console (SH). |
| crypto | **PR/SH** + ND | `crypto` | WebCrypto algorithms (hash/hmac/subtle/digest) = PR/SH (cross-ctx); ciphers/KeyObject/X509/OS-CSPRNG = ND. Engine surfaces the `crypto` global. |
| dgram | **ND** | — | UDP sockets; backend. |
| diagnostics_channel | **PR** (exposed by ND) | — | Pure publish/subscribe; cross-context. |
| dns | **ND** | — | Resolver / OS + network; backend. |
| domain | **ND** | — | Deprecated; error-domain over async infra; backend. |
| events | **PR** | (`EventTarget` global = E) | `EventEmitter` pure = PR. `EventTarget`/`Event`/`CustomEvent` are engine globals. |
| fs | **ND** (+ PR for constants/path bits) | — | Syscalls; backend. Pure sub-bits (constants, path joins) reuse PR. |
| http | **ND** + PR/SH | — | Transport/sockets = ND; header/URL/message parsing = PR/SH; `Headers` is an engine global. |
| http2 | **ND** | — | Backend; HPACK codec could be PR. |
| https | **ND** | — | http + tls; backend. |
| inspector | **E** / ND | — | RTS-engine debug surface; CDP compatibility a phased engine goal (no V8). |
| module | **E** + ND | (`require`/`module`/`exports` = E-injected) | Resolver/loader = engine; CJS `require` backend glue = ND. |
| net | **ND** | — | TCP sockets; backend. |
| os | **ND** | — | OS info; backend. |
| path | **PR** | — | Pure path algebra; cross-context. (Move from rts-shared to PR.) |
| perf_hooks | **E** global + PR + AS | `performance` | Engine surfaces `performance`; marks/measures = PR; `eventLoopUtilization` = AS. |
| process | **ND** (+ E global + AS) | `process` | Backend info/spawn = ND; `process` global binding = E; `nextTick`/`hrtime` = AS/E. |
| punycode | **PR** | — | Pure; deprecated; cross-context. |
| querystring | **PR** | — | Pure; cross-context. |
| readline | **ND** | — | tty/stream backend. |
| repl | **E** / ND | — | Uses the engine eval; backend I/O glue. |
| stream | **PR/SH** + ND | (WHATWG stream globals = E) | WHATWG stream state machines = PR/SH (cross-ctx); fs/net-backed Node streams = ND. |
| string_decoder | **PR** | — | Pure UTF-8 incremental decode; cross-context. |
| test | **PR/SH** + ND | — | Runner logic cross-context (PR/SH); fs/tty reporters = ND. |
| timers | **E** globals + AS | `setTimeout`… | Engine surfaces the global family; primitive lives in AS; `node:timers` mirrors. |
| tls | **ND** | — | rustls over TCP; backend. |
| trace_events | **E** / ND | — | Tracing = engine hook; category management backend glue. |
| tty | **ND** | — | Terminal; backend. |
| url | **PR** (+ E globals) | `URL`,`URLSearchParams` | Parsing algorithm = PR (cross-ctx); `fileURLToPath`/`pathToFileURL` = SH+ND; engine surfaces the globals. |
| util | **PR/SH** + ND | — | `format`/`inspect`/`types`/`promisify`/`parseArgs`/`styleText` = PR/SH; bits needing `process` = ND. |
| v8 | **E** | — | RTS engine introspection: heap stats (collector), serialize (structuredClone infra), snapshots. No V8. |
| vm | **E** | — | RTS JIT + own eval/context. No V8. |
| wasi | **ND** + E | — | Needs a Wasm engine (engine or rts-node-embedded); WASI syscalls map to ND fs/clock/random. Experimental. |
| worker_threads | **E/AS** + ND | (`MessageChannel`… = E) | Threading model = engine/AS; `Worker` spawn/bootstrap glue = ND; message ports = engine channels. |
| zlib | **PR/SH-capable** + ND | (`CompressionStream` = E) | Compression algorithms are browser-capable (PR/SH); Node zlib stream wrappers = ND. |
| sqlite | **ND** | — | Native SQLite; backend; experimental. |
| globals | **E** | (all) | The engine owns the global surface; impls sourced per §3. |

---

## 7. Applying this in the module docs

Each `<module>.md` §5.1 (native impl mapping) should defer to this matrix: state
which parts are PR (shared/promotable), which are ND (backend-specific), which are
E (engine capability), and whether the module contributes/consumes an engine
global. When a module's §5 currently implies "rts-node reimplements X" for
something that is cross-context, that logic belongs in `rts-primitives` per §2
step 2 — the module doc's plan should say "reuse/add to rts-primitives", not
"duplicate in rts-node".

## 8. Open questions (tie to architecture.md §13)

- Exact split line between `rts-primitives` (pure primitive) and `rts-shared`
  (Web-standard non-primitive) for borderline cases (URL, streams, crypto algos).
  Rule of thumb: has a native literal / is a fundamental value → primitive;
  Web-standard class with no literal → shared.
- ~~Whether `rts-async` is a distinct crate or folded into `rts-engine`~~ —
  RESOLVED (owner): async is a primitive → **in `rts-engine`**, native tokio driver
  behind a `native-async` feature (so the engine still builds for wasm/browser).
- Whether `worker_threads` message ports reuse the engine channel primitive
  directly or a thin engine async wrapper.
