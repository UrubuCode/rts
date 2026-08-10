# Node.js compatibility — `rts-node`, measured

What the new engine answers when a program written for Node runs on it.

**Every number here was produced by running a program**, not by reading Rust.
That distinction is the whole point of this file: a module that exports the
right forty names and answers none of them correctly reads as complete in the
source and fails in the first minute of use. Several modules below are exactly
that, and the source could not have told us which.

---

## How this was measured, and how to redo it

```bash
cargo build -p rts-host --example run_fixture
target/debug/examples/run_fixture.exe probe.ts
```

`run_fixture` runs one file through the new engine in its own process — an
abort kills the child rather than the harness, so one crash does not blind the
rest of the measurement.

Each module was probed three ways:

1. **Enumeration** — `Object.keys(namespace)` and the prototypes of every class
   it exports, diffed against the Node 22 documented surface.
2. **Behaviour** — every present export called with realistic arguments, and the
   *answer* checked. A function that exists and returns `undefined` counts as
   missing, not present.
3. **Round trip, where a round trip is the point** — bytes across a socket,
   a message across a thread, a digest against a published test vector, a
   compressed buffer back to its original.

Date of measurement: **2026-08-08**, debug build, Windows 11, `main` at
`4e46dfe9`.

---

## The headline is not a module

Five engine-level traits cost more Node compatibility than every missing export
in this document combined. They are listed first because fixing any module
below without fixing these produces a module that still cannot be used.

### 1. Nothing throws

```
assert.ok(0)          → prints "AssertionError [ok] #1: ..." and CONTINUES, exit 0
o.doesNotExist()      → does not throw, exit 0
fs.readFileSync(missing) → returns undefined
```

`try { … } catch (e) { if (e.code === "ENOENT") … }` is inoperative across the
whole of `fs`. `assert` is a logger, not an assertion library. Calling a method
that does not exist is silent, which means every "missing export" in this
document presents to a program as `undefined` rather than as an error.

This is the same defect already recorded as the runtime throw-discipline
problem: `raise` works and unwinds, and no native checks `thrown`.

### 2. `import x from "node:fs"` is `undefined`

Default import returns nothing for every `node:` module. Only
`import * as x from "node:fs"` and named imports work. This is the most common
import form in real Node code, so the failure is hit before any API is reached —
and, per trait 1, it fails silently.

### 3. An unregistered specifier is indistinguishable from an empty one

```
import * as t from "node:stream/web";        → 0 keys, exit 0
import * as b from "node:totally_bogus_xyz"; → 0 keys, exit 0
```

(The example was `node:timers/promises` when this was measured; that one is
registered now, and the trait it demonstrates is unchanged.)

Module resolution never fails. Eleven real Node specifiers are absent (table
at the end) and every one of them imports "successfully" into a namespace with
no members.

### 4. `console.log` drops arguments past the fourth, and does not inspect

```
console.log("a","b","c","d","e","f","g") → a b c d
console.log({a:1,b:{c:[1,2,3]}})         → [object]
console.log("%s|%d","str",42)            → %s|%d str 42
```

`String({a:1})` correctly gives `[object Object]`, so the `[object]` is
console's own stringifier, not the coercion. This is the highest-impact wrong
answer in the audit because every other defect is diagnosed through it. Also
missing entirely: `table`, `dir`, `group*`, `time*`, `count*`, `assert`,
`trace`, `clear`.

### 5. String literals are unescaped twice

```
"C:\\a\\b"  → length 4, charCodeAt(3) === 8 (backspace)   // Node: length 6
"a\\nb"     → length 3                                     // Node: length 4
"\\"        → length 1, code 92                            // correct
```

`C:\\a\\b` is unescaped to `C:\a\b` and then unescaped **again**, eating `\a`
and turning `\b` into a backspace. A lone `\\` survives because the second pass
finds nothing to consume. This is a compiler defect, not a module one, and it
falsified the first reading of `node:path` and `node:url` in this very audit —
both looked broken on Windows until the probes were rewritten with
`String.fromCharCode(92)`.

### Two hard crashes

| what | result |
|---|---|
| `http.get(...)`, `http.request(...)`, `res.setHeader(...)` | `RefCell already borrowed` at `rts-core/src/entry/current.rs:93`, non-unwinding panic, exit 127 |
| `tty.isatty(99)` (invalid fd) | exit 127, no output at all |

The HTTP one is a reentrant `with_runtime`: `http::outgoing::set_header` →
`with_runtime` → `http::common::key` → `with_runtime` again. It makes every
HTTP client path unreachable.

---

## Score

