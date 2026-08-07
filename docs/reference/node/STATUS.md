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

**34 of 42 documented modules are registered.** A module being registered means a
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
`cluster` · `domain` · `trace_events` · `http2` (framing and HPACK only — see below)

`Buffer` and `console` are not modules: `Buffer` is a class in the runtime
(`rts-core-rwk`, where `layering.md` puts it) and `console` is a global installed
by `rts-std-rwk`.

## Not registered, and what each waits on

Ordered by what unblocks the most rather than by size.

| module | doc | waits on |
|---|---|---|
| `http2` | 1165 | framing and HPACK are DONE and tested against the RFC. What is missing is the session/stream lifecycle — see below. |
| `worker_threads` | 688 | a second engine context on another thread. `rts-host-rwk` compiles for N regions already — this is the first module that needs the host, not just the runtime. |
| `repl` | 371 | `readline`, and a way to compile a string at run time — the host has one, this crate cannot reach it. |
| `vm` | 1102 | the same: compiling source from inside a running program. |
| `sqlite` | 1113 | a pure-Rust SQLite — `crates.md` §4.12 names `turso_core`; a dependency decision. |
| `wasi` | 722 | a WebAssembly runtime — `crates.md` names `wasmi`. |
| `inspector` | 1072 | a debugger protocol over a socket, and a debugger to speak it. |

## The two defects with tests that fail when fixed

Both are pinned in `crates/rts-host-rwk/tests/node_modules.rs`, asserting what
the engine DOES so that fixing them breaks the test:

- **`setTimeout(f, 0)` alone never fires.** The host pumps due timers where it
  drains microtasks, and it is reached, so "nothing pumps" is not the cause.
- **`builtinModules` is a function** where Node has a data property holding an
  array, so a program reading its `length` gets a function's arity.

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

## `node:http2` — the half that is real, and why the other half was refused

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

`connect`, `createServer`, the session and stream classes: **not built**.
Refused rather than half-built, and the reason is specific — a session that
claims `'stream'`, `close()` and `goaway()` without an owned frame-dispatch
loop, flow control, and the rapid-reset mitigation (CVE-2023-44487) the spec
calls mandatory is exactly the module that looks finished and drops frames it
never learned.

## `node:vm` and `node:repl` — written, not registered, and what stops them

Both are built on `entry::evaluate`, the capability the host hands down. Both are
unregistered because calling it from inside a running program **aborts**: it
installs a fresh context while the caller's is still installed, and the
thread-local holding one is a single slot.

That is a finding about the engine rather than about either module. Two ways out,
and they are not equivalent:

- **The evaluator refuses re-entry** — cheap, and it makes `vm.runInNewContext`
  answer `undefined` from inside a program, which is most of what a program does.
- **The slot becomes a stack** — a program can evaluate source, which is what the
  modules are for. It is also the shape `worker_threads` will need for a second
  context, so it is the one that pays twice.

Until then, both modules' code stands and their doc comments describe a
capability nothing can reach.
