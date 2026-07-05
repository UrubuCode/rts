# `rts-codegen` engine redesign — canonical design document

> **Status:** canonical specification of the codegen engine redesign (the
> TypeScript/JavaScript → native code engine, Cranelift backend). It is THE
> reference document: the team and future agents follow **this**. The live crate
> of the redesign is `crates/rts-codegen-new/`; the old, frozen engine is
> `crates/rts-codegen-old/`. Until the strangler-fig migration finishes, the
> `bin`/`cli` remain plugged into `rts-codegen-old`.
>
> **Language:** pt-BR. **Thesis:** *prove-monomorphic-and-unbox where the type
> system can (preserving the winning numeric path); fall to ONE honest tagged
> in-value representation + shapes + AOT-safe data inline-caches where it
> can't.*

---

## Table of contents

1. [Historical context — the truth about the 100%](#1-contexto-histórico--a-verdade-sobre-os-100)
2. [What the old engine really is (the problems to fix)](#2-o-que-o-motor-antigo-realmente-é-os-problemas-a-corrigir)
3. [What is genuinely good (preserve intact)](#3-o-que-é-genuinamente-bom-preservar-intacto)
4. [The new thesis and the mappings to the crate's modules](#4-a-nova-tese-e-os-mapeamentos-para-os-módulos-do-crate)
5. [Pillar 1 — PolyValue (`value.rs`): a 64-bit NaN-boxed value](#5-pilar-1--polyvalue-valuers-um-valor-nan-boxed-de-64-bits)
6. [Pillar 2 — Representation lattice (`repr.rs`)](#6-pilar-2--lattice-de-representação-reprrs)
7. [Pillar 3 — Soundness rule + trust in TS types](#7-pilar-3--regra-de-solidez--confiança-nos-tipos-ts)
8. [Pillar 4 — Shapes (`shape.rs`) + data inline caches (`ic.rs`)](#8-pilar-4--shapes-shapers--inline-caches-de-dado-icrs)
9. [Pillar 5 — Single lowering path (`front/run/`), no MIR](#9-pilar-5--caminho-único-de-lowering-frontrun-sem-mir)
10. [Pillar 6 — Data-driven dispatch (`dispatch.rs`) + generated ABI (`abi_gen.rs`)](#10-pilar-6--dispatch-data-driven-dispatchrs--abi-gerada-abi_genrs)
11. [Why this is simpler than V8 (and the honest cost)](#11-por-que-isto-é-mais-simples-que-o-v8-e-o-custo-honesto)
12. [Strangler-fig migration plan](#12-plano-de-migração-strangler-fig)
13. [What gets deleted from the old engine (by name)](#13-o-que-é-deletado-do-motor-antigo-por-nome)
14. [Appendix — crate module map](#14-apêndice--mapa-de-módulos-do-crate)
15. [Module system (new engine)](#15-sistema-de-módulos-motor-novo)

---

## 1. Historical context — the truth about the 100%

On 2026-06-06/07 RTS reached **100% cross-runtime parity** (372/372, 0
divergences; TS suite 1719/1719), tag `v0.0-202606072107`, commit `27e16378`.
This is factual and verifiable in git. **But it is important to say honestly how
it was achieved:** on the **old engine**, by grinding hardcoded special cases
inside gigantic files — the *same* architecture it has today:

| File (`rts-codegen-old`)                                       | LOC    |
|----------------------------------------------------------------|--------|
| `src/codegen/lower/expressions/calls/mod.rs`                   | 4622   |
| `src/codegen/lower/passes/parallelism.rs`                      | 3189   |
| `src/codegen/lower/passes/this_arrow.rs`                       | 2810   |
| `src/codegen/lower/expressions/operators.rs`                   | 2616   |
| `src/codegen/lower/expressions/members.rs`                     | 2592   |
| `src/codegen/jit.rs`                                           | 2784   |

The 100% was the **local maximum of the hardcoded approach**, not a validation
of the design. The old engine's own `MAINTENANCE.md`, at the 100% commit,
admitted the wall in so many words:

> *"a bool loses its tag when crossing a function boundary because parameters/
> returns are `i64` and `true == 1`. Tagging bools as sentinels was TRIED and
> REVERTED — it breaks 83 TS tests. It needs a real bool-type-tracking
> refactor, not a sentinel hack."*

This is the **value representation problem**, self-identified at the peak.

After the 100%, the fixture set grew from 391 → 612 (harder cases) and the
badge dropped to 70.7%. In that window, 126 codegen commits drained hardcoded
logic toward the Registry (**good**) — but also added `Entry::FloatPrim`
("narrow-storage"), reintroducing *boxing* (**wrong direction**, see §2.2). The
redesign exists so the next plateau is not another local maximum of hacks.

---

## 2. What the old engine really is (the problems to fix)

### 2.1 Value representation: an overloaded `i64` slot

There is **one** `i64` ABI slot that means, depending on context:
`{ pure int, GC handle, boxed float, string handle, sentinel
undefined/null/bool = i64::MIN + k }`. The "type tag" **was not eliminated** —
it was *scattered* across four compile-time side-tables inside a `FnCtx` with
**93 fields** (`crates/rts-codegen-old/src/codegen/lower/ctx.rs`):

```rust
// ctx.rs — as quatro side-tables (linhas 498/503/509/514)
pub fresh_handle_set:        HashSet<cranelift_codegen::ir::Value>,
pub optional_chain_values:   HashSet<cranelift_codegen::ir::Value>,
pub var_member_call_values:  HashSet<cranelift_codegen::ir::Value>,
pub var_vec_slot_values:     HashSet<cranelift_codegen::ir::Value>,
```

Plus AST-shape heuristics (`is_map_get_call`), plus runtime re-tag helpers
(`__RTS_FN_RT_FLOAT_BOX/UNBOX/EQ_AMBIG/NUM_ARITH`).

It is **unsound by construction**: a new container accessor that *forgets* to
register an `ir::Value` in the right side-table silently mis-coerces the value
— **a silent wrong numeric result**, the worst possible failure mode (no crash,
no diagnostic).

### 2.2 `Entry::FloatPrim` re-boxes floats

To fit into `Map<String,i64>` / `Vec<i64>`, fractional floats are re-boxed.
The doc-comment in `crates/rts-engine/src/heap/handles.rs:485` itself confesses:

> *"`FloatPrim` is a primitive number whose f64 bits don't fit in the
> container's i64 without ambiguity — so it is boxed and the read-back
> (typeof/===/arith/INSPECT) unwraps it as a primitive NUMBER."*

"No boxing" is false one layer down. And it **does not scale**: every new
container-storable type would need its own BOX/UNBOX/EQ/ARITH quartet. Today
`Entry` already carries `String`, `FloatPrim`, `StringBox`, `NumberBox`, … —
each with its own little re-tag zoo.

### 2.3 Objects: V8 dictionary as the *default*

An object's default is `HashMap<String,i64>` + `__proto__` links + a class tag
as a string. This is **V8's dictionary/slow-mode turned into the default**. The
`members.rs` (2592 LOC) then hand-rolls ~30 compile-time paths to *steer away*
from the hashmap = hidden-class optimization **without a hidden class**. A real
("flat") struct layout exists, but it is **gated behind an environment
variable** — i.e. the fast path is the exception, not the rule.

### 2.4 Virtual dispatch: linear string comparison

Method override is resolved by **O(N) string-literal allocations +
`gc.string_eq` per call site per override**. It is a megamorphic inline-cache
implemented as a *linear string comparison*.

### 2.5 Two optimizer tiers

`HIR → MIR (84-instruction SSA; fold/cse/dce/fma/narrow/inline passes) →
Cranelift`. The MIR passes **re-do** what Cranelift's egraph
(`use_egraphs=true`, set in `emit.rs:91` and `jit.rs:97`) already does.
`crates/rts-mir/src/passes/fold.rs:16` itself admits:

> *"Float folding is intentionally omitted — Cranelift's e-graph pass with
> `use_egraphs=true` already covers it intraprocedurally."*

Worse: the MIR only accepts a numeric *whitelist*; **~99% of real JS silently
bails** to a **separate and complete** AST→Cranelift path. That is **two
complete codegens** maintained in parallel.

### 2.6 `guards.rs` is dead code

`crates/rts-engine/src/abi/guards.rs::guard_for` — the supposed coercion
authority for `any` arguments — has **zero production call sites**. The only
references to `guard_for` are its own definition (line 45) and three calls
*in the file's own tests* (lines 64/72/80). The real coercion is ad-hoc:
`TPL_COERCE_AUTO` scattered across 12 occurrences in `operators.rs` and dozens
of other files.

### 2.7 `jit.rs`: 1113 manual `add_fn!`

`crates/rts-codegen-old/src/codegen/jit.rs` registers **exactly 1113** runtime
symbols by hand (`add_fn!`). A rename → *link OK* + **SIGILL from an ABI
mismatch at runtime**, with no build-time verification whatsoever. It is an
entire class of latent bugs the compiler does not catch.

### 2.8 Duplication in the switchboard

In `calls/mod.rs`: `JSON.stringify` logic appears ~5× duplicated, `Math.max`
2×, hardcoded lists of `console.*`. The 4622-LOC switchboard is the heart of
the architectural problem: **builtins in the engine**, instead of metadata in
the Registry.

---

## 3. What is genuinely good (preserve intact)

### 3.1 The monomorphic numeric path

Flat `extern "C"` primitives (`AbiType` = 8-variant enum; `StrPtr` is the only
2-slot case), intrinsic inlining (`sqrt`/`abs`/`min`/`max` as direct Cranelift
IR), and **Cranelift's egraph as the real optimizer**. Metrics: Monte Carlo
~5× Bun, AOT 16.9 ms. **DO NOT TOUCH.** This path is the product.

### 3.2 The Registry / PRIMORDIAL doctrine

The engine names *directly* ONLY the primordial classes (`String`, `Object`,
`Array`, `Function`, `Promise`, `Boolean`, `Number`, `Error` + subclasses).
Everything else resolves via the **real Registry** (`registry.rs` builds from
`Engine::new()` + the `register`/`register_class_spec` fns; `registry_call.rs`
is the generic marshal from the `Member`'s `AbiType`s) → **a single generic
INVOKE**. Correct and scalable.

**The dividing line is NATIVE SYNTAX (binding clarification from the owner):**
- **Native syntax ⇒ PRIMITIVE ⇒ codegen-direct (rts-primitives):** literals
  `""`/`123`/`true`/`[]`/`{}`/function/**`/re/` (RegExp has native syntax → it
  is a primitive, NOT Registry)**/template, + `Error` (primordial). The engine
  names + lowers the syntax directly; impl in `rts-primitives`. **`Error`
  migrated to `.ts`** (prelude include, §3.2.1): the fields/methods impl is in
  `rts-primitives/src/error.ts`, no longer hardcoded in codegen.
- **No native syntax ⇒ rts-shared utility lib, indirect:** `Date`/`Map`/`Set`/
  `WeakMap`/`WeakSet`/`JSON`/`URL`/`Math`/`Promise`/`Proxy`/typed-arrays/backend —
  reached via `new X()`/statics, **never reimplemented as codegen `__rtsadp_*`
  tables**. Two sub-paths, both with the engine never naming the class:
  - **Registry (data dispatch):** `Date` is the reference (done) — ctor/
    statics/methods resolve via `MethodSpec`/`Sig.default_args`/flags through
    `is_pure_registry_class` + `registryclass.rs`. Target of `URL`/typed-arrays/
    `TextEncoder`/backend (they have a real Rust impl, no native syntax).
  - **`.ts` stdlib (rts-shared/stdlib):** **COLLECTIONS** (`Map`/`Set` — done;
    `WeakMap`/`WeakSet` — done, strong-ref interim) are ambient `.ts` classes,
    NOT Registry. Reason: keys/values of **arbitrary** type that the i64 Rust
    backend cannot hold without the PolyValue containers (P2, deferred); the
    `.ts` (arrays holding PolyValue) covers it. `WeakMap`/`WeakSet` become
    **real** weak when the GC weak phase exists (§5.7, deferred until ~90%
    cross-runtime); until then they are strong-ref (same as the Rust v0 stubs).

### 3.2.1 `Error` migrated to `.ts` (prelude include) — DONE

`Error` is PRIMORDIAL (the engine may NAME it for throw/catch), but its
fields/methods IMPLEMENTATION **stopped being hardcoded in codegen** and now
lives in `.ts`: `crates/rts-primitives/src/error.ts` (exposed as
`rts_primitives::ERROR_TS`, re-exported by the `rts_runtime::ERROR_TS` facade).
It is included as a declarations-only prelude via `e.include(rts_runtime::ERROR_TS)`
in `registry.rs build_registry()`, **BEFORE** `MAP_SET_TS` (the `extends Error`
subtypes need to see the base `Error` first — the prelude is a merged program;
the `include` order is the declaration order).

- `class Error { message; name; stack; constructor(message?){ this.message =
  message ?? ""; this.name = "Error"; this.stack = engine.trace_capture(); }
  toString(){ ... } }` + `TypeError`/`RangeError`/`ReferenceError`/`SyntaxError`/
  `URIError`/`EvalError`/`AggregateError` as `class X extends Error`. `.stack`
  is a REAL trace via the PRIVATE global `engine.trace_capture()` (Error.ts is
  prelude-origin, so the privacy gate allows it).
- `new Error("x")` constructs through the normal USER-class path (Vec
  shape-id + `message/name/stack` slots); `.message`/`.name`/`.stack` are
  ordinary slots; `toString()` is the `.ts` method; `instanceof` walks the
  user-class inheritance chain (shape-id + descendants).
- **`extends Error` (user class):** the prelude is built FIRST and its
  `ClassTable` is passed as AMBIENT classes to the user program's
  `collect_classes` (`build_from_program(.., ambient)`), so `class X extends
  Error` resolves the parent against the prelude's real Error — the engine
  **no longer synthesizes a virtual Error parent in codegen**
  (`class/builtin.rs` was DELETED).
- **throw/catch interop:** a `throw new Error("x")` puts the `TAG_OBJECT` WORD
  in the error slot; `catch (e)` binds `e` as an opaque Tagged local WITHOUT a
  static class. `e.message`/`.name`/`.stack` fall into the dynamic fallback
  `__rtsadp_obj_get` (reads the slots by key from the shape-id header of the
  thrown object), and `e instanceof Error` uses `dynamic_user_instanceof`
  (compares the shape-id against Error+descendants). Both JS-correct.
- **Hardcode DELETED** (from the new engine only): `class/builtin.rs` (virtual
  parent synth) + its call-site; in `globalclass.rs` the `class_meta` lines of
  the Error family, `err_meta`/`ERROR_METHODS`, `is_error_class`, the
  `__rtsadp_err_message/name/stack` props, and the `instanceof`'s
  `__rtsadp_is_error`; in `wrappers.rs` the `__rtsadp_err_new*` ctors + props +
  `__rtsadp_is_error`; in `toprimitive.rs` the
  `is_error_instance`/`__rtsadp_err_to_string` path; the new engine's
  `registry.rs` `register_*_error_class_spec`; and the corresponding JIT
  symbols in `runtime_link.rs`/`abi_sig.rs`.
- **Kept on purpose:** the Rust runtime `globals::error` + the
  `__RTS_FN_GL_ERROR_*`/`__RTS_FN_GL_IS_ERROR` externs and the
  `register_*_error_class_spec` — the FROZEN OLD engine (`rts-codegen-old`)
  still uses them (members.rs, operators.rs, jit.rs). Not deleted (live old
  engine path).
- **Known limitation (not a tested-path regression):** calling a METHOD on an
  opaque CAUGHT error (`catch (e) { e.toString() }`) needs shape-keyed method
  dispatch (IC) — a future increment; today the dynamic `toString` returns the
  generic object default. The DATA surface (`e.message`/`.name`/`instanceof`)
  works. `e.toString()` is correct when `e` has a STATIC class
  (`new Error("x").toString()`).

### 3.2.2 PRIMITIVE methods migrated to `.ts` — `Boolean` (proof-of-mechanism) — DONE

The step that validates the "primitive method libraries become `.ts`"
direction: a method called on a PRIMITIVE receiver (`true.toString()`,
`flag.valueOf()`) is routed to a prelude `.ts` class, with the primitive
WRAPPED (boxed) as `this`. **This is NOT a JS prototype** — the new engine is
shape-based, with no real prototype objects. The resolution is the SAME as for
a user-class instance: the engine finds the `(method, arity)` in the prelude's
ambient class at COMPILE TIME and emits a direct `call` of the synthesized
method (`__rtsn_method_*`), passing the boxed primitive as `this`. Boolean is
the prover (minimal surface, almost no low-level ops) before String/Number.

- **File:** `crates/rts-primitives/src/boolean.ts` (`rts_primitives::BOOLEAN_TS`,
  re-exported by the `rts_runtime::BOOLEAN_TS` facade). Included as a
  declarations-only prelude via `e.include(rts_runtime::BOOLEAN_TS)` in
  `registry.rs build_registry()` (after `ERROR_TS`). `class Boolean {
  toString(): string { return this ? "true" : "false"; } valueOf(): boolean {
  return this ? true : false; } }` — NO constructor and NO fields: only
  prototype methods. The bodies read `this` AS THE PRIMITIVE (the boxed bool
  word), through an ordinary Tagged truthiness test (the SAME truthiness the
  Error `.ts` methods use) — no new low-level op was needed.
- **Dispatch mechanism (reusable for String/Number):**
  `front/run/method.rs::try_primitive_class_method(recv, prim_class, method, args)`
  — finds the ambient class in `self.classes`, resolves the method via
  `desc.method_fn(method)`, wraps the primitive (`box_value`) as `this` and
  calls it via `call_synth_fn` (the SAME path as `try_class_method`). The
  routing sits in `try_method_dispatch`: when the receiver is a proven bool
  (`Repr::Bool`) or Tagged-known-bool (`JsKind::Bool`) and `recv_class_of`
  fails, it tries `try_primitive_class_method(.., "Boolean", ..)`; `Ok(None)`
  falls to the dynamic/bail path (never a guess).
- **`new Boolean(x)` (WRAPPER, typeof === "object"):** still the engine's
  wrapper trampoline, NOT the `.ts` class (which is prototype-only).
  `is_global_class_ctor` gained the `is_wrapper_primordial` exception
  (Boolean/Number/String): a primordial wrapper remains a global-class ctor
  EVEN with the ambient `.ts` class — so `new Boolean(true)` constructs the
  wrapper object and the primitive method call `true.toString()` routes to the
  `.ts` class, without collision.
- **Coercions unchanged (regression guard):** `String(b)`/`${b}`/bool in a
  string `+` use the runtime ToString/`+` trampolines (`__rtsadp_to_string`/
  `__rtsadp_add`); `Boolean(x)` (coercion call, not `new`) uses
  `__rtsadp_g_boolean` in `globals.rs`. None of these go through the `.ts`
  class.
- **Hardcode removed vs kept:** NO bool method hardcode existed in the new
  engine to remove — `true.toString()`/`false.valueOf()` previously BAILED
  (`recv_class_of` returns `None` for bool; there is no `RecvClass::Boolean` in
  `dispatch.rs`). This change ADDS the path, without deleting dispatch. **Kept
  on purpose:** the Rust runtime `globals::boolean` + `__RTS_FN_GL_BOOLEAN_*` +
  `register_boolean_class_spec` — the new engine's `new Boolean(x)` wrapper
  still uses them (via `__rtsadp_w_boolean_new`) and the FROZEN OLD engine does
  too.
- **How String/Number follow the pattern:** move the `.ts` method lib (a
  prototype-only `class String`/`class Number`) to `rts-primitives`, include it
  in `build_registry()`, and route the PROVEN primitive receiver (string
  `JsKind::Str` / number `Repr::Float64`/`Int*`) via
  `try_primitive_class_method(.., "String"/"Number", ..)` BEFORE the
  `dispatch.rs::resolve_method` table path (which then drains line by line).
  `is_wrapper_primordial` already covers all three wrappers. The difference vs
  Boolean: String/Number need the `.ts` bodies to read the primitive via real
  low-level ops (length/charAt/format) — where an op is missing, the honest
  minimum is a private `rts:engine` helper, preferring to reuse the existing
  coercion/Registry.

### 3.2.3 `console` migrated to `.ts` (singleton global object) — DONE

`console` is the third case of "the engine does not name it, it is `.ts`":
unlike Error (a primordial class) and Boolean/Number/String (PRIMITIVE method
libs), `console` is a singleton GLOBAL OBJECT. Before, it was hardcoded in the
front (`is_console_ident` matching `n == "console"` + `lower_console_log`
formatting in codegen) — a violation of the "front only references" doctrine.
Now:

- **File:** `crates/rts-primitives/src/console.ts` (`rts_primitives::CONSOLE_TS`,
  re-exported by the facade). Included via `e.include(rts_runtime::CONSOLE_TS)`
  in `build_registry()`. It is a `class Console` + `const console = new Console()`;
  `console.log(...)` is an ORDINARY method call on an ambient instance, routed
  through normal dispatch (`try_class_method`). The front does NOT name console —
  `is_console_ident`/`lower_console_log`/`console_format_line` were DELETED.
- **`engine.*` bridges (the irreducible logic, runtime-side):** three private
  helpers in `engineobj.rs` that the `.ts` calls: `engine.display(x)` (render of
  ONE value → string; ToString-or-inspect decided at RUNTIME via
  `__rtsadp_inspect`, so the front no longer does the static object-vs-scalar
  split), `engine.print_line(s)` / `engine.eprint_line(s)` (capture-aware sink →
  `__rtsadp_print_line(ptr,len,to_stderr)`, stdout/stderr). The variadic join
  with spaces is pure `.ts` (iterates `...args`).
- **File:** `crates/rts-shared/src/stdlib/console.ts` — `console` is a
  NON-primordial backend class, so the `.ts` lives in `rts-shared/stdlib`
  (next to `json.ts`/`map_set.ts`), NOT in `rts-primitives` (only primordials
  there: error/object/boolean/number/string). Exposed via
  `rts_runtime::stdlib::CONSOLE_TS`.
- **Read-only singleton globals reach user functions — DATA-DRIVEN, without
  naming console:** a top-level `const console = new Console()` is, by default,
  INVISIBLE inside `function f() { console.log(..) }` (a function resolves a
  free ident only via local/param, gcell #195, or a global constant). The
  solution names console NOWHERE in the front — it matches the PATTERN
  `const X = new Y()`: `funcval::singleton_instance_globals(funcs, main)`
  detects every top-level `const X = new Y()` REFERENCED from inside a
  function/arrow (the real reachability condition), promotes it to a #195 gcell
  and carries `name → class` in a `gcell_classes` map (threaded through
  `LoweredProgram`→`module_jit`→`Lowerer`, parallel to `gcells`);
  `static_instance_class` recovers the gcell's class from that map. Only those
  referenced in a function are promoted (a top-level-only singleton stays a
  normal local — promoting all would mis-dispatch
  `const p = new Point(); p.parseValue()`). `prelude_fn_names` includes the
  prelude's singletons as non-capture ambient in the user build (an arrow
  `x => console.log(x)` does not become a capture). Latent bug closed along the
  way: `collect_free_stmt` did not descend into `Try`/`Throw`/`For*`
  (incomplete free-ident scan). NO name allow-list — the data-driven equivalent
  of what @drysius did for Registry classes (`is_pure_registry_class`, never
  `if class == "Date"`).
- **`finally` with a call and a pending error (unwind fix exposed):** since
  `console.log` became a method call (several sub-calls with
  `emit_post_call_error_check`), a `finally { console.log(..) }` reached during
  unwind used to abort (the pending error slot diverted the flow before the
  print). `lower_try` now SAVES-and-clears the pending error before the
  finalizer and RESTORES it afterwards (`emit_finally_restore_propagate`) — JS
  semantics (finally runs normally; the error resurfaces after; a throw in the
  finally itself takes precedence). Direct `io.print` did not expose the bug
  (1 extern, no post-call check).

#### 3.2.4 `globalThis` — singleton global object (foundation: VALUES) — DONE

`globalThis` stopped being an "unbound identifier" bail and became a singleton
global OBJECT with dynamic get/set of VALUES (`globalThis.x = 5; globalThis.x`,
write-from-inside-a-function visible afterwards, `typeof globalThis === "object"`
— matches bun). It reuses the existing keyed object representation (Vec with
shape-id in slot 0), so `globalThis.prop` get/set routes through the same
dynamic trampolines `__rtsadp_obj_get`/`__rtsadp_obj_set` (incl. new-key append
via shape transition) — no new property machinery.

- The singleton is the codegen-owned extern `__rtsadp_globalthis()`
  (`value/globalthis.rs`): `OnceLock<u64>` creating once an empty keyed object
  (slot 0 = empty shape, same as `{}`), returning the `TAG_OBJECT` word.
- **GC**: the object lives only in the `OnceLock` (outside every stack), so it
  is registered as a permanent root via `global_roots::add` (the same pin used
  by top-level globals/N-API). The mark normalizes the NaN-boxed word and
  `Entry::Vec` tracing keeps the stored values alive.
- `globalThis` resolves as a bare identifier in `lower_ident` (after
  local/gcell, like `NaN`/`undefined`) → the singleton's word;
  `lower_member`/`lower_member_assign` detect the `globalThis` receiver
  (`is_globalthis_receiver`) and route to the dynamic path. It is a LANGUAGE
  GLOBAL (not a non-primordial class) — the engine may name it, like `NaN`.
- Process-global singleton: one `rts run` is 1 program = 1 fresh globalThis
  (correct). The unit-test harness shares the process → tests are
  self-contained (set-then-get / unique absent key) to be order-independent.

**Deferred follow-up (NOT in this increment):** `globalThis.X = class X {…}`
(class-as-value) + `new (globalThis.X)()` (dynamic construction) need
class-EXPRESSIONS in the HIR (today `: C`/`class` expr degrade) + `new` over a
dynamic value. That is what fully unlocks the `globalThis.Map = class Map`
pattern.

#### 3.2.1 Number migrated to `.ts` (same pattern as Boolean)

`number` is a PRIMITIVE (literal syntax `123`), so the VALUE stays unboxed
(double/int on the fast path) — only the METHOD LIBRARY migrated. The
irreducible numeric formatting (float→string, radix, toFixed/toPrecision/
toExponential) is DELICATE and already exists ONCE in Rust
(`rts-primitives/src/number.rs`, the `__RTS_FN_GL_NUMBER_*` externs); the `.ts`
bodies do NOT reimplement it — they call PRIVATE `engine.num_*` helpers that
wrap those formatters (one source of truth), exactly like
`engine.trace_capture()`/`engine.arch()`.

- **File:** `crates/rts-primitives/src/number.ts` (`rts_primitives::NUMBER_TS`,
  re-exported by the `rts_runtime::NUMBER_TS` facade). Included as a
  declarations-only prelude via `e.include(rts_runtime::NUMBER_TS)` in
  `registry.rs build_registry()` (after `BOOLEAN_TS`). `class Number` with only
  prototype methods (NO ctor/fields): `valueOf()` (pure body `return this`),
  `toString(radix = 10)`, `toFixed(digits = 0)`, `toPrecision(precision = -1)`,
  `toExponential(digits = -1)`, `toLocaleString()` (defers to `toString(10)` —
  no locale data in the runtime). The JS defaults live in the `.ts`
  default-params (the engine's default-param prologue fills `undefined`); the
  bodies read `this` AS THE PRIMITIVE number (boxed word →
  `coerce(Tagged, Float64)` decodes by tag).
- **Irreducible formatting bridge (`rts:engine`):** 4 new private members in
  `crates/rts-std/src/engine/mod.rs`, each `(n: F64, arg: I64) => Handle`
  (string), wrapping the corresponding Rust formatter: `num_to_string_radix`
  (`__RTS_FN_NS_ENGINE_NUM_TO_STRING_RADIX` → `__RTS_FN_GL_NUMBER_TO_STRING_RADIX`),
  `num_to_fixed`, `num_to_precision`, `num_to_exponential`. Lowering in
  `front/run/engineobj.rs` (`lower_engine_num` — marshals receiver→F64 +
  arg→I64, calls via `call_runtime`, reboxes the handle as `TAG_STR`); sigs in
  `value/abi_sig.rs`; JIT symbols in `runtime_link.rs::engine_symbols()`.
- **Routing + drained rows:** in `try_method_dispatch`, when `recv_class_of`
  proves `RecvClass::Number`, it tries
  `try_primitive_class_method(.., "Number", ..)` BEFORE `dispatch.rs::resolve_method`.
  The 4 `NUMBER_ROWS` rows (`toFixed`/`toPrecision`/`toExponential`/`toString(radix)`
  → `__RTS_FN_GL_NUMBER_*`) were DRAINED — `NUMBER_ROWS` is now `&[]` (kept
  empty: a numeric method the `.ts` class does not cover still BAILS explicitly
  via `resolve_method → None`, never a guess). A NOT-proven-numeric receiver
  (Tagged/unknown) keeps the existing dynamic/bail path.
- **Kept on purpose:** `new Number(x)` (WRAPPER, typeof === "object") still
  follows the engine's wrapper trampoline (`__rtsadp_w_number_new`), covered by
  `is_wrapper_primordial`; the Rust formatters `__RTS_FN_GL_NUMBER_*` +
  `register_number_class_spec` (frozen OLD engine on the CLI + wrapper path).
  Coercions (`${n}`/`"x" + n`/`String(n)`) stay on the runtime trampolines,
  unchanged.
- **String follows the SAME pattern (DONE — see 3.2.2).**

#### 3.2.2 String migrated to `.ts` (same pattern as Number)

`string` is a PRIMITIVE (literal syntax `""`), so the VALUE remains a `TAG_STR`
PolyValue (string-pool handle) — only the METHOD LIBRARY migrated. The
irreducible Unicode string logic (case folding, trim, UTF-16 code-unit
indexing, slice/substring by char, pad, replace) already exists ONCE in Rust
(`rts-primitives/src/string/`, the `__RTS_FN_GL_STRING_*` externs); the `.ts`
bodies do NOT reimplement it — they call PRIVATE `engine.str_*` helpers that
wrap those impls (one source of truth), exactly like `engine.num_*`.

- **File:** `crates/rts-primitives/src/string.ts` (`rts_primitives::STRING_TS`,
  re-exported by the `rts_runtime::STRING_TS` facade). Included via
  `e.include(rts_runtime::STRING_TS)` in `registry.rs build_registry()` (after
  `NUMBER_TS`). `class String` with only prototype methods (NO ctor/fields):
  `toUpperCase`/`toLowerCase` (+ `toLocale*` deferring to the plain fold),
  `trim`/`trimStart`/`trimEnd` (+ `trimLeft`/`trimRight` aliases), `charAt`,
  `charCodeAt`, `at`, `repeat`, `slice(start, end=2147483647)`,
  `substring(start, end=2147483647)` (the large default clamps to length =
  "to end"), `indexOf`, `lastIndexOf`, `includes`, `startsWith`, `endsWith`,
  `padStart(len, pad=" ")`/`padEnd(len, pad=" ")`, `concat` (fold of up to 4
  args with `""` defaults), `replace`/`replaceAll` (STRING search). The bodies
  read `this` AS THE PRIMITIVE string (boxed word → `engine.str_*` does the
  table-load to the real handle).
- **Irreducible string bridge (`rts:engine`):** 21 new private members in
  `crates/rts-std/src/engine/mod.rs`, each wrapping the corresponding
  `__RTS_FN_GL_STRING_*` (handle in/out for strings, `I64` for indices):
  `str_to_upper`, `str_to_lower`, `str_trim`, `str_trim_start`, `str_trim_end`,
  `str_char_at`, `str_char_code_at`, `str_at`, `str_repeat`, `str_slice`,
  `str_substring`, `str_index_of`, `str_last_index_of`, `str_includes`,
  `str_starts_with`, `str_ends_with`, `str_pad_start`, `str_pad_end`,
  `str_concat`, `str_replace`, `str_replace_all` (symbols
  `__RTS_FN_NS_ENGINE_STR_*`). Lowering in `front/run/engineobj.rs`: the
  bespoke `engine.*` map was GENERALIZED for this family via a small descriptor
  table (`EngineStr`/`StrArg`/`StrRet` + `engine_str_member`) —
  `lower_engine_str` marshals receiver→handle, each string arg→handle /
  number→i64, and reboxes the return (string→`TAG_STR`, number→`Int64`,
  bool→`Bool`); adding a member is one data line, no new Cranelift code.
  Sigs in `value/abi_sig.rs`; JIT symbols in `runtime_link.rs::engine_symbols()`.
  (Design decision: the full refactor to resolve `engine.*` via Registry +
  `registry_call` was NOT done, because the receiver-less path with a string
  handle + the privacy gate would add surface/risk to a private namespace; the
  descriptor table delivers the same "adding a member is data, not code" with
  low risk.)
- **Routing + drained rows:** in `try_method_dispatch`, when the receiver is
  `RecvClass::String`, it runs `try_string_regex_method` + `try_string_special`
  (regex and `split`/1-arg `slice`, which the `.ts` class does NOT cover) and
  THEN `try_primitive_class_method(.., "String", ..)` BEFORE `STRING_ROWS`. The
  migrated rows were DRAINED from `STRING_ROWS`; only `codePointAt` (composes
  surrogate pairs), `localeCompare` (locale order) and the deprecated 2-arg
  `substr` remain — the `.ts` class does not cover them, so they still resolve
  through the generic table path.
- **`split` KEPT on the engine path (not drained):** `split` returns an ARRAY;
  it stays in `try_string_special` (marshalling an array through a single
  string→string `engine` helper is not clean). `.length` on a proven string is
  NOT in the `.ts` (the engine reads it directly via `obj.rs` →
  `__rtsadp_dyn_length`; the class system has no `length` getter hook for a
  primitive, so a `.ts` getter would be dead).
- **Kept on purpose:** `new String(x)` (WRAPPER, typeof === "object") still
  follows the engine's wrapper trampoline (`__rtsadp_w_string_new`), covered by
  `is_wrapper_primordial`; `String.fromCharCode`/`fromCodePoint` + statics stay
  on the global path (`try_global_static_call` — now with an
  `is_global_static_class` gate that does NOT bail when the `.ts` class
  `String` exists, since it only has instance methods); the Rust impls
  `__RTS_FN_GL_STRING_*` + `register_string_class_spec` stay (frozen OLD engine
  on the CLI + wrapper path). The migrated `__RTS_FN_GL_STRING_*` are now
  called Rust→Rust from inside `rts-std` (linked there, not via JIT);
  `runtime_link.rs` only registers the ones the engine's lowering still emits
  directly.
- **Test harness:** `render_source_with_prelude` now composes the engine's
  embedded includes (`includes_prelude()`) BEFORE the test's prelude —
  mirroring a real compile (the embedded prelude is ALWAYS present), so
  primitive→prelude dispatch (e.g. `s.charCodeAt(i)` on a native string field)
  is available to the user's prelude classes.

### 3.3 The `ValTy` instinct

A compile-time semantic tag separate from the machine type. The redesign
**generalizes** this in the representation lattice (`repr.rs`).

### 3.4 SHA256 object cache + shared `compile_program`

`crates/rts-codegen-old/src/cache.rs` caches by `file_sha256` +
`compiler_fingerprint` (with transitive-dep invalidation). `compile_program` is
shared by JIT and AOT (`FnCtx.module = &mut dyn Module`). In the new engine the
whole-module JIT lives in `src/front/run/module_jit.rs::compile_program`; AOT
will share that same `front/run/` path when it is built.

---

## 4. The new thesis and the mappings to the crate's modules

> **Identity:** *prove-monomorphic-and-unbox where the type system can
> (preserving the winning numeric path); fall to ONE honest tagged in-value
> representation + shapes + AOT-safe data inline-caches where it can't.*

| Pillar | Crate module                                 | Replaces in the old engine |
|-------|----------------------------------------------|---------------------------|
| 1. PolyValue | `crates/rts-codegen-new/src/value.rs` | the 4 side-tables, `Entry::FloatPrim`, the `FLOAT_*` helpers |
| 2. Repr lattice | `crates/rts-codegen-new/src/repr.rs` | `ValTy` + AST-shape heuristics |
| 3. Soundness + TS trust | `repr.rs` + `front/run/lower.rs` + `guards` | scattered `TPL_COERCE_AUTO`, dead `guards.rs` |
| 4. Shapes + ICs | `shape.rs` + `ic.rs` | default `HashMap<String,i64>`, dispatch via `gc.string_eq` |
| 5. Single lowering | `front/run/lower.rs` + `front/run/module_jit.rs` | the MIR tier and the duplicated AST codegen |
| 6. Dispatch + generated ABI | `dispatch.rs` + `abi_gen.rs` | the `calls/mod.rs` switchboard, the 1113 `add_fn!` |

Each pillar has its section below, concrete and buildable.

---

## 5. Pillar 1 — PolyValue (`value.rs`): a 64-bit NaN-boxed value

> Module: `crates/rts-codegen-new/src/value.rs` (referenced by `lib.rs` as
> `value::PolyValue`; it is the crate's **Increment 1**). Note: at the time
> this doc was written, `value.rs` did not yet exist on disk — `lib.rs` already
> declares it (`pub mod value;`) and this section is the specification that
> implements it.

### 5.1 The exact bit layout

A `PolyValue` is **a single `u64`**. The idea (NaN-boxing) exploits the fact
that IEEE-754 doubles have a large space of NaN bit-patterns no "real" double
produces after canonicalization. We reserve the negative-qNaN quadrant for
boxed values; everything outside it is a genuine inline `f64`.

```text
PolyValue (u64)
═══════════════════════════════════════════════════════════════════════════════
  BOX_BASE = 0xFFF8_0000_0000_0000   ← qNaN negativo: o "espaço boxed"

  boxed   ⟺  (bits & BOX_BASE) == BOX_BASE
  inline  ⟺  caso contrário → é um f64 real (reinterpret_cast direto)

Quando boxed:
  bit  63        : 1  (sinal — parte do BOX_BASE)
  bits 62..51    : 1…1 (expoente todo-1 + bit alto da mantissa — BOX_BASE)
  bits 50..48    : TAG (3 bits)
  bits 47.. 0    : PAYLOAD (48 bits)

TAG (bits 50..48):
  0  reservado (símbolo — futuro)
  1  INT32        payload = i32 (zero-extended em 48 bits; sinal no bit 31)
  2  SINGLETON    payload = qual singleton (undefined/null/false/true/hole/empty)
  3  STR          payload = slot da HandleTable (string GC)
  4  OBJECT       payload = slot da HandleTable (objeto com shape)
  5  FUNCTION     payload = slot da HandleTable (Function)
  6  reservado (bigint — futuro)
  7  reservado
```

Rust definition (the canonical form `value.rs` exports):

```rust
/// Um valor JS de 64 bits NaN-boxed. Inline f64 OU um boxed tagueado.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PolyValue(pub u64);

pub const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
const TAG_SHIFT: u32 = 48;
const TAG_MASK:  u64 = 0x7;                       // 3 bits
const PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;  // 48 bits

#[repr(u64)]
pub enum Tag { Symbol=0, Int32=1, Singleton=2, Str=3, Object=4, Function=5, BigInt=6 }

#[repr(u64)]
pub enum Singleton { Undefined=0, Null=1, False=2, True=3, Hole=4, Empty=5 }
```

### 5.2 NaN canonicalization — why real doubles never collide

The only way a genuine `f64` could fall into the boxed space would be to be,
itself, a negative qNaN. `from_f64` canonicalizes **every** NaN to the
*positive* qNaN before storing:

```rust
impl PolyValue {
    pub fn from_f64(x: f64) -> PolyValue {
        if x.is_nan() {
            // qNaN canônico POSITIVO — fora de BOX_BASE por construção.
            PolyValue(0x7FF8_0000_0000_0000)
        } else {
            PolyValue(x.to_bits())
        }
    }

    pub fn is_boxed(self) -> bool { (self.0 & BOX_BASE) == BOX_BASE }

    pub fn as_f64(self) -> f64 {
        debug_assert!(!self.is_boxed());
        f64::from_bits(self.0)
    }
}
```

Result: real doubles (including `±Infinity`, `-0.0`, and the positive canonical
NaN) are **disjoint** from the boxed space. `NaN === NaN` remains `false` in JS
— that is `===` semantics, handled in the lowering, not in the bit-pattern.

### 5.3 box / unbox as pure Cranelift operations

All of this becomes pure IR the egraph folds:

```rust
// tag(v): extrai os 3 bits de tag (só faz sentido se boxed).
pub fn tag(self) -> u64 { (self.0 >> TAG_SHIFT) & TAG_MASK }

// box_int32(i): INT32 boxed.
pub fn box_int32(i: i32) -> PolyValue {
    PolyValue(BOX_BASE | ((Tag::Int32 as u64) << TAG_SHIFT) | (i as u32 as u64))
}

// unbox_int32(v): payload de volta em i32.
pub fn unbox_int32(self) -> i32 { (self.0 & PAYLOAD_MASK) as u32 as i32 }

// box_handle(tag, slot): STR/OBJECT/FUNCTION boxed apontando para um slot.
pub fn box_handle(tag: Tag, slot48: u64) -> PolyValue {
    PolyValue(BOX_BASE | ((tag as u64) << TAG_SHIFT) | (slot48 & PAYLOAD_MASK))
}
pub fn slot(self) -> u64 { self.0 & PAYLOAD_MASK }
```

In the lowering, each of these is a short sequence of `bitcast` / `band` /
`bor` / `icmp` / `select` — **operations Cranelift's egraph folds**. A
redundant `box(unbox(x))` disappears in the optimizer (this is exactly why
box/unbox must be pure IR, not extern calls — see pillar 5).

`typeof v` becomes **a single tag inspection**:

```text
typeof:
  !is_boxed(v)            → "number"          (f64 inline)
  tag == INT32            → "number"
  tag == SINGLETON:
      Undefined           → "undefined"
      Null                → "object"          (o famoso bug-feature de JS)
      False/True          → "boolean"
  tag == STR              → "string"
  tag == OBJECT           → "object"  (ou "function" se for callable — checa shape)
  tag == FUNCTION         → "function"
```

### 5.4 GC-safety: the payload is a *slot index*, not a pointer

This is the property that makes NaN-boxing safe for RTS's precise GC. The
`rts-engine` `HandleTable` already encodes handles this way
(`crates/rts-engine/src/heap/handles.rs:3-9`):

```text
Handle u64 do HandleTable existente:
  [63..48] generation (16 bits)
  [47.. 5] per-shard table slot (43 bits)
  [ 4.. 0] shard index (5 bits)
```

The **lower 48 bits** (slot + shard) fit **exactly** in the PolyValue's 48-bit
`PAYLOAD_MASK`. Heap references are **slot indices**, never raw pointers.
Consequence: even if the GC moves/reallocates a slot's backing store, the
PolyValue does not need to change — it carries the index, not the address.

**Required GC change (`rts-engine`):** the conservative stack scanner
(`gc/collector.rs`, which today sweeps words looking for `u64` handles) must
learn to **recognize a boxed-handle word** and extract the slot. The rule: a
word `w` on the stack is a potential root if `(w & BOX_BASE) == BOX_BASE` AND
`tag(w) ∈ {STR, OBJECT, FUNCTION}`; in that case the root is `slot(w)`. Inline
ints, inline floats and singletons are **not** roots (they reference no heap).
This is more precise than today — words that *look like* handles but are
floats stop being false positives.

### 5.5 The generation-bits decision (honestly)

The 48-bit payload carries **only the slot** (slot + shard); the existing
handle's 16 generation bits sit **above bit 48** and **do not fit** in the
boxed PolyValue. The design decision:

- **The generation is validated on the slab side**, not embedded in the
  PolyValue. When the runtime resolves an OBJECT/STR/FUNCTION PolyValue to the
  `Entry`, it accesses the slot and the slab's current generation.
- **A live PolyValue keeps the slot reachable.** The stack scanner marks the
  slot (§5.4), so the sweep does not free that slot while the PolyValue is
  alive. Therefore **a stale-generation read cannot occur for live values**:
  the slot is not recycled underneath a PolyValue that still references it.
- **WeakRef/FinalizationRegistry caveat:** weak references, by definition, do
  *not* keep the slot alive. For them the generation matters — a WeakRef must
  store the full `(slot, generation)` (64 bits, outside the PolyValue) and
  check the generation on deref. This is correct and expected: WeakRefs are the
  only place where a stale generation is semantically observable, and RTS
  already treats WeakRef as a special case (issue #217). PolyValue covers the
  strong case (the overwhelming majority); WeakRef carries the full 64-bit
  handle.

### 5.6 What this deletes

- The **4 side-tables** (`fresh_handle_set`, `optional_chain_values`,
  `var_member_call_values`, `var_vec_slot_values`) — the tag now lives *in the
  value*.
- `Entry::FloatPrim` — fractional floats fit inline in the PolyValue (they are
  inline f64) and in `PolyValue` containers.
- The `__RTS_FN_RT_FLOAT_BOX/UNBOX/EQ_AMBIG/NUM_ARITH` zoo — box/unbox are pure
  IR; equality and arithmetic operate over tags, not re-tag helpers.

### 5.7 Future GC — generational copying + weak phase (PLAN, deferred)

> **Status: deferred.** Decision (2026-06-20): the current GC (precise
> mark-sweep with Cranelift stack maps) stays as is; the generational upgrade
> will only be done **when cross-runtime is at ~90%**. This section documents
> the target so it is not lost. Until then, new features assume the current
> mark-sweep.

**The advantage that decides the design: handle indirection.** Since the
PolyValue carries a *slot index*, not a raw pointer (§5.4), **moving an object
is cheap** — the Achilles' heel of every moving GC (finding + rewriting every
pointer to the moved object) does not exist in RTS: the object moves in the
backing store, only the `slot→address` in the slab is updated, and the index in
the PolyValues **does not change**. This uniquely positions RTS for a
**generational copying** GC, which most engines pay dearly for.

**Target: generational copying (nursery).** The generational hypothesis is very
strong in JS (the overwhelming majority of objects die young — loop
temporaries, intermediate `{}`/`[]`):

- **Young gen (nursery):** *bump-allocate* (alloc = increment a pointer). Full →
  **minor GC**: copies only the survivors to the old gen; scans only the young
  + the *remembered set*. Most temps die here → never promoted → dirt-cheap
  collection. Moving = trivial (handle indirection).
- **Old gen:** mark-sweep / mark-compact of the survivors, runs **rarely**.
- **Write barrier:** records old→young refs (remembered set) so the minor GC
  does not sweep the old gen — small cost on property-writes of old objects.
- **Multi-thread:** per-thread nursery (TLAB) → lock-free alloc; fits the
  existing shard-aware `HandleTable`.
- **Stack maps** (Cranelift `UserStackMap`, already exists) for precise roots.

Gain: fast alloc (bump), cheap minor GC (the common case), rare major GC,
compaction (no fragmentation) — and the cost of moving comes for free. It is
what V8/JSC do; in RTS the expensive part is already neutralized.

**Weak phase (#217) — does NOT need the generational GC.** The current
mark-sweep already does real weak with **one new phase between mark and
sweep**, and it can be done before the generational upgrade:

1. Normal mark — but does **not** mark through `WeakMap`/`WeakSet` keys nor the
   `WeakRef` target (they remain "candidates to die").
2. **Weak phase** (post-mark, pre-sweep): for each `WeakMap`/`WeakSet`/`WeakRef`,
   if the target **was not marked** → remove the entry / `deref`→`undefined`;
   `FinalizationRegistry` → enqueue the callback.
3. Normal sweep.

Handle indirection helps again: `WeakRef` stores the full `(slot, generation)`
(§5.5); if the slot was freed/reused (generation bumped), `deref` returns
`undefined` in O(1), no scan. **Until the weak phase exists, `WeakMap`/`WeakSet`
are a strong-ref `.ts` stdlib** (see §10.x doctrine) — functionally equal to
the Rust v0 stubs, no automatic collection; the `.ts` covers arbitrary
key/value types.

---

## 6. Pillar 2 — Representation lattice (`repr.rs`)

> Module: `crates/rts-codegen-new/src/repr.rs` (already exists, skeleton ready).

### 6.1 The enum and the join rule

```rust
pub enum Repr {
    Int32,            // i32 desembrulhado num registrador i64 (small-int fast path)
    Float64,          // f64 desembrulhado num registrador f64 (o caminho vencedor)
    Bool,             // 0/1 desembrulhado
    Ref(RefKind),     // handle GC de kind estaticamente conhecido (ainda slot, não ptr)
    Tagged,           // desconhecido / união / any → o PolyValue uniforme
}
pub enum RefKind { Str, Object, Array, Function, Registered }
```

The central rule, **total and decidable**:

```rust
pub fn join(self, other: Repr) -> Repr {
    if self == other { self } else { Repr::Tagged }
}
```

All soundness derives from this: **every `ir::Value` has exactly ONE `Repr`**.
Where two arms disagree, the representation **widens to `Tagged`** (the
PolyValue). box/unbox are **explicit IR nodes** inserted at the proven
boundaries — a **TOTAL function of the IR**, in direct contrast to the old
engine's side-table approach, where the tag was "tracked elsewhere" and could
desynchronize.

### 6.2 Where values stay unboxed

A value is kept `Int32`/`Float64`/`Bool`/`Ref` (in a register, no box)
**only** where the front-end **PROVES** monomorphism, from:

- **literals** (`42` → `Int32`, `3.14` → `Float64`, `true` → `Bool`, `"x"` →
  `Ref(Str)`);
- **TS annotations validated at untrusted boundaries** (see pillar 3);
- **local flow** (the result of proven-numeric arithmetic stays numeric).

### 6.3 Totality at the hard points (nothing leaks into "tracked elsewhere")

The join rule must be honest precisely where the old engine cheated with
side-tables. For each hard point, the representation is decided *in the IR*:

- **Loop-header phis:** the phi's `Repr` is the join of *all* predecessors
  (entry + back-edge). If the back-edge can produce `Tagged`, the phi is
  `Tagged` from the start of the loop — there is no "late promotion". This
  requires a cheap fixpoint over the loop's CFG before choosing the header's
  representation (one pass; the lattice has height 2, so it converges
  immediately).
- **Exceptions bound in catch:** the `catch (e)` binding is always `Tagged` (an
  exception can be any value). No exception.
- **Destructuring bindings:** each binding gets the `Repr` of the source
  element/property; if the source is a `PolyValue` container, the binding is
  `Tagged` (unless proven monomorphic by a validated annotation).
- **Closure-captured vars:** the environment record stores `PolyValue`
  (`Tagged`) by default; it only unboxes if *every* read AND write of the
  captured var agrees on a monomorphic `Repr` (capture analysis). The
  conservative default is `Tagged` — correct, never silently wrong.
- **Generator state:** the generator's state machine persists `PolyValue` in
  its slots (`Tagged`); values are unboxed *after* resume, at the point of use,
  if proven.

In **none** of these cases is there a "this value is secretly not-a-pure-int"
stored in a `HashSet`. The representation is a property of the IR, period.

---

## 7. Pillar 3 — Soundness rule + trust in TS types

### 7.1 The problem: TS types are unsound

A `: number` parameter **can** receive a string coming from untyped JS or from
an `any`. Blindly trusting the annotation and unboxing as `Float64` produces
exactly the old engine's silent bug, in different clothes.

### 7.2 The single rule

> **Unbox based on a representation PROVED at the point; and INSERT runtime
> checks at the untrusted boundaries.**

Untrusted boundaries (where a tag check is inserted before unboxing):

- parameters of **exported functions** / public entry points;
- values of type **`any`**;
- results of **`JSON.parse`**;
- results of **external / Registry-resolved calls** (the boundary returns
  `PolyValue`);
- any boundary where the compiler cannot prove the producer.

Inside a proven region (result of numeric arithmetic, literal, local var with
monomorphic flow), **no check** — that is the winning fast path.

### 7.3 The polymorphic `+` — no guessing by AST shape

`a + b`:

- **both proven number** (`Int32`/`Float64`, or both unboxable without a check)
  → native `iadd`/`fadd`. **Fast path, zero cost.**
- **otherwise** → **ONE** `ADD_GENERIC(PolyValue, PolyValue) -> PolyValue` that
  runs the **real JS `+` algorithm** (`ToPrimitive` on both; if either becomes
  a string → concatenation; else `ToNumber` + addition). **Never** guessing by
  AST shape (`is_map_get_call` and friends die here).
- **inline fast path for the secretly-monomorphic case:** before calling
  `ADD_GENERIC`, the lowering emits a cheap inline tag check — if both
  PolyValues are INT32/inline float, it adds right away; it only falls into the
  generic if the tags disagree. This recovers performance when the static type
  failed to prove but the runtime is, in fact, numeric.

The same pattern (inline fast-path + honest generic) applies to `===`, `<`,
`==`, etc.

### 7.4 `guards.rs` becomes real (or is replaced)

The **single** coercion authority comes to actually exist. `guard_for`
(`crates/rts-engine/src/abi/guards.rs`) is either promoted to the real path
(all coercion/check insertions go through it) or replaced by an equivalent
module in `rts-codegen-new`. What must **not** continue: `guard_for` as dead
code with `TPL_COERCE_AUTO` scattered across 16 files doing the real coercion
ad-hoc. **One authority, one place.**

### 7.5 An annotation NEVER demotes a value — the Float64→Int corollary

The rule of §7.2 ("unbox based on a PROVED representation") has a corollary in
the opposite direction, equally binding: **the annotation/inferred type may
never DEMOTE a value's proven representation.** In JS `number` is ONE type
(IEEE-754 double); `/` ALWAYS produces a real (`44100 / 48000 === 0.91875`, not
`0`). When a `const`/`let` init is proven `Float64` (result of `/`, of float
arithmetic, etc.) but the HIR inferred the binding's type as `Int*` (because
the operands were integer literals), `lower_let` must **not** widen the local
to the `Int*` annotation: that would coerce the real `Float64` into an integer
slot, truncating the fraction. The widening rule (`stmt.rs::lower_let`) only
promotes to the unboxed annotation when it does NOT demote the value
(`!demotes_float`); a `Float64` under an `Int*` annotation keeps `Float64`.
Historical bug this closes: `const ratio = 44100/48000` became `0`, and an
audio resampler read frame 0 forever (silence). It affects any
`const x = <float expr>` initialized from integers, not just audio.

---

## 8. Pillar 4 — Shapes (`shape.rs`) + data inline caches (`ic.rs`)

> Modules: `crates/rts-codegen-new/src/shape.rs` + `crates/rts-codegen-new/src/ic.rs`
> (skeletons ready).

### 8.1 Hidden classes (shapes)

An object is `{ shape_id, slots: [PolyValue; N] }`. The `Shape` is the layout
shared by all objects constructed the same way:

```rust
pub struct Shape {
    pub id: ShapeId,
    pub slots: HashMap<String, SlotIdx>,        // nome → índice de slot inline
    pub transitions: HashMap<String, ShapeId>,  // árvore de transição (add-property)
    pub proto: Option<ShapeId>,                 // shape do prototype (proto ICs)
}
```

- **Property access** = compare `shape_id` + load at a fixed offset
  (`slots[slot_of(shape, key)]`). Not a hash lookup.
- **Object construction** walks the **transition tree**: `{}` → add `"x"` →
  add `"y"` produces a deterministic chain of shapes; two objects with the same
  key sequence **share** the same final shape.
- **Method dispatch** is keyed on the *shape's class*, not on a chain of
  `gc.string_eq`.

This replaces the **default** `HashMap<String,i64>` (§2.3). The flat layout
becomes the **default**, not gated behind an env-var.

### 8.2 Data inline caches — AOT-safe, no self-modifying code

Classic V8 ICs **patch machine code** at runtime. With Cranelift AOT
object-files you cannot self-modify code portably. So an RTS IC is a **data
cell** that the emitted code loads and checks:

```text
Site de acesso  obj.x  com uma PropIcCell adjacente (segmento de dado gravável):

    sid = load obj.shape_id
    if  sid == cell.shape           ; um icmp num u32 carregado
        v = load obj.slots[cell.slot]   ; fast path: offset fixo
    else
        v = slow_path(obj, "x", &cell)  ; resolve via shape, ATUALIZA a cell
```

```rust
#[repr(C)]
pub struct PropIcCell {
    pub shape: ShapeId,   // shape esperado
    pub slot:  SlotIdx,   // offset do slot
    pub state: u32,       // discriminante de IcState, mutado pelo slow path
}
```

The guard is an `icmp` over a loaded `u32`; the cell lives in a writable data
segment → **works identically for JIT and AOT**. It is the simplification that
keeps the engine lean while turning megamorphic string lookup into a
*pointer-compare*.

### 8.3 The IC state machine

```text
Uninit ──(1ª shape vista)──▶ Mono{shape,slot}
   │                              │
   │                              ├─(mesma shape)──▶ fast path
   │                              └─(shape nova)───▶ Poly (tabela inline pequena, K shapes)
                                                        │
                                                        └─(K excedido)──▶ Mega (sempre chama o resolver genérico)
```

`uninit → mono → poly → mega`. It replaces **both**: the hashmap property-bag
(default) and the O(N) string-comparison dispatch.

### 8.4 Dictionary mode only for pathological cases

It falls back to a dictionary (`HashMap`) **only** for pathological objects:
mass computed keys, giant maps, frequent `delete`. The common path never
touches a hashmap.

### 8.5 The simplicity line (deliberately kept)

Shapes + mono/poly/mega data ICs + transition tree + dictionary fallback.
**AND NOTHING ELSE.** Explicitly **out of scope**:

- **NO** speculative deopt.
- **NO** on-stack replacement (OSR).
- **NO** dependent-code invalidation graph.
- **NO** hidden-class deprecation.

These are the V8 complexity sources RTS chooses *not* to pay for (§11).

---

## 9. Pillar 5 — Single lowering path (`front/run/`), no MIR

> Modules: `crates/rts-codegen-new/src/front/run/lower.rs`
> (`Lowerer::lower_function`, the HIR → Cranelift lowering) +
> `crates/rts-codegen-new/src/front/run/module_jit.rs`
> (`compile_program`, the whole-module JIT).

### 9.1 One path, not two

`HIR → Cranelift IR`, direct. Cranelift's egraph (`use_egraphs=true`) is the
**ONLY** optimizer. The old engine had **two complete codegens** (the "AST
authoritative" one and the `HIR→MIR→Cranelift` one that re-did the egraph and
fell back to the AST for ~99% of real JS). Here there is **one**.

### 9.2 The front-end's exact job in the IR

The front-end's only responsibility is what Cranelift **genuinely cannot do**
(JS semantics), and nothing beyond that:

- **JS-semantic coercions:** `ToNumber` / `ToString` / `ToBoolean`;
- **resolution of the polymorphic `+`** (pillar 3);
- **box/unbox insertion** (pillar 1) — as pure IR, for the egraph to fold;
- **emission of the shape/IC sites** (pillar 4);
- **narrow-int wrap semantics** (i8/u8/i16/u16 — what `narrow.rs` used to do);
- **exception edges** (try/catch edges).

**Everything else is delegated to Cranelift's egraph:** const-fold, CSE, DCE,
FMA, strength reduction, intraprocedural inlining. This deletes the redundant
MIR tier and the duplicated AST codegen (~3000 LOC).

### 9.3 Why box/unbox must be pure IR (not extern calls)

If `box`/`unbox` were extern calls, the egraph could not see through them and a
redundant `box(unbox(x))` would survive. Since they are pure `bitcast`/`band`/
`bor`/`select` (pillar 1, §5.3), the egraph **folds the redundant pair** — the
PolyValue cost vanishes exactly in the places where the representation was
already monomorphic in fact. This is the technical reason pillar 1 and pillar 5
are coupled.

---

## 10. Pillar 6 — Data-driven dispatch (`dispatch.rs`) + generated ABI (`abi_gen.rs`)

> Modules: `crates/rts-codegen-new/src/dispatch.rs` + `crates/rts-codegen-new/src/abi_gen.rs`.

### 10.1 Every non-primordial method is a `MethodSpec`

The engine directly names ONLY primordials. Everything else (`Map`/`Set`/`Date`/
`RegExp`/`console`/`JSON`/`Math`/…) is `MethodSpec` metadata (name, arity/
overloads, argument coercions, symbol, optional intrinsic) resolved through
**ONE** generic path:

```rust
pub enum Target {
    Intrinsic(&'static str),  // inline como IR Cranelift nativa (spec marca intrínseco)
    Extern(&'static str),     // emit de um `call` typed ao símbolo extern (caminho genérico)
    ShapeMethod,              // dispatch via shape/IC (método de objeto de usuário)
}

pub fn resolve_method(recv_kind: &str, method: &str, argc: usize) -> Option<Target> {
    // dirigido inteiramente por SPECS / GLOBAL_CLASS_SPECS — zero special-case por método
}
```

Dispatch over a `PolyValue`: read the tag → heap kind → the kind's method table
(primordial: direct; registered: Registry lookup) → emit. Intrinsics
(`sqrt`/`abs`/`min`/`max`) still inline as Cranelift IR when the spec marks it.

This deletes the 4622-LOC `calls/mod.rs` switchboard: the 5×-duplicated
`JSON.stringify`, the 2× `Math.max`, the hardcoded `console.*` lists — all
become a metadata entry.

### 10.2 Generated ABI — killing the link-OK/SIGILL class

The **1113** manual `add_fn!` of `jit.rs` are **DERIVED** from the same `SPECS`
the codegen reads:

```rust
pub struct SymbolEntry { pub name: &'static str, pub ptr: *const u8 }

pub fn jit_symbols() -> Vec<SymbolEntry> {
    // itera SPECS, emite (símbolo, fn_ptr); ASSERT de cobertura em tempo de build:
    // todo símbolo referenciado pelo codegen existe com assinatura lowered casada.
}
```

The **build-time coverage assertion** verifies that every symbol the codegen
references exists **with a matching lowered signature**. A rename that
previously produced *link OK + runtime SIGILL* now **fails the build** — the
entire bug class dies.

### 10.3 The typical extern "C" boundary survives

The monomorphic path keeps crossing the boundary with **typed extern "C"
primitives** (`AbiType`, §3.1) — intact. PolyValues cross the boundary with a
**tagged-in/tagged-out convention** for the generic runtime calls. The two
coexist: monomorphic pays the typed fast path; generic pays the tagged one.
Intrinsic inlining coexists with both.

### 10.4 `AbiType::Handle`: HEAP handle vs opaque RESOURCE handle

An `AbiType::Handle` at the boundary has TWO natures the marshal must
distinguish (`front/run/registry_call.rs`):

- **GC HEAP handle** (`gc.string_*`, `string.*` → string): the value is a
  string/object from the `HandleTable`. On return it reboxes as
  `TAG_STR`/`TAG_OBJECT` PolyValue; on an arg it does `emit_table_load` to
  recover the real handle.
- **opaque RESOURCE handle** (`audio.*`, `buffer.alloc`, `net.*`, `thread.*`):
  the value is a RAW `u64` from the namespace's own table (1,2,3…), whose TS
  type is `number` — it is NOT a GC handle. It reboxes as an INTEGER PolyValue
  (not object), and on an arg passes the integer VERBATIM. Boxing the raw id as
  `TAG_OBJECT` would make a later `emit_table_load` dereference a nonexistent
  slot → SIGILL (historical bug: `audio.open_output()` → `sample_rate()`).

The distinction is DATA, derived from the member's `ts_signature`: return
`: string` ⇒ heap handle (`ResolvedCall.ret_is_string_handle = true`); else
resource handle. `lower_builtin_call` passes `JsKind::Str` vs `JsKind::Number`
to the rebox according to that bit. No new ABI metadata — the TS signature
already carries the information.

---

## 11. Why this is simpler than V8 (and the honest cost)

### 11.1 V8 complexity that RTS legitimately SKIPS

- **No bytecode interpreter / Ignition.** RTS compiles straight to native;
  there is no interpreted tier.
- **No TurboFan speculative tier.** There is no speculative recompilation based
  on collected type feedback.
- **No deopt / OSR.** Since there is no speculation, there is no
  de-optimization nor on-stack replacement to exit speculated code that failed
  its assumption.
- **No inline-cache code patching.** RTS ICs are *data* (§8.2), not
  self-modifying code.
- **No hidden-class deprecation / dependent-code graph.** V8 maintains a graph
  of which compiled functions depend on which hidden classes to invalidate them
  when a class is deprecated. RTS has no such graph: data ICs simply re-resolve
  on the site's next execution.

### 11.2 The honest cost (stated bluntly)

- **Polymorphic/megamorphic code is slower than V8.** Without the speculative
  tier, a truly megamorphic site pays the generic resolver every time (the IC
  becomes `Mega`).
- **Hot unannotated JS pays a tag-check.** Where the static type does not prove
  monomorphism, the inline fast-path (§7.3) still costs one tag `icmp` per
  operation. V8 would elide it after speculative warmup; RTS does not.
- **First execution has no speculative warmup.** There is no feedback
  collection that improves the code on the 2nd pass beyond filling the data
  ICs.

**The trade is deliberate:** RTS trades V8's speculative peak for an engine
**orders of magnitude smaller and sound by construction**, keeping the
monomorphic numeric path ~5× Bun (which is RTS's target use case: typed TS
compiled to native). For heavy dynamic JS, RTS will be slower than V8 — and
that is acceptable and expected.

---

## 12. Strangler-fig migration plan

`rts-codegen-old` stays plugged into the `bin`/`cli`. `rts-codegen-new` is
built **behind** it, phase by phase, highest-leverage-first. **The honesty/build
floor never loosens; the parity number stays real** (no fixture deleted/
disabled/hardcoded to inflate the metric; nothing that crashes/hangs committed
as "pass"; the build always compiles).

| Phase | Deliverable | Done criterion | Regression guard |
|------|---------|--------------------|--------------------|
| **P0** | `value.rs` (PolyValue) — **done in Increment 1** | pure model + Cranelift JIT roundtrip exhaustively tested | `value.rs` unit tests green |
| **P1** | Delete the MIR mental model + lower **one** numeric fn through the new path (HIR→Cranelift direct) | one numeric fn runs end-to-end via `front/run/` (`lower.rs` + `module_jit.rs`) producing the same result as the old one | numeric A/B fixture against the old engine |
| **P2** | PolyValue containers replacing `i64`+`FloatPrim` | `Map`/`Vec` store `PolyValue`; `Entry::FloatPrim` removable | heterogeneous-container suite (fractional float in Map/Vec) green |
| **P3** | Shapes + ICs for objects | default object uses shape + data IC; `HashMap` only pathological | object/property-access + dispatch suite green, no `gc.string_eq` per override |
| **P4** | Data-driven dispatch + `abi_gen` | `resolve_method` drives everything via SPECS; symbols derived with coverage assert | coverage assert passes; no manual `add_fn!` remaining |
| **P5** | Cutover | rename `rts-codegen-new` → `rts-codegen`; retire `rts-codegen-old` | full TS suite + cross-runtime parity ≥ the `v0.0-202606072107` tag's, real number |

Each phase runs the suite incrementally (not only at the end). Regression is
allowed only if **explicit and justified** in the commit/PR; silent regression
blocks merge.

---

## 13. What gets deleted from the old engine (by name)

Canonical list of what **goes** when the cutover (P5) happens:

1. The **4 side-tables** in `ctx.rs`: `fresh_handle_set`, `optional_chain_values`,
   `var_member_call_values`, `var_vec_slot_values` (replaced by the tag in the
   PolyValue + the `Repr` lattice).
2. `Entry::FloatPrim` in `rts-engine/src/heap/handles.rs` (floats fit inline in
   the PolyValue).
3. The runtime re-tag helpers `__RTS_FN_RT_FLOAT_BOX` / `_UNBOX` /
   `_EQ_AMBIG` / `_NUM_ARITH` (box/unbox are pure IR).
4. The **entire MIR tier** — the codegen's use of the `rts-mir` crate (84-inst
   SSA + `fold`/`fma`/`cse`/`dce`/`narrow`/`inline` passes), which re-did
   Cranelift's egraph.
5. The **duplicated AST codegen** — the second complete path that existed only
   as the MIR's fallback.
6. `guards.rs::guard_for` **as dead code** — it becomes the real coercion
   authority (pillar 3) or is replaced; what does not survive is the current
   state (defined, zero production call sites, ad-hoc `TPL_COERCE_AUTO` doing
   the work).
7. The **1113 manual `add_fn!`** of `jit.rs` (derived from SPECS by `abi_gen`).
8. The **objects with `HashMap<String,i64>` as default** (replaced by shapes +
   inline slots; hashmap only pathological).
9. The **string-comparison dispatch** (O(N) `gc.string_eq` per override,
   replaced by shape-id + IC).
10. The **switchboard duplications** in `calls/mod.rs`: the 5×-duplicated
    `JSON.stringify`, the 2× `Math.max`, hardcoded `console.*` lists (they
    become `MethodSpec` metadata).
11. The **AST-shape heuristics** (`is_map_get_call` and similar) — the coercion
    decision becomes proven-`Repr` / tag-check based, never inspection of the
    tree's shape.

---

## 14. Appendix — crate module map

`crates/rts-codegen-new/src/`:

State: **all modules below are implemented** — there is no `todo!` left in the
crate (the redesign's skeleton ladder has been surpassed; this is the real
engine).

| File/directory | Role | Pillar | State |
|-------------------|-------|-------|--------|
| `lib.rs` | manifest + module reexports | — | ready |
| `value/` | 64-bit NaN-boxed `PolyValue` (`mod.rs` + ops + `abi_adapter`) | 1 | implemented |
| `repr.rs` | `Repr` lattice + `RefKind` + `join` | 2 | implemented |
| `shape.rs` | `Shape` / `ShapeTable` / transition tree | 4 | implemented |
| `ic.rs` | `IcState` / `PropIcCell` (data IC) | 4 | implemented |
| `dispatch.rs` | data-driven `Target` / `resolve_method` | 6 | implemented |
| `abi_gen.rs` | `SymbolEntry` / `jit_symbols` derived from SPECS | 6 | implemented |
| `runtime_link.rs` / `registry_link.rs` | runtime + Registry surface for the JIT | 6 | implemented |
| `front/hir_lower/` | numeric lowering of a `HirFunc` (prove-monomorphic subset) | 5 | implemented |
| `front/run/lower.rs` | `Lowerer::lower_function` — HIR → Cranelift, single path | 5 | implemented |
| `front/run/module_jit.rs` | `compile_program` — whole-module JIT (symbols via `abi_gen`) | 5 | implemented |
| `front/run/{expr,stmt,call,binop,assign,loops,...}.rs` | lowering per expression/statement construct | 5 | implemented |
| `front/run/class/` | classes (constructor/method/`this`/`extends`/static/getters) via shapes | 4 | implemented |
| `front/run/{method,method_array,method_dyn,obj,objstatic}.rs` | method dispatch and property access (shape-keyed) | 4 | implemented |
| `front/run/registry.rs` / `registry_call.rs` | Registry construction and generic marshal via `AbiType` | 6 | implemented |
| `front/run/desugar/` | template literals, optional chaining, destructuring | 5 | implemented |

> AOT on the new engine has not been built yet; when it is, it will share the
> `front/run/` path (there is no separate `pipeline.rs` anymore — it was
> deleted).

## 15. Module system (new engine)

The old engine flattened the module graph in `crates/rts-codegen-old/src/module/`
(BFS, dedup by canonical path, 3-color cycle detection, `flatten_for_jit`).
The new engine **neither extends nor extracts** that code: it reimplements a
clean subsystem in `crates/rts-codegen-new/src/front/modules/` (`mod.rs` +
`resolve.rs` + `graph.rs` + `flatten.rs` + `error.rs`, each < 500 lines),
depending on no old-engine crate and reading the runtime only through the
`rts-runtime` facade.

### Decision: resolve fresh, don't extract

The old `module/` carries features out of scope for the new engine (manifest
walking, workspace packages, node_modules, remote http(s) imports, `.ometa`
cache, `export` line-scan) coupled to its `CompileOptions`/diagnostics. Reusing
the **structure** (BFS + dedup + 3-color cycle + DFS post-order) costs
~250 lines; dragging the whole crate in would reintroduce the coupling the
redesign exists to cut. Moreover the export set now comes from a real AST flag
(`exported: bool` on `FunctionDecl`/`ClassDecl`, set in the parser at
`ModuleDecl::ExportDecl`), not from a fragile line-scan — a soundness
improvement, not a copy.

### M1 / M2 / M3 sequencing

- **M1 — ES + relative-filesystem + builtin branch — DONE (user path).**
  ES2015 `import`/`export`, relative specifiers (`./x`, `../x`) with the
  candidate list `x.ts, x.rts, x.js, x/index.{ts,rts,js}`, and the builtin
  branch (`rts`, `rts:<ns>`, `node:<ns>`) resolved **without touching disk**.
  - **M1a — DONE.** Pure resolver/graph + tests (`front/modules/`):
    `load_program(entry) -> ResolvedProgram { program, bindings }`.
  - **M1b — DONE.** The `ResolvedProgram` runs end-to-end through the lowering.
    Public entry `front::run::run_path(&Path)` (+ `render_path` for capture in
    tests): `load_program` → `apply_bindings` → `build_from_program` (USER
    side, NOT prelude) → `merge_programs(includes_prelude, user)` →
    `compile_program` → JIT → run. **CLI wiring:** `rts run-new <file>` calls
    `run_path` (resolves relative imports from the entry's directory); the
    eval/`-e` path stays in `run_source` (string, no disk imports).
    - **`Binding::Local` (user module) — wired.** An `import { a as b }` is
      remapped: `apply_bindings` (in `front/run/module_entry.rs`) renames every
      identifier REFERENCE `b` to the exported name `a` via a focused
      `swc_ecma_visit::VisitMut` over each `Stmt`/function body/class
      initializer of the flattened program — only `Ident` nodes (member-access
      props and object keys are `IdentName`/`PropName`, untouched). A plain
      `import { a }` (local == orig) is a no-op. Known limitation: the binding
      map is global/flattened, so a local homonymous with an alias is renamed
      too (fine in the common case of distinct names).
    - **`Binding::Builtin` (`rts:<ns>`) — DONE (dispatch wired).** An import
      `import { member } from "rts:<ns>"` now CALLS the real namespace
      function. Mechanism (choice: **Registry-register**, not `abi::lookup` —
      no `abi::SPECS`/`abi::lookup` is reachable through the facade; the `abi`
      only re-exports the type vocabulary): `front/run/registry.rs::build_registry`
      registers the public namespaces (`ns::io::register`, `ns::math::register`)
      on the same `Engine` that already builds the Registry classes;
      `namespace_member(ns, member, argc) -> Option<ResolvedCall>` resolves the
      real `__RTS_FN_NS_*` + its `Sig` (`AbiType`s) via
      `registry().module("rts:<ns>")`. The `local → (ns, member)` binding is
      threaded `LoweredProgram.builtins → Lowerer.builtins`; the call glue is
      in `front/run/call.rs` (`lower_call`, `Ident` branch →
      `lower_builtin_call`), marshalling through the SAME `emit_registry_call`
      as the classes (`recv = None` — a namespace function has no `this`). JIT
      symbols installed in `runtime_link::jit_symbols` (io print/eprint already
      there; math's sqrt/abs/floor added). **Wired namespaces:** `rts:io`,
      `rts:math`. **Bare `"rts"` (`ns == ""`):** imports a namespace OBJECT
      (`io`), not a member — `namespace_member` returns `None` and the call
      bails honestly (gap: namespace-object import + `io.print` access is
      future work). An unknown member or a builtin used as a VALUE (not called)
      also bail explicitly. **`import * as ns` remains a gap** (dropped in the
      parser — M1a; no binding reaches the resolver). Coverage:
      `front/run/tests/builtin_import.rs`.
- **M2 — CommonJS.** `module.exports` / `exports.name` / `require(...)`
  requires parser work (today it does not lower those forms); next step.
- **M3 — incremental `.ometa` cache.** Transitive dependency hash to invalidate
  AOT objects when any imported module changes (port of the old
  `transitive_deps_hash`), reintroduced only when the new engine's AOT exists.

### `ResolvedProgram` / `Binding` contract (what M1b consumes)

`pub fn load_program(entry: &Path) -> FrontResult<ResolvedProgram>` returns:

```text
ResolvedProgram {
    program:  rts_ast::ast::Program,          // todos os módulos concatenados em
                                              // ordem de dependência (DFS post-order),
                                              // SEM nenhum Item::Import/ExportNamespace
    bindings: HashMap<String, Binding>,       // nome LOCAL importado -> destino
}

enum Binding {
    Builtin { ns: String, member: String },   // `import {print} from "rts:io"`
                                              //   -> Builtin{ ns:"io", member:"print" }
                                              // `rts` puro -> ns vazio; member = nome
    Local   { name: String },                 // nome exportado por outro módulo
                                              //   USER, visível no programa plano
}
```

M1b, when lowering an identifier, consults `bindings`: a `Builtin` resolves the
real `__RTS_FN_*` symbol via the Registry (Pillar 6 — `front/run/registry.rs`);
a `Local` is the top-level name already present in the flattened `program`. The
resolution of the **builtin member** is deferred to M1b on purpose: M1a only
records the intent (ns + member from the `import` itself), without enumerating
SPECS — a namespace's member set is verified at dispatch, not at module
resolution.

### Explicit-error posture (no silent miscompilation)

Every failure mode is an EXPLICIT error, never a silent drop/last-wins —
exactly the redesign's honesty floor:

- import cycle (`a → b → a`) → error with the cycle's path;
- importing a name the module does NOT export → error naming the symbol;
- top-level name collision between two USER modules in the flat program →
  error (no last-wins);
- pure npm/workspace specifier (out of M1 scope) → honest `Unsupported`;
- `export * as ns from "./mod"` (the parser today drops the `import * as ns`)
  → honest `Unsupported`, recorded as a gap, not silenced.

`ModuleError` (internal, structured, testable per variant) converts to
`Unsupported` at the public boundary via `From`, keeping the subsystem in the
same `FrontResult` language as the rest of the front-end.

> Known gap (not fixed in M1a): `import * as ns from "..."` is dropped in the
> parser (`crates/rts-parser/src/lowering_items.rs`, `ImportSpecifier::Namespace`);
> the resolver cannot see that binding. Fixing the parser is M1b/M2 work.

Relevant external cross-references:

- `crates/rts-engine/src/heap/handles.rs` — `[gen|slot|shard]` handle layout
  the PolyValue's 48-bit payload reuses (§5.4/§5.5); the GC change point.
- `crates/rts-engine/src/abi/guards.rs` — `guard_for` (pillar 3), dead today.
- `crates/rts-codegen-old/` — frozen engine; source of the LOC citations and
  the deleted items (§13).
- `CLAUDE.md` + `.claude/rules/` — the PRIMORDIAL-vs-Registry doctrine (§3.2)
  and the honesty/build floor (§12), which this redesign respects in full.
