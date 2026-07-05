# 06 — Implementation strategies and roadmap

> How RTS could load/use `.node`. Strategies evaluated, the 80/20 core, the
> prebuilts/`NODE_MODULE_VERSION` question, and a phased recommendation.

## 6.1 The four strategies

### Strategy A — full N-API (mirror Bun/Deno)

RTS exports **all** `napi_*`/`uv_*` symbols, loads `.node` via dlopen,
translates `napi_value` ↔ RTS handle.
- **Pros:** maximum compatibility with the N-API ecosystem.
- **Cons:** ~150 fns (real range ~110-160) + behavioral parity + event-loop
  bridge. Bun took **years** and is still at ~76% on Node's suites.
- **Verdict:** it is the destination, not the starting point.

### Strategy B — minimal shim / 80/20 core ✅ (starting point)

Implement only the subset the most-used addons need (~40 synchronous fns,
§6.2). Runs most simple N-API addons (parsers, hashing, synchronous
compression).
- **Verdict:** **the best first step** — delivers a real addon running early.

### Strategy C — JIT only, never AOT ✅ (architecturally coherent)

Support `.node` only in `rts run` (JIT: there is already executable memory +
runtime dlopen is acceptable); **forbid** it in `rts compile` (preserves the
self-contained nature of the `.rtslib`).
- **Verdict:** **the most coherent** with the architecture. Combines with B in
  the initial phase. AOT is left for later (Deno's self-extracting model,
  optional).

### Strategy D — recompile the addon as `.rtslib` ❌ (does not solve compat)

Only viable when **source** is available **and** it does not use N-API/V8 (the
`.rtslib` uses machine types + direct symbol, not `napi_env`/`napi_value`). For
most binary npm addons it would be a **rewrite**, not a recompilation.
- **Verdict:** the `.rtslib` is **not** a compatibility route to the `.node`
  ecosystem — it is a **parallel** native format (performant first-party). The
  two are **complementary**, not substitutes.

## 6.2 The 80/20 core (~40 functions of Strategy B)

The subset actually exercised by a typical CRUD/utility addon:

- **Registration:** `napi_register_module_v1` + the `napi_module` struct.
- **Callbacks:** `napi_create_function`, `napi_get_cb_info`, `napi_call_function`.
- **Creating values:** `napi_create_double/int32/uint32/int64/bigint`,
  `napi_create_string_utf8`, `napi_create_array`, `napi_create_object`,
  `napi_get_boolean`, `napi_get_undefined`, `napi_get_null`, `napi_get_global`.
- **Extracting:** `napi_get_value_double/int32/string_utf8/bool`,
  `napi_get_array_length`, `napi_get_element`.
- **Properties:** `napi_set_named_property`, `napi_get_named_property`,
  `napi_set_property`, `napi_get_property`, `napi_define_properties`.
- **Types:** `napi_typeof`, `napi_is_array`, `napi_instanceof`.
- **Errors:** `napi_throw`, `napi_throw_error/type_error`, `napi_create_error`,
  `napi_is_exception_pending`, `napi_get_and_clear_last_exception`.
- **Handle scopes:** `napi_open/close_handle_scope`,
  `napi_open_escapable_handle_scope`, `napi_escape_handle`.
- **References:** `napi_create_reference`, `napi_get_reference_value`,
  `napi_delete_reference`, `napi_reference_ref/unref`.
- **Wrap (phase 2):** `napi_wrap`, `napi_unwrap`, `napi_define_class`,
  `napi_add_finalizer`.
- **Instance data:** `napi_set/get_instance_data`.

**Left for later:** buffers/typedarrays/arraybuffers (`napi_create_buffer/
external_buffer/typedarray/dataview/arraybuffer`), promises
(`napi_create_promise`, `resolve/reject_deferred`), async work, threadsafe
functions (these require the event loop — the hard step).

## 6.3 The prebuilts and `NODE_MODULE_VERSION` question

### Good news: N-API addons skip the version check at load

The `napi_module` struct carries its own `NAPI_MODULE_VERSION`; Node's
`NODE_MODULE_VERSION` check **only bites non-N-API addons**. Confirmed
empirically: Bun reports `NODE_MODULE_VERSION 127` (= Node 22) and N-API addons
load without complaining about the version (issue `oven-sh/bun#14105`). **RTS
does not need to fake a `NODE_MODULE_VERSION` when `dlopen`-ing an N-API
addon.**

### But it needs one for the EARLIER phase: installing/selecting the prebuilt

`npm`/`prebuild-install`/`node-pre-gyp`/`node-gyp-build` choose which `.node` to
download by `(platform, arch, abi)`. RTS would report:
- `process.platform` → `win32`/`linux`/`darwin`;
- `process.arch` → `x64`/`arm64`;
- `process.versions.node` → e.g. pretend Node 22;
- `process.versions.modules` → the `NODE_MODULE_VERSION` (e.g. `127`).

Canonical source of the mapping: `doc/abi_version_registry.json` (Node repo) and
the npm package **`node-abi`**.

### Better news: modern N-API prebuilds are per (napi|abi)+platform+arch

`prebuildify` names `node.napi.node`, `electron.abi40.node`,
`node.napi.uv1.armv8.node`; `napi-rs` publishes `index.darwin-x64.node`,
`snappy.linux-x64-gnu.node` (platform-scoped packages `@scope/pkg-linux-x64-
gnu`). **N-API** packages publish **one binary per (platform, arch)** — **not**
one per Node version (precisely because of ABI stability).

