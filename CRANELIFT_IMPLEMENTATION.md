# CRANELIFT_IMPLEMENTATION — move runtime calls into native IR

> Written in English per the documentation-language rule (`CLAUDE.md` §
> Conventions). Working language stays Portuguese.

Goal: everything the engine can express as **Cranelift instructions** should be
emitted as IR, not as an `extern "C"` call into the Rust runtime. A call is a
register spill, a barrier the e-graph cannot see through, and (in a hot loop) the
dominant cost. An instruction is none of those.

Reference for what Cranelift 0.131 offers:
[`docs/specs/cranelift-explications.md`](docs/specs/cranelift-explications.md).

---

## Step 0 — build the intrinsic path (DONE)

**Status:** ✅ done · **Effort:** medium · **Risk:** low · **Unblocks 1–4**

`rts-engine::abi` defines an `Intrinsic` enum (`Sqrt`, `AbsF64`, `MinF64`,
`MaxF64`, `AbsI64`, `MinI64`, `MaxI64`, `ReceiverIdentity`) and `math/mod.rs`
registers `sqrt` with `Some(Intrinsic::Sqrt)` — but **`rts-codegen-new` never
reads it**. `grep -rn "Intrinsic" crates/rts-codegen-new/src/` returns nothing,
and the IR confirms it:

```
# rts ir over `math.sqrt(x)` — no `sqrt` instruction, only:
v2 = call fn0(v0, v1)
```

So *every* math call is an extern call today, `sqrt` included. Both `CLAUDE.md`
("Intrinsics inline … sqrt, abs_f64, …, random_f64 → direct Cranelift IR") and
`.claude/rules/05-codegen-notes.md` describe a mechanism the current engine does
not have — they describe the DELETED old engine. Fix those docs with this step.

The work: in the generic Registry call path (`registry_call.rs` /
`registry.rs::resolve`), check the resolved member's `intrinsic` field and, when
`Some`, emit IR instead of the call. That is one dispatch point; every step below
then becomes "add an enum variant + an emission arm + flip the registration".

`ReceiverIdentity` already has a separate emission path documented on the enum —
check whether it is honoured before assuming nothing works.

### What shipped

`front/run/intrinsic.rs` — `Lowerer::emit_intrinsic`, called from
`lower_builtin_call` once the args are lowered, reading the new
`ResolvedCall.intrinsic` field. Covers `Sqrt`, `AbsF64`, `MinF64`, `MaxF64`,
`AbsI64`, `MinI64`, `MaxI64`.

It **falls back, never fails**: an operand whose `Repr` is not a proven scalar
(`Tagged`, a string, …) or a wrong arg count returns `None` and the caller emits
the ordinary extern call. So the coercion authority stays in one place — the
intrinsic path only takes sites it can prove.

Verified in the IR: `math.sqrt(x)` was `v2 = call fn0(v0, v1)` and is now
`v5881 = sqrt v5879`.

**Measured** (AOT, `Measure-Command`, median of 5): `pi_machin` 16 → **13 ms
(-19%)**; `monte_carlo_pi` 83 → **81 ms** — barely moved. Step 3 later measured
WHY: its `math.random` calls cost only ~1 ns each, so there was never much there
to recover. TS suite 717 passed / 8 failed
(recorded baseline 709/15); engine unit tests 825/4 = documented baseline.

`CLAUDE.md` and `.claude/rules/05-codegen-notes.md` claimed this mechanism
existed while it did not. It exists now — with the caveat that `random_f64` is
still a call — Step 3 found that cranelift-jit cannot declare TLS data at all, so
that entry in the docs stays aspirational indefinitely.

---

## Rules for every step

1. One step per commit, with a **measured** before/after.
2. Validate: TS suite failing-file list **byte-identical** (`comm` diff, not
   counts — the suite is non-deterministic by ±1), unit tests at baseline
   (825/4), AOT crash-free.
3. Measure with `Measure-Command` against a `cmd /c exit` baseline — a bash
   harness adds ~40 ms of spawn overhead and silently inflates every number.
4. Emission order must stay deterministic (a `HashMap` iteration feeding codegen
   already produced broken AOT binaries once).

---

## Step 1 — `num` bit operations → single instructions

**Status:** ✅ done · **Effort:** low · **Risk:** low

**Result: the call died, the conversions did not.** All 13 mapped members emit
their instruction (`rts ir` over a bit-op loop: 0 calls to `__RTS_FN_NS_NUM_*`),
but the loop only went **142 → 139 ms (2%)**. The same IR still carries 16
`fcvt` instructions: TS `number` is an f64, so every operand round-trips
f64→i64→f64 around the now-inline op. **That conversion, not the call, is what
dominates.** The lever for this class of code is a typed/unboxed integer local
(Repr work), not more intrinsics — worth knowing before investing in steps 3–4
expecting a large number.

