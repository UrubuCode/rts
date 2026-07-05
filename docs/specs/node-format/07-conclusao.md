# 07 — Conclusion: feasibility of `.node` support in RTS

> Executive synthesis. Feasibility verdict, the key decisions, and the concrete
> next steps.

## TL;DR

**Supporting `.node` in RTS is feasible, but only through the N-API door,
preferably in JIT mode, and never for direct-V8/NAN addons.** There is no
absolute technical blocker — Bun proved that N-API can be implemented on top of
a non-V8 engine. The real barriers are **work volume** (~150 functions, range
~110-160), the **event-loop long tail** (libuv vs tokio) and a **philosophical
conflict** between the `.node`'s dynamic `dlopen` and the self-contained promise
of AOT mode (`.rtslib`).

## The 5 facts that decide everything

1. **A `.node` is an ordinary DLL/`.so`/`.dylib`**, with a swapped extension;
   the entry point is the exported function **`napi_register_module_v1(napi_env,
   napi_value) -> napi_value`**. (Verified in the code: `node_binding.cc`.)

2. **`napi_value` and `napi_env` are opaque pointers.** The addon never
   dereferences them — it only passes them back to `napi_*` functions. This
   lets RTS map them to its **`HandleTable`/`RuntimeValue`** without needing V8.
   (Verified: `napi_value` = "an opaque pointer".)

3. **N-API is ABI-stable and engine-independent; raw V8 is NOT.** N-API/
   `node-addon-api` addons are supportable; `nan`/direct-`v8.h` addons require
   **emulating V8's binary layout** (tagged pointers, internal fields inlined in
   the `.node`) — infeasible. **Scope = pure N-API.** (The identical stance of
   Bun and Deno; `better-sqlite3`/`bcrypt`/`canvas` fail in both.)

4. **N-API addons skip the `NODE_MODULE_VERSION` check at load** — RTS does not
   need to fake a Node version for the `dlopen`. It only needs correct
   `platform`/`arch` for **prebuilt selection** (modern N-API prebuilds via
   `napi-rs`/`prebuildify` are per platform, not per Node version).

5. **The host must export `napi_*` (+ a `uv_*` subset) from its binary.** On
   Windows the `win_delay_load_hook` embedded in the `.node` falls back to
   `GetModuleHandle(NULL)` → resolves in RTS.exe; on Linux/macOS it requires
   `--export-dynamic`. **RTS does not relink the `.node` — it only does dlopen +
   dlsym and provides the symbols.**

## Verdict per divergence

