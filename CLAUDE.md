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
- **MANDATORY RULE: ITERATION SPEED** — no `cargo build --release` and no full TS
  suite while developing; `cargo check -p <crate>` / `cargo run -- run` and only
  the tests of the area touched. Full gate at commit time only.
- **MANDATORY RULE: REGRESS WHEN NECESSARY (EXPLICITLY)**
- **MANDATORY RULE: PRIMORDIAL-vs-REGISTRY DOCTRINE** (engine names only
  primordials; everything else via the Registry/SPECS; NO builtins in the engine)
- **MANDATORY RULE: FOLLOW THE REDESIGN DESIGN DOC** — the canonical plan is
  `docs/specs/rts-codegen-new-design.md`; pick work from its migration phases
  (P0→P5), not from a fixture-grind roadmap (see "HONEST CURRENT STATUS" below
  and `.claude/rules/00-meta.md`)
- **MANDATORY RULE: read_before_commit.sh GATE + FILE LAYOUT** — run
  `bash scripts/read_before_commit.sh` and read its whole output before every commit
  touching the engine; file-size ceilings codegen ≤1000 / engine ≤700 / rest ≤500 (split into a
  folder/subfolder); the engine names ONLY primordials —
  `rts-shared`/`rts-std` are **NOT** native/primitive and a direct mention is a
  regression
- **MANDATORY RULE: READ THE EGUI/WEB ENGINE PLAN BEFORE TOUCHING IT** —
  before changing ANYTHING in the egui / HTML / web UI engine (`crates/rts-egui/`,
  the `.ts` UI lib over it, or any web/HTML-engine code) you MUST first read the
  canonical plan in full: `docs/specs/html-engine/rts-html-roadmap.md` (roadmap
  F0–F5) + `docs/specs/html-engine/rts-html-north-star.md` (frozen vision) +
  `docs/specs/html-engine/arquitetura.md` + `docs/specs/egui-ui-crate-design.md`.
  Then pick work from the roadmap's phases in order. **STRICTLY MANDATORY — no
  exceptions.** See the dedicated section below.

Keep this list in sync with the sections below. The honesty + build floor
(parity number stays real, no crash/hang committed as "pass", build must
compile) never lifts under any mode.

## MANDATORY RULE: READ THE EGUI/WEB ENGINE PLAN BEFORE TOUCHING IT

**STRICTLY MANDATORY — no exceptions.** The egui / HTML / web UI engine is built
to a frozen plan. Before you change ANYTHING in it — `crates/rts-egui/`, the
high-level `.ts` UI library layered over it, or any web/HTML-engine code — you
**MUST FIRST read the canonical plan in full**:

- `docs/specs/html-engine/rts-html-roadmap.md` — operational roadmap **F0–F5**
  (the phase order you pick work from)
- `docs/specs/html-engine/rts-html-north-star.md` — the **frozen** north-star
  vision (do not redesign it; it is congelado)
- `docs/specs/html-engine/arquitetura.md` — architecture decision (evolve the
  light engine in-place; egui-layout by default, absolute paint only surgically
  in F4)
- `docs/specs/egui-ui-crate-design.md` — the `rts-egui` crate design

### How to apply

1. Reading those docs end-to-end is a **precondition** to any egui/web edit — not
   optional, do not skip, do not assume content, do not start coding from memory.
2. Pick work from the roadmap's phases **in order** (F0→F5), highest-leverage
   first; do not jump ahead of a phase's prerequisites.
3. The north-star is **frozen**. If you believe the plan must change, propose the
   change to the doc FIRST and get confirmation — never silently diverge from it
   in code.
4. If the plan is stale (code no longer matches), update the plan doc in the same
   PR — never leave a lying plan in effect.
5. If a user instruction conflicts with the plan, ask for confirmation before
   violating it. Do not decide alone.

This exists because the web/egui engine must follow ONE agreed plan across
contributors — an agent improvising its own UI architecture is the failure mode
this rule prevents.

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

Minimum suite **before merge** — not during development, see the ITERATION SPEED
rule below:

```bash
cargo build --release             # clean build
cargo test --release --lib        # unit + integration
target/release/rts.exe test       # TS suite (if PR touches runtime/codegen/GC)
```

### Practical rules

