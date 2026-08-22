# What the absent IR pass costs a loop, in nanoseconds

**Measured 2026-08-21, not yet acted on.** `bench/isolated/src/bin/loop_shapes.rs`.

The answer is concentrated in one place and it is not the place the IR makes it
look like: **`ToInt32` costs 3.1–3.7 ns per occurrence, and boxing costs about
one nanosecond for a whole round trip.** Anyone reading the emitted code would
guess the other way round.

---

## What the engine emits

`rts ir` on `let a = 0; for (let i = 0; i < n; i++) a += arr[i & 1023];` produces
this per iteration:

```text
v63: I32 = ToInt32(v55)            i, which is F64
v64: I32 = ToInt32(v62)            1023.0 — a CONSTANT, converted at run time
v65: I32 = Bitwise(And, v63, v64)
v66: F64 = ToF64(v65)
v67: Tagged = Widen(v66)           re-boxed to be passed
v68: Tagged = Call(__rts_get_indexed, [arr, v67])
...
Guard { input: v54, expect: F64 }  `a`, arriving as a Tagged block parameter
Guard { input: v68, expect: F64 }
v76: F64 = FloatArith(Add, v74, v75)
v77: Tagged = Widen(v76)           and re-boxed for the back edge
```

Two of those are not what the program asked for, and `crates/rts-cranelift/src/ir/fold.rs`
declines both **by name**:

- **`ToInt32` over a constant.** `fold.rs` folds exactly two things — a guard
  whose answer is known, and `x * 1.0` — and its own header says why nothing
  else: *"Anything needing a fixed point, a traversal, or knowledge of a second
  block belongs in a pass and not here."* There is no such pass.
- **The accumulator is `Tagged` across the back edge.** `guard_answer` answers
  about one instruction, and *"a value that reaches a block parameter through two
  widened predecessors answers `None`. That is a traversal, and a traversal is a
  pass."*

## The measurement

Five shapes of the same loop, each adding exactly one of the defects to the one
above it, so each row's difference from its predecessor is that transformation's
price. Release, best of three, calibrated iteration count.

| | ns/op | Δ |
|---|---:|---:|
| 1. everything unboxed, machine index | 1.585 | |
| 2. + `ToInt32` on the index | 5.288 | **+3.70** |
| 3. + `ToInt32` on the constant mask too | 8.371 | **+3.08** |
| 4. + the index passed NaN-boxed | 8.741 | +0.37 |
| 5. + accumulator `Tagged` across the back edge | 9.394 | +0.65 |

**Row 3 is the one worth staring at.** Folding `ToInt32(1023.0)` — a constant,
known at compile time, converted on every single iteration — is worth **3.08 ns**,
which is roughly *twice* what the entire loop costs when nothing is in its way.

**Rows 4 and 5 are worth about a nanosecond between them.** The NaN-boxing round
trip that dominates the IR visually is nearly free, which corroborates what
`crates/rts-cranelift/src/tags/mod.rs` already claims about itself: *"the encoded
form costs under a nanosecond more than the alternatives … The encoding is
infrastructure to get right, not an optimization target."* That claim is now
measured from the outside as well as asserted from the inside.

## Why `ToInt32` costs that much

`crates/rts-cranelift/src/lower/body.rs:280` lowers it to a **branch-free
seven-instruction serial float chain**: `trunc`, `fmul`, `trunc`, `fmul`, `fsub`,
`fcvt_to_sint_sat`, `ireduce`. Every step depends on the last, `trunc` is the
long pole, and the whole thing feeds the call that follows it.

The lowering is carefully argued and its reasoning is not in question — the
comment there records replacing a `divsd` with a reciprocal multiply, proves the
substitution exact (2⁻³² is a power of two, so both spellings only adjust an
exponent), and measures `a = a | 0` at 11.6 ns on a dependency chain against
2.98 ns off it. The chain is what the language asks for **when the operand is
not known**.

The operand here *is* known. `1023.0` is a literal.

## What this does and does not license

**It licenses folding, and folding first.** `ToInt32` of a constant is a
compile-time computation with no semantic subtlety: the language's answer for a
literal double is a literal `i32`, and the seven-instruction chain computes
exactly that. Two occurrences per `i & mask` in every masked-index loop in every
program.

**It does not license a cheaper `ToInt32` in general** on this evidence. That is
a separate change with a real trade — the two-instruction fast path
(`fcvt_to_sint_sat` guarded on |x| < 2⁶³) is sound but overturns
`ir/inst.rs`'s stated design point that the sequence is branch-free, which
RULE 0 says must be changed first, with the reason.

**And it does not license writing a general pass yet.** What is measured here is
one transformation on one loop shape. A pass over the IR is weeks of work in a
crate whose README forbids reaching around the boundary to do it; this
experiment says the *first* transformation to put in one, not that the pass pays
for itself. The next number needed is how many `ToInt32`-over-a-constant sites a
real program has, which is a counter and not a timing.

**One caution about where the fold goes.** `ToInt32` is already spelled twice in
Rust — `value/convert.rs` and `emit/fold.rs` — and `fold.rs`'s own header says a
fold that disagrees with the runtime is worse than no fold. A third copy is the
defect. Whatever folds it must call one of the two that exist.

## An honest note about how this was measured

The first run of this experiment reported the constant conversion as **free**,
and it was wrong. `to_int32(mask)` is loop-invariant, so LLVM hoisted it out —
which is precisely the transformation the engine does not perform. The fix was to
make the constant opaque *inside* each loop rather than once outside.

That failure is worth more than the row it corrected. **An isolated experiment
measures the compiler it is compiled with, not the one it is about**, and a
model that lets the host compiler do the very optimisation under study will
report that the optimisation is worthless. The rule it produces: when modelling
a missing transformation, check that the model's compiler is not quietly
supplying it.
