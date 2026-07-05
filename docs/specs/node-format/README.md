# Study: Support for `.node` (Node.js native addons) in RTS

> **Status:** research in progress (started 2026-06-11).
> **Author:** assisted technical investigation (multi-agent + verified web sources).
> **Goal:** understand in depth what the `.node` format is, how it
> works in Node.js, and which fundamental divergences RTS faces should it
> want to support loading/running native `.node` addons from the npm ecosystem.

## Why this study exists

RTS compiles TypeScript to a native binary with a minimal Rust runtime and a
"machine types" ABI (`extern "C"`, no `JsValue`, no boxing). There is already an
analogous and **static** proposal — the [`.rtslib`](../rtslib-external-namespaces.md)
(one `.o` object per triple, linked at compile time). The `.node` is the **opposite**:
a dynamic library loaded at runtime via `dlopen`, coupled to the N-API ABI
(which assumes a V8-style host). Supporting `.node` means reconciling two
very different execution models.

## Document index

| Document | Content |
|---|---|
| [`01-formato-binario.md`](01-formato-binario.md) | What a `.node` physically is (PE/ELF/Mach-O), how Node loads it, entry symbol, `node_module` struct, `NODE_MODULE_VERSION` |
| [`02-napi-abi.md`](02-napi-abi.md) | The N-API/Node-API ABI: `napi_value`, `napi_env`, handle scopes, lifecycle, callbacks, refs/finalizers, the core function set |
| [`03-acoplamento-v8-libuv.md`](03-acoplamento-v8-libuv.md) | How much an addon depends on V8 vs N-API, libuv/event loop, symbols the host must export, delay-load on Windows |
| [`04-precedente-bun-deno.md`](04-precedente-bun-deno.md) | How Bun (on JSC) and Deno (on V8) implemented N-API; what works/breaks; real cost |
| [`05-divergencias-rts.md`](05-divergencias-rts.md) | The fundamental RTS × `.node` divergences: value, ABI, GC, event loop, JIT/AOT — blocker vs engineering |
| [`06-estrategia-roadmap.md`](06-estrategia-roadmap.md) | Implementation strategies, 80/20 core, `NODE_MODULE_VERSION`/prebuilds, phased recommendation |
| [`07-conclusao.md`](07-conclusao.md) | Executive synthesis, viability verdict, and next steps |

## TL;DR

**Supporting `.node` is viable, but only through the N-API door, preferably in
JIT mode, and never for V8-direct/NAN addons.** There is no absolute technical
blocker (Bun proved N-API can be implemented on top of a non-V8 engine). The
real barriers are: **volume** (~150 `napi_*` functions, range ~110-160), the **long tail of the event loop**
(libuv vs RTS's tokio), and a **philosophical conflict** between the dynamic
`dlopen` of `.node` and the self-contained promise of AOT (`.rtslib`).

**The 5 facts that decide everything:**
1. A `.node` is an ordinary DLL/`.so`/`.dylib`; entry point = `napi_register_module_v1`.
2. `napi_value`/`napi_env` are **opaque** → RTS can map them to its `HandleTable` without V8.
3. N-API is ABI-stable and engine-independent; raw V8/NAN is not → **scope = pure N-API**.
4. N-API addons **skip** the `NODE_MODULE_VERSION` check at load; modern prebuilds are per platform+arch.
5. The host only needs to **export `napi_*`** from its binary + `dlopen` + `dlsym` — it does not relink the `.node`.

**Recommendation:** start with **JIT + the 80/20 core** (~40 synchronous fns);
`.rtslib` and `.node` are **complementary** (performant first-party × npm compat).
Details in [`07-conclusao.md`](07-conclusao.md).

**Methodology:** multi-agent research (6 axes, web search) over primary
sources (Node docs + source code, Bun/Deno code), with **adversarial
verification in two complete independent runs** (115 claims verified,
**0 refuted**, ~31 nuances corrected and incorporated). Details and verdict
table in [`07-conclusao.md`](07-conclusao.md).
