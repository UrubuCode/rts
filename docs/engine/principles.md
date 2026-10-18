# The principles a compiled language has to hold, and why these ones

RTS compiles TypeScript to native code, so it is judged by what a machine
language is judged by. This document states the goals that follow from that,
each with the test a change can be held against, and each decided by comparing
what **Rust**, **C#** and **Go** do — because the three disagree, and the
disagreements are where the choice is real.

It exists because of an audit on 2026-10-01 that found **eight families where one
question had several answers** in this tree: fifteen ways to obtain a cell, seven
ways to ask "is this an array", three readers of one property, and `f.call`
decided by three predicates that check different things. One of those divergences
was already a wrong answer against Node. None of them was written carelessly;
each was the shortest path at the moment it was written, and that is exactly why
a principle is needed rather than a cleanup.

`one-form-per-question.md` is the companion: this document says what the rules
are, that one says which form is canonical per family.

---

## The frame: what the three languages optimise FOR

| | what it refuses to give up | what it pays for that |
|---|---|---|
| **Rust** | a cost you did not write | compile-time proof obligations on the author |
| **C#** | a cost you did not write *being visible* — the JIT may do anything | a profile-shaped runtime, deoptimisation, a JIT |
| **Go** | predictability, and one path | peak throughput, and some generality |

**RTS takes C#'s bargain with a cheaper landing, and that is the decision this
document rests on.** A dynamic language cannot have its values proved: in
JavaScript there is no amount of whole-program analysis that settles every type,
every shape and every callee, so a compiler that only ever proves leaves the
remaining cases at the generic speed forever. The answer is to **guess and keep a
way back** — and `deopt-lateral.md` is the form it takes here: the tier landed in
is the **generic body of the same function**, already compiled into the same
binary, so there is no interpreter to reconstruct a frame for. The machine
already owns most of the mechanism, because suspending and resuming a frame from
liveness is what generators and `async` use (`frame::plan_suspension_with`,
`ResumeLabel`, `resumable_form`); a deoptimiser is that capability's third
client rather than a second implementation of it.

Two consequences, and both are dated rather than timeless. **Today** there is no
fall: `emit/` must prove, which is why `emit/inline.rs` is a whole-program proof
and says so in its own header. **With MIR** a guard may stand where a proof
cannot, which is why MIR is not a tidier emitter but the point of the exercise —
guards as dataflow values, a type domain to a fixed point, and paired resume
points are exactly what a guess needs in order to be undoable, and
`deopt-lateral.md` states that none of it is schedulable before that stage
exists.

Where RTS is nearer **Go**: what gets REPORTED. A number that only holds while a
guess holds is not a number a program can rely on, so both tiers are counted
separately — which is what `rts prove` already does, splitting every function
into what runs when every speculation holds and the second tier under it.

Where RTS is nearer **Rust**: a contract belongs in a type, not in a function
name. Where it is nearer **C#**: a specialisation is *derived from a
declaration*, never written twice.

---

## P1 — One question, one answer, and the answer is derived rather than remembered

**The test.** Can two places answer this question differently? If yes, the
answer is in the wrong shape — not because somebody will be careless, but
because there is no mechanism that notices.

**What the three do.** Rust makes it a type: `GlobalAlloc` has one `alloc`, and
ownership is unforgeable rather than documented. C# derives it: the JIT picks the
allocation helper **from the type**, so the author of a library cannot pick the
wrong one. Go derives it too: `mallocgc` takes the type and the size class comes
out of it.

**What RTS must do, given that its types are not static.** State the rule once,
have every side *call* it, and add something that rejects a caller who disagrees.
Done twice on 2026-10-01 and both are the model: `gc::traces_field` is one
predicate that the write side (`barrier_for`) and the read side
(`trace::edges_of`) both call, and the debug assertion in `edges_of` refuses a
field that contradicts it — which is what caught five wrong declarations within
minutes, one of them load-bearing.

**What this forbids.** A second spelling of a predicate "because the caller has
different data in hand". Change the predicate's parameters instead: that is how
`traces_field` ended up taking a `FieldLayout` rather than a registry and two
indices.

---

## P2 — The failure mode belongs to the operation, never to an argument

**The test.** Read the signature. Does it tell you what happens when it cannot
succeed — and is that answer the same for every argument?

**What the three do.** All three agree here, which is why it is a principle and
not a preference: Rust's allocation returns null and `handle_alloc_error` is the
one escalation; C# throws `OutOfMemoryException`, always; Go's allocation failure
is fatal, always.

