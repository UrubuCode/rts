# `node:` on the new engine — what exists, what does not

**Measured, not remembered.** The two lists come from `ls docs/reference/node/`
and from the `install` table in `crates/rts-node-rwk/src/lib.rs`. Regenerate
rather than edit from memory:

```bash
ls docs/reference/node/*.md | xargs -n1 basename | sed 's/\.md$//' \
  | grep -vE "architecture|crates|implementation-plan|INDEX|layering|node_completed|rts-std-migration|globals|STATUS" | sort > /tmp/ref.txt
grep -oE '^\s+\("([a-z_]+)"' crates/rts-node-rwk/src/lib.rs | tr -d ' ("' | sort -u > /tmp/done.txt
comm -23 /tmp/ref.txt /tmp/done.txt
```

**39 of 42 documented modules are registered**, and `node:http2` is now a working client and server rather than framing alone. A module being registered means a
program can import it and call what it provides — it does NOT mean the surface is
complete. Each module's own doc carries a "Not implemented, by name" section, and
that is the authority on its gaps; this file is about which modules exist at all.

---

## Registered

`assert` · `async_hooks` · `buffer` · `child_process` · `crypto` ·
`diagnostics_channel` · `dns` · `events` · `fs` (+ `fs/promises`) · `net` ·
`os` · `path` · `perf_hooks` · `process` · `punycode` · `querystring` ·
`stream` · `string_decoder` · `test` · `timers` · `tty` · `url` · `util` · `v8` · `zlib` ·
`console` · `dgram` · `http` · `https` · `module` · `readline` · `tls` ·
`cluster` · `domain` · `repl` · `sqlite` · `worker_threads` · `trace_events` · `vm` · `wasi` · `http2`

`Buffer` and `console` are not modules: `Buffer` is a class in the runtime
(`rts-core-rwk`, where `layering.md` puts it) and `console` is a global installed
by `rts-std-rwk`.

## Not registered, and what each waits on

Ordered by what unblocks the most rather than by size.

| module | doc | waits on |
|---|---|---|
| `inspector` | 1072 | a debugger protocol over a socket, and a debugger to speak it. |

## No defect is pinned as unfixed any more

Both that were are fixed.

**`setTimeout(f, 0)` alone now fires.** The note on it said "nothing pumps" was
not the cause, which was true and stopped one step early: a delay of `0` is
clamped to `1`ms exactly as Node clamps it, and the host's single end-of-turn
pump ran microseconds after the schedule, so the timer was not due. Pumping once
can only fire what is already due. `timers::drain` adds the waiting an event loop
does — pump, sleep to the nearest deadline, repeat — and an interval deliberately
does not hold a program open, because the alternative is every fixture with a
stray interval hanging.

Finding it exposed a second, worse bug: the timer table was one process-wide
`Mutex`, so one thread's pump fired another thread's callback with handles naming
cells in a region it does not have. It is thread-local now, like the context and
for the same reason.

`builtinModules` was the other and is fixed: it was a member FUNCTION where Node
has a data property holding an array, so a program reading its `length` got a
function's arity. It is built once in `module::namespace` from the same list
`isBuiltin` answers from.

## The rule every module here pays

`with_runtime` holds a `RefCell` borrow for its body. An ambient helper or entry
point called inside one is a nested borrow, and an `extern "C"` frame cannot
unwind — so it **aborts the process** rather than failing. Nine such aborts have
been found across four modules; every one was this.

The context-taking pairs are in `crates/rts-core-rwk/src/entry/modules.rs`:
`undefined_in`, `null_in`, `text_in`, `make_object`, `make_array_in`,
`set_prototype_in`, `is_object`, `make_number`, `bytes_of`, `make_buffer`,
`settled`, `make_prototype`, `make_instance`. `deep_copy` is the one exception
and its doc says why: its walk takes and releases its own borrows, so it must
**not** be called from inside one.

A helper that can only be called correctly beats one that must be — where a
helper is always called from inside a borrow, change its signature to take the
context rather than wrapping each call. That is what `node:stream`'s fix did.

---

## One ordering that is load-bearing, not tidy