Two honest numbers, because one number would hide the thing that matters.

**Surface:** ~462 of ~818 documented Node 22 exports are present across the 43
module specifiers audited — **≈56%**. This counts presence only.

**Behaviour:** of 43 module specifiers,

| state | count | meaning |
|---|---|---|
| complete | 3 | surface and behaviour both match Node |
| near-complete | 6 | one known divergence, usable |
| partial | 20 | core works, significant holes or wrong answers |
| skeleton | 12 | classes exist, the thing the module is *for* does not work |
| crashes | 2 | aborts the process on a normal call |

A program that only reaches the first nine rows runs. Anything reaching
`stream` flowing-mode, `http`, `net` I/O, `assert` throwing, or `console`
formatting does not.

---

## Per module

State legend: **completo** · **quase** (near-complete) · **parcial** ·
**esqueleto** · **crash**.

### Complete

| module | exports | notes |
|---|---|---|
| `os` | 23/23 | every export verified, including `cpus()`, `networkInterfaces()`, `userInfo()` in the exact Windows shape, `constants`. No divergence found. |
| `punycode` | 6/6 | round trips verified incl. emoji (`toASCII("😀.com")` → `xn--e28h.com`); `ucs2.decode` returns code points, correct. |
| `string_decoder` | 1/1 | tested at exactly the point that defines it — multi-byte sequences split across chunk boundaries in utf8, utf16le and base64. All correct. The most correct module in the crate. |

### Near-complete

| module | exports | the one divergence |
|---|---|---|
| `path` | 16/16 | the **default export is bound to posix on win32**: `path.sep === "/"`, `path.delimiter === ":"`. `path.win32.*` is entirely correct. |
| `querystring` | 6/6 | `stringify({a:null,b:undefined})` → `"a=null&b=undefined"`; Node → `"a=&b="`. Also `decode !== parse` (in Node they are the same function object). |
| `zlib` | 38/44 | one-shot and callback forms round-trip byte-exact for gzip/deflate/deflateRaw/brotli/unzip; `crc32("abc")` correct; `{level}` honoured. **The stream forms produce nothing** — `createGzip()` has only `close/flush/reset`, no `data`/`end` events, no `pipe`. Missing: the whole zstd family, `codes`, top-level constant aliases. |
| `perf_hooks` | 13/17 | `monitorEventLoopDelay` genuinely measures. `Histogram.percentile()`/`.percentiles` are `undefined` — the canonical use of a histogram (p50/p99) is absent. |
| `diagnostics_channel` | 5/6 | `channel`/`publish`/`subscribe`/`hasSubscribers` and `tracingChannel.traceSync` all correct. Missing the `Channel`/`TracingChannel` class exports and `traceCallback`. |
| `dgram` | 2/2 | **real UDP round trip verified** — bind, `listening`, `address()`, send, `message` with correct `rinfo`. But the delivered Buffer's `toString()` returns `"[object Object]"`, and a socket cannot send to itself. |

### Partial

**`buffer`** — 13/14 module exports, 25/~50 prototype methods. All seven
encodings correct (including the ascii high-bit strip). `Buffer.from(ArrayBuffer)`
**returns an empty buffer** (Node gives a zero-copy view). The numeric accessors
present are correct but the set is arbitrary — only LE float/double, no signed
8/16-bit, no BigInt widths, no `swap16/32/64`, no `lastIndexOf`, no iterators.

**`url`** — 11/13. Parsing is genuinely good: relative resolution, IPv6, default
port normalisation, IDN, percent-encoding. But **every `URL` setter is inert** —
components are data properties, not accessors, so `u.pathname = "/q"` never
reserialises `href`. `URLSearchParams` is **not iterable** (`Array.from` → 0).

**`util`** — 17/29, and **`promisify` is absent**. That is the single
highest-damage missing export in the crate: any module doing
`const x = util.promisify(fn)` dies immediately. `parseArgs` returns `undefined`.
`inspect` is one depth level short, renders Map/Set as `{}`, and loses function
names. `util.types` has 3 of ~33 predicates.

**`fs`** — 47/110. The sync API is broad and mostly right. Beyond trait 1
(no I/O error ever throws): **the entire callback API is absent** (42 functions),
`writeSync`/`readSync`/`writevSync`/`readvSync` all return 0 and move no bytes,
`readFileSync` without an encoding does not return a Buffer, `Stats` has only
the `*Ms` fields (no `atime`/`mtime` Date getters), `readdirSync({recursive})` is
ignored, and no stream or class is exported (`createReadStream`,
`createWriteStream`, `Stats`, `Dirent`, `Dir`, `ReadStream`, `WriteStream`).