- **Run the full suite before merge.** You must know exactly which tests pass and
  which regress. "It broke and I don't know why" is never acceptable. **While
  iterating, run only the tests for the area you touched** — see the ITERATION
  SPEED rule.
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

## MANDATORY RULE: ITERATION SPEED — no release build, no full suite, while working

Rust compiles slowly and this repo's release profile is a SHIPPING profile
(`lto = "thin"`, `codegen-units = 1`, `strip = "symbols"`, `opt-level = "z"`) —
a full `cargo build --release` is minutes. Running it, and running the whole TS
suite, after every edit is the single biggest drag on making progress here. Both
are **merge-time** activities, not development-time activities.

### While developing

- **Do NOT `cargo build --release`.** Use `cargo check -p <crate>` for "does it
  compile", and `cargo run -- run file.ts` when you need to actually execute.
  `cargo run` builds debug (~3× faster to compile, ~10× slower to run — fine for
  correctness, never for benchmarks).
- **Do NOT run the full TS suite.** Run only the files covering the area you
  touched: `target/release/rts.exe test tests/<relevant>.test.ts`, or
  `cargo test -p <crate> --lib <filter>` for Rust tests. A full
  `rts.exe test` run is ~740 files and minutes of wall clock.
- **Build the narrowest crate that proves your change.** `cargo check -p rts-macro`
  is seconds; `cargo build --release` (workspace) is minutes. Touching the macro
  crate does not require building the CLI.
- **Never benchmark a debug build.** Numbers from `cargo run` are meaningless for
  performance claims — if you are measuring, say which profile produced it.

### Before commit — then, and only then

Run the full gate: `cargo build --release`, the unit suite, the TS suite if the
change touches runtime/codegen/GC, and `bash scripts/read_before_commit.sh`. The
honesty floor is unchanged: you must know exactly what passes and what regresses
before merging.

### Why this rule exists

The failure mode it prevents is real and was observed: a session spent more wall
clock on repeated 5-minute release builds and 15-minute suite runs than on the
actual engineering, which both slows delivery and pushes toward guessing instead
of checking (because checking is expensive). Making the cheap check cheap is what
keeps verification honest.

## MANDATORY RULE: read_before_commit.sh GATE + FILE LAYOUT

Before **every** commit that touches `crates/rts-codegen-new/`, run the gate and
read its full output:

```bash
bash scripts/read_before_commit.sh            # full gate (includes cargo build)
bash scripts/read_before_commit.sh --no-build # fast static-only pass while iterating
```

The gate (in `scripts/`) encodes the binding rules below so a commit cannot
silently violate them. It separates **HARD** failures (exit non-zero — never
commit) from **REVIEW** lists (read every entry; pre-existing debt must shrink,
never grow).

### Rule A — the engine is native/primitive-only; rts-shared/rts-std are NOT

The engine (`rts-codegen-new`) interacts directly ONLY with the native/primitive
surface: the PRIMORDIAL classes via `rts-primitives`, reached through the
`rts-runtime` facade. **`rts-shared` and `rts-std` are NOT native/primitive** —
they are the non-primordial utility/backend libraries. A direct dependency on,
`use` of, or hardcoded mention of `rts-shared`/`rts-std` (or any non-primordial
class: `Map`/`Set`/`Date`/`URL`/`Proxy`/…) **in the engine is a
REGRESSION**. Everything non-primordial resolves through the **Registry**
(`registry.rs` / `registry_call.rs`), never a hardcoded per-class path. This is
the same PRIMORDIAL-vs-REGISTRY doctrine below, restated as a commit gate. A
dedicated `*class.rs` (e.g. `dateclass.rs`) is a **draining target** — never add
a new non-primordial path to one.

> The gate HARD-fails on a forbidden dep/`use`; it REVIEW-lists every
> non-primordial class name found in codegen (test/fixture files are split out as
> expected). The current draining targets are `front/run/dateclass.rs` and
> `front/run/globalclass.rs`.

### Rule B — file-size ceilings (tiered)

Per-layer line ceilings: **codegen** (`crates/rts-codegen-new/src/`) **≤ 1000**,
**engine** (`crates/rts-engine/src/`) **≤ 700**, **everything else ≤ 500**. When a
file would grow past its ceiling, split it into a **folder/subfolder** of cohesive
submodules
(the `mod.rs` + sibling-files pattern already used across the engine). The gate
REVIEW-lists every offender; the list must only shrink. New code lands in a
small, focused module, not appended to an already-oversized file.

