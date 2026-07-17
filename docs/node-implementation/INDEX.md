# RTS Node.js Implementation — Index

Specs for implementing the **complete Node.js 25 API** natively in the RTS
`rts-node` crate. Every stable core module, the documented deprecated ones, and
the experimental ones are specced here. Parity target: Node.js **25.x**.

> **Status: implementation under way.** Each `<module>.md` is an implementation
> blueprint (full API surface + types + RTS impl plan) and carries its OWN
> `Status` row — that row, not this index, is the truth for a given module. Where
> a module has landed, a "what actually landed" section records the divergences
> from its plan. Live code is in `crates/rts-node/src/`.

## Read first

| Doc | What |
|---|---|
| [`architecture.md`](./architecture.md) | The design: `rts-node` independence, crate layout + the 2 foundation hoists, ABI, opaque handles, async model, threading, buffer interop, `.ts`-shim split, the **no-V8 rule**, globals=engine, tiers, open decisions. |
| [`layering.md`](./layering.md) | **Layer placement doctrine**: globals belong to the engine; `rts-primitives` = cross-context (browser+backend) logic; engine gaps fillable polymorphism-correct; the placement decision tree + a per-module matrix (which layer owns each concern). |
| [`implementation-plan.md`](./implementation-plan.md) | **Master plan**: current state (verified), duplicated-resource remove/relocate table, the P-1 foundation phases (Entry::Backend, async→engine + carve-outs, primitives promotions, codegen `node:` routing), P0→P2 module→crate→layer map, prioritization (mature-pure first), per-PR verification, risks. |
| [`crates.md`](./crates.md) | **Crate selection**: license policy (permissive + weak-copyleft, no royalty) + a consistent purity bar (reject C-compile/vendor, accept libc-FFI), vetted crates per capability domain, rejected list with reasons, and **pure-Rust TLS** (RustCrypto CryptoProvider, no `ring`). |
| [`rts-std-migration.md`](./rts-std-migration.md) | The rts-std surgery: exact remove/keep buckets, move mechanics, crypto split, async→engine, `Entry::Backend`, execution order, regression watch. |

## Core decisions (see architecture.md §2)

1. `rts-node` is **independent of `rts-std`** — own backend-specific native impls,
   no `rts-std` dep, no mirror. May depend on `rts-engine` + `rts-primitives`
   (async now lives in the engine). *Backend-specific* logic is duplicated;
   *cross-context* pure logic is **shared via `rts-primitives`**, not copied.
2. Node-duplicate modules are **removed from `rts-std`** (which shrinks to
   audio/asio + Web-global infra). Backend bodies move to `rts-node`.
3. **No V8** — `node:v8`/`vm`/`inspector` reproduce the API surface but back it
   with **RTS-engine** equivalents, for all callers. Never embed/emulate V8.
4. **Globals belong to the engine.** `rts-node` mirrors only the `node:` *module*
   API; it adds no globals. The global surface is engine-surfaced (see
   [`layering.md`](./layering.md) §3).
5. **`rts-primitives` = cross-context logic** (correct in browser *and* backend);
   the placement of every concern is in [`layering.md`](./layering.md) §6. Engine
   gaps are filled *in the engine*, polymorphism-correct.
6. Full Node 25 coverage, tiered P0/P1/P2 (tiers drive order, not scope).
7. **Async is a primitive → lives in `rts-engine`** (event loop / promise / microtask
   / timers; native tokio driver behind a `native-async` feature). No `rts-async`
   crate — dissolves the cyclic-dep risk.
8. **TLS is pure-Rust** — `rustls` with a RustCrypto `CryptoProvider` (the same
   crates node:crypto uses); **no `ring`/`aws-lc-rs` C** (crates.md §6).
9. **Prioritize mature pure-Rust first**; push immature-pure (node:sqlite via
   `turso_core`, node:wasi via `wasmi`, full TLS-provider hardening) to the **end**.

## Foundation prerequisites (P-1, before per-module impl)

