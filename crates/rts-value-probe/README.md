# rts-value-probe

Measurement-only crate. It exists to answer two questions with numbers **before**
anyone changes the engine:

1. **Is the `PolyValue` NaN-box itself worth redesigning?** — kernel C compares
   the NaN-boxed word against a two-slot `{tag, value}` pair (the QuickJS-64 /
   Porffor-approach-2 shape) and against untagged `f64`, in a loop with no heap
   and no calls.
2. **Which of the proposed levers actually moves the number?** — kernels A and B
   run the `bench/objbench.ts` inner loop through a ladder of variants, each
   removing exactly ONE thing from the one before it, so the delta between two
   adjacent rows is that lever and nothing else.

```bash
cargo run --release -p rts-value-probe     # RELEASE ONLY — a debug number is not a number
cargo test -p rts-value-probe              # the model/parity tests
```

## What it replicates

The probe links **nothing** from RTS on purpose — a number it produces cannot be
blamed on unrelated runtime work. What it copies, and from where:

| probe | real code | what is preserved |
|---|---|---|
| `poly.rs` | `adapters/value/layout.rs`, `heap/poly.rs` | `BOX_BASE`, tags, `number_result` re-tightening |
| `slab::sharded` | `heap/handles.rs`, `heap/payload_ops.rs` | 32 shards, `Mutex` per shard, `payload & 31` → shard, `payload >> 5` → slot, `Box<Vec<i64>>` object |
| `rt::probe_vec_get_locked` | `__rtsn_vec_get_by_payload` | one shard lock per field touch, opaque `extern "C"` call |
| `rt::probe_adp_mul/add` | `genops_arith::arith` | `to_number` ×2 → `f64` op → `number_result` |
| object layout | `obj.rs:97-116` | slot 0 = shape id, fields at `1 + slot_index` |
| ISA flags | `module_jit.rs:70-84` | `opt_level=speed`, `preserve_frame_pointers`, verifier off |

Every variant is validated by **checksum**: all rows of a kernel must compute the
same sum, or the row is printed with `CHECKSUM MISMATCH` and the number is void.

## Measured (2026-08-01, x86-64 Windows, release, medians of 7)

`sizeof(Slot) = 56 bytes`. Every row of a kernel produced the same checksum.

### The one-line summary

Nothing in this table is about the NaN-box. Every large number is one of three
things: **an extern call where a load would do**, **a shard `Mutex` on a read**,
or **an allocation/copy that did not need to happen**.

| primordial | today | best variant | factor |
|---|---|---|---|
| Object — field read | 15.43 ns | 1.18 ns | **13×** |
| Object — construction | 100.65 ns | 0.73 ns | **138×** |
| Object — dictionary read | 63.82 ns | 1.00 ns | **64×** |
| Array — element read+write | 17.69 ns | 0.70 ns | **25×** |
| String — `s += "x"` (10k) | 53.68 ms | 0.34 ms | **158×** |
| String — `===` | 19.83 ns | 1.18 ns | **17×** |
| Boolean — `if (x)` | 3.25 ns | 1.07 ns | **3×** |
| Number — tagged int `+` | 7.29 ns | 0.70 ns | **10×** |
| Operators on Tagged (`& << >>> *`) | 5.8–8.1 ns | 0.7–1.9 ns | **4–8×** |
| Operators — `%` | 8.23 ns | 3.35 ns | **2.5×** |
| Operators (`=== == <`) | 3.0–3.2 ns | 0.9 ns | **3.5×** |
| **value representation** | **1.79 ns** | **0.73 ns** | **2.4×** |

### Kernel A — `s = s + p.x * p.y`, 3M iterations, no allocation

| variant | ns/iter | delta from the row above |
|---|---|---|
| A0 today — locked call ×2 + generic call ×2 | 15.81 | — |
| A1 +proven per-slot `Repr` (arith inline) | 10.84 | **−4.97** |
| A2 +inline tag guard, Tagged kept | 11.49 | (−4.32 vs A0) |
| A3 +no shard lock on the read | 6.30 | **−4.54** |
| A4 +direct-addressed load (no call) | 1.03 | **−5.27** |
| A4g +IC shape guard (the honest IC hit) | 1.16 | +0.13 |
| A5 +escape analysis (object gone) | 0.70 | −0.33 |

