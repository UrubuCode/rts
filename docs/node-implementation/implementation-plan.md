# RTS Node.js Implementation — Master Plan

> The concrete, ordered plan to build the Node.js 25 API in RTS: where each piece
> goes, what must move/merge first, and the crates that back it. Grounded in a
> repo-wide structure-fit + duplication scan. Companions:
> [`architecture.md`](./architecture.md) (design) · [`layering.md`](./layering.md)
> (placement) · [`crates.md`](./crates.md) (dependencies) ·
> [`rts-std-migration.md`](./rts-std-migration.md) (rts-std surgery) ·
> [`INDEX.md`](./INDEX.md) (module map).

Status: **plan — not executed.** Execute after the open decisions
(architecture.md §13, and §7 below) are signed off.

---

## 1. Current state (verified on disk)

- **`rts-node` is a dead scaffold.** 6 files (`crypto/fs/os/path/process/util`)
  that borrow rts-std's `__RTS_FN_NS_*` symbols by name; zero reverse deps;
  `rts-runtime` does not depend on it; contributes zero JIT symbols. **Discard all
  6 — do not patch** (they use the wrong `__RTS_FN_NS_*` convention instead of the
  target `__RTS_FN_NODE_*`).
- **`node:X` silently resolves to `rts:X`.** `front/modules/resolve.rs::is_builtin()`
  accepts `node:*` (good), but `front/modules/flatten.rs::builtin_ns()` (lines
  ~199-210) strips **both** `rts:` and `node:` to the *same* bare key, so
  `node:fs` lands on rts-std's `rts:fs`. **Confirmed unfixed.**
- **`rts-async` does not exist.** The async infra lives in `rts-std`.
- **`Entry::Backend` does not exist** — but its target traits do
  (`rts-engine/src/collector/mod.rs:81-95`: `Traceable`, `GcPayload`), and
  `rts-engine/Cargo.toml` carries a `TEMP até Fase 2` note anticipating exactly
  this migration.
