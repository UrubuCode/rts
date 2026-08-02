# RTS_CLASS_IMPLEMENTATION.md — lowering a `class` to machine code

**Status:** C0 DONE, implementation started (2026-08-02). Every number below is
from a probe kernel or from the real engine. §1.4 (kernel W) is the C0
prerequisite and it CLEARS the gate C0 set. Two engine changes have landed since:
one-call instance allocation (`new P()` 632 -> 531 ns/iter) and T3 thread-affine
regions behind `RTS_REGIONS=1` (531 -> 359 ns/iter, default OFF — it exposes a
pre-existing re-entrant shard lock, see the commit).

Every number is **[M]** measured on this machine, **[S]** sourced with a link, or
**[E]** an estimate — treat **[E]** as a hypothesis to test, never as a result.

Measurement conditions for every **[M]**: `cargo build --release`, x86-64
Windows, medians of 7, checksums equal across every row of a kernel. Probe rows
reproduce with `cargo run --release -p rts-value-probe -- <filter>`. Engine
numbers reproduce with `target/release/rts.exe run <file>`.

## How this relates to the other documents

| Document | What it owns |
|---|---|
| `docs/specs/rts-codegen-new-design.md` | The canonical engine architecture (PolyValue, Repr lattice, shapes, single lowering) |
| `docs/specs/rts-threading-model.md` | Regions, promotion on publication, the "payload = slot index, never a pointer" invariant |
| `docs/specs/gc-generational-design.md` | Weak phase now, copying nursery later; why moving is cheap |
| `RTS_OPTIMIZATION.md` | The measured cost ladder and the refuted premises |
| `crates/rts-value-probe/README.md` | Every kernel's raw table **and its caveats** — read the caveats before quoting |
| **this file** | What a `class` costs today, why, what to build, and **how to modularize it** |

---

## §1 What was measured

### §1.1 The real engine [M]

`class P { x: number; y: number; getx(){return this.x} sum(){return this.x+this.y} }`,
3M iterations, release:

| | ms | ns/iter |
|---|---|---|
| free function call | 6 | 2.0 |
| `p.x` — one field read | 39 | 13.0 |
| `p.getx()` — method wrapping one field | 43 | 14.3 |
| `p.sum()` — two fields | 79 | 26.3 |
| `new P(i,1)` + one read | 1668 | **555** |

The emitted body of `getx`:

```text
v3 = band v0, 0xffff_ffff_ffff        ; untag `this`
v4 = call fn0(v3, 1)                  ; __rtsn_vec_get_by_payload -> shard Mutex
```

The `1` is a **compile-time constant slot** — the front-end already resolved
`this.x` to a slot index (`obj.rs:608-636`). "Fast path" today means "no key
scan"; it is still an opaque call holding a lock.

### §1.2 Kernel M — method + `this` [M]

`cargo run --release -p rts-value-probe -- m`. `s = s + p.sum()`, 3M iters.

| variant | ns/iter |
|---|---|
| M0 today — real call, tagged `this`, 2 locked field calls, generic `+` | 16.29 |
| M1 +proven Repr (`+` inline) | 10.93 |
| M2 +untagged `this` | 10.97 |
| M3 +no lock | 6.27 |
| M4 +`this` in a register, fields are loads | 2.08 |
| M5 +method inlined, IC shape guard | 1.13 |
| M6 +escape analysis | 0.69 |

### §1.3 Kernel H — object management [M]

`cargo run --release -p rts-value-probe -- h`. `s = s + p.x + p.y`, 3M iters.

| variant | ns/iter |
|---|---|
| H0 today — `call __rtsn_vec_get_by_payload` | 11.28 |
| H1 −shard `Mutex` | 6.57 |
| H2 −`Box<Vec<i64>>` (inline slots, still a call) | 5.49 |
| H3 −the call — handle→addr in pure IR, guarded unbox | 1.80 |
| H4 +`Repr` on the field (tag guard gone) | 1.51 |
| H5 overflow bag — the `(p as any).z` fallback | 1.90 |
| H6 escape analysis | 0.76 |
| H7 chunked storage — the stable-storage precondition | 1.54 |
| H8 packed stride 32 B | 1.51 |
| H9 region, base as `iconst` | 0.99 |
| **H10 MOVABLE — one slot indirection, blocks relocated** | **1.50** |
| H11 movable + region | 0.99 |