A0 → A4g is **13.6×**. The three big steps are worth roughly the same (~5 ns each)
and they are INDEPENDENT: proving the Repr, removing the lock, and removing the
call each pay off on their own.

### Kernel B — `const p = new P(i, i+1); s = s + p.x * p.y`, 1M iterations

| variant | ns/iter |
|---|---|
| B0 today — locked slab alloc + locked reads + generic ops | 271–306 |
| B1 arena bump alloc + direct load + inline arith | 2.97 |
| B2 escape analysis — no allocation at all | 0.71 |

**~100×.** Allocation through the sharded-`Mutex` slab is ~300 ns of the ~305,
i.e. it is not one cost among several — it is the whole kernel.

### Kernel W — construction + field writes: `new P(f0..f3)` then read back, 50k objects

`cargo run --release -p rts-value-probe -- w`. The write half of kernel H, on the
same movable block layout — `RTS_CLASS_IMPLEMENTATION.md` §7 C0, which blocks C2
until it exists. Each row removes exactly one thing; the read-back is the
checksum, so no row can quietly store less.

| variant | ns/iter | what changes |
|---|---|---|
| W0 today — `vec_new_object` + one LOCKED push per field | TODO(measure) | — |
| W1 block alloc, direct store | TODO(measure) | −the lock and the `Box<Vec<i64>>` |
| W2 store via handle | TODO(measure) | −the handed-back address; recompute it in IR |
| W3 +card-mark barrier | TODO(measure) | +one card mark per field store |
| W4 region, const base | TODO(measure) | −shard routing; base is an `iconst` |
| W5 filled at alloc | TODO(measure) | −the separate store pass |

Fill these in from a release run; do not quote a remembered figure. The question
C0 asks is whether W1/W2 move construction off the ~300 ns W0 is expected to
reproduce — if they do not, cheap reads (kernel H) do not move class-heavy code.

### Kernel M — method call + `this`: `s = s + p.sum()`, 3M iterations (2026-08-02)

Measured in the REAL engine first (release, 3M iters, `class P {x, y}`):
`p.x` = 13.0 ns, `p.getx()` = 14.3 ns, `p.sum()` (two fields) = 26.3 ns, and a
free function call = 2.0 ns. So a method costs ~1.3 ns more than the field read
it wraps, and two fields cost exactly twice one field. The emitted body is
`band this, 0xffff_ffff_ffff` + one `call __rtsn_vec_get_by_payload` per field.

| variant | ns/iter | what it removes |
|---|---|---|
| M0 today — real call, tagged `this`, 2 locked field calls, generic `+` | 16.29 | — |
| M1 +proven Repr — `+` inline | 10.93 | the generic-arith call |
| M2 +untagged `this` — receiver is the raw payload | 10.97 | the `band` |
| M3 +no lock — unlocked field calls | 6.27 | the shard `Mutex` |
| M4 +`this` in register — receiver is an ADDRESS, fields are loads | 2.08 | the per-field call |
| M5 +method inlined — IC shape guard + 2 loads, no call | 1.13 | the method call |
| M6 +escape analysis — object gone | 0.69 | the object |

**14.4×, and none of it is dispatch.** The two levers people reach for first are
the two that measure zero and one: untagging `this` is **+0.04 ns (free — noise)**,
and the method call itself is **~0.95 ns**. Method dispatch is already a direct
monomorphic call. What a class costs is `this.field`: the shard `Mutex` is 4.66 ns
and the opaque call-instead-of-load is 4.19 ns, i.e. **8.85 of M1's 10.93 ns (81%)
is field representation**. This is kernel A's conclusion arriving through the
method path — the receiver being `this` changes nothing.

