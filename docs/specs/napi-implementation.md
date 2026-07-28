# N-API — Implementation plan (living tracking doc)

> **State on the cutover branch (`feat/rts-codegen-new`, 2026-06-19):** the
> `crates/rts-napi/` crate (159 N-API fns) was PORTED to the new-engine branch — the
> engine layer (`rts-engine`: `Entry::ArrayBuffer`/`BigInt`/`NapiExternal` +
> stable ptr + finalizer queue `drain_pending_napi_finalizers`) is present,
> the crate compiles (59 lib tests green) and `rts-runtime` re-exports it via
> `pub use rts_napi as napi`. **PENDING on the new engine:** the LOWERING of
> `require('x.node')` → `__RTS_FN_NS_NAPI_LOAD_ADDON` (the new engine does not yet have
> `require`/dynamic-import) + retaining the `napi_*` symbols in the bin's export table
> (`force_link`) so the addon's `dlopen` can resolve them. The text below describes the
> complete implementation (originated on the old engine, now deleted).

> **Status:** Phase 0+1+2 + classes + ArrayBuffer + async-work + threadsafe +
> **real BigInt** + **Promise↔async** + **legacy registration** ✅ — **159/159 fns
> implemented (0 stubs)**. Node v22 parity: crc32, xxhash (+class), uuid,
> **bcrypt** sync (60) **and async** (`hash(...).then`→60), ArrayBuffer
> (`fill(100)`=22532), async-work callback (`compute(7)`=49), BigInt (exact
> round-trip >2^53), chained napi Promise (`makeP().then`→string/number),
> **legacy addon** via `napi_module_register` (`legacyMsg`).
>
> **Tracking issues:**
> - [#1547](https://github.com/UrubuCode/rts/issues/1547) — general N-API tracking
> - [#1548](https://github.com/UrubuCode/rts/issues/1548) — required **engine** APIs (for @drysius): `Entry::ArrayBuffer` stable ptr, hidden slot for `wrap`, event loop hooks (#207), real BigInt (#219)

## Current coverage (124/159 fns)

| Category | Status |
|---|---|
| Loader, values, strings, objects, props, exceptions, external | ✅ implemented |
| functions/callbacks, handle scopes, references | ✅ |
| type checks, coerce, Buffer, Date, Symbol, BigInt(i64), wrap/unwrap | ✅ Phase 2 |
| Promise/deferred, type tags, finalizer, syntax errors, make_callback | ✅ Phase 2c/2d |
| **native classes** (`napi_define_class` + `new addon.X()` + `inst.method()`) | ✅ |
| **ArrayBuffer/TypedArray/DataView** (`Entry::ArrayBuffer` stable ptr, #1548) | ✅ |

### 0 stubs remaining — complete surface (159/159)

| Fn | Qty | Reason |
|---|---|---|
| ~~arraybuffer/typedarray/dataview~~ | ~~11~~ | ✅ DONE (`Entry::ArrayBuffer` stable ptr) |
| ~~async_work + callback scopes~~ | ~~9~~ | ✅ DONE synchronous |
| ~~threadsafe functions~~ | ~~8~~ | ✅ DONE **inline** (cross-thread limitation, see below) |
| ~~bigint uint64/words~~ | ~~4~~ | ✅ DONE (real `Entry::BigInt`, #219) |
| ~~uv_event_loop + cleanup hooks~~ | ~~3~~ | ✅ DONE (fake uv_loop ptr; no-op hooks) |
| ~~`napi_module_register`~~ | ~~1~~ | ✅ DONE (real **legacy** registration — `module_register.rs`) |

**Legacy registration (`napi_module_register`) — IMPLEMENTED.** It is not
V8-coupled: the `napi_module` struct is pure C-ABI and `nm_register_func` has the
signature `(env, exports)→exports`, identical to `napi_register_module_v1`. The legacy
addon calls `napi_module_register(&mod)` from a **static constructor** at `dlopen`;
this enqueues the `nm_register_func` in `module_register.rs`. The loader
(`loader.rs`) drains the queue before loading, loads the `Library` (constructor
runs), and — if `napi_register_module_v1` does not exist — uses the enqueued
legacy module. Validated E2E with a C addon that only uses the legacy path
(`legacyMsg = from_legacy_register`).

**Known limitations (dependent on the real event loop, #207):**
- **threadsafe function** runs `call_js_cb` **inline** (on the calling thread),
  it does not post to the JS thread. Addons calling the TSFN from **another thread**
  (e.g.: async `bcrypt.hash()`, which uses napi-rs's internal threadpool) crash —
  the callback needs to run on the JS thread via a queue drained by the loop.
  `bcrypt.hashSync()` works.
- **`napi_get_uv_event_loop`** returns a **fake** opaque `uv_loop_t` (stable
  non-null). Addons that only pass the loop along work; those that call `uv_*` directly
  need the **libuv-over-tokio shim**.
- **Promise↔async** works for resolution **on the same thread**: `napi_create_
  promise` + `resolve_deferred` + `.then()` from TS (validated `resolve(42)`→42).
  The `.then` is routed to `__RTS_FN_GL_PROMISE_THEN`; the value is unwrapped
  (FloatPrim→f64 bits) before the resolve.

The stubs degrade gracefully (failure, not crash). Everything is in the export table so
`napi-sys`'s `load_all()` can complete.

## Module map (`crates/rts-napi/src/`)

| File | Content |
|---|---|
| `lib.rs` | `napi_get_version`/`node_api_module_get_api_version_v1` + `force_link()` (retention of all symbols in the bin) + `napi_property_descriptor` |
| `types.rs` | ABI types: `napi_value`/`napi_env`/`napi_ref`/... (`repr(transparent)`), `napi_status`/`napi_valuetype` (`repr(C)`, fixed order), `napi_callback`/`napi_finalize` |
| `env.rs` | `RtsNapiEnv` (api_version + pending_exception + scopes + refs); `value_from_handle`/`handle_from_value` |
| `loader.rs` | `__RTS_FN_NS_NAPI_LOAD_ADDON` (libloading + `napi_register_module_v1` + per-path cache) |
| `values.rs` | scalars (double/int32/uint32/int64/bool) + sentinels + `napi_typeof` |
| `strings.rs` | `create/get_value_string_utf8` (2-pass protocol) |
| `objects.rs` | object/array/props/elements + `get_global`/`is_array`/`instanceof` |
| `functions.rs` | `create_function`/`get_cb_info`/`call_function` + callback trampoline (`__RTS_FN_RT_NAPI_DISPATCH_CALLBACK`) + `invoke_napi_callback` helper |
| `scopes.rs` | handle scopes (`[u64;32]` chunks in Box as GC roots via `global_roots`) |
| `references.rs` | `napi_ref` strong (root) / weak |
| `errors.rs` | throw/create error + pending exception on the env |
| `externals.rs` | `napi_create/get_value_external` (`Entry::NapiExternal`) |
| `classes.rs` | `napi_define_class`/`new_instance` + `__RTS_FN_RT_NAPI_NEW_INSTANCE`/`INVOKE_METHOD` |
| `phase2.rs` | type checks, property checks, coerce, Buffer, Date, Symbol, BigInt(i64) |
| `phase2b.rs` | latin1/utf16 strings, wrap/unwrap, instance-data, version, property keys |
| `phase2c.rs` | Promise/deferred, type tags, finalizer, coerce_to_string, syntax errors, cleanup hooks |
| `phase2d.rs` | external strings, make_callback, is_sharedarraybuffer |
| `arraybuffer.rs` | ArrayBuffer/TypedArray/DataView over `Entry::ArrayBuffer` (engine, #1548) — stable ptr, views via Map |
| `async_work.rs` | **synchronous** async work (queue runs execute+complete), async_init/destroy, callback scopes, post_finalizer, cleanup hooks, fake uv_event_loop (#1548) |
| `threadsafe.rs` | **inline** threadsafe functions (call_js_cb on the calling thread) — cross-thread limitation (#207) |
| `bigint.rs` | real BigInt over `Entry::BigInt` (negative+u64 words, arbitrary, #219) |
| `surface.rs` | the 35 remaining stubs (`napi_generic_failure`) — generated from the list |
| `napi_symbols.list` | **single source** of the exported names (consumed by `symbols.rs` and by the root `build.rs`) |

Codegen integration points: `import_resolver.rs` (intercepts `.node`),
`calls/mod.rs` + `indirect.rs` (`addon.method()`, `inst.method()`),
`new_expr.rs` (`new addon.X()`), `jit.rs` (registers the internal symbols),
root `build.rs` (export-table), `heap/handles.rs` (`Entry::NapiExternal` + finalizer
queue).

> **How to use this doc:** each step has `[ ]` checkboxes. Mark `[x]` when done,
> always with the exit criterion verified. Update this file in the same
> commit as the change it describes.
> **Research base:** an 8-doc `.node` feasibility study (`docs/specs/node-format/`),
> DELETED 2026-07-28 once its verdict shipped as `crates/rts-napi/`. Its
> conclusions are carried below — this is now the only N-API document.

---

## How to test with a REAL npm addon (win32)

Validated with `@node-rs/crc32`, `@node-rs/xxhash` (function + **class**), `@napi-rs/uuid`.

```bash
# 1. Install a prebuilt napi-rs addon
mkdir scratch && cd scratch && npm init -y
npm install @node-rs/xxhash
cp node_modules/@node-rs/xxhash-win32-x64-msvc/*.node xxhash.node

# 2. (Windows) addons without delay-load need the host's import lib.
#    The napi-rs prebuilts already use delay-load → they resolve GetModuleHandle(NULL)=rts.exe.
#    If an addon fails at link, generate the import lib:
dumpbin /EXPORTS rts.exe | grep napi_  > napi_host.def   # + "EXPORTS" header
lib /DEF:napi_host.def /OUT:rts.lib /MACHINE:X64 /NAME:rts.exe

# 3. Run on RTS and compare with Node
echo 'import a from "./xxhash.node"; console.log(a.xxh32("hello world"));' > t.ts
rts run --allow-native-addons t.ts          # → 3468387874
node -e 'console.log(require("./xxhash.node").xxh32("hello world"))'  # same
```

**Key lesson (why ALL symbols are exported):** `napi-sys`
(the napi-rs runtime) resolves the **entire** N-API surface at once in
`load_all()` via `GetProcAddress`. Exporting only the implemented ones makes real addons
panic with *"symbol has not been loaded"* even when only using the core. That is why
`surface.rs` exports the 35 unimplemented ones as stubs — `load_all()`
completes and only the fns actually called need to work.

---

## Context

RTS compiles TS/JS to a native binary with a minimal Rust runtime and a machine-type
ABI. We want to load **native `.node` addons** from the npm ecosystem — the
compatibility door to binary packages (parsers, hashing, compression).

The feasibility study concluded: **feasible only via N-API** (stable ABI,
opaque `napi_value`/`napi_env` → mappable to the `HandleTable` without V8), **preferably
in the JIT** (`dlopen` is natural), **never for V8-direct/NAN addons** (they would require emulating
V8's binary layout — out of scope, like Bun and Deno). A `.node` is an ordinary
DLL/`.so`/`.dylib` whose entry point is `napi_register_module_v1(napi_env, napi_value)`.

### Canonical decisions (made; do not reopen without justification)

1. **Delivered scope:** Phase 0 (loader) + Phase 1 (synchronous core) + Phase 2
   (Buffer, Date, Symbol, wrap/unwrap, type tags, Promise/deferred, finalizer) +
   **native classes** (`napi_define_class`/`new`/method). **124/159 fns.**
   **Still out (blocked by the engine, #1548):** arraybuffer/typedarray/dataview
   with stable ptr, async/threadsafe (event loop #207), real BigInt (#219).
   *(The original scope was only Phase 0+1; it was extended as the engine allowed.)*
2. **Crate `crates/rts-napi/`** depends on `rts-engine` + `rts-shared` + `libloading`
   (+ `indexmap`). Cross-crate symbols (`__RTS_FN_GL_FUNCTION_CALL`,
   `__RTS_FN_NS_PROMISE_*`) are called via `extern "C"` resolved at the bin's link.
3. **AOT (`rts compile`):** forbid `.node` with a clear error. Self-extracting (Deno-style)
   only designed, not implemented.
4. **Pure N-API only.** V8-direct/NAN → clear error.
5. **Symbol model (adversarially resolved):** the `napi_*` fns exist
   **only** as raw `#[unsafe(no_mangle)] pub extern "C"` symbols in `rts-napi`.
   Do **NOT** create `NamespaceSpec`/SPECS members for them — `validate_symbol`
   (`crates/rts-engine/src/abi/symbols.rs`) requires the `__RTS_` prefix + scope ∈
   {NS,GC,ABI,GL}; `napi_create_double` doesn't pass, and carving an exception would be dead weight
   (codegen never emits `napi.*`). The "registration layer" becomes a **declarative list**
   `pub const NAPI_EXPORTED_SYMBOLS: &[&str]` (single source to generate the export-table and
   a coherence test). **Consequence:** napi stays **out** of `rts.d.ts`
   automatically (without a `NamespaceSpec`, `emit_types.rs` does not see them).

### Correctness invariants (do not violate)

- **`napi_value` is ALWAYS a stable `u64` handle** (live `HandleTable` slot) **or**
  one of the 5 JS sentinels (`i64::MIN..=MIN+4`, `gen==0`). **Never** a raw scalar i64.
- **Every number** (`create_double/int32/uint32/int64`) is **always boxed** in
  `Entry::FloatPrim(f64)` — never inline — to have stable identity and be
  GC-traceable inside the addon's opaque native frame.
- **Anti-UAF:** write the handle into the handle-scope chunk **before** returning the
  `napi_value` to the addon. `alloc_entry` triggers GC every 256 allocs — zero window
  between alloc and registration as a root.
- **`.node` resolves undefined against the `rts` bin's export table** (OS loader),
  **not** against `JITBuilder::symbol`. They are orthogonal mechanisms: `jit.symbol(name,ptr)`
  only serves JIT-generated code to find externs (and TS never references `napi_*`).

---

## Code fact map (verified)

| Piece | File : symbol | Note |
|---|---|---|
| `rts` bin | root `Cargo.toml` `[[bin]]` `src/main.rs` | **NOT** in `rts-cli` (which is a lib) |
| Release profile | root `Cargo.toml` `[profile.release]` | `lto=true`, `opt-level="z"`, `codegen-units=1`, `strip="symbols"` ← **biggest risk** |
| `.cargo/config.toml` | **does not exist** | create one or use `cargo:rustc-link-arg-bin=rts=...` in the root `build.rs` |
| Intercept `.node` | `crates/rts-codegen/src/module/import_resolver.rs` | `validate_source_extension` (~435), `resolve_source_candidate` (236), `resolve_package_entry` (331), `resolve_node_modules_import` (461) |
| `ModuleKind` | `crates/rts-codegen/src/module/mod.rs` (~24-43) | missing `NativeAddon`; mirror the `Builtin` treatment in `detect_cycle` (221/225), `flatten_for_jit` (395), `transitive_deps_hash` (368), `disk_paths` (477); the `load` loop reads `read_to_string` (98) ← breaks on binary |
| JIT pipeline | `crates/rts-codegen/src/codegen/jit.rs` | `register_runtime_symbols` (137) = only JIT externs; **irrelevant** for resolving the `.node` |
| run/compile pipeline | `crates/rts-codegen/src/pipeline.rs` | `run_jit_with_imports` (run); `compile_file` (AOT) ← forbid `.node` |
| `HandleTable`/`Entry` | `crates/rts-engine/src/heap/handles.rs` | `Entry` (327-509), `alloc_entry` (1083), `get` (917), `free` (881), `cleanup_entry` (660), `trace_children` (687), `sweep_unmarked` (984); sentinels `i64::MIN..MIN+4`; `Entry::FloatPrim` (492) |
| GC roots | `crates/rts-engine/src/collector/global_roots.rs` | `add(addr)`/`remove(addr)`/`for_each`; the scanner reads `*(addr as *const u64)`, filters `gen!=0` |
| Error slot | `crates/rts-std/src/collector/error.rs` | `__RTS_FN_RT_ERROR_SET` (59), `_ERROR_GET` (70), `_ERROR_CLEAR` (87) |
| String pool | string_pool.rs | `__RTS_FN_NS_GC_STRING_NEW(ptr,len)`, `_STRING_PTR`, `_STRING_LEN`; `read_string_handle` |
| Callconv trampoline | `crates/rts-primitives/src/function/ops.rs` | `packed_shim` `extern "C" fn(*const i64,i64)->i64` (62, **no cap**), `invoke_all_i64` (187, cap 16 — **do not use**), `__RTS_FN_GL_FUNCTION_CALL` (801), `FunctionData.keep_alive` (793) |
| User call conv | `crates/rts-codegen/src/codegen/lower/compile/util.rs` | `user_call_conv` → `default_call_conv` (extern "C") for address-taken/lifted |
| CLI flags | `crates/rts-cli/src/cli/mod.rs` | `CliFlags` (20), `parse_flags` (~127), `CompileOptions` in `compile_options.rs` |
| Collections | `crates/rts-shared/src/collections/{map,vec}.rs` | reuse Map/Vec ops — do not reimplement |

---

# PHASE 0 — Loader (dummy addon loads)

## Step 0 — SPIKE export-table (de-risk; blocks everything)

Prove that a `#[unsafe(no_mangle)] pub extern "C" fn napi_create_double(...)` symbol
linked into the `rts` bin appears in the export table in **debug AND release** (with
`lto+strip="symbols"+opt-level="z"`), and that a minimal `.node` resolves it via
`dlsym(GetModuleHandle(NULL), ...)`.

- [x] 1 symbol `napi_test_export` `#[unsafe(no_mangle)] pub extern "C"` in the `src/main.rs` bin (`#[used]` is invalid on a fn — retention comes from `/EXPORT`, not from it)
- [x] Root `build.rs`: `emit_napi_export_args()` emits `cargo:rustc-link-arg-bin=rts=/EXPORT:<sym>` (win/MSVC), `-Wl,--export-dynamic` (linux), `-Wl,-exported_symbol,_<sym>` (macOS), conditional on `CARGO_CFG_TARGET_OS`/`_ENV`
- [x] **Validated on Windows:** `dumpbin /EXPORTS target\release\rts.exe` shows `napi_test_export` **even with `strip="symbols"`+`lto`+`opt-level="z"`** → `/EXPORT` forces the entry into the PE export directory and survives the strip. No profile override needed on Windows.
- [ ] Linux/macOS: validate on CI when available (risk 1/2 of the list) — only Windows proven locally
- **Exit (Windows):** ✅ symbol present in the release export table. Real `.node` resolving via `GetModuleHandle(NULL)` = Step 4's integration test.
- **Note:** the test symbol was reverted after validation; the `build.rs` mechanism remains and Step 1 feeds it with `NAPI_EXPORTED_SYMBOLS`.

## Step 1 — `rts-napi` skeleton crate + complete export-table ✅

- [x] `crates/rts-napi/` member in the workspace `Cargo.toml`; deps `rts-engine` + `rts-shared` + `libloading = "0.8"` (already in `Cargo.lock`)
- [x] `rts` bin depends on `rts-napi` (direct dep)
- [x] **Single source `crates/rts-napi/napi_symbols.list`** (one name/line) — consumed by `symbols.rs` (`include_str!` → `exported_symbols()`) AND by the root `build.rs` (`include_str!` → export args). **55 symbols** (80/20 core)
- [x] `src/types.rs`: `napi_value`/`napi_env`/`napi_ref`/`napi_handle_scope`/`napi_callback_info` (`#[repr(transparent)]`), `napi_status`/`napi_valuetype` (`#[repr(C)]`, ABI-fixed order), `napi_callback`/`napi_finalize`, `NAPI_AUTO_LENGTH`
- [x] `src/env.rs`: `RtsNapiEnv` (skeleton: `api_version`; scopes/refs enter in Steps 8/9) + `into_raw`/`from_raw` + `value_from_handle`/`handle_from_value` + `RTS_NAPI_VERSION=8`
- [x] The ~55 fns as `#[unsafe(no_mangle)] pub extern "C"` stubs (`napi_stub!` macro) returning `napi_generic_failure`, with **real ABI signatures**. `napi_get_version`/`node_api_module_get_api_version_v1` already implemented. (`#[used]` is invalid on a fn — retention via `/EXPORT`+`force_link`)
- [x] `build.rs` `emit_napi_export_args()`: `/EXPORT:<sym>` (win/MSVC), `--export-dynamic`+`-u <sym>` (linux), `-u`+`-exported_symbol _<sym>` (macOS), from the single source
- [x] **Symbol retention:** `rts_napi::force_link()` references the ptr of every `napi_*` fn; `main.rs` calls it via `black_box` → prevents LTO from discarding the rlib (without this: `LNK2001: 55 unresolved externals`)
- [x] `cargo test -p rts-napi` tests (3/3): no duplicates, N-API prefix, count 55
- [x] **Validated:** `dumpbin /EXPORTS target/release/rts.exe` → **55 distinct `napi_*`/`node_api_*` names** in the export table (ICF merges the identical stub bodies into a single RVA — expected; the names are all resolvable by `dlsym`; the fold undoes itself when Steps 5-12 give distinct bodies)
- [x] Smoke: `rts run` works (no regression from `force_link`)
- **Exit:** ✅ `rts.exe` links; 55 `napi_*` in the release export table.

## Step 2 — `Entry::NapiExternal` + GC hooks ✅

- [x] `Entry::NapiExternal(Box<NapiExternalData>)` in `handles.rs` (before `Free`); `NapiExternalData { data, finalize: Option<extern "C" fn(env,data,hint)>, finalize_hint }` (raw pointers — the engine does not depend on `rts-napi`). Manual `Debug` + `unsafe impl Send` (opaque pointers, never dereferenced by the engine; finalize only fired on the JS thread)
- [x] Global queue `PENDING_NAPI_FINALIZERS` (Mutex<Vec>) + `pub fn drain_pending_napi_finalizers()` — `rts-napi` drains outside the lock and fires with the right `napi_env`
- [x] `cleanup_entry`: the `NapiExternal` arm **enqueues** `(data, finalize, hint)` — it does **not** call finalize under the shard lock (deadlock/reentrancy); actual firing = Phase 2
- [x] `trace_children`: falls into `_ => {}` (no GC children) — correct
- [x] No exhaustive debug-name match to cover
- [x] Test `napi_external_finalizer_is_queued_not_called`: round-trip of the opaque ptr; free enqueues (0 calls under lock); drain returns 1 with correct data/hint; external without finalizer does not enqueue
- **Exit:** ✅ `cargo test -p rts-engine` 51/51 (+5+1); alloc+free of `NapiExternal` without calling finalize under lock.

## Step 3 — `.node` import interception ✅

- [x] `ModuleKind::NativeAddon` + `as_str()="native-addon"` + helper `is_synthetic_leaf()` (groups `Builtin`+`NativeAddon` so no site is forgotten)
- [x] Synthetic-leaf sites via `is_synthetic_leaf()`: `detect_cycle` (2×), `transitive_deps_hash`, `flatten_for_jit`. `disk_paths` **includes** `NativeAddon` on purpose (it is a real file on disk — only synthetic `Builtin` excluded)
- [x] `ModuleGraph::load` loop: if `kind == NativeAddon`, do **not** `read_to_string`; insert `SourceModule::from_native_addon` (default program, empty exports) + `continue`
- [x] `validate_source_extension(path, allow_native)` accepts `node` when `allow_native`; `resolve_source_candidate` passes `true` (lets the path through), `resolve_entry_path` passes `false` (a `.node` entry = error). Helper `is_native_addon`
- [x] `classify_resolved()` — single point that maps ext→kind + applies the `--allow-native-addons` gate; used in the 3 paths (relative, node_modules, manifest dependency)
- [x] Flag `--allow-native-addons`: `CliFlags` + `parse_flags` + `CompileOptions.allow_native_addons`
- [x] `native_addon_imports: HashMap<String, String>` (local→abs path) in the `Program`; captured in `flatten_for_jit` when an `Item::Import` resolves to a `NativeAddon` module (default + named)
- [x] AOT (`compile_file`): `graph.first_native_addon()` → clear error forbidding `.node` in `rts compile`
- [x] Without the flag → clear `E005` error with suggestion
- [x] **Validated e2e:** (1) without the flag → `E005`; (2) with the flag → graph loads (`.node` becomes a leaf, not parsed as TS), program runs; (3) `rts compile` → AOT error
- [x] **TS suite 1710/1710** (630 files), zero regression
- **Exit:** ✅ complete interception; security gate; AOT forbidden.

## Step 4 — Loader + handshake + codegen bind ✅

- [x] `crates/rts-napi/src/loader.rs` — `__RTS_FN_NS_NAPI_LOAD_ADDON(path_ptr, path_len) -> u64`:
  - [x] `libloading::Library::new(path)` + global per-path cache (`LOADED_ADDONS: Mutex<HashMap>`) keeping the `Library` alive for the process (addon fn_ptrs must not dangle) and providing **idempotence** (same `.node` → same handle)
  - [x] resolves `napi_register_module_v1` (**2-args** `(napi_env, napi_value)`); absent → clear error (legacy registration = out of scope)
  - [x] creates `exports = alloc_entry(Entry::Map)`; fabricates `RtsNapiEnv`; calls `register(env, exports)`; uses the return value if non-null, otherwise the created exports
  - [x] invalid/null path → handle 0 (no panic)
- [x] **Wiring:** `rts-runtime` re-exports `rts-napi as napi`; `rts-codegen::napi`; loader registered in the JIT (`jit.rs` `add_fn!`). `force_link` retains the symbol
- [x] **Codegen bind:** `Program.native_addon_imports` → thread-local (`passes::native_addon`) populated in `compile_program`; `lower_ident_expr` emits `LOAD_ADDON(path)` when the ident is an addon; `lower_typeof` classifies addon as `"object"` (before the "undefined" fallback)
- [x] **Tests:** loader unit (invalid/null path) + **integration test** (`tests/loader_integration.rs`) that compiles a real dummy addon via `rustc` cdylib→`.node`, loads it, validates the `Entry::Map` alive, and idempotence
- [x] **e2e validated:** real `.node` addon (Rust cdylib with `napi_register_module_v1`) → `import addon from "./real.node"` → `typeof addon === "object"` via `rts run --allow-native-addons`
- [x] **TS suite 1710/1710**, `rts-napi` 7/7, `rts-engine` 56/56 — zero regression
- **Exit (Phase 0):** ✅ **a real `.node` loads and runs in `rts run`.**

---

## ✅ PHASE 0 COMPLETE

The load cycle of a `.node` addon works end-to-end in the JIT:
import interception → security gate → dynamic loader → N-API handshake
→ exports bound to TS. The ~40 Phase 1 `napi_*` fns are still stubs
(`napi_generic_failure`); the addon loads but cannot **do** anything useful yet
until Phase 1 gives them bodies.

---

# PHASE 1 — ~40 synchronous fns (real addon)

> **✅ PHASE 1 COMPLETE** — all 8 steps (5-12) implemented; **0 stubs
> remaining** (the ~55 fns have real bodies). `rts-napi` 30 unit + 2 integration
> (loader + parity-vs-Node); 55 symbols in the export table; TS suite 1710/1710.
>
> **🎯 NODE PARITY CONFIRMED:** the same N-API addon (`add(a,b)` via
> `napi_create_function`/`get_cb_info`/`get_value_double`/`create_double`)
> produces **identical** output on Node v22 and on RTS — `add(2,3)=5`, `add(10,7)=17`,
> `add(-1,1)=0`. Validated by direct differential comparison.

## Step 5 — Scalar marshalling + typeof

- [ ] `napi_create_double/int32/uint32/int64` (always `FLOAT_BOX` → `Entry::FloatPrim`)
- [ ] `napi_get_value_double/int32/uint32/int64/bool` (`FLOAT_UNBOX` + ToInt32/ToUint32 cast)
- [ ] `napi_get_boolean/undefined/null/global` (sentinels `i64::MIN..` + singleton Map for `global`)
- [ ] `napi_typeof` (invalid handle → `napi_undefined`; do not assume number as `__RTS_FN_RT_TYPEOF_HANDLE`)
- **Exit:** round-trip `create_double(3.14)` → typeof number → `get_value_double`==3.14; each sentinel classifies correctly.

## Step 6 — Strings

- [ ] `napi_create_string_utf8` (`NAPI_AUTO_LENGTH=-1` → strlen; `__RTS_FN_NS_GC_STRING_NEW`)
- [ ] `napi_get_value_string_utf8` (**2-pass protocol**: `buf=NULL` → `*result=byte_len` without NUL; copy → `min(len, bufsize-1)`, **`floor_char_boundary`**, mandatory NUL, `*result` excludes NUL)
- **Exit:** measurement without NUL; copy truncates at a char boundary; round-trip "café".

## Step 7 — Objects / arrays / props

- [ ] `napi_create_object/array/array_with_length` (`alloc_entry(Map/Vec)`, holes = `i64::MIN+4`)
- [ ] `napi_set/get_named_property`, `napi_set/get_property`, `napi_set/get_element`, `napi_get_array_length`, `napi_is_array`
- [ ] Reuse ops from `rts-shared/src/collections/{map,vec}.rs` — do not reimplement
- **Exit:** obj set/get prop; array len 3 set/get; typeof object.

## Step 8 — Handle scopes (main correctness risk)

- [ ] `ScopeChunk { slots: [u64; N], used, next: Option<Box<ScopeChunk>> }` in a `Box` (**stable** address — do **not** use `Vec<u64>`, which reallocates and changes the base the scanner reads)
- [ ] scope stack on the `RtsNapiEnv`; `napi_open/close_handle_scope` → `global_roots::add/remove` **per chunk**; when full, chain a new chunk + new `add`
- [ ] automatic recording of each `napi_value` handle in the top scope **before** returning (anti-UAF)
- [ ] `napi_open_escapable_handle_scope` + `napi_escape_handle` (promotes to the parent 1×; 2nd time → `napi_escape_called_twice`)
- **Exit:** open a scope, create 200 strings (>1 chunk, >256 allocs = GC tick), force mark+sweep, none collected; after close+GC, collected; escape survives the close; 2nd escape → error.

## Step 9 — References

- [ ] `RefTable` (Slab) on the env; `napi_create/delete_reference`, `napi_reference_ref/unref`
- [ ] strong (refcount>0) = `Box<u64>` + `global_roots::add`; weak (0) = no root, keeps the handle
- [ ] `napi_get_reference_value` → collected weak returns undefined (`get(handle).is_none()` via gen check)
- **Exit:** strong survives GC; unref → weak + GC → `get_reference_value`==undefined; delete removes the root.

## Step 10 — Exceptions

- [ ] `napi_throw` (`__RTS_FN_RT_ERROR_SET`)
- [ ] `napi_throw_error/type_error/range_error` (`msg: *const c_char` → `CStr`; `make_error_obj(name,msg)` → `Entry::ErrorObj`; sets the slot)
- [ ] `napi_create_error/type_error/range_error` (`msg` as a String `napi_value` → `read_string_handle`; does **not** set the slot)
- [ ] `napi_is_exception_pending` (`_ERROR_GET()!=0`); `napi_get_and_clear_last_exception` (`_ERROR_GET`+`_ERROR_CLEAR`)
- **Exit:** `throw_type_error` → pending true → `get_and_clear` returns obj name="TypeError"; a throw escaping to TS top-level is reported with the correct name.

## Step 11 — Functions / callbacks (bidirectional trampoline)

- [ ] **Direction 1 (TS calls native fn):** `napi_create_function(env,name,len,cb,data,&result)` → `Entry::Function` with `packed_shim` pointing to the generic trampoline `extern "C" fn(*const i64,i64)->i64` (**no cap** — not `invoke_all_i64`). `(cb,data)` kept alive via `FunctionData.keep_alive: Arc<...>` or a side table per handle. The trampoline builds a `NapiCallbackInfoData`, calls `cb(env,info)`, converts the return
- [ ] **Direction 2 (addon calls TS fn):** `napi_call_function(env,recv,func,argc,argv,&result)` → packs argv into an `Entry::Vec` → `__RTS_FN_GL_FUNCTION_CALL(func_handle, recv, args_vec)` (`ops.rs:801`)
- [ ] `napi_get_cb_info` (in/out `argc`: capacity→actual; rest = undefined; `*this`, `*data`)
- [ ] `napi_define_properties` (`.value`→set_named; `.method`→create_function+set_named; ignores getter/setter/attributes in Phase 1)
- **Exit:** addon `add(a,b)` runs; bidirectional callback (addon invokes a TS fn passed as arg) runs.

## Step 12 — `napi_create/get_value_external` + polish

- [ ] `Entry::NapiExternal` round-trip; `napi_typeof` → external
- [ ] Confirm napi **out** of `rts.d.ts` (no `NamespaceSpec` → `emit_types.rs::generate()` unchanged)
- **Exit (Phase 1):** a real synchronous addon (hashing or synchronous compression) runs.

---

## Step 8 — Handle scopes ✅

- [x] `crates/rts-napi/src/scopes.rs`: `ScopeChunk { slots: [u64; 32], used, next }` in a `Box` (**stable** address — a `Vec` would reallocate and break the roots). `Scope` = linked list of chunks; `ScopeStack` on the `RtsNapiEnv`
- [x] Each used slot is individually registered in `global_roots::add(&slots[i])`; closing the scope unregisters all (via `Drop`)
- [x] `napi_open/close_handle_scope`, `napi_open/close_escapable_handle_scope`, `napi_escape_handle` (promotes to the parent 1×; 2nd time → `napi_escape_called_twice`)
- [x] `track_in_env` integrated into the creation fns (`box_number`, `create_string_utf8`, `create_object/array/array_with_length`) — records the handle in the top scope **before** returning (anti-UAF)
- [x] Tests: open+track(35 handles, >1 chunk)+close registers/unregisters N roots; escape promotes to the parent and survives the close; 2nd escape fails
- **Exit:** ✅ handles alive inside the addon's native frame are GC roots; collected when the scope closes.

## Step 9 — References ✅

- [x] `crates/rts-napi/src/references.rs`: `RefTable` (slab Vec+free-list) on the `RtsNapiEnv`; `RefEntry { target: Box<u64>, refcount, rooted }`
- [x] strong (refcount>0) = `Box<u64>` registered in `global_roots`; weak (0) = no root. `set_strong` re-registers/unregisters on the transition
- [x] `napi_create_reference` (initial refcount), `napi_delete_reference`, `napi_reference_ref/unref`, `napi_get_reference_value` (collected weak → undefined via `with_entry(...).is_none()`)
- [x] Tests: strong↔weak toggles the root; unref→0 removes the root, ref→1 re-adds, delete removes; collected weak (free_handle) → `get_reference_value` undefined; initial refcount 0 = weak
- **Exit:** ✅ strong refs keep the value alive across calls; weak ones reflect collection.

## Remaining steps (get_global / instanceof / define_properties) ✅

- [x] `napi_get_global` (objects.rs): lazy per-process singleton Map (`globalThis`)
- [x] `napi_instanceof` (objects.rs): heuristic over the instance's `__rts_class` vs the constructor's `Function.name` (common case; no hierarchy)
- [x] `napi_define_properties` (functions.rs): honors `utf8name`/`value`/`method`/`data` (method → `create_function`+`set_named`); ignores getter/setter/attributes in Phase 1
- **0 stubs remaining** — `napi_stub!` macro removed.

## Step 11 — Functions / callbacks (bidirectional trampoline) ✅

- [x] `crates/rts-napi/src/functions.rs`:
  - [x] `napi_create_function`: allocates a marker `Entry::Function` (`fn_ptr=0`) and registers `(cb, env, data)` in a `NAPI_CALLBACKS: Mutex<HashMap<handle, NapiFn>>` indexed by the handle
  - [x] `__RTS_FN_RT_NAPI_DISPATCH_CALLBACK(handle, this, args_handle, out_result) -> i64`: shim called at the start of `__RTS_FN_GL_FUNCTION_CALL` (rts-primitives, via `extern "C"` resolved by link/JIT). If the handle is in the registry, it builds a `CallbackInfo`, calls `cb(env, info)`, writes the result handle into `out_result` and returns 1; otherwise returns 0 (normal dispatch proceeds)
  - [x] `napi_get_cb_info`: reads the `CallbackInfo`; `argc` in/out (capacity→actual, rest filled with undefined); `this_arg`/`data`
  - [x] `napi_call_function`: packs argv into an `Entry::Vec` and calls `__RTS_FN_GL_FUNCTION_CALL` (reverse direction: addon calls a TS fn)
- [x] **Codegen:** `addon.method(args)` intercepted in `calls/mod.rs` (before the namespace lookup, otherwise it becomes "unknown namespace member") → `lower_native_addon_method_call` (`indirect.rs`): `LOAD_ADDON` → `MAP_GET_STR(method)` → packs args as `napi_value` (numbers via `FLOAT_BOX`) → `FUNCTION_CALL` → result handle (ambiguous, unwrapped by the concat)
- [x] **JIT:** `__RTS_FN_RT_NAPI_DISPATCH_CALLBACK` + `__RTS_FN_RT_MAP_GET_STR` registered; `force_link` retains the dispatch
- [x] **e2e validated:** real `add(a,b)` addon (`napi_create_function`+`get_cb_info`+`get_value_double`+`create_double`+`set_named_property`) → `addon.add(2,3)===5` on RTS
- [x] **Parity vs Node v22:** same output (5/17/0) — `tests/parity_vs_node.rs` (graceful skip without MSVC/Node)
- **Exit:** ✅ an addon exposing TS-callable functions runs, with Node parity.

### ⚠️ Host import lib (needed for real addons on Windows)

A `.node` leaves the `napi_*` symbols undefined, resolved against the host at
runtime. On Windows the linker requires an **import library** when compiling the
addon. Today it is generated manually:
```
dumpbin /EXPORTS rts.exe | grep napi_  → napi_host.def
lib /DEF:napi_host.def /OUT:rts.lib /MACHINE:X64 /NAME:rts.exe
# compile the addon with  -L <dir> -l rts
```
**Follow-up:** `rts` should emit an `rts.lib`/`.def` in the distribution
directory (as Node does with `node.lib`) so that `npm`/`node-gyp`/`napi-rs`
can link addons against it without manual steps. N-API prebuilts (napi-rs/
prebuildify) use delay-load and resolve via `GetModuleHandle(NULL)` →
they work without a relink (the `win_delay_load_hook` falls back to rts.exe).

## Risks / unknowns (spike before the corresponding step)

1. **[CRITICAL] `strip="symbols"` + `lto=true` + `opt-level="z"` vs export-dynamic.**
   Prove that `/EXPORT`/`--dynamic-list`/`-exported_symbols_list` survive the release
   strip on the `rts` bin on the 3 OSes. If not, a profile override for the bin. → Step 0.
2. **[HIGH] macOS two-level vs flat namespace.** dlopen resolving undefined against the
   main image may require `-exported_symbols_list` (keeps two-level) — avoid
   `-flat_namespace`. Spike on macOS CI.
3. **[HIGH] `cargo:rustc-link-arg-bin=rts=` with LTO.** Confirm granularity (only the
   bin) and that `#[used]`+`no_mangle`+`/EXPORT` survive LTO finalization. → Step 0.
4. **[MEDIUM] `napi_value` + reentrant GC.** Prove "record in the scope before returning"
   and "number always `FloatPrim`" under a GC tick every 256 allocs inside a call to the
   addon. Design spike before Step 8.
5. **[MEDIUM] Callconv trampoline + keep-alive.** `Entry::Function` has no native `data`
   field; decide `keep_alive: Arc` vs a side table. Confirm the `packed_shim` path
   (not `invoke_all_i64`, cap 16). `Library` not dropped while addon handles
   are alive. Spike before Step 11.
6. **[MEDIUM] Windows delay-load fallback.** Depends on the `win_delay_load_hook` embedded in the
   `.node` falling back to `GetModuleHandle(NULL)`. Addons without the hook (rare) → clear error.

**Secondary unknowns (clear-error in Phase 0, non-blocking):** legacy registration
`napi_module_register` via static constructor; napi-rs packages with a **JS wrapper** that
`require('./x.node')` (Phase 0 covers only direct `.node` import + literal `.node` `main`);
addons without `win_delay_load_hook`.

---

## Test addon (win32 x64)

- **Steps 0-4 (loader/export-table):** download a **prebuilt** win-x64 `.node` without a
  toolchain — `npm i` in a scratch project of a simple napi-rs package and grab the
  `node_modules/.../*.node`. Validates export-table + loader without compiling anything.
- **Step 11 (`add(a,b)`):** compile with **napi-rs** (`@napi-rs/cli`: `napi new` →
  `add(a,b)` template → `napi build`). The repo already has MSVC + Rust → cleaner than
  node-gyp (which would require Python). **Pin the addon's NAPI version ≤ the implemented one.**
  `node-addon-api` is an equivalent alternative.
- Validate that the addon exports `napi_register_module_v1` via dlsym (not legacy registration).

---

## Future phases (35 remaining stubs — blocked by the engine, see #1548)

- **✅ Phase 2 (DONE):** Buffer, Date, Symbol, `napi_wrap`/`unwrap`,
  `napi_define_class` + native classes, type tags, Promise/deferred,
  `napi_add_finalizer` (via `Entry::NapiExternal`).
- **arraybuffer/typedarray/dataview (11 fns):** needs `Entry::ArrayBuffer`
  with a **stable mutable pointer** in the engine (#1548 item 1). I plot the fns on
  top as soon as it exists.
- **async/threadsafe (19 fns):** `napi_create_async_work`/threadsafe functions/
  `napi_get_uv_event_loop` — depend on the **real event loop** (#207, #1548 item 3).
  Long tail (gaps even in Bun).
- **real BigInt (4 fns):** `bigint_uint64`/`bigint_words` — depend on a real
  `Entry::BigInt` (#219). Today `bigint_int64` uses `FloatPrim` (loses >2^53).
- **distribution/npm:** emit `rts.lib` in the distribution (like `node.lib`) so
  `npm`/`node-gyp`/`napi-rs` can link addons against `rts.exe` without manual steps.
- **AOT self-extracting** (Deno model): product decision deferred.
- **Never:** V8-direct/NAN addons (legacy `module_register` registration is the only
  out-of-scope stub, not engine-blocked).

Tracking: general tracking #1547, engine APIs #1548.
