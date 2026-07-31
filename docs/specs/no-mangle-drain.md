# Draining `no_mangle` — the hand-written symbol campaign

**Goal:** zero hand-written `#[unsafe(no_mangle)] pub extern "C" fn` in the tree.
Every runtime symbol is declared with the `rtse` macros and linked by
`rts-symbol-baker`, per the binding rule in `CLAUDE.md`
("MANDATORY RULE: SINGLE SOURCE OF TRUTH").

This document is the working plan. It records what was **measured**, not
estimated — re-measure before trusting any number here.

Canonical background: `docs/specs/rts-macro-single-source.md`.

---

## 1. The landscape, measured 2026-07-31

Counting the ATTRIBUTE (`#[unsafe(no_mangle)]` / `#[no_mangle]` at line start),
not textual mentions — the naive `grep -c no_mangle` over the tree returns 1184
because the macro and the baker carry the string in their own generator source.

```
grep -rhoE "^\s*#\[(unsafe\()?no_mangle" crates/ --include=*.rs | wc -l
```

**1126 attributes**, across 12 crates:

| crate | count | note |
|---|---:|---|
| `rts-shared` | 404 | the bulk of the campaign |
| `rts-napi` | 164 | **mostly NOT convertible** — see §2 |
| `rts-node` | 157 | already 441 `rtse::` uses — partially drained |
| `rts-dom` | 103 | |
| `rts-std` | 90 | already 469 `rtse::` uses — mostly drained |
| `rts-primitives` | 73 | |
| `rts-egui` | 57 | see the MANDATORY egui-plan rule before touching |
| `rts-engine` | 40 | |
| `rts-input` | 20 | |
| `rts-render` | 7 | |
| `rts-macro` | 6 | generator source, not real symbols |
| `rts-runtime` | 5 | |

Classified by symbol NAME (the convention tells you whether it is ours):

| convention | count | disposition |
|---|---:|---|
| `__RTS_FN_*` | **951** | the real conversion target |
| `napi_*` / `node_api_*` | ~154 | **permanent carve-out**, see §2 |
| other | ~21 | baker/macro test fns + a few already on the new `__rtsadp_*` / `__rtsn_*` convention |

**So the campaign is ~951 symbols, not 1184.**

## 2. The carve-out: N-API is a FOREIGN ABI, do not convert it

`rts-napi`'s `napi_*` and `node_api_*` symbols are the **Node-API C contract**.
A compiled `.node` addon links against those exact names. They are not RTS
symbols that happen to be hand-written — the name IS the interface, and the
baker's naming rule (`rts_abi::scope`) would rename them and break every addon.

Leave them. If the drain gate ever counts them, it should exclude
`crates/rts-napi/` by name and say why. The 4 `__RTS_*` symbols inside
`rts-napi` (the loader surface) ARE convertible.

Same question should be asked before touching `rts-egui` / `rts-input` /
`rts-render`: check whether any symbol is an external contract before renaming.

## 3. How a conversion goes

Per `docs/specs/namespace-creation-guide.md`:

1. Replace the hand-written `#[unsafe(no_mangle)] pub extern "C" fn __RTS_FN_...`
   with the appropriate `rtse` attribute — `#[rtse::abi]` for a free ABI fn,
   `#[rtse::class]` + `#[rtse::method]` / `#[rtse::statical]` / `#[rtse::ctor]`
   for class surface, `#[rtse::function]` for a free registry function.
2. The symbol name is **derived** by `rts_abi::scope`; do not spell it.
3. One row in the `REGISTER` list
   (`rts-codegen-new/src/front/run/registry_build.rs`) if a new class/namespace.
4. `cargo run -p rts-symbol-baker`, commit the regenerated artefact.
   `cargo run -p rts-symbol-baker -- --check` must be clean — the gate HARD-fails
   on drift.

## 4. Traps, all of them observed rather than predicted

These cost real debugging time. Read before starting.

**A symbol renames — every consumer must move with it.** The baker will happily
emit the new name while a hand-written signature row, a `.ts` prelude, or a
codegen `call_runtime("...")` still names the old one. Grep the OLD name across
`--include=*.rs --include=*.ts` before deleting it.

**A `Handle` return without `ret_ts` becomes a TS `object` and deadlocks.**
Recorded from the `rts-std` conversion. Changing the macro's default would break
~40 files, so annotate per site.

**An unlisted ABI shape reads as "not a function", silently.** The runtime-CI
marshaller (`rts-runtime/src/adapters/value/dynci.rs`) matches on
`(args, ret)` shapes and returns `None` for anything unenumerated; the caller
reports that as `TypeError: <m> is not a function`. So a correctly registered
method whose signature has no arm looks ABSENT rather than failing loudly. If a
converted member is "missing" at runtime, check the marshaller arms first.

**Deleting a `.ts` prelude class costs more than its method list.** Measured on
`object.ts`, which turned out to be serving FOUR roles at once:
  1. the instance-method library (the obvious one);
  2. the base that `extends Object` resolved against;
  3. the source of MATERIALIZED prototype slots, which is what `"m" in obj`
     walks — a Registry class does not create slots;
  4. the only populator of `desc.statics`, i.e. the only reason
     `const f = Object.keys` (a static read as a VALUE) worked.

  Role 4 has **no replacement in the engine today, for any class** —
  `Number.isNaN` read as a value fails identically. Check all four before
  deleting a prelude class.