### Kernel H — object management: today vs Static-Hermes-shaped, 3M iterations (2026-08-02)

`s = s + p.x + p.y`, same workload as kernel M. Static Hermes is the closest
existing system to RTS — nominal typed classes, slot indices assigned at
declaration, `PrLoad` by constant index with no guard, ICs quarantined to the
untyped path. RTS already resolves `this.x` to a compile-time constant slot
(`obj.rs:608-636`); what it does with that constant is a `call` that takes a
shard `Mutex` and dereferences a `Box<Vec<i64>>`.

Hermes emits a load from a RAW POINTER. **RTS cannot**: the conservative GC
scanner decodes `gen|slot|shard` handle words and validates them against the
live `HandleTable`, and the threading model is defined on "payload = slot index,
never a pointer". So the row that matters is H3 — handle→address computed in
PURE IR (`base = shard_bases[payload & 31]; addr = base + (payload >> 5)*STRIDE`),
one extra hoistable load against a call plus a mutex.

| variant | ns/iter | delta | what it removes |
|---|---|---|---|
| H0 today — `call __rtsn_vec_get_by_payload` | 11.20 | — | — |
| H1 −shard `Mutex` | 6.87 | **−4.33** | the lock |
| H2 −`Box<Vec<i64>>` — inline slots, still a call | 5.29 | −1.58 | one indirection |
| H3 −the call — handle→addr in pure IR, guarded unbox | 1.72 | **−3.57** | the opaque call |
| H4 +`Repr` on the field — tag guard gone | 1.49 | −0.23 | the guarded unbox |
| H5 overflow bag — the `(p as any).z` fallback | 1.74 | +0.25 vs H4 | (prices the fallback) |
| H6 escape analysis — object gone | 0.69 | −0.80 | the object |
| H7 chunked storage — H4 through a chunk table | 1.54 | **−0.00** | (prices the precondition) |
| H8 packed stride 32 B — H4 with 4 words not 8 | 1.51 | −0.00 | (prices the stride) |
| H9 region, base as `iconst` — no shard routing | 0.99 | **−0.52** | the shard-base load |
| H10 **MOVABLE** — one slot indirection, blocks relocated | 1.50 | **−0.01 vs H4** | (prices regional GC) |
| H11 movable + region | 0.99 | **−0.00 vs H9** | (both at once) |

**7.9× (11.39 → 1.45), 16.5× to the floor. Handle identity is not what costs.**
The pure-IR address computation survives its extra load and still beats the call
by 3.57 ns — so the GC and threading constraints, which forbid a raw pointer,
do **not** forbid the Hermes result. The two costs are the shard `Mutex` (4.83)
and the opaque call (3.57); the `Box` is a third of either.

Three more rows exist because each prices a thing that could have killed the
design:

- **H5 — the property-bag fallback is affordable.** The census's HARD #4
  (`(p as any).z = 1` is legal today, and a fixed-offset struct has nowhere to
  put `z`) needs an overflow arm. It costs **0.25 ns** over a direct slot,
  against **63.82 ns** for the dictionary read in kernel OBJ. Fixed slots + an
  overflow bag is a real option; dictionary-mode demotion is not the only way to
  stay correct.
- **H7 — the stable-storage precondition is free.** H3/H4 quietly assume the
  shard base never moves, which a growing `Vec` cannot promise. Real storage has
  to be a chunk list. Routing through a chunk table costs **nothing measurable**
  (1.66 vs 1.49, inside run-to-run variance) because the two-level index
  collapses into ONE flat table load — `table[shard * CHUNKS + chunk]` — so the
  extra cost is a shift and a mask, not a second dependent load.
