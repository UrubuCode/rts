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

**28 of 42 documented modules are registered.** A module being registered means a
program can import it and call what it provides — it does NOT mean the surface is
complete. Each module's own doc carries a "Not implemented, by name" section, and
that is the authority on its gaps; this file is about which modules exist at all.

---

## Registered

`assert` · `async_hooks` · `buffer` · `child_process` · `crypto` ·
`diagnostics_channel` · `dns` · `events` · `fs` (+ `fs/promises`) · `net` ·
`os` · `path` · `perf_hooks` · `process` · `punycode` · `querystring` ·
`stream` · `string_decoder` · `test` · `timers` · `tty` · `url` · `util` · `v8` · `zlib` ·
`console` · `dgram` · `http` · `module` · `readline`

`Buffer` and `console` are not modules: `Buffer` is a class in the runtime
(`rts-core-rwk`, where `layering.md` puts it) and `console` is a global installed
by `rts-std-rwk`.

## Not registered, and what each waits on

Ordered by what unblocks the most rather than by size.

| module | doc | waits on |
|---|---|---|
| `https` | 777 | `tls`. |
| `tls` | 644 | in progress: the crates are in the manifest and a provider is being built, per §6 option (b). |
| `http2` | 1165 | `tls` and HPACK. |
| `worker_threads` | 688 | a second engine context on another thread. `rts-host-rwk` compiles for N regions already — this is the first module that needs the host, not just the runtime. |
| `cluster` | 355 | `child_process` (registered) and IPC, which does not exist. |
| `repl` | 371 | `readline`, and a way to compile a string at run time — the host has one, this crate cannot reach it. |
| `vm` | 1102 | the same: compiling source from inside a running program. |
| `sqlite` | 1113 | a pure-Rust SQLite — `crates.md` §4.12 names `turso_core`; a dependency decision. |
| `wasi` | 722 | a WebAssembly runtime — `crates.md` names `wasmi`. |
| `inspector` | 1072 | a debugger protocol over a socket, and a debugger to speak it. |
| `domain` | 597 | deprecated in Node; needs the async-id tracking `async_hooks` refuses. |
| `trace_events` | 743 | a tracing sink, and the async-id tracking again. |

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
