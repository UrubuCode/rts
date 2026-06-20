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
- **MANDATORY RULE: PRIMORDIAL-vs-REGISTRY DOCTRINE** (engine names only
  primordials; everything else via the Registry/SPECS; NO builtins in the engine)
- **MANDATORY RULE: FOLLOW THE REDESIGN DESIGN DOC** — the canonical plan is
  `docs/specs/rts-codegen-new-design.md`; pick work from its migration phases
  (P0→P5), not from a fixture-grind roadmap (see "HONEST CURRENT STATUS" below
  and `.claude/rules/00-meta.md`)
- **MANDATORY RULE: read_before_commit.sh GATE + FILE LAYOUT** — run
  `bash read_before_commit.sh` and read its whole output before every commit
  touching the engine; no engine source file > 500 lines (split into a
  folder/subfolder); the engine names ONLY primordials —
  `rts-shared`/`rts-std` are **NOT** native/primitive and a direct mention is a
  regression

Keep this list in sync with the sections below. The honesty + build floor
(parity number stays real, no crash/hang committed as "pass", build must
compile) never lifts under any mode.

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

## MANDATORY RULE: read_before_commit.sh GATE + FILE LAYOUT

Before **every** commit that touches `crates/rts-codegen-new/`, run the gate and
read its full output:

```bash
bash read_before_commit.sh            # full gate (includes cargo build)
bash read_before_commit.sh --no-build # fast static-only pass while iterating
```

The gate (at the repo root) encodes the binding rules below so a commit cannot
silently violate them. It separates **HARD** failures (exit non-zero — never
commit) from **REVIEW** lists (read every entry; pre-existing debt must shrink,
never grow).

### Rule A — the engine is native/primitive-only; rts-shared/rts-std are NOT

The engine (`rts-codegen-new`) interacts directly ONLY with the native/primitive
surface: the PRIMORDIAL classes via `rts-primitives`, reached through the
`rts-runtime` facade. **`rts-shared` and `rts-std` are NOT native/primitive** —
they are the non-primordial utility/backend libraries. A direct dependency on,
`use` of, or hardcoded mention of `rts-shared`/`rts-std` (or any non-primordial
class: `Map`/`Set`/`Date`/`Symbol`/`URL`/`Proxy`/…) **in the engine is a
REGRESSION**. Everything non-primordial resolves through the **Registry**
(`registry.rs` / `registry_call.rs`), never a hardcoded per-class path. This is
the same PRIMORDIAL-vs-REGISTRY doctrine below, restated as a commit gate. A
dedicated `*class.rs` (e.g. `dateclass.rs`) is a **draining target** — never add
a new non-primordial path to one.

> The gate HARD-fails on a forbidden dep/`use`; it REVIEW-lists every
> non-primordial class name found in codegen (test/fixture files are split out as
> expected). The current draining targets are `front/run/dateclass.rs` and
> `front/run/globalclass.rs`.

### Rule B — no source file over 500 lines

No file under `crates/rts-codegen-new/src/` may exceed **500 lines**. When a file
would grow past 500, split it into a **folder/subfolder** of cohesive submodules
(the `mod.rs` + sibling-files pattern already used across the engine). The gate
REVIEW-lists every offender; the list must only shrink. New code lands in a
small, focused module, not appended to an already-oversized file.

### Rule C — resolve blocking limitations first (focus shift is allowed)

When the main feature you picked is blocked by a missing engine capability
(e.g. mutable env-record captures before async, a Registry hook before a class
migration), it is correct to **shift focus and implement the blocker first**,
then return to the main feature. State the shift explicitly in the commit/PR.
Keep the change modest and incremental — small focused modules, gate green.

## HONEST CURRENT STATUS — engine redesign in progress (strangler-fig)

The project is mid-redesign of its codegen engine. Read this so you do not act on
stale numbers or a stale architecture.