`install` builds `events` and `stream` BEFORE the table it iterates.
`make_prototype` is idempotent **by name**, so whoever asks for `"EventEmitter"`
first decides what is on it — and a module that merely chains onto it asks with
an empty member list. When `console` and `dgram` joined the table, they sorted
ahead of `events`, won the name, and `emit` silently reached no listener.

Caught by the fixture that pins `emit`, not by a compiler. Any module added here
that chains onto a base must either sort after it or the base must be built
first, and building first is the answer that does not depend on a name.

## `node:tls` — what the provider covers, and the one gap that matters

Built per `crates.md` §6 option (b): a `rustls::CryptoProvider` assembled from the
RustCrypto crates `node:crypto` already uses. No `ring`, no `aws-lc-rs`, no C.

Covered: TLS 1.3, AES-128-GCM and ChaCha20-Poly1305, ECDSA-P256/Ed25519/RSA-PKCS1
verification, ECDSA-P256 and Ed25519 signing.

X25519 is covered and PREFERRED, with P-256 kept as the fallback so a peer that
only speaks it still works. It was absent for one commit, and the reason is worth
keeping: the provider was written while `x25519-dalek`'s `static_secrets` feature
was off, every constructor taking raw secret bytes is gated behind it, and P-256
was substituted and REPORTED rather than the dependency being reached around.
Three wrong answers were available — a `ring`-backed path, a new dependency, and
a secret derived by hand from a hash — and the third would have compiled,
handshaken, and been broken cryptography. The secret's 32 bytes come from
`getrandom`, the same call the rest of the provider uses.

Also absent: TLS 1.2 entirely, AES-256-GCM, P-384, RSA-PSS, RSA signing, and
encrypted PKCS#8 keys. PEM and PKCS#8 are read by hand — `rustls-pemfile`,
`pkcs8` and `der` are not dependencies, and `node:crypto` declined asymmetric-key
work for the same missing infrastructure, which is corroboration rather than
coincidence.

## Three modules that are mostly refusals, and why they were still worth writing

`cluster`, `domain` and `trace_events` are registered and each is honest about
being a shell around a mechanism that does not exist. That turns a program
failing at an arbitrary later call into one failing at its import, where the
cause is legible.

- **`cluster`** forks real processes through `child_process`, and there is no
  IPC and no handle passing — so workers cannot share a listening socket, which
  is what `cluster` is actually FOR. It is process spawning wearing the name.
- **`domain`** has a real `enter`/`exit`/`active` stack, and `run` cannot catch a
  throw, because a native cannot catch one crossing back through it. Catching is
  the module's entire value proposition, so what is left is bookkeeping.
- **`trace_events`** tracks enabled categories correctly and emits nothing: there
  is no tracing sink anywhere in the engine.

Each says this at the top of its own module doc rather than only here.

## `node:http2` — a working client and server over h2c

Done and pinned by 37 unit tests against the RFCs' own vectors: the 9-byte frame
header, the connection preface, and `SETTINGS`/`HEADERS`/`DATA`/`WINDOW_UPDATE`/
`RST_STREAM`/`GOAWAY`/`PING`/`PRIORITY`; and HPACK (RFC 7541) in full — static
table, integer and string primitives, dynamic table with byte-budgeted eviction,
all four header-field representations, checked against C.6.1's worked example.

Huffman DECODING works and is checked against C.4.1/C.4.2. Huffman ENCODING does
not exist and the encoder writes literals. That asymmetry is deliberate: a peer
may Huffman-encode whatever this side emits, so decoding is mandatory and
encoding is not. Getting it backwards yields a client that works until a server
compresses.

`connect`, `createServer`, the session and stream classes: **built**, and built
with the three things their absence was justified by. `session.rs` holds the
frame-dispatch loop, real connection- and stream-level flow control, and the
rapid-reset mitigation (CVE-2023-44487) the spec calls mandatory — a budget of
resets per connection that never decays, because a window that forgives lets an
attacker pace itself under it forever. Nine unit tests run a real client against
a real server through two byte buffers, with no socket and no heap; an
end-to-end fixture runs both over a real TCP socket.

**h2c only, and `createSecureServer` is deliberately absent.** HTTP/2 over TLS is
chosen by ALPN, which `node:tls` here does not have — a client that opened a TLS
socket and started sending frames would be talking to a server that believes it
agreed to HTTP/1.1. `connect("https://…")` refuses by name.