`checked_*` and `saturating_*` deliberately keep their calls: sentinel and
saturation semantics with no 1:1 instruction, where guessing trades correctness
for speed. `reverse_bits` likewise stays a call.

These are `extern "C"` calls today and each maps to exactly one Cranelift
instruction (§7, §18 of the reference):

| RTS (extern call) | Cranelift |
|---|---|
| `count_ones` | `popcnt` |
| `leading_zeros` | `clz` |
| `trailing_zeros` | `ctz` |
| `swap_bytes` | `bswap` |
| `rotate_left` / `rotate_right` | `rotl` / `rotr` |
| `checked_add` / `checked_mul` | `sadd_overflow` / `umul_overflow` (returns `(value, flag)`) |
| `saturating_add` / `saturating_sub` | `sadd_sat` / `ssub_sat` |
| `wrapping_add` / `wrapping_sub` | plain `iadd` / `isub` (wrapping is the default) |

`reverse_bits` has no direct Cranelift instruction — leave it as a call.

Start here: mechanical, isolated, and it validates the path for the rest.

---

## Step 2 — `switch` → jump table

**Status:** ✅ done · **Effort:** low · **Risk:** low · **Measured 1.93×**

The plan below understated the problem: `front/run/switch.rs` was not a chain of
`brif`, it was a chain of **extern CALLS** — one `__rtsadp_strict_eq` per case,
in source order. A 20-case `switch` paid up to 20 calls to reach the last one.
Cranelift's `Switch` builder (§11) picks `br_table` or a binary search itself:

```rust
let mut sw = Switch::new();
sw.set_entry(case_value, case_block);
sw.emit(&mut builder, discriminant, default_block);
```

Only applies when every case test is an integer constant; keep the existing
chain as the fallback for the general (expression-test) case.

### The exactness guard (the whole risk of this step)

TS `number` is an f64, so the discriminant arrives as `Float64` and must be
narrowed to an integer key — and **truncating alone is wrong**: `switch (1.5)`
would enter `case 1:`. The emitted code converts to i64, converts BACK to f64,
and enters the table only when the round-trip is bit-exact. That single `fcmp`
also settles every awkward value correctly:

| discriminant | result | why |
|---|---|---|
| `1.5` | default | round-trip differs |
| `NaN` | default | compares false everywhere — and `NaN === NaN` is false in JS too |
| `±Infinity`, `1e300` | default | saturating convert clamps, round-trip differs |
| `-0` | `case 0` | round-trips to `0.0`, and `-0 === 0` in JS |

Duplicate case keys keep the FIRST body (JS first-match dispatch).

**Measured: 351 → 182 ms (1.93×)** on a 20-case switch over 5M iterations. IR
confirms 0 remaining `strict_eq` calls and one `br_table`. Every edge above was
cross-checked against Node (byte-identical) and pinned in
`tests/switch_int_table_edges.test.ts`.

Unlike step 1, this one is a real win: the cost removed was a CALL per case, not
a conversion around an already-cheap op.

---

## Step 3 — thread-local data + inline `random_f64`

**Status:** ⛔ BLOCKED — do not attempt as written · **Effort:** medium · **Risk:** medium

Two findings, both measured/verified, kill this step in its planned form.

**1. cranelift-jit cannot do TLS at all.** `cranelift-jit-0.131.0/src/backend.rs`
asserts on the path this step depends on:

```rust
assert!(!tls, "JIT doesn't yet support TLS");   // declare_data, declare_anonymous_data, define_data
```

`cranelift-object` supports it, so an AOT-only implementation would compile —
but `rts run` is the primary path, and it would PANIC. Divergent JIT/AOT codegen
is not an acceptable price for this.

**2. The call was never the cost.** Measured (AOT, `Measure-Command`, median of
7, 10M iterations):

| program | time |
|---|---|
| empty loop | 22 ms |
| loop + `math.random()` | 32 ms |

