# 05 — Fundamental divergences: RTS × `.node`

> The heart of the study. Each divergence classified as a **fundamental
> blocker** or **engineering work**, anchored in RTS's real architecture
> (Cranelift, machine-typed `extern "C"` ABI, `HandleTable`, mark+sweep GC,
> tokio).

## Executive summary of the 5 divergences

| # | Divergence | Nature | Classification |
|---|---|---|---|
| 1 | Value representation (`napi_value` vs RTS bits/handles) | marshalling layer | 🟡 medium engineering |
| 2 | ABI/loading (exported `napi_*` + dlopen vs static link) | volume + loader | 🟠 high engineering (volume) |
| 3 | GC/finalizers (V8 `napi_ref` vs mark+sweep) | root/sweep hooks | 🟡 medium engineering |
| 4 | Event loop (libuv vs tokio) | bridge + uv shim | 🟠 medium-high, long tail |
| 5 | JIT/AOT (runtime dlopen vs self-contained binary) | philosophical conflict | 🔴 philosophical blocker in AOT |
| — | **(extra)** direct-V8/NAN addons | emulate V8 layout | 🔴 near technical blocker → **out of scope** |

**Verdict:** none of the 5 is an **absolute** blocker. The real barrier is
**volume** + the **long tail** of raw `v8::`/`uv_*`, and the **philosophical
conflict** with the static `.rtslib` in AOT mode.

---

## Divergence 1 — Value representation 🟡

**The problem.** N-API addons operate on `napi_value` (opaque handle). RTS
represents JS values as **native bits** `i64`/`f64` or **`u64` handles** in a
`HandleTable`. There is no "JS object on the heap" in the V8 sense.

**Why it is NOT a blocker.** The addon **never** dereferences `napi_value` — it
only passes it back to `napi_*` functions. So RTS controls 100% of the
representation. Bun is the existence proof (maps `napi_value` → `JSC::JSValue`
without V8).

**What RTS builds:** a marshalling layer — `napi_value` becomes a
**stable handle** (index into the `HandleTable`/handle scope), never a raw
pointer to a `RuntimeValue` (otherwise the sweep collects it mid-call — the
"handle collected before use" bug already documented in RTS). Boxing/unboxing:
- `number` → `i64`/`f64` box;
- `string` → GC pool handle (`gc::string_*`);
- `object`/`array` → `collections.map_*`/`vec_*` handle;
- native pointer → `napi_external` (`u64` handle).

**RTS's advantage over Bun here:** RTS's GC is **non-moving** (mark+sweep, no
copying/relocation). In V8 (moving GC) the handle scope is mandatory so the GC
can update pointers; in RTS a `napi_value` can be a stable handle without
relocation — simplifying the semantics.

---

## Divergence 2 — ABI and loading 🟠

Two parts.

**(a) Implementing the N-API surface.** RTS would have to export **~150
functions** `napi_*`/`node_api_*` as real `extern "C"` from the runtime (the
real count is **~110-160**: `node-api-headers` v10 ~111, Node headers ~161, Bun
"156/156", Deno 163 incl. libuv). It is the **largest volume of pure work**, but:
- **there is no conceptual blocker** — each `napi_*` translates to RTS
  primitives (`gc.*`, `collections.map_*`, `string.*`);
- RTS **already** has the typed `extern "C"` paradigm (40+ namespaces,
  `__RTS_FN_*` symbols), so +~150 `extern "C"` symbols is **natural to the
  model**;
- the effort is **high but linear/incremental** (Bun proved it is doable,
  without V8).

**(b) Loading dynamically.** RTS needs `libloading` (dlopen/
LoadLibrary) to open the `.node`, resolve `napi_register_module_v1`, and
fabricate a `napi_env` (pointer to an `RtsNapiEnv`: `HandleTable`, handle-scope
stack, exception slot, tokio bridge). Straightforward work.