- **Move the async primitive into `rts-engine`** (event loop, promise settle,
  microtask queue, timers; tokio driver behind a `native-async` feature). Owner
  decision — async is primitive. No separate `rts-async` crate; dissolves the
  cyclic-dep risk. *(Module specs written earlier may still say "rts-async" — read
  it as "the engine async primitive".)*
- **`Entry::Backend(Box<dyn Traceable>)`** in `rts-engine` for `rts-node` opaque
  handles (Stats/Server/Socket/FileHandle/Worker/…).
- **Re-point codegen `node:` routing** from the `rts:`-prefix-collapse onto
  node-owned namespaces; wire `rts-node` into the dep graph + JIT symbol harvest.

## Module specs (43)

Legend — **Tier**: P0 core-first · P1 · P2 later/experimental. **Stability** per
Node 25. All specs written ✅.

### P0 — core (17)

| Module | `node:` | Stability | Key surface | Spec |
|---|---|---|---|---|
| assert | `node:assert` | Stable | `assert`, `strict`, `AssertionError` | [assert.md](./assert.md) |
| buffer | `node:buffer` | Stable | `Buffer`, `Blob`, `File`, `atob`/`btoa` | [buffer.md](./buffer.md) |
| console | `node:console` | Stable | `Console`, `console` global | [console.md](./console.md) |
| events | `node:events` | Stable | `EventEmitter`, `once`/`on`, `EventTarget` interop | [events.md](./events.md) |
| fs | `node:fs` (+`/promises`) | Stable | `fs.*Sync`/cb/promise, `Stats`, `Dir`, `FileHandle`, watch | [fs.md](./fs.md) |
| globals | — (global object) | Stable | `Buffer`/`URL`/`fetch`/`structuredClone`/`AbortController`/… | [globals.md](./globals.md) |
| http | `node:http` | Stable | `Server`, `ClientRequest`, `IncomingMessage`, `Agent` | [http.md](./http.md) |
| os | `node:os` | Stable | `platform`/`arch`/`cpus`/`networkInterfaces`/`userInfo` | [os.md](./os.md) |
| path | `node:path` | Stable | `join`/`resolve`/`parse`, `posix`, `win32` | [path.md](./path.md) |
| process | `node:process` (+global) | Stable | `argv`/`env`/`nextTick`/`stdout`/signals/`hrtime` | [process.md](./process.md) |
| querystring | `node:querystring` | Stable (legacy) | `parse`/`stringify`/`escape`/`unescape` | [querystring.md](./querystring.md) |
| stream | `node:stream` (+`/web`,`/promises`,`/consumers`) | Stable | `Readable`/`Writable`/`Duplex`/`Transform`, WHATWG streams | [stream.md](./stream.md) |
| string_decoder | `node:string_decoder` | Stable | `StringDecoder` | [string_decoder.md](./string_decoder.md) |
| timers | `node:timers` (+`/promises`) | Stable | `setTimeout`/`setInterval`/`setImmediate` + promises | [timers.md](./timers.md) |
| url | `node:url` | Stable | `URL`/`URLSearchParams`, `fileURLToPath`, legacy `parse` | [url.md](./url.md) |
| util | `node:util` (+`/types`) | Stable | `promisify`/`inspect`/`format`/`parseArgs`/`styleText` | [util.md](./util.md) |
| worker_threads | `node:worker_threads` | Stable | `Worker`, `MessagePort`/`Channel`, `parentPort`, `workerData` | [worker_threads.md](./worker_threads.md) |

### P1 (15)