**The violation in this tree.** `native::plain_with_room` returns `Option<u32>`,
and for `slots > 15` its wide arm calls `alloc_spanning_or_die`, which exits the
process. One function, two failure modes, chosen by the *value* of an argument. A
caller that wrote `let Some(cell) = … else { recover }` has recovery code that
cannot run. `objects::object_new_wide` and `array::allocate_array_cell_with_room`
have the same shape, and `functions.rs:218` takes the other side of it — it picks
the recoverable form and then *discards* the failure, so a region with no room
produces a function with no `.prototype` and a program that is quietly wrong.

**The rule.** One operation, one failure mode, stated in the type. Where a
caller genuinely needs both, that is two operations with two names and two
return types — never one function that decides by looking at a number.

---

## P3 — State that belongs to a value lives with the value, or exactly one total
## classification knows about it

**The test.** Add a new piece of per-value state in your head. Does the collector
find out automatically, or does somebody have to remember to tell it?

**What the three do — and this is the sharpest comparison in the document.** Go
derives the pointer map from the type: `gcdata` is a bitmap saying which words of
this type are pointers, generated by the compiler, and the collector reads it. C#
does the same through the method table's GC descriptor. Rust derives tracing from
the type as well. **In all three, "which words are references" is a fact about
the type, computed once, and no human maintains a list.**

RTS has the opposite today: twenty-three `Aside<T>` side tables beside the cells,
plus two lists deciding liveness — `roots::context_roots` and `side_tables` —
and `lost-roots.md` is the record of what that costs: a `for`-`of` that ended
early, a `JSON.parse` that answered objects with no properties, both with the
process exiting zero.

**Half of the mechanism already exists and it is the right half to copy.**
`side_tables::SideTable` is a total `match`, so **the compiler refuses the crate
until a new table has an answer**. What is missing is the other half, and it is
what the 14.x work is: the payload inside the cell, with the layout declaring the
repr of each slot, so that "which words are references" becomes derived here too
exactly as it is in Go.

**And this is the principle that decides whether the collector can ever move.**
Go and C# relocate objects because the pointer map is derived; a conservative
scan plus hand-written lists cannot, because a word that merely looks like a
reference must not be rewritten. So P3 is not hygiene — it is the precondition
for the whole of 6.4/6.5.

---

## P4 — A value's identity has one spelling

**The test.** How many distinct Rust types in this tree mean "that cell"? Every
one of them is a place a mover would have to update, and a place a conservative
scan can fail to see.

**What the three do.** One spelling: a pointer. Go, C# and Rust all name a heap
object exactly one way, and that is why relocation is a loop over known slots.

**What RTS has.** A `u32` cell index, a `Value` carrying `TAG_REFERENCE`, a
`Slot` into a slab, a raw address from `region.address_of`, and a spanning
allocation's interior cells which are *not* independent identities at all. The
hazard is already documented from the other direction: `lost-roots.md`'s second
mechanical check is "does any native hold a cell index across something that can
allocate?", because **a bare `u32` in a Rust local is not something the stack
scan can see**.

**The rule.** The index is an internal encoding, not a currency: it does not
cross an API boundary where a `Value` or a rooted guard would do. Where it must,
the function says so in its name and the caller's window is closed by `Rooted`.

---

## P5 — A fast path is a proof or a guess with a recorded way back, and it says which

**The test.** If this fast path is taken when it should not be, is the result
slow or is it **wrong**? A wrong answer is the only outcome forbidden, and there
are exactly two ways to avoid it: prove the claim, or guard it and record where
to land.

**What the three do.** C# guesses and deoptimises — it can afford to because it
keeps metadata saying where every source value lives at every deopt point, and a
baseline tier to land in. Go does not guess at all. Rust decides before running.
**RTS takes C#'s side, with a landing nobody else has**: the generic body of the
same function, already in the binary (`deopt-lateral.md`). The reason is the one
at the top of this document — a dynamic language cannot be fully proved, so a
compiler that only proves concedes every unproven case permanently.

**What this forbids, in both directions.** A guess with no landing — a
speculation emitted without a `DeoptPoint` and a paired resume label — is the
defect this principle exists for, and it is unrepresentable rather than
discouraged once D1 and D2 of `deopt-lateral.md` are in: a guard is a terminator,
so its failure path cannot be omitted (`rts-cranelift/README.md` rule 11's
neighbour). And in the other direction: a *proof* restated several times, which
is the same claim with several definitions and no mechanism noticing when they
drift.

**The violation in this tree, today, is the second kind.** `f.call` is decided
three times, by three predicates that do not check the same things: the tree
rewrite in `class_layout/rewrite.rs` checks only the syntactic shape and never
asks whether `Function.prototype.call` is still the language's; the door in
`methods/body.rs` asks primordiality but not the receiver; the native in
`function_direct.rs` asks about own `call` and own `prototype` but not
primordiality. Three layers, three proofs of one claim — and `apply` over a
literal list is decided twice inside the compiler alone, with different element
limits.

