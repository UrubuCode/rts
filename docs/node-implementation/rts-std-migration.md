# rts-std → rts-node Migration Plan

> The exact surgery to strip Node-duplicate modules out of `rts-std` and make
> `rts-node` an independent owner of them. Grounded in a repo-wide dependency
> scan. Read [`architecture.md`](./architecture.md) first for the why.

Status: **plan only — not executed.** Execute after the specs are reviewed and
the open decisions in `architecture.md` §13 are signed off.

---

## 1. The key facts that shape the migration

1. **`rts-node` today has no bodies.** It is a `NODE_SPECS` table mapping
   `node:fs.readFileSync` → the *string* `"__RTS_FN_NS_FS_READ_TEXT"`. It borrows
   rts-std's `#[unsafe(no_mangle)] extern "C"` definitions by name at link time.
   So "move a module to rts-node" = **move the extern bodies into `rts-node` and
   give them `__RTS_FN_NODE_*` symbols** — not create a new crate (it already
   exists) and not a rename.

2. **Two surfaces land on the same symbols.** `rts:fs` (native std surface, via
   `registry_build.rs` `ns::fs::register`) and `node:fs` (compat, via
   `NODE_SPECS`) both resolve to `__RTS_FN_NS_FS_*`. Removing the module from
   rts-std removes **both** unless re-pointed — hence open decision §13.1.

3. **`net`/`tls` are standalone.** `fetch` uses `ureq` (own TLS/DNS/socket),
   `http_server` uses `actix-web`/tokio. Neither calls rts-std `net`/`tls`. Grep
   for `(crate|super)::(net|tls)` in `globals/` + `http_server/` = **no matches**.
   So `net`/`tls` are clean node-duplicates, freely movable.

4. **`crypto` is dual-role.** `__RTS_FN_NS_CRYPTO_SHA256_DIGEST` (+ randomBytes /
   randomUUID / getRandomValues) **backs the Web `crypto` global** (documented in
   `rts-std-surface.md:172`), which is web-global infra that stays. The rest is
   node:crypto. → **do not move the whole module**; see §4.

5. **`path` and `util` are NOT in rts-std.** `__RTS_FN_NS_PATH_*` lives in
   `rts-shared/src/path/`; `node:util` aliases `rts_shared::fmt`
   (`register_node_util`). rts-std needs no change for these. (For rts-node
   independence, rts-node reimplements `node:path`/`node:util` natively rather
   than borrowing rts-shared — duplication accepted.)

6. **Internal couplings inside the node-duplicate cluster:**
   - `rts-std/engine/mod.rs` (a KEEP module, the private prelude bridge) calls
     `__RTS_FN_NS_OS_ARCH`.
   - `rts-std/process/mod.rs` calls `OS_PLATFORM`, `OS_ARCH`, and `ENV_CWD`.
   → move `os` + `env` + `process` **as one cluster**, and give the KEEP `engine`
   module a tiny local `arch()`/`platform()` shim so it never link-depends on
   `rts-node`.

7. **Codegen/adapters reference none of these externs by symbol** — only through
   the `ns::*::register` metadata rows in `registry_build.rs`. So the only codegen
   change is re-pointing those rows.

---

## 2. Bucket map (what moves, what stays)