- **H9 — killing the shard-base load is the biggest lever left.** With one
  thread-local region the base is an `iconst` (what Dart's reserved `GDT`
  register and V8's pointer compression each buy), and the row lands at
  **0.90 ns — 1.65× faster than H4 and only 1.3× off the escape-analysis floor.**
  The payload is still a slot index, not a pointer, so GC and region migration
  are unaffected. This is the threading model's "shards are proto-regions"
  cashed out.

**H10/H11 — the regional-GC question, and the one that nearly sank the design.**
H3/H4/H9 derive an object's address from the HANDLE BITS. That is incompatible
with what the GC docs actually plan: `rts-threading-model.md` promotes by
"re-homing the entries into shared shards and updating the slots", and
`gc-generational-design.md` says the whole architectural advantage is that
"moving = update slot→address in the HandleTable (**the indirection makes this
≈ free**, no pointer-patching)". A handle-derived address deletes exactly that
indirection, so promotion or a nursery copy would change the handle and strand
every live copy of the word.

H10 keeps ONE indirection — but as a plain word load, not a `Box<Vec<i64>>`
deref behind an opaque call holding a `Mutex`:

```text
stab  = load [SLOT_TABLES + shard*8]   ; loop-invariant
block = load [stab + idx*8]            ; THE indirection — one word
x     = load [block + 8*(1 + slot)]
```

Every object's block is **actually relocated** into a to-space before the row
runs, and read back through the same handle — so the checksum is evidence that
the handle survived a move, not just a load count. Result: **H10 = 1.50 vs H4's
1.51, and H11 = 0.99 vs H9's 0.99. The movable form is free.** Moving an object
is one word written, no live word rewritten, and reads cost the same as the
non-movable layout. The regional/generational plan and the Hermes-shaped field
access are not in tension.

**But that "free" has a precondition worth stating loudly.** An earlier version
evacuated each block into its own `vec![0i64; 8]` — 1024 scattered mallocs — and
H10 swung between **4.4 and 8.2 ns** run to run. The indirection is free only
when the collector produces a CONTIGUOUS to-space. A non-compacting collector
that leaves survivors scattered turns the extra load into a cache miss and gives
back the entire win. That is an argument *for* the copying nursery, not merely
compatibility with it.

**H8 says the fixed stride is not free but does not bite here.** A 2-field class
in an 8-word (64 B) stride wastes 5 words; halving the stride to 32 B changed
nothing measurable at this working set (1024 objects — 64 KB vs 32 KB, both
resident). At a working set that exceeds L2 the packed row should win, and this
kernel does not test that. Do not read H8 as "stride is free".

### Kernel ARR — array element `s += a[j]; a[j] = a[j] + 1`, 3M iterations

| variant | ns/iter |
|---|---|
| R0 today — `VEC_GET` + `VEC_SET` locked calls + generic add | 17.69 |
| R1 +inline arithmetic | 9.24 |
| R2 +direct load/store, elements still boxed PolyValue words | 0.75 |
| R3 +packed `f64` elements (V8 `PACKED_DOUBLE_ELEMENTS`) | 0.70 |

**25×.** Note R2 → R3: unboxing the ELEMENTS is worth 0.05 ns. A `number[]`
storing tagged words instead of raw doubles is not the problem; reaching them
through a locked call is.

### Kernel OBJ — one property read, dictionary vs shape, 3M iterations

| variant | ns/iter |
|---|---|
| O0 `IndexMap` under lock, `key_text` allocates a `String` per read | 63.82 |
| O1 same map and lock, key already interned | 31.45 |
| O2 shape compare + fixed-offset load | 1.00 |

**64×**, and half of the dictionary cost is `key_text` alone — a `String`
allocation on every property read. CLAUDE.md describes O2 as what property access
does; O0 is what an untracked receiver actually takes.

### Kernel BOOL — `s += x ? 1 : 0` on a Tagged value, 20M iterations

| variant | ns/iter |
|---|---|
| T0 today — unconditional `call __rtsadp_to_boolean` | 3.25 |
| T1 +inline double test, call only on a miss | 1.52 |
| T2 proven `Repr::Bool` — no test at all | 1.07 |

Every `if`, `while`, `&&`, `||` and ternary over an unproven value is a real
call today. The inline guard is a pure-IR change with no representation change.

### Kernel INT — `s = s + a[j]` on tagged int32, 2M iterations