**Implication:** N-API addons with the `napi` prebuild tag are selected by
**platform+arch** and do **not** require RTS to fake a specific Node version for
the binary choice — they only require correct `platform`/`arch`.
**Prioritizing the `napi-rs`/`prebuildify` ecosystem minimizes the
masquerading.** Packages that only publish an `abi<n>` tag (old
`prebuild-install` style) would force RTS to report a specific
`process.versions.modules`.

## 6.4 Permission model (lesson from Deno)

Loading a `.node` runs **native code outside the sandbox**. Deno requires
`--allow-ffi` (accepts a path list). **RTS should require an explicit flag**
(e.g. `--allow-native-addons` / `--allow-ffi`) to load `.node`,
mirroring Deno — security and explicit intent.

## 6.5 Concrete phased recommendation

> Each phase delivers **verifiable value** (a real addon running) before paying
> the cost of the next one.

### Phase 0 — Discovery (minimal loader, JIT only)
- Intercept `require('./x.node')` / `import` of `.node` in
  `resolve_node_modules_import` → route to the N-API loader.
- `libloading` (dlopen/LoadLibrary) + resolve `napi_register_module_v1`.
- Minimal `napi_env` (`RtsNapiEnv` struct) + `napi_value` ↔ `u64` handle
  translation.
- **Only** on the JIT path (`rts run`) — Strategy C.
- **Ensure export-dynamic** of the `napi_*` symbols from the RTS binary
  (`.def`/`/EXPORT` on Windows; `--export-dynamic`/version script on
  Linux/macOS).
- **Exit criterion:** a trivial N-API addon (1 function that adds two numbers)
  loads and runs.

### Phase 1 — 80/20 core (Strategy B)
- The ~40 synchronous fns of §6.2: scalar values, `string_utf8`, array, object,
  named properties, `create_function`/`get_cb_info`/`call_function`, `typeof`,
  throw/exception, handle scopes, references.
- **Exit criterion:** a real synchronous N-API addon runs (e.g. a hashing or
  synchronous compression one, or `esbuild` on the synchronous path).

### Phase 2 — Native objects and GC
- Buffers/typedarrays/arraybuffer + `napi_wrap`/`unwrap`/`define_class`/
  finalizer (integration with GC roots + sweep queueing finalizers).
- **Exit criterion:** an addon exposing a native class with a resource (e.g. a
  synchronous DB wrapper) runs and releases resources correctly.

### Phase 3 — Async (the hard step)
- Promises (map to `promise.create`/#437), async work
  (→ `rt().spawn_blocking`), threadsafe functions (MPSC queue drained on the JS
  thread), `napi_get_uv_event_loop` + a **libuv mini-shim** (`uv_async_t`,
  opaque `uv_loop_t`, `uv_queue_work`) over the global tokio.
- Clear error message for unsupported `uv_*` symbols (like Bun).
- **Exit criterion:** an asynchronous addon (background worker + callback)
  runs.

### Phase 4 — Distribution / npm
- Report coherent `process.platform`/`arch`/`versions.node`/`versions.modules`
  so that `npm`/`prebuild-install`/`node-gyp-build` download the right prebuilt.
  Prioritize `napi-rs`/`prebuildify` (selected by platform).
- **Exit criterion:** `rts i <package-with-napi-addon>` downloads the right
  `.node` and it loads.

### AOT — deferred / optional
- Keep it **forbidden** in `rts compile` (preserves self-contained), **or**
  adopt Deno's **self-extracting** model (embed + extract to tempdir +
  dlopen), documented as an explicit exception to the `.rtslib`.

### Never
- **Direct-V8 / NAN** addons (out of scope, like Bun and Deno). Clear error
  message: "addon uses the V8 C++ API, unsupported — use the N-API variant".

## 6.6 Suggested code organization (Bun/Deno template)

Mirroring the separation that worked in Bun (`napi.cpp` + `napi.zig`) and
Deno's declarative list (`symbol_exports.json`):

```
crates/rts-runtime/src/napi/
  mod.rs            — RtsNapiEnv, loader (libloading), register handshake
  values.rs         — napi_create_*/napi_get_value_* (↔ HandleTable/gc/collections)
  props.rs          — properties, define_properties
  functions.rs      — create_function, call_function, get_cb_info, callconv trampoline
  scopes.rs         — handle scopes (extra roots for the GC), references, wrap/finalizers
  errors.rs         — throw, pending exception (↔ thread-local error slot)
  async.rs          — async work, threadsafe functions (↔ async_rt/tokio), uv shim
  symbols.rs        — declarative list of exported N-API symbols (→ .def/export table)
```

The machinery RTS already has (`rts.d.ts` generation from `abi::SPECS`, the
`symbols.rs` convention) would generate the N-API export table.

## Chapter conclusion

- **Starting point: B + C** (80/20 core, JIT only). Destination: A (full
  N-API).
- The `.rtslib` is **complementary** (performant first-party), not a compat
  route.
- N-API prebuilds by **platform+arch** minimize Node-version masquerading —
  prioritize `napi-rs`/`prebuildify`.
- A 5-phase roadmap, each with a **real addon** as its exit criterion.

## Sources

- https://bun.com/docs/runtime/node-api · https://github.com/oven-sh/bun/issues/158 · /14105
- https://nodejs.org/api/n-api.html · /api/addons.html
- https://github.com/nodejs/node/blob/main/doc/abi_version_registry.json
- https://www.npmjs.com/package/node-abi · /prebuildify · /prebuild-install
- https://napi.rs/docs/cli/build · /deep-dive/native-module
- https://docs.deno.com/runtime/fundamentals/node/ · /ffi/ · /reference/cli/compile/
- https://github.com/denoland/deno/issues/23266 · /pull/28934
