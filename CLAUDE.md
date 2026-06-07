# CLAUDE.md

## RULE #0 — MANDATORY ABSOLUTE META-RULE

**Before starting ANY task, you MUST read this CLAUDE.md in full and follow
ALL rules it defines — no exceptions, no omissions, no "picking the important
ones". Every rule in this file is binding.**

This is the first and most important rule. It governs all others.

### How to apply

1. On the first message of each session (and whenever this file changes), read
   `CLAUDE.md` end to end before touching code.
2. Each `## MANDATORY RULE:` section is binding even when the task context seems
   not to require it.
3. Each `## Conventions`, `## Rules`, `## ABI ...`, `## Structure ...` section
   defines conventions that must be respected in any code change.
4. If a rule conflicts with a user instruction, ask for confirmation before
   violating the rule. Do not decide alone.
5. If a rule is stale (code no longer matches), update CLAUDE.md in the same PR —
   never leave a lying rule in effect.

### Current mandatory rules in this file

- **RULE #0** (this) — read and follow everything
- **MANDATORY REQUIREMENT: local-rules.md** (check and read if it exists)
- **MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)**
- **MANDATORY RULE: FOLLOW `../ROADMAP-CORRECAO.md`** (topological order for
  cross-runtime bug fixes; detailed in `.claude/rules/00-meta.md`)
- **CROSS-RUNTIME PUSH MODE (parity ≥ 90%)** — while active, the process
  constraints above are suspended to push parity to 100%; the honesty + build
  floor never lift

Keep this list in sync with the sections below.

## MANDATORY REQUIREMENT: local-rules.md

Before starting any task, you **MUST** check whether `local-rules.md` exists at
the project root. **If it exists, reading it is mandatory** — not optional, do
not skip, do not assume content, do not proceed without reading. If it does not
exist, proceed normally. When present, treat its content as additional rules set
by the developer working on this local copy; they take priority over generic
preferences for the whole session. `local-rules.md` is per-developer and is not
versioned (already in `.gitignore`).

## MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)

Regression is allowed when necessary — but it must **always be explicit and
justified**, never silent. This replaces the old "zero regression" rule.

Minimum suite before merge:

```bash
cargo build --release             # clean build
cargo test --release --lib        # unit + integration
target/release/rts.exe test       # TS suite (if PR touches runtime/codegen/GC)
```

### Practical rules

- **Run the full suite before merge.** You must know exactly which tests pass and
  which regress. "It broke and I don't know why" is never acceptable.
- **A regression is acceptable only when** (a) it is intentional (changed
  behavior / removed feature) or a necessary tradeoff for the change, **and**
  (b) it is documented explicitly in the commit/PR with justification.
- **Silent or unexplained regression still blocks merge.** Each regressing test
  must be either updated to the new expected behavior, or listed explicitly as a
  known regression with reason + tracking issue.
- **A broken build blocks merge** unless explicitly justified in the same PR.
- **Codegen tests/fixtures (`tests/*.test.ts`, `tests/fixtures/*`) are part of
  the suite.** If behavior changed on purpose, update them and justify.
- **Large multi-area PRs run the suite incrementally** during development, not
  only at the end.

### Why this rule exists

With 2 devs + AI accelerating velocity, the danger is *silent* regression
piling up until the suite becomes a lie (green tests, broken uncovered paths).
The discipline here is not "never break a test" — it is "never break a test
without knowing and saying so". Explicit, justified regression is fine;
invisible regression rots the project.

## CROSS-RUNTIME PUSH MODE (parity ≥ 90%) — process constraints suspended

**Active when** cross-runtime parity ≥ 90% (currently 94.3%, badge in
`README.md`). The goal flips to reaching **100%**, and the *process* constraints
below are SUSPENDED so change can land at any cost — except the honesty + build
floor, which never lifts. When parity drops back below 90%, this mode
deactivates and the suspended rules resume automatically.

### Suspended while active
- **Roadmap topological order** (`FOLLOW ../ROADMAP-CORRECAO.md`) — pick any
  fixture / feature / epic in any order.
- **Ask-before-regression** — regressions may land without per-change
  confirmation. Still *logged* in the commit/PR body; net parity across a work
  session must not drop.