| rts-std module | Bucket | Action |
|---|---|---|
| `fs/` | NODE-DUPLICATE | **MOVE** to rts-node |
| `os/` | NODE-DUPLICATE (coupled) | **MOVE** as cluster w/ env+process; shim arch/platform for `engine/` |
| `env/` | NODE-DUPLICATE (coupled) | **MOVE** with process cluster |
| `process/` | NODE-DUPLICATE (coupled) | **MOVE** as cluster w/ os+env |
| `net/` | NODE-DUPLICATE | **MOVE** to rts-node (adds `node:net`) |
| `tls/` | NODE-DUPLICATE | **MOVE** with net |
| `crypto/` | DUAL-ROLE | **SPLIT** — web-crypto primitive stays; node:crypto reimplemented in rts-node |
| `audio/` | RTS-NATIVE-UNIQUE | **KEEP** |
| `asio_audio/` | RTS-NATIVE-UNIQUE | **KEEP** |
| `io/` | INFRA (backs `console`) | **KEEP** |
| `globals/*` (13) | WEB-GLOBAL-INFRA | **KEEP** |
| `http_server/` | RTS-native server (actix) | **KEEP** (separate from node:http) |
| `ffi/` | RTS-native C interop | **KEEP** |
| `events/` (top-level) | `rts:events` primitive | **KEEP** (node:events is a TS wrapper; reimplemented in rts-node) |
| `test/` | RTS test harness | **KEEP** |
| `runtime/` (async_rt, tokio_ctx) | ASYNC PRIMITIVE | **MOVE** → `rts-engine` (under `native-async`) |
| `event_loop.rs` | ASYNC PRIMITIVE | **MOVE** → `rts-engine` |
| `promise/`, `promise_slot.rs` | ASYNC PRIMITIVE | **MOVE** → `rts-engine` (struct already there) |
| `globals/timers/` | ASYNC PRIMITIVE (used by node timers too) | **MOVE primitive** → `rts-engine`; keep the web global wrapper in rts-std |
| `collector/`, `gc_surface.rs` | RUNTIME INFRA | **KEEP** (GC surface) |
| `engine/` (private prelude) | RUNTIME INFRA | **KEEP** (add arch/platform shim) |
| `thread/`, `sync/`, `atomic/`, `time/` | RUNTIME INFRA | **KEEP** (or move thread to rts-engine if worker_threads needs it) |

`rts-shared`: `path/`, `fmt/` — **no change** (not rts-std).

---

## 3. Move mechanics (per moved module)

For each of `fs`, `os`, `env`, `process`, `net`, `tls`:

1. **Move the bodies.** Relocate the `#[unsafe(no_mangle)] extern "C"` fns from
   `crates/rts-std/src/<mod>/` into `crates/rts-node/src/<mod>/`, renaming symbols
   `__RTS_FN_NS_<NS>_<NAME>` → `__RTS_FN_NODE_<MOD>_<NAME>`. rts-node depends only
   on `rts-engine` + `rts-primitives` (async comes from the engine) — port any
   GC/handle calls to the engine `HandleTable` directly.
2. **Delete the rts-std module** and its `mod.rs` registration.
3. **Repoint the facade.** Remove the `pub use rts_std::{fs,os,env,process,net,
   tls}` lines in `crates/rts-runtime/src/namespaces/mod.rs` (≈ lines 37–73); add
   `pub use rts_node::<mod>` instead.
4. **Repoint the registry rows.** Move the `ns::<mod>::register` rows in
   `crates/rts-codegen-new/src/front/run/registry_build.rs` (lines 66, 68, 81–85)
   to resolve from the rts-node namespace. Per decision §13.1, either keep the
   `rts:<mod>` alias resolving to the rts-node bodies, or drop the `rts:<mod>`
   overlap entirely and expose only `node:<mod>`.
5. **Wire the JIT harvest.** Ensure rts-node's `__RTS_FN_NODE_*` externs are in
   the JIT symbol table — add `rts-node` as a `rts-runtime` dependency so the
   staticlib carries them and the Registry harvest
   (`registry::all_jit_symbols()` / `adapter_symbols`) picks up the fn-ptrs.
6. **Rebuild the AOT staticlib** (`cargo build -p rts-runtime`) so the moved
   symbols are in the archive.

---

## 4. crypto split (special)

Do **not** move the module wholesale. Partition:

- **Stays in rts-std (or hoist to rts-shared) as web-global infra:** the primitive
  behind the Web `crypto` global — `crypto.subtle.digest` (SHA family),
  `crypto.getRandomValues`, `crypto.randomUUID`. Keep the
  `__RTS_FN_NS_CRYPTO_SHA256_DIGEST` symbol where the `globals` surface expects it.
- **Reimplemented natively in rts-node as `node:crypto`:** `Hash`/`Hmac`,
  `Cipheriv`/`Decipheriv`, `Sign`/`Verify`, `KeyObject`, `ECDH`/`DiffieHellman`,
  `randomBytes`/`scrypt`/`pbkdf2`, `generateKeyPair`, `X509Certificate`, and the
  Node Web Crypto (`crypto.webcrypto`). rts-node uses its own crate deps
  (`sha2`/`ring`/`rustls`/`rsa`/…), duplicating the primitive rather than sharing
  — consistent with the accepted-duplication decision. node:crypto's SubtleCrypto
  may re-export the same algorithms; it does not call rts-std.

---

## 5. Async primitive → `rts-engine` (owner decision)