### Rule C — resolve blocking limitations first (focus shift is allowed)

When the main feature you picked is blocked by a missing engine capability
(e.g. mutable env-record captures before async, a Registry hook before a class
migration), it is correct to **shift focus and implement the blocker first**,
then return to the main feature. State the shift explicitly in the commit/PR.
Keep the change modest and incremental — small focused modules, gate green.

## HONEST CURRENT STATUS — new engine is LIVE (cutover already happened)

Read this so you do not act on stale numbers or a stale architecture. **The
strangler-fig migration is over: the new engine is the only engine.**

**The old engine is DELETED.** `crates/rts-codegen-old/` and the `rts-mir/` crate
no longer exist (not in the workspace, not on disk). `rts run` / `rts compile` /
`rts test` / `rts eval` all execute through the **new engine**
(`crates/rts-codegen-new/`, value model in `crates/rts-runtime/src/adapters/`). The
`run-new` command and `scripts/measure_new.sh` still exist as the campaign harness from
the migration; `run`/`run-new` are now the same engine. The old overloaded-`i64`
value model, the 4.6k-LOC switchboard, and the 1113 manual `add_fn!` are gone.

**The truth about the 100%.** On 2026-06-06/07 RTS reached **100% cross-runtime
parity** (372/372; TS suite 1719/1719), tag `v0.0-202606072107`, commit
`27e16378` — on the OLD engine. That was the *local maximum of a hardcoded
approach* on an unsound value model, not validation of its design. It is the bar
the new engine must re-clear, not a number to quote as current.

**The current number (honest, measured).** The new engine's cross-runtime parity
is **~76.5%** as of 2026-07-05 (auto-updated badge; climbed from 31.5% on
2026-06-23 while filling JS/TS coverage back in on the sound value model).
**Do not quote 70.7%, 94.3%, or 100% — those are the old engine.** Always
re-measure with `cross_runtime_report.json`; do not cite a remembered figure.

**The thesis (unchanged).** *Prove-monomorphic-and-unbox where the type system
can (keep the winning numeric path); fall to ONE honest in-value tagged
representation (`PolyValue`, a 64-bit NaN-box) + hidden-class shapes + AOT-safe
data inline-caches where it can't.* Canonical plan still
`docs/specs/rts-codegen-new-design.md` — **read it before any engine work**
(note: its file-path map is partly stale post-cutover; the value model now lives
in `rts-runtime/src/adapters/`, not `rts-codegen-new/src/*.rs`; the `repr`/
`shape`/`state` lowering-time slices are in `rts-codegen-new/src/` proper).

