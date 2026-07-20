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

## Ordering

Original: 0 → 1 → 2 → 3 → 4 → 5 → 6 → 7.
Revised after measuring 0–3: **0 → 1 → 2 → 5 → 6**, with 3 blocked and 4
deprioritised.

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

## Status board

| # | Step | Effort | Measured | State |
|---|---|---|---|---|
| 0 | intrinsic path (engine had none) | medium | `pi_machin` 16 → 13 ms | ✅ done |
| 1 | `num` bit ops → instructions | low | call gone; 142 → 139 ms (2%) | ✅ done |
| 2 | `switch` → jump table | low | **351 → 182 ms (1.93×)** | ✅ done |
| 3 | TLS + inline `random_f64` | medium | call is only ~1 ns; ceiling ~7 ms/10M | ⛔ blocked (JIT has no TLS) |
| 4 | atomics: mutex off the fast path | medium | **4 threads 1034 → 227 ms (4.6x)** | ✅ done (not via IR) |
| 5 | TypedArrays on a real buffer | high | — | next — biggest win |
| 6 | SIMD bulk ops | high | — | not started |
| 7 | stack slots (escape analysis) | very high | — | not started |
