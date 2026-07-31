# Draining `no_mangle` — the hand-written symbol campaign

**Goal:** zero hand-written `#[unsafe(no_mangle)] pub extern "C" fn` in the tree.
Every runtime symbol is declared with the `rtse` macros and linked by
`rts-symbol-baker`, per the binding rule in `CLAUDE.md`
("MANDATORY RULE: SINGLE SOURCE OF TRUTH").

**Status 2026-07-31: 1126 → 233 attributes.** Of what remains, 157 are the
permanent N-API contract (§2), 57 are `rts-egui` (blocked on the MANDATORY
egui-plan rule), 6 are `rts-macro`'s own generator source (the `no_mangle` the
macro EMITS — not a symbol), and **13 are genuine carve-outs**, each with its
reason written next to the declaration (§2b). See §6 for what is left to do.

This document is the working plan. It records what was **measured**, not
estimated — re-measure before trusting any number here.

Canonical background: `docs/specs/rts-macro-single-source.md`.

---

## 1. The landscape

Counting the ATTRIBUTE (`#[unsafe(no_mangle)]` / `#[no_mangle]` at line start),
not textual mentions — the naive `grep -c no_mangle` over the tree returns 1184
because the macro and the baker carry the string in their own generator source.

```
grep -rhoE "^\s*#\[(unsafe\()?no_mangle" crates/ --include=*.rs | wc -l
```

### Measured 2026-07-31, BEFORE the campaign — 1126 attributes

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

### Measured 2026-07-31, AFTER — 233 attributes

| crate | count | what is left |
|---|---:|---|
| `rts-napi` | 159 | 157 N-API contract (§2) + 2 out-param carve-outs |
| `rts-egui` | 57 | **untouched** — blocked on the MANDATORY egui-plan rule |
| `rts-macro` | 6 | generator source: the `no_mangle` the macro EMITS, not a symbol |
| `rts-engine` | 3 | `STRING_NEW`/`STRING_PTR`/`STRING_FROM_STATIC` (§2b) |
| `rts-node` | 2 | the two `PumpFn` pointers (§2b) |
| `rts-std` | 1 | `FETCH_RESPONSE_OK` — returns `i8` (§2b) |
| `rts-runtime` | 1 | `__rtsadp_ta_view_base_len` — out-params (§2b) |
| `rts-primitives` | 1 | `__RTS_FN_RT_PROXY_RESOLVE` — out-params (§2b) |

`rts-shared`, `rts-dom`, `rts-input` and `rts-render` are at **zero**. Excluding
the two blocked//not-a-symbol groups (`rts-egui` 57, `rts-macro` 6) and the
N-API contract (157), **13 hand-written declarations remain**, each with its
reason written next to it in the source.

## 2b. The carve-outs that are NOT N-API

Seven distinct reasons, all discovered by the macro REFUSING the signature —
which is the system working. None is a workaround; each is recorded in the
source next to the declaration.

* **Out-params.** `*mut T` is a stack slot the caller owns and the callee writes
  THROUGH, not a value crossing by copy. There is no single-slot ABI spelling
  for that, and the `U64`-address escape used for `*const u8` would misstate the
  direction of the data. `__rtsadp_ta_view_base_len`,
  `__RTS_FN_RT_PROXY_RESOLVE`, `__RTS_FN_RT_NAPI_INVOKE_METHOD`,
  `__RTS_FN_RT_NAPI_DISPATCH_CALLBACK`.
* **Raw bytes that must not be validated.** `__RTS_FN_NS_GC_STRING_NEW` and
  `__RTS_FN_NS_GC_STRING_FROM_STATIC` take `(*const u8, i64)` that must NOT gain
  the UTF-8 validation a `&str` param imposes, and `__RTS_FN_NS_GC_STRING_PTR`
  RETURNS `*const u8`, which has no spelling in `rts_abi::tymap`. (Elsewhere the
  `(u64, i64)` address form solved this; here the pair is also named by a
  hand-written `abi_sig` row as `StrPtr`, so it stays as the pair it is.)
* **`usize` returns.** `__RTS_FN_NODE_DGRAM_PUMP` / `__RTS_FN_NODE_NET_PUMP` are
  `PumpFn` fn POINTERS registered with `rts_engine::loop_sources::register_pump`,
  whose contract returns `usize`. They are never called by symbol from generated
  code, so a descriptor buys nothing.
* **`i8` return — a flag, not a carve-out.** `__RTS_FN_GL_FETCH_RESPONSE_OK`
  returns `i8`. CLAUDE.md is explicit that a Bool crosses as **i64 and never as
  i8**, so this return type is suspect on its own terms. It was deliberately NOT
  "fixed" during a mechanical drain: correcting it is an ABI change that needs
  its own verified commit.

### The address spelling (`u64`), and when NOT to use `&str`

129 functions took a raw `(*const u8, i64)` pair. Each became `(u64, i64)` with
the pointer re-introduced as the body's first statement
(`let ptr = ptr as *const u8;`), so no function body changed. `U64 + I64` and
`StrPtr` both lower to the same two i64 slots, so the machine ABI is identical.

`&str` is the other option the macro offers and is the WRONG one for these: the
bytes come out of `Entry::String`, and `&str` would impose UTF-8 validation the
current path does not perform.

### `macro_rules!` bodies are invisible to the baker

The baker walks syn items, so a `#[unsafe(no_mangle)]` emitted from inside a
`macro_rules!` body is in NOBODY's source of truth. Four such factories existed
(`ta_ctor!`, `atomics_rmw!`, `nav_fn!`, `arity_variants!` — 64 symbols). All four
now use `#[rtse::abi]`. Two idioms keep the name from being spelled twice:

* **Bare `#[rtse::abi]`** prefixes the Rust fn name with `__` and RENAMES the fn
  to the result. So invoking the macro with the name minus its two leading
  underscores reproduces the existing symbol exactly, and the Rust ident is
  `__RTS_…` again after expansion — nothing referencing it has to move.
* **`#[rtse::abi(native)]`** derives `__rtsn_<fn name>`; used where a rename was
  acceptable (`ta_ctor!` → `__rtsn_ta_new_*`). These 8 originally used the `abi`
  scope (`__rtsa_*`); that scope was deleted on 2026-07-31 — it duplicated
  `native`, since everything it covered is "the Cranelift IR cannot express
  this" — and they were re-pointed at `native`.

They still do not reach the baked table. That is a real baker limitation, now
stated rather than an unexamined silence.

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
3. `cargo test --release --workspace --lib` **does not link** — test binaries
   reference `__rtsadp_*` symbols that live in `rts-runtime`, a crate above
   them. Measured 2026-07-31 on clean `main`: this affects `rts-std`,
   `rts-node`, **and also `rts-shared` (36 unresolved), `rts-primitives` (35)
   and `rts-napi` (36)** — i.e. every crate below `rts-runtime`, not just the
   two originally listed. Do not read those link failures as a regression. Note
   also that `cargo test --release --lib` alone runs **0 tests** (root lib only).
4. Diff the failing-test SET against the clean tree, not the count.
5. Bake the symbol table before and after and diff the NAME SET (§6). A
   name-preserving slice must show zero added and zero removed.

### The baseline, measured on clean `main` 2026-07-31

Re-measure rather than trusting these, but they are what "no regression" meant
throughout this campaign:

* `target/release/rts.exe test` → **771/774 files, 2837/2849 tests**. The three
  failing files are `claude-dom-script-globals` (1 test),
  `claude-object-statics-como-valor` (crashes) and
  `claude-stringify-wrapper-objects` (10 tests) — the last two are the §7
  regressions.
* `cargo test --release -p rts-codegen-new --lib` → **837 passed, 7 failed** on
  clean `main`, including
  `abi_sig::baked_existence::every_named_symbol_is_in_the_baked_table`, whose
  own source-extraction sanity check trips (it finds 4 names for 6 match arms).
  That guard is currently not guarding anything; worth its own fix.
* `cargo test -p rts-engine --lib` → 45 passed (was 48 before the dead `env.rs`
  tests went with the dead code they tested).

## 6. Order — DONE except `rts-egui`

Lowest risk first, so the mechanics were proven before the big surface:

1. ✅ **`rts-runtime` (5 → 1)** and **`rts-engine` (40 → 3)**.
2. ✅ **`rts-primitives` (73 → 1)**.
3. ✅ **`rts-shared` (404 → 0)** — the bulk.
4. ✅ **`rts-dom` (103 → 0)**, **`rts-node` (157 → 2)**.
5. ✅ **`rts-std` (90 → 1)**, plus **`rts-input` (20 → 0)** and
   **`rts-render` (7 → 0)**, neither of which exported a foreign-ABI name.
6. ⛔ **`rts-egui` (57)** — the ONLY remaining convertible group. Read the frozen
   egui plan (`docs/specs/html-engine/rts-html-roadmap.md` F0–F5 +
   `rts-html-north-star.md` + `arquitetura.md` +
   `docs/specs/egui-ui-crate-design.md`) IN FULL first, per the MANDATORY rule in
   `CLAUDE.md`. Not started here for exactly that reason.
7. ✅ `rts-napi` — the convertible `__RTS_*` are done
   (`__RTS_FN_NS_NAPI_LOAD_ADDON`, `__RTS_FN_RT_NAPI_NEW_INSTANCE`); the other
   two have out-params (§2b). The N-API contract is untouched.

### The one strong invariant this campaign held

Every slice was checked by baking the symbol table before and after and diffing
the NAME SET. For the `rts-std`/`rts-node`/`rts-dom`/`rts-input`/`rts-render`
slice and for the `rts-napi` slice the result was **2191 symbols before, 2191
after, zero added, zero removed** — mechanical proof that the conversion is
name-preserving, which no amount of reading the diff would give you.

Use it on any future slice: it is cheaper than the release build and catches the
§4 rename trap directly.

## 6b. Dead symbols found and deleted along the way

The drain doubled as an audit: a symbol nothing names is a symbol you notice
when you have to touch every one of them. 27 in `rts-engine` turned out to be
unreachable leftovers of the DELETED engine's overloaded-i64 value model and
were removed rather than converted — whole files in four cases. `cell.rs`
(codegen cells are `emit_vec_get`/`_set`), `tagged_raw.rs` (`.raw` is desugared
in `desugar/tpl.rs`), `float_box.rs` (CLAUDE.md already recorded PolyValue as
deleting these), `env.rs` (only its own `#[cfg(test)]` called it), the
`i64::MIN+n` sentinel family in `coerce.rs`, four probes in `alloc.rs`
(`Array.isArray` runs through `adapters/value/globalops.rs`), and
`CLASS_REGISTER_PARENT` (codegen seeds via the Rust `register_parent`).

Each was confirmed by grepping the name across `*.rs` AND `*.ts` — which
matches string literals, so a Registry `instanceof_predicate("…")` or a codegen
`call_runtime("…")` would have shown up. The full suite then agreed.

Still-live dead-ish code NOT removed, for the record: `this_slot.rs` is
**write-only**. `rts-primitives/src/function/ops.rs` pushes and pops the
thread-local `this` stack in seven places and NOTHING reads it —
`__RTS_FN_RT_THIS_GET` has no caller anywhere, and the engine passes `this` by
other means. It was converted rather than deleted because unpicking the
`pushed_this_slot` control flow in the hot dispatch path is its own change with
its own verification, not a drain side effect.

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
