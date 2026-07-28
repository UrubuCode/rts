# Future optimization — closing the gap to native Rust

Status: **Phase 0 LANDED and measured (2026-07-26). Phases 1–6 are plan.**
Written 2026-07-25.

This document explains where the remaining performance gap between RTS and
hand-written native Rust actually lives, and the ordered plan to close it. It is
a companion to [`rts-codegen-new-design.md`](rts-codegen-new-design.md) — that
doc defines the *model* (PolyValue, Repr lattice, shapes + data ICs, single
lowering); this one defines the *optimization work that model enables but that
we have not built yet*.

---

## 0. The premise: the gap is not Cranelift

The reflex diagnosis for "RTS is 10–50× slower than the equivalent Rust" is "the
backend is weak". That diagnosis is wrong and following it wastes months.

Cranelift vs LLVM on straight-line scalar code is roughly **1.1–1.5×**. It does
not produce a 10× gap. Our own scoreboard proves the point from the other
direction: Monte Carlo 10M runs **26.8 ms JIT / 16.9 ms AOT** against Bun's
91.8 ms. On that benchmark the whole hot loop is proven-monomorphic `f64` with no
heap traffic, and RTS is already at machine speed.

So the operating principle:

> **Where the representation is proven and the heap is untouched, we are already
> at Rust speed. The gap is exactly the code that leaves that region.**

Two axes follow, and every phase below serves one of them:

- **Axis A — widen the proven region.** Every value that falls to `Repr::Tagged`
  pays box/unbox plus a tag check on each use.
- **Axis B — remove heap traffic inside it.** Every object allocation pays a slab
  slot, a shard, an indirection per field access, and a GC tick every 256 allocs.
  Rust pays none of these — its equivalent object is a stack struct or an element
  of a contiguous `Vec`.

### Where the multiples actually come from

| Cause | Typical cost | Axis |
|---|---|---|
| Conservative `Tagged` on loop-header phis / captures / catch bindings | 5–20× in a hot loop | A |
| Per-object HandleTable allocation + indirection + GC tick | 10–30× in allocating code | B |
| Non-inlinable `call` to an extern runtime helper (full optimization barrier) | 2–10× | B |
| String ops that allocate where Rust borrows | 3–10× | B |
| No loop auto-vectorization (Cranelift has none) | 4–8× on numeric kernels | ceiling |
| Cranelift vs LLVM codegen quality | 1.1–1.5× | ceiling |

Only the last two are inherent. The first four are ours to remove.

---

## Phase 0 — instrument the cause, not the symptom ✅ LANDED

**Implemented in `crates/rts-codegen-new/src/stats.rs`, opt-in via
`RTS_REPR_STATS=1`.**

```bash
RTS_REPR_STATS=1 target/release/rts.exe run bench/objbench.ts
```

Four event kinds are recorded, each with the ENGINE `file:line` that made the
decision (via `#[track_caller]` — no edits at the ~165 call sites), the user
function, and whether the lowering was inside a loop:

| Event | Hook | What it costs |
|---|---|---|
| `BOX` | `Lowerer::box_value` | native → `PolyValue` widening |
| `UNBOX` | `Lowerer::coerce` (Tagged→native arms) | tag select + decode |
| `TAGGED-BINDING` | param bind, `let` bind, `bind_tagged_local`, catch bind, for-of bind | every use pays a tag check |
| `RUNTIME-CALL` | `value::emit_marshal::emit_call{,_sig}` | full optimization barrier; allocation sites derived by symbol name |

The prelude (~5k `.ts` lines) is counted separately and reported as one summary
line, so it cannot drown the user program in the histogram. The dump fires at the
end of `module_jit::populate_module`, which both the JIT and AOT backends share.
Zero cost and zero behavior change when the env var is unset.

### First measurement (2026-07-26)

Two benchmarks, same engine, opposite ends of the proven region.

**`bench/objbench.ts`** — 3M iterations of `const p = new P(i, i+1); s += p.x*p.y`:

| | time | vs Rust |
|---|---|---|
| native Rust (`rustc -O`) | 2.01 ms | 1× |
| Bun | 26.65 ms | 13× |
| **RTS** | **2058 ms** | **1024×** |