### §1.4 Kernel W — construction + writes [M] — C0, and it settles the plan

`cargo run --release -p rts-value-probe -- w`. Construct one 4-field object, store
the fields, read them all back (the read-back is the checksum), 50k objects.

| variant | ns/iter | the single thing it changes |
|---|---|---|
| **W0 today** | **347.42** | `vec_new_object` + one LOCKED push per field, locked read-back |
| W1 block alloc, direct store | 5.84 | −the lock and the `Box<Vec<i64>>`; allocator hands back the block address |
| **W2 store via handle** | **7.22** | −the handed-back address: `handle → block` in pure IR — **the C2 number** |
| W3 +card-mark barrier | 9.27 | W2 + one unconditional card mark per field store |
| W4 region, const base | 5.13 | W2 with no shard routing, slot-table base an `iconst` |
| W5 filled at alloc | 8.44 | −the separate store pass: fields supplied to the allocator |

**C0's question was whether construction stays expensive, and the answer is no:
347 → 7.22 ns, 48×.** That is a bigger lever than the read ladder's 11.28 → 1.50
(7.5×), and it means cheap reads are worth building — the case §7 C0 said to
settle before starting C2.

Three further readings, none of them guessable from the read ladder:

- **The write barrier costs 2.05 ns** (W2 → W3, 28%). A moving/generational
  collector is affordable on this layout; it is not free, and this is the first
  number for it.
- **Regions pay on the write path too**: 7.22 → 5.13 (W4), 2.1 ns, independent of
  the read-side 1.50 → 0.99 (H11).
- **Filling fields AT allocation is SLOWER than storing after** (8.44 vs 7.22).
  Counter-intuitive, and it says the win in the engine's one-call allocation
  (`vec_new_object_shaped`) comes from removing LOCKED CALLS, not from fusing the
  stores. On a lock-free block layout the fusion stops paying.

Calibration: W0's 347 ns against the real engine's 531 ns/iter for `new P(i,1)` +
one read — the same order, with the probe under-modelling (no GC tick on the block
rows; see the README caveats).

---

## §2 What is REFUTED

Do not spend a session on these. Each was a plausible hypothesis killed by a
number.

1. **"Class/`this` is slow because of dispatch."** [M] A method costs **~0.95 ns**
   over the field read it wraps (M4→M5), and dispatch with a proven class is
   already a direct monomorphic `call __rtsn_method_C_m` — no vtable
   (`class/dispatch.rs:734`). Redesigning vtables/calling conventions buys ≤1 of
   the ~26 ns.
2. **"Untagging `this` costs something."** [M] M1→M2 is **+0.04 ns**. The `band`
   is free.
3. **"Handle identity forbids a flat layout."** [M] Computing handle→address in
   pure IR pays one extra hoistable load and still beats the call by 3.57 ns
   (H2→H3). GC and threading forbid a *raw pointer*; they do not forbid the
   result.
4. **"The `Box<Vec<i64>>` indirection is the problem."** [M] It is 1.58 ns of
   11.28 (H1→H2), and H10 shows a *well-formed* indirection costs **zero**.
5. **"A regional/copying GC is incompatible with fixed-offset fields."** [M]
   H10 = 1.50 vs H4's 1.51, with every block actually relocated into a to-space
   and re-read through the same handle. See §4.
6. **"Copy V8: hidden classes + polymorphic inline caches + deopt."** RTS is a
   compiler, not an engine. Static Hermes pre-computes the slot the hidden class
   would have yielded and quarantines ICs to the untyped path **[S]**; Dart AOT
   proves globally and needs neither IC nor deopt **[S]**. Full deoptimization
   also requires a baseline tier and frame-state metadata RTS does not have.

---

## §3 The constraint census

From reading `collector/scan.rs`, `heap/handles.rs`, `objops.rs`,
`class/mod.rs`, `module_aot.rs`, `rts-threading-model.md`.

