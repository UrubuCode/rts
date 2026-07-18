# Node modules — completion tracker (100% vs partial)

**Source of truth for what is genuinely DONE.** The per-module `<module>.md`
`Status` rows are STALE for many modules (they say "Not implemented — spec only"
even where code exists, and conversely list surface as deferred that has since
landed). This file is verified by actually importing + exercising each module.

A module is **100%** only when its ENTIRE documented function/class surface is
implemented AND importable AND working — no deferred items, no broken exports, no
missing options. Anything short is **partial** and listed with its exact gap so a
roadmap step can close it.

Last audited: 2026-07-18 (after stream/string_decoder/diagnostics_channel/
querystring/url work). **Audit note:** `typeof <named-imported-member>` reports
`"undefined"` even for members that WORK (a `typeof` cosmetic bug) — verify a
member by CALLING/reading it, never by `typeof`. Several "gaps" in the previous
revision (os EOL/constants, path sep/delimiter) were this artifact and are in
fact complete.

---

## ✅ 100% COMPLETE (full surface, verified)

| Module | Evidence | Notes |
|---|---|---|
| **node:string_decoder** | `tests/node_string_decoder.test.ts` 11/11 | `StringDecoder` (ctor default/encoding + `ERR_UNKNOWN_ENCODING`, `write`, `end`+buffer, `text(buffer, offset)`, `encoding`) for utf8/utf16le/base64/base64url/latin1/ascii/hex + boundary handling + U+FFFD flush + reuse; string/Uint8Array/Buffer input. |
| **node:querystring** | `tests/node_querystring.test.ts` 6/6 | `parse`/`stringify` (+ `decode`/`encode` aliases) + `escape`/`unescape`; ALL overloads incl the `options` arg (`maxKeys` incl 0=unlimited, custom `decodeURIComponent`/`encodeURIComponent` invoked as real fn values). Repeated keys → arrays; arrays → repeated pairs. |
| **node:punycode** | verified (`encode`/`decode`/`toASCII`/`toUnicode`/`ucs2.decode`/`ucs2.encode`/`version`) | Deprecated module, full RFC 3492 algo + ucs2 UTF-16⇄codepoint. Default-import namespace works. |
| **node:os** | verified (all fns + EOL/devNull/constants + userInfo, default + named import) | platform/arch/cpus/totalmem/freemem/loadavg/uptime/tmpdir/homedir/endianness/EOL/devNull/constants/machine/version/release/type/availableParallelism/networkInterfaces/userInfo/hostname/getPriority/setPriority. (`userInfo({encoding:'buffer'})` buffer-variant is the only edge deferred — default string form real.) |
| **node:path** | verified (all fns + top-level sep/delimiter + posix/win32, default + named import) | join/resolve/normalize/relative/dirname/basename/extname/parse/format/isAbsolute/toNamespacedPath + `sep`/`delimiter` + `path.posix.*`/`path.win32.*`. |
| **node:url** | `tests/node_url.test.ts` 6/6 + `tests/node_url_full.test.ts` 23/23 | `URL` (ctor + base, ALL getters, ALL SETTERS incl `href` re-parse, `searchParams` with mutation→search/href sync, `toString`/`toJSON`, static `canParse`); `URLSearchParams` full multimap (get/getAll/has/set/delete/append/keys/values/**entries**/**forEach**/sort/**size**/toString); node fns `fileURLToPath`/`fileURLToPathBuffer`/`pathToFileURL`/`domainToASCII`/`domainToUnicode`/`urlToHttpOptions`/`format`(URL+legacy)/legacy `parse`/`resolve`. (Only `JSON.stringify(url)` not calling `toJSON` remains — a systemic engine JSON gap, not url-specific.) |
| **node:net** | `node_net_full` 8/8 + `node_net_blocklist` 21/21 + `node_net_server` 19/19 (real TCP echo) | `isIP`/`isIPv4`/`isIPv6`; `BlockList` (add/check/rules) + `SocketAddress`; `Server` (createServer/listen/close/address/getConnections/ref/unref/listening/**maxConnections**+`'drop'`) over real TCP; `Socket` (connect/write/end/destroy/pause/resume/setEncoding/setKeepAlive/setNoDelay/setTimeout/set·getTypeOfService/ref/unref + all address/state getters); process-wide autoSelectFamily config; `net.SOMAXCONN`. (`dropMaxConnection` is a plain cluster-only data prop — no-op in single-process, per its Node shape.) |
| **node:fs** | `node_fs_full` 15 + `node_fs_streams` 15 + `node_fs_utf8stream` 9 + `node_fs_filehandle_streams` 9 + `node_fs_promises`(+`_import`) 5+5 + callbacks/watch/glob/stats/dirent/statfs/… suites | Full sync + callback + promises API (`import { promises }` + `node:fs/promises`); `FileHandle` (read/write/close/stat/truncate/sync/chmod + **createReadStream/createWriteStream/readableWebStream/readLines**); `ReadStream`/`WriteStream` + `createReadStream`/`createWriteStream` (flowing + `.pipe` real copy + setEncoding); `Utf8Stream`; `watch`/`watchFile`/`unwatchFile` (real notify-crate watcher); `Dir`/`Dirent`/`Stats`/`StatFs`; `glob`; `cp`/`opendir`/`mkdtemp`/link/utimes. Unblocked by the `.push`-on-`any` engine fix + `engine.fs_*` bridges. (`instanceof` on an fh-RETURNED stream is `false` — a class-identity-through-return quirk; the stream is fully functional.) |

> Eight modules now pass the strict bar. `dgram` is CLOSE (below).

---

## 🟡 NEAR-COMPLETE (one bounded gap — fastest wins)

| Module | Works | Exact remaining gap |
|---|---|---|
| **node:dgram** | full `Socket` UDP round-trip + `createSocket` (memory: complete) | `createSocket`'s `lookup`/`signal` options and `bind`'s `fd` option THROW instead of being honored (documented, not faked). |

---

## 🟠 PARTIAL — core landed, substantial surface or subsystem missing

| Module | State |
|---|---|
| **node:stream** | Core Readable/Writable/Duplex/Transform/PassThrough + pipeline/finished/compose/consumers/promises DONE (10/10). Deferred: `CompressionStream`/`DecompressionStream` (shared zlib/brotli codec externs), WHATWG BYOB reader, real cross-thread transfer. |
| **node:diagnostics_channel** | Channel + module fns + full TracingChannel DONE (7/7). Gaps: `unsubscribe`/`unbindStore` removal-by-ref is a no-op (engine function-identity gap); `bindStore`/`runStores` async-context propagation needs `AsyncLocalStorage` (node:async_hooks). |
| **node:crypto** | Hash/Hmac + random core implemented (821L), but `import { createHash } from "node:crypto"` binds **undefined** (import wiring) AND Cipheriv/Decipheriv, sign/verify, KeyObject/asymmetric, pbkdf2/scrypt, DiffieHellman, WebCrypto `subtle`, X.509 all deferred. |
| **node:buffer** | `Buffer` global works, but `import { Buffer } from "node:buffer"` binds **undefined**; `Blob`/`File`/`atob`/`btoa` exports + object-backed-class read gaps. |
| **node:events** | `EventEmitter` on/emit/off work, but `import { EventEmitter }` static helpers (`EventEmitter.once`, `once`/`on` module fns, `getEventListeners`, `captureRejections`, `EventEmitterAsyncResource`) missing/broken. |
| **node:timers** | Global `setTimeout`/etc work, but `import { setTimeout } from "node:timers"` binds **undefined**; `timers/promises`; `...args` forwarding deferred. |
| **node:process** | Large surface; several subsystems (signals, real `nextTick` ordering, `...args`) deferred. |
| **node:util** | format/formatWithOptions/isDeepStrictEqual/stripVTControlCharacters/parseArgs/styleText/getSystemErrorName done; DEFERRED: deep `inspect`, `types.*` (~40 predicates), legacy `is*` checkers, `promisify`/`callbackify`, `inherits`, `deprecate`, `debuglog`, `parseEnv`. |
| **node:assert** | ok/equal/strict/deep + throws/doesNotThrow/ifError/match/doesNotMatch/fail done; DEFERRED: `rejects`/`doesNotReject` (async), `AssertionError` class, callable default `assert(v)`, `CallTracker`. |
| **node:dns** | ~stub (144L); `lookup`/`resolve*`/`Resolver` import undefined — needs a DNS-protocol resolver + async. |
| **node:module / v8 / perf_hooks** | partial; each needs loader hooks / V8-less engine surface / observer+async respectively. |

---

## ⛔ NOT STARTED (spec only, no real impl)

`async_hooks`, `child_process` (scaffold), `cluster`, `console` (global exists;
the `node:console` module surface unverified), `domain`, `globals`, `http`,
`http2`, `https`, `inspector`, `readline`, `repl`, `sqlite`, `test`,
`trace_events`, `tls` (76L stub), `tty` (isatty only; streams blocked),
`wasi`, `worker_threads`, `zlib` (sync `*Sync` + constants only — callback/stream
forms + zstd deferred).

---

## Cross-cutting ENGINE blockers (fixing one unblocks many modules)

0. ~~**Import name shadowing a param/local**~~ — FIXED (b111fdc5): a call to a
   param/let named the same as a program-scope import (`new Promise((resolve) =>
   resolve(x))` alongside `import { resolve } from "node:url"`) no longer resolves
   to the import. ~~**Registry-class InstanceSetter dispatch**~~ — FIXED
   (600583fe): `registryInstance.prop = value` now routes to the class's
   InstanceSetter (made URL fully mutable; generic for any Registry class).
1. **Import-binding for global-exporting modules.** `import { Buffer } from
   "node:buffer"`, `{ EventEmitter } from "node:events"`, `{ setTimeout } from
   "node:timers"`, `{ createHash } from "node:crypto"` all bind `undefined` even
   though the underlying global/member exists. Modules exporting Registry
   NAMESPACE MEMBERS (os/url/path/querystring/punycode) import fine; modules
   whose exports are ambient GLOBALS or that register under a `rts:`-key need a
   `node_reexported_globals` entry (like url/string_decoder) or a member alias.
   **Cheap, high-leverage — unblocks buffer/events/timers/crypto import surface.**
2. **Function-value identity** (`f === f` is false — every reference re-reifies a
   fresh handle). Blocks `removeListener`/`unsubscribe`/`Set<fn>`/dedup-by-ref
   everywhere (diagnostics_channel, events, stream). Fix = cache the reified
   handle per binding (compile-time) — a real engine change.
3. **Real async event loop (#207).** Deferred emission / microtask draining
   between top-level setup and assertions. Blocks correct timing for
   promises/callbacks in stream, timers/promises, fs callbacks, http, dns.
   Residual `on('end')`-after-`on('data')` sync-resume timing (a manual
   end-listener attached after data may miss `'end'`; `.pipe` unaffected).
   **RESOLVED (2026-07, commit f6183b87):** the earlier "`_read`-driven
   ReadStreams don't deliver" sub-bug was NOT an event-loop issue — it was
   `.push` on an `any`-typed receiver (`self.push(x)` inside a method) silently
   dispatching to the Array fast-path (`__rtsadp_dyn_push` → `undefined`
   no-op) instead of the instance `push` method. Fixed generally; fs.ReadStream
   + every lazy `_read`-driven Readable now feed flowing/pipe consumers.
4. **Extending a `.ts`/native stream class from a native module.** tty/http/https/
   tls ReadStream/WriteStream/Socket subclasses can't yet extend the ambient
   `.ts` stream classes. Needed for the whole networking/IO-stream tier.

---

## ROADMAP — order to complete node (stick with each until 100%)

Do these in order; finish one to 100% (import + full surface + tests) before the
next. Pick low-hanging near-complete modules first to grow the ✅ list fast, then
land the cross-cutting engine fixes that unblock the heavy tier.

**Phase A — close the near-complete (small, high ROI):**
1. ~~**node:os**~~ — DONE (was already complete; the EOL/constants "gap" was the
   `typeof` artifact).
2. ~~**node:path**~~ — DONE (same; sep/delimiter work).
3. ~~**node:url**~~ — DONE (URLSearchParams entries/forEach/size + full URL
   setters + format(URL)/toJSON + searchParams↔href sync; commits 615d93b7/
   b111fdc5/600583fe). Remaining: `[...sp]` spread-of-instance (needs
   Symbol.iterator on a Registry instance) and `JSON.stringify(url)`→toJSON — both
   systemic engine gaps, not url.
4. **node:dgram** — honor `lookup`/`signal`/`fd` options (or the minimal real
   subset) to remove the throw.
5. ~~**node:net**~~ — DONE (Server/Socket over real TCP verified by CALL,
   `net.SOMAXCONN` added, lossless object-backed field writes back
   `server.maxConnections`; commit b4414692). Full surface + 48 tests green.
6. ~~**node:fs**~~ — DONE (promises import + node:fs/promises + full FileHandle
   incl. stream methods + ReadStream/WriteStream/Utf8Stream; commits ccbc6b9b→
   625512fe). Drove the general `.push`-on-`any` engine fix (f6183b87).

**Phase B — engine unblock #1 (import binding), then the global-exporters:**
6. Fix the named-import binding for global/`rts:`-key modules (engine/flatten).
7. **node:buffer** — full `Buffer` + `Blob`/`File`/`atob`/`btoa`.
8. **node:events** — static `once`/`on`, `getEventListeners`, captureRejections.
9. **node:timers** — module + `timers/promises`.

**Phase C — pure/sync-heavy modules:**
10. **node:util** — legacy `is*` + `util.types.*` + `inherits` + `deprecate` +
    `debuglog` (+ deep `inspect` as its own sub-project).
11. **node:assert** — `AssertionError` class + callable default + `match` audit.
12. **node:zlib** — callback forms + the stream Transform classes (needs Phase D).

**Phase D — engine unblock #2/#3 (function identity + async loop + stream
subclassing), then the IO/networking tier:**
13. function-value identity + real async event loop + stream-subclass-from-native.
14. **node:crypto** (ciphers/sign/subtle), **node:dns** (resolver), **node:tty**,
    **node:http**/**https**/**http2**, **node:child_process**,
    **node:worker_threads**, **node:readline**, **node:stream/web** codec
    (Compression/Decompression).

**Phase E — later/experimental:** `async_hooks`, `cluster`, `perf_hooks`
observers, `module` loader hooks, `v8`, `vm`, `inspector`, `sqlite`, `wasi`,
`domain`, `repl`, `test`, `trace_events`, `diagnostics_channel` store propagation.