| variant | ns/iter |
|---|---|
| N0 today — `call __rtsadp_add` (int32 → `f64` → exactness check → re-narrow) | 7.29 |
| N1 +inline int32 guard → native `iadd` → rebox | 1.28 |
| N2 proven `Repr::Float64` — plain `fadd` | 0.70 |

N1 omits the overflow check a correct implementation needs, so it is a lower
bound, not an exact figure.

### Kernel STR-APPEND — `s = s + "x"`, 10k appends

| variant | total ms |
|---|---|
| D0 today — `STRING_CONCAT` through the snapshot layer | 53.68 |
| D1 same immutable result, each side copied once | 16.93 |
| D2 append in place (mutable accumulator) | 0.34 |
| D3 Rust `String::push_str` | 0.011 |

D0 → D1 is **3.2× for free**: identical observable behaviour, identical
allocation count for the RESULT — the snapshot layer just copies each operand an
extra time (`snapshot_entry` clones, then `snapshot_to_bytes` clones again).
D1 → D2 is the quadratic-to-linear change and is NOT free: JS strings are
immutable, so appending in place needs a proof that the old value is dead.

### Kernel STR-EQ — `===` on two 24-byte strings differing in the last byte

| variant | ns/iter |
|---|---|
| E0 today — handle identity, else content compare under the lock | 19.83 |
| E1 interned — equal content implies equal handle, one integer compare | 1.18 |
| E2 raw `memcmp`, bytes already in hand | 9.80 |

E2 is inflated by the `black_box` that stops the comparison folding to a
constant, so treat it as a conservative floor. E0 → E1 is **17×**, but interning
moves the cost to construction (a hash lookup per string built) — this is a
tradeoff, not a free win, and this probe does not measure the construction side.

### Kernel OPS — one row per operator, 10M iterations, Tagged operands

The engine already lowers all of these natively when both operands are proven
non-Tagged (`binop.rs:492` for the bitwise/shift family, `lower_compare` for the
relationals, `lower_arith` for the arithmetic). What is missing is the middle
rung: with a Tagged operand it goes straight to `box, box, call` with no inline
test for the secretly-monomorphic case (`binop.rs:596`, `binop_eq.rs:52`). Since
`Repr::Ref` is dead, "Tagged" is every value that came off the heap.

| operator | X0 today | X1 inline guard | X2 proven Repr | today → guard |
|---|---|---|---|---|
| `===` | 2.98 | 1.60 | 0.92 | 1.9× |
| `!==` | 2.98 | 1.59 | 0.91 | 1.9× |
| `==` | 3.20 | 1.60 | 0.92 | 2.0× |
| `!=` | 3.25 | 1.61 | 0.91 | 2.0× |
| `<` | 2.97 | 1.38 | 0.91 | 2.2× |
| `<=` | 3.20 | 1.37 | 0.90 | 2.3× |
| `>` | 3.19 | 1.40 | 0.91 | 2.3× |
| `>=` | 3.19 | 1.37 | 0.91 | 2.3× |
| `+` | 5.30 | 1.38 | 0.69 | **3.8×** |
| `-` | 5.92 | 1.39 | 0.69 | **4.3×** |
| `*` | 5.67 | 1.36 | 0.69 | **4.2×** |
| `/` | 5.94 | 1.38 | 1.13 | **4.3×** |
| `&` | 7.96 | 2.33 | 1.71 | **3.4×** |
| `\|` | 7.53 | 2.30 | 1.69 | **3.3×** |
| `^` | 8.02 | 2.30 | 1.69 | **3.5×** |
| `<<` | 7.14 | 2.30 | 1.82 | **3.1×** |
| `>>` | 7.56 | 2.29 | 1.83 | **3.3×** |
| `>>>` | 7.98 | 2.32 | 1.85 | **3.4×** |
| `%` | 8.29 | 5.31 | 4.87 | 1.6× |
| `**` | 17.53 | 12.55 | 11.82 | 1.4× |

Unary and short-circuit forms (one operand):