**`fs/promises`** — 31/45. `open()` returns a real FileHandle whose `read`,
`readFile`, `writeFile`, `stat`, `truncate`, `sync`, `close` work.
`FileHandle.write(string)` rejects with the value `undefined` — not even an
Error. Missing `glob`, `opendir`, `watch`, `constants`.

**`process`** — 29/65. `env` (read/write/delete), `cwd`/`chdir`, `hrtime` +
`hrtime.bigint`, `nextTick` ordering, `stdout.write`, EventEmitter surface all
work. `version` is `"v0.1.0"` and `versions` is `{node, rts}` — version-sniffing
code misfires. **`memoryUsage` and `cpuUsage` are absent**, which is what every
benchmark and diagnostic tool calls first.

**`events`** — 3/9. `EventEmitter` itself is solid: ordering, `prependListener`,
removal during `emit`, `rawListeners`, `eventNames` all correct. But an `'error'`
with no listener **aborts the process** with the message erased
(`uncaught 'error' event: an object`) instead of throwing, no
`MaxListenersExceededWarning` is ever emitted, and the promise helper `once()`
and async-iterator `on()` are both missing.

**`stream`** — 8/~25, and the split is sharp: **pull mode works, flowing mode
does not exist**. `read()`, `push`, `objectMode`, backpressure with real
`'drain'`, `pipe()`, `Transform`, `PassThrough` all move data. But the `'data'`
event **never fires**, `'end'` never fires, `finished()`'s callback never runs,
`pipeline()` moves the data but never calls back, and there is no
`Symbol.asyncIterator` — so `for await (const chunk of readable)` is impossible
from both sides (the compiler also refuses `for await`). Missing all of
`map/filter/take/reduce/toArray`, the web-stream bridges, and `compose`.

**`timers`** — 6/9. **`setInterval` fires exactly once** — it behaves as
`setTimeout`, for both the global and the module version. `timers.setImmediate`
is synchronous (the global one is correctly ordered). The returned handle is a
`number`, so `unref`/`ref`/`refresh` do not exist and no timer can be unref'd.

**`async_hooks`** — 6/8. `AsyncLocalStorage.run` nests correctly and **survives
an `await`**. It does *not* survive a `setTimeout` or a `.then()` — `getStore()`
returns `undefined` in both. `executionAsyncId()` is always 0 and `createHook`
callbacks never fire.

**`crypto`** — 14/78. **Hashes and HMACs are correct** — all nine algorithms in
`getHashes()` match their published `"abc"` vectors, and HMAC md5/sha1/sha256
match theirs. Then:

- **`pbkdf2Sync` ignores the `digest` argument.** `sha1` and `sha256` return
  byte-identical output, and neither matches RFC 6070:
  ```
  pbkdf2Sync("password","salt",1,20,"sha1")   = 120fb6cf…a86548c9
  pbkdf2Sync("password","salt",1,20,"sha256") = 120fb6cf…a86548c9
  RFC 6070 expects (sha1)                     = 0c60c80f…2fe037a6
  ```
- **`scryptSync` is not scrypt and discards N/r/p.** `{N:16,r:1,p:1}` and
  `{N:1024,r:8,p:16}` produce identical bytes; neither matches RFC 7914.
- **`hkdfSync` returns 0 bytes.**

A key derived here opens nothing derived by Node, and the scrypt cost parameter
is decorative. Absent entirely: all ciphers, all signing, all key objects, DH,
ECDH, X509, `webcrypto`/`subtle`, and every async variant.

**`v8`** — 8/21. **`serialize()` returns its own input** — not a Buffer, the
same object. Every round-trip test passes trivially because nothing is
converted, and nothing can be written to a file or sent to a worker.
`getHeapStatistics()` returns 5 of Node's 14 fields.

**`sqlite`** — 2/3, and the core genuinely works: in-memory and on-disk
databases, `exec`, `prepare`, `all`/`get`/`run` with correct types and
`lastInsertRowid`, persistence across close/reopen. **Named parameters are never
bound** — `$n`, `:n`, `@n` and bare all come back `null`; only positional `?`
works. Invalid SQL does not throw.

**`net`** — 14/14 exports and **no I/O**. `isIP*`, `BlockList` and
`SocketAddress` are correct end to end. But `net.Socket.prototype` **is
`dgram.Socket.prototype`** (verified by identity) — it carries `bind`/`send`/
`addMembership` and lacks `setNoDelay`/`setKeepAlive`/`remotePort`.
`server.address` is `undefined`, `listen()`'s callback never fires, `listening`
never fires, and a loopback TCP echo moved **zero bytes in both directions with
no error and no hang**.

