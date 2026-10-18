# The lateral fall: a deoptimiser with nowhere to land

A deoptimiser is what lets a compiler **guess** instead of prove, and undo the
guess when it fails. This engine has no interpreter and no baseline tier, so the
textbook mechanism — reconstruct an interpreter frame and resume it — is not
available. This document is the form that is, why it is affordable, and the part
of it that is genuinely hard.

It depends entirely on `four-stages.md`. Nothing here is schedulable before E3.

---

## What a deoptimiser is, for the record

The compiler assumes something it has not proved — `x` is always int32, `o`
always has this shape, `Math` was never reassigned — emits fast code under the
assumption, and puts a cheap guard on it. When the guard fails, the deoptimiser
leaves through a side exit, **reconstructs the state the function would have had
if it had never been optimised** (the eliminated locals, the object dissolved
into registers by scalar replacement, the frames of the functions that were
inlined), and transfers into a lower tier at the matching point.

Step two is the hard one, and it is why guessing is otherwise unaffordable:
every deoptimisation point carries metadata saying where each source value lives
at that machine address — register, stack slot, constant, or *"rematerialise:
allocate this, then store these three registers into these fields"*.

The payoff is that being wrong becomes cheap, so nothing has to be proved about
the whole program. The cost is a second execution engine.

---

## The finding that makes this affordable here

`crates/rts-cranelift/src/frame/` is already most of one. It has `SuspendPlan`,
`plan_suspension_with(func, &Liveness)`, `ResumeLabel`, `ResumeMode`,
`FrameLayout::declare` and `resumable_form` — that is: **spill a frame's live
state into a record with a resume position, computed from liveness.**

Its own header states that suspending and resuming a frame is a machine
capability rather than a language feature, and that owning it there is what makes
generators and async functions the same feature instead of three implementations
of varying honesty. A deoptimiser is the third client of that capability, not a
second implementation of it.

---

## The decision that frames everything

**The tier landed in is the generic body of the same function**, already compiled
into the same binary. So this is not V8's deoptimiser; it is a *lateral fall* —
suspend the specialised frame, resume the generic one at the matching point.

That makes the hard part easy and the easy part explicit:

- there is no interpreter frame to reconstruct, only a suspension record, which
  the machine already knows how to build;
- but it requires **paired resume points**: every guard in the specialised tier
  needs a `ResumeLabel` at the same program point in the generic one. That is
  only guaranteeable if both bodies are lowered from the *same MIR*, which is why
  this cannot start before E3.

### Against the alternatives

*Add an interpreter.* It is a second execution engine with its own semantics to
keep in agreement, in a project whose AOT binaries would have to carry it. The
generic tier is already compiled and already correct.

*Fall to a runtime call that re-enters.* A call cannot resume mid-function, so
every guard would become a function boundary — which is the entry-only fall that
exists today and is exactly the ceiling this removes.

---

## The plan

### D0 — preconditions

None of them belong to this plan, and none is negotiable.

- **E2 (resolve) and E3 (MIR)** of `four-stages.md`. Without one MIR generating
  both tiers, point pairing is a convention between two emitters and drifts
  silently.
- **Precise roots.** A deoptimisation record is a structure full of live
  references at a safepoint. Without precise tracing it is `lost-roots.md`'s
  silent class in a new shape.
- `reuse-check` before the first line, over `frame/`, `gc/` and `observe/`.

### D1 — `DeoptPoint` in the MIR

A guard stops being "branch to the generic body at entry" and becomes
`guard(cond) else deopt(id)`. Each `DeoptPoint` carries the program-point id
shared by both tiers, the MIR values live there, and the reason. It is a MIR node
rather than side metadata, because passes must see it in order not to move code
behind a guard.

Gate: `rts mir` prints the points. No new code runs.

### D2 — pairing the two tiers

Lowering the MIR twice, the generic tier emits a `ResumeLabel` at each
`DeoptPoint` id and the specialised tier emits the side exit. `rts-mir`'s
`guard/pair.rs` refuses a module where an id exists in one tier and not the
other. Pairing is a MIR-level fact — same MIR, two lowerings — so it is checked
there; the machine only confirms that the label exists.

This check is mechanical because it is the only defence against drift.

Gate: a fixture that fails when a label is removed.