| operator | U0 today | U1 inline |
|---|---|---|
| `typeof` | 2.51 | 0.91 |
| `!` | 2.76 | 1.37 |
| unary `-` | 5.06 | 1.15 |
| `??` | 0.69 | 0.68 |

(ns/iter. Both arms of every guard are emitted and reachable.)

`??` is the control result: it is already pure IR (two integer compares against
the null/undefined singleton words), U0 and U1 are the same number, and there is
nothing to fix. Every other row above has a call in it.

`typeof` is a LOWER bound: the real trampoline returns an interned string word
that user code then compares (`typeof x === "number"`), and the probe returns the
boolean of that comparison directly. Note also that the engine const-folds
`typeof` for every statically-known operand (`expr.rs:301-390`), so this row is
the genuinely-dynamic case only.

Three things fall out:

1. **The guard recovers 60–75% of the gap to the proven floor without any type
   analysis.** It is a local IR change at the operator site — no Repr proof, no
   shape, no escape analysis. That makes it the cheapest lever in the whole
   probe to actually implement.
2. **The bitwise/shift family is the most expensive generic call**, above `*` and
   nearly 3× `===`, because the trampoline runs `ToInt32` (ToNumber → finite test
   → truncate → two casts) on each operand and re-boxes. It is also where the
   guard wins most in absolute terms (−5.7 ns).
3. **`%` is the outlier and the reason for the extra row below.**

### `%` — the variant the engine does not have

`binop.rs:575` takes the native `srem` path only when the divisor is a **known
non-zero constant**; every other `%` calls fmod, and fmod is the slowest single
thing in this probe. A RUNTIME guard can take `srem` far more often — it needs
both operands to round-trip through `i64` exactly, a non-zero divisor (`srem`
traps where JS yields `NaN`), and a non-zero dividend (`-0 % 3` is `-0` in JS but
`0` through the integer path):

| variant | ns/iter |
|---|---|
| X0 today | 8.23 |
| X1 inline guard, still fmod | 5.30 |
| X2 proven Repr, still fmod | 4.86 |
| **X3 runtime int guard → `srem`** | **3.35** |

X3 beats X2. That is the notable result: for `%`, the engine's *best case today*
is not the floor — a runtime-guarded integer path is **1.45× faster than the
compile-time-proven float path**, because the proof buys nothing while fmod
remains a call and Cranelift has no `frem`.

### `**` — the same story, much larger

`binop.rs:644` calls `__RTS_FN_NS_MATH_POW` on the proven path too, and `powf` is
the slowest single operation in the probe. There is no special case for a literal
exponent. On `x ** 2` — the form real code overwhelmingly writes:

| variant | ns/iter |
|---|---|
| X0 today | 18.53 |
| X2 proven Repr (raw f64, still calls `Math.pow`) | 12.72 |
| **X3 `b == 2` → `fmul`** | **1.62** |

**11.5× against today, 7.9× against the engine's own best case.** And this one
does not even need a runtime guard: when the exponent is a LITERAL the check is a
lowering-time decision with zero cost, so the real number is at least this good.
The general `**` row above (exponent cycling 1..7, guard hitting 1/7 of the time)
shows the same guard costs nothing when it misses: 12.55 vs 12.72 unguarded.

### Kernel C — the value representation itself, 20M iterations

| variant | ns/iter |
|---|---|
| C0 NaN-box, integer-domain guard | 1.79 |
| C0b NaN-box, FP-domain guard (`ucomisd` self-compare) | 1.48 |
| C1 two-slot `{tag, value}` | 0.98 |
| C2 native `f64`, no tag | 0.73 |

The two-slot form is reproducibly **~0.81 ns/iter** faster than the NaN-box here
(1.79 → 0.98). Note `C2 native f64` is a CEILING, not an alternative — an
untagged `f64` cannot hold an object or a string, so the honest comparison for
"should we replace PolyValue" is C0 vs C1, not C0 vs C2.
**The mechanism is NOT established.** The hypothesis that it was the GPR→XMM move
forced by an integer-domain tag check is REFUTED: C0b keeps the value in the FP
domain throughout and lands on the same number as C0.