**`worker_threads`** — 13/18, one direction only. A worker really runs on a real
thread: `workerData` arrives intact, `parentPort.postMessage` reaches the main
thread's `message` handler, `exit` fires with the right code, `kill()` works.
But **`Worker#postMessage` does not exist**, so main→worker is impossible and
the worker's own `message` handler can never fire; **`terminate` does not
exist**; and **file-path workers do not work at all** — only `{eval:true}`
sources, because there is no module loader. `MessageChannel`, `MessagePort`,
`BroadcastChannel`, `SharedArrayBuffer` and `Atomics` are all absent as globals
too, so shared-memory interop is not untested but impossible.

**`child_process`** — 5/8. `execSync`, `execFileSync` and `spawnSync` do real
round trips with correct stdout, stderr and exit status. But **`spawn()` gives
the child no `stdout`/`stderr`/`stdin`** — stdio is inherited by the parent
instead of piped; `execSync` does not throw on non-zero exit; the `cwd` option
fails with `os error 267`; and `exec`, `execFile` and **`fork`** do not exist,
so there is no IPC at all.

**`cluster`** — 13/14. `fork()` really spawns an OS process that re-runs the
script with `isWorker === true` — correct Node semantics. But no cluster event
ever fires (`fork`, `online`, `exit` all silent), `worker.send` is `undefined`,
`cluster.worker` is never populated in the child, and **`process.env` mutations
are not inherited by children**. That last one turned an ordinary guard variable
into an unbounded fork chain during the audit. Treat `cluster.fork()` as
dangerous until fixed.

**`assert`** — 14/19. The comparisons themselves are right — `deepStrictEqual`
handles nesting, Date, RegExp, type distinction and circular refs. But per trait
1 **a failed assertion does not throw**, `new AssertionError({...})` returns an
empty object that is not an `instanceof Error`, Map/Set are always unequal, and
`throws`/`doesNotThrow`/`rejects`/`doesNotReject` do not exist.

**`module`** — 13/18. `createRequire` genuinely resolves builtins;
`isBuiltin` and `builtinModules` (42 names) work. Five exports are no-ops
returning `undefined`: `stripTypeScriptTypes`, `register`, `registerHooks`,
`findPackageJSON`, `enableCompileCache`. `Module` is an object, not a class —
no `prototype`, no `_load`/`_resolveFilename`/`runMain`.

**`inspector`** — 5/6, and honest about its limits. `Runtime.evaluate` really
evaluates (`"1+1"` → 2), `Runtime.getHeapUsage` returns real numbers, and
unsupported CDP methods return an explicit error rather than lying:
`"Profiler.start needs a sampling profiler, which this engine does not have"`.
`node:inspector/promises` is the same callback namespace — `await session.post()`
resolves to `undefined`.

**`dns`** — 10/~50. `lookup("localhost")` works in callback and promise form.
`getServers()` returns `[]`. Everything else — `Resolver`, all `resolve*`,
`reverse`, `lookupService`, every error constant — is absent.

**`http2`** — 6/11. `constants` and `getDefaultSettings()` are correct and
complete. Nothing else runs: `listen`'s callback never fires, and an h2c
loopback produced no `stream`, no `response`, no `error` — silence.

### Skeleton

| module | exports | what does not work |
|---|---|---|
| `console` | 5/23 | see trait 4. `new Console(out, err)` does write to a custom sink — that part is real. |
| `domain` | 2/2 | `d.run()` **does not trap errors** — a throw inside it aborts the process with the `error` handler registered. `domain.active` is never set. There is no isolation at all. |
| `vm` | 5/10 | evaluates numbers and booleans only — strings, objects and arrays come back `undefined`. **The context object is invisible to the code** (`runInNewContext("x*2",{x:21})` → `undefined`). Multi-statement programs return `undefined`. A throw inside is not catchable and aborts. |
| `wasi` | 1/1 | the instance is an **empty object** — no `wasiImport`, `start`, `initialize`, `getImportObject`. Doubly untestable: `globalThis.WebAssembly` is `undefined`. |
| `readline` | 5/9 | **`line` events never fire** and `question()`'s callback never runs, from a stream or from real piped stdin. Only the cursor/ANSI helpers do work. `readline/promises` is empty. |
| `repl` | 2/8 | prints the prompt, evaluates nothing — downstream of readline never reading. |
| `test` | 8/15 | **no TAP output and no reporting at all** — a passing file and a failing file are indistinguishable. A throwing test aborts the whole process. `after()` never runs. The context object `t` is `undefined`, so subtests, `t.assert` and `t.mock` are unreachable. |
| `tls` | 9/15 | `getCiphers()` returns 2 entries (Node: ~150); `rootCertificates` is **empty**, so no chain could ever verify. `TLSSocket`, `Server`, `SecureContext` are not exported. |
| `https` | 6/6 | `listen` never fires; `https.get` to a closed port emits `error` synchronously, before a handler can attach. No handshake is reachable. |
| `trace_events` | 2/2 | category bookkeeping only — **no trace event is ever produced**, no file, no perf_hooks integration. |
| `http2` | 6/11 | listed above under partial for its correct constants; behaviourally a skeleton. |
| `dns` | 10/50 | listed above; only `lookup` exists. |