- **Small-PR scope** — large multi-crate refactors and the deferred epics
  (#195 mutable closures, #207 async event loop, #216/#222 Symbol, #218 Proxy,
  #219 BigInt, #223 dynamic import) are now in scope.
- **Ceremony** — progress-bar / read-everything ritual is optional.

### Never suspended (honesty + build floor)
- **The parity number stays real.** No deleting, disabling, skipping,
  hardcoding, or input-special-casing a fixture to inflate parity. A fixture
  counts as passing only when the runtime genuinely produces the correct output
  through the same code path any other input would take.
- **No crashing / hanging code committed as "pass".** ACCESS_VIOLATION /
  Cranelift verifier error / stack overflow / infinite loop on a fixture means
  it did **not** pass.
- **Build must compile.** A broken build still blocks merge.

### Rationale
Per `MAINTENANCE.md`, the remaining fixtures need full feature completion, not
bounded patches — a half-feature crashes rather than producing a wrong-but-closer
output, so there is no "regress X to pass Y" trade to police. The process
ceremony slows that work without guarding the only real risk (faking the
metric); the honesty floor guards that directly.

## Project

RTS is a TypeScript-to-native compiler/runtime using Cranelift as codegen
backend. Goal: compile TS/JS to native binaries with a minimal Rust runtime,
shipped as a standalone toolchain (no external runtime support library).

Runtime is organized around the `crates/rts-abi/` + `SPECS` contract, with a
module-graph pipeline + incremental cache. Two execution paths: JIT via
`cranelift_jit::JITModule` (`rts run`, direct executable memory) and AOT via
`cranelift_object::ObjectModule` (`rts compile`, external linker).

See `RTS_REFACTOR.md` for the current refactor direction (crate workspace).

## Architecture

Cargo workspace, 10 crates in `crates/`. `src/` is the `rts` bin facade
(re-exports); `src/main.rs` calls `rts_codegen::register_runtime_artifacts` +
`rts_cli::cli::dispatch`. Real paths live under `crates/<crate>/src/`.

```
crates/
  rts-ast/         — internal AST
  rts-parser/      — SWC parse; arrow/fn expressions → top-level Item::Function
  rts-diagnostics/ — structured errors
  rts-abi/         — single ABI contract (SPECS, types, symbols, guards,
                     signatures, Intrinsic, global_class.rs, handles.rs)
  rts-hir/         — typed HIR (I8..I128/F32/F64/Bool/Str/Handle/Array/Function/
                     Class/Object/Any/Unknown)
  rts-mir/         — SSA MIR (60+ Insts; Terminators Return/Jump/Brif/Switch/
                     TailCall/Trap; passes fold/fma/cse/dce/narrow/verify/inline)
  rts-codegen/     — Cranelift codegen + type_system + module/ + pipeline + cache
    src/codegen/
      emit.rs      — ObjectModule emitter (AOT)
      object.rs    — ObjectArtifact wrapper (use-slicing, AOT)
      jit.rs       — JITModule emitter (rts run)
      lower/       — AST authoritative lowering over &mut dyn Module
        expressions/ statements/
      mir_codegen/ — MIR → Cranelift IR (default; auto-fallback to AST on bail)
    src/type_system/ — type checker, registry, resolver
    src/module/      — module resolver + dependency graph
    src/nodespace/   — Node.js builtin shims (fs, os, path, process, crypto, util)
    src/pipeline.rs  — orchestrates build/run (incl. run_jit)
  rts-runtime/     — builtin module "rts" + "rts:<ns>" submodules + 40+ namespaces
    src/namespaces/  — runtime namespace impls
      globals/       — global JS classes (number, string, date, regexp, ...)
    src/runtime/     — async_rt.rs (global tokio), tokio_ctx.rs (sync/async bridge)
  rts-linker/      — native link (system linker + object backend fallback)
  rts-cli/         — CLI (run, compile, apis, init, repl, eval, ir)

src/                — bin facade (re-exports), runtime_objects.rs, main.rs
```

> `rts-codegen` became a catch-all (pipeline, type_system, module, cache,
> eval_jit), diverging from `RTS_REFACTOR.md`. Phase 3 (MIR) delivered — MIR
> default since f7b924b/23dd4b7. Phase 4 in progress, 5/8 done: atomics (4.1),
> inline+integration+fixed-point (4.2/4.3/4.7), CSE (4.5), FMA (4.8), arr[i]=v +
> e2e smoke (4.4/4.6). Remaining: escape analysis, SIMD, real narrow storage.

Pipeline (default, MIR ON):

```
TS → SWC → AST → HIR → MIR → inline (fixed-point, ≤4 iters)
                          → optimize (fold → fma → cse → dce)
                          → mir_codegen → Cranelift → JIT/AOT
                          ↘ AST authoritative (auto-fallback)
```

Hybrid routing via `RTS_USE_MIR`: unset/`1`/`on`/`all` = MIR ON (default);
`0`/`off`/`none` = AST only; `fn1,fn2,...` = MIR only for listed fns. Each user
fn tries HIR→MIR→Cranelift; on an unmodeled construct (member on `this`/objects,
classes, async/await, address-taken fns, string in user-fn params/ret) it falls
back to AST codegen silently, no semantics lost. Both AOT/JIT share
`compile_program`; `FnCtx.module` is `&mut dyn Module`.

## ABI (`crates/rts-abi/`) — single contract

All surface between codegen and runtime goes through `crates/rts-abi/`. No
per-namespace `SPEC/MEMBERS/dispatch()`, no `__rts_call_dispatch`.

- `abi::SPECS` (`mod.rs`) — static slice of every registered namespace (40+).
  Single source consumed by codegen, runtime, JIT, and the `rts.d.ts` generator.
- `abi::lookup(qualified)` — `"io.print"` → `&NamespaceMember`.
- `abi::global_class_lookup(class, method)` — resolves global JS class methods
  (`Number.isNaN`, `Date.now`, …) via `GLOBAL_CLASS_SPECS`.
- `member.rs` — `NamespaceSpec`, `NamespaceMember`, `Intrinsic`. Each member:
  `name`, `kind` (`Function | Constant | AsyncFunction`), `symbol`, `args[]`,
  `returns`, `doc`, `ts_signature`, `intrinsic`. When `intrinsic` is `Some`,
  codegen emits Cranelift IR directly instead of `call <symbol>`.
- `global_class.rs` — `GlobalClassSpec` + `GLOBAL_CLASS_SPECS`: registry of
  builtin global classes (Number, String, Date, RegExp, Error, EventEmitter,
  TextEncoder/Decoder, Response, Promise, URL, console, timers, fetch,
  performance) with static + instance methods.
- `handles.rs` — `HandleTable` ABI constants/helpers (encode/decode gen+slot).
- `types.rs` — `AbiType`: `Void | Bool | I32 | I64 | U64 | F64 | StrPtr |
  Handle`. `StrPtr` = 2 Cranelift slots (`ptr` + `len`). `Bool` maps to `I64`.
- `signature.rs` — `lower_member()` → Cranelift `LoweredSignature`.
- `symbols.rs` — convention `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` (e.g.
  `__RTS_FN_NS_IO_PRINT`, `__RTS_FN_GL_NUMBER_IS_NAN`). Macro `rts_sym!`;
  `validate_symbol()` enforces uppercase ASCII.
- `guards.rs` — `guard_for(expected, caller)` decides passthrough/coerce/trap at
  call sites with `any` args.

### Machine ABI — typed extern "C", no dispatch

No `JsValue`, no `__rts_call_dispatch`, no boxing at the codegen/runtime
boundary. Each namespace function is a typed `extern "C"` symbol.

| TS type  | `AbiType`    | Cranelift repr                | Note                          |
|----------|--------------|-------------------------------|-------------------------------|
| `number` | `I64`/`F64`  | `i64`/`f64`                   | native bits, no boxing        |
| `bool`   | `Bool`       | `i64` (0/1)                   | extern "C" returns i64        |
| `string` | `StrPtr`     | 2 slots `(i64 ptr, i64 len)`  | UTF-8; static ptr or GC buffer|
| handle   | `Handle`     | `u64`                         | `HandleTable` (gen:16+slot:48)|
| void     | `Void`       | —                             | no return                     |
| ints     | `I32`/`U64`  | `i32`/`u64`                   | counts, status, sizes         |

- Each member is `#[unsafe(no_mangle)] pub extern "C" fn __RTS_FN_NS_<NS>_<NAME>`
- No namespace fn accepts/returns `JsValue` at the `extern "C"` boundary
- Dynamic strings are GC-allocated and return a `u64` handle; read via
  `gc::string_ptr(handle)` + `gc::string_len(handle)`
- `any`-typed call args go through `abi::guards::guard_for(...)`

## Per-namespace file structure

```
crates/rts-runtime/src/namespaces/<ns>/
  mod.rs       — re-export submodules + publish NamespaceSpec
  abi.rs       — NamespaceMember declarations (static table, source of truth)
  <group>.rs   — operational impl grouped by responsibility (read/write/dir/…)
```

`mod.rs` is import map + spec export only. No per-namespace `dispatch()` — each
function is a direct `#[no_mangle] extern "C"`.

### Active namespaces (40+)

`io`, `fs`, `gc`, `math`, `num`, `bigfloat`, `time`, `env`, `path`, `buffer`,
`string`, `process`, `os`, `collections`, `hash`, `fmt`, `crypto`, `net`, `tls`,
`thread`, `atomic`, `sync`, `parallel`, `mem`, `hint`, `ptr`, `ffi`, `regex`,
`runtime`, `test`, `trace`, `ui`, `alloc`, `json`, `date`, `http_server`,
`promise`, `events` + `globals/` sub-namespaces. Covers std::* + parallelism +
HTTPS + UI + JSON + Date + native HTTP server (actix-web) + global JS classes.

Highlights:
- `gc/` — string pool + `HandleTable` (slab, 16-bit gen + 48-bit slot). `Entry`:
  String, BigFixed, Buffer, ProcessChild, Map, Vec, Function, PromiseAsync, Free.
- `math/` — basic/trig/minmax/consts/random (xorshift64). Intrinsics: sqrt,
  abs/min/max f64/i64, random_f64.
- `bigfloat/` — i128 decimal fixed-point (scale ≤36); pi via Machin + Maclaurin.
- `crypto/` — SHA-256 inline (FIPS 180-4), base64/hex, CSPRNG (BCryptGenRandom /
  /dev/urandom). Streaming SHA-256 via `sha2` crate.
- `net/`+`tls/` — TCP/UDP/DNS via std::net; TLS 1.2/1.3 via rustls + webpki-roots
  (HTTPS end-to-end, no OpenSSL/schannel).
- `thread/` — 4 mechanisms (std spawn+join; tokio spawn_blocking; tokio
  fire-and-forget; fixed 8-worker pool). Comparison table in `thread/abi.rs`.
- `http_server/` — native HTTP/1.1 via actix-web over shared tokio. Sync→async
  bridge. Peak ~29k req/s.
- `parallel/` — rayon map/for_each/reduce; backs the silent-parallelism passes.
- `regex/` — `regex` crate. `runtime/` — eval_file/eval + hot-reload. `ui/` —
  FLTK 1.x. `trace/` — Bun-style frame stack. `events/` — EventEmitter.

### Globals (`crates/rts-runtime/src/namespaces/globals/<class>/`)

Each: `mod.rs` (spec) + `abi.rs` (member table) + `rt.rs` (extern "C" impl).
Registered in `GLOBAL_CLASS_SPECS`, resolved by codegen via `global_class_lookup`.

`number`, `string`, `date`, `regexp`, `error`/`TypeError`/`RangeError`/
`SyntaxError`, `events` (EventEmitter), `console`, `json`, `timers`, `fetch`,
`performance`, `global_this` (globalThis/undefined/null/Infinity/NaN + isNaN/
isFinite/parseInt/parseFloat/encode/decodeURIComponent), `text_encoding`
(TextEncoder/Decoder), `url` (URL + URLSearchParams), `symbol` (Symbol + well-
known), `weakmap`/`weakset` (strong semantics, #217 tracks weak refs), `boolean`.

## Runtime internals

### HandleTable shard-aware
32 lock-free shards. `alloc_entry` round-robins by thread; `shard_for_handle`
decodes O(1) from low bits. All 17+ handle-based namespaces migrated.

### Shared tokio runtime (#399)
`crates/rts-runtime/src/runtime/async_rt.rs` exports `rt()` — global multi-thread
`OnceLock<Runtime>`. `on_thread_start`/`stop` hooks register each worker in
`gc/thread_registry` so the GC scanner sees live handles in tokio tasks. Every
async feature reuses this runtime. What crosses the JIT (extern "C") is only an
opaque u64; Rust-rich types (Arc<T>, Channel, JoinHandle, JITModule) live in the
shard map keyed by that id, or in GC handles with a lifetime guard.

### GC — mark+sweep with Cranelift stack maps
GC is precise mark+sweep using Cranelift `UserStackMap`, with conservative scan via
`SuspendThread + GetThreadContext` for all registered threads. Codegen calls
`declare_value_needs_stack_map(val)`; `jit.rs` registers return-PCs in
`stack_map_registry`. Every `GC_TICK_INTERVAL = 256` allocs, `finish_cycle()`
runs `mark_stack_roots()` + `sweep_all_shards()`. `mark_stack_roots()` on Windows
uses `GetCurrentThreadStackLimits` (Win32) — **not** `gs:[0x10]` (TIB.StackBase
sometimes < RSP → scanner marks nothing → live handles collected; bug PR #400).

### State
No central state system — each namespace owns its own via `Arc<Mutex<T>>`
(`OnceLock` init) or `thread_local!` caches.

## Language capabilities (codegen)

- Object/array literals via `collections.map_*`/`vec_*`.
- Classes: constructor, method, this, extends, super(args), super.method,
  static, getters/setters. `__rts_class` tag → real virtual dispatch.
- Rust-style operator overload: `a + b` → `a.add(b)` at compile time when the
  class defines the method.
- `for...of` over arrays; try/catch/finally phase 1 (thread-local error slot, no
  real unwind — #128 phase 2). String equality via `gc.string_eq`.
- async/await Promise-centric (#437). Function class: call/apply/bind/toString +
  name/length + `new Function("body")` via runtime eval.
- Destructuring (#210): array/object, defaults, rest, nested, in params/for-of/
  catch, alias.
- Expanded JS builtins (epic #226): Array/Object/Math/String/Symbol/URL/Date/
  TextEncoder + encode/decodeURIComponent + WeakMap/WeakSet + Boolean + parseInt
  radix. See `.claude/rules/03-features.md` for the per-category list.

### async / Promise / Function (#437)

`async function f(...)` → `expand_async_functions` rewrites to
`f = (args) => promise.create(__async_inner_f, args)`. `promise.create(fn, args)`
allocates a pending PromiseAsync, resolves the fn, `rt.spawn_blocking(invoke +
settle)`. `await x` → `promise.wait(x)`. Function payload =
`Entry::Function { fn_ptr, arity, name, bound_this, bound_args, is_arrow,
source, keep_alive }`. `invoke_n` trampoline transmutes to
`extern "C" fn(i64...) -> i64`. Known limits: thisArg ignored in `.call`,
no `fn.prototype`/`arguments`, no async in `new Function`. Spec:
`docs/specs/async-promise-function.md`.

### Silent parallelism (Level-1)

3 codegen passes rewrite common TS to `parallel.*` automatically (user never
mentions threads): `array_methods_pass` (`arr.map/forEach/reduce(userFn)`),
`reduce_pass` (accumulator loop, associative ops only), `purity_pass`
(`for...of` calling only `pure: true` members, no assignments). 96 fns marked
`pure: true`. Spec: `docs/specs/silent-parallelism.md`.

## Codegen optimizations

- **Intrinsics inline** (`abi::Intrinsic`): sqrt, abs_f64, min/max_f64, abs_i64,
  min/max_i64, random_f64 → direct Cranelift IR.
- **TCO**: user fns in `CallConv::Tail`; `return f(x)` → `return_call`
  (needs `preserve_frame_pointers=true` on x86-64).
- **First-class fn pointers** (#97 ph1): `Expr::Ident` → `func_addr` i64;
  `call_indirect` with provisional Tail sig.
- **Jump table switch**, **imm forms** (`iadd_imm`/`band_imm`/`ishl_imm`),
  **MemFlags::trusted** on global/RNG loads, **f64 mod via libc fmod**,
  **constants as properties** (`math.PI`).
- **MIR passes** (`mir_codegen/passes/`): fold (const fold + strength
  reduction), fma (`a*b+c`, conservative), cse (intra-block), dce (fixed-point,
  preserves side-effects), inline (`INLINE_BUDGET=16`, fixed-point ≤4 iters),
  narrow (I8/U8/I16/U16 canonicalization), verify.

### Inline asm (`std::arch::asm!`) — legitimate, in use

Used where safe Rust can't express ABI/register control. Live cases:
`gc/collector.rs` (`mov {}, rsp` for the root scanner) and
`globals/function/ops.rs::invoke_all_i64` (#1281, Win64 trampoline, dynamic
arity N — replaced an arity-≤8 match that gave wrong results / ACCESS_VIOLATION).
Rules: always `#[cfg(...)]` per target + portable fallback; list all clobbers
(`clobber_abi("win64")` conflicts with explicit `out("rax")`); respect target
ABI (Win64: 4 reg args + 32 shadow + 16-aligned before `call`); document the
assumed convention; zero-regression discipline still applies.

## Conventions

- Code language: Rust (English identifiers). Communication language: Portuguese.
- Conventional commits: `feat:`, `fix:`, `perf:`, `refactor:`, `docs:`, `chore:`.
- New namespace must be registered in `abi::SPECS` (and the generated `rts.d.ts`).
  CI lints the committed `rts.d.ts` against the generator.
- Build via `cargo` directly — `xtask` removed.

### Design rules
- Don't implement high-level APIs in Rust — Rust exposes only raw primitives via
  `"rts"`. Global JS classes live in `globals/<class>/` + `GLOBAL_CLASS_SPECS`.
- `rts.d.ts` contains only `declare module "rts"`.
- Numeric handles (u64) for runtime resources.
- Standalone distribution: runtime resolved by precompiled `.o/.obj`
  (`RTS_RUNTIME_OBJECTS_DIR` or `runtime-objects` next to `rts`); no build-time
  external download.

### No legacy code
Dead code is removed immediately — never comment out, never "just in case". Code
not reached by any live path is deleted in the same commit that killed it.
`todo!()`/`unimplemented!()` are acceptable WIP markers; commented code is not.
`dead_code` warnings are treated as errors.

## Progress bar for long tasks

For multi-step work (new namespace, multi-file fix) show an ASCII progress bar
per significant change:

```
[▰▰▰▱▱▱▱▱▱▱] 30% — short current-step description
```

10 segments, real percentage. Update on each concrete change (file created,
build passed, test ran, commit made). On error: prefix `❌ erro:` and roll back
to where confidence dropped. Final: `[▰▰▰▰▰▰▰▰▰▰] 100% ✅ — summary (PR #N, X/Y)`.

## GitHub issues

When starting an issue, mark it taken first (`gh issue comment <num>` and, if
collaborator, `gh issue edit <num> --add-assignee @me`). On finishing (PR
merged), comment with the PR link and close when appropriate.

## Testing creativity

Don't stop at happy-path. Cover variations in `tests/`: empty/conditional/
nested/in-loop/in-try-catch/in-member-call; combine with adjacent features;
TS/JS edge cases (undefined, null, recursion, tail call, reserved words). When a
variation fails out of the current PR's scope, open an issue with the minimal
repro and remove it until the follow-up. Tests live in `tests/*.test.ts`
(`rts:test`). Pre-compute values at top-level before `describe` (calling
instance methods inside `test()` closures can hit GC: handle collected before
use).

## How to test

```bash
cargo test --lib                                              # Rust unit tests
cargo build --release                                         # release build
$env:RUST_BACKTRACE="full"; target/release/rts.exe run file.ts            # JIT
$env:RUST_BACKTRACE="full"; target/release/rts.exe compile -p file.ts out # AOT
$env:RUST_BACKTRACE="full"; target/release/rts.exe test tests/foo.test.ts # TS suite
target/release/rts.exe apis                                   # list APIs
```

**Mandatory:** always set `RUST_BACKTRACE=full` before running `rts.exe`.
Without it crashes show a shallow stack; the crash handler (`src/crash.rs`)
needs it for full frames.

### Fast iteration: `cargo run -- run` vs `build --release`

| Command | When | Full rebuild | Binary |
|---|---|---|---|
| `cargo run -- run file.ts` | iterate codegen/runtime fix, "does it compile + run" | ~30s (debug) | ~10x slower |
| `cargo run --release -- run file.ts` | one-shot release | ~100s | fast |
| `cargo build --release` + `target/release/rts.exe run` | benchmarks, full TS suite | ~100s | fast |
| `target/release/rts.exe run file.ts` | re-run `.ts` with no Rust change | 0s | fast |

`cargo run` always checks staleness and recompiles — there is no "run without
compiling". Debug compiles ~3x faster but runs ~10x slower; **never benchmark in
debug**. If you only changed `.ts`, call `target/release/rts.exe` directly. Note
`cargo run` wraps the program exit code (program exit 1 → cargo "didn't exit
successfully"); expected, not a bug.

### Debugging individual failures
Always run the single failing file before the full suite (avoids timeout/noise):

```bash
target/release/rts.exe test tests/foo.test.ts
target/release/rts.exe ir tests/foo.test.ts 2>&1 | head -60
```

`rts ir` diagnoses: "unknown namespace member X.Y" (missing codegen handler /
ABI entry), SIGILL (invalid IR), access violation (null ptr load/store), wrong
result (iconst 0 placeholder, bad cast). Rebuild before debugging suspected
failures — `target/release/rts.exe` may be stale after merges.

### `rts ir` for perf

`target/release/rts.exe ir file.ts 2>&1 | head -100` prints full Cranelift IR
per user fn + `__RTS_MAIN` (stderr, no execution). Use when suspecting
inefficient codegen: redundant load/store in hot loops (vars not promoted to
Cranelift Variables), duplicated lowered subexpressions
(try_operator_overload/try_bin_imm lowering before checking use), unneeded
`uextend` before `brif`, f64↔i32 conversions in hot loops, repeated
`global_value`, extern calls that could be inline intrinsics. Example (4a418d1):
`x*x + y*y <= 1.0` had 6× `fmul x x` in IR; fix → 1× each (~6% faster Monte
Carlo). Use `-e`/`eval` for snippets (no relative imports).

## Benchmarks

Canonical in `bench/`: `monte_carlo_pi.ts`, `pi_bigfloat.ts`, `pi_machin.ts`.
Scoreboard (medians, 2026-05-01):

| Bench                       | RTS JIT | RTS AOT | Bun     | Node     |
|-----------------------------|---------|---------|---------|----------|
| Monte Carlo 10M             | 26.8 ms | 16.9 ms | 91.8 ms | 113.9 ms |
| Monte Carlo 10M (8 workers) | 30.3 ms | —       | 147.6 ms (Workers) | — |

RTS AOT vs Bun: **5.14×**. RTS multi-thread vs Bun Workers: **4.66×**. HTTP
server peak **29k req/s** (78% of pure-Rust actix). Full suite:
`powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1`.

## Runtime vs Compile

Both share the same Cranelift codegen via `compile_program`; `FnCtx.module` is
`&mut dyn Module`. `rts run` → JITModule, in-memory, all ABI symbols registered
in `JITBuilder::symbol` (`jit.rs`). `rts compile` → use-slicing, only needed
module objects, final binary. Object naming: `<module>.o` (`.m` for cache
metadata).

## Artifact layout (roadmap phase 1, in progress)

```
<project>/
  src/main.ts  package.json  tsconfig.json
  node_modules/.rts/objs/{runtime/,compile/}   modules/ (.ometa cache)
  release/<project_name>   — only on rts compile
```

## Status — epic #226 (JS/TS parity)

TS suite: **1015/1015 (100%)**. Heavy child issues still open (need refactor,
out of small-PR scope): #195 mutable closures, #207 real async event loop, #216
Symbol computed key, #217 weak WeakMap/Set + FinalizationRegistry, #222 Map/Set
Symbol.iterator, #223 dynamic import, #301 var hoisting in user fn, #304
toString/valueOf coercion, #305 integer overflow (>~9e18 saturates), #477
infinite generator, #211/#219/#225 generators/BigInt/Intl (candidate-discard).

## Docs

`docs/specs/` holds feature specs, design decisions, technical notes — index at
`docs/specs/INDEX.md`. High-level direction in `RTS_REFACTOR.md`. Detailed rules
in `.claude/rules/` (00-meta → 05-codegen-notes), each binding.
