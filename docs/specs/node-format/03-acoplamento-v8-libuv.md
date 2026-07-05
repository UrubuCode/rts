# 03 — Coupling with V8, libuv, and the host runtime

> What a host must provide for a `.node` to load and run. Verified
> against `nodejs.org/api/addons.html`, `nodejs.org/api/n-api.html`,
> `node-gyp/src/win_delay_load_hook.cc`, Node/Bun issues, and Bun's technical
> blog.

## 3.1 A `.node` is NOT self-contained — it has undefined symbols

An addon is compiled leaving symbols such as `napi_*`, `uv_*`, `v8::*`, `node::*`
as **undefined**; the dynamic linker resolves them from the **host binary** already
loaded in memory (node.exe / electron.exe / RTS.exe itself).

Official docs (confirmed verbatim):
> "Only the libuv, OpenSSL, V8, and zlib symbols are purposefully re-exported by
> Node.js and may be used to various extents by addons."

Issue `nodejs/node#52282`: when Node is compiled as a shared library, a
`node.def` lists all exported symbols, and any host embedding the lib
needs to link against `node.def` to "re-export the necessary symbols" that
the `.node` expects.

**Central requirement for RTS:** to load a `.node`, RTS does **not** need to
parse/relink anything — it only needs to:
1. `dlopen` the `.node`, and
2. **export** from its own executable the table of `napi_*` symbols (and, for
   async, a `uv_*` subset) that the `.node` references as undefined.

Without that export, the `dlopen` fails with *undefined symbol*.

## 3.2 How much an addon depends on V8 vs just N-API

Three routes (decreasing coupling — see
[`01-formato-binario.md`](01-formato-binario.md) §1.8):

| Route | Depends on | Stable ABI? | Supportable in RTS? |
|---|---|---|---|
| **Node-API** (C) | only `napi_*` symbols | ✅ yes | ✅ **yes** |
| **node-addon-api** (C++ header-only) | reduces to `napi_*` symbols | ✅ yes (inherits) | ✅ **yes** |
| **nan** (C++ over V8) | `v8::Local`, `v8::Isolate`, `v8::HandleScope`… | ❌ no | ❌ expensive |
| **direct V8** (`v8.h`) | V8's binary layout | ❌ no | ❌ unviable |

The ABI guarantee applies **only** to `<node_api.h>`. The docs state that
`<node.h>`, `<node_buffer.h>`, `<uv.h>`, `<v8.h>` have **no** ABI stability
across majors.

## 3.3 Why V8-direct/NAN addons are the hard case

The problem is not just "exporting V8 symbols". **Inline** functions from V8's headers
are **compiled INTO the `.node`** and the host cannot intercept them.

Bun's blog (*how-bun-supports-v8-apis-without-using-v8*):
- `v8::Object::GetInternalField` is inline; "performs some checks and then calls
  `v8::Object::SlowGetInternalField` if they fail" — those checks
  dereference pointers using **V8's tagged-pointer scheme** (lower 2
  bits = type);
- that code "gets compiled into each native module. **We can't change any of
  it**";
- JSC uses **NaN-boxing** (51 bits in NaN, 48-bit heap ptr / 32-bit int) — a layout
  incompatible with V8's tagged pointers.

To support those addons, the host would have to **emulate V8's memory
layout** (tagged pointers, a `HandleScope` with a slot stack the GC scans, internal
fields at fixed offsets, fake V8-type "Maps"). Bun documents this as
**multi-part and incomplete** work (issue `oven-sh/bun#4290`).

**Conclusion for RTS:** supporting N-API is tractable and well-defined (~150 fns,
range ~110-160);
emulating V8's binary ABI is a separate, huge and fragile project. **RTS should
target ONLY N-API** and declare V8-direct/NAN addons as unsupported — the same
frontier Bun spent years pushing and which Deno (even with real V8) also does not
cross (`better-sqlite3` fails in both).

## 3.4 Native asynchronous work via libuv

Function set:
- **simple async:** `napi_create_async_work`, `napi_delete_async_work`,
  `napi_queue_async_work`, `napi_cancel_async_work` — internally
  `uv_queue_work(loop, req, work_cb, after_work_cb)` (libuv's threadpool,
  `after` runs on the loop thread);
- **thread-safe:** `napi_create_threadsafe_function` + `call/acquire/release/
  ref/unref` — they use a `uv_async_t` (`uv_async_send` wakes the loop from another
  thread);
- **event loop:** `napi_get_uv_event_loop(env, uv_loop_t**)` — gives the raw loop.

NAN/direct addons call `uv_default_loop()` to get the loop.

**Divergence for RTS:** RTS runs **tokio**, not libuv. For N-API async:
- `napi_create_async_work` → `rt().spawn_blocking(execute_cb)`, `complete_cb`
  posted back to the RTS "main thread";