**Friction with the RTS model:** the `.rtslib` (existing proposal) is **static
link**, machine types, direct symbol. The `.node` is **dynamic dlopen** with an
indirect call. Supporting `.node` means **building a SECOND dynamic loader
alongside the static one**, not extending the existing one. See Divergence 5.

**Concrete integration point in the code:**
`crates/rts-codegen/src/module/import_resolver.rs::resolve_node_modules_import`
today only accepts `.rts/.ts/.js` (the `resolve_source_candidate` function
rejects other extensions). A `.node` (or a `package.json` whose `main` points to
a `.node`) would be **intercepted there** and routed to the N-API loader instead
of the TS compilation pipeline.

---

## Divergence 3 — GC and finalizers 🟡

**The problem.** `napi_ref`/`napi_wrap`/`napi_add_finalizer` tie lifetime and
finalization to V8's GC. RTS has mark+sweep with Cranelift stack maps.

**Critical sub-problem — handle scopes invisible to the stack map.** A live
`napi_value` inside a C addon **does not appear** in the Cranelift stack map
(the frame belongs to the addon, native code opaque to RTS). So
`mark_stack_roots()` would not see it and the sweep would collect it mid-call.

**What RTS builds:**
1. **Handle scopes as extra roots:** each handle scope is a vector of handles
   registered as an additional root in the GC scanner (like Bun: a scannable
   slot array); closing the scope unregisters it.
2. **`napi_ref` with refcount:** strong (refcount > 0) counts as a GC root;
   weak (0) does **not** mark, but keeps the handle so it can return `null`
   post-collection.
3. **Finalizers in the sweep:** when freeing an `Entry` with an associated
   N-API finalizer, `sweep_all_shards()` **queues** the `napi_finalize` call —
   executed **outside** the marking phase ("second-pass" timing: calling into
   the engine during the weak callback is unsafe).

**RTS's state:** it already acknowledges the difficulty of weak refs — issue
**#217** (WeakMap/WeakSet today with strong semantics). The N-API integration is
the same family of problem. It is real, orderly GC engineering, but
**known — not a blocker**.

---

## Divergence 4 — Event loop / async 🟠

**The problem.** `napi_create_async_work`/`napi_queue_async_work` and threadsafe
functions assume the **libuv loop**. `napi_get_uv_event_loop` returns a raw
`uv_loop_t*`. RTS uses **tokio**.