What the number does settle is the SCALE. The whole representation question is
worth ~0.4 ns/iter. Each individual lever in kernel A is worth ~5 ns, and the
allocation lever in kernel B is worth ~300 ns — **~700× the representation
delta.** Swapping `PolyValue` for a two-slot value would be a large, invasive
change to buy the smallest measurable item on this list.

## What it does NOT prove

Read this before quoting any number from it.

- **It is single-threaded.** The `unlocked` and `arena` variants are sound *here*
  because the probe runs one thread. They are NOT a design that would be correct
  under RTS's real multi-thread surface. The lock delta they measure is "what the
  lock costs a single thread", which is the right question only because RTS pays
  it even when nothing is shared — it is not a claim that the lock can simply be
  deleted.
- **No GC.** Kernel B's `reset()` between runs stands in for a sweep; it does not
  model marking, root scanning, or the fragmentation a real nursery would face.
- **`size_of::<Slot>()` is approximated** with a padding variant. The real `Entry`
  has ~50 variants and the widest one sets the slab stride. The probe prints its
  own size so the assumption is visible; if the real one is much wider, the
  locked variants here are optimistic.
- **The fast paths do not re-tighten.** `genops::number_result` turns an exact
  integral result back into a tagged int32; the inline arms in A1/A2/A4 keep it a
  double, which is what a real inline fast path would do too — but it means A0 and
  the rest are not bit-identical in the intermediate representation, only in the
  final value.
- **It measures one shape of workload per kernel**, all of them monomorphic and
  hot. It says nothing about megamorphic dispatch, deeply polymorphic call sites,
  or cold code, and the levers may rank differently there.
- **A single machine, single run of the harness.** Medians of 7, but no
  cross-machine validation and no confidence interval.
- **String interning (E1) is priced on the READ side only.** Interning moves cost
  to construction; this probe never builds a string through an intern table, so
  E1 is an upper bound on the benefit, not a net figure.
- **N1's inline int32 arm omits the overflow check** a correct implementation
  needs, so that row is a lower bound.
- **D2's append-in-place is not drop-in.** JS strings are immutable; doing this
  legally requires proving the previous value is dead.
- **Kernel H's shard bases are stable only because the probe pre-reserves them.**
  The block slabs `resize` each shard up front, so the base the IR loads never
  moves. A real implementation cannot just grow a `Vec` — a realloc would
  invalidate every live object address mid-run. **H7 prices the chunk-table form
  of the fix and finds it free**, but H7 still never actually grows: the cost of
  committing a new chunk, and of the branch that checks whether one is needed on
  the ALLOCATION path, is not measured anywhere here.
- **Kernel H hoists nothing across a safepoint, because it has no safepoints.**
  The shard-base load is loop-invariant and the egraph will hoist it. In the real
  engine a GC tick (every 256 allocations) or a region migration can run inside
  that loop; if storage were ever re-homed while a hoisted base sat in a
  register, the address would be stale. Nothing in this kernel exercises that,
  and no row should be read as evidence that hoisting the base is safe.
- **Kernel H uses `MemFlags::trusted` on every load.** Real lowering cannot: a
  field load must not be reordered across a call that could mutate the object.
  The rows are therefore an upper bound on what correct aliasing metadata allows.
- **Kernel H's guarded rows always take the fast arm.** The fields hold inline
  doubles, so `is_double` never falls to `probe_to_number`. The slow arm is
  emitted and reachable (it is a guard, not a bet), but H0–H3 measure the
  monomorphic case; a field that actually holds tagged int32s would pay the call.
- **Kernel H does not model the write side.** Field WRITES through the inline
  slab would need whatever write barrier the eventual GC requires; kernel B's
  `B1b` row prices a card-mark barrier separately, and H has no equivalent.
  **Kernel W is the fix** (`RTS_CLASS_IMPLEMENTATION.md` §7 C0) — it is the
  construction+store ladder over the same movable block layout. Its own caveats
  follow.
