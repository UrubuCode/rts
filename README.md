<div align="center">

<img src=".github/imgs/logo.png" alt="RTS — TypeScript that flies" width="220" />

# `rts_`

### **TypeScript compiled to a native binary. No runtime. No heavy GC. No excuses.**

*A vulture in sunglasses is never in a hurry — it has already arrived.*

[![Cranelift](https://img.shields.io/badge/backend-Cranelift-orange?style=flat-square)](https://cranelift.dev)
[![Rust](https://img.shields.io/badge/runtime-Rust-black?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Single Binary](https://img.shields.io/badge/output-single%20binary-blue?style=flat-square)](#)
<!-- CROSS_RUNTIME_BADGE_START -->
[![Bun/Node parity](https://img.shields.io/badge/Bun%2FNode%20parity-76.5%25-yellowgreen?style=flat-square)](docs/specs/cross-runtime-testing.md)
<!-- CROSS_RUNTIME_BADGE_END -->

</div>

<!-- CROSS_RUNTIME_STATS_START -->
## 🌐 Cross-runtime parity

JS spec compatibility validated against **Bun** and **Node** on 609 standalone TS fixtures.

```
[▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱] 76.5%   453/592 fixtures pass
```

| Metric | Value |
|---|---|
| **Parity** | **76.5%** (453/592) |
| ✅ RTS = Bun = Node | 453 |
| ❌ RTS diverges | 60 |
| 💥 RTS runtime error | 79 |
| 🛠️  **Left to fix** | **139** |
| ⚠️ Bun ≠ Node (skip) | 17 |
| 🚫 Rejected (RTS-only) | 0 |
| 📦 Total fixtures | 609 |

_Updated: 2026-07-05 — [how to add a fixture](docs/specs/cross-runtime-testing.md)_

<!-- CROSS_RUNTIME_STATS_END -->

---

## 🦅 What it is

**RTS** is a compiler + runtime that takes your `.ts` and spits out a native `.exe`.
It is not a transpiler, not a bundler, not a wrapper around V8 — it is Cranelift
generating machine code directly from the SWC AST, with a minimal Rust runtime
and a typed, boxing-free ABI.

Two paths, same codegen:

| Mode | Command | What it does |
|------|---------|-----------|
| 🚀 **JIT** | `rts run app.ts` | Compiles to executable memory and runs. Zero disk. |
| 📦 **AOT** | `rts compile -p app.ts out` | Object file → linker → standalone binary (~3 KB). |

---

## ⚡ Performance — RTS vs Bun vs Node

Benchmarks run on Windows 11 (100 runs, 5 warmups, median).

### Monte Carlo π — 10M iterations

<table>
<tr>
<td align="center" width="33%">
<b>Bun</b><br/>
<code>91.8 ms</code><br/>
<sub>baseline</sub>
</td>
<td align="center" width="33%">
<b>Node.js</b><br/>
<code>113.9 ms</code><br/>
<sub>1.24× slower than Bun</sub>
</td>
<td align="center" width="33%">
<b>RTS AOT</b> 🦅<br/>
<code>16.9 ms</code><br/>
<sub><b>5.43× faster than Bun</b><br/><b>6.74× faster than Node</b></sub>
</td>
</tr>
</table>

### Monte Carlo π — 10M iterations (8 workers)

<table>
<tr>
<td align="center" width="50%">
<b>Bun Workers</b><br/>
<code>147.6 ms</code>
</td>
<td align="center" width="50%">
<b>RTS multi-thread</b> 🦅<br/>
<code>30.3 ms</code><br/>
<sub><b>4.87× faster than Bun Workers</b></sub>
</td>
</tr>
</table>

### HTTP Server — req/s (sustained load)

<table>
<tr>
<td align="center" width="50%">
<b>Bun.serve</b><br/>
<code>~14k req/s</code>
</td>
<td align="center" width="50%">
<b>RTS http_server</b> 🦅<br/>
<code>29k req/s</code><br/>
<sub><b>2.07× faster than Bun.serve</b><br/>78% of pure-Rust actix</sub>
</td>
</tr>
</table>

### Summary

| Bench                          | Bun       | Node      | **RTS AOT** | RTS vs Bun | RTS vs Node |
|--------------------------------|-----------|-----------|-------------|-----------:|------------:|
| Monte Carlo 10M (1 thread)     | 91.8 ms   | 113.9 ms  | **16.9 ms** | **5.43×**  | **6.74×**   |
| Monte Carlo 10M (8 threads)    | 147.6 ms  | —         | **30.3 ms** | **4.87×**  | —           |
| HTTP throughput                | ~14k req/s| —         | **29k req/s** | **2.07×** | —           |

**Why faster?** RTS compiles TS to a native binary via Cranelift —
no JIT warmup, no GC pause, no dynamic dispatch on the hot paths. Common
loops automatically rewrite to `parallel.*` (rayon) without the user ever
mentioning threads (silent parallelism). The shard-aware HandleTable (32
lock-free shards) scales allocation in parallel.

---

## 🔮 Silent Parallelism — the user doesn't ask, the compiler delivers

> ⚠️ **Old engine (frozen).** The 3 silent-rewrite passes live in
> `rts-codegen-old` and are NOT carried into the new engine without re-justification.
> Described here as historical capability; the new engine prioritizes the soundness
> floor first.

You write this:

```ts
const arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let sum = 0;
for (const x of arr) sum = sum + x;
```

And the codegen recognizes the associative-accumulator pattern and rewrites it, before lowering IR:

```ts
sum = parallel.reduce(arr, 0, __par_reduce_0);  // rayon, transparent
```

Covers pure `for...of`, `arr.map/.forEach/.reduce`, and the classic
`s = s + EXPR` pattern. 96 functions already marked `pure: true` (math, string, num,
fmt, path, hash, mem) feed the recognition. Details in
[`docs/specs/silent-parallelism.md`](docs/specs/silent-parallelism.md).

---

## 🧰 The runtime stack — the whole `std::*`, in pure Rust

37 namespaces. No dependency on OpenSSL, schannel, libuv, or any
external runtime.

| Family | Namespaces |
|---|---|
| **I/O & FS** | `io` `fs` `path` `process` `env` `os` |
| **Compute** | `math` `num` `bigfloat` `fmt` `hash` `crypto` |
| **Memory** | `gc` `buffer` `mem` `alloc` `ptr` `ffi` |
| **Concurrency** | `thread` `atomic` `sync` `parallel` |
| **Network** | `net` `tls` `http_server` (embedded actix-web) |
| **Data** | `collections` `string` `regex` `json` `date` |
| **Async** | `events` (EventEmitter), native Promise + Function |
| **Meta** | `runtime` `test` `trace` `hint` |

🌐 **Painless HTTPS**: `rustls` + `webpki-roots` (Mozilla CAs embedded in the binary)
🧵 **Threading**: 4 coexisting mechanisms — `spawn/join`, `spawn_async`,
   `spawn_detached` (8-worker pool, 5M spawn/s), `scope` auto-join
🔒 **Shard-aware HandleTable**: 32 mutually lock-free shards

---

## 🕸️ Native HTML/CSS render engine

<div align="center">
<img src=".github/imgs/urubu-mascote.png" alt="The UrubuCode — the mascot, drawn with nothing but CSS boxes and animated by the RTS render engine" width="560" />

*The mascot above is an HTML page — no images: just CSS boxes + `@keyframes`, rendered and animated by the engine.*
</div>

RTS has its **own HTML+CSS render engine, in pure Rust** (`crates/rts-dom`),
following the canonical browser pipeline: `DOM → CSS cascade → layout (x,y,w,h) →
display list → paint`. The backend (egui/wgpu) only **paints** — the DOM owns
everything, headless and testable without a window.

What already renders faithfully (validated **number by number against real Chrome**,
`getBoundingClientRect` element by element — the Bootstrap cover and the `.row`/`.col`
grid come out **pixel-perfect**):

- **Full cascade** (specificity, `!important`, inheritance, `@media` per viewport)
  with **per-element custom properties** (`var()` with per-component override)
- **Flexbox**: `grow`/`shrink`/`basis`, `align-self`, `order`, real stretch,
  column with absorbing `margin:auto`, gap, wrap
- **Units**: `rem`/`em`/`%`/`vw`/`vh` and **`calc()`** (fluid typography),
  negative margins, `box-sizing`
- **Flow**: rich inline (links/bold flowing inside a paragraph), `float`,
  `position: absolute/fixed` v1, scroll containers with their own scrollbars
- **CSS animations**: `@keyframes` + `transition` with full easing —
  colors, sizes, and margins interpolating (the vulture above flaps its wings with this)
- **External resources**: local `<link rel="stylesheet">` + `@import`

Test any local page with one line:

```bash
rts run examples/view.ts examples/urubu.html                            # the mascot
rts run examples/view.ts examples/bootstrap-5.3.8-examples/cover/index.html  # real Bootstrap
rts run examples/view.ts path/to/your/index.html                    # your site
```

Full state and technical backlog: [issue #1793](https://github.com/UrubuCode/rts/issues/1793).

---

## 🎯 What the language understands today

✅ **Control flow** — `if/else`, `while`, `do-while`, `for`, `switch`
   (native jump table via `br_table` when all cases are integer literals)

✅ **Functions** — declaration, expression, arrow, **tail call optimization**
   (`return f(x)` becomes `return_call`), first-class function pointers

✅ **Classes** — `constructor`, methods, `this`, `extends`, `super(...)`,
   `super.method(...)`, static, getters/setters, **real virtual dispatch**,
   **Rust-style operator overload** (`a + b` becomes `a.add(b)` at compile time)

✅ **async / await** — Promise-centric pipeline with shared tokio.
   `Promise.create` does `spawn_blocking`, automatic settle via thread-local
   error slot. Complete Function class (`call/apply/bind/toString/new Function`).

✅ **Big decimal** — `bigfloat` in i128 fixed-point, ~30 digits. π via Machin
   hits 29 correct digits (f64 delivers 16).

✅ **Containers** — object/array literals via `collections.map_*`/`vec_*`,
   member access, assignment, free nesting

✅ **try/catch/finally** (phase 1) — thread-local error slot; real unwind
   not yet (#128)

✅ **Others** — `enum`, nested+rename destructuring, spread in literals,
   regex, default params, exports/imports, JSON, Date, console.*, Map/Set v0,
   essential Array/String prototypes

❌ **Not supported yet** — generators, decorators, full generics,
   `satisfies`, call spread `f(...args)`, closures with real mutable capture
   (#195 in phase 1)

---

## 🏗️ Architecture

> **Redesign in progress (strangler-fig).** The codegen engine is being
> rewritten from scratch behind the old, frozen one. The active **new** engine is
> `crates/rts-codegen-new/` (single HIR→Cranelift path, no MIR; `PolyValue`
> NaN-boxed value; shapes + data inline caches; data-driven dispatch). The
> old `crates/rts-codegen-old/` (dual HIR→MIR / AST, overloaded `i64` value)
> is **frozen** and goes away at cutover. Canonical plan:
> [`docs/specs/rts-codegen-new-design.md`](docs/specs/rts-codegen-new-design.md).

Cargo workspace in `crates/`. `src/` is the facade of the `rts` bin (re-exports
the crates); real paths live under `crates/<crate>/src/`.

```
crates/
├─ rts-ast/          internal AST
├─ rts-parser/       SWC parse → AST
├─ rts-diagnostics/  structured errors
├─ rts-engine/       ⚡ heap GC + ABI contract (SPECS, AbiType, Intrinsic, symbols) + Registry
├─ rts-hir/          typed HIR (I8..I128/F32/F64/Bool/Str/Handle/Array/Function/Class/Object/Any)
├─ rts-mir/          SSA MIR — used ONLY by rts-codegen-old (frozen); goes away at cutover
├─ rts-codegen-old/  FROZEN engine (dual MIR/AST, switchboard, manual add_fn!)
├─ rts-codegen-new/  ACTIVE engine — value.rs (PolyValue), repr.rs, shape.rs, ic.rs,
│                    dispatch.rs (data-driven), abi_gen.rs (ABI generated from SPECS), lower/ (single path)
├─ rts-primitives/   PRIMORDIAL classes (String/Object/Array/Function/Promise/Boolean/Number/Error)
├─ rts-shared/       non-primordial universal (math/num/collections(Map/Set)/json/globals + stdlib/*.ts)
├─ rts-std/          backend (io/net/tokio/console/promise/audio)
├─ rts-runtime/      thin facade (pub use of the four above) + AOT staticlib
├─ rts-node/         node:* shims (fs, os, path, process, crypto, util)
├─ rts-napi/         N-API (.node addons) via libloading + HandleTable
├─ rts-linker/       native link (system linker + object fallback)
└─ rts-cli/          run · compile · apis · init · repl · eval · ir
```

### Pipeline (new engine — single path, no MIR)

```
TS → SWC → AST → HIR (rts-hir) → lower/ (HIR → Cranelift IR, ONE path) → Cranelift egraph → JIT/AOT
```

There is no MIR tier and no dual AST/MIR in the new engine. The **Cranelift egraph**
(`use_egraphs=true`) is the ONLY optimizer (const-fold, CSE, DCE, FMA, strength
reduction, intraprocedural inlining). The front-end only does what Cranelift cannot
(JS semantics): `ToNumber/ToString/ToBoolean` coercions, the polymorphic `+`,
box/unbox insertion (pure IR the egraph folds), shape/IC site emission,
narrow-int wrap, exception edges. AOT/JIT share `compile_program`
(`FnCtx.module` is `&mut dyn Module`).

**PRIMORDIAL-vs-Registry doctrine (central to the new engine):** the engine NAMES only
the primordial classes; everything else resolves via the data-driven Registry — **no
non-primordial name hardcoded in the front, not even in an "allow-list"** (see
[`CLAUDE.md`](CLAUDE.md) § anti-hardcode). The non-primordial globals (console,
Map/Set, JSON, Date) live as prelude `.ts` (`rts-shared/stdlib/*.ts`) and
call private `engine.*` bridges; the front does not name them.

**Boxing-free ABI**: each namespace function is a symbol
`#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`. No `JsValue`,
no central dispatcher. `i64`/`f64` in native bits, strings as UTF-8
`(ptr, len)`, opaque `u64` handles for resources. In the new engine the JIT symbol
table is DERIVED from `SPECS` (`abi_gen.rs`), not manual `add_fn!`.

---

## 🚀 Get started in 30 seconds

```bash
# Install
git clone https://github.com/UrubuCode/rts && cd rts
cargo build --release

# Run
./target/release/rts run examples/console.ts

# Compile to a binary (~3 KB, no runtime DLL)
./target/release/rts compile -p examples/console.ts hello
./hello
```

### CLI

```bash
rts run file.ts                  # in-memory JIT
rts compile -p file.ts out       # AOT with use-based slicing
rts apis                         # list APIs registered in abi::SPECS
rts ir file.ts                   # dump Cranelift IR (for codegen debugging)
rts init my-app                  # project scaffolding
```

---

## 🔬 Codegen debugging

Want to see exactly what Cranelift is generating?

```bash
rts ir file.ts 2>&1 | head -50
```

Prints the IR of every user fn + `__RTS_MAIN` without executing. Great for hunting
redundant loads/stores in hot loops, unnecessary extern calls, and
intrinsic opportunities. See `CLAUDE.md` § Codegen debugging.

---

## 🎯 JS/TS compatibility

> **Honest number:** the **new** engine's real cross-runtime parity is the block
> [🌐 Cross-runtime parity](#-cross-runtime-parity) at the top (generated by CI against
> Bun+Node). The old engine hit 100% (372/372) at tag `v0.0-202606072107` — a
> local maximum of a hardcoded approach on an unsound value model; the
> redesign exists to break through that wall, not to repeat the number. Do NOT quote
> "1015/1015"/"100%" as current state.

What the **new** engine already covers (under construction, parity climbing):

- **Core syntax**: classes (extends/super/static/getters/setters),
  destructuring, spread in literals, optional chaining, nullish coalescing,
  arrow/function expressions, template literals
- **Async**: Promise + async/await (synchronous path without `await`; real event loop
  still open, #207)
- **JS globals as prelude `.ts` (data-driven)**: Object + statics,
  Boolean/Number/String prototypes, Error family, console.*, Map/Set, JSON, Date —
  none named in the front
- **Operators**: JS-spec division (`/` ALWAYS f64 — `44100/48000 === 0.91875`,
  even when assigned to a `const`), comparisons, ternary, bitwise, shifts
- **try/catch/finally** phase 1 (thread-local error slot; finally runs and
  re-propagates the error correctly)
- **Diagnostics**: an unresolved identifier becomes a compile error, never a
  segfault — and never a wrong value (the redesign's soundness floor)

Heavy items still open (some in redesign phase): real async event loop
(#207), closures with mutable capture (#195), TCO, Proxy (#218), typed
arrays/DataView/ArrayBuffer, Symbol/Reflect/BigInt (#216/#219). Master JS/TS
parity tracker: [#226](https://github.com/UrubuCode/rts/issues/226).

---

## 📚 Documentation

- 🛠️ [`CLAUDE.md`](CLAUDE.md) — internal architecture + codebase rules (includes § anti-hardcode)
- 📖 [`docs/specs/`](docs/specs/) — technical feature specs
- 🗺️ [`docs/specs/rts-codegen-new-design.md`](docs/specs/rts-codegen-new-design.md) — canonical plan of the engine redesign
- 🐛 Issues: master JS/TS parity tracker at [#226](https://github.com/UrubuCode/rts/issues/226)

---

## 🛡️ Guardrails

- ✋ No `xtask` — the build is pure `cargo`
- ✋ No runtime-support download at build time
- ✋ No Rust/Cargo dependency in the AOT binary's final environment
- ✋ Single distributed binary, runs on any Windows/Linux/macOS without installing anything

---

<div align="center">

**Made with 🦅 by [UrubuCode](https://github.com/UrubuCode)**

*If Bun is a rocket, RTS is a bird of prey.*

</div>