**What RTS builds:**
- `napi_create_async_work` → `rt().spawn_blocking(execute_cb)`; `complete_cb`
  posted back to the thread executing JS (the `promise.create`/#437 model);
- threadsafe function → MPSC queue drained on the JS thread;
- `napi_get_uv_event_loop` → the **thorniest** point: addons that link libuv
  directly and use `uv_async_t` on the raw loop would require a **minimal
  `uv_loop_t` shim** over tokio (Bun exports only a subset of `uv_*`).

**Why it is the highest-risk area.** It is a **long tail**: the synchronous path
(most utility addons) does not touch this and works without the loop. But the
raw `uv_loop` shim is where **Bun itself still has gaps**. It ties into RTS
issue **#207** (real async event loop still open) — the bottleneck.

**Not a blocker**, but the async item should be a **late phase** with a clear
error message when an unsupported `uv_*` symbol is called (as Bun does).

---

## Divergence 5 — JIT vs AOT 🔴 (philosophical)

**The problem.** In **AOT** mode (`rts compile`) RTS produces a
**self-contained** native binary (the `.rtslib` promise: "one binary, zero
files, no dynamic loading"). But a third-party `.node` is a **relocatable
shared lib** that **cannot** be statically linked like an `.o` — it expects to
resolve `napi_*` from the host at runtime via a dynamic symbol table.

**The only way out (Deno precedent).** `deno compile` historically **failed**
with addons (`#23266`). The solution (`#28934`, `deno_rt_native_addon_loader`):
**embed** the `.node` in the binary and, at startup, **extract to a tempdir +
dlopen**. Admitted limitations: does not work on a **read-only** FS nor if the
lib needs other real files on disk.

**Why static linking of `.node` does NOT exist.** The OS needs a **file on
disk** to `mmap`+relocate the shared lib — there is no `dlopen` of code that
lives only inside the binary.

**Conclusion per mode:**
- **JIT (`rts run`):** there is already executable memory and the process is
  already dynamic → `dlopen` is **natural and frictionless**. ✅
- **AOT (`rts compile`):** either **forbid** `.node` (preserves the
  self-contained purity), or adopt Deno's explicit **self-extracting** model
  (breaks "one binary, zero files"). ⚠️ An explicit architectural tradeoff that
  **contradicts the static `.rtslib`**.

---

## Extra divergence — direct-V8 / NAN addons 🔴 (out of scope)

The **only candidate for a technical blocker** (partial). Addons that link
against `v8::*` (or use `nan`/`node-addon-api` expanding to **inline V8**)
depend on **V8's binary layout**: inline functions compiled inside the
`.node` do *raw field reads* at fixed offsets (tagged pointers, internal
fields) that the host **cannot intercept** (see
[`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) §3.3).

To support them, RTS would have to **synthesize a fake V8 binary ABI** —
an immense and fragile effort (Bun documents it as multi-part and incomplete;
Deno, even with real V8, does not run `better-sqlite3`).

**Recommendation:** RTS should target **ONLY** `.node` compiled against stable
Node-API (the modern majority of npm via `napi-rs`/`node-addon-api` in N-API
mode) and **declare direct-v8 addons as unsupported** — exactly the stance of
Bun and Deno.

---

## Table of integration points with existing RTS

| N-API piece | RTS infra it reuses | Related issue/spec |
|---|---|---|
| `napi_value` ↔ handle | `HandleTable` (slab, gen+slot, 32 shards) | — |
| `napi_create_string_utf8` | GC string pool (`gc::string_*`) | bug #235 (string with `\0`) |
| `napi_create_object`/`set_named_property` | `collections.map_*` / `RuntimeValue::Object` | — |
| handle scopes as roots | `mark_stack_roots()` / `thread_registry` | — |
| weak `napi_ref` / finalizers | `sweep_all_shards()` + refcount | #217 (weak WeakMap/Set) |
| `napi_create_promise` | `promise.create` / `PromiseAsync` | #437 (async/Promise) |
| async work / TSFN | `async_rt::rt()` / `spawn_blocking` / `tokio_ctx` | #207 (event loop) |
| `napi_callback` (CDECL) | callconv-switching trampoline | `invoke_all_i64` precedent |
| `napi_create_external` / buffers | `buffer` namespace / `Vec<u8>` in the HandleTable | — |
| `napi_define_class` (dynamic class at runtime) | RTS classes today only at compile time | ⚠️ the tightest point |
| `napi_throw` / pending exception | thread-local error slot (try/catch phase 1) | #128 |

## Chapter conclusion

- **No absolute blocker.** The real barriers are **volume** (Div. 2), the
  **async long tail** (Div. 4) and the **AOT philosophical conflict** (Div. 5).
- The **only near technical blocker** (direct-V8/NAN) is solved by
  **restricting the scope to pure N-API** — a stance validated by Bun and Deno.
- Most pieces have **reusable RTS infra** (`HandleTable`, GC, promise, tokio).
  The tightest point is `napi_define_class` (dynamic class at runtime, which RTS
  today only does at compile time) — avoidable in phase 1 by targeting addons
  that **only export functions**.

→ Strategy and roadmap in [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md).

## Sources

- https://nodejs.org/api/n-api.html · https://nodejs.org/en/learn/modules/abi-stability
- https://bun.com/docs/runtime/node-api · /blog/how-bun-supports-v8-apis-without-using-v8-part-1 · part-2
- https://github.com/oven-sh/bun/issues/158 · /23136
- https://github.com/denoland/deno/issues/23266 · /pull/28934
- https://github.com/nodejs/node-addon-api/blob/main/doc/external_buffer.md