- **`rts-primitives` charter is narrower than layering.md needs.** Its Cargo.toml
  header defines it as PRIMORDIAL-only; layering.md decision 3 broadens it to
  "primordials + cross-context logic". CLAUDE.md/01-architecture.md must be updated
  in the same PR (RULE #0: don't leave a stale rule).
- **Stray dead scaffold:** `crates/rts-css/` exists on disk with a `src/` but no
  `Cargo.toml` and is not a workspace member — unrelated cleanup, flagged.

---

## 2. Duplicated resources — remove / relocate

From the repo-wide audit. "Target" = the single layer that should own it.

| Resource | Today (duplicated in) | Action → target |
|---|---|---|
| **Base64 codec** | `rts-std/crypto/mod.rs`; `rts-std/globals/text_encoding/instance.rs` (2nd copy, atob/btoa); dead `rts-node/crypto` | Extract ONE → **rts-primitives**; atob/btoa + subtle + node:crypto/buffer all call it |
| **SHA-256 + hex** | `rts-std/crypto/mod.rs`; dead `rts-node/crypto` | Promote pure hash+hex → **rts-primitives**; web-crypto global + node:crypto both consume. OS CSPRNG may stay per-backend (accepted) |
| **EventEmitter** | `rts-std/events/mod.rs` (low-level ns); `rts-std/globals/events/mod.rs` (richer, has once/eventNames) | Collapse to the richer one → **rts-primitives**; node:events re-exports. Delete the low-level ns (its "back node:events from rts-std" plan is stale) |
| **Path algebra** | `rts-shared/path/mod.rs` (live `path` ns) | Move → **rts-primitives** (already pure) before node:path lands |
| **util numeric format/parse** | `rts-shared/fmt/mod.rs` (`register_node_util`); registry_build.rs:64; dead `rts-node/util` | Move pure format/parse → **rts-primitives**; node:util (rts-node) becomes the only Node-name mapping; drop `register_node_util` |
| **WHATWG URL/URLSearchParams parse** | `rts-shared/globals/url/{mod,instance}.rs` (1462 lines, pure) | Move parse algorithm → **rts-primitives** before node:url; fileURLToPath/pathToFileURL stay split (SH + ND) |
| **fs sync surface** | `rts-std/fs/mod.rs` node-name aliases + Bool-typed member set; dead `rts-node/fs` | Backend-specific → **rts-node** owns its own `std::fs`; remove rts-std's interim node aliases in the same PR |
| **process/env/os info** | `rts-std/process/mod.rs` node aliases; `rts-std/{os,env}`; dead `rts-node/{process,os}` | Backend-specific → **rts-node**; remove the interim node-named members glued onto rts-std |
| **stdio fd write/read** | `rts-std/io/mod.rs` (reused correctly by console) | Keep in rts-std (or expose a small primitive) — process.stdout/readline/tty need it; accepted trivial reuse |
| **deep-equal / value inspect** | `rts-std/test/bundle.ts` (toEqual); `rts-shared/stdlib/console.ts` (`engine.display`) | Build ONE inspect/deep-equal on `engine.display` → **rts-primitives**; assert/util/test consume it |
| **async runtime/loop/promise/timers** | `rts-std/{promise_slot,promise,event_loop,runtime/async_rt}.rs`, `globals/timers` | Move *down* → **rts-engine** as the async primitive (tokio driver feature-gated). See §3 |

---

## 3. Phase P-1 — Foundation (prerequisites; nothing Node ships before these)

Ordered. Each is its own PR; run the full suite (§6) after each.

### P-1.0 — Doctrine sync
Update `CLAUDE.md` + `.claude/rules/01-architecture.md` + `rts-primitives/Cargo.toml`
header: rts-primitives = "primordials **+ cross-context shared logic**". Same PR as
the first promotion. (RULE #0.)

### P-1.1 — `Entry::Backend` in `rts-engine`
Add `Entry::Backend(Box<dyn Traceable>)` to the closed `Entry` enum
(`heap/handles.rs` ~366-533) and one dispatch arm in `impl Traceable for Entry`
(~1082-1085). Target traits already exist (`collector/mod.rs:81-95`) — a
one-variant + one-arm change, not new design. Unblocks rts-node opaque handles
(Stats/FileHandle/Server/Socket/Worker/Cipher). *Downstream (optional, later):
migrate the OS-coupled variants (TcpListener/…/HttpResponse, ~415-460) out of the
engine and drop the `TEMP` crypto deps from `rts-engine/Cargo.toml`.*

### P-1.2 — Move the async primitive into `rts-engine`  ⚠ largest/riskiest
Owner decision: **async is a primitive → it lives in `rts-engine`**, not a
separate `rts-async` crate. Move *down* from `rts-std` into the engine, behind a
`native-async` feature (default-on for the toolchain; a future wasm/browser build
omits it and supplies a host loop):
- `rts-std/runtime/async_rt.rs` (shared tokio `rt()`) + `runtime/tokio_ctx.rs`
- `rts-std/event_loop.rs` (`run_event_loop` + `__RTS_FN_RT_RUN_EVENT_LOOP`)
- `rts-std/promise_slot.rs` impl (the `PromiseSlot` *struct* already lives in the engine)
- `rts-std/promise/mod.rs` (1335 lines) + `PENDING_PROMISE_TASKS` + error-slot bridge
- timers primitive from `rts-std/globals/timers/{mod,instance}.rs`

Because the primitive now sits at the **bottom** of the graph, the earlier
cyclic-dependency risk is gone (`rts-std` and `rts-node` both depend *up* on the
engine, never sideways). **Two carve-outs still required** (they were welded into
unrelated files):
1. The **microtask queue** (`MICROTASK_QUEUE`, `drain_microtasks`,
   `mark_microtask_roots`, `enqueue_microtask_*`) is physically inside
   `rts-std/globals/text_encoding/instance.rs` (~lines 305-926). Carve it out into
   the engine; leave the encode/decode behind.
2. `gcell_snapshot` / `gcell_restore` / `mark_gcell_roots` (module-global
   thread-local promotion across `spawn_blocking`) live in
   `rts-std/collector/collector.rs` and are called by `promise::create_spawn`.
   Move these three into the engine (they are GC/thread-local machinery — a natural
   engine fit); the rest of `collector/` can stay.

Then: both `rts-std` and `rts-node` consume async from `rts-engine`. Verify
`http_server`, `fetch`, timers still pass; verify the engine still builds with
`native-async` off (no tokio leakage into the wasm-safe surface).

### P-1.3 — `rts-primitives` promotions + greenfield
- **Promote (move down):** `path` (from rts-shared), `EventEmitter` (from
  rts-std/globals/events, ~503 lines, pure), `util.format`/`inspect` (from
  rts-shared/fmt), WHATWG `URL` parse (from rts-shared/globals/url).
- **Greenfield (nothing to move):** `querystring`, `punycode`, `string_decoder`,
  `assert` compare, base64/hex/utf-8 codecs, WHATWG-stream state machines,
  deep-equal/inspect primitive on `engine.display`.
- **Missing engine capability (gates node:buffer):** `Buffer` as a real
  `Uint8Array` subclass + the TypedArray primordial machinery — none exists today
  (`rts-shared/buffer/mod.rs` is a raw handle byte-buffer, not the class). Build in
  the engine/primitives per layering.md §4.
- Add crate deps to `rts-primitives`: `url`, `idna`, `percent-encoding`,
  `form_urlencoded`, `encoding_rs` (promote existing-transitive → direct).

### P-1.4 — Codegen `node:` routing
- Fix `front/modules/flatten.rs::builtin_ns()` so `node:*` maps to node-owned
  namespace keys (e.g. `node_fs`), **not** the `rts:*` keys.
- Add `rts-node` to `rts-runtime/Cargo.toml` + `pub use rts_node::…` in
  `rts-runtime/src/namespaces/mod.rs`.
- Add `node_*` register rows in
  `rts-codegen-new/src/front/run/registry_build.rs`; drop the interim
  `register_node_util` alias once node:util is real.
- Confirm the JIT harvest (`registry::all_jit_symbols()` / `adapter_symbols`)
  picks up `__RTS_FN_NODE_*`.

### P-1.5 — Scaffold real `rts-node`
Discard the 6 dead files. New layout: `rts-node` deps = `rts-engine` +
`rts-primitives`; async from `rts-engine`; symbols `__RTS_FN_NODE_<MOD>_<NAME>`;
each module = native externs + a `.ts` shim.

---

## 4. Module implementation (P0 → P1 → P2)

Each module: **layer** it lands in · **crates** · notes. Sync surface first
(P-a), then callback/promise (P-b, uses the engine async primitive), then
classes/events (P-c).

**Prioritization (owner):** do what has a **mature pure-Rust path first**; push
anything whose only pure-Rust option is **immature/experimental** —
node:sqlite (`turso_core` BETA), node:wasi (`wasmi`), and full TLS-provider
hardening — to the **end**, regardless of its P-tier. Every domain *has* a
pure-Rust path (crate research confirmed), so nothing is truly impossible; "last"
means immature, not unavailable.

### P0 — core
| Module | Layer | Crates / mechanism |
|---|---|---|
| path | rts-primitives | (promoted, pure) |
| querystring | rts-primitives | `form_urlencoded` + shim (array-fold/custom-sep/maxKeys) |
| string_decoder | rts-primitives | `encoding_rs` + incremental UTF-8 |
| assert | rts-primitives | deep-equal/inspect primitive |
| url | rts-primitives (+E globals) | `url`+`idna`+`percent-encoding`; legacy parse shim |
| util | rts-primitives + rts-node | `format`/`inspect`/`types` (PR); process-coupled bits (ND) |
| events | rts-primitives | (promoted EventEmitter) |
| buffer | rts-primitives (+E `Buffer` global) | greenfield `Buffer:Uint8Array` + codecs; `Blob`/`File` (SH) |
| console | E global + rts-primitives + rts-std/io | (already works; formatting → PR) |
| os | rts-node | `sysinfo`+`rustix`+`whoami`; hand-roll getrusage |
| process | rts-node (+E global, +engine async) | `std::process`/`std::env`; nextTick/hrtime via the engine async primitive |
| fs | rts-node | `std::fs` (+`Entry::Backend` for Stats/FileHandle/Dir/watch via `notify`) |
| stream | rts-primitives + rts-node | WHATWG state machines (PR); fs/net-backed streams (ND) |
| timers | E globals + engine async | (engine async primitive) |
| http | rts-node | `hyper`+`http`+`http-body(-util)`+`hyper-util` over node:net |
| worker_threads | rts-engine + rts-node | threading model (engine); Worker spawn glue (ND) |
| globals | rts-engine | engine-surfaced; impls sourced per layering.md §3 |

### P1
| Module | Layer | Crates / mechanism |
|---|---|---|
| crypto | rts-primitives (algos) + rts-node | RustCrypto set (§4.1-4.3, crates.md); ⚠ `rsa` Marvin-timing note |
| net | rts-node | `std::net`/`tokio` sockets; `Entry::Backend` for Server/Socket |
| dgram | rts-node | UDP via `tokio`/`std::net` |
| dns | rts-node | `hickory-resolver` (default features) |
| https | rts-node | `hyper`+`tokio-rustls` (pure-Rust RustCrypto provider — §6 crates.md) |
| tls | rts-node | `rustls`+`tokio-rustls`+`webpki-roots`+`x509-parser`; **pure-Rust `CryptoProvider`, no ring** (basic TLS 1.3 early, full parity/hardening last) |
| child_process | rts-node | `std::process` + reader/waiter threads; IPC |
| cluster | rts-node | over child_process + net |
| zlib | rts-node | `flate2`(miniz)+`brotli`+`ruzstd` |
| readline | rts-node | `rustix` raw-mode (+ optional `rustyline`) |
| tty | rts-node | `rustix`/`std::io::IsTerminal`+`terminal_size`+`supports-color` |
| module | rts-engine + rts-node | resolver/loader (engine); CJS require glue (ND) |
| perf_hooks | E global + rts-primitives + engine async | `performance` global; marks/measures |
| async_hooks | rts-engine + rts-node | context-frame stack (engine, GC-rooted) |
| test | rts-primitives + rts-node | runner logic (PR); reporters/fs/tty (ND) |

### P2 — later / experimental / deprecated
| Module | Layer | Crates / mechanism |
|---|---|---|
| http2 | rts-node | `h2` (add when scoped) |
| v8 | rts-engine | RTS heap stats + structured-serialize — **no V8** (architecture.md §11) |
| vm | rts-engine | RTS JIT/eval + own context — **no V8** |
| inspector | rts-engine/rts-node | RTS debug surface — **no V8 CDP** (deferrable) |
| diagnostics_channel | rts-primitives | pure pub/sub |
| trace_events | rts-engine/rts-node | tracing hook |
| repl | rts-engine/rts-node | engine eval + line editor |
| sqlite | rts-node | `turso_core` (pinned pure-rust-crypto — crates.md §4.12) |
| wasi | rts-node | `wasmi`+`wasmi_wasi` (needs the WASM engine) |
| punycode | rts-primitives | `idna::punycode` (deprecated) |
| domain | rts-node | over async infra (deprecated) |

---

## 5. rts-std after the moves

Reduces to: `audio`, `asio_audio`, future UI; the Web-global infra it keeps
(`io`, `http_server`, `globals/{abort,blob,console,event_target,fetch,form_data,headers,message_channel,performance,readable_stream,text_encoding}`);
`sync`, `atomic`, `ffi`, `env`, `engine/` prelude, `gc_surface.rs`. The
`gcell_snapshot/restore/mark_gcell_roots` GC/thread-local machinery moves to
`rts-engine` (P-1.2); the rest of the `collector/` mark+sweep driver stays in
rts-std for now.

---

## 6. Per-PR verification

Each PR (foundation or module):
```bash
bash scripts/read_before_commit.sh          # engine gate (if touching rts-codegen-new)
cargo build --release -p rts-runtime         # rebuild AOT staticlib (moved symbols)
cargo build --release
cargo test --release --lib
target/release/rts.exe test                  # TS suite (if runtime/codegen/GC touched)
```
Explicit-regression discipline: the `rts:fs`/`rts:os` surface change (open decision
architecture.md §13.1) and every intentional move is documented in its PR.

---

## 7. Risks & open decisions

1. **Async carve-outs (was the cyclic-dep trap — now defused).** Putting the async
   primitive in `rts-engine` (bottom of the graph) removes the inversion risk. But
   the two welded pieces still must be carved out during P-1.2: the microtask queue
   (in `text_encoding/instance.rs`) and `gcell_*` (in `collector/collector.rs`)
   move into the engine. **Do them in P-1.2, not after.**
2. **TLS is pure-Rust (decided).** `ring` is dropped; node:tls uses a pure-Rust
   RustCrypto `CryptoProvider` (crates.md §6). Basic TLS 1.3 lands early; full
   cipher-suite/parity + constant-time hardening is end-of-plan work.
3. **Fate of `rts:fs`/`rts:os`** — drop the OS-overlap vs re-point to rts-node
   (architecture.md §13.1). Recommend drop; audit `tests/`/`bench/` for `rts:fs`
   imports first.
4. **License NOTICE** — record `webpki-roots` (CDLA), `idna`→ICU4X (Unicode-3.0),
   `moka` (Apache portion), dalek (BSD-3), `whoami` (BSL-1.0), `notify` (CC0) in a
   NOTICE/attribution file (all accepted, none blocking).
5. **Cleanup (independent):** remove the stray `crates/rts-css/`; collapse the
   duplicate `mio` (0.8+1.2, via `notify`→8.2) and `webpki-roots` (0.26+1.0)
   versions; `zstd` (C-binding) stays build-tooling-only.

---

## 8. Critical path (one line)

P-1.0 doctrine → P-1.1 `Entry::Backend` → **P-1.2 async→engine + carve-outs** →
P-1.3 primitives promotions (+Buffer) → P-1.4 codegen node: routing → P-1.5
scaffold rts-node → P0 (path/querystring/url/util/events/buffer/fs/os/process/http)
→ P1 (crypto/net/dns/**tls pure-Rust provider**/zlib/…) → **last: immature-pure**
(sqlite/`turso_core`, wasi/`wasmi`, full TLS parity/hardening) → P2 (v8/vm/inspector).