Also absent: `PUSH_PROMISE` and every `pushStream`, the
`Http2ServerRequest`/`Http2ServerResponse` compatibility API (`createServer`
raises `'stream'` and nothing else), trailers, and `CONTINUATION`.

The loop taught this module the distinction it needed: a session with a live
stream answers `Pending::In`, because a response is coming and the program asked
for it, while an idle listening server answers `Blocked`. Both were `Blocked` at
first, and the fixture caught it at once — nothing held the loop, so the client
got one pass and the response the server had already sent was never delivered.
The mirror of that mistake is a stream never forgotten, which holds the loop open
forever; that hung the same fixture until `end_stream` removed it.

## `node:vm` and `node:repl` — registered, and the two things that made them work

Both are built on `entry::evaluate`. Both were written and left unregistered
because calling it from inside a running program ABORTED, and two separate
defects were behind that.

**The context slot became a stack.** Installing a context OVERWROTE what was
there, so the inner program destroyed the caller's heap and the first entry point
after the evaluation found none. The rejected alternative was making the
evaluator refuse re-entry: cheaper, and it removes the abort by removing the
feature. A stack is also the shape `worker_threads` needs for a second context.

**The evaluator tries the expression form first.** `compile` wraps a script in a
function and a function reaching its end answers `undefined`, so there was no
completion value — `runInNewContext("1 + 2")` answered `undefined` with nothing
wrong anywhere. `evaluate_source` compiles `return (source);` and falls back to
the plain form, which is what keeps `let x = 1; x` compiling. Making the wrapper
return its last expression statement for EVERY program was rejected: it changes
what `compile` means for the suite to fix what one seam asks for.

A reference still does not cross — it belongs to the region that made it — and a
fixture pins that alongside the value crossing and the caller's heap surviving.

## `node:wasi` — a different shape, named as one

There is no `WebAssembly` global in this engine — checked, not assumed. Node's
`node:wasi` hands its import object to `WebAssembly.instantiate`, so with no such
function there is nothing to hand it to.

So `start(bytes)` takes the module's raw bytes and runs them through `wasmi`
itself. `getImportObject`/`wasiImport` still exist with the right shape and key,
and their functions are inert: the real host calls are wired into `wasmi`'s
linker where nothing in JavaScript can reach them. That is a divergence from
Node's API and the module doc says so in its second paragraph rather than
implying parity.

**No filesystem access is ever granted.** `preopens` is parsed and stored and
never consulted; every `path_*` call answers `ENOTCAPABLE`/`EBADF` regardless of
what was configured. Refusing is safe and granting by accident is not, so the
refusal is unconditional rather than conditional on a table being right.

Real preview1 calls: `args_*`, `environ_*`, `clock_time_get`, `random_get`,
`proc_exit`, `sched_yield`, `fd_write` (stdout/stderr), `fd_read` (stdin),
`fd_close`. A module importing anything not linked fails at `instantiate` — a
named failure rather than a silent one.

A unit test hand-assembles a WASM binary byte by byte and runs it through the
real `wasmi` path: it writes to stdout and exits with 7, and the test asserts the
7. That is worth more than any claim in this file.

## `node:sqlite` — and the one value that does not cross cleanly

Over `turso_core`, pure Rust and SQLite-file-compatible. NULL, TEXT, REAL and
BLOB round-trip: BLOB is a `Buffer` in both directions.

INTEGER crosses as a `number` while a double holds it exactly, and as a BIGINT
when it would not. It silently rounded for one commit, and the fix was not in the
module: the runtime's only bigint constructor parsed TEXT and took the ambient
borrow, so a native holding the context could not call it. `make_bigint` is the
context-taking pair that removed it — the seventh of that shape.

`setReadBigInts` is still accepted and still does nothing: it asks for EVERY
integer as a bigint, and what happens instead is that only the ones that need to
be are.

Binding the other way: a whole `number` in `i64` range becomes INTEGER, and a
unit test pins the trap that `i64::MAX as f64` rounds UP past the range.

## `node:worker_threads` — a real thread running a real engine

A `Worker` starts an OS thread, and that thread calls the host's evaluator: it
compiles the source, installs its own context, its own region and its own copy of
every module, runs to the end and goes away. Nothing is shared — not a heap, not
a cell, not a lock. It could not be written before the context stack, because a
second context could not exist.