### Crashes

| module | trigger |
|---|---|
| `http` | `http.get`, `http.request`, `res.setHeader` → `RefCell already borrowed`, exit 127 |
| `tty` | `isatty(<invalid fd>)` → exit 127, silent |

---

## Specifiers

49 of 60 Node 22 specifiers are registered. Per trait 3, the 11 absent ones do
not fail to import — they yield an empty namespace, exactly like a nonexistent
module.

**Absent:** `assert/strict`, `constants`, `dns/promises`, `quic`,
`readline/promises`, `sea`, `stream/consumers`, `stream/promises`, `stream/web`,
`test/reporters`, `util/types`.

**`node:timers/promises` landed on 2026-08-10** — `setTimeout`, `setImmediate`,
`setInterval` (as an async iterable), `scheduler.wait` and `scheduler.yield`,
each honouring `options.signal`. `tests/node_timers_promises.test.ts` is what
says so: 8 assertions, run. It shares `node:timers`' own queue rather than
owning a second one, so `setTimeout(cb, 5)` and `await setTimeout(5)` have one
order; `crates/rts-node/src/timers/promises.rs` states the two divergences
(an abort is noticed on the next turn of the loop, not at the call, and an
interval's tick is scheduled per `next()`).

`sys` was listed absent here and is registered — it is `node:util` under its
deprecated name, and `lib.rs` names the same object twice on purpose. The row
was wrong rather than the code.

Of what is left, `stream/web`, `stream/promises`, `stream/consumers`,
`util/types` and `dns/promises` are idiomatic in modern Node code and cost the
most.

`node:inspector/promises` is registered but resolves to the callback namespace,
so it is present in name only.

---

## What to fix first

Ordered by how much Node code each unblocks, not by effort.

1. **Make things throw.** Trait 1. It is one decision in the runtime and it
   converts `fs`, `assert`, `child_process`, `sqlite` and every "silently
   returns undefined" line above from wrong answers into correct ones.
2. **Default import.** Trait 2. One resolver change; unblocks the most common
   import form in the ecosystem.
3. **`console.log`** — the 4-argument truncation and object inspection. Trait 4.
   Everything else is debugged through it.
4. **The `RefCell already borrowed` in `http`.** A reentrant `with_runtime`;
   until it is fixed no HTTP client exists.
5. **`stream` flowing mode** — `'data'`, `'end'`, and `Symbol.asyncIterator`.
   `zlib`'s streams, `readline`, `repl`, `http` bodies and `child_process` pipes
   are all downstream of this one missing mechanism.
6. **`util.promisify`.** One function, and a large fraction of real modules call
   it at load time.
7. **`net` I/O and `server.address()`.** `http`, `https` and `http2` cannot work
   before `net` moves a byte.
8. **`crypto` KDFs** — `pbkdf2` honouring `digest`, a real `scrypt`, a
   non-empty `hkdf`. These currently return confidently wrong bytes, which is
   worse than returning none.
9. **String literal double-unescape.** Trait 5 — a correctness bug that also
   makes every Windows-path test lie.
10. **`cluster.fork()` env inheritance**, because the current behaviour
    fork-bombs a machine on an ordinary guard pattern.

---

## What this file does not claim

- No module was tested against the public internet. `dns` resolution beyond
  `localhost`, real TLS handshakes and `https` requests are **untested**, and
  `tls`'s empty `rootCertificates` means they would fail regardless.
- `wasi` is untestable here for two independent reasons (no `wasiImport`, no
  `WebAssembly` global).
- Export counts for Node's side are taken from the Node 22 documentation and
  are approximate at the margins (deprecated and undocumented members are
  excluded). The counts for our side are exact — they came from
  `Object.keys`.
- Everything measured in a **debug** build. Nothing here is a performance claim.