### D3 — the record and the transfer

The side exit spills the live values according to the `FrameLayout` that
`plan_suspension_with` already computes, and jumps to the generic tier at the
label. It reuses `resumable_form`; if something there does not fit, the change is
*in* `frame/` and not a second implementation of it.

Gate: a fixture where the guard fails on the 5th of 10 loop iterations and the
answer matches Node, plus `rts ir` showing exactly one side exit.

### D4 — rematerialisation

This is where the real work is. An object dissolved by scalar replacement does
not exist at the point of the fall, and the generic body expects it. Each
`DeoptPoint` carries a **recipe**: allocate shape S, write these values into
these fields, bind it here.

Without D4, scalar replacement and deoptimisation are mutually exclusive — and
they are the two that are worth the most.

Gate: the answer matching Node **and** an allocation counter, because "answered
correctly" cannot distinguish rematerialising from never having dissolved. This
is `entry-tax.md`'s rule: assert on the side effect, never on the result.

### D5 — the sticky fall

There is no JIT, so there is no recompilation: a guard that failed will fail
again. One bit per specialisation — *this one has fallen* — and subsequent entries
go straight to the generic tier.

Without it, a program that violates the assumption inside a loop pays the fall
per iteration and ends up **slower** than with no specialisation at all.

Gate: the clock. `rts-codegen/README.md` rule 11 — a mechanism that only shows
up in performance is not testable by assertion.

### D6 — inlined functions

A side exit inside an inlined body needs N resumes, and that is where V8's
complexity lives. The cheap way out: **inline the same body in the generic tier
too.** Point ids stay 1:1 and one frame is one frame. It costs generated code and
costs no frame-reconstruction machinery.

Until D6 exists, a function that inlines another **refuses** to deoptimise inside
the inlined body and falls at entry, as it does today.

---

## Where it lives

| piece | crate | module |
|---|---|---|
| `PointId`, `DeoptPoint`, the two tiers, pairing | `rts-mir` | `guard/` |
| the recipe, in terms of shape and binding | `rts-mir` | `guard/remat.rs` |
| the side exit: spill and jump | `rts-cranelift` | `frame/exit.rs` |
| `PointId → (ResumeLabel, slots)` | `rts-cranelift` | `deopt/` |
| the recipe as machine operations | `rts-cranelift` | `deopt/remat.rs` |
| the sticky bit, the fall counter | `rts-core` | `deopt.rs` |
| the assertions that both sides agree | `rts-host` | `tests/deopt.rs` |

Two boundaries worth fixing in writing:

1. **`PointId` is an opaque `u32` to the machine.** It pairs and resumes; it does
   not interpret. The day `deopt/` needs to know *why* a guard failed in order to
   decide something, policy has leaked downward.
2. **The recipe crosses the boundary already translated.** `rts-mir` speaks of
   shape and binding; `rts-cranelift` receives `TypeId` and offsets. One
   translation, in the lowering, like everything else.

A fall does not cross the runtime. It is a jump inside the same compiled
function, so there is no new entry point — and if one appears, something was
designed backwards.

---

## What this buys, and what it does not

It buys the thing that is inexpressible today: **assumptions checked in the
middle of execution**, not only at entry. The assumption that only turns out
false on the thousandth iteration of a loop is the whole of the current ceiling.

It does not buy guessing without a witness. The guard still needs a compiled
generic body, so this remains closed-world with roughly 2× generated code. And it
does not buy specialisation guided by run-time observation — that is the AOT
profile (`rts compile` consuming a profile from an earlier `rts run`), which is
independent of this plan and composes with it: the profile chooses what to
specialise, the deoptimiser pays when the profile was wrong.

| | a deoptimiser | the lateral fall |
|---|---|---|
| where it lands | interpreter, via frame reconstruction | the generic body, same binary |
| cost of falling | expensive, rare | one jump |
| cost of existing | 2nd engine + metadata per safepoint | ~2× generated code |
| may guess? | yes, from profiles | no — a guard needs a compiled witness |
| where it may fall | anywhere, any point | only where a guard was emitted |

One honesty note to keep attached to this document: it is **a second execution
tier per function**, not a small mechanism. What makes it viable is that half of
it was built for another reason — and that claim is for `reuse-check` to confirm
before D1, not after.