**What crosses is a copy.** A reference belongs to the region that made it, so
`worker_threads/portable.rs` carries `undefined`, `null`, booleans, numbers,
strings, arrays and plain objects, and turns anything else into a named marker.
An object's keys come from `entry::member_names`, which is `Object.keys`'s own
walk rather than a second one.

**Two host pairs were added for it**, and one of them was a defect first:

- `entry::is_array_in` — the ambient `is_array` takes its own borrow, so any walk
  over a value's structure aborted on it.
- `entry::string_in` — asks whether a value **is** a string. `text_in` is
  `ToString`, and asking it as a type test made every number cross as a string:
  the copy arrived looking correct until `value.a + value.b.c` answered `"12"`
  instead of `3`. A coercion that can be mistaken for a test will be.

**Parent → worker is a poll, not an event.** `worker.postMessage` queues and the
worker reads it with `receiveMessageOnPort(parentPort)`, which is a real Node API
for exactly this. `parentPort.on('message', …)` exists and never fires: a worker
here has no event loop, its thread runs the source to the end and stops. Stated
rather than approximated.

`terminate()` sets a flag, since nothing can stop a thread at an arbitrary
instruction. A worker observes it through `isTerminating()` — **not a Node API**,
under a name a program can only reach deliberately, because inventing a
Node-shaped name for a non-Node capability is how a divergence stops being
visible.

**The host joins every worker** before a program is finished, where it already
pumps timers: Node keeps a process alive while a worker runs, and this host would
otherwise return the moment the last statement did, leaving a `'message'` queued
in a table nothing reads again.

The table is keyed by the OWNING thread, which was not optional: the worker's own
program runs `join_all` too, found its own entry, and waited on the thread that
was waiting on it — and its `pump` would have emitted onto a JS instance
belonging to the parent's region. The first hung the suite; the second is worse
for being silent.

`eval: false` is refused by name: a filename would have to be resolved the way an
import is, and this crate has no loader — the same missing piece `createRequire`
is refused for.

## The event loop, and the six copies of it that existed before

Six modules had grown the same recipe — a background thread that cannot call
into JavaScript, a table of native records, and a `pump` that turns those into
calls on the program's thread: `fs/watch`, `net`, `dgram`, `child_process`,
`timers`, `worker_threads`. The recipe is right. Six copies are not, and they had
already diverged where it counts: the host named **two** of them by hand, so the
other four only delivered if the program happened to call into that same module
again.

`entry::loops` replaces the list of names with registration. A module declares
itself a `LoopSource`; the host runs one loop — drain microtasks, ask every
source to deliver and to say when it wants to be asked again, sleep that long,
repeat.

**Why `rts-core-rwk` and not `rts-cranelift`.** The machine owns the *ordering*
(`src/sched/` — promises, continuations, the order they run in), because that is
observable and the compiler emits for it. A deadline in milliseconds is not: it
needs a wall clock and a way to wait, which not every target has, and a source's
callback is a **value**, which the machine layer does not know exists. So
`pump_sources` never sleeps — it answers how long the host should.

**What holds a program open.** Only a source answering `Pending::In`: timers and
unfinished workers. A source answering `Pending::Blocked` — a listening socket, a
watcher, a live child — is pumped on every pass and does not keep the loop alive,
because a program that starts a server would otherwise never end. Stated rather
than discovered.

Two defects were found by writing the first test that exercised this end to end,
and both had been invisible because nothing called the code:

- **`fs.watch` aborted the process.** `chained_prototype` called the ambient
  `set_prototype` from inside a `with_runtime` body — a nested borrow, which an
  `extern "C"` frame cannot unwind past.
- **`fs.watch(path, listener)` registered no listener.** Node overloads on
  `(path[, options], listener)`, and the two-argument form — the one every
  program writes — puts the function in the OPTIONS slot. Every event it queued
  was delivered to nobody. `entry::is_callable_in` is what resolves the overload,
  and `fs::options_and_listener` is where both `watch` and `watchFile` resolve
  it once.

Every table of this shape is now keyed by the OWNING thread. They are
process-wide statics holding JS instance handles, and the loop pumps on every
thread — so without it a worker's pass would emit onto cells in the parent's
region. The same fault `node:worker_threads` found first.
