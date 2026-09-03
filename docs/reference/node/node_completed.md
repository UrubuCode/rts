# Node modules — completion tracker

**Source of truth for what is genuinely done**, verified by *running* the
engine, not by reading a module's own prose. `docs/reference/node/STATUS.md`
answers a different question — which specifiers are *registered* — and this
file does not repeat it. This file answers: does the fixture pass, and what
does the module's own "Not implemented, by name" section still say is
missing.

**Audited 2026-09-02**, by: `wc -l`/`find` over every `crates/rts-node/src/`
module folder; reading every module's own top-of-file doc comment IN FULL
(the crate's convention — see `crates/rts-node/src/lib.rs` — is that a module
states its own gaps by name, which makes it the best "exact gap" source that
exists, and is what every row below is drawn from); running
`target/fast/rts.exe test` against every dedicated fixture; and, for two
specific failures, an independent `node -e` call against the real Node.js
v20.19.5 installed on this machine, to tell an engine bug from a fixture that
assumed a platform this machine is not.

**The previous audit (2026-07-18) is not trustworthy and was not extended.**
It listed `async_hooks`, `child_process`, `cluster`, `domain`, `http`, `http2`,
`https`, `inspector`, `readline`, `repl`, `sqlite`, `test`, `trace_events`,
`tls`, `wasi`, `worker_threads`, `zlib` as "spec only, no real impl". Every one
of those has a real module folder with real `.rs` files today (see the tables
below for exact line/file counts), and most have real, passing fixtures. The
gap between what that file claimed and what exists is the reason this rewrite
exists at all — CLAUDE.md calls a stale tracker sending work to the wrong
place the worst failure mode a doc can have, and that is what this one was
doing.

---

## The trap the next person measuring this needs to know about

**Nine `node:fs` fixtures fail on THIS machine only because `C:\tmp` does not
exist.** On Windows, Rust resolves a path written as `/tmp/x` to `C:\tmp\x` —
not to a temp directory that exists by default — so any fixture that writes
under `/tmp` fails with a plain `ENOENT`, indistinguishable at a glance from a
real bug. These are **not** engine defects. `mkdir C:\tmp` before triaging a
Windows run.

---

## Headline numbers

- **Full suite: 819 of 848 `*.test.ts` files pass** (2026-09-02,
  `target/fast/rts.exe`, ONE PROCESS PER FILE — an uncaught exception and an
  endless loop each take the process with them, so a single-process harness
  would report whatever it reached first as the score).
- **It was 805 before this session's work**, and the difference is stated the
  only way this repository accepts: compared PER FILE against a kept binary of
  the tree as it stood before the first edit. **14 gained, 0 lost.** The net
  number never was the claim — `+14` is equally consistent with 14 gained and
  with 16 gained against 2 lost, and only one of those is shippable.
- **The `node:`/`net:`/`tls:` corpus is 110 files** (`node_*`, `net_*`,
  `tls_*` — there is no `ws_*` or bare `http*`-prefixed file; `node:http`'s
  own fixture is `node_http_full.test.ts`, counted under `node_*`).
- **106 of those 110 pass, and the 4 that do not are not `node:` at all** —
  they import the bare `rts` specifier's `net`/`tls`, which this crate does
  not own (see the table below). So **every fixture that imports a `node:`
  specifier passes**, which is a different and stronger sentence than the
  count, and is the one worth reading.
- **This section was itself caught by the race it describes, and the correction
  is left visible rather than tidied away.** A first pass read
  `node_tty_full.test.ts` and `node_util_basic.test.ts` as failing when they
  had already been rewritten, in this same tree, by another agent working
  concurrently. The audit spotted that pair — but then made the identical
  mistake twice more, and did not spot those: it recorded `node_util_full` and
  `node_tls_full` as failing with a "verified" root cause each, when both had
  been fixed minutes earlier by two other agents. Its stated count of six real
  failures was four by the time the file was saved.

  So the lesson is not "watch out for concurrent edits" — the audit knew that,
  wrote it down, and still published two stale findings as verified fact.
  **A measurement of a tree that is being written to is not a measurement**, and
  no amount of care inside the reading makes it one. What this file now carries
  was re-measured after every agent had stopped.

### The four real failures, all four explained

