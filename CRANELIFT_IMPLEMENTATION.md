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

## The mechanism that already exists

The engine has an intrinsic path: a `Member` registered with
`Some(Intrinsic::…)` is emitted as IR instead of `call <symbol>`.

```rust
// rts-shared/src/math/mod.rs — sqrt IS an intrinsic:
func("sqrt", "__RTS_FN_NS_MATH_SQRT", …, Some(Intrinsic::Sqrt))

// …random_f64 is NOT (despite CLAUDE.md listing it as one):
func("random_f64", "__RTS_FN_NS_MATH_RANDOM_F64", …, None)
```

So each step below is: add an `Intrinsic` variant, emit the IR, flip the
registration. The pattern is proven by `sqrt`.

**Note:** `CLAUDE.md` claims `random_f64` is an intrinsic. It is not — the doc is
stale on that line and should be corrected when Step 3 lands.

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

**Status:** not started · **Effort:** low · **Risk:** low

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

**Status:** not started · **Effort:** low · **Risk:** low

`front/run/switch.rs:65` emits a **chain of `brif`** — one compare per case, so a
20-case `switch` costs up to 20 sequential compares. Cranelift's `Switch` builder
(§11) picks `br_table` or a binary search itself:

```rust
let mut sw = Switch::new();
sw.set_entry(case_value, case_block);
sw.emit(&mut builder, discriminant, default_block);
```

Only applies when every case test is an integer constant; keep the existing
chain as the fallback for the general (expression-test) case.

---

## Step 3 — thread-local data + inline `random_f64`

**Status:** not started · **Effort:** medium · **Risk:** medium

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

**Status:** not started · **Effort:** medium · **Risk:** medium

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

1 → 2 (both mechanical, validate the path) → 3 (TLS unlocks math as primitives)
→ 4 → 5 (biggest single win) → 6 → 7.

## Status board

| # | Step | Effort | Measured | State |
|---|---|---|---|---|
| 1 | `num` bit ops → instructions | low | — | not started |
| 2 | `switch` → jump table | low | — | not started |
| 3 | TLS + inline `random_f64` | medium | — | not started |
| 4 | atomics → atomic IR | medium | — | not started |
| 5 | TypedArrays on a real buffer | high | — | not started |
| 6 | SIMD bulk ops | high | — | not started |
| 7 | stack slots (escape analysis) | very high | — | not started |
