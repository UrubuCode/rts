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
[![Bun/Node parity](https://img.shields.io/badge/Bun%2FNode%20parity-77.9%25-yellowgreen?style=flat-square)](the spec removed 2026-08-03 (see git history))
<!-- CROSS_RUNTIME_BADGE_END -->
<!-- NODE_SUITE_BADGE_START -->
[![Node test suite](https://img.shields.io/badge/Node%20test%20suite-52.4%25-yellow?style=flat-square)](scripts/node_tests/README.md)
<!-- NODE_SUITE_BADGE_END -->

</div>

<!-- CROSS_RUNTIME_STATS_START -->
## 🌐 Cross-runtime parity

JS spec compatibility validated against **Bun** and **Node** over 1516 standalone TS fixtures.

```
[▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▰▱▱▱▱] 77.9%   1179/1513 fixtures passing
```

| Metric | Value |
|---|---|
| **Parity** | **77.9%** (1179/1513) |
| ✅ RTS = Bun = Node | 1179 |
| ❌ RTS diverges | 246 |
| 💥 RTS runtime error | 88 |
| 🛠️  **Left to fix** | **334** |
| ⚠️ Bun ≠ Node (skip) | 2 |
| 🚫 Rejected (RTS-only) | 0 |
| 📦 Total fixtures | 1516 |

_Updated: 2026-08-30 — [how to add a fixture](the spec removed 2026-08-03 (see git history))_

<!-- CROSS_RUNTIME_STATS_END -->

<!-- NODE_SUITE_STATS_START -->
## 🟢 Node test suite

As bibliotecas `node:` medidas contra **a suíte de testes do próprio Node** (`test/parallel`), um processo por ficheiro, sem tradução nenhuma.

```
[▰▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱] 52.4%   1659/3168 ficheiros passando
```

| Metric | Value |
|---|---|
| **Compatibilidade** | **52.4%** (1659/3168) |
| ✅ Sai com 0 | 1659 |
| ❌ Asserção falhou | 668 |
| 💥 Exceção não apanhada | 829 |
| ⏱️ Não terminou | 12 |
| ➖ Fora da conta | 374 |

Os 374 de fora leem os módulos **internos** do Node (`internal/…`, `_http_common`) ou pedem `--expose-gc`: o próprio Node só os corre com uma flag, e nenhum runtime de terceiros os pode passar. Contados à parte porque as duas alternativas mentem — somá-los às falhas afirma que há centenas de bibliotecas por fazer, apagá-los esconde que 10% do corpus nunca foi uma pergunta sobre isto.

**Por grupo** (os dez maiores). O grupo é o prefixo do ficheiro na suíte — a forma como o Node os agrupa, não uma classificação nossa: `test-worker-*` é `worker_threads` e `test-child-*` é `child_process`.

| Grupo | % | ok/total |
|---|---|---|
| `test-http-*` | **74.6%** | 261/350 |
| `test-http2-*` | **100.0%** | 222/222 |
| `test-fs-*` | **41.0%** | 89/217 |
| `test-tls-*` | **91.0%** | 162/178 |
| `test-stream-*` | **37.8%** | 62/164 |
| `test-net-*` | **64.7%** | 88/136 |
| `test-worker-*` | **14.6%** | 18/123 |
| `test-child-*` | **21.4%** | 21/98 |
| `test-crypto-*` | **98.9%** | 93/94 |
| `test-process-*` | **33.3%** | 28/84 |

**As causas mais frequentes** — uma mensagem repetida é um nome em falta, não N problemas

| Ficheiros | Mensagem |
|---|---|
| 224 | `AssertionError [strictEqual] #N: Expected values to be strictly equal:` |
| 139 | `TypeError: Cannot read properties of undefined (reading '…')` |
| 121 | `AssertionError [throws] #N: Missing expected exception.` |
| 59 | `rts: uncaught '…' event: an object` |
| 48 | `AssertionError [ok] #N: The expression evaluated to a falsy value: fal` |

_Updated: 2026-08-24 — [como isto é medido](scripts/node_tests/README.md)_
<!-- NODE_SUITE_STATS_END -->

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

## ⚡ Performance — honest numbers, new engine

> **Context.** The engine was rewritten from scratch on a sound value model
> (`PolyValue` NaN-box + shapes + inline caches) and the campaign so far has
> been **correctness-first** (parity badge above). The deleted old engine's
> peak (Monte Carlo AOT **16.9 ms**, 5.4× faster than Bun; HTTP 29k req/s)
> is the documented performance **target to re-clear**, not the current
> state. Numbers below are **end-to-end process time** (startup included —
> AOT runtime init is ~70 ms of every figure) measured now on the new engine.

<!-- BENCH_STATS_START -->
### 📊 Measured benchmarks (auto-updated by CI)

End-to-end process time (includes startup/JIT compile), median of 20 runs after 3 warmups, GitHub Actions `windows-latest` — commit `1ae0643`.

| Bench | Bun | Node | Deno | RTS JIT | **RTS AOT** | AOT vs Bun | AOT vs Node |
|---|---|---|---|---|---|---:|---:|
| Hello/startup | 31 ms | 65 ms | 54 ms | 51 ms | **33 ms** | **0.94×** | **1.96×** |
| Monte Carlo π 10M — vs the same xorshift in JS | 431 ms | 786 ms | 759 ms | 463 ms | **450 ms** | **0.96×** | **1.75×** |
| …the same RTS run, vs JS using native `Math.random` | 89 ms | 260 ms | 214 ms | 464 ms | **447 ms** | **0.20×** | **0.58×** |
| π Machin f64 (RTS only) | — | — | — | 24 ms | **14 ms** | — | — |
| 3M objects allocated (RTS only) | — | — | — | 356 ms | **339 ms** | — | — |
| …the same loop without allocating — the difference is the collector | — | — | — | 47 ms | **35 ms** | — | — |
| 3M objects, reached through a method (RTS only) | — | — | — | 165 ms | **154 ms** | — | — |
| two fields read from classes of 2/5/10/20 (RTS only) | — | — | — | 49 ms | **29 ms** | — | — |
| string indexing, input doubled four times (RTS only) | — | — | — | 668 ms | **652 ms** | — | — |
| one loop, state as a local / captured / property | 262 ms | 530 ms | 432 ms | 214 ms | **196 ms** | **1.34×** | **2.71×** |

_Updated: 2026-08-30 — run locally with `powershell -File bench/benchmark.ps1`_

<!-- BENCH_STATS_END -->

**Why native wins (and where the work is).** RTS compiles TS to machine
code via Cranelift — no JIT warmup, no interpreter tier, native 64-bit
integer arithmetic JS engines can't touch without BigInt.

**Read the Monte Carlo row as the honest statement of where the work is.**
RTS AOT (1.08 s) is barely ahead of RTS JIT (1.12 s), and that gap is the
whole finding: AOT removes the compile step, and the compile step was
never the cost. Both paths emit the same IR, so both pay the same thing.

This paragraph used to say the `PolyValue` NaN-box "only pays where code is
actually polymorphic and the Cranelift egraph folds redundant box/unbox
away". **Measured 2026-08-28, that is not what happens:**

- **The egraph is off.** `opt_level` is left at Cranelift's default, which
  is `none` — no GVN, no LICM, no redundant-load elimination.
  `target/mod.rs` documents this and env-gates the setting behind
  `RTS_CL_OPT`; turning it on moves Monte Carlo by less than the run-to-run
  noise, because the mid-end cannot see across an opaque call and this
  engine's IR is mostly opaque calls.
- **So the box/unbox is not folded.** `rts ir` on `bench/monte_carlo_pi.ts`
  emits **19 `Widen` and 25 `Guard` for 12 real float operations** — every
  intermediate re-boxed to `Tagged` and proven back for the next operation.
- **And it compounds.** A value that arrives `Tagged` fails the
  precondition of `emit/call.rs`'s `machine_operation`, the one place this
  engine turns a library call into an instruction. `Math.floor(x)` is one
  instruction when `x` is a loop local and a **full JavaScript call** when
  `x` reached it through a guard — which is most real code, and it fails
  silently because falling back is a correct answer.

The pass that folds a redundant widen/guard pair across a block boundary is
the single missing piece, and it is missing rather than disabled:
`docs/codegen/the-missing-pass.md` prices it and `ir/fold.rs` declines it by
name. That, not startup and not the value model, is the tuning phase.

---

## 🧰 The runtime stack — the whole `std::*`, in pure Rust

40+ namespaces today — being reshaped into per-module `rts:*` imports
(camelCase, JS globals for everything the language already covers): see
[`docs/engine/architecture.md`](docs/engine/architecture.md). No
dependency on OpenSSL, schannel, libuv, or any external runtime.

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

✅ **Control flow** — `if/else`, `while`, `do-while`, `for`, `for-of`/`for-in`
   (real iterator protocol with `IteratorClose` on break), `switch`
   (native jump table via `br_table` when all cases are integer literals),
   labeled break/continue

✅ **Functions** — declaration, expression, arrow, closures with mutable
   capture (cells), **tail call optimization** (`return f(x)` becomes
   `return_call`), first-class function pointers, `call/apply/bind/toString`,
   `new Function` (runtime compile), spread call `f(...args)`

✅ **Classes** — `constructor`, methods, `this`, `extends`, `super(...)`,
   `super.method(...)`, static methods/fields + `static {}`, getters/setters,
   real `C.prototype` + `constructor.name`, **shape-keyed virtual dispatch**,
   private fields `#x`, **Rust-style operator overload** (`a + b` becomes
   `a.add(b)` at compile time)

✅ **Generators & async** — `function*`/`yield`/`yield*` (lazy state
   machine), async generators + `for await`, `async/await` with a real
   microtask queue (`.then` chains in spec order), Promise combinators
   (`all/allSettled/race/any`), thenable adoption, `withResolvers`

✅ **Objects** — hidden-class shapes + inline caches, getters/setters in
   literals, computed keys, spread, `Object.*` statics, property descriptors,
   freeze/seal, prototype chain, `Proxy` (get/set/delete/apply/ownKeys traps)
   + `Reflect`, `Symbol` (+ well-known, `Symbol.iterator` protocol)

✅ **Data** — TypedArrays/`ArrayBuffer`/`DataView`/`SharedArrayBuffer` +
   `Atomics`, `Map`/`Set`/`WeakMap`/`WeakSet` (TS stdlib), `WeakRef`/
   `FinalizationRegistry` (strong interim, #217), JSON (+JSON5), `Date`,
   `RegExp` (exec/matchAll/named groups/`d` flag), big decimal (`bigfloat`,
   i128 fixed-point ~30 digits), full destructuring (incl. assignment
   targets), template literals + tagged templates + `String.raw`

✅ **Errors** — `try/catch/finally`, real `throw`/instanceof over the Error
   family, error slot + pending-error unwind at call edges

✅ **Web/Node surface** — fetch/Headers/FormData/Blob/streams, URL,
   TextEncoder/Decoder, timers + microtasks, EventTarget/AbortController,
   console, `node:fs/os/path/process/crypto/util` shims, N-API addons
   (`.node`), `import.meta`

❌ **Not there yet** — decorators, full generics/type-checker, real weak
   semantics (#217), `Intl` (partial), full BigInt semantics, real async
   event loop for every path (#207), `var` hoisting edge cases (#301)

---

## 🏗️ Architecture

> **The cutover happened, twice.** The old engine went first; then
> `rts-codegen-new` — the crate this section used to describe — was itself
> **deleted on 2026-08-10**, once `ir`, `eval` and `emit-types` had been
> rebuilt on the engine below. Its doctrine went with it and is the model for
> nothing. Canonical design:
> [`docs/engine/architecture.md`](docs/engine/architecture.md); binding rules
> live in each crate's own `README.md`.

Cargo workspace in `crates/`. `src/` is the facade of the `rts` bin (re-exports
the crates); real paths live under `crates/<crate>/src/`.

**Two crates and a boundary.** `rts-codegen` is the language and knows no
machine; `rts-cranelift` is the machine and knows no language. Either rule alone
is a preference; both at once means a decision has exactly one place it can be
made.

```
crates/
├─ rts-cranelift/    the machine — IR, representations, GC contract, frames,
│                    calls, unwinding. The ONLY crate that touches Cranelift.
├─ rts-codegen/      the language — JS/TS tree, SWC bridge, emit, type pass
├─ rts-core/         the runtime — values, heap, objects, coercion, entry points
├─ rts-host/         where the three meet, and where a program runs
├─ rts-macro/        #[rtse::entry] / #[rtse::class] — declare one, derive four
├─ rts-std/          the `rts:` surface, and the globals
├─ rts-node/         the `node:` surface (fs, os, path, process, crypto, util…)
├─ rts-runtime/      the AOT staticlib a compiled program links against
├─ rts-napi/         N-API (.node addons) — 146 symbols, a real addon runs
├─ rts-dom/          headless HTML+CSS engine (DOM → cascade → layout → paint)
├─ rts-egui/         window/paint backend (egui/wgpu)
├─ rts-render/       rts-input/  rts-dom-bridge/  rts-physics/  rts-ui/
├─ rts-linker/       native link (system linker + object fallback)
└─ rts-cli/          run · compile · test · ir · eval · emit-types · repl
```

### Pipeline

```
TS → SWC → rts-codegen syntax tree → emit → rts_cranelift::ir → verify
   → lower/ → Cranelift → JIT (executable memory) | AOT (object + link)
```

One path, no MIR tier. `rts ir` prints the middle stage — this engine's own
representation, not Cranelift's `.clif`.

**What optimizes it: almost nothing, and that is the honest statement.** This
section used to claim the Cranelift egraph (`use_egraphs=true`) was the
optimizer. `opt_level` is left at Cranelift's default of `none`, which gates the
egraph mid-end out entirely — `target/mod.rs` documents that and puts the knob
behind `RTS_CL_OPT`, where turning it on measures as noise because the mid-end
cannot see across an opaque call. So the box/unbox pairs the front end inserts
are **not** folded away by anything; see the performance section above and
[`docs/codegen/the-missing-pass.md`](docs/codegen/the-missing-pass.md).

The front end does what Cranelift cannot — JS semantics:
`ToNumber`/`ToString`/`ToBoolean` coercions, the polymorphic `+`, widen/guard
insertion, shape and inline-cache site emission, narrow-int wrap, exception
edges. AOT and JIT share one lowering and differ only in the destination.

**One source, generated views.** A runtime symbol is declared by an attribute
and never written by hand: `#[rtse::class]` derives the wrappers, the install
lists, the registration **and** the TypeScript declaration `rts emit-types`
prints — four views from one `impl` block. There is no symbol table to bake,
because a native here is a **function pointer beside a cell**, not a name a
linker resolves. See
[`docs/engine/authoring-natives.md`](docs/engine/authoring-natives.md).

The one permanent exception is `rts-napi`'s 146 `napi_*` declarations: a foreign
C ABI whose names *are* the interface, since a compiled `.node` addon links
against those exact strings.

**Where the three crates agree, they are made to.** An entry point crosses as
ABI scalars, and `rts-host` is the only crate that may name the compiler's
statement of the entry-point set and the runtime's derivation of it at once — so
that is where the two are asserted equal, by name, rather than assumed.

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
rts test [path]                  # the *.test.ts corpus
rts ir file.ts                   # this engine's own IR, no execution
rts emit-types [out.d.ts]        # TypeScript declarations, from #[rtse::class]
rts init my-app                  # project scaffolding
rts i [pkg@version …]            # install from package.json or args
rts clean                        # remove generated object/cache directories
```

---

## 🔬 Codegen debugging

Want to see exactly what the engine emitted?

```bash
rts ir file.ts 2>&1 | head -50
```

Prints **this engine's own IR** — not Cranelift's `.clif`, which only exists
inside `lower/` after every decision has already been taken. A callee legend
sits at the top, so a `Call { callee: FuncId(7) }` can be read back to a name.

It is the tool for the questions that matter here: how many `Widen`/`Guard`
pairs surround each real operation, which operations became runtime calls
instead of instructions, and where a throw check follows something that cannot
throw. [`docs/guides/reading-ir.md`](docs/guides/reading-ir.md) is how to read
it; [`docs/codegen/`](docs/codegen/) is what past readings settled.

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
- 📖 [`the specs removed 2026-08-03 (see git history)`](the specs removed 2026-08-03 (see git history)) — technical feature specs
- 🗺️ [`docs/engine/architecture.md`](docs/engine/architecture.md) — canonical plan of the engine redesign
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