- **Kernel W allocates 50 000 objects, not 3 000 000.** One allocation per
  iteration means the count is bounded by slab capacity: the W4 single-region row
  puts every object in `moving_slab`'s region 0, capped at 65 536 blocks. So W's
  rows are directly comparable to each other, and to H only per-iteration — the
  loops are not the same length, and W's shorter loop keeps a smaller working set
  resident than a 3M-object run would.
- **Kernel W's rows never reuse or free a block.** Each iteration bumps a fresh
  one and the slab is rewound between timed runs, so nothing measures allocating
  into a fragmented heap, the branch that commits a new chunk, or the cost of a
  block that outlives a nursery. §4.4 already showed the movable form's win
  depends on a COMPACTING collector; W assumes one and does not model it.
- **Kernel W's card mark is unconditional and per field store.** No generational
  filtering, no SATB / dirty-card enqueue, no remembered set — the cheapest
  honest barrier, so W3−W2 is a LOWER bound on a real barrier. It is also the
  worst case in the other direction: the fields are doubles, and with the precise
  field map §8.3 makes mandatory they would need no barrier at all. Read W3−W2 as
  "the cost of not having `fieldmap.rs`", not as a fixed tax on construction.
- **Kernel W's card table is masked, not heap-base-relative.** A production
  barrier computes `(addr − heap_base) >> 9`; the probe computes
  `(addr >> 9) & CARD_MASK` so it stays in bounds without knowing the heap's
  extent. Same instruction count, so the cost is not distorted — but the probe's
  table is 64 KB and aliases, which a real one does not.
- **Kernel W's read-back is part of the row.** Every row stores F fields and then
  reads them all back, because that read-back IS the checksum that stops a
  variant from silently doing less work. Kernel H already prices reads (11.20 ns
  locked, 1.49 ns direct), so the read component is known — but no W number is a
  pure store cost, and W0 in particular pays four LOCKED reads on top of its four
  locked pushes.
- **Kernel W's `probe_block_alloc*` trampolines do no GC tick.** Kernel B found
  the every-256-allocations `finish_cycle` mattered enough to need a replica
  (`probe_gc_tick`); W's block rows have none, so their allocation cost is
  optimistic against an engine that would still have to collect.

## Two of the probe's own numbers were wrong before they were right

Recorded because the correction is the useful part:

1. The first version reset the slab INSIDE the timed closure, charging kernel B's
   allocation variants for a teardown the engine does not do per loop. B0 read
   271–306 ns/iter; with `setup` moved outside the timer it reads ~100.
2. The first `eq_today` cloned one side to escape a borrow — an allocation the
   real `with_two_entries` never makes. It read 40.63 ns/iter; replicating the
   real two-entry lock brought it to 19.83. The original number was a strawman
   that would have overstated the case for interning by 2×.
3. The first `!==`/`!=` trampolines called the probe's `#[inline(never)]`
   `strict_eq`, forcing a second call the real build (same crate, opt-3) very
   likely inlines. They read 4.12 / 4.32 ns/iter and looked like a free win over
   `===`; with the body replicated instead they read 2.98 / 3.25 — the same as
   `===`. **That finding evaporated**, which is the correct outcome.

## Operator coverage

Covered: `=== !== == != < <= > >= + - * / % ** & | ^ << >> >>> typeof ! -(unary)
??`, plus `if(x)`/`&&`/`||` truthiness via the BOOL kernel and `new` via kernel B.

NOT covered by a dedicated row, and why: `=` (a store, priced by kernel A/OBJ),
`?.` and `?:` (branch + the same ToBoolean/null test as `??`/BOOL),
`in`/`instanceof`/`delete` (dictionary and shape operations, priced by kernel
OBJ), `...` spread and destructuring (allocation, priced by kernel B), `=>`
(closure creation — allocation plus a capture environment, not measured here).
`+` on strings is the STR-APPEND kernel, not the `+` row.