| # | Constraint | Verdict |
|---|---|---|
| 1 | Conservative GC scanner decodes `gen\|slot\|shard` handle words and validates against the live `HandleTable` | **HARD — on addressing mode only** |
| 2 | PolyValue `Tagged` slot needs a runtime type tag | SOFT |
| 3 | Reflection: `Object.keys/entries/assign`, spread, `in`, `delete`, `defineProperty`, `instanceof` | SOFT — fallback trampolines exist |
| 4 | Dynamic shape growth: `(p as any).z = 1` is legal and `shape_with_added_key` willingly grows the object | **HARD** until the compiler proves no dynamic escape reaches the instance |
| 5 | Subclassing — already flat, parent-first, no vtable (`class/mod.rs:17-20`) | NON-ISSUE |
| 6 | AOT — slot indices and shape ids already baked as immediates; only the registry is re-seeded (`module_aot.rs:228-242`) | NON-ISSUE (favourable) |
| 7 | Threading — region migration re-homes entries and updates slots | **HARD — on addressing mode only** |

**Constraints 1 and 7 do not forbid a flat struct. They forbid a raw pointer.**
Both require the object's outward identity to stay a handle. §4 is how that is
satisfied at zero cost.

**Constraint 4 is the only one that forces a fallback**, and H5 prices it at
**0.25 ns** over a direct slot — against 63.82 ns for the dictionary read in the
probe's kernel OBJ. Fixed slots + an overflow bag; never dictionary-mode
demotion.

---

## §4 The target design

Static Hermes is the model, not V8: nominal classes, slot index assigned at
declaration, load by constant index, ICs confined to the untyped path **[S]**.
RTS already does the hard half — `obj.rs:608-636` resolves `this.x` to a
constant slot.

### §4.1 Addressing — movable, handle-identified

```text
stab  = load [SLOT_TABLES + shard*8]   ; loop-invariant
block = load [stab + idx*8]            ; THE indirection — one word
x     = load [block + 8*(1 + slot)]    ; Hermes's _sh_prload_direct
```

Moving an object = copy the block, store its new address into that one word. The
handle never changes and no live PolyValue word is rewritten — exactly the
property `gc-generational-design.md` calls the architectural advantage, kept.

With one region per thread the first load becomes an `iconst` (H11, 0.99 ns) —
what Dart buys with its reserved `GDT` register **[S]**.

### §4.2 Block layout

```text
word 0            shape id
words 1..=N       direct field slots   (Hermes DIRECT_PROPERTY_SLOTS)
last word         overflow pointer     (0 = none)  -> constraint #4's fallback
```

Fixed stride so `idx * STRIDE` is a shift. Growing an object is natural in the
movable form: allocate a bigger block, copy, update the slot word.

### §4.3 The precondition, priced

Storage must be genuinely stable — a `Vec` realloc would invalidate every live
address. A chunk list works and is **free** [M]: the two-level index collapses
into one flat table load, `table[shard * CHUNKS + chunk]`, so the cost is a
shift and a mask, not a second dependent load (H7).

### §4.4 The compaction precondition — not optional

**The indirection is free only when the collector produces a CONTIGUOUS
to-space** [M]. An earlier probe version evacuated each block into its own
`vec![0i64; 8]` — 1024 scattered allocations — and the movable row swung between
**4.4 and 8.2 ns** run to run. A non-compacting collector gives the entire win
back. This is an argument *for* the copying nursery, not merely compatibility
with it.

---

## §5 What this design GIVES UP, and must pay back in the compiler

Today's `vec_get_by_payload` is a **total function**: it validates the slot is
live, that the `Entry` is a `Vec`, and bounds-checks the index; every failure
returns `0` (`payload_ops.rs:78-87`). A raw load does none of that — it trades
"returns undefined" for **memory corruption**.

This is the same trade Hermes, Dart and AssemblyScript make on their typed
paths, and it is legitimate for a compiler. But the three checks must be
replaced by **proof in the front-end**, and wherever proof is unavailable (`any`,
`as` casts, `JSON.parse`, Registry-resolved returns, untyped boundaries) the slow
total path must remain. That is Hermes's `CheckedTypeCast` at ingress **[S]**.

**A second, already-written invariant is broken.** `vec_push_by_payload`'s doc
comment states deliberately: *"no pointer survives the call boundary in either
direction"*. This design makes addresses survive into generated code, in
registers, hoisted out of loops. Replace it with an explicit safepoint rule
(§8.2) — do not silently ignore it.

---

## §6 Module layout — where the code goes

The binding rules: **codegen ≤ 1000 lines, engine ≤ 700, everything else ≤ 500**;
past the ceiling, split into a folder of cohesive submodules. Run
`bash scripts/read_before_commit.sh` before every engine commit.

### §6.1 Files already over ceiling — do NOT add to them