**Registering a class changes CONSTRUCTION routing.** `is_global_class_ctor`
used to treat "registered" as "constructible", so adding instance members to
`Object` silently rerouted `new Object()` into `emit_registry_ctor` and it failed
with "no matching constructor". Now decided by whether the class carries a ctor.

**A new symbol requires the baker; a cached program requires `CACHE_VERSION`.**
`CACHE_VERSION` is hand-maintained in two places.

## 5. Verification protocol (non-negotiable)

The rule that saved this campaign from shipping a false claim: **every
"pre-existing failure" assertion must be produced by running the failing test on
a clean tree, not inferred from a commit message.** In this session a failure was
documented as pre-existing on the strength of another commit's notes; measuring
`main` directly disproved it, and the claim had already been written into a
commit message.

Per change:
1. `cargo check -p <crate>` while iterating — never `--release`, never the full
   suite (the ITERATION SPEED rule).
2. Before commit: `cargo build --release`, `target/release/rts.exe test`,
   `cargo test --release -p <crate> --lib` for each touched crate, and
   `bash scripts/read_before_commit.sh`.
3. `cargo test --release --workspace --lib` **does not link** — `rts-std` /
   `rts-node` test binaries reference `__rtsadp_*` symbols that live in
   `rts-runtime`, a crate above them. Run per crate. Note also that
   `cargo test --release --lib` alone runs **0 tests** (root lib only).
4. Diff the failing-test SET against the clean tree, not the count.

## 6. Suggested order

Lowest risk first, so the mechanics are proven before the big surface:

1. **`rts-runtime` (5)** and **`rts-engine` (40)** — small, and the engine's are
   GC/env internals with few consumers.
2. **`rts-primitives` (73)** — primordials, the pattern is already proven here
   (String/Boolean/Number/Object are done; `error.ts` is the last `.ts` left).
3. **`rts-shared` (404)** — the bulk. Namespace-shaped (`__RTS_FN_NS_<NS>_*`),
   so it drains namespace by namespace: `alloc`, `bigfloat`, `math`, `num`, …
4. **`rts-dom` (103)**, **`rts-node` (157)** — already partly drained.
5. **`rts-std` (90)** — mostly drained already.
6. **`rts-egui` (57)** — ONLY after reading the frozen egui plan
   (`docs/specs/html-engine/`), per the MANDATORY rule.
7. `rts-napi` — only the 4 `__RTS_*`; leave the N-API contract alone.

## 7. Open work that is NOT part of this campaign

Carried over, so it is not lost:

* **Two live regressions on `main`:**
  * `claude-stringify-wrapper-objects` (10 tests) — `JSON.stringify(new Number(5))`
    yields `undefined`. Cause traced OUT of the Object migration by
    elimination: disabling the `dynci` untagged→`Object` fallback, the
    `try_primitive_class_method` fall-through, and the `Object` class
    registration each left the failure intact. Bisect `e1ce6e5a` against main's
    `52e5ada9` instead.
  * `claude-object-statics-como-valor` — needs the missing "Registry class
    static read as a VALUE" path (§4, role 4).
* **`RtseTrace` has zero users.** The hook, `alloc_rtse_traced` and its test
  exist and pass, but the macro's ctor/return path still calls `alloc_rtse`.
  Wiring it up unblocks `Map`/`Set`/`WeakMap` in Rust and closes #217 (a weak
  collection is the class that deliberately does NOT trace its keys).
* **GC precise scanning is transport-only.** `parcompile`/`module_jit` extract
  and register `UserStackMap`s, but nothing calls
  `declare_value_needs_stack_map` — the three occurrences in the tree are
  comments. The root set is `Repr::Ref(_)` / `Repr::Tagged` (`repr.rs`). Fix the
  scratch-module hazard FIRST: `PENDING`/`REGISTRY` are process-global while
  `bake.rs::capture_compiled` populates a separate `JITModule` with its own
  `FuncId` numbering.
* **`STRING_CONCAT` is O(n²).** `alloc.rs` snapshots and copies both operands per
  concat; the engine's own comment records measuring 20k concats → 288 MB and
  80k → 3.6 GB, mitigated only by triggering GC earlier. Needs a cons-string /
  rope `Entry` variant.
* **`objops` get-miss allocates per method dispatch** — `format!("__get_{key}")`
  + a non-deduplicated `intern_poly` on every prototype-method dispatch. Design
  sketched: a global "program has accessors" flag set in the key-insertion
  funnel, complete by construction.
* **`Map` with OBJECT keys is still O(n)** — `map_set.ts`'s `__hkey` puts every
  non-primitive key in bucket 0.
* **Object drain incomplete** — 20 `__rtsadp_obj_*` remain in the engine,
  `objstatic.rs` is still 654 lines of hardcoded routing.
* **AOT semantic gaps** — `new Function` (the `COMPILE_FN_HOOK`) and the pickle
  fn-by-reference table are registered only on the JIT path.
* **Stale docs** — `CLAUDE.md` and `.claude/rules/02-runtime.md` still describe
  precise `UserStackMap` GC scanning as the CURRENT state; it is not. The
  Map/Set doctrine also needs updating for the move into `rts-primitives`.