| Fixture | What fails | Why |
|---|---|---|
| `net_basic.test.ts` | `TypeError: Cannot read properties of undefined (reading 'tcp_listen')` | Imports `{ net } from "rts"` — the bare, pre-`node:` low-level namespace this crate does not own or provide (see "Not the same surface" below). **Not a `node:net` defect** — `node:net`'s own fixtures (`node_net_full`, `node_net_server`, `node_net_blocklist`) pass in full. |
| `net_tcp_echo.test.ts` | same shape, `reading 'exists'` | same cause |
| `net_udp_echo.test.ts` | same shape, `reading 'udp_bind'` | same cause |
| `tls_basic.test.ts` | same shape, `reading 'tcp_connect'` | Imports `{ net, tls } from "rts"` — same bare namespace, not `node:tls`. `node:tls`'s own fixture is below. |

**`node_tls_full` and `node_util_full` were in this table and are not any
more.** Both pass. They are named here rather than deleted because what put
them in it was the concurrency described above, not a defect: `tls` gained the
two CCM suites (`provider/mod.rs`'s `cipher_names()` lists all five RFC 8446
§B.4 suites, and `tls/mod.rs`'s outer doc — which this audit correctly flagged
as disagreeing with the inner one — was corrected with them), and
`node_util_full`'s two assertions were fixture bugs, fixed in the fixture:
`getSystemErrorName` now asks `process.platform` for the right number, because
libuv puts `ENOENT` at `-2` on POSIX and `-4058` on Windows, and `styleText`
now asserts what is true with no TTY, which is what real Node answers there too.


**Reproduce**: copy `tests/node_*.test.ts tests/net_*.test.ts tests/tls_*.test.ts`
into an empty directory and run `target/fast/rts.exe test <that directory>`.

### "Not the same surface" — `net_basic`/`net_tcp_echo`/`net_udp_echo`/`tls_basic`

These four import `{ net, tls } from "rts"` (the bare specifier), not
`from "node:net"`/`"node:tls"`. That bare `net`/`tls` was old-engine surface;
CLAUDE.md's own inventory of what the bare `rts` specifier carries today —
`num`, `math`, `hint`, `time`, `gc`, `atomic` — does not include `net` or
`tls`, so `net`/`tls` read `undefined` on the bare import by design, and every
member read off `undefined` throws. These four fixtures were never rewritten
for the `node:net`/`node:tls` modules this crate provides and predate them.
They are not evidence against `crates/rts-node/src/net/` or
`crates/rts-node/src/tls/`, which have their own fixtures (below) and mostly
pass.

---

## Modules with a dedicated fixture — all green

Every row below has at least one `.test.ts` that imports the module by its
`node:` specifier and every one of those fixtures passes (2026-09-02, isolated
run). "Gap" is condensed from the module's OWN "Not implemented, by name"
section — read the file for the full reasoning, not repeated here.