| File | Lines | Ceiling | Note |
|---|---|---|---|
| `crates/rts-natives/src/heap/handles.rs` | 2761 | 500 | draining target |
| `crates/rts-codegen-new/src/front/run/obj.rs` | 1582 | 1000 | draining target — member lowering lives here |
| `crates/rts-codegen-new/src/front/run/class/synth.rs` | 823 | 1000 | near ceiling |
| `crates/rts-codegen-new/src/front/run/class/dispatch.rs` | 788 | 1000 | near ceiling |

New code lands in new modules. Field-access lowering must NOT be appended to
`obj.rs`.

### §6.2 Proposed new modules

**Runtime side — `crates/rts-natives/src/heap/blocks/`** (new folder, each file
≤ 500):

| File | Owns |
|---|---|
| `mod.rs` | Re-exports + the layout constants (`STRIDE_WORDS`, `DIRECT_SLOTS`, `OVERFLOW_WORD`) that the codegen also reads — ONE definition, not two |
| `store.rs` | Chunked, stable block storage per shard/region; chunk table; commit-on-demand |
| `slots.rs` | The slot table: `handle -> block address`, and `relocate` (the one-word write) |
| `alloc.rs` | Block allocation + field fill; the `new` fast path |
| `fieldmap.rs` | Per-shape field kind map (which words are references) — the GC's precondition for §7.3 |

**Codegen side — `crates/rts-codegen-new/src/front/run/field/`** (new folder):