The histogram names every part of that 1024×, per loop iteration:

```
RUNTIME-CALL           1389           17     <- 17 extern calls per iteration
BOX                      11            1
UNBOX                     1            1
TAGGED-BINDING            5            1

-- RUNTIME CALLS INSIDE loops --
   5  __RTS_FN_NS_GC_POLY_TO_HANDLE     run
   3  __RTS_FN_NS_COLLECTIONS_VEC_GET   run
   2  __RTS_FN_NS_COLLECTIONS_VEC_SET   run
   1  __RTS_FN_NS_COLLECTIONS_VEC_NEW   run    <- the allocation
   1  __rtsadp_add / __rtsadp_mul       run    <- GENERIC arithmetic, not fadd/fmul
   1  __rtsadp_class_proto_init         run    <- per-`new` prototype work, in-loop
   1  __rtsadp_obj_set_proto            run
-- TAGGED bindings --
   1  p                                 run    <- the instance has no proven repr
```

**`bench/monte_carlo_pi.ts`** — the proven-monomorphic numeric path, for contrast:

```
RUNTIME-CALL           1383            2     <- and both are math.random
BOX                      10            0
UNBOX                     0            0
TAGGED-BINDING            3            0
```

Zero boxes, zero unboxes, zero Tagged bindings inside the loop. This is the
27 ms/10M-iteration path that already beats Bun by 3.4×.

### What the measurement decided

1. **The premise held.** The gap is not the backend. Same Cranelift, same egraph:
   0 in-loop calls → faster than Bun; 17 in-loop calls → 77× slower than Bun.
2. **Phase 2 (escape analysis) is confirmed as the top lever, and it is bigger
   than estimated.** `p` never escapes `run`, so `VEC_NEW` + `VEC_GET`×3 +
   `VEC_SET`×2 + `POLY_TO_HANDLE`×5 + `class_proto_init` + `obj_set_proto` —
   13 of the 17 in-loop calls — are removable outright.
3. **A new finding not in the original plan: generic arithmetic on slot loads.**
   `p.x * p.y` emits `__rtsadp_mul`, not `fmul`, because `VEC_GET` returns
   `Tagged`. Per-slot `Repr` on the `Shape` (Phase 4) fixes this independently of
   escape analysis, and it is cheaper to build. It should probably run BEFORE
   Phase 3, contrary to the ordering below.
