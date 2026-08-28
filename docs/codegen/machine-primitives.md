# What the machine costs, and why nothing could say so until now

**The machine's own call is 1.1 ns. A built-in call in a compiled program is
35.** Settled 2026-08-28, over `b83eac1a`, release, on one Windows machine.

Everything between those two numbers belongs to the layers above the machine,
and until this measurement existed there was no way to say that rather than
believe it.

---

## 1. The instrument was blind, and said so if you read it

`crates/rts-cranelift/src/probe/` has existed for as long as the crate has, and
its module documentation makes a strong claim: *"A regression in them is a
regression in the machine layer. A slow program whose probe numbers are
unchanged is a problem somewhere else."* The crate README closes with the same
intent — "what is left is depth: more fixtures, more targets, and the numbers
the probe produces being watched over time".

Here is what it produced:

```text
  arithmetic     2.18 ns  1.0x
  field_read     2.08 ns  1.0x
  widen_narrow   2.14 ns  1.0x
  type_guard     2.85 ns  1.3x
```

**`field_read` reads below `arithmetic`, and it cannot be cheaper.** A field
read is an addition's worth of address arithmetic *plus* a load; it is strictly
more work. The ordering is noise, and noise that inverts a strict ordering is an
instrument reporting that it has no resolution left.

The cause was in the harness's own comment all along. Each fixture was one
primitive behind an `extern "C" fn(i64) -> i64` pointer, and that pointer was
measured — by this repository, in that file — at **1.27 ns**, against primitives
that cost a fraction of one.

**Subtracting the floor could never have fixed it.** The module doc says the
distance from the floor is the number, and that is right, but a subtraction
removes an *offset*; the problem was *resolution*. Amortising is what recovers
resolution, so every fixture now runs its primitive inside a counted loop and
the call is paid once per measurement instead of once per operation.

Two properties keep that honest, and a new fixture has to keep both:

- **The trip count arrives in a register.** A constant invites the code
  generator to unroll the loop or evaluate it while compiling, and then the row
  is about the optimizer.
- **The primitive sits on the loop's dependency chain.** Otherwise it is
  loop-invariant, gets hoisted out, and the row reads as the floor — which looks
  exactly like "this primitive is free".

---

## 2. What the machine costs

`cargo run --release -p rts-cranelift --example probe_run`, 200 000 000 inner
iterations per row.

| fixture | ns/op | above floor | what it is |
|---|---:|---:|---|
| `loop_floor` | 0.46 | — | a compare, a branch and an increment |
| `arithmetic` | 0.45 | ~0 | one proven addition |
| `widen_narrow` | 0.47 | +0.01 | made generic and proven back |
| `field_read` | 0.68 | **+0.22** | a reference becoming an address, and a load |
| `type_guard` | 1.15 | **+0.68** | reading what an object says it is, and narrowing |
| `call_direct` | 1.59 | **+1.12** | a direct call to a known function, and its return |

**It repeats.** Three consecutive runs, the "above floor" column: `field_read`
+0.22 / +0.22 / +0.23, `type_guard` +0.68 / +0.68 / +0.68, `call_direct`
+1.13 / +1.12 / +1.16. A ruler whose smallest reliable division is about
0.02 ns, against one that could not tell 0.22 from 0.68 the day before. That
repeatability is what lets the module's "these numbers are a contract" claim
mean anything: a row moving by a tenth of a nanosecond is now a signal rather
than weather.

Four things this settles, and each was previously an assumption:

**Proven arithmetic is free.** Not cheap — free. It costs less than the loop
carrying it, because the processor has the slack to retire it alongside the
compare and the branch. Rule 10 of the crate README — no operation accepts both
a proven and a generic operand — is what buys this, and this is the number that
says it was worth the awkwardness.

**Widening and narrowing are free.** `fixtures.rs` predicted this in a comment
("both are bit operations rather than calls, so an optimizer can see through the
pair. If this number ever approaches a call, that property broke") and could not
check it. It can now: +0.01 ns. **The value encoding is not where performance is
won**, which `tags/mod.rs` has claimed from the beginning and which is now
measured rather than asserted.

**A field read costs a fifth of a nanosecond.** `mem/mod.rs` says the measured
lever was "a call where a load would do"; this is the load side of that trade,
and it is as cheap as the design promised.

**A guard costs two thirds of a nanosecond**, which is the header load, the
compare and the branch — and it is the price of rule 11 (widening automatic,
narrowing only through a guard). Worth knowing before anyone proposes removing
one.

---

## 3. What it settles about everything above

`native-call-floor.md` measured, in compiled JavaScript on the same machine:

| | ns |
|---|---:|
| the machine's own direct call *(this document)* | **1.1** |
| `f(a)` — a static call in a compiled program | 2.8 |
| `c.m(a)` — a method | 23.1 |
| `set.has(7)` — a built-in | 33.5 |

So of the 33.5 ns a built-in costs, **the machine's call is 1.1 and the other
32.4 are somebody else's**. That is the sentence rule 3 of the crate README
exists to make sayable — *"this is the property that makes performance
attributable"* — and it could not be said before today, because the instrument
that was supposed to say it could not resolve its own rows.

It also settles the direction of the remaining work, which
`native-call-floor.md` §7 ranks and this confirms rather than discovers: the
three per-activation stacks, the generic call protocol, and a calling convention
with a stack slot are all above the machine. **Nothing in that list is a
machine-layer defect**, and a proposal to make the machine's call cheaper is
starting 32 ns away from the problem.

---

## 4. One principle the fixtures had to obey, worth stating here

**An `I64` cannot be made generic.** The generic form is a NaN box with a
48-bit payload, so a full machine integer does not fit in it, and
`lower::value` refuses by name rather than truncating. It surfaced when
`widen_narrow` was rewritten to carry the accumulator every other fixture
carries; that fixture carries an `I32` instead, and the comment in `open_loop`
says why so the next person does not rediscover it from a `CannotWiden` panic.

This is not a limitation to route around. It is what makes a generic value one
word, which is what makes widening free, which is the row above.

---

## 5. What these numbers do not say

- **One machine, one day.** Windows 11 Pro 26200, release. A ratio between rows
  travels; an absolute does not.
- **`field_read` reads a constant reference**, so its load is loop-invariant.
  It sits above the floor today, so nothing hoisted it — but if that row ever
  collapses onto the floor, the first thing to check is the code generator, not
  a win.
- **Nothing here allocates**, deliberately and as it always has been: allocation
  is a runtime entry point, and what a stand-in costs says nothing about what a
  real one does. The allocation number lives in `object-model.md`.
- **`call_direct` is the cheapest possible call** — a known callee, one proven
  argument, one proven return, no safepoint, no barrier, nothing to resolve. A
  call that does any of those costs more, and none of that extra is measured
  here.

---

## 6. The environment this leaves

The command is `cargo run --release -p rts-cranelift --example probe_run`, and
it is in `.claude/skills/perf-claim/SKILL.md` where it belongs. That skill named
`cargo test -p rts-cranelift --test probe` until 2026-08-28 and **that target has
never existed**, so the one command the repository's own performance discipline
gave for attributing a cost to a layer printed a list of unrelated test targets
and exited successfully.

An example rather than a test, deliberately: a timing test fails on a busy
machine, in shared CI, on a Tuesday morning. The probe is a ruler for whoever is
measuring, not a gate.