| Module | `src/` | Fixture(s) — tests | Real gap the module names for itself |
|---|---|---|---|
| `assert` | 5 files, 1181L | 2 files — 23 | `rejects`/`doesNotReject` need a promise that settles LATER (host can only mint an already-settled one); `AssertionError` has no reachable primordial `Error` to extend; `Map`/`Set`/`Promise` refuse deep-compare (report unequal rather than silently agree); typed-array compare ignores concrete constructor. |
| `buffer` | 2 files, 773L | 7 files — 24 | `Blob`/`File` over a raw `ArrayBuffer` (not a view) reads zero bytes; `resolveObjectURL` always `undefined` (nothing can mint an id — `URL.createObjectURL` is itself refused); every `ERR_*` this file would throw answers a no-op stand-in instead (native can't raise past `TypeError` from here yet). `Buffer` class itself, `atob`/`btoa`, `Blob.stream()` all real. |
| `child_process` | 8 files, 1852L | 1 file — 3 | `spawnSync`/`execSync`/`execFileSync` real (incl. real `/bin/sh -c` shell-out for `execSync`); `spawn()`/`ChildProcess` real but event-driven only at the next call. `exec`/`execFile`/`fork`/IPC (`send`/`disconnect`/`channel`)/piped-stdio `'data'` events all refused by name. `kill()` always force-kills regardless of signal name (no `libc`/`windows-sys` dep). |
| `constants` | 1 file, 128L | 1 file — 5 | Flat union of `os`/`fs`/`crypto` constants. Missing the 6 `PRIORITY_*` + `UV_UDP_REUSEADDR` (post-date Node's own flatten — correctly excluded, not a gap) and the OpenSSL half of `crypto.constants` (`crypto` itself doesn't link OpenSSL). |
| `crypto` | 17 files, 3274L | 18 files — 61 | `Hash`/`Hmac`/random/all 3 KDFs (both spellings)/4 AES modes (128/256-GCM, CBC)/X25519+XEdDSA (deliberately non-Node names, see the module doc)/WebCrypto `digest` all real. Missing: ciphers beyond those 4; the WHOLE asymmetric+X.509 tier (`KeyObject`, `Sign`/`Verify`, `DiffieHellman`/`ECDH`, `X509Certificate`, `generateKeyPair*`) — needs DER/PEM/PKCS8/SPKI infra not added; every `SubtleCrypto` method but `digest`. |
| `dgram` | 12 files, 1725L | 3 files — 32 | Full UDP `Socket` round-trip, `createSocket`, `BlockList`, IPv4 multicast-by-address, buffer sizing, `connect`/`disconnect`/`remoteAddress` all real. Missing: `setMulticastInterface` by name/index on `udp6` (`ENOSYS`, no `std` resolver); source-specific multicast join (validated, not performed — platform struct layout unverified); `udp6` multicast TTL (`std` has no setter); `createSocket`'s `lookup`/`signal`, `bind`'s `fd`. |
| `diagnostics_channel` | 3 files, 838L | 2 files — 15 | Channel identity-by-name, publish/subscribe, full `TracingChannel`, `bindStore`/`unbindStore`/`runStores` over a real `AsyncLocalStorage` all real. Missing: symbol-named channels; `ERR_INVALID_ARG_TYPE` throws (answers `undefined`/`false` instead); `TracingChannel.traceCallback` (4 native arg slots exhausted before the wrapped fn); subscriber-exception isolation. |
| `dns` | 5 files, 661L | 1 file — 3 | `lookup()` real (OS `getaddrinfo` path); `resolve4()` real DNS-protocol client (`hickory-resolver`). Missing: `resolve6`/`resolveAny`/`Caa`/`Cname`/`Mx`/`Naptr`/`Ns`/`Ptr`/`Soa`/`Srv`/`Tlsa`/`Txt`/`reverse`/`Resolver` class (each needs its own `RData` decode, none built); `lookupService` (no service-name DB this crate will fabricate). |
| `events` | 1 file, 769L | 1 file — 13 | `EventEmitter` real, on ONE shared prototype (a factory-shape bug that made `Object.keys`/prototype identity wrong is fixed). `addAbortListener` and Symbol-keyed names work (via `entry::closure_new`). Missing: `EventTarget`/`Event`/`CustomEvent` (separate WHATWG globals not reachable from here); `events.on`/`events.once` (need a promise built and driven from a native — see cross-cutting notes); `rawListeners`' real once-wrapper with a `.listener` back-reference. |
| `fs` | 22 files, 5003L | 29 files — 145 | Full sync surface + promises + `FileHandle` + Read/WriteStream/`watch`/`glob`/`Dir`/`Dirent`/`Stats`/`StatFs`. Every documented invalid argument now raises the right `TypeError` with the right `code` (this fixed 37 "Missing expected exception" failures against Node's own suite). Async **callback-taking** forms are out of scope by architecture — a native cannot safely re-enter compiled code — only the `*Sync` forms and the `notify`-backed `watch` queue exist. |
| `http` | 10 files, 2014L | 1 file — 8 | Real HTTP/1.1 head parser + chunked/`Content-Length` body decoder (hand-written — the one piece nothing else in the crate had); real `Server`/`IncomingMessage`/`ServerResponse`/`ClientRequest`/`Agent` over real `net.Socket`. Missing: keep-alive/pipelining (always `Connection: close`); `100-continue`; `CONNECT`/`Upgrade`; write-side trailers; a **streaming** `ClientRequest` (`.end()` blocks for the whole exchange — no event loop to post a later turn into). |
| `module` | 4 files, 1033L | 1 file — 12 | `builtinModules`/`isBuiltin`/`Module.wrap`/`findPackageJSON`/`SourceMap` class real. `register`/`registerHooks`/`stripTypeScriptTypes`/`enableCompileCache` answer `undefined` (a value a program can test) rather than silently doing nothing — each needs a resolver hook, a TS parser dependency, or a cache this module does not own. |
| `net` | 9 files, 2026L | 3 files — 49 | Full TCP `Socket`/`Server` over real sockets (background thread + queue-and-pump delivery to the JS thread), `BlockList`, `SocketAddress`, `isIP` family. IPC (Unix sockets/named pipes) now **refused out loud** (`ERR_INVALID_ARG_VALUE`) rather than silently. Missing: Happy Eyeballs/`autoSelectFamily` (needs `dns.lookup`, cross-module); `fd`/`onread`/`signal` options; `'drop'`/`maxConnections` enforcement (settable, unread); `getConnections()` always `0`. |
| `os` | 6 files, 1957L | 1 file — 26 | Every function real, computed per-call (deliberately not cached, matching Node's own property-vs-function inconsistency). Missing: `getPriority`/`setPriority`/`userInfo` don't throw for a bad pid/unresolvable user (native can't raise a `SystemError` from here); `cpus()` has no per-core detail off Linux/Windows; `totalmem`/`freemem`/`uptime` answer `0` off Linux/Windows; `availableParallelism` is not cgroup-aware. |
| `path` | 1 file, 590L | 2 files — 60 | All of `join`/`resolve`/`normalize`/`relative`/`dirname`/`basename`/`extname`/`parse`/`format`/`isAbsolute`/`toNamespacedPath` + `sep`/`delimiter` + `posix`/`win32` (both real, one generic core parameterized on the separator) + `matchesGlob` (`*`, `**`, `?`) real, as pure string ops (not routed through `std::path`, which gets several of these wrong — see the module doc). Missing: `matchesGlob` brace-expansion/character-classes (answers `false`, same as a genuine non-match). |
| `perf_hooks` | 5 files, 1543L | 1 file — 11 | User Timing timeline, `PerformanceObserver`, `timerify` (via `entry::closure_new`), one `Performance` singleton shared with `rts-std`'s global `performance` (one `now`, one origin — was two disagreeing objects), recordable+interval histograms all real. Missing: `eventLoopUtilization`/`nodeTiming` (nothing measures idle time or boot milestones — refused rather than a fabricated `1.0`/`-1`); resource-timing family (no `fetch`/`http` feed); `percentile`/`percentiles` (needs the raw distribution, not the running aggregates kept). |
| `process` | 7 files, 1805L | 5 files — 25 | `platform`/`arch`/`pid`/`argv`/`env`/`versions`/`execPath` real properties; `memoryUsage`/`cpuUsage`/`resourceUsage`/`kill` real and cross-platform (POSIX `getrusage`/`kill`, Windows `GetProcessTimes`/`K32...`/`TerminateProcess`); `nextTick` a real loop source; `getActiveResourcesInfo` over the `timers`+`net` tables. Missing: every process-level event (`'beforeExit'`, `'uncaughtException'`, `'unhandledRejection'`, signals — no engine dispatch point exists for any of them); `send`/`disconnect`/`channel` (no IPC); the GC-heap fields of `memoryUsage`. |
| `punycode` | 1 file, 397L | 1 file — 22 | Full RFC 3492 Bootstring (`encode`/`decode`/`toASCII`/`toUnicode`) + `ucs2` real. Missing: full IDNA/UTS-46 validation (Bidi/ContextJ/NFC — Node's own `punycode` doesn't do this either, per the spec's own security note); a deprecation warning (no general mechanism in this crate yet); the bare `require("punycode")` specifier (only `node:punycode` registered). |
| `querystring` | 1 file, 314L | 2 files — 29 | `parse`/`stringify` (+`decode`/`encode` aliases, genuinely the same function under two names)/`escape`/`unescape` + every options overload real. Missing: the result object has an ordinary prototype, not Node's null-prototype hardening (a `__proto__` key lands as an own property); `escape`/`unescape`'s exact table is not verified byte-for-byte against a real Node binary (the module's own doc says so). |
| `stream` | 9 files, 2138L | 1 file — 14 | `Readable`/`Writable`/`Duplex`/`Transform`/`PassThrough` + `pipeline`/`finished`/`compose`/`consumers`/`promises`/`web` — **all now real** (the module doc corrects two earlier refusals of `stream/web` and `stream/promises` as stale). Missing: `isErrored`/`isReadable`, `addAbortSignal`, `fromWeb`/`toWeb` pairs, `.wrap()`, the async-iteration helper family (`map`/`filter`/`forEach`/`toArray`/`reduce`/…). |
| `string_decoder` | 1 file, 504L | 2 files — 28 | Full `StringDecoder` (all 6 encodings, incl. holdback bookkeeping for a split multibyte character) real; now genuinely **throws** (`TypeError`) for an unknown encoding. Missing: the class raised is `TypeError`, not Node's own dedicated one; a non-string/non-byte-view argument to `write`/`end` silently decodes as `""` instead of raising; no `FinalizationRegistry` to drive disposal. |
| `timers` | 2 files, 805L | 2 files — 12 | `setTimeout`/`setInterval`/`setImmediate`+`clear*` real, **including a timer with nothing scheduled after it** (the host's `drain()` sleeps to the nearest deadline and pumps again). Missing: no `Timeout`/`Immediate` object, so `.ref()`/`.unref()`/`.refresh()` have nothing to hang on; only 1 of a callback's trailing `...args` is forwarded (4-slot native convention). |
| `tls` | 17 files, 2593L | 1 file — 3 of 3 | Real `rustls`-backed handshake, `Socket`/`Server` client+server, TLS 1.3 only. Provider covers all **five** RFC 8446 §B.4 suites — AES-128/256-GCM, ChaCha20-Poly1305, and the two CCM ones, which needed their own crate (`ccm`, registered in `crates.md` with the note that it is NOT inside the NCC Group review the GCM/ChaCha20 row cites). Missing: TLS 1.2 entirely; P-384 key exchange; RSA-PSS/Ed448 verify; RSA signing from our own `SecureContext`; PFX/PKCS12; encrypted PKCS#8 keys. |
| `tty` | 3 files, 831L | 1 file — 6 | `isatty`/`getWindowSize`/`getColorDepth`/`hasColors`/cursor writes to a `Writable`/the color-environment heuristic all real. Missing: `'resize'` event (needs a `SIGWINCH`/`ReadConsoleInputW` poller this crate has nowhere to run); legacy-Windows cursor path (no VT support → nothing written rather than literal escape text); `net.Socket` ancestry (no stream base to extend — no real `.pipe()`/backpressure). |
| `url` | 5 files, 1423L | 2 files — 29 | Full `URL`+`URLSearchParams` — every getter/setter incl. `href` re-parse, LIVE `searchParams`↔`href` sync, `canParse` — through real accessor pairs on `URL.prototype` (not stamped snapshot properties any more) + `fileURLToPath`/`pathToFileURL`/`domainToASCII`/Unicode/`urlToHttpOptions`/`format`/legacy `parse`/`resolve`, all real. Missing: `href = x` is the only setter that should throw on failure and doesn't yet; a malformed constructor argument answers an inert instance rather than throwing (a decision now available, not yet taken and measured); `URLPattern`; `createObjectURL`/`revokeObjectURL`. |
| `util` | 14 files, 2698L | 8 files — 49 (7 files fully green; `node_util_full.test.ts` is 14 of 16 — its 2 failures are in the table above, both verified as fixture bugs, not engine bugs) | `format`/`formatWithOptions`/`isDeepStrictEqual`/`stripVTControlCharacters`/`parseArgs`/`styleText`/`getSystemErrorName`/`deprecate`/`debuglog`/`callbackify`/`promisify`/`inherits` **all real now** — two earlier "needs a closure this crate cannot build" refusals were lifted by `entry::closure_new`/`entry::promise_new`. Missing: every `util.types.*` brand predicate but `isArray`/`isArrayBufferView`/`isModuleNamespaceObject` (~37 — no brand check on the host surface for `Map`/`Set`/`Promise`/`Proxy`/`RegExp`/boxed primitives); deep `inspect` (named as its own sub-project). |
| `v8` | 2 files, 654L | 1 file — 10 | `getHeapStatistics`/`getHeapSpaceStatistics` real (the `Context`'s own region capacity/used, in bytes); `serialize`/`deserialize` real — a genuine wire codec, not the `structuredClone` walk. Missing: `getHeapCodeStatistics`/`getCppHeapStatistics` (nothing tracks a compiled-code region or a second heap); `setFlagsFromString` (no runtime flag namespace); the whole profiler/snapshot family (no collector to hook — this engine has no GC-triggered profiling at all yet). |
| `zlib` | 5 files, 1490L | 3 files — 18 | One-shot + callback-twin buffer functions, `crc32`, all 9 streaming `Transform` classes+factories, `constants`, over `flate2` (deflate/gzip)+`brotli`, answering real `Buffer` instances. Missing: the WHOLE Zstd sub-surface (no dependency; Node itself marks it Experimental); `zlibBase.params()` mid-stream change (`flate2`'s adapters don't expose it); `dictionary`/`windowBits`/`memLevel`/`strategy` (not settable through the adapters this module is built on — a legal value silently no-ops, an illegal one still raises `ERR_OUT_OF_RANGE`). |
| `ws` (npm `"ws"`, not a `node:` module — see `STATUS.md`) | 9 files, 2304L | 1 file — 6 | WebSocket **client** (`new WebSocket(url)`) real, over the same RFC 6455 core the (unbuilt) server half would share. Missing: `new WebSocketServer({ server })` — `node:http` does not emit `'upgrade'`, so there is no socket to hand it; refused rather than an option that silently does nothing. |

---

## No dedicated fixture — not verified by execution

These modules have real `.rs` code and a real "Not implemented, by name"
section, but **no `.test.ts` file in this repository imports them by their
`node:` specifier** (checked every quoting style). Their row states what
their own doc claims and what line/file count backs it — not whether the
claim is true. Read "not verified" literally: it means what it says, not
"probably fine."

| Module | `src/` | What the module's own doc claims is real | What it names as absent |
|---|---|---|---|
| `async_hooks` | 4 files, 767L | `AsyncLocalStorage` (`run`/`getStore`/`enterWith`) and `AsyncResource` (`runInAsyncScope`/`emitDestroy`) as real Rust push/call/pop; `createHook` fires for resources this module itself creates. | `.bind`/`.snapshot` (static and instance) — the doc cites "no callable with a captured environment" as the wall. **That premise may be stale**: `entry::closure_new` exists now and is used successfully for the identical shape in `child_process`, `events`, `module`, `perf_hooks`, `stream`, `util` (see cross-cutting notes) — not re-attempted here, so not claimed fixed either. |
| `cluster` | 1 file, 369L | Primary/worker split over `child_process`'s own real spawn; `env`'s `NODE_UNIQUE_ID` marker. | NO IPC at all — `worker.send`/`.disconnect`/`.isConnected`, the `'message'`/`'listening'`/`'disconnect'` events, socket handoff all need a channel that does not exist. `schedulingPolicy` stored, never enforced. |
| `console` (the `Console` **class** — distinct from the ambient global `console`, which is `rts-std`'s and out of this crate) | 386L | `log`/`info`/`debug`/`dir`/`error`/`warn`/`trace`/`assert`/`group`/`groupEnd`/`count`/`countReset`/`time`/`timeEnd`/`timeLog`/`table`, wired through `node:util`'s own structural formatter (one struct walk, reached twice, not a second one). | `dirxml`, `profile`/`profileEnd`/`timeStamp` (no-op in real Node too), `clear`, `%s`/`%d`-style format specifiers inside `log`/`error`. `tests/console_stub_methods.test.ts` exercises the **global** `console` (a `rts-std` surface, different code, out of this file's scope) and is not evidence either way for this class. |
| `domain` | 292L | The active-domain stack, real, over the same frame-stack SHAPE `async_hooks` uses (a separate stack, deliberately — a domain frame and an `AsyncLocalStorage` frame answer different questions). | `bind`/`intercept` — same closure-environment wall `async_hooks` names, same "may be stale, not re-attempted" caveat. `domain.add`/`.remove` don't reroute another emitter's `'error'` (would need overriding `emit` while still reaching the original prototype's `emit` for every other event — no un-invoking prototype-walk exists on this host surface). |
| `http2` | 13 files, 3384L | The wire layer, in full and unit-tested against RFC 9113/7541's own shapes: the 9-byte frame header, connection preface, all 8 core frame types; complete HPACK (static table, integer/string primitives incl. Huffman decode, dynamic table+eviction, encoder+decoder). | The ENTIRE session/stream lifecycle — `connect`/`createServer`/`Http2Session`/`Http2Stream`/the Compatibility API. **No class in this module is constructible by a program** — stated by the module's own doc as a deliberate stop, not an oversight: a session needs a live handshake, `SETTINGS` exchange, flow-control windows and a multiplexed dispatch loop over `net`/`tls` that nothing here builds yet. |
| `https` | 5 files, 576L | `createServer`/`Server` + `request`/`get` + `Agent`/`globalAgent` real, reusing `http`'s own tested parser/`IncomingMessage`/`ServerResponse`/`ClientRequest` and `tls`'s tested handshake — no second HTTP parser anywhere in this module. | Inherits every gap `http`'s row above names (keep-alive, `100-continue`, `CONNECT`/`Upgrade`, a streaming `ClientRequest`), plus: no separate `https.Agent` (hands back `http.Agent` itself); one `SecureContext` serves every connection (no per-hostname SNI routing). No fixture imports `"node:https"` under any quoting style — what's real above is inferred from reusing two OTHER modules' tested code paths, not from running `https` itself. |
| `inspector` | 3 files, 510L | `open`/`close`/`url`/`waitForDebugger` over a real loopback `TcpListener`+HTTP discovery responder; `Session.post` for `Runtime.evaluate` (via `entry::evaluate`) and heap usage, genuinely real. | No WebSocket upgrade and no JSON-RPC command loop — so nothing can actually ATTACH past discovery. `Profiler.start`/`HeapProfiler.takeHeapSnapshot` (no sampling profiler — same gap `v8` names). |
| `readline` | 383L | `Interface` (real `EventEmitter`-chained, line-splitting over a `Readable`) + `clearLine`/`cursorTo`/etc. writing real ANSI to a caller-supplied `Writable`, all real. | `node:readline/promises` in full (needs a promise that resolves LATER from a native — same gap `events.on`/`.once` name); `emitKeypressEvents` and every terminal-mode keybinding (needs raw mode, which `tty` already refuses); history; `completer`. |
| `repl` | 289L | A line-at-a-time REPL over `readline`'s `Interface`, each line a genuinely fresh `entry::evaluate` call. | **Stated as THE limit up front**: no persistent cross-line context — `let x` on one line is not visible on the next, which is the one thing a REPL exists to give. Everything else absent (`_`/`_error`, `.editor`/`.save`/`.load`, history file I/O) follows from that one limit. |
| `sqlite` | 4 files, 788L | `DatabaseSync`/`StatementSync` real over `turso_core` (pure-Rust, SQLite-file-compatible), every extern driven synchronously in-thread. | `Session`/`changeset`/`patchset` (no session extension in `turso_core`'s public API); `aggregate`/`function`/`setAuthorizer` (no callback hook to attach a JS function to); `enableLoadExtension` (no `sqlite3_load_extension` equivalent — the exact thing a pure-Rust engine lacks). *The one file matching `"node:sqlite"` (`node_module_full.test.ts`) only checks `isBuiltin()` list membership — no `DatabaseSync` call exists anywhere in this repository's fixtures.* |
| `test` (`node:test`) | 2 files, 307L | `test`/`describe`/`it`/the 4 hooks run real, synchronously and immediately (this crate cannot await a body, so nothing is scheduled). `before`/`after` honored by scope; `beforeEach`/`afterEach` run outermost-to-innermost. | **Stated as THE limit**: a thrown error inside a test body is not caught here at all — a failing test's outcome is undefined by this module, not silently reported as a pass. `mock.*` absent (same closure-environment question as `async_hooks`, not re-attempted). `tests/test.test.ts` exercises `rts:test` — this repository's OWN harness, a different module — not `node:test`. |
| `trace_events` | 180L | Category enable/disable is a genuine reference-counted set — two `Tracing` objects enabling the same category and one disabling it leaves it enabled, per spec. | The entire trace-event CORE (a record format, a file writer, any producer) — the module's own doc says this belongs below `rts-node`, in `rts-engine`, since `v8`-category events originate inside the GC/compiler. No CLI flag populates a category floor. |
| `wasi` | 4 files, 983L | A real `wasmi`-backed WASI preview1 host — `start`/`initialize`/`getImportObject` work, taking the Wasm module's RAW BYTES rather than an already-instantiated `WebAssembly.Instance` (there is no `WebAssembly` global in this engine at all — named up front as the one real divergence from Node's documented shape). | EVERY filesystem syscall (`path_open`, `fd_seek`, `fd_readdir`, …) answers `ENOSYS`/`ENOTCAPABLE` always (no fd table beyond stdin/out/err). `sock_*`, `poll_oneoff` — `ENOSYS`, matching real WASI runtimes' own stance per the module's cited reference. |
| `worker_threads` | 3 files, 798L | `new Worker(source, { eval: true })` starts a genuine OS thread running its own full engine instance (own compiler call, own region, own module copies — nothing shared). `eval: false` refused by name (no loader to resolve a path). Delivery: the worker's thread never calls a listener directly; it queues data and the PARENT turns it into `'message'`/`'error'`/`'exit'` on its own thread. | `MessagePort`-BETWEEN-threads (the same-thread local pair IS real), `BroadcastChannel`, `transferList`, `resourceLimits` (nothing bounds a worker's heap), `worker.stdin`/`stdout`/`stderr`, `getHeapSnapshot`, `cpuUsage`. |
| `vm` | 330L | `runInNewContext`/`runInContext`/`runInThisContext` genuinely real over `entry::evaluate` + a real `Context` object; object identity IS preserved for what crosses. **One incidental fixture** — `tests/claude-page-scope-declara.test.ts` exercises `createContext`+`runInContext`+`var`-vs-`let` leak semantics directly and passes — but no file in this repository is a *dedicated* `vm` fixture. | **Stated as THE limit**: `entry::evaluate` compiles+runs source as its own brand-new, disconnected program — no caller variable is visible to it, no declaration it makes is visible back; only a value needing no region (number/boolean/singleton) crosses, and an object (including a function) reads back as `undefined`, indistinguishable from a compile failure. `vm.Module`/`SourceTextModule` (needs a dynamic linked module record — imports resolve statically here). |

---

## Cross-cutting notes

**The named-import-binds-`undefined` blocker from the 2026-07-18 audit is
CLOSED, verified by execution.** `node_buffer_full.test.ts` does
`import { atob, btoa } from "node:buffer"`, `node_timers_full.test.ts` does
`import { setTimeout, ... } from "node:timers"`, `node_crypto_full.test.ts`
does `import { createHash, ... } from "node:crypto"` — all three pass. A
program importing a named member of a global-exporting module now gets the
real thing.

**A native can raise a catchable error, and it is a real capability, adopted
piecemeal.** `rts_core::entry::throw_type_error` is public and used by `fs`
(every documented invalid argument, via `crate::errors`), `assert` (a failed
assertion), `string_decoder` (an unknown encoding), and `url` (partially).
Most other modules' OWN current doc comments — `net`, `os`, `process`,
`dgram`, `tty`, `wasi`, `diagnostics_channel` among them — still say a native
in THEIR file cannot raise a catchable exception at all. That is not
necessarily stale the way the closure claim below is: several of them need a
richer shape than a plain `TypeError` (a `SystemError` carrying `code`/
`errno`/`syscall`, or Node's own named error classes), which
`throw_type_error` alone does not give them. Read each module's own doc for
which case it is; this file does not adjudicate all of them.

**A native callable can now carry captured state, and it is a real
capability, adopted piecemeal.** `rts_core::entry::closure_new` mints a
callable bundling a code address with a captured value, and it is used
successfully today in `child_process` (async listener wiring),
`diagnostics_channel`, `events` (`addAbortListener`), `module`
(`require`/`resolve`/`paths`), `perf_hooks` (`timerify`), `stream`
(`promises`' settlers), and `util` (`promisify`/`deprecate`/`debuglog`/
`callbackify`). Four modules' "Not implemented" sections —
`async_hooks` (`.bind`/`.snapshot`), `domain` (`bind`/`intercept`),
`test_runner` (`mock.*`), and `events` itself (`rawListeners`' real
once-wrapper) — still cite "no callable with an environment slot" as the wall.
**That premise looks stale next to the working uses above, but this audit did
not attempt any of the four rebuilds, so it is flagged rather than claimed
closed.**

**The delivery model is one architectural fact, stated once instead of the
twelve near-identical times each module states it.** This engine has no event
loop. A background-thread event (`net`'s `'data'`, `dgram`'s `'message'`,
`tls`'s `'secureConnect'`, `fs.watch`'s change, a `child_process`/
`worker_threads` message) is queued as plain data on whichever thread produced
it and delivered — synchronously, calling the JS listener directly — at the
start of the NEXT native call this program makes into *that same module*.
`node:timers`' `drain()` is the one exception: the host calls it at the end of
a turn, and it sleeps to the nearest due deadline and pumps again, which is
why a bare `setTimeout` with nothing after it still fires. A program that
calls `net.listen()` (or `dgram.bind()`, or `fs.watch()`, …) and then makes no
further call into that module never observes its own pending event, however
long the process runs.

**Doc drift is not hypothetical — this audit found a live instance while
reading, and it has since been closed.** `crates/rts-node/src/tls/mod.rs`'s
own top-of-file doc said the provider covered "AES-128/256-GCM and
ChaCha20-Poly1305 only" while `tls/provider/mod.rs`'s own doc — more specific,
and the one this file's numbers were verified against — described more. Two
true-sounding statements about one tree, disagreeing because one was not
updated when the other's code changed. Both now name the same five RFC 8446
§B.4 suites, and `cipher_names()` lists all five.

That it was found by READING two docs against each other, rather than by any
test, is the point worth keeping: nothing in the suite can fail because a
comment is wrong. This is the same class as the stale tracker at the top of
this file, one crate down.

---

## Reproducing this audit

```bash
# structure — files and lines per module
find crates/rts-node/src/<module> -name '*.rs' | xargs wc -l

# a module's own claimed gaps — always the freshest source in this crate
sed -n '/^\/\/! # Not implemented/,/^use /p' crates/rts-node/src/<module>/mod.rs

# isolated fixture run — avoids racing concurrent edits to the shared tests/ tree
mkdir /some/scratch/dir
cp tests/node_*.test.ts tests/net_*.test.ts tests/tls_*.test.ts /some/scratch/dir/
target/fast/rts.exe test /some/scratch/dir     # or a fresh --profile fast build
```

`target/fast/rts.exe` used for this audit was built 2026-09-02 21:12 from this
tree's state at that time; it does not carry any edit made after that instant
by any of the other agents sharing this working tree. Rebuild
(`cargo build --profile fast`) before trusting a number against a tree that
has since changed.