- threadsafe function → an MPSC queue drained on the thread executing JS (matches
  RTS's `promise`/`spawn_blocking` model, #437);
- `napi_get_uv_event_loop` is the **thorniest** point — addons that use
  `uv_async_t` on the raw loop would require a **minimal `uv_loop_t` shim** over tokio.

Bun does **not** run on libuv (Linux/macOS): `uv_default_loop()` is not the
runtime's loop, so addons using it directly break (issues `oven-sh/bun#14830`,
`#20453`, `#25220`); they only work via `napi_get_uv_event_loop` + a libuv shim.
This is the area where **even Bun still has parity gaps**.

## 3.5 Symbol resolution on Windows: delay-load

`node-gyp` injects `src/win_delay_load_hook.cc` into every addon. The mechanism:
- registers `__pfnDliNotifyHook2 = load_exe_hook`;
- when the loader tries to load the `HOST_BINARY` DLL (e.g.: `node.exe`), the hook
  intercepts at `dliNotePreLoadLibrary`, compares via `_stricmp(info->szDll,
  HOST_BINARY)`, and instead of looking for the `.exe` on disk returns
  `GetModuleHandle(TEXT("libnode.dll"))` or, if null, **`GetModuleHandle(NULL)`**
  (handle of the process itself);
- thus the `napi_*`/`uv_*`/`v8` symbols are resolved from the host
  executable — **it works even if the host was renamed**.

**For RTS on Windows:** it suffices to (1) **export** `napi_*`/`uv_*` from its own
`.exe` (equivalent to a `node.def` / `/EXPORT`), and (2) the `win_delay_load_hook`
already embedded in the `.node` will fall into the `GetModuleHandle(NULL)` fallback → it will look for the
symbols **in RTS.exe**. RTS only needs to ensure those symbols are in the
binary's export table.

## 3.6 Symbol resolution on Linux/macOS

Node loads via `dlopen` (default `RTLD_LAZY|RTLD_LOCAL`, configurable via
`process.dlopen` flags + `os.constants.dlopen`). On macOS `dlopen` acts as
`RTLD_GLOBAL` by default; Linux uses `RTLD_LOCAL`. The `napi_*`/`uv_*`/
`v8` symbols undefined in the `.node` are resolved against the already-loaded host binary.

**For RTS on Linux/macOS:** `RTS.exe` needs to be linked with the
`napi_*`/`uv_*` symbols **dynamically visible** — that is, **not** `-fvisibility=
hidden` for them; probably a *version script* / `--dynamic-list` or
`--export-dynamic`. Without this, even with `napi_*` implemented, the `.node`'s
dynamic `dlsym` will not find them. **A link detail to watch in RTS.**

## 3.7 Precedent: how Bun and Deno export the symbols

- **Bun** (JSC, non-V8): implements N-API from scratch. `napi.cpp` (JSC bindings) +
  `napi.zig` (TSFN/async/loop) + `bindings/v8/` (V8 shim for NAN) +
  **`src/symbols.txt`** (list of exported symbols). 156/156 fns. `napi_value`
  ↔ `JSC::JSValue`.
- **Deno** (V8 via rusty_v8): `napi_value` **is** an actual `v8::Local`
  (direct mapping). Crate `deno_napi`; symbols declared via the
  `#[napi_sym::napi_sym]` macro, names in
  **`cli/napi_sym/symbol_exports.json`**, `.def` generated by
  `tools/napi/generate_symbols_lists.js`. Loader via `libloading`. Same entry
  point `napi_register_module_v1`.

**For RTS:** the mold is clear — a **declarative list of N-API symbols**
(analogous to Deno's `symbol_exports.json` and RTS's own `abi::SPECS`/`symbols.rs`),
with generation of `.def`/export table. The machinery RTS already has to
generate `rts.d.ts` from `SPECS` would generate the N-API exports.

## Chapter conclusion

- The host must **export `napi_*` (+ `uv_*` subset)** from its binary and do
  `dlopen` + `dlsym(napi_register_module_v1)`.
- On Windows the delay-load hook falls into `GetModuleHandle(NULL)` → resolves in
  RTS.exe; on Linux/macOS it requires `--export-dynamic` for the N-API symbols.
- N-API is tractable; **V8-direct/NAN requires emulating V8's binary layout** —
  out of scope.
- async/libuv is the hard step — tokio bridge + minimal `uv_loop` shim (an area
  with gaps even in Bun).

## Sources

- https://nodejs.org/api/addons.html · /api/n-api.html
- https://github.com/nodejs/node-gyp/blob/main/src/win_delay_load_hook.cc
- https://github.com/nodejs/node/issues/52282
- https://github.com/oven-sh/bun/issues/158 · /4290 · /14830 · /20453 · /25220
- https://bun.com/blog/how-bun-supports-v8-apis-without-using-v8-part-1 · part-2
- https://docs.rs/deno_napi/latest/deno_napi/
- https://github.com/denoland/deno/tree/main/ext/napi
- https://docs.libuv.org/en/v1.x/async.html
- https://man7.org/linux/man-pages/man3/dlopen.3.html