**The truth about the 100%.** On 2026-06-06/07 RTS reached **100% cross-runtime
parity** (372/372, 0 divergences; TS suite 1719/1719), tag `v0.0-202606072107`,
commit `27e16378`. Factual and git-verifiable — **but** it was the *local maximum
of a hardcoded approach* on the OLD engine, not validation of its design. That
engine's value model (a single `i64` ABI slot overloaded to mean
int/handle/boxed-float/string/sentinel, with the type tag smeared across four
compile-time side-tables) was admitted to be the wall by the old engine's own
(now-deleted) `MAINTENANCE.md`. It is **unsound by construction** and does not
scale.

**The current number.** After the 100% the fixture set grew 391 → 612 (harder
cases) and parity is now **70.7%**. That is the honest figure to quote. Do not
cite "94.3%" or "100%/push mode" — those framings are dead.

**The redesign.** A ground-up engine is being built **strangler-fig style behind
the frozen old one**. Frozen old engine: `crates/rts-codegen-old/` (still plugged
into the bin/cli). Active redesign: `crates/rts-codegen-new/`. The canonical plan
is `docs/specs/rts-codegen-new-design.md` — **read it before any engine work**.
Its thesis: *prove-monomorphic-and-unbox where the type system can (keep the
winning numeric path); fall to ONE honest in-value tagged representation
(`PolyValue`, a 64-bit NaN-box) + hidden-class shapes + AOT-safe data inline-
caches where it can't.*