| File | Owns |
|---|---|
| `mod.rs` | Entry points `lower_field_get` / `lower_field_set`, and the decision "proven or not" |
| `addr.rs` | The pure-IR `handle → block address` emission (§4.1), both sharded and region forms |
| `guard.rs` | Ingress checks where proof is absent (§5), and the shape guard for structurally-typed receivers |
| `overflow.rs` | The overflow-bag arm (constraint #4) |

**Type information — `crates/rts-codegen-new/src/front/run/class/fieldty.rs`**
(new file): the per-field `Repr` that `synth.rs` currently discards (§7.2).

### §6.3 What must NOT be duplicated

Layout constants (`STRIDE_WORDS`, slot numbering, the overflow-word index) are
read by the runtime AND baked as immediates by the codegen. **One definition, in
`rts-natives`, imported by the codegen.** A second copy is exactly the mirror
table the SINGLE SOURCE OF TRUTH rule exists to prevent — and the
dependency-direction bans were removed (2026-07-28) precisely so this import is
legal. Do not hand-write a mirror to avoid an edge.

Every new runtime symbol is declared with `#[rtse::abi]` and picked up by
`cargo run -p rts-symbol-baker`. Never hand-write a `#[no_mangle] extern "C"`
name, and never add a row to `value/abi_sig.rs` or `adapters/dispatch.rs`.

---

## §7 Phases

Ordered by measured delta ÷ risk. Each phase is committable on its own and must
leave the suite in a known state (regression allowed only if explicit and
justified).

### C0 — measure the other half — DONE (2026-08-02), see §1.4

Everything measured so far is **reads**. The engine's worst number is
`new P()` = **555 ns [M]** — 37× a field read — and nothing has priced field
**writes** or the write barrier on this layout.

DONE. Kernel W is built (`emit/kernel_w.rs`, `bench/writes.rs`, `slab/cards.rs`) over the
movable block layout: block alloc + slot install + field fill, write with and
without a card-mark barrier, against today's
`__rtsn_vec_new_object` + one locked `push` per field. The gate it set — "if construction stays at 555 ns, cheap reads do not move
class-heavy code" — is CLEARED: construction goes 347 -> 7.22 ns on the block
layout (48x), a bigger lever than the read ladder. C2 is justified.

### C1 — per-field type information

`synth.rs` currently discards `x: number`: there is no `field_numbers` set
beside `field_arrays`/`field_strings`, and `obj.rs:616` hands back
`Repr::Tagged` unconditionally. Record the declared field `Repr` in
`class/fieldty.rs`. Worth 0.23 ns on a double field [M] and more on int32
fields **[E]**, but its real value is that C3 and §7.3 both need it.

### C2 — the movable block layout

`heap/blocks/` (§6.2) + `field/addr.rs`. Reads go from
`call __rtsn_vec_get_by_payload` to three loads. Keep the old path live for
unproven receivers. Expected 11.28 → ~1.50 ns per two-field read [M, probe].

**NOT STARTED — and §8.4 is its prerequisite, not its footnote (2026-08-02).**
An implementation pass established that neither of the two objections this
document spends most of its risk budget on is what stops C2:

- The **GC race** does not stop it. A receiver held in the frame is a
  conservative root, so its slot cannot be swept or reused during the read, and
  a GC tick fires only from `alloc_entry` — only at a call. That is the same
  property `collector/scan.rs` already relies on when it scans six callee-saved
  registers and no caller-saved ones, and removing the field-read call does not
  weaken it.
- **§8.2 hoisting** does not stop it either, and the reason is specific to this
  storage: the chunk-table base is a `static` address, a table entry is
  published once (`Release`) and never rewritten, and a chunk never moves and is
  never freed. The base and the slot address are loop-invariant *in fact*. What
  would break it is the moving collector of §4.4 — and the one-word block
  indirection is exactly what keeps it safe there, since relocation rewrites the
  block word, not the slot address.

What stops it is **§8.4**. A raw load is sound only if the words cannot be freed
or reallocated under it, so the field words must live in the fixed-stride block
of §4.2 rather than in `Entry::Vec`'s `Box<Vec<i64>>` — a buffer the sweep drops
and any `push` reallocates. That is a new `Entry` variant, and `Entry::Vec` is
matched at **316 sites across seven runtime crates**, with object-vs-array
decided *dynamically* by `objops::looks_like_object` reading `(slot0, len)` —
there is no static split to inherit. A site missed in that fork returns a
silently wrong value for a class instance instead of failing loudly, and
`RTS_SLAB=1` does not contain it: a knob exists to be turned on and measured.

The narrowing that looks available is not: **C6 (landed) already removes the
allocation for every instance that provably stays local**, so what C2 is left to
speed up is precisely the escaping set — the set the 316 sites must keep reading
correctly. Restricting C2 to "safe" receivers therefore restricts it to the
empty set.

So the split is a phase of its own, ordered **before** C2: one representation
for shaped instances, `Entry::Vec` left to arrays, landed and measured on its
own. Only then is C2 an emission change. Nothing was landed for C2 here — the
layout constants and the `#[rtse::abi]` chunk-table/stride symbols belong to it
and are still unwritten, because an unused constant is dead code and an unused
symbol is a row the gate has to carry.

### C3 — the overflow bag

`field/overflow.rs`. Closes constraint #4 without dictionary-mode demotion.
0.25 ns over a direct slot [M].

### C4 — ingress proof and guards

`field/guard.rs`. Replace the three checks §5 removed with front-end proof plus
a total slow path at every unproven boundary. **This is the phase that makes C2
sound; C2 must not ship to users without it.**

### C5 — regions — PARTLY DONE (2026-08-02)

Thread-affine shard ranges landed as `crates/rts-natives/src/heap/regions.rs`
(`RTS_REGIONS=1`, default OFF): 26% on an allocation-heavy loop (3822 -> 2930 ms
for 1M escaping instances) and 34% on `new P()` before escape analysis removed
that benchmark`s allocation entirely. It passes the whole TS suite now — it did
not at first, and the reason is the useful part: thread affinity raised a LATENT
re-entrant shard-lock deadlock from ~1-in-32 to ~1-in-2 and made 8 test files hang
deterministically. 15 runtime sites were allocating or re-normalizing a handle
inside a `with_entry`/`with_rtse` closure. Regions did not introduce that; they
made it reproducible.

What is NOT done: the slot-table base is not yet an `iconst` (that needs C2`s
addressing mode), so the 1.50 -> 0.99 ns this phase is priced at has not been
collected. Local/regional collection is T4 of the threading model and untouched.


One region per thread makes the slot-table base an `iconst`: 1.50 → 0.99 ns [M].
Depends on `rts-threading-model.md` phases, not on this document. Also the only
way to retire the shard `Mutex` honestly — see §8.1.

### C6 — escape analysis + SROA on HIR — DONE (2026-08-02)

`crates/rts-codegen-new/src/front/run/escape/` (`RTS_ESCAPE=0` to disable). A
`new C(..)` bound to a local that provably does not escape becomes one Cranelift
`Variable` per field: no allocation, no shape-tag store, no prototype link, no IC
site. Measured 1050 -> 1 ms on a 2M-iteration loop; node is 13 ms on the same
program.