**The rule.** One predicate per claim, named for the claim, called by every layer
that wants it. A layer may decline to use it; it may not re-derive it. And where
the claim is not provable, it is a guard with a landing rather than a fourth
predicate.

**The violation in this tree.** `f.call` is decided three times, by three
predicates that do not check the same things: the tree rewrite in
`class_layout/rewrite.rs` checks only the syntactic shape and never asks whether
`Function.prototype.call` is still the language's; the door in `methods/body.rs`
asks primordiality but not the receiver; the native in `function_direct.rs` asks
about own `call` and own `prototype` but not primordiality. Three layers, three
different proofs of one claim — and `apply` over a literal list is decided twice
inside the compiler alone, with different element limits.

**The rule.** One predicate per claim, named for the claim, called by every layer
that wants it. A layer may decline to use it; it may not re-derive it.

---

## P6 — A number has exactly one issuer

Already binding (`rts-core/README.md` rule 3: keys come from the machine's
`KeyRegistry`, and a second numbering would be a second shape tree one level
up). Stated here because the audit found the generalisation violated: **an entry
point is written by hand in five places** — `runtime/mod.rs`, `domain/mod.rs`,
`core/entry/table.rs`, `host/entries/mod.rs` and the lowering — and its typed
door in a sixth, inside a `match` in a different crate from the native it serves.

C# is not better here (`CorInfoHelpFunc` and `jithelpers.h` are hand-wired), so
this is not a principle borrowed from anyone. It is one RTS can hold *better*
than its references, because the mechanism is already in the tree:
`#[rtse::class]` derives four views from one `impl` block. A door declared on the
method, like C#'s `[Intrinsic]` but generated, is the same move.

---

## P7 — The two layers decide different things, and where they must agree the
## agreement is derived

`rts-codegen` is the language and knows no machine; `rts-cranelift` is the
machine and knows no language (`architecture.md`). What this document adds is the
seam: when a fact must be known on both sides — a signature, a field's repr, a
convention — exactly one side computes it and the other calls. The attribute that
derives an ABI signature from a Rust signature is that rule already working; the
tracer calling `gc::traces_field` is it working again.

---

## P8 — The floor is computed before the path is built

A machine language has an answer to "how fast could this be": the instructions
that are necessary. So a change justified by speed states the floor, the measured
number, and the gap — and `perf-claim` is the procedure. Twice on 2026-10-01 this
turned out to matter more than the optimisation: a tracer change measured
**slower** than what it replaced until the lookup moved out of the slot loop, and
a `toPrecision` fix was worth six times less than the typed door it does not yet
have.

**And with a deoptimiser the reporting rule gets sharper, not looser.** Once a
guess is allowed, a single number stops describing the program: it describes the
tier the measurement happened to stay in. So both are reported — the armed path
and the one under it — which is what `rts prove` counts and why its own module
refuses to add them together. The honest claim after D1 is a pair, never a
figure: *this is what it costs while the guess holds, and this is what the fall
costs when it does not.*

---

## What this means for the two emitters

The MIR stage is the one that stays, and `emit/` is to be removed — that is the
direction, and this document is where it is written down rather than implied.

**The reason is P5, and it is stronger than tidiness.** MIR is not a cleaner
emitter; it is the only place a GUESS can live. A deoptimiser needs guards as
dataflow values, a type domain carried to a fixed point, effects, safepoints and
**paired resume points — which are only guaranteeable if both tiers are lowered
from the same MIR** (`deopt-lateral.md`). The running emitter cannot hold any of
that, so everything it does fast it must first prove, and `emit/escape.rs`,
`inline.rs`, `proven.rs` and `evidence.rs` are whole-program syntactic
approximations of facts that have nowhere else to go. In a language whose values
cannot be fully proved, "prove or stay generic" is a ceiling, and MIR is what
removes it.

So the duplication is not the problem being solved — it is the symptom. The thing
being bought is the ability to be wrong cheaply.

Two rules follow, and they are binding:

1. **New work goes into `lower/` + `machine/`.** A change to `emit/` is made only
   to keep a program compiling while the door still declines its function, and
   such a change is recorded as a refusal to be removed rather than as a
   feature. Otherwise the duplication the audit measured — 37 002 lines against
   11 191, with seven language features implemented twice — keeps growing.
2. **Nothing is removed from `emit/` before a differential test exists.**
   `RTS_MIR` appears in four places in the repository and **none of them is a
   test**: nothing runs one program through both paths and compares. Today the
   twenty hand-written refusals in `through_mir.rs` are what prevent a
   disagreement from running; the moment `emit/` is gone they have nothing left
   to decline to. The test comes first, the deletion second.

---

## How to use this

A change is held against P1–P8 the way it is held against the honesty floor:
not as advice, but as questions with answers. If a change has to break one, the
principle changes first, with the reason, and the document says so — the same
rule `CLAUDE.md` states for a crate's README.