### How work is picked now
Follow the design doc's **migration phases (P0→P5)**, highest-leverage first, not
a fixture-by-fixture grind. Large multi-crate work and the deferred epics (#195
mutable closures, #207 async event loop, #216/#222 Symbol, #218 Proxy, #219
BigInt, #223 dynamic import) are in scope where a phase calls for them.

### The honesty + build floor (NEVER lifts, no mode suspends it)
- **The parity number stays real.** No deleting, disabling, skipping,
  hardcoding, or input-special-casing a fixture to inflate parity. A fixture
  counts as passing only when the runtime genuinely produces the correct output
  through the same code path any other input would take.
- **No crashing / hanging code committed as "pass".** ACCESS_VIOLATION /
  Cranelift verifier error / stack overflow / infinite loop on a fixture means
  it did **not** pass.
- **Build must compile.** A broken build still blocks merge.
- **At cutover (design doc P5), parity must be ≥ the `v0.0-202606072107` tag,
  measured real** — the redesign exists so the next plateau is not another local
  max of hacks, not to trade the number away.

## MANDATORY RULE: PRIMORDIAL-vs-REGISTRY DOCTRINE

This doctrine **survives the redesign and is central to the new engine too**
(`docs/specs/rts-codegen-new-design.md` §3.2 / §10): the new engine *extends* it
to swallow the hardcoded minority via data-driven dispatch.

The engine (the codegen) is a native motor for a JS/TS language. It may reference
**directly, by name** ONLY the PRIMORDIAL classes — the minimal set that
constitutes the language. Everything else (the "extra environment") is registered
and resolved **dynamically through the Registry** (`global_class_lookup` /
`try_global_class_instance_method` / class metadata like `instanceof_predicate`),
with NO hardcoded mention in codegen, and **NO builtins implemented in the
engine** — only metadata in the Registry.

- **Primordial set** (engine MAY name): `String`, `Object`, `Array`, `Function`,
  `Promise`, `Boolean`, `Number`, `Error` (+ `TypeError`/`RangeError`/
  `ReferenceError`/`SyntaxError`/`URIError`/`EvalError`/`AggregateError`).
- **Everything else = Registry only** (engine MUST NOT name): `Map`, `Set`,
  `WeakMap`/`WeakSet`/`WeakRef`/`FinalizationRegistry`, `RegExp`, `Date`,
  `Symbol`, `URL`, `BigInt`, `Intl.*`, `Proxy`, `Reflect`, `DataView`/
  `ArrayBuffer`, and all backend classes (Console/Fetch/Timers/Performance/Blob/
  TextEncoder/Decoder/EventTarget/Headers/FormData/ReadableStream*/etc.).
- **A direct mention of a non-primordial class in codegen = REGRESSION** to drain.
- **NEVER implement `Symbol` as an engine shortcut.** The Symbol class lives in
  the Registry as extra-environment; the engine must carry ZERO Symbol mentions.
  Language features that historically leaned on well-known symbols (iteration,
  coercion, instanceof) are re-expressed via compile-time desugar to internal
  `__rts_wk_*` names — never a runtime Symbol hook in the engine.
- The mechanism in the runtime layer: declare a class's metadata on the spec
  (`ClassBuilder::instanceof_predicate`, member symbols, `default_args`, flags)
  in rts-primitives/rts-shared/rts-std, and route `recv.method(args)` through
  `try_global_class_instance_method`.

### The dividing line is NATIVE SYNTAX (new-engine clarification — binding)

The practical rule for the new engine: **does the thing have a native literal /
syntactic form?**

- **Native syntax ⇒ PRIMITIVE ⇒ codegen-direct (rts-primitives).** Anything written
  with native syntax is a primitive the engine handles directly (the impl lives in
  `rts-primitives`, but the engine NAMES it and lowers its syntax): string literals
  `""` (`String`), numbers `123` (`Number`), `true`/`false` (`Boolean`), array
  literals `[]` (`Array`), object literals `{}` (`Object`), `function`/arrows
  (`Function`), **regex literals `/re/` (`RegExp` — it HAS native syntax, so it is
  native/primitive, NOT Registry)**, template literals, and `Error`+subclasses
  (primordial). These interact directly with codegen.
- **No native syntax ⇒ rts-shared UTILITY LIB ⇒ Registry, indirect.** The JS
  utility libraries you reach via `new X()`/static calls with NO literal form:
  `Date`, `Map`, `Set`, `WeakMap`/`WeakSet`, `JSON`, `URL`, `Math` (methods),
  `Promise`, `Intl.*`, `Proxy`/`Reflect`, typed arrays, and all backend classes.
  These resolve through the real `Registry` (`crates/rts-codegen-new/src/front/run/
  registry.rs` builds it from `Engine::new()` + the `register`/`register_class_spec`
  fns; `registry_call.rs` is the generic marshal-from-`AbiType` path) — the engine
  NEVER reimplements them as codegen `__rtsadp_*` tables. `Date` is the reference
  migration (done); `Map`/`Set` are the next to migrate.
- **Reclassification vs the bullet above:** `RegExp` moves to the native/primitive
  side (it has `/re/` syntax). The "Registry only" list still holds for the
  no-native-syntax utilities. This refines, not contradicts, "no builtins in the
  engine": a primitive's *implementation* still lives in `rts-primitives`; the
  engine just has a more-direct lowering for its native syntax.

### How the redesign enforces this
In the new engine the doctrine is no longer maintained by *draining* hardcoded
arms one at a time (the old-engine grind). It is the default: every non-primordial
method is a `MethodSpec` metadata entry resolved through ONE generic path
(`crates/rts-codegen-new/src/dispatch.rs`, `resolve_method`), and the JIT symbol
table is **derived from `SPECS`** (`abi_gen.rs`) with a build-time coverage assert
— so a direct mention of a non-primordial class in the new engine is simply not
how dispatch is written. See design doc §10. The old engine
(`rts-codegen-old/`) still carries the hardcoded switchboard (`calls/mod.rs`
~4.6k LOC) and 1113 manual `add_fn!`; those are frozen and deleted at cutover,
not patched further.

### Runtime layer (still valid for both engines)
The primordial classes live in the `rts-primitives` crate (depends only on
`rts-engine`, wasm-safe); non-primordial universal surface in `rts-shared`;
backend in `rts-std`; `rts-runtime` is the thin facade (`pub use` of all four).
This partition is unchanged by the engine redesign — both `rts-codegen-old` and
`rts-codegen-new` read the runtime through the facade.

## Project

RTS is a TypeScript-to-native compiler/runtime using Cranelift as codegen
backend. Goal: compile TS/JS to native binaries with a minimal Rust runtime,
shipped as a standalone toolchain (no external runtime support library).

Runtime is organized around the ABI `SPECS` contract (in `rts-engine::abi`), with
a module-graph pipeline + incremental cache. Two execution paths: JIT via
`cranelift_jit::JITModule` (`rts run`, direct executable memory) and AOT via
`cranelift_object::ObjectModule` (`rts compile`, external linker).

The canonical direction for the engine is `docs/specs/rts-codegen-new-design.md`.

## Architecture

Cargo workspace in `crates/`. `src/` is the `rts` bin facade (re-exports);
`src/main.rs` calls into the codegen + `rts_cli::cli::dispatch`. Real paths live
under `crates/<crate>/src/`.

> **Two codegen crates during the strangler-fig migration.**
> `crates/rts-codegen-old/` is the **frozen** old engine (HIR→MIR→Cranelift dual
> path + AST fallback, the overloaded-`i64` value model, the 4.6k-LOC
> switchboard, 1113 manual `add_fn!`) — still plugged into the bin/cli until
> cutover. `crates/rts-codegen-new/` is the **active redesign** (single
> HIR→Cranelift lowering, `PolyValue` NaN-box value model, shapes + data ICs,
> data-driven dispatch + generated ABI). Canonical design:
> `docs/specs/rts-codegen-new-design.md`.

> **Runtime layer partition:** the old monolith is split into an acyclic graph
> `rts-engine` (heap GC + ABI vocab/SPECS + Registry/builder + collector
> contract) ← `rts-primitives` (PRIMORDIAL classes — see the Primordial doctrine
> above) + `rts-shared` (universal non-primordial: math/num/collections(Map/Set)/
> json/globals…) ← `rts-std` (backend: io/net/tokio/console/promise impl) ←
> `rts-runtime` (thin facade, `pub use` of all four; AOT staticlib). The codegen
> reads everything via the `rts-runtime` facade. This partition is shared by both
> codegen crates and is **not** changed by the engine redesign.

```
crates/
  rts-ast/          — internal AST
  rts-parser/       — SWC parse; arrow/fn expressions → top-level Item::Function
  rts-diagnostics/  — structured errors
  rts-engine/       — heap GC, ABI contract (abi:: SPECS, types, symbols,
                      signatures, Intrinsic, global_class, handles), Registry
  rts-hir/          — typed HIR (I8..I128/F32/F64/Bool/Str/Handle/Array/Function/
                      Class/Object/Any/Unknown)
  rts-mir/          — SSA MIR — used ONLY by rts-codegen-old (frozen); the
                      redesign deletes the MIR tier
  rts-codegen-old/  — FROZEN old engine (dual HIR→MIR / AST path, switchboard)
  rts-codegen-new/  — ACTIVE redesign; see module map below + design doc
    src/value.rs    — PolyValue (64-bit NaN-box; the one in-value tagged repr)
    src/repr.rs     — Repr lattice (Int32/Float64/Bool/Ref/Tagged) + join — soundness core
    src/shape.rs    — hidden classes (Shape / transition tree / slot layout)
    src/ic.rs       — AOT-safe data inline caches (PropIcCell, uninit→mono→poly→mega)
    src/dispatch.rs — data-driven method resolution (Target / resolve_method via SPECS)
    src/abi_gen.rs  — JIT symbol table DERIVED from SPECS (kills manual add_fn!)
    src/lower/      — single HIR → Cranelift lowering path (no MIR)
    src/pipeline.rs — shared JIT (run_jit) + AOT (compile_aot)
  rts-primitives/   — PRIMORDIAL classes (String/Object/Array/Function/Promise/
                      Boolean/Number/Error+subclasses)
  rts-shared/       — non-primordial universal (math/num/collections/json/globals)
  rts-std/          — backend (io/net/tokio/console/promise impl)
  rts-runtime/      — thin facade ("rts" + "rts:<ns>" submodules); AOT staticlib
  rts-node/         — Node.js builtin shims (fs, os, path, process, crypto, util)
  rts-napi/         — N-API: Node.js native addons (.node) via libloading + the
                      engine HandleTable (ArrayBuffer/BigInt/External). 159 N-API
                      fns. Loader `__RTS_FN_NS_NAPI_LOAD_ADDON`; re-exported by
                      rts-runtime as `napi`. Spec: docs/specs/napi-implementation.md
  rts-linker/       — native link (system linker + object backend fallback)
  rts-cli/          — CLI (run, compile, apis, init, repl, eval, ir)

src/                — bin facade (re-exports), runtime_objects.rs, main.rs
```

### New-engine pipeline (the redesign, single path)

```
TS → SWC → AST → HIR → lower/ (HIR → Cranelift IR, one path) → Cranelift egraph → JIT/AOT
```

There is **no MIR tier and no dual AST/MIR codegen** in the new engine. The
Cranelift egraph (`use_egraphs=true`) is the **sole** optimizer (const-fold, CSE,
DCE, FMA, strength reduction, intraprocedural inlining). The front-end only does
what Cranelift genuinely cannot (JS semantics): `ToNumber`/`ToString`/`ToBoolean`
coercions, the polymorphic `+`, box/unbox insertion (as pure IR the egraph
folds), shape/IC site emission, narrow-int wrap semantics, exception edges. Both
AOT/JIT share `compile_program`/`pipeline.rs`; `FnCtx.module` is
`&mut dyn Module`. See design doc §9 (Pilar 5) and the per-pillar sections.

> The frozen `rts-codegen-old/` still runs the old hybrid `HIR→MIR→Cranelift`
> (default) with silent AST fallback, gated by `RTS_USE_MIR`. That machinery is
> NOT carried into the new engine.

## ABI (`rts-engine::abi`) — single contract

All surface between codegen and runtime goes through `rts-engine::abi`. No
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
- `guards.rs` — `guard_for(expected, caller)`. NOTE: in the old engine this is
  **dead code** (zero production call sites; coercion is ad-hoc `TPL_COERCE_AUTO`
  scattered across files). The redesign makes coercion ONE real authority (design
  doc §7, Pilar 3) — `guard_for` is either promoted to the real path or replaced
  by an equivalent in `rts-codegen-new`; the scattered ad-hoc coercion does not
  survive.

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
`declare_value_needs_stack_map(val)`; the JIT emitter registers return-PCs in
`stack_map_registry`. Every `GC_TICK_INTERVAL = 256` allocs, `finish_cycle()`
runs `mark_stack_roots()` + `sweep_all_shards()`. `mark_stack_roots()` on Windows
uses `GetCurrentThreadStackLimits` (Win32) — **not** `gs:[0x10]` (TIB.StackBase
sometimes < RSP → scanner marks nothing → live handles collected; bug PR #400).

**Required change for the new engine (design doc §5.4, Pilar 1):** the
conservative stack scanner must learn to recognize **NaN-boxed `PolyValue` handle
words**. A stack word `w` is a potential root iff `(w & BOX_BASE) == BOX_BASE`
AND `tag(w) ∈ {STR, OBJECT, FUNCTION}`; the root is the 48-bit `slot(w)`. Inline
ints, inline floats, and singletons are NOT roots. This is *more* precise than
today (float words that merely look like handles stop being false positives). GC
safety holds because the payload is a HandleTable **slot index**, not a raw
pointer.

### State
No central state system — each namespace owns its own via `Arc<Mutex<T>>`
(`OnceLock` init) or `thread_local!` caches.

## Language capabilities (target semantics the engine must cover)

This is the JS/TS surface the engine must support — the same intent under both
engines. The OLD engine implemented these on the overloaded-`i64` value model
(per-feature notes in parentheses describe its mechanism, frozen). The NEW engine
must reproduce the same **semantics** through the `PolyValue`/shapes/IC model;
see the design doc's coverage plan and `.claude/rules/03-features.md`.

- Object/array literals (old: `collections.map_*`/`vec_*`; new: shapes + slots).
- Classes: constructor, method, this, extends, super(args), super.method,
  static, getters/setters (old: `__rts_class` string tag + O(N) `gc.string_eq`
  vtable; new: shape-id + data IC dispatch).
- Rust-style operator overload: `a + b` → `a.add(b)` at compile time when the
  class defines the method.
- `for...of` over arrays; try/catch/finally phase 1 (thread-local error slot, no
  real unwind — #128 phase 2). String equality.
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

### Silent parallelism (Level-1) — OLD ENGINE ONLY

3 codegen passes in `rts-codegen-old` rewrite common TS to `parallel.*`
automatically (`array_methods_pass`, `reduce_pass`, `purity_pass`). This
machinery is **frozen in the old engine and is NOT carried into the new engine
unless re-justified** against the redesign. Spec:
`docs/specs/silent-parallelism.md`.

## Codegen optimizations (new engine)

- **The Cranelift egraph is the sole optimizer** (`use_egraphs=true`): const-fold,
  CSE, DCE, FMA, strength reduction, intraprocedural inlining. There is **no
  second optimizer tier** — the old MIR passes (fold/fma/cse/dce/narrow/inline)
  re-did exactly what the egraph already does and are deleted with the MIR tier.
- **box/unbox as pure Cranelift IR** (`bitcast`/`band`/`bor`/`icmp`/`select`): a
  redundant `box(unbox(x))` is folded by the egraph, so the `PolyValue` cost
  vanishes exactly where the representation was already monomorphic (design doc
  §9.3). This is why box/unbox must NOT be extern calls.
- **Intrinsics inline** (`abi::Intrinsic`): sqrt, abs_f64, min/max_f64, abs_i64,
  min/max_i64, random_f64 → direct Cranelift IR. Preserved (intrinsic spec tag).
- **TCO**: user fns in `CallConv::Tail`; `return f(x)` → `return_call`
  (needs `preserve_frame_pointers=true` on x86-64).
- **First-class fn pointers**, **imm forms**, **MemFlags::trusted** on
  global/RNG loads, **f64 mod via libc fmod**, **constants as properties** — the
  front-end emits these; the egraph cleans up.
- **Shapes + data ICs** (design doc §8): property access is shape-id compare +
  fixed-offset load (not hash lookup); method dispatch is shape-keyed, not O(N)
  string compare. Narrow-int (i8/u8/i16/u16) wrap semantics are a front-end
  responsibility on the IR.

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
cargo build --release -p rts-runtime                          # AOT runtime archive (see note)
cargo build --release                                         # release build
$env:RUST_BACKTRACE="full"; target/release/rts.exe run file.ts            # JIT
$env:RUST_BACKTRACE="full"; target/release/rts.exe compile -p file.ts out # AOT
$env:RUST_BACKTRACE="full"; target/release/rts.exe test tests/foo.test.ts # TS suite
target/release/rts.exe apis                                   # list APIs
```

**AOT archive is a two-step build.** `rts-runtime` is `crate-type =
["rlib","staticlib"]`; Cargo bundles all deps + every `__RTS_*` symbol into the
staticlib that `build.rs` embeds for AOT linking. Cargo only emits that staticlib
when `rts-runtime` is a *direct* target, so build it first (`cargo build
-p rts-runtime`) before building `rts`. Skipping it does NOT break the build or
JIT — `build.rs` embeds a placeholder and `rts run` works; only `rts compile`
(AOT) errors with a "rebuild the runtime archive" message until the staticlib
exists. The two-step replaced a fragile build.rs that hand-picked dependency
rlibs and could not disambiguate duplicate variants on CI (serde_core/time).

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

## Artifact layout

```
<project>/
  src/main.ts  package.json  tsconfig.json
  node_modules/.rts/objs/{runtime/,compile/}   modules/ (.ometa cache)
  release/<project_name>   — only on rts compile
```

## Status

On the OLD engine the TS suite hit 1719/1719 and cross-runtime parity hit 100%
(372/372) at tag `v0.0-202606072107` — a local max of a hardcoded approach (see
"HONEST CURRENT STATUS" above). The fixture set then grew to **612** and parity
is now **70.7%**; the `rts-codegen-new` redesign is being built strangler-fig to
clear the real wall (the unsound value model), not to chase the next plateau of
hacks. Heavy items still open (some now in-scope for redesign phases): #195
mutable closures, #207 real async event loop, #216/#222 Symbol, #217 weak
WeakMap/Set + FinalizationRegistry, #218 Proxy, #219 BigInt, #223 dynamic import,
#301 var hoisting, #304 toString/valueOf coercion.

## Docs

`docs/specs/` holds feature specs, design decisions, technical notes — index at
`docs/specs/INDEX.md`. **Canonical engine direction:
`docs/specs/rts-codegen-new-design.md`** (the redesign plan; read before engine
work). Detailed rules in `.claude/rules/` (00-meta → 05-codegen-notes), each
binding.