| Divergence | Verdict |
|---|---|
| 1. Value representation | 🟢 **Not a blocker** — `napi_value` ↔ handle marshalling layer (RTS's non-moving GC even simplifies it) |
| 2. ABI/loading | 🟡 **High engineering by volume** — ~150 `extern "C"` (range ~110-160) + libloading; conceptually aligned with RTS |
| 3. GC/finalizers | 🟢 **Not a blocker** — handle scopes as roots + finalizers in the sweep; #217 family |
| 4. Async event loop | 🟡 **Medium-high, long tail** — tokio bridge easy; raw `uv_loop` shim hard (gaps even in Bun); #207 family |
| 5. JIT/AOT | 🔴 **Philosophical blocker in AOT** — natural in JIT; AOT requires self-extracting (breaks self-contained) |
| extra. Direct-V8/NAN | 🔴 **Out of scope** — solved by restricting to pure N-API |

## The recommended architectural decisions

1. **Target ONLY N-API.** Declare direct-V8/NAN addons unsupported, with a
   clear error message. (Validated by Bun and Deno.)
2. **Start with JIT (`rts run`).** That is where `dlopen` is natural. In AOT:
   forbid initially, or explicit self-extracting later.
3. **`napi_value` = stable indirection** (handle in the `HandleTable`/handle
   scope), **never** a raw `RuntimeValue` pointer (avoids "handle collected
   before use").
4. **Reuse the existing infra:** `HandleTable`, mark+sweep GC, `promise`
   (#437), `async_rt`/tokio, thread-local error slot, `symbols.rs` convention +
   generation from `SPECS`.
5. **Require a permission flag** (`--allow-native-addons`) — native code
   outside the sandbox (the lesson of Deno's `--allow-ffi`).
6. **`.rtslib` and `.node` are complementary**, not substitutes: `.rtslib` =
   performant first-party (static link, machine types); `.node` = compatibility
   with the npm ecosystem (dynamic dlopen, N-API shim).

## Realistic effort

- **80/20 core (Phase 0+1, ~40 synchronous fns, JIT only):** the first real
  addon running is an **achievable and well-bounded** milestone.
- **Full N-API (Strategy A):** **many months** — Bun took years and is still at
  ~76% on Node's suites. It is a destination, not a starting point.
- **The expensive step:** the async event loop (Phase 3) — where Bun itself has
  gaps.

## Concrete next steps

1. **Validate the integration point:** confirm that
   `resolve_node_modules_import`
   (`crates/rts-codegen/src/module/import_resolver.rs`) is the place to
   intercept `.node` (today it rejects extensions ≠ `.rts/.ts/.js`).
2. **Phase 0 PoC:** `libloading` loader + `napi_register_module_v1` + minimal
   `napi_env` + export-dynamic of the symbols, running a trivial N-API addon in
   `rts run`. Use `esbuild` or a `napi-rs` "hello world" addon as the target.
3. **Open an epic** with the 5 phases of [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md),
   each phase with a real addon as its exit criterion.
4. **Define the conformance baseline:** run Node's `js-native-api` and
   `node-api` suites (as Bun does in CI) to measure parity — integrates with
   the cross-runtime fixture system RTS already has.
5. **Decide the AOT policy:** forbid `.node` in `rts compile` (preserves
   `.rtslib`) or plan self-extracting — a product decision, not a technical one.

## Document map

- [`01-formato-binario.md`](01-formato-binario.md) — what the `.node` is,
  loader, symbols, `NODE_MODULE_VERSION`.
- [`02-napi-abi.md`](02-napi-abi.md) — the N-API ABI: values, env, scopes, refs,
  the core function set.
- [`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) — symbols the host
  exports, delay-load, libuv.
- [`04-precedente-bun-deno.md`](04-precedente-bun-deno.md) — how Bun and Deno
  did it; what breaks; cost.
- [`05-divergencias-rts.md`](05-divergencias-rts.md) — the 5 divergences
  classified, integration points.
- [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md) — strategies, 80/20,
  prebuilds, phased roadmap.

## Confidence and methodology note

This study was produced by multi-agent research (6 parallel axes with web
search) over **primary** sources: Node's official documentation
(`n-api.html`, `addons.html`), Node's source code (`node_binding.cc`,
`node.h`, `node_api.h`, `node_version.h`, `js_native_api_*.h`), Bun's code and
blog, and `deno_napi`/Deno issues.

**Adversarial verification (two complete independent runs):** each structural
technical claim went through a skeptical verifier that tried to refute it
against primary sources. Aggregate result of the two runs:

| Metric | Run A | Run B |
|---|---|---|
| Research findings | 60 | 76 |
| Claims verified | 50 | 65 |
| **Confirmed** | 35 | 49 |
| **Partial** (nuance, not invalidation) | 15 | 16 |
| **Refuted** | **0** | **0** |
| Unverified (rate-limit/session limit) | 4 | 4 |

**Zero refutations across 115 verifications.** The ~31 "partial" verdicts were
all **nuance corrections** already incorporated into the documents:

- `NODE_MODULE_VERSION 147` = **Node 26** (not 27) — corrected in §1.7.
- The N-API invocation uses `napi_module_register_by_symbol` with **5 args**
  (`+module_api_version`) in current Node — §1.3.
- `NODE_C_CTOR` on MSVC uses a static struct constructor, **not** `.CRT$XCU` — §1.4.
- N-API surface = **~110-160 functions** (not a fixed "156") — §2.5, doc 05/06.
- Bun's current paths: `src/jsc/bindings/napi.cpp` + `src/runtime/napi/napi.zig`
  + `src/jsc/bindings/v8/` (not `src/bun.js/bindings/...`) — doc 04.
- `napi_value` in Deno = newtype `#[repr(transparent)]
  NapiValue(Option<NonNull<v8::Value>>)`, nullable — doc 04.
- JSC's GC is **non-moving** (Riptide); the failure of Bun's naive impl. is
  **rooting/GC visibility**, not movement — doc 04.
- `bcrypt` migrated to N-API (v4.0.0) and works in Bun — doc 04 §4.4.

The central facts were confirmed **verbatim against Node's source code**:
`napi_register_module_v1`, `node_register_module_v<N>`, the
`NODE_MODULE_VERSION` mismatch message, `thread_local_modpending`, and the check
`if ((mp->nm_version != -1) && (mp->nm_version != NODE_MODULE_VERSION))` with
the comment `// -1 is used for Node-API modules` (which proves the N-API
exemption).