So **10M extern calls cost 10 ms — about 1 ns each.** An inline xorshift is
itself ~8 instructions, so the ceiling for this whole step is roughly 5–7 ms per
10M randoms. The premise in the original plan ("the blocker for inlining the
PRNG is its state") had the priority backwards: the state is the hard part and
the reward is small.

For calibration, a userland xorshift written in TS takes **194 ms** for the same
10M — 6× SLOWER than calling into the runtime. The extern call is not the
problem in this workload; f64 arithmetic in the interpreterless-but-boxed
userland path is.

### If it is ever revisited

The only JIT-viable shape is an extern that returns the ADDRESS of the
thread-local state, hoisted per function, with the xorshift inlined against that
pointer. That keeps ONE state shared with `math.seed` (a separate codegen-owned
state would silently break seeding). But a hoisted pointer is unsound across an
`await`: the function can resume on a different tokio worker and would then
write another thread's state — a data race, for ≤7 ms per 10M calls. It would
need to be restricted to functions with no suspension point, and it is not worth
that complexity today.

The blocker for inlining the PRNG is its **state**: it lives in a Rust
`thread_local!`. Cranelift can declare thread-local data directly (§14):

```rust
module.declare_data("__RNG_STATE", Linkage::Local, /*writable*/ true, /*tls*/ true)
```

With that, xorshift64 becomes ~10 instructions inline (load, 3 × `ishl`/`ushr` +
`bxor`, store, `ushr`, `fcvt_from_uint`, `fdiv`) instead of a call.

**Do NOT inline against a plain (non-TLS) global.** The threaded benchmark runs 8
workers on the same PRNG; a shared global would both corrupt the sequence and
reintroduce the cache-line contention that made parallel scaling negative.

This step is the enabler for treating the rest of `math` as primitives.

---

## Step 4 — `atomic` namespace → atomic IR

**Status:** ✅ done, but **not as planned** · **Effort:** medium · **Risk:** medium

**The premise was wrong, and measuring first is what caught it.** The atomic
value lives behind a HandleTable handle, and `with_entry` LOCKED the shard mutex
on every operation just to find its address. A namespace whose entire purpose is
lock-free concurrency serialized every op through a mutex.

Cost breakdown (AOT, `Measure-Command`, median of 7, 10M iterations):

| program | time | per op |
|---|---|---|
| empty loop | 22 ms | — |
| `+ atomic.i64_load` | 58 ms | 3.6 ns (lookup + a plain `mov`) |
| `+ atomic.i64_fetch_add` | 68 ms | 4.6 ns (lookup + `lock xadd`) |

The atomic instruction is ~1 ns of that. **Emitting it as Cranelift IR was never
the lever** — and it could not be emitted anyway, since the engine holds a
handle, not an address, and resolving the address is exactly the expensive part.

Contention is where it actually hurt: 4 threads × 2M `fetch_add` took **1034 ms**
against ~37 ms for the same 8M ops on one thread — NEGATIVE scaling, a lock
convoy rather than atomic contention.

**Shipped instead:** each accessor memoizes the last `handle → address` it
resolved, per thread, so repeated ops on one counter never touch the shard. The
safety argument (heap-stable `Box`, the lookup/use window already existed, no
`free` in the namespace, full-handle key defeats slot recycling, one memo per
type defeats reinterpretation) is written at the definition in
`rts-std/src/atomic/mod.rs`.

**Measured: 4 threads × 2M 1034 → 227 ms (4.6×).** Single-thread 10M is 68 → 69
ms — **unchanged**: an uncontended mutex was already cheap, so the residual
~3.3 ns/op is call and marshalling. This buys scalability, not single-thread
speed. `tests/atomic_ptr_memo.test.ts` pins what a naive memo corrupts silently.

### The `Atomics` primordial is a different job

`CLAUDE.md` lists `Atomics` (over SharedArrayBuffer) as primordial. THAT one is
genuinely inlinable with `atomic_rmw`/`atomic_cas`, because a typed-array element
has a base address and an index — no per-op handle lookup. It is blocked on step
5, not on this step.

`rts-std/src/atomic/mod.rs` implements the namespace in Rust. Cranelift has the
whole surface natively (§19): `atomic_load` / `atomic_store`, `atomic_rmw`
(Add/Sub/And/Or/Xor/Nand/Xchg/Umin/Umax/Smin/Smax), `atomic_cas`, `fence`, with
the full `MemoryOrder` set.

This is also the right foundation for the `Atomics` primordial that `CLAUDE.md`
lists, and it composes with the threading work.

---

## Step 5 — TypedArrays on a real buffer

**Status:** not started · **Effort:** high · **Risk:** medium · **Biggest win**

Today they are **"Vec-backed level A"** (`registry_build.rs:184`): a
`Uint8Array` is a `HandleTable` `Entry::Vec` of **PolyValue words**. So `a[i]`
costs an extern `VEC_GET` plus box/unbox — for a type whose entire purpose is a
contiguous, untagged byte buffer.

Target: back a TypedArray with a raw buffer and lower element access to a single
instruction (§12):

| access | instruction |
|---|---|
| `Uint8Array[i]` | `uload8` |
| `Int8Array[i]` | `sload8` |
| `Uint16Array[i]` / `Int16Array[i]` | `uload16` / `sload16` |
| `Uint32Array[i]` / `Int32Array[i]` | `uload32` / `sload32` |
| `Float64Array[i]` | `load F64` |
| stores | `istore8` / `istore16` / `istore32` / `store` |

Address is `base + (index << log2(elem_size))` — `ishl_imm` + `iadd`, both folded
by the e-graph when the index is constant.

Needs: a buffer-backed heap entry, a bounds check (`icmp` + `trapnz`, or a
`select` to `undefined` for JS semantics), and GC awareness of the buffer. This
is where the engine stops paying a function call per byte.

---

## Step 6 — SIMD for bulk operations

**Status:** not started · **Effort:** high · **Risk:** medium

Cranelift **does not autovectorize** (§22) — vector types must be emitted
explicitly (§20): `splat`, `shuffle`, `swizzle`, `vany_true` / `vall_true`,
`vhigh_bits`, and lane-wise arithmetic on `I8X16` / `I32X4` / `F64X2`.

Natural targets, all currently byte-at-a-time: `indexOf` / `includes` over an
array, substring search, `fill` / `copyWithin`, UTF-8 encode/decode.

Do this only after Step 5 — it needs the contiguous buffer to be worth anything.

---

## Step 7 — stack slots for non-escaping objects

**Status:** not started · **Effort:** very high · **Risk:** high

Every object lands in the `HandleTable` today. An object proven not to escape
could live in a `stack_addr` slot (§13): no allocation, no GC pressure, and the
register allocator may remove it entirely.

The blocker is analysis, not emission: **Cranelift does not do escape analysis**
(§22), so the HIR layer has to prove non-escape. Treat as a project, not a step.

---

---

## Step 8 — the API changed: `Intrinsic` enum → per-member `NativeEmit` (DONE, drain pending)

**Status:** ✅ mechanism shipped · **Effort:** medium · **Risk:** low · **Owner decision**

Steps 0–1 were built on `abi::Intrinsic`, a CLOSED enum. That was wrong in the
same way the doctrine says hardcoding is wrong: every new operation needed a
variant in `rts-engine` **plus a `match` arm inside `rts-codegen-new`** — engine-
side knowledge of a non-primordial. It also capped what an operation could be:
whatever the enum could name.

A member now carries its own emission instead:

```rust
// in rts-shared / rts-primitives, NEXT TO the spec that owns the operation
.member(native(
    func("sqrt", "__RTS_FN_NS_MATH_SQRT", Sig::new(vec![F64], F64), …),
    |b, args| { let [x] = args else { return None }; Some(b.ins().sqrt(*x)) },
))
```

The engine has ONE generic call site (`Lowerer::emit_native`). Adding a natively
emitted operation is now a change to a spec, never to the engine.

### Contract

- `NativeEmit` is a **fn pointer**, not a boxed closure: `Member` stays
  `Clone`/`Send`/`Sync` with no allocation, and non-capturing closures coerce
  automatically. An emitter that needed captured state would be program-dependent
  and does not belong on a member spec.
- **Operands arrive already coerced to the member's declared `Sig`**, through the
  same `Lowerer::coerce` the call path uses. Without this an `Int32` operand
  would reach an f64 instruction and produce invalid IR — and every emitter would
  end up re-implementing coercion, forking the Pilar-3 authority.
- **`None` falls back to the ordinary call.** `symbol`/`fn_ptr` stay registered,
  so reflection / FFI / an unproven receiver keep working. An emitter can only
  make a site faster, never break one.
- A `Tagged` operand is not provably numeric and takes the call.
- **Never reachable from userland (security, owner decision):** an emitter is
  spec/engine surface — not in `rts.d.ts`, not a callable TS namespace.

**Cranelift is allowed below the engine** (owner decision, 2026-07-20).
`rts-engine`/`rts-shared`/`rts-primitives` may depend on `cranelift-codegen`/
`cranelift-frontend`. Cranelift is pure Rust with no C dependencies, so the
universal layer still builds for every target including wasm/browser; the older
"no compiler backend below the engine" reading does not apply to it.

### Done (commit 9eeb192b)

All 20 operations migrated to per-member emitters (6 math + 13 num + sqrt), then
`abi::Intrinsic`, `Member.intrinsic`, `NamespaceMember.intrinsic`,
`Lowerer::emit_intrinsic` and its six helpers, and `ResolvedCall.intrinsic` were
all DELETED. The engine now has exactly one inline-emission entry point,
`emit_native`. `ReceiverIdentity` (the one non-arithmetic variant) was verified
dead — `.valueOf()` routes through the ambient prelude `.ts` class, not the tag —
so it went with the enum.

---

## Step 9 — userland arithmetic through emitters (the xorshift row)

**Status:** not started · **Effort:** medium · **Risk:** medium · **Highest-value next**

Measured: a xorshift64 written in **TS userland** takes **194 ms** for 10M
iterations, while calling `math.random` into Rust takes **31 ms**. Userland
arithmetic is 6× SLOWER than an extern call — so the cost is in how the engine
lowers `+ - * / %` and shifts on `number`, not in call overhead.

Two known contributors, both now addressable with step 8's mechanism:

1. **`%` on f64** goes out to `fmod` instead of being emitted.
2. **Int-shaped work round-trips**: step 1 measured 16 `fcvt` instructions
   surviving in a bit-op loop, because TS `number` is an f64 and every integer
   operation converts f64→i64→f64 around the op.

Keeping values in **f64 as the canonical base** (which is what `PolyValue`
already is — a NaN-box over f64) and emitting f64 operations natively avoids that
detour. This is the row of the README benchmark table that the current numbers
misrepresent (see the caveat below), and the one where RTS should genuinely win.

---

## Step 10 — the JIT's fixed ~185 ms (NOT a codegen problem)

**Status:** not started · **Effort:** medium · **Risk:** low · **Biggest end-user gap**

From the README's own table, the JIT − AOT delta per benchmark:

| bench | JIT | AOT | delta |
|---|---|---|---|
| MC π 10M (xorshift) | 316 ms | 120 ms | 196 ms |
| MC π 10M (`Math.random`) | 305 ms | 120 ms | 185 ms |
| π decimal ~30 digits | 201 ms | 15 ms | 186 ms |
| MC 10M threaded | 266 ms | 81 ms | 185 ms |
| π Machin f64 | 197 ms | 14 ms | 183 ms |

**Constant ~185 ms across workloads of completely different size and shape.** A
cost that does not scale with work is not generated-code quality — JIT and AOT
share the same lowering and the same `opt_level`. It is fixed startup.

That matters for prioritisation: **every other step in this file is steady-state
work and cannot close this gap.** On the rows where RTS loses to Bun/Deno, the
delta IS the entire loss — the steady-state number already wins.

Two candidate causes, neither confirmed:

- **Compile time** (registry build + prelude lowering per run). The `.o`/`.ometa`
  cache and a precompiled prelude are the direct attack.
- **DLL loading** (owner's hypothesis — the same class of problem that the AOT
  `/DELAYLOAD` work fixed). Weak counter-evidence: `rts --version` measured 12 ms
  after delay-load, and that path already pays process + DLL startup. But
  `--version` does not build the registry or touch the JIT, so it does not settle
  it.

**Measure the phase breakdown (`RTS_TIMING=1`) before choosing.** Three
hypotheses in this campaign were implemented before being measured and all three
were wrong; do not add a fourth.

### MEASURED (2026-07-21) — it is prelude compilation, NOT DLL loading

`RTS_TIMING=1 rts run` on a trivial `console.log("hi")` (median of 2):

| phase | ms |
|---|---|
| prelude parse+lower | **50.4** |
| populate_module (clif) | **38.9** |
| machine-compile (parallel) | 20.8 |
| build fn IR + main | 13.4 |
| merge programs | 11.1 |
| prune prelude | 8.9 |
| user lower | 0.2 |
| (jit symbol harvest/install, make_module, finalize) | < 2 total |

The whole ~140 ms is spent COMPILING THE 251 KB PRELUDE — parsed, lowered,
pruned, and machine-compiled on EVERY run, for a program that does one
`console.log`. `user lower` is 0.2 ms. **The owner's DLL hypothesis is ruled
out**: DLL/process startup is already paid and does not appear here; the cost is
recompiling a FIXED input every run.

So the fix is the `.o`/`.ometa`-style prelude cache (the very first request of
this campaign): compile the prelude ONCE and reuse it. The ~90 ms of
prelude-specific work (parse+lower + its share of populate/machine-compile) is
what a cache removes; `user lower` + the user program's own compile is the
irreducible remainder (a few ms for a small program).

**Decision (owner, 2026-07-21):** go for the resident-precompiled-prelude
(machine code + metadata), targeting ~23 ms. Binary size is a non-concern (up to
~100 MB), so pruning is dropped and the WHOLE prelude is precompiled. Do it in
slices, Slice 1 first, each measured behind a behaviour-neutral fallback.

### The plan (agent-designed, key facts verified)

Verified before starting: the prelude→user dependency is all PLAIN DATA (Class
table, gcell/global ids, interned shape id→keys, ambient fn names) — no Cranelift
handles or pointers, so it is serializable. And the id assignment is
DETERMINISTIC: `class_decls` come from `program.items.iter()` (source-ordered
AST, `mod.rs:470`), `topo_order` uses its HashMaps for lookup only (output driven
by the source-ordered loop, `class/inherit.rs:45`), so shape ids intern in a
stable order across runs — confirmed empirically (prune 534/297 identical over 3
runs). This determinism is the make-or-break for caching baked ids; it holds.

Phase fate (measured now → projected), resident-code design:

| phase | now | after |
|---|---|---|
| process/runtime floor | 14 | 14 (hard wall) |
| prelude parse+lower | 47 | ~0 (deserialize metadata, 2–4) |
| prune prelude | 7 | 0 (dropped) |
| merge programs | 9 | 1–2 |
| build fn IR + main | 13 | ~0.3 (user only) |
| machine-compile | 21 | 1–3 (user only) |
| **total** | **~123** | **~23 (band 23–26)** |

Honest floor: after the prelude compile is gone the **14 ms process floor**
dominates; sub-20 ms would need separately shrinking runtime/tokio/GC init, out of
scope here.

### Slices (each measured, each behind a fallback)

- **Slice 1 — cache the lowered prelude METADATA + HIR, still machine-compile it.**
  ✅ DONE + ON by default (`73623f0f`). serde on the HIR / `ClassTable` /
  `LoweredProgram`; a shape-registry snapshot/seed API (`d48318e3`); a disk cache
  in `prelude_cache.rs` keyed by prelude-text hash, hooked in
  `build_program_for_prelude`. Kills the 47 ms parse+lower (→ ~4 ms deserialize).
  **Measured 123 → 79 ms (1.56×).** One correctness bug found + fixed: the cache
  seeds the shape registry, which is illegal for a NESTED compile
  (`new Function`/eval) that runs while the outer program's shapes are live — so
  the cache is gated to the TOP-LEVEL compile (`global_shape_count()==0`) and a
  nested compile re-lowers as before. Full suite 723/8 = baseline under the cache.
- **Slice 2 — resident prelude machine code.** A `rts-prelude-baker` workspace bin
  compiles the prelude alone to `prelude.o` (ObjectModule, `aot_str` ON,
  `Linkage::Export`), linked into `rts.exe`; its fns register in
  `adapter_symbols::jit_symbols` and user code declares them `Import`. Partition
  ids (seed shapes by id, offset user gcells by G). Kills machine-compile +
  build-IR + prune. **~79 → ~23 ms.** The big, hard slice.

  **Feasibility spike DONE — verdict GO, on path B (baker), not path A (byte cache).**
  Proven by experiment: `define_function_bytes` with captured `bytes+relocs`
  replays correctly into a fresh `JITModule` same-process (a `caller→callee`
  round-trip returned the right value). BUT FuncIds are NOT deterministic across
  programs (measured: max `User` reloc index shifted 723→726 between two user
  programs) — because `prune_prelude` keeps a DIFFERENT reachable prelude subset
  per user program, so cached `User{index}` relocs (100% of relocs are
  `User{FuncId}`) are program-specific. Path A would need a per-run mini-linker
  remapping 8000+ relocs by name; **path B gets name resolution for free from the
  real linker + `JITBuilder::symbol`**, so the FuncId fragility vanishes. `aot_str`
  MUST be ON for the baked prelude (JIT currently bakes process-local string
  handles). Unproven, to validate in commit-1: cross-process baked prelude end to
  end, GC stack-map coverage for resident frames, the exact ms saved, reified
  prelude fn values + shape seeding with resident code.

  **PROGRESS (in flight).** The ahead-of-time + build-link foundation is DONE and
  validated; the run-path CONSUMER remains.
  - *Commit 1 (`cdd5fbbe`) + hardening (`45e94b32`)* — `rts-prelude-baker`
    workspace bin + `front::run::bake::bake_prelude()`: lowers the WHOLE prelude
    (unpruned), emits `prelude.o` with `Linkage::Export` (a `BAKE_EXPORT`
    thread-local `populate_module`/`thunk` consult) + a `PreludeManifest` (whole
    lowered prelude, shape snapshot, error-class snapshot, exported-symbol set,
    gcell count, prelude-text hash). Prelude `__rtsn_main` → `__rtsn_prelude_main`
    (no collision with the user main). A `#[ignore]` determinism test proves the
    id-bearing data (shapes, gcell ids, symbols) is byte-identical across bakes.
    Bakes **1735 fns / 82 shapes / 8 gcells** → ~1.4 MB object. Adversarially
    reviewed (export set exact, main rename safe, privacy gate matches
    `merge_programs`, `Linkage::merge(Export,Local)=Export` relied on + pinned in a
    comment); thread-locals Drop-guarded so a mid-bake panic can't leak the flag.
  - *Commit 2a (`bf2afd37`)* — the baker also emits `prelude_symbols.rs`, a
    `@generated` `prelude_symbols() -> Vec<(&str,*const u8)>` (via `#[link_name]`
    aliases so any symbol name stays a valid ident), the resident address table
    compiled into `rts.exe`.
  - *Commit 2b (`dfacb573`)* — OPT-IN build wiring, guarded so the DEFAULT build is
    unchanged. `RTS_PRELUDE_DIR` → `build.rs` links `prelude.o` into `rts.exe`
    (`cargo:rustc-link-arg`; its undefined `__RTS_*`/`__rtsadp_*` resolve against
    rts-runtime in the SAME link — ONE runtime instance, the coherence a prelude
    DLL could NOT give: a DLL bundles its own runtime copy → two heaps/handle
    tables → incoherent) and `include!`s the table + manifest; `main` installs both
    via `rts_cli::install_resident_prelude` → `resident::install`. Unset/absent →
    inert stub (`prelude_symbols()` empty) → the run path keeps the fallback.
    **Validated:** default `cargo build` green + `rts run` unchanged; a resident
    build LINKS (all 1735 symbols resolve, rts.exe +1.95 MB) and RUNS correctly
    (resident symbols linked-but-unused — the consumer is not wired yet).
  - *Commit 3 (REMAINING — the hard part)* — the run-path CONSUMER. When
    `resident::is_installed()` AND `manifest.prelude_hash == key(includes_prelude())`
    AND top-level (`global_shape_count()==0`): seed shapes + error classes from the
    manifest, register `resident::symbols()` on the `JITBuilder`, build ONLY the
    user program with the prelude's ambient class/fn metadata, but declare every
    referenced prelude fn `Linkage::Import` (resolving to the resident address)
    using the manifest's prelude `FnSig`s — NOT compiling any prelude body/thunk —
    offset user gcells by `manifest.gcell_count`, and call resident
    `__rtsn_prelude_main` before the user main. Every path falls back on hash
    mismatch / missing sig / nested compile. Still to validate: GC stack-map
    coverage for the resident frames, reified prelude fn values against resident
    code, and the actual **~79 → ~23 ms**. This is the big, SIGILL-sensitive slice;
    it is fully gated (the default stub build is untouched regardless).
- **Slice 3/4** — drop pruning + trim merge from the run path once the prelude is
  fully resident.

### Fallback (behaviour-neutral, permanent)

If the embedded artifact is absent, its hash key mismatches the current prelude
text, or a prelude symbol fails to install, `build_with_includes` takes today's
path (`build_program(prelude)` + `merge_programs`). Same behaviour as now; also
what `rts compile` (AOT) keeps using. The honesty floor is guarded by a CI diff of
stdout under baked-vs-fallback across a fixture set.

### Highest-severity risk

Baked shape ids (immediates in prelude code) and gcell ids must be seeded at run
time in the EXACT order the baker interned them — seed by explicit id, never
re-intern, and assert `global_shape_count() == expected` after seeding. A mismatch
is a silent miscompile / SIGILL (the class of bug `rts-adapters/src/state.rs`
documents from mis-timed resets).

---

## Ordering

Original: 0 → 1 → 2 → 3 → 4 → 5 → 6 → 7.
Done so far: 0, 1, 2, 4, 5a, 5b, 8 (3 blocked; 4 done by another route).
Remaining: **10 → 9 → 6 → 7**.

If the goal is "RTS JIT should beat Bun/Node/Deno", **10 comes first** — it is the
only step that touches the number the user actually sees.

### What the first four steps actually taught

The plan's premise — "a call is the dominant cost" — held for exactly one of
them. Measured cost of the thing removed:

| step | what was removed | result |
|---|---|---|
| 2 | one extern CALL per switch case | **1.93×** |
| 0 | a call around a single machine instruction (`sqrt`) | 19% on a sqrt-heavy bench |
| 1 | a call around a cheap bit op, but the f64↔i64 conversions stayed | 2% |
| 3 | (a ~1 ns call) | not worth doing, and JIT-blocked anyway |
| 4 | a MUTEX per op (not the atomic instruction) | **4.6× under 4 threads** |

The pattern: **removing a call pays when the call was doing dispatch or was
wrapped around real work — not when it wrapped one cheap instruction whose
operands still need converting.** For arithmetic-heavy TS the remaining cost is
representational (every `number` is an f64 that round-trips through i64), which
is Repr/typed-locals work, not intrinsic work.

That is why 5 (TypedArrays: a function call *per element access*, plus box/unbox)
is now the next target rather than 4.

### A caveat about the README benchmark table

The row "Monte Carlo π 10M (same xorshift algorithm)" is **not** a like-for-like
comparison: it pits an RTS builtin implemented in Rust against a userland BigInt
loop in the other runtimes. Measured RTS userland against Bun userland, they tie
(4747 vs 4820 ms). The 43× in that row is not a language-speed result and should
not be used to prioritise work.

## Status board

| # | Step | Effort | Measured | State |
|---|---|---|---|---|
| 0 | intrinsic path (engine had none) | medium | `pi_machin` 16 → 13 ms | ✅ done |
| 1 | `num` bit ops → instructions | low | call gone; 142 → 139 ms (2%) | ✅ done |
| 2 | `switch` → jump table | low | **351 → 182 ms (1.93×)** | ✅ done |
| 3 | TLS + inline `random_f64` | medium | call is only ~1 ns; ceiling ~7 ms/10M | ⛔ blocked (JIT has no TLS) |
| 4 | atomics: mutex off the fast path | medium | **4 threads 1034 → 227 ms (4.6x)** | ✅ done (not via IR) |
| 5a | level-B allocation storm (view_parts + write index) | medium | **write 9.4×, bench 6× (7.5→1.26 s)** | ✅ done |
| 5b | native `uload8`/`istore8` + hoisted base | high | **bench 1.26 s → 319 ms; 7.5 s → 319 ms overall (~23×), ~2.5× off Node** | ✅ done |
| 6 | SIMD bulk ops | high | — | not started |
| 7 | stack slots (escape analysis) | very high | — | not started |
| 8 | `Intrinsic` enum → per-member `NativeEmit` | medium | enum DELETED, all 20 ops on specs | ✅ done |
| 9 | userland arithmetic via emitters (`%`, int round-trips) | medium | userland xorshift 194 ms vs 31 ms call | after 10 |
| 10 | the JIT's fixed ~185 ms | medium | constant across all 5 benches | **NEXT — measure phases first** |

### Step 5a — the allocation storm (DONE, commit 4da2ae6d)

Level-B typed arrays (`new Uint8Array(new ArrayBuffer(n))`, the standard idiom)
cost **~3.7 µs per element access**. An agent investigation located the cause by
IR + code reading + a GC-disable probe:

- `view_parts` ran on every `view[i]` and asked for its four `__ta_*` fields BY
  NAME through full `__rtsadp_obj_get`. Each name went through `intern_poly` →
  `STRING_NEW`, a NON-deduped heap allocation — four fresh string handles per
  access — and each `obj_get` relocked the shard and re-cloned the shape's key
  vector. ~50 mutex locks + ~12 allocations to read four integers that sit in
  fixed slots. The string flood pushed the handle table past its GC floor, so
  mark+sweep then ran every 256 allocations — paid twice. **A read-only loop with
  GC disabled aborted at 5,000,000 live handles.**
- The WRITE path also did `key_text(key).parse::<i64>()` — stringify a numeric
  index and reparse it, per write.

Fixed both allocation-free: `view_parts` reads the five fixed slots under one
`with_entry` and validates slot 0 against the interned view shape (the
hidden-class contract); the write arm takes the index off the key PolyValue via
the shared `dyndispatch::array_index_key`. Measured (JIT): writes 6.09 → 0.65 s
(9.4×), the 20.5M bench 7.5 → 1.26 s (6×), and the GC-disabled loop now runs
billions of accesses without growing the table.

Two methodology notes from this step, worth keeping:

- **AOT measurements need `cargo build -p rts-runtime` first.** AOT links the
  prebuilt runtime archive; several "no movement" measurements earlier in this
  campaign were invalid because of a stale archive. Measure on JIT (live code)
  unless you have just rebuilt the archive.
- The earlier claim that the positional fix "measured 2654→157 ns but did not
  move end-to-end" was itself a stale-archive artifact: on JIT the same fix moves
  end-to-end 6×.

### Step 5b — native element load/store (DONE: 491ab47e / 36993e7f / bc9ac26f)

Landed in slices, each measured. The mechanism is NOT `NativeEmit` (that is for
member calls; `a[i]` is index lowering) — it is a dedicated fast-path arm in
`lower_index`/`lower_index_assign` next to the `HeapShape::Array` arm.

- **Slice 1** — `__rtsadp_ta_view_base_len(view) -> (base_ptr, count)`, the raw
  resolver, with the pointer-soundness argument (buffer never resized/moved).
- **Slices 2–3** — a new `HeapShape::TypedArrayView { elem_log2, signed, float }`
  proven at the ctor site (where `is_buffer_view` was already detected), and
  `front/run/ta_native.rs` emitting a bounds check + `base + (i << log2)` +
  width/kind load/store for all 8 kinds. **bench 1.26 s → 881 ms.**
- **Slice 5 (hoist)** — base+count resolved ONCE at the ctor site into two
  Cranelift Variables, read per access with `use_var`. Removed the residual
  per-access lock; the big win: **bench 881 → 319 ms.**

**Overall step-5b: 7.5 s → 319 ms (~23×), from ~58× off Node down to ~2.5×** on
wall clock (most of the residual is process startup, not the loop). Correctness
byte-identical to Node across `tests/typedarray_view_element_ops.test.ts`
(cross-view sharing, wrap, widths, OOB, interleaved-views cache).

Deferred: views received as a param / call return / after reassignment stay on
the dynamic path (never proven a `TypedArrayView`). Separate pre-existing bug
(confirmed on HEAD): the `.buffer` accessor on a level-B view returns the wrong
handle.