| Module | `node:` | Stability | Key surface | Spec |
|---|---|---|---|---|
| async_hooks | `node:async_hooks` | Mixed | `AsyncLocalStorage`, `AsyncResource`, `createHook` | [async_hooks.md](./async_hooks.md) |
| child_process | `node:child_process` | Stable | `spawn`/`exec`/`execFile`/`fork` (+Sync), `ChildProcess` | [child_process.md](./child_process.md) |
| cluster | `node:cluster` | Stable | `fork`, `Worker`, scheduling, IPC | [cluster.md](./cluster.md) |
| crypto | `node:crypto` (+webcrypto) | Stable | `Hash`/`Hmac`/`Cipheriv`/`Sign`, `randomBytes`, `subtle` | [crypto.md](./crypto.md) |
| dgram ✅ | `node:dgram` | Stable | `Socket` (UDP), `createSocket` | [dgram.md](./dgram.md) |
| dns | `node:dns` (+`/promises`) | Stable | `lookup`/`resolve*`, `Resolver` | [dns.md](./dns.md) |
| https | `node:https` | Stable | `Server`/`request`/`get`/`Agent` over TLS | [https.md](./https.md) |
| module | `node:module` | Stable | `createRequire`, loader hooks, `SourceMap`, `builtinModules` | [module.md](./module.md) |
| net ✅ | `node:net` | Stable | `Server`, `Socket` (TCP), `BlockList`, `SocketAddress`, `connect` | [net.md](./net.md) |
| perf_hooks | `node:perf_hooks` | Stable | `performance`, `PerformanceObserver`, histograms | [perf_hooks.md](./perf_hooks.md) |
| readline | `node:readline` (+`/promises`) | Stable | `Interface`, `createInterface`, keypress | [readline.md](./readline.md) |
| test | `node:test` | Stable | `test`/`describe`/`it`, hooks, `mock`, reporters | [test.md](./test.md) |
| tls | `node:tls` | Stable | `TLSSocket`, `Server`, `connect`, `SecureContext` | [tls.md](./tls.md) |
| tty | `node:tty` | Stable | `ReadStream`/`WriteStream`, `isatty` | [tty.md](./tty.md) |
| zlib | `node:zlib` | Stable | gzip/deflate/brotli sync+async+streams | [zlib.md](./zlib.md) |

### P2 — later / experimental / deprecated (11)

| Module | `node:` | Stability | Key surface | Spec |
|---|---|---|---|---|
| diagnostics_channel | `node:diagnostics_channel` | Stable (recent) | `channel`, `subscribe`, `tracingChannel` | [diagnostics_channel.md](./diagnostics_channel.md) |
| domain | `node:domain` | **Deprecated** | `Domain`, `create` | [domain.md](./domain.md) |
| http2 | `node:http2` | Stable | `Http2Server`, sessions, streams, HPACK | [http2.md](./http2.md) |
| inspector | `node:inspector` | Stable API | **RTS-engine debug surface** (no V8 CDP) | [inspector.md](./inspector.md) |
| punycode | `node:punycode` | **Deprecated** | `encode`/`decode`/`toASCII`/`toUnicode` | [punycode.md](./punycode.md) |
| repl | `node:repl` | Stable | `REPLServer`, `start` | [repl.md](./repl.md) |
| sqlite | `node:sqlite` | **Experimental** | `DatabaseSync`, `StatementSync`, `Session` | [sqlite.md](./sqlite.md) |
| trace_events | `node:trace_events` | Experimental | `createTracing`, categories | [trace_events.md](./trace_events.md) |
| v8 | `node:v8` | Stable API | **RTS-engine** heap/serialize (no V8) | [v8.md](./v8.md) |
| vm | `node:vm` | Stable | **RTS JIT + own context** (no V8) | [vm.md](./vm.md) |
| wasi | `node:wasi` | **Experimental** | `WASI` class (needs a Wasm engine) | [wasi.md](./wasi.md) |

## Notes on the no-V8 trio

`v8.md`, `vm.md`, `inspector.md` document Node's full API surface but their
implementation sections map **exclusively to RTS-engine equivalents** — heap
stats from the RTS mark+sweep collector, serialize via RTS structured-serialize,
`vm` contexts via the RTS JIT/eval, inspector via RTS's own debug surface (CDP
compatibility is a phased goal, not a V8 embedding). Governed by
[`architecture.md`](./architecture.md) §11.

## Coverage

- **43** module specs + 2 support docs (`architecture.md`, `rts-std-migration.md`).
- Sourced from the authoritative Node.js 25 docs
  (`nodejs.org/docs/latest-v25.x/api/` + `github.com/nodejs/node`).
- Deprecated (`domain`, `punycode`) and experimental (`wasi`, `sqlite`) are
  specced but low priority; `wasi` is a strong deferral candidate (needs a Wasm
  engine).