The doc`s ordering advice (inline first, then EA, then SROA) held: Cranelift`s
inlining pass was enabled earlier the same day, and the egraph then keeps the
scalarised fields in registers across the whole loop.

One finding worth carrying: the first capture bail used `arrow_free_idents`, which
looks INSIDE `Arrow` nodes — but arrows are LIFTED into their own `HirFunc`s
before this scan runs, so it answered "nothing is captured" and a captured local
was scalar-replaced into a `ReferenceError`. The signal that survives lifting is
the captures map the lowering already carries. A boundary fixture caught it
(`tests/escape_analysis_semantics.test.ts`), which is the argument for writing
those fixtures as boundaries rather than happy paths.


0.99 → 0.76 ns [M], and it is the phase that deletes allocations entirely.
Cranelift has neither EA nor SROA and its egraph cannot see through an
allocation call, so this lives in the HIR front-end. Order matters: **inline
first, then EA, then SROA** — HotSpot does scalar replacement only, not stack
allocation, and reports the win is largely enabled by inlining **[S]**.

---

## §8 Open risks — read before implementing

### §8.1 Multi-thread is the largest unproven assumption

Removing the shard `Mutex` is **4.71 ns of the 11.28** (42% of the win) and the
probe is **single-threaded**. Nothing here proves the lock can be deleted; it
proves what the lock costs one thread. The honest route is C5 (regions), not
"delete the mutex".

### §8.2 Safepoints and hoisting

The slot-table base and the loaded block address are loop-invariant and the
egraph will hoist them. In the real engine a GC tick (every 256 allocations) or
a region migration can run inside that loop, and migration rewrites exactly
those words. A hoisted block address across a safepoint is a stale pointer.
There is **no safepoint in any probe kernel**, so no row is evidence that
hoisting is safe. Define the rule explicitly (§5).

### §8.3 C1 × C2 interact — precise field maps become mandatory

Conservative marking tolerates mistaking an unboxed `f64` for a reference (a
false root is harmless). A **copying** collector does not — it would rewrite the
word. So `fieldmap.rs` is a *prerequisite* of combining unboxed fields with a
moving collector, not an optimization. Build it before C1 lands on a moving heap.

### §8.4 Arrays and objects share `Entry::Vec`

`vec_set_by_payload` grows with HOLEs (sparse-array semantics). Arrays grow
without bound; a fixed-stride block does not. The design therefore requires
splitting the object and array representations, which no measurement here
covers.

### §8.5 Unmeasured

- **Writes, construction, write barrier** — C0 exists to fix this.
- **Compile time and code size.** Each field access goes from one `call` to ~6 IR
  instructions. Compile time is a measured lever in this repo; more IR per access
  may cost there. Unmeasured.
- **Workload monoculture.** 1024 objects, two `number` fields, monomorphic hot
  site, working set in L2. Nothing tests polymorphic sites, cold code, reference
  fields (which bring GC and barriers), or a working set past L2 — where the
  fixed stride (H8) should start to bite.
- **Aliasing.** Every probe load uses `MemFlags::trusted`. Real lowering cannot:
  a field load must not be reordered across a call that could mutate. All rows
  are an upper bound.
- **Structural typing.** A parameter typed `{x: number}` can receive instances of
  unrelated classes with different offsets. Needs a shape guard or a passed
  offset (Swift's witness approach **[S]**). Unmeasured.
- **Nothing measured end-to-end in the real engine.** H0 (11.28) tracks the
  engine (13.0), which calibrates the probe — but every delta is probe-only.

---

## §9 Rules this work must obey

- **PRIMORDIAL-vs-REGISTRY doctrine.** `Object` is primordial, so the engine may
  name it. Do not add a non-primordial class name to codegen control flow.
- **SINGLE SOURCE OF TRUTH.** `#[rtse::abi]` declares; `rts-symbol-baker` bakes.
  `cargo run -p rts-symbol-baker -- --check` must be clean.
- **File ceilings** (§6) and `bash scripts/read_before_commit.sh` before every
  engine commit.
- **The honesty floor.** No fixture deleted, disabled, or special-cased to move a
  number. Nothing that crashes or hangs is a "pass". Re-measure parity; never
  quote a remembered figure.
- **Regression is allowed when explicit and justified**, never silent.