4. **`class_proto_init` / `obj_set_proto` run per `new`, inside the loop.** They
   are loop-invariant for a fixed class — hoistable to the loop preheader (or to
   the class's one-time init) with no new analysis. Cheapest available win.
5. **`math.random` emits a `call`** even though `abi::Intrinsic` lists
   `random_f64` as inlinable. Worth checking whether the intrinsic path still
   fires — a regression there would be silent today.

---

## Hoisting the per-class prototype wiring ✅ LANDED

The first optimization Phase 0 pointed at, and the one whose size Phase 0 got
wrong in the useful direction.

`new C(args)` emitted, per construction:

```text
proto = __rtsadp_class_proto_init(<class>, <parent>)   // per CLASS, idempotent
for each method m: __rtsadp_obj_set(proto, "m", <reify m>)   // per CLASS
__rtsadp_obj_set_proto(instance, proto)                // per INSTANCE
```

Only the last line is per-instance. The rest depends solely on the class — a
compile-time constant — yet ran on every `new`, and `reify` **allocates** an
`Entry::Function` each time. `bench/objbench.ts` under-reported the cost because
its class has no methods (2 of 17 in-loop calls); a class with 4 methods pays
**9** per construction.

`crates/rts-codegen-new/src/front/run/class/protohoist.rs` prescans the body for
the classes it constructs, and `lower_function`'s prologue wires each one ONCE in
the entry block (which dominates every `new`), parking the proto word in a
Cranelift `Variable` — the same trick `gcell_cache` uses, for the same reason
(reusing an effectful call's result across blocks makes egraph elaboration
panic). A lazy first-use cache does NOT work here: when the first use is inside
the loop, its defining call still runs every iteration.

Measured on `bench/objbench_methods.ts` (1M constructions of a 4-method class):

| | in-loop calls | time (median of 3) |
|---|---|---|
| before | 21 | 7332 ms |
| after | 12 | ~620 ms |

**≈11×.** `bench/monte_carlo_pi.ts` unchanged (it constructs nothing).

### The gate — and why the first version was wrong

The first version hoisted unconditionally, on the stated premise that
`class_proto_init` is idempotent so the moment of the call cannot matter. That
premise was false, and the runtime says so in its own comment: the chain link is
guarded by `if proto_of(proto).is_none()` because it must *"NEVER overwrite a
[[Prototype]] the user already wired (`F.prototype = Object.create(Base.prototype)`
runs before the first `new F()`)"*. The init is **order-dependent by design** —
it defers the decision to the first construction precisely so an earlier
prototype replacement survives.

Hoisting to the entry block runs it BEFORE such an assignment, so the guard sees
an unlinked proto, wires the default root, and the program's replacement never
reaches the instances. It regressed `tests/fn_prototype_set_explicit.test.ts`
(`legs`/`barks` read `undefined`) — caught by the suite, not by review.

So the hoist is gated program-wide: if ANY function writes through a `.prototype`
member, no function hoists and every `new` wires inline as before. Program-wide
because the write and the `new` need not share a function. Conservative in the
only safe direction — a missed hoist costs speed, never correctness.
`tests/class_proto_hoist_gate.test.ts` pins both halves in one program.

**The transferable lesson:** "idempotent" is not the same as "order-independent".
Before hoisting anything out of a loop, read the callee for order dependence
rather than inferring it from the name.

### Per-slot `Repr`: unbox at the USE, never at the READ

A prototype of this (a per-class `field_numbers` set + unbox in `lower_arith`)
was built, measured at **1.62×**, and then discarded in favour of doing it
properly through the `Shape` — but it found a trap that the real implementation
must not walk into.

**Unboxing at the READ site is wrong.** A field declared `n: number` that the
constructor never assigns still holds `undefined`, and `console.log(z.n)` must
print `undefined`. Decoding the slot to an `f64` at the read makes it print
`NaN`. Measured, not theorised — the first version of the prototype did exactly
this and the regression was caught by probing an unassigned field.

**Unboxing at the USE site is right.** Only arithmetic forces the number, and
`undefined * 1` is `NaN` in JS anyway, so the observable stays correct while the
optimization still lands: `p.x * p.y` emits `fmul` because both operands were
decoded on their way into the operator, not on their way out of the slot.

So a slot's `Repr` describes what the slot is PROVEN to hold, and licenses a
decode at the point of consumption. It does not license rewriting every read as
a native-typed load. The two differ exactly on the values a JS program can
observe before they are used.

**Measure this axis on the right shape of program.** The same change measured
~6% on `objbench.ts` and 1.62× on a loop doing only field arithmetic after a
single construction. `objbench.ts` is dominated by allocation traffic, which
masks the arithmetic entirely — so a benchmark that constructs in the loop will
report that per-slot `Repr` is not worth doing. Build the isolating case (one
construction, then a hot loop of pure `p.x * p.y`-style work) before concluding
anything about this phase.

## Phase 1 — close the conservative `Tagged` widenings (Axis A)

`Repr::join` is deliberately total and pessimistic
(`crates/rts-codegen-new/src/repr.rs`): disagreement widens to `Tagged`. That is
correct and must stay correct. But several widenings today are *artifacts of
lowering order*, not of genuine disagreement.

### 1a. Loop-header phis — two-pass fixpoint

When we create a loop header we have not yet seen the back-edge, so the phi
defaults to `Tagged`. This is the single most expensive widening because it sits
in the hot loop by definition.

Fix: lower optimistically with the entry predecessor's `Repr` and record the phi
sites; after the back-edge is known, recompute the `join`. If it disagrees,
re-lower the function with that phi pinned to `Tagged`. This converges in at most
two iterations because the lattice has height 2 (`R → Tagged`) and the transfer
function only ever descends. Soundness is structural: the second pass can only
widen, never narrow.

### 1b. Immutable captures

A `const` capture never reassigned, of a proven type, does not need a `Tagged`
cell. Only a *mutable* capture requires the boxed env-record slot. Splitting
these two cases removes box/unbox from every closure over a numeric constant.

### 1c. Numeric accumulator widening (`Int ⊔ Float`)

`let s = 0; ... s += 0.5;` currently truncates or widens to `Tagged` — the known
`int-accumulator-float` defect. Two parts to the fix:

1. Join the `Repr` of **all** assignments to a binding, not only its initializer
   (the fixpoint of 1a delivers this for free).
2. Add the one principled special case to the lattice:
   `Int32 ⊔ Float64 = Float64` and `Int64 ⊔ Float64 = Float64`, instead of
   `Tagged`. This is sound — both are unboxed numbers and the coercion is an
   explicit `fcvt_from_sint` IR node, exactly like today's box/unbox nodes. It
   keeps the classic numeric accumulator in a register.

**Acceptance:** in a numeric loop with an accumulator and a closure, `rts ir`
shows zero `band`/`bor` tag manipulation per iteration.

**Expected gain:** 3–10× on loops that currently widen.

---

## Phase 2 — escape analysis + scalar replacement (Axis B)

**The highest-return item in this document.** An object that does not escape its
creating function should not exist at runtime at all.

### Analysis

An object created by a literal in function `f` *escapes* if it is: returned;
passed as a call argument; stored into a field of another object or into an
array; captured by a closure; or reached by a computed-key write that invalidates
its static shape. If none of those hold, it does not escape.

That is a conservative intraprocedural analysis over the HIR — no interprocedural
summary needed for the first slice.

### Transform

For a non-escaping object with a statically known shape (the transition tree in
`crates/rts-codegen-new/src/shape.rs` already gives us this at the creation site):

- Emit **no allocation**.
- Each slot becomes a Cranelift `Variable` carrying the proven `Repr` of the
  value stored into it.
- Each property access on it, already shape-resolved, becomes a direct read/write
  of that Variable.
- The egraph then const-folds, CSEs and DCEs across what used to be heap traffic.

This removes, in one transform: the allocation, the GC tick pressure, the
HandleTable indirection on every field access, and the IC shape compare. It is
the difference between "10× slower than Rust" and "1.3× slower than Rust" for
small-object-oriented code.

**Prerequisite:** shape known statically at the creation site — we have it. If a
computed key arrives later, mark escaped and bail. Conservative is correct.

**Expected gain:** 5–30× on allocation-heavy benchmarks.

---

## Phase 3 — inline bump allocation (Axis B)

Whatever still escapes after Phase 2 is genuinely heap-resident. Its allocation
should still not be a `call` into a sharded slab.

```
p = load(nursery_top)
n = p + size
if n > nursery_limit -> call slow_path      // cold edge
store(nursery_top, n)
```

Three instructions instead of a call plus shard arbitration. This needs the
nursery from [`gc-generational-design.md`](gc-generational-design.md), but the
bump path itself is independent of copying — bump + existing sweep is a valid
intermediate state and can land before the generational collector does.

---

## Phase 4 — typed dense containers (Axis B)

A proven `number[]` should be a contiguous `f64` buffer, not an array of
`PolyValue`. Then `a[i]` lowers to `load.f64 [base + i*8]` with one bounds check,
and a loop over a numeric array emits byte-for-byte what Rust emits.

Without this, every element access pays a box and an unbox, and every numeric
array loop carries two extra ops per element.

The same idea generalizes to object layout: `Shape` can carry a per-slot `Repr`,
so `slots` stops being a uniform `[PolyValue; N]` and becomes a typed layout with
per-field offsets. That is a larger change to `shape.rs`, and it is deliberately
sequenced **after** Phase 2 — escape analysis will have already deleted a large
share of the objects that would benefit.

Hoisting the bounds check out of `for (i = 0; i < a.length; i++)` requires LICM
over the check; if the egraph does not deliver it, hoist it in the front-end at
the loop header.

---

## Phase 5 — front-end inlining of user functions (Axis B)

Cranelift performs intraprocedural inlining, but only over what is present in the
IR it is handed. A TS→TS call lowered as a `CallConv::Tail` call is an
optimization barrier: no inlining, no CSE across it, and every live value spills.

Inline in the front-end, before handing the function to the egraph, when the
callee is: small (body under N IR instructions), non-recursive, fixed-arity, and
called with arguments of proven `Repr`. Once the bodies are merged, the egraph
does const-fold and CSE *across* the former boundary — which is precisely where
Rust's advantage comes from.

---

## Phase 6 — the ceiling

After the phases above, the residual gap to Rust is structural:

- **Auto-vectorization.** Cranelift has no loop vectorizer, and issue `#92` was
  closed as infeasible without writing our own. Numeric kernels stay 4–8× behind
  LLVM and V8 on this axis. Realistic options: expose explicit SIMD intrinsics as
  a namespace and hand the control to the developer, or accept the gap. Writing a
  vectorizer is a separate multi-month project.
- **Genuinely dynamic code** (`any`, `JSON.parse` results, polymorphic
  arguments). Closing this needs speculative tiering with deoptimization — profile
  at runtime, recompile assuming the observed type, guard, bail out on
  violation. Large and invasive; only worth considering after every phase above
  has landed, and it is fundamentally at odds with AOT (an AOT build sees no
  profile).

---

## Recommended order

The original ordering was:

```
Phase 0  →  Phase 1  →  Phase 2  →  Phase 3  →  Phase 4  →  Phase 5
(measure)   (Tagged)    (escape)    (bump)      (dense)     (inline)
```

**Phase 0's measurement revises it.** The `objbench` histogram says the cheapest
large wins come before the expensive ones:

```
Phase 0 ✅ → hoist loop-invariant `new` work → Phase 4a (per-slot Repr on Shape)
          → Phase 2 (escape analysis) → Phase 1 (Tagged widenings) → 3 → 5
```

- ~~**Hoist `class_proto_init`/`obj_set_proto` out of the loop**~~ ✅ **LANDED.**
  See "Hoisting the per-class prototype wiring" below — measured **~11×** on a
  4-method class, far above the 2-of-17 the methodless `objbench.ts` suggested.
- **Phase 4a, per-slot `Repr` on `Shape`** — turns `__rtsadp_add`/`__rtsadp_mul`
  back into `fadd`/`fmul` for every proven-typed field read. Smaller than full
  dense containers and pays off on all object code, not just arrays.
- **Phase 2, escape analysis** — still the biggest single lever (13 of 17 in-loop
  calls on this benchmark), just the most work.
- **Phase 1** drops below them: on real measured programs the artificial `Tagged`
  widenings were far rarer than the heap traffic. Keep the loop-header-phi
  fixpoint, but it is no longer the first thing to build.

Re-measure after each step; the ordering above is what the first histogram said,
not a fixed plan.

---

## Working loop and acceptance criterion

```bash
RTS_REPR_STATS=1 target/release/rts.exe run bench.ts   # histogram of CAUSE (phase 0)
target/release/rts.exe ir bench.ts 2>&1                # call/load/tag per iteration
# attack the largest cause
hyperfine 'target/release/rts.exe run bench.ts' './bench_rust'   # confirm, never assume
```

`bench/objbench.ts` is the reference allocation-heavy case for this loop (the
2058 ms / 1024×-vs-Rust baseline above). `bench/monte_carlo_pi.ts` is the
already-fast contrast — it must not regress while the phases land.

The per-phase acceptance criterion is read off the IR of the hot loop, not off a
stopwatch:

> **Zero `call`, zero `load`/`store` of a local variable, and zero `band`/`bor`
> tag manipulation per iteration.**

When the hot loop's IR contains only arithmetic, we are emitting the same code
Rust emits, and everything left is Cranelift-vs-LLVM — about 1.3×, and done.

---

## Relationship to the other plans

- [`rts-codegen-new-design.md`](rts-codegen-new-design.md) — defines the value
  model these optimizations operate on. Nothing here contradicts it; Phases 1–5
  are the optimizations the Repr lattice and the shape/IC machinery were designed
  to make possible.
- [`gc-generational-design.md`](gc-generational-design.md) — Phase 3 is the
  allocation-side half of that plan, and can land ahead of the copying collector.
- **Coverage comes first.** Cross-runtime parity is the active goal
  (`.claude/rules/00-meta.md`). This plan is what to do once correctness coverage
  stops being the binding constraint — with the exception of Phase 0, which is
  cheap, non-invasive, and worth building early because it makes every later
  decision measured.