### How work is picked now
Drive coverage up by attacking the largest failure cluster from the cross-runtime
/ `scripts/measure_new.sh` histogram (measure → attack biggest cluster → re-measure),
highest-leverage first. The deferred epics (#195 mutable closures — partly landed
via mutable-local-capture, #207 async event loop, #216/#222 Symbol, #218 Proxy —
get/set/delete traps landed, #219 BigInt, #223 dynamic import) are in scope where
the cluster calls for them.

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
  `ReferenceError`/`SyntaxError`/`URIError`/`EvalError`/`AggregateError`),
  `Symbol`, `BigInt`, `Proxy`, `Reflect`. **`Symbol` is PRIMORDIAL** — a
  fundamental language built-in with a unique primitive type and
  `typeof x === "symbol"`. The engine MAY name it (route `Symbol(desc)` /
  `Symbol.for` / `.description` directly); its impl + spec metadata live in
  `rts-primitives` (`src/symbol`) — PRIMORDIAL impls belong in `rts-primitives`,
  not `rts-shared` (owner directive 2026-07-24; the earlier `rts-shared/globals/
  symbol` placement was a layering mistake, now corrected). Reflect/Proxy impls
  are mid-relocation to `rts-primitives` too (blocked on moving the shared
  property-descriptor helpers `is_non_writable/enumerable/configurable` first).
  (Reclassified 2026-06-26 — was Registry-only.) **Reclassified 2026-07-03
  (owner decision) — also PRIMORDIAL**: `BigInt` (new primitive type,
  `typeof "bigint"`, native `123n` syntax the generic operators tag-dispatch),
  `Proxy` (traps the engine's own property paths consult — obj_get/idx_get/
  fn_invoke already route `proxy_parts`), `Reflect` (1:1 mirrors of the
  engine's internal ops [[Get]]/[[Set]]/[[OwnKeys]]/[[Construct]]),
  `ArrayBuffer`/`SharedArrayBuffer`/`DataView`/TypedArrays (`Int8Array`…
  `Float64Array`, `BigInt64Array`/`BigUint64Array`, `Uint8ClampedArray` — the
  raw MEMORY model; element indexing is engine-lowered), `Atomics`
  (memory-model operations), `WeakRef`/`FinalizationRegistry` (GC-coupled,
  #217), `Math` (formalization — its core ops are already IR intrinsics).
  Iterator/generator PROTOCOL is primordial (the engine owns the
  interactions); the API bodies stay in classes so the main API surface can
  evolve without touching the engine. The rule of thumb: **primordial =
  defines or intercepts what a VALUE is (tag, trap, internal op, memory, GC);
  Registry = operates over existing values.** Impls still live in the runtime
  layer.
- **Everything else = Registry only** (engine MUST NOT name): `Map`, `Set`,
  `WeakMap`/`WeakSet`, `Date`, `URL`, `Intl.*`, and all backend classes
  (Console/Fetch/Timers/Performance/Blob/TextEncoder/Decoder/EventTarget/
  Headers/FormData/ReadableStream*/etc.).
- **A direct mention of a non-primordial class in codegen = REGRESSION** to drain.
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
- **No native syntax ⇒ rts-shared UTILITY LIB, indirect.** The JS utility
  libraries you reach via `new X()`/static calls with NO literal form: `Date`,
  `Map`, `Set`, `WeakMap`/`WeakSet`, `JSON`, `URL`, `Math` (methods), `Promise`,
  `Intl.*`, `Proxy`/`Reflect`, typed arrays, and all backend classes. The engine
  NEVER names them / reimplements them as codegen `__rtsadp_*` tables. Two
  sub-paths, both engine-name-free:
  - **Registry (data dispatch):** `Date` is the reference (done) — ctor/statics/
    methods resolve via `MethodSpec`/`Sig.default_args`/flags through
    `is_pure_registry_class` + `registryclass.rs`. Target of `URL`/typed-arrays/
    `TextEncoder`/backend (real Rust impl, no native syntax).
  - **`.ts` stdlib (`rts-shared/src/stdlib/*.ts`):** **COLLECTIONS** (`Map`/`Set`
    done; `WeakMap`/`WeakSet` done, strong-ref interim) are ambient `.ts` classes,
    NOT Registry — they need arbitrary key/value types the i64 Rust backend can't
    hold without the PolyValue containers (P2, deferred); the `.ts` (arrays of
    PolyValue) covers it. `WeakMap`/`WeakSet` become REAL weak when the GC weak
    phase lands (design doc §5.7, deferred until ~90% cross-runtime); until then
    strong-ref, like the Rust v0 stubs.
- **Reclassification vs the bullet above:** `RegExp` moves to the native/primitive
  side (it has `/re/` syntax). The "Registry only" list still holds for the
  no-native-syntax utilities. This refines, not contradicts, "no builtins in the
  engine": a primitive's *implementation* still lives in `rts-primitives`; the
  engine just has a more-direct lowering for its native syntax.

### NATIVE EMITTERS — a member may carry its own Cranelift emission

**Owner decision (2026-07-20).** A Registry member may declare a `NativeEmit`
(`rts_engine::member`): a non-capturing closure that emits Cranelift IR at the
call site instead of `call <symbol>`.

```rust
// in rts-shared / rts-primitives, next to the spec that owns the operation
.member(native(
    func("sqrt", "__RTS_FN_NS_MATH_SQRT", Sig::new(vec![F64], F64), …),
    |b, args| { let [x] = args else { return None }; Some(b.ins().sqrt(*x)) },
))
```

Why this supersedes the `Intrinsic` enum: a closed enum forces the ENGINE to
carry a variant plus a `match` arm per operation — engine-side knowledge of a
non-primordial, which is exactly what the doctrine forbids. With `NativeEmit` the
emission lives WITH THE SPEC and the engine has one generic call site.
`Intrinsic` is legacy in drainage; do not add variants to it.

Binding rules:

- **Cranelift is allowed below the engine.** `rts-engine`/`rts-shared`/
  `rts-primitives` may depend on `cranelift-codegen`/`cranelift-frontend`.
  Cranelift is pure Rust with no C dependencies, so the universal layer still
  builds for every target including wasm/browser. The older "no compiler
  backend below the engine" reading of the layering does **not** apply to
  Cranelift.
- **Never reachable from userland (security).** An emitter is spec/engine
  surface only: it must not appear in `rts.d.ts` and must not become a callable
  TS namespace. User code reaches the member by its ordinary JS name.
- **Fallback, never failure.** Returning `None` from an emitter makes the engine
  emit the ordinary call, so `symbol`/`fn_ptr` stay registered and reflection /
  FFI / unproven receivers keep working. An emitter can only make a site faster.
- **Operands arrive coerced to the member's declared `Sig`**, through the same
  `coerce` the call path uses — an emitter must not re-implement coercion.
- **The heap is out of scope.** Scalar computation (arithmetic, bits, compare,
  convert) becomes IR. Anything touching strings/objects/`Vec`/allocation needs
  the HandleTable and the allocator, which do not exist in IR; those members keep
  their call. "Everything in Cranelift" has that ceiling, and it comes from the
  memory model, not from Cranelift.

### How the engine enforces this
The doctrine is the default in the engine: every non-primordial method is a
`MethodSpec` metadata entry resolved through ONE generic path
(`crates/rts-codegen-new/src/front/run/registry_call.rs` +
`crates/rts-runtime/src/adapters/dispatch.rs`, `resolve_method`), and the JIT symbol
table is **derived from `SPECS`** (the `adapter_symbols/` module, harvested from
Registry fn-ptrs with a drift/coverage guard) — so a direct mention of a
non-primordial class in the engine is simply not how dispatch is written. See
design doc §10. (The old engine's hardcoded switchboard and 1113 manual
`add_fn!` are deleted, not patched further.)

### Runtime layer
The primordial classes live in the `rts-primitives` crate (depends only on
`rts-engine`, wasm-safe); non-primordial universal surface in `rts-shared`;
backend in `rts-std`; `rts-runtime` is the thin facade (`pub use` of all four).
The engine reads the runtime through this facade.

### ANTI-HARDCODE — how to add a feature WITHOUT naming a non-primordial (binding)

The #1 way a contributor (human or agent) regresses the doctrine is by hardcoding
a non-primordial NAME in the new engine's front-end "just this once". **Do not.**
A direct `"console"`/`"Date"`/`"Map"`/… literal in `crates/rts-codegen-new/`
control flow is a REGRESSION — **even inside an allow-list / a `match name {}` /
a `const NAMES: &[&str]`** (reviewer @drysius rejected exactly this framing; an
allow-list of a non-primordial name is still naming it). The ONLY names the front
may write are the PRIMORDIAL set (String/Object/Array/Function/Promise/Boolean/
Number/Error+subclasses).

When you reach for a non-primordial name, STOP and resolve it by **shape/data**:

1. **Is there a structural pattern instead of a name?** Match the SHAPE, not the
   identity. Real example — making `const console = new Console()` reach inside
   user functions: the fix does NOT special-case `"console"`; it matches the
   pattern `const X = new Y()` referenced-from-a-function
   (`funcval::singleton_instance_globals`), promotes it to a gcell, and carries
   `name → class` in a `gcell_classes` map the lowering reads. Generic for ANY
   singleton, zero name in the front. (Class dispatch did the same: `Date` went
   from `if class == "Date"` to `is_pure_registry_class` + `registryclass.rs`.)
2. **Is it a method/static/ctor?** It's a `MethodSpec`/Registry entry resolved by
   the ONE generic path (`dispatch.rs::resolve_method`, `registry_call.rs`). Add
   the metadata on the spec (`rts-primitives`/`rts-shared`/`rts-std`), not an arm.
3. **Is it a whole global object/class (console, Map, JSON, Date)?** Write it as a
   `.ts` PRELUDE and `e.include` it; the irreducible bits become PRIVATE
   `engine.*` bridges in `engineobj.rs`. **Where the `.ts` lives:** PRIMORDIAL →
   `rts-primitives/src/*.ts` (error/object/boolean/number/string); NON-PRIMORDIAL
   → `rts-shared/src/stdlib/*.ts` (console/json/map_set). Putting a non-primordial
   `.ts` in `rts-primitives` is wrong (console is a backend class, not primordial).
4. **No shape/data exists yet?** That's the signal a SPEC is missing metadata —
   add the flag/field there (`flags`/`default_args`/`ts_signature` on the Member),
   then the generic path picks it up. Never patch the front to compensate.

The `scripts/read_before_commit.sh` gate flags a non-primordial name in
`crates/rts-codegen-new/` for review — if it fires on your change, you took the
hardcode path; go back to the list above.

## Project

RTS is a TypeScript-to-native compiler/runtime using Cranelift as codegen
backend. Goal: compile TS/JS to native binaries with a minimal Rust runtime,
shipped as a standalone toolchain (no external runtime support library).

Runtime is organized around the ABI `SPECS` contract (in `rts-engine::abi`). Two
execution paths share the engine's lowering: JIT via `cranelift_jit::JITModule`
(`rts run`, direct executable memory) and AOT via `cranelift_object::ObjectModule`
(`rts compile`, emits `.o` + native link).

The canonical direction for the engine is `docs/specs/rts-codegen-new-design.md`.

## Architecture

Cargo workspace in `crates/`. `src/` is the `rts` bin facade (re-exports);
`src/main.rs` calls into the codegen + `rts_cli::cli::dispatch`. Real paths live
under `crates/<crate>/src/`.

> **One codegen engine (post-cutover).** The old engine (`rts-codegen-old/`) and
> the `rts-mir/` crate are **DELETED**. `crates/rts-codegen-new/` is the live
> engine (single HIR→Cranelift lowering, no MIR tier). The AOT-linked runtime
> trampolines (`PolyValue` NaN-box + `__rtsadp_*`) live in
> `crates/rts-runtime/src/adapters/` — folded in from the former standalone
> `rts-adapters` crate (dissolved: `rts-runtime` was already the direct
> dependency both the crate and the `rts` bin needed, so there was no reason for
> a separate crate); the lowering-time-only slices (Repr lattice, shapes,
> codegen-state reset) live in `crates/rts-codegen-new/`. `adapters::dispatch`
> (Registry method metadata a runtime trampoline also reads) is in the same
> crate now, so the old cross-crate dependency-direction constraint is gone.
> Canonical design: `docs/specs/rts-codegen-new-design.md` (its file-path map
> predates both extractions — trust the tree on disk over the doc's paths).

> **Runtime layer partition:** acyclic graph `rts-engine` (heap GC + ABI
> vocab/SPECS + Registry/builder + collector contract) ← `rts-primitives`
> (PRIMORDIAL classes — see the Primordial doctrine above) + `rts-shared`
> (universal non-primordial: math/num/collections(Map/Set)/json/globals…) ←
> `rts-std` (backend: io/net/tokio/console/promise impl) ← `rts-runtime` (thin
> facade, `pub use` of all four, plus `adapters/` — the value model; AOT
> staticlib). The engine reads everything via the `rts-runtime` facade.

```
crates/
  rts-ast/          — internal AST
  rts-parser/       — SWC parse; arrow/fn expressions → top-level Item::Function
  rts-diagnostics/  — structured errors
  rts-abi/          — THE ABI CONTRACT, standing alone and dependency-free:
                      AbiType, SymbolDesc, NamespaceMember, the __RTS_* symbol
                      convention, signature lowering. Sits at the BOTTOM of the
                      graph so the codegen and the proc-macro can depend on the
                      contract without depending on an implementation of it.
                      Re-exported as `rts_engine::abi` for compatibility.
  rts-macro/        — the `#[rtse::*]` authoring macros (lib name `rtse`). The
                      SINGLE SOURCE OF TRUTH for symbols: `#[rtse::abi]` takes a
                      plain Rust fn and owns every ABI concern (extern "C",
                      no_mangle, the symbol name) plus emits the `SymbolDesc`
                      const, derived from the Rust signature so drift is
                      unrepresentable. See docs/specs/rts-macro-single-source.md
  rts-engine/       — heap GC, Registry/builder, collector contract. The ABI
                      vocabulary now lives in rts-abi and is re-exported here
  rts-hir/          — typed HIR (I8..I128/F32/F64/Bool/Str/Handle/Array/Function/
                      Class/Object/Any/Unknown)
  rts-codegen-new/  — THE engine (single HIR → Cranelift lowering, no MIR). Map:
    src/repr.rs         — Repr lattice (Int32/Float64/Bool/Ref/Tagged) + join
    src/shape.rs        — hidden classes (compile-time shape interning)
    src/state.rs        — codegen state (reset between runs)
    src/value/         — value-model emission + ABI signatures + marshalling
    src/front/hir_lower — AST/HIR → lowering front
    src/front/run/     — the lowering itself (expr/stmt/call/class/registry/…),
                         module_jit.rs (JIT) + module_aot.rs (AOT object emission)
    src/adapter_symbols/ — JIT symbol table harvested from Registry fn-ptrs
                         (drift/coverage guard); replaces manual add_fn!
  rts-primitives/   — PRIMORDIAL classes (String/Object/Array/Function/Promise/
                      Boolean/Number/Error+subclasses)
  rts-shared/       — non-primordial universal (math/num/collections/json/globals)
  rts-std/          — backend (io/net/tokio/console/promise impl)
  rts-runtime/      — thin facade ("rts" + "rts:<ns>" submodules) + AOT staticlib
                      build.rs embeds. src/adapters/ (formerly the standalone
                      rts-adapters crate) = value/ (PolyValue 64-bit NaN-box +
                      every `__rtsadp_*` trampoline) + dispatch.rs (data-driven
                      resolve_method, read by both codegen and a runtime
                      trampoline — no cross-crate cycle since the move)
  rts-node/         — Node.js builtin shims (fs, os, path, process, crypto, util)
  rts-napi/         — N-API: Node.js native addons (.node) via libloading + the
                      engine HandleTable (ArrayBuffer/BigInt/External). 159 N-API
                      fns. Loader `__RTS_FN_NS_NAPI_LOAD_ADDON`; re-exported by
                      rts-runtime as `napi`. Spec: docs/specs/napi-implementation.md
  rts-egui/         — egui-based GUI / web-UI engine. Follow the FROZEN plan
                      (docs/specs/html-engine/ + egui-ui-crate-design.md) — see the
                      MANDATORY egui/web-plan rule at the top of this file
  rts-linker/       — native link (system linker + object backend fallback);
                      per-target runtime archives (cross-compile prep, #1724)
  rts-cli/          — CLI (run, run-new, compile, apis, init, repl, eval, ir)

src/                — bin facade (re-exports), runtime_objects.rs, main.rs
```

### Engine pipeline (single path, no MIR)

```
TS → SWC → AST → HIR → front/run (HIR → Cranelift IR, one path) → Cranelift egraph → JIT/AOT
```

There is **no MIR tier and no dual AST/MIR codegen**. The Cranelift egraph
(`use_egraphs=true`) is the **sole** optimizer (const-fold, CSE, DCE, FMA,
strength reduction, intraprocedural inlining). The front-end only does what
Cranelift genuinely cannot (JS semantics): `ToNumber`/`ToString`/`ToBoolean`
coercions, the polymorphic `+`, box/unbox insertion (as pure IR the egraph
folds), shape/IC site emission, narrow-int wrap semantics, exception edges. AOT
(`module_aot.rs`, `rts compile` emits `.o` + native link) and JIT
(`module_jit.rs`, `rts run`) share the lowering; `FnCtx.module` is
`&mut dyn Module`. See design doc §9 (Pilar 5) and the per-pillar sections.

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
- `guards.rs` — `guard_for(expected, caller)`. The engine makes coercion ONE real
  authority (design doc §7, Pilar 3); the old ad-hoc `TPL_COERCE_AUTO` scattering
  was removed with the old engine.

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
`process`, `os`, `collections`, `hash`, `fmt`, `crypto`, `net`, `tls`,
`thread`, `atomic`, `sync`, `parallel`, `mem`, `hint`, `ptr`, `ffi`, `regex`,
`runtime`, `test`, `trace`, `alloc`, `json`, `date`, `http_server`,
`promise`, `events` + `globals/` sub-namespaces. Covers std::* + parallelism +
HTTPS + JSON + Date + native HTTP server (actix-web) + global JS classes.

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
- `regex/` — `regex` crate. `runtime/` — eval_file/eval + hot-reload.
  `trace/` — Bun-style frame stack. `events/` — EventEmitter. (UI namespace
  removed — FLTK dropped; egui-based UI planned.)

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

This is the JS/TS surface the engine must support. Some parenthetical notes below
describe how the deleted old engine implemented a feature — historical context
only. The engine reproduces these **semantics** through the `PolyValue`/shapes/IC
model; see the design doc's coverage plan and `.claude/rules/03-features.md`.

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
  **Documentation language: English** — all docs/specs/README are written and
  maintained in English (owner decision 2026-07-05).
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

**Read the ITERATION SPEED mandatory rule first.** The commands below are split
by WHEN you run them; running the merge-time column while iterating is what the
rule exists to stop.

### While developing (seconds)

```bash
cargo check -p <crate>                     # "does it compile" — the default loop
cargo test -p <crate> --lib <filter>       # only the tests of the area touched
cargo run -- run file.ts                   # execute without a release build
cargo run -- test tests/foo.test.ts        # one TS file, debug binary
```

### Before commit only (minutes)

```bash
cargo build --release                      # release build (also produces the AOT archive)
target/release/rts.exe test                # FULL TS suite — merge gate, not a dev loop
bash scripts/read_before_commit.sh         # the engine gate
$env:RUST_BACKTRACE="full"; target/release/rts.exe run file.ts            # JIT
$env:RUST_BACKTRACE="full"; target/release/rts.exe compile -p file.ts out # AOT
target/release/rts.exe apis                                   # list APIs
```

Benchmarks are release-only, always — a debug number is not a number.

**AOT archive is still a two-step build — it MOVED, it did not go away.** The
staticlib `build.rs` embeds for AOT linking is **`rts-runtime`'s**: `rts-runtime`
is `crate-type = ["rlib","staticlib"]` and, since the former standalone
`rts-adapters` crate folded into it as `src/adapters/`, its staticlib now
carries every `__RTS_*` extern "C" symbol AND the codegen-owned `__rtsadp_*`
trampolines in one archive — no merge, no duplicate symbols.

Run `cargo build -p rts-runtime` BEFORE building `rts`. Cargo emits a
`staticlib` only for a package built as a DIRECT TARGET, and being a direct
DEPENDENCY of the `rts` bin is NOT the same thing — so this is exactly the
pre-step `cargo build -p rts-adapters` used to be, under a new name. Verified by
measurement: without it a plain `cargo build` leaves a STALE archive carrying no
`__rtsadp_*` symbols, and `rts compile` dies in the linker.

Skipping the pre-step does NOT break JIT — `build.rs`
embeds a placeholder and `rts run` works; only `rts compile` (AOT) errors with a
"rebuild the runtime archive" message until the staticlib exists.

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
per user fn + `__rts_startup` (stderr, no execution). Use when suspecting
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

## Open heavy items

(Status + parity number: see "HONEST CURRENT STATUS" above.) Still open: #195
mutable closures (partly landed), #207 real async event loop, #216/#222 Symbol,
#217 weak WeakMap/Set + FinalizationRegistry, #218 Proxy (get/set/delete traps
landed), #219 BigInt, #223 dynamic import, #301 var hoisting, #304 toString/valueOf
coercion.

## Docs

`docs/specs/` holds feature specs, design decisions, technical notes — index at
`docs/specs/INDEX.md`. **Canonical engine direction:
`docs/specs/rts-codegen-new-design.md`** (the redesign plan; read before engine
work). **Canonical std-surface direction: `docs/specs/rts-std-surface.md`** —
the approved public-surface redesign (JS/Web globals + per-module camelCase
`rts:<ns>` imports exporting the Rust std; hard cutover; bytes = TypedArrays;
comptime `includeBytes`/`includeString`; `native()` reverse-FFI +
`rts compile --lib`; primitive relocation into rts-primitives; phases F0→F8) —
read it before any change to namespaces/public API. **Engine threading
direction: `docs/specs/rts-threading-model.md`** (per-thread regions + shared
heap with promotion on publication; `threadLocal`/`shared`/`channel` surface;
phases T0→T5). Detailed rules in `.claude/rules/` (00-meta → 05-codegen-notes),
each binding.