Blocking prerequisite for any async Node surface in rts-node (§7 of
architecture.md). **Async is a primitive → move it *down* into `rts-engine`**,
behind a `native-async` feature (so the engine still builds for wasm/browser, which
supplies a host loop). No separate `rts-async` crate. Move out of `rts-std`:

- `runtime/async_rt.rs` — `rt()`, `build_shared_runtime()` (shared tokio runtime,
  under `native-async`) + `runtime/tokio_ctx.rs`.
- `event_loop.rs` — `run_event_loop()` + `__RTS_FN_RT_RUN_EVENT_LOOP`.
- `promise_slot.rs` — `new_pending`/`.../wait_blocking` settle fns (the
  `PromiseSlot` *struct* already lives in `rts-engine/src/heap/handles.rs`).
- `promise/mod.rs` — `drain_pending_promises`, unhandled-rejection reporting.
- timers primitive (`setTimeout`/`setInterval`/`setImmediate` queue) — the
  web-global wrapper stays in `rts-std/globals/timers`.
- **carve-outs (welded elsewhere):** the microtask queue in
  `globals/text_encoding/instance.rs` (~305-926) and
  `gcell_snapshot`/`gcell_restore`/`mark_gcell_roots` in `collector/collector.rs`
  move into the engine too (else they'd leave an inverted dep).

Because async now sits at the **bottom** of the graph, `rts-std` and `rts-node`
both consume it from the engine — no back-edge, no cyclic dependency.
`http_server`/`fetch`/globals keep working; rts-node's callbacks/http/timers gain a
reachable runtime. Verify the engine still builds with `native-async` off.

---

## 6. `Entry::Backend` extension (rts-engine)

For rts-node opaque handles (Stats/Server/Socket/FileHandle/Worker/Cipher/…),
land the planned `Entry::Backend(Box<dyn Traceable>)` variant in
`rts-engine/src/heap/handles.rs` (currently a Phase-2 TODO at `handles.rs:43`),
with the collector marking nested handles via `Traceable`. Then rts-node
registers its own payload types without editing the engine's `Entry` enum per
type. Existing generic variants (`Buffer`, `Vec`, `Map`, `String`,
`ProcessChild`, `Hasher`) are reused directly where they fit.

---

## 7. Execution order

1. **P-1a** `Entry::Backend` extension in rts-engine (§6). Small, unblocks rich
   handles.
2. **P-1b** Move the async primitive into `rts-engine` (§5, `native-async` feature)
   + the microtask/gcell carve-outs. Unblocks async surface; verify rts-std still
   builds, `http_server`/`fetch`/timers still pass, and the engine builds with
   `native-async` off.
3. **P-1c** Wire `rts-node` into the dep graph (`rts-runtime` dep) + JIT harvest +
   codegen `node:` re-routing (architecture.md §4). rts-node still empty — just
   plumbing, verify nothing breaks.
4. **Per-module moves**, one PR each, following §3, in P0-first order
   (`fs` → `os`/`env`/`process` cluster → `net`/`tls` → crypto split). Each PR:
   move bodies, delete rts-std module, repoint facade + registry, rebuild
   staticlib, run the suite. Explicit-regression discipline: the `rts:<mod>`
   surface change (decision §13.1) is an intentional, documented regression.
5. **New Node modules** with no rts-std ancestor (`http`, `stream`, `buffer`,
   `events`, `zlib`, `dns`, `dgram`, `child_process`, `worker_threads`, …) are
   built fresh in rts-node per their specs — no rts-std removal involved.

---

## 8. Regression watch

- Removing `rts:fs`/`rts:os`/… (if decision §13.1 = drop) breaks any test/program
  importing `rts:fs`. Audit `tests/` and `bench/` for `from "rts:fs"` etc. before
  the drop; migrate them to `node:fs` or keep the alias.
- `engine/` (prelude) `OS_ARCH` and `process` couplings — verify the arch/platform
  shim resolves after the move (no dangling `__RTS_FN_NS_OS_*` reference).
- Web `crypto` global must keep working after the crypto split — smoke-test
  `crypto.subtle.digest`, `crypto.getRandomValues`, `crypto.randomUUID`.
- `console` must keep working — it depends on `io` (kept), not on any moved module.
- Rebuild the AOT staticlib (`cargo build -p rts-runtime`) after every move or
  `rts compile` will reference stale/missing symbols.
