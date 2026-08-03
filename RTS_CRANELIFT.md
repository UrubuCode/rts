# RTS_CRANELIFT.md — the machine layer

`rts-cranelift` is the **only** crate in this repository that is allowed to
depend on `cranelift-codegen` / `cranelift-frontend` / `cranelift-module` /
`cranelift-jit` / `cranelift-object`. Every other crate — `rts-codegen` (JS/TS),
the future Lua codegen, any tool — reaches the machine through `rts-cranelift`
and never through Cranelift itself.

Four crates depend on Cranelift today: `rts-codegen-new`, `rts-value-probe`,
`rts-engine` and `rts-shared`. The last two do so because a registry member may
carry a native emitter — a closure handed a Cranelift builder. Those emitters are
not an exception to the invariant; they are consumers of it. An emitter receives
the machine layer's builder, not Cranelift's, and the capability survives
unchanged.

This document defines what the crate is, what it is not, the invariants it
enforces, and the rule that decides whether a given capability belongs inside it.

---

## 1. Motivation

The engine that exists today calls `builder.ins()` from the language front-end.
Three consequences follow from that single fact, and all three are measured, not
suspected:

1. **Match soup.** Every machine-level decision (box or not, call or inline,
   handle or pointer) is re-taken at each call site, as a `match` over
   language-level shapes. There is no one place where a decision lives, so there
   is no one place to fix it.
2. **Illegal states are representable.** A caller can — and does — write
   `"undefined".to_string()` into a value slot. Nothing in the type system says
   no. Bugs of this class are found at runtime, one fixture at a time.
3. **Attribution is impossible.** When a program is slow, "the compiler" is one
   undifferentiated blob. There is no boundary at which to say *the machine layer
   is fast, the language layer is slow*.

A second language makes all three worse, not merely doubled: Lua would arrive and
re-derive the same GC discipline, the same atomics, the same calling-convention
handling, in a second dialect of the same match soup.

`rts-cranelift` exists to draw the boundary. Below it: the machine, pure,
language-agnostic, benchmarkable in isolation. Above it: languages, which express
semantics and nothing else.

---

## 2. Definition

**`rts-cranelift` is an extension of Cranelift.** It implements what Cranelift
does not provide and what a high-performance language runtime requires:

- a garbage collector contract the code generator participates in (safepoints,
  write barriers, root reporting, regional heap placement),
- atomic and threading primitives with an ownership/effect model the backend can
  act on,
- a generic tagged-value machinery that a language *parameterizes* but does not
  define,
- the ABI: signatures, calling conventions, aggregate returns, multi-value
  returns, foreign calls,
- the symbol surface for the operations Cranelift genuinely cannot express.

**It contains no knowledge of any source language.** No `undefined`, no `nil`, no
`this`, no metatable, no prototype, no `typeof`. If a symbol, a type, an enum
variant or a comment inside `rts-cranelift` names a JavaScript or Lua concept,
that is a defect in the layering, not a shortcut.

**Everything inside it is pure in the engineering sense**: total, deterministic,
verifiable, and testable without any language front-end present. A change to
`rts-cranelift` is provable by its own fixtures.

---

## 3. What belongs inside — the deciding rule

A capability belongs to `rts-cranelift` when it is **true of the machine**, and
to the language crate when it is **true of the language**.

Operationally:

> A capability belongs inside if it can be specified, implemented and tested
> without naming a source-language construct.

Examples, applied:

| Capability | Where | Why |
|---|---|---|
| `sqrt`, `clz`, `rotate`, integer wrap, float compare | inside | machine op |
| safepoint, write barrier, root slot, region placement | inside | GC contract is machine-level |
| atomic load/store/RMW, fence, thread spawn contract | inside | memory model |
| tagged value: layout, tag space, box/unbox, singleton table | inside (mechanism) | representation is machine-level |
| *what* tag 2 means (`undefined` vs `nil`) | outside (language registers it) | meaning is language-level |
| signature lowering, calling convention, multi-value return | inside | ABI is machine-level |
| `+` semantics, coercion, prototype lookup, metatable chain | outside | language semantics |
| shape/inline-cache mechanism (compare id, load at offset) | inside | dispatch mechanism is machine-level |
| which property names a shape has | outside | language data |

The rule is not a style preference. It is the property that makes the crate
benchmarkable in isolation, and therefore the property that makes performance
attributable.

---

## 4. The symbol rule

Cranelift lowers scalar computation to instructions. It cannot allocate, cannot
call the operating system, cannot touch global state. That boundary — and only
that boundary — is where a runtime symbol is justified.

> **A symbol exists if and only if the operation touches the heap, the operating
> system, or global mutable state. Pure scalar computation is IR. No
> exceptions.**

This rule is grepable and therefore enforceable as a gate. It also fixes the
current situation in which symbols exist for operations Cranelift already emits
inline, paying a call for arithmetic.

The rule has an immediate, identified consequence. A self-contained cluster of
roughly seventy declared symbols covers operations that map one-to-one onto a
single machine instruction — absolute value, minimum, maximum, square root,
clamping, multiplication, leading-zero count, size and alignment queries,
compiler hints. Each of those currently costs a call to compute something the
processor does in one instruction. They are the first deletion set, and they are
deleted by being emitted rather than by being removed.

Symbols owned by `rts-cranelift` are **direct**: they are the small, closed set
that patches Cranelift's gaps (allocation, GC slow paths, string/heap primitives,
OS entry points). They are declared in the crate, not through the language-side
symbol authoring macro, because they are not language surface. Language-level
symbols keep their existing single-source-of-truth path and are unaffected by
this document.

How small: on the order of **thirty to fifty**. Allocation, the write barrier's
slow path, the collection trigger, thread creation and joining, the few string
and heap primitives that genuinely copy, handler registration for unwinding, the
scheduler's park and enqueue and timer entry points, and a foreign-call
trampoline. Atomic operations are instructions, not symbols. Everything else in
the several hundred declarations that exist today is language surface, which the
machine layer must not know exists.

That size is what makes the set legitimate to enumerate by hand. A hand-written
list of fifty machine entry points, explicitly numbered, is the same discipline
as the singleton table of §7. A hand-written list of eight hundred is the failure
mode this repository already documented and drained.

One asymmetry is worth recording because it halves the problem. Ahead-of-time
compilation needs no table at all: an undefined symbol reference in an object
file is resolved by the linker against the runtime archive, using the object
format's own symbol table. Only just-in-time compilation, which has no linker in
the loop, needs a name-to-address mapping in memory. Machinery that builds that
mapping for both paths is solving a problem one of them does not have.

---

## 5. Structure of the crate

The crate is organized by responsibility, not by file size. Each module below is
a boundary with its own invariants, its own tests, and its own documentation of
what it guarantees.

```
rts-cranelift/
  ir/          the intermediate representation the languages emit
  verify/      the verifier: rejects representable-but-illegal programs
  value/       generic tagged-value machinery + the tag registry
  abi/         types, signatures, calling conventions, aggregate + multi-return
  gc/          safepoints, barriers, root reporting, regional placement
  mem/         allocation contract, object layout, shape/IC mechanism, strings
  sync/        atomics, fences, ownership and effect model, threading contract
  frame/       suspension: frame transformation, parking, resumption
  sched/       promises, continuations, queues, timers, readiness, cancellation
  unwind/      protected regions, cleanup, handler transfer
  guard/       guard nodes and their failure paths
  lower/       IR -> Cranelift IR (the only place `builder.ins()` is called)
  target/      JIT and object backends behind one interface, code lifecycle
  fault/       traps, stack limits, position mapping for faults
  observe/     position maps, symbol names, sampling hooks, counters
  symbols/     the closed set of gap-patching runtime symbols
  probe/       standalone fixtures and benchmarks (no language front-end)
```

`lower/` is the *only* module permitted to import Cranelift instruction builders.
Every other module manipulates the crate's own IR. This is what keeps the machine
decisions in one place instead of at each call site.

---

## 6. The IR, and why a helper library is not enough

If the language front-end calls helper functions that call `builder.ins()`, the
decisions are still taken at the call sites — the match soup simply moves behind
a nicer name. The boundary only becomes real when there is a representation in
the middle:

```
language AST/HIR  ->  rts-cranelift IR  ->  Cranelift IR  ->  machine code
```

The middle representation buys three properties a helper library cannot:

1. **A verifier.** Illegal states become unrepresentable rather than merely
   discouraged. Writing a string into a value slot is a construction error, not a
   runtime surprise.
2. **One lowering.** A decision — box/unbox placement, barrier insertion,
   safepoint placement, call-vs-inline — is written once and serves every
   language.
3. **Isolated tests.** IR fixtures compile and run with no language present. This
   is the mechanism by which the crate's value is proved and by which a slow
   program is attributed to the language layer rather than the machine layer.

### Type system of the IR

Every IR value carries exactly one representation:

```
I8 | I16 | I32 | I64 | F32 | F64 | Bool | Ref(TypeId) | Tagged
```

`join(a, b) = if a == b { a } else { Tagged }` — total and decidable. The
verifier rejects arithmetic on `Tagged` operands: a language that wants to add
two unknown values must lower an explicit generic operation, not an `iadd`. This
is the exact discipline that exists today as a convention; here it is a rule the
machine enforces.

### Deterministic values vs generic values

Two paths, chosen by the IR type, not by front-end heuristics:

- **Deterministic**: representation proven at the point of use. The value lives
  in a register in its native form. No tag, no check, no box.
- **Generic**: representation unknown. The value is tagged. Box and unbox are
  **pure IR** (bit operations and selects), never calls — this is what allows the
  optimizer to fold a redundant box/unbox pair and make the generic
  representation cost nothing wherever it was already monomorphic. Lowering box
  or unbox to a call destroys this property and is prohibited.

The folding this depends on is performed by the optimizer's egraph phase. No
build in this repository sets that flag explicitly today; it holds by the
compiler's default. A property this design rests on is stated, not inherited by
accident: the machine layer sets it and asserts it.

Between the two paths sits a third case that the measurements single out as the
cheapest available improvement: a value that is generic by type but monomorphic
in fact. An inline tag check on the generic path — no proof, no shape, no
analysis — recovers most of the distance to the proven floor. The IR therefore
has a first-class guard node, cheap and inline, rather than treating a
representation check as an exceptional path.

---

## 7. The tagged value: mechanism inside, meaning outside

`rts-cranelift` owns the *layout*: how many tag bits, where they sit, which
bit-patterns are reserved, how a payload is extracted, how a real floating-point
value is distinguished from a tagged one. It owns the guarantee that the payload
of a heap-referencing value is a table index rather than a raw pointer, which is
what makes the representation safe for a moving or regional collector.

`rts-cranelift` does **not** own the meaning, and it does not know who is asking.
The registry takes a count of value kinds and a count of singletons and returns
encodings. It records no owner, no namespace, no language identity — a value kind
is a number with a layout, and that is the whole of it.

- singletons are **explicitly numbered** and the numbering is the registry's
  output, not a constant anyone writes down twice,
- every consumer reads the same numbering from the same place,
- no singleton is ever expressed as a string, a magic literal, or a bit pattern
  reconstructed at a call site.

This is deliberately weaker than a per-language namespace would be. A namespace
would let the machine layer answer "which language does this value belong to",
and there is no machine-level operation that needs the answer. Declining to model
it is what keeps the layer honest: a capability that cannot express a language's
identity cannot accidentally depend on it.

The consequence is that a second language brings its own value kinds without
touching the machine layer, and the machine layer can be reasoned about without
knowing which language is running — or that there is more than one.

Two constraints are inherited from measurement rather than chosen. First, the
representation is not where performance is won: isolating it shows the current
tagged form costs under a nanosecond more than the alternatives, which is orders
of magnitude below the cost of allocation and of indirect access. The tagged
representation is therefore treated as infrastructure to get right, not as an
optimization target. Second, the tag space is small — three bits, with most
values already assigned and the remainder reserved. A second language does not
get to mint tags freely; it registers value kinds within the singleton space, and
any proposal to widen the tag space is a change to the layout with its own
justification.

---

## 8. Garbage collection

The collector contract is machine-level and therefore lives here. Three
constructs are first-class IR nodes, not something emitted ad hoc during
lowering:

- **Safepoint.** Carries the explicit set of live references at that point. This
  is the producer side of precise stack maps: the verifier requires that every
  live reference at a safepoint occupies a slot the map declares. Without a
  first-class node the map is either absent or incomplete, and an incomplete
  precise map under-approximates the root set — which frees live memory. A
  conservative fallback for any frame lacking a map is part of the contract, not
  an optional nicety.
- **Write barrier.** Implicit in every reference store. A regional or generational
  collector whose barriers are inserted by hand will eventually miss one path,
  and the resulting corruption is non-deterministic and expensive to find.
  Insertion belongs to lowering, where it cannot be forgotten.
- **Region placement.** Allocation carries the region it targets. Escape
  information supplied by the language (see §9) is what allows an allocation to
  be placed in a thread-local region and collected without synchronization.

The underlying mechanism is not invented here. The compiler already supports
marking a value as a reference: doing so guarantees the value is spilled to a
stack slot and recorded at every point where it is live across a safepoint, and
the compiled output exposes the resulting map keyed by code offset. Two such
declarations exist — one for a value, one for a mutable local — and this
repository calls neither. That is the entire gap between the current conservative
scan and a precise one: not a missing capability, an uncalled one.

Two rules of the mechanism are constraints on everything else, and both were
verified rather than assumed. Every non-tail call is a safepoint, unconditionally
— there is no way to declare a call not to be one. Tail calls are not safepoints,
because the frame is gone before control transfers. The second rule is why a
reference cannot be kept alive merely by being about to be tail-passed.

A consequence worth recording because it is currently true by accident: the
collector only runs from inside allocation, allocation is a call, and every call
is a safepoint. Cooperative safepointing is therefore already compatible with the
existing collection trigger. It should be a stated design constraint rather than
a coincidence nobody has noticed.

**Precise where declared, conservative per frame elsewhere.** A frame with a
descriptor is scanned exactly; a frame without one — a thunk, a replayed
function, anything compiled outside this path — falls back to a conservative scan
*of that frame's range only*, not of the whole stack. This is what makes precise
and imprecise frames coexist in one call stack, which is what makes the migration
of §18 possible at all. The first fixture is therefore a mixed stack, not a probe
in isolation.

Regional collection and a conservative whole-stack scanner cannot share one heap
in one process without producing non-deterministic bugs. While both engines
coexist, either the heaps are separate or the new path inherits the conservative
discipline until cutover.

### One frame descriptor, three consumers

Root reporting, unwinding and suspension are not three tables. A call inside a
protected region, inside a suspendable function, holding live references is *one
instruction* in all three concerns, and three tables keyed by the same program
point would have to be kept in agreement by hand. They are one record:

```
FrameDescriptor {
    pc            the program point this record describes
    roots         the references live here, by slot and kind
    region        the innermost protected region, if any
    resume_label  the resumption point, if this is one
}
```

The payoff is not tidiness. A parked frame — one waiting on a promise, holding no
position on any call stack — is still reasoned about *as a frame*: its roots are
read out of its spilled storage using the same records, and its protected region
is preserved, so resuming inside a handler re-establishes the correct cleanup
chain. Today the equivalent exists as two integers carried by hand through code a
language's parser generates. One record makes it a property of the machine
instead of a convention each language would have to reinvent.

---

## 9. Threading, ownership and effects

A backend cannot derive thread-safety from machine code. By the time values are
registers, aliasing, ownership and scope have been erased. Any design in which
"the machine layer analyzes the output and decides where threads are safe" is
undecidable as stated.

What the machine layer *can* do is act on facts the language supplies. The IR
therefore carries, per value and per operation:

- **Ownership**: `Local` (does not escape the current thread), `Shared`
  (reachable by more than one thread), `Immutable` (never written after
  publication).
- **Effect**: `Pure`, `ReadHeap`, `WriteHeap`, `Extern`.

With those two annotations the backend makes real decisions: eliminate a barrier
for a `Local` store, place an allocation in a thread-local region, prove a loop
body independent and parallelize it, keep an `Immutable` load out of a
synchronization edge. Every one of those is machine-level and belongs here. The
*analysis* that establishes ownership — escape analysis over language semantics —
belongs to the language layer, which is the only layer that has the information.

This split is the honest version of "deterministic multithreading": the decision
is made by the machine, on evidence the language is obliged to provide, and the
absence of evidence degrades to the safe answer rather than to a guess.

An escape analysis already exists in the current code generator, but it answers a
different question: whether a local object leaves its function, so that its
fields can live in registers instead of the heap. It is single-threaded and
per-function. No part of the current pipeline tracks whether a value crosses a
thread boundary, and the argument passed to a spawned thread travels as raw bits
with no checking at all. The ownership annotation described here is therefore new
work, not a rename of something present.

---

## 10. ABI

The ABI is defined here, as explicit types with explicit implementations, not as
a table of constants:

- an ABI type describes size, alignment, register class, and how many machine
  slots it occupies;
- a signature is built from ABI types and lowered to a Cranelift signature by one
  function;
- a calling convention is **data**, not code: a language declares the convention
  it needs and the machine layer implements it.

Two requirements shape the design and must be present from the start rather than
retrofitted:

- **Aggregate and user-defined return types.** A language must be able to declare
  a type, receive an identifier for it, and use it in a signature. Layout, field
  offsets and reference-ness live in one shared type registry that the ABI, the
  garbage collector and the language all read. Duplicating that metadata in a
  second place is exactly the failure mode this repository has already paid for.
- **Multiple return values.** Not sugar: for some languages it is the calling
  convention. An ABI designed around single returns will be rewritten the day a
  second language arrives, so it is designed around multiple returns from the
  beginning.

The current ABI is the counter-example that motivates both requirements. Its type
vocabulary is entirely scalar — there is no aggregate, no struct, no array. A
return position holds exactly zero or one machine slot, and no code path in the
repository ever constructs a signature with more than one. A string is two slots
as a parameter and cannot be returned at all; the workaround is to return a
handle instead. Every function boundary is therefore scalar-in, scalar-out, and
anything that does not fit is pushed onto the heap.

That is a workable model for one language whose values are already uniform. It is
not a foundation. A language with genuine multiple returns cannot express its
calling convention in it, and a language with value types pays an allocation for
every boundary crossing — which the measurements identify as the single largest
cost in the system. The ABI is rebuilt rather than extended for this reason.

Three facts from the compiler shape the rebuild, each verified:

**Aggregate classification does not exist below us.** The compiler performs no
classification of a structure into register-sized pieces. It sees a flat list of
scalars per slot and nothing else. So the machine layer either implements
classification — deciding when a small aggregate travels in registers and when it
travels by reference — or it declines to, and pays an allocation at every
boundary crossing. Declining is not an option, because that allocation is the
cost the measurements identify as dominant. The classifier is written once,
parameterized by target, driven by the shared type registry, and the rules differ
enough per target that this must be data rather than duplicated code.

**Multiple returns work until they run out of registers.** Returns are assigned
to registers until the budget is exhausted, after which compilation fails with an
explicit instruction to use a structure-return parameter instead. There is a flag
that performs that rewrite silently. The machine layer does not set it: a flag
that changes an emitted signature underneath the abstraction is precisely the
kind of inferred effect §16 forbids. The ABI layer classifies its own returns and
inserts the out-pointer itself, visibly, in one place.

**Calling conventions come in two categories and neither is extensible.** There
are conventions that are stable across a library boundary, one per target, and
conventions that are internal and explicitly not stable — including the one that
supports tail calls. A language's own convention is not a convention in this
sense at all: a leading receiver parameter, a trailing rest-argument slice,
multiple returns adjusted to a caller's arity, are all *protocols expressed in
the parameter and return lists*, riding on top of an internal convention. They
are data the language registers, never a new convention the machine layer would
have to name — which it could not do without naming the language.

Tail calls carry their own constraint: the callee's convention must match the
caller's exactly, and the return types must match exactly. That makes a
tail-recursive group a unit — the whole group compiles under the tail convention
or none of it does. The machine layer decides that for the group rather than
letting a call site emit an edge the verifier would reject.

---

## 11. Suspension: coroutines, generators, async

Suspending and resuming a call frame is a machine capability, not a language
feature, and every language of interest needs it — coroutines in one, generators
and asynchronous functions in another. Retrofitting suspension into a code
generator that assumed a linear frame is a rewrite, so the IR carries explicit
suspend and resume nodes and the machine layer owns the state transformation.
Languages express *where* suspension happens; the machine layer decides *how*.

Half of this capability already exists, in the wrong layer and for one language.
Generators are a genuine suspendable state machine: locals that must outlive a
suspension are spilled into an explicit frame, a program counter selects the
resume point, and try/catch bookkeeping travels with it. But the transformation
is performed by the language's parser, and the code generator merely recognizes a
fixed protocol of calls and forwards them to the runtime. The machine layer owns
no part of it, so a second language reaching for coroutines would have to
reproduce the entire transformation in its own front-end.

Asynchronous functions, by contrast, do not suspend at all. The body compiles as
an ordinary linear function, the call site spawns it on a worker, and awaiting
blocks that worker's thread. Concurrency comes from occupying a thread, not from
releasing a frame. That is why suspension appears here as a machine capability:
one mechanism, owned below the languages, is what makes coroutines, generators
and non-blocking asynchronous frames the same feature rather than three
implementations of varying honesty.

### Why the frame is transformed rather than the stack switched

There are two ways to suspend: transform the function so its live state lives in
an explicit record, or give it a real stack of its own and switch stacks. The
second is not available. The compiler does have a stack-switching instruction,
and it is implemented for exactly one target — the one this project does not
develop on. Beyond availability, a switched stack introduces a second frame
representation that the root scanner and the unwinder would both have to
understand, in a design whose whole premise is that those two and suspension
share one representation.

The comparison settles it independently of availability. The language whose
runtime switches stacks owns its entire backend and calling convention precisely
so it can, and rewrites references whenever a stack moves. The two
implementations closest to this situation — a portable scripting runtime and a
production JavaScript engine — both transform frames instead, for the same
reason: a compiled frame is not relocatable, and making it so costs more than
flattening it. Frame transformation is not a stopgap here; it is what the
reference implementations converge on for compiled code.

The change from today is where the transformation happens. It moves out of one
language's parser and into lowering, driven by the same liveness the root
reporting already computes, producing a spill record sized per function rather
than a structure shaped for one language's generators.

---

## 12. Concurrency: promises, tasks and the scheduler

A promise is not a language feature. It is a machine-level object with three
states, a result slot, a list of continuations, and a scheduling discipline that
says when those continuations run relative to everything else. Every part of that
is below the languages, and every language that wants asynchrony — promises in
one, coroutine schedulers in another — needs the same object.

The machine layer therefore owns the whole concurrency substrate:

- **The promise object**: state, result, continuation list, and the transitions
  between states, with the transitions atomic and the states exhaustive. A
  promise that is resolved with another promise adopts it; a promise resolved
  twice keeps the first result. These are invariants of the object, enforced
  where the object is defined rather than re-checked at each call site.
- **Continuations**: a continuation is a suspendable frame (§11) plus the value
  it is waiting for. Attaching a continuation to a pending promise parks the
  frame; settling the promise makes it runnable. Nothing blocks a thread.
- **The scheduler**: a run queue and a higher-priority continuation queue **per
  region**, with a defined drain order. The order is part of the contract because
  observable ordering is what makes asynchronous programs deterministic or not.
  The order is: drain the continuation queue to empty — including continuations
  enqueued *by* that draining — then take exactly one item from the run queue,
  then drain again. A language declares which queue its construct uses; it does
  not implement a queue.

  Per region, not global. A global queue makes every resumption cross a region
  boundary, which is to say makes every wait a publication event — and the whole
  premise of thread-local regions is that local values need no synchronization. A
  global queue would make regions decorative. Per promise is worse: a promise's
  continuation list already is its wait set, and a second queue per object adds
  state without answering where a runnable continuation actually executes.
- **Timers and external readiness**: a source that becomes ready outside the
  program — a timer expiring, an operating-system handle signalling — enqueues
  through one entry point. Sources register with the scheduler rather than each
  subsystem inventing a way back in.
- **Cancellation**: a pending operation can be cancelled, and a cancelled
  continuation is dropped rather than run. Retrofitting cancellation is harder
  than including it.

The current design is the argument for moving this. An asynchronous function
compiles as an ordinary linear function; the call site spawns it onto a worker
thread; awaiting blocks that worker. Concurrency is bought by occupying a thread.
That works and is measurable, but it means the number of concurrent operations is
bounded by the thread pool, ordering is whatever the operating system's scheduler
decides, and the code generator has to know about spawning at call sites. With
suspendable frames and a real queue, an awaiting operation costs a parked frame
rather than a thread, ordering is defined by the machine layer, and the code
generator emits one node.

The relationship to §11 is not incidental — it is the same mechanism. Coroutines,
generators, asynchronous functions and promise continuations are four names for
suspend-and-resume over a scheduler. Building them once is the reason to build
them here.

---

## 13. Exceptions and unwinding

Throwing is a machine capability: it terminates a frame, runs whatever cleanup
the frame declared, and transfers to a handler that may be many frames up. All
three parts require knowing the frame layout, which the machine layer owns and
the language does not.

The IR carries the handler structure explicitly — protected regions, handler
edges, cleanup that must run whether or not an exception passed through. Lowering
turns that into the target's mechanism. The value that travels is opaque to the
machine layer: languages disagree about what may be thrown, and none of that
disagreement reaches here.

The compiler provides a real primitive for the dispatch half. A call may carry an
exception table naming a normal-return edge and tag-matched handler edges, with
the return value or an opaque payload arriving as a block argument. It does not
interpret the tags, does not know what the payload is, does not search across
frames, and does not run cleanup. Those three — tag matching, cross-frame handler
search, and cleanup on the way out — are the machine layer's work, and they are
built on the same frame table described below.

The reason to use that primitive rather than keep the present arrangement is the
ordinary path. With an exception table, the non-throwing edge is a fall-through
that costs nothing; the price is paid only when something is actually thrown.
Signalling through a slot checked after every call pays on every call forever,
including the overwhelming majority that never throw.

One consequence is visible to languages and is better stated now than discovered
later: a call in tail position discards its frame before control transfers, so it
cannot also be the call a handler is installed around. Returning a call's result
directly and catching that call's exception are mutually exclusive.

This matters beyond correctness. The current implementation signals errors
through a thread-local slot checked after calls, without real unwinding. That
shape has a cost in the ordinary path — a check that exists because there is no
mechanism — and it interacts badly with everything else in this document: a frame
that suspends across a protected region, a collector that must find roots in a
frame being unwound, a continuation that rejects a promise. Those interactions
are only expressible if unwinding, suspension and root reporting are designed
against the same frame model.

---

## 14. Guards and deoptimization

Everything fast in this design rests on an assumption: a representation is what
it was proven to be, a shape is the shape the cache recorded, an operand is the
type the guard expects. Assumptions need a defined outcome when they fail.

The machine layer therefore owns the guard and its failure path: a guard node
carries the condition and the state required to continue safely if the condition
does not hold. The failure path is a first-class, verifiable construct, not a
call into a slow function that happens to produce the same answer.

No such machinery exists anywhere in the current engine. Where an assumption
would be useful, the code either proves it conservatively or declines to
specialize. That is why the measurements show an inline check recovering most of
the available gain: the check is currently the whole optimization, because there
is nowhere to bail to. Adding a bail path is what makes everything above it
possible.

---

## 15. Code lifecycle, faults and observability

Three responsibilities follow from owning code generation, and they have no home
in a language crate.

**Lifecycle.** Compiling, caching, replaying, relocating and releasing compiled
code. Just-in-time and ahead-of-time are two consumers of one pipeline, not two
pipelines. Compiled code has an owner, a lifetime and a relationship to the data
it references — including inline-cache cells, which are writable data that must
be reset or carried across a replay. Tiering, if it ever exists, is a lifecycle
policy and belongs here.

**Faults.** Stack exhaustion, arithmetic traps, invalid memory access. A trap has
to map back to a position in the program to be reported, and the mapping is
generated during lowering. Guard pages, stack limits and signal handling belong
with the layer that laid out the frames.

**Observability.** Position mapping, symbol names for stack traces, sampling
hooks and counters. A profiler that cannot attribute time to source positions is
guessing, and only the code generator knows the correspondence. This is also what
makes the attribution claim of §13 checkable in production rather than only in
the probe.

Two smaller capabilities belong here for the same reason:

- **Foreign calls.** Calling a native function with a foreign convention,
  including out-parameters and types the internal ABI does not model. It is an
  ABI concern and belongs with the ABI.
- **Weak references and finalization.** Reachability is decided by the collector,
  so anything conditioned on reachability is decided there too.

And one that belongs here because of a measurement: **string representation**.
Appending to a string is the single largest factor in the probe, over a hundredfold,
and the cause is a pair of copies, not anything about strings as a concept.
Layout, interning, and whether a string can be appended in place are decisions
about memory, and interning was measured as a genuine tradeoff — it moves cost
from comparison to construction — which is exactly the kind of decision that must
be made once, with evidence, in one place.

---

## 16. The interface

The interface is a builder over the IR, in the crate's own vocabulary. It never
exposes a Cranelift type, and it makes the invariants of §14 structural rather
than advisory.

Three properties shape it:

**Representation is in the type.** A value handle carries its representation.
Adding two numeric values and adding two generic values are different operations
with different names, because they generate different code and have different
costs. There is no operation that silently accepts either — that ambiguity is
exactly what produces a match at every call site.

**Effects are declared, not inferred.** An operation that allocates says so, and
receives the safepoint that allocation implies. An operation that stores a
reference receives its barrier. The caller does not remember to ask; the caller
cannot decline.

**Unsafe capability is named.** Relaxed memory access, hoisting an address across
a safepoint, assuming a shape without a guard — each is a distinct, explicitly
named operation with its precondition documented, so that reading the generated
code shows what was assumed. The probe's fastest measurements depend on
relaxations that real lowering may not be entitled to; naming them is how that
distinction survives contact with production code.

### Representation is carried, not parameterized

A value handle carries its representation as data, and the verifier checks it.
Encoding the representation in the Rust type of the handle was considered and
rejected on three grounds, each structural rather than aesthetic. The lowering
frequently does not know a representation until a merge is computed, so a
translation function would have no expressible return type. Argument lists, block
parameters and field lists are heterogeneous by nature, so a typed handle forces
an enum back into existence at every collection. And the compiler's own value
handle is untyped with a typed lookup, so a typed handle fights the layer it must
eventually become.

The property that matters survives anyway, and by a stronger mechanism than
types: **no operation accepts both.** Proven-numeric addition rejects a generic
operand; generic addition is a separate, differently named operation. There is no
single addition that inspects its operands and branches — that branch is the
match this crate exists to delete.

### Locals and merges

Mutable locals come in two forms, and the split is forced rather than chosen. The
compiler's automatic merge insertion unifies machine types; it has no notion of
joining two representations to the generic one and boxing on the losing edge.
That decision belongs to the machine layer.

So: a local whose representation is fixed for its whole lifetime uses the
automatic form. Anything that can merge with a different representation travels
as an explicit block parameter, and the branch builder compares each argument's
representation against the target's and inserts the conversion itself. That is
one place to audit box insertion instead of one per call site.

The reverse direction is refused. Widening to generic is inserted automatically;
narrowing from generic is never synthesized, because it can fail at runtime and
the machine layer does not manufacture a fallible operation implicitly. Narrowing
requires a guard, written by the language, with its failure path (§14).

### Constants are five things, not one

Scalars materialize as immediates. Read-only aggregates and text live in a data
section, deduplicated by content across the whole module. Symbol addresses
materialize as addresses. And separately from all of those: **a constant that is
a reference the collector must understand.** Its storage is static and never
moves, so reading it needs no barrier — but it participates in representation
joins, in barrier insertion when stored into something mutable, and in root
reporting when live across a safepoint. Declaring that once, at the constant,
keeps every consumer of a reference uniform. Omitting the category forces a
special case at every use.

Whether a constant becomes an immediate or a load is not a language decision. It
is a property of the constant's kind and of the target's instruction encodings,
decided once during lowering.

### A field access is not a load

A raw load requires the caller to assert width, alignment and trust. Every such
assertion is a decision taken at a call site — the disease named in §1. A field
access asserts nothing: width and representation come from the type registry, the
access flags are derived from the declared layout, the barrier follows from the
field being a reference and the object being on the heap, and it is the single
attachment point for the shape and cache mechanism. An object with a dynamic
shape uses the same call and resolves its offset through a cache cell instead of
a constant. The caller cannot tell, and has no reason to.

Raw access still exists, because the string and buffer code inside the machine
layer has no registered layout to consult. It is a named, lower tier — not the
everyday entry point.

### Calls

Five call shapes, and deliberately not more: direct, indirect, and the tail
variants of each. A shape-guarded call is not a sixth — it is a guard (§14) whose
success path contains a direct call, which composes with tail position for free
instead of requiring a cross product of node kinds.

Suspension is not a call shape either. Whether a function may suspend is a
property of *that function*, declared once where it is defined, not re-declared
at every site that calls it. A language calling a suspending function emits an
ordinary call; the parking sequence is inserted because the callee says so. The
caller does not remember to ask.

### Runtime entry points are resolved once, not per call site

Every reference to a runtime entry point currently re-declares it by name into
the module, allocating a string and probing a hash table each time. Fifty calls
to the same entry point pay fifty lookups for one declaration. The compiler
already assigns a compact identifier at declaration and uses only that afterwards
— the missing piece is on our side: a per-module table, indexed by a compile-time
discriminant, that declares each entry point once and hands back the identifier
for every later use. No string is hashed on the emission path.

That table is per module instance by construction, since the identifiers are not
portable between modules. It must be rebuilt for a fresh module and never carried
across a replay boundary — a distinction that has already caused one bug in this
repository.

Concretely, the surface a language builds against:

- a **module** — declares functions and data, produces either executable memory
  or an object file, from one description;
- a **function builder** — blocks, parameters, terminators, and the IR
  operations, all typed by representation;
- a **type registry** — declare an aggregate, receive an identifier, use it in
  signatures and in allocation; field offsets and reference-ness derived once;
- a **tag registry** — value kinds and singletons are declared and encodings come
  back, with no record of who declared them and no bit pattern written by hand;
- a **heap interface** — allocate in a region, read and write fields, the shape
  and cache mechanism;
- a **scheduler interface** — create a promise, settle it, park a frame, enqueue
  a continuation, register a readiness source;
- a **verifier** — total, run on every build in development, rejecting a program
  that violates any invariant in §20.

The verifier is the load-bearing part. Every rule in this document that is stated
as an invariant is only real if something rejects the program that breaks it.
Documentation that is not enforced degrades into documentation that is wrong,
which this repository has already paid for more than once.

---

## 17. How a language uses it

A language crate contains a front-end, a semantic model, and a lowering to the
machine layer's IR. It contains no machine knowledge.

The division at each decision point:

| Decision | Language | Machine layer |
|---|---|---|
| what `a + b` means | resolves to numeric add, generic add, or a call | emits the instruction, the guard, or the call |
| whether a value is an integer | proves it from types and flow | represents it, and checks it where unproven |
| what `undefined` is | registers it as a singleton with a number | encodes it, decodes it, compares it |
| that a property exists | knows the key | shape, slot, cache, load |
| that an object is constructed | knows the fields | region, layout, allocation, safepoint |
| that a value does not escape | analyzes it | acts on the annotation |
| that a function suspends here | marks the point | transforms the frame, parks it, resumes it |
| that an error is thrown | defines what may be thrown | terminates the frame, runs cleanup, transfers |
| that a call is asynchronous | marks it | promise, continuation, queue, ordering |

The lowering is a translation, not a decision procedure. When a language crate
finds itself deciding *how* rather than *what*, a capability is missing from the
machine layer and the correct response is to add it there — not to reach around
it. That rule is what prevents the machine layer from becoming a library that
everyone bypasses in the interesting cases, which is the failure mode that
produced the current situation.

The second language is the test. If adding one requires changing the machine
layer for reasons other than a genuinely new machine capability, the boundary is
in the wrong place and this document is wrong.

---

## 18. Coexistence with the current engine

The current engine keeps running and keeps being the shipping path. Nothing is
deleted on the strength of a plan.

Two rules govern the transition:

- **No parallel drift.** A second engine developed in isolation diverges. The
  existing front-end migrates one construct family at a time onto the new
  machine layer, behind a switch, with the full test suite measuring both paths.
  The machine layer is therefore validated by the real corpus rather than by
  purpose-built fixtures.
- **Removal is a consequence, not a goal.** A crate is removed when nothing
  depends on it, verified by the build, and not before.

---

## 19. How the crate proves its value

The claim being made is that after this work, slowness is attributable to the
language layer and not to the machine layer. That claim is only meaningful if it
can be tested without a language present.

`probe/` therefore contains fixtures written directly in the crate's IR:
allocation, field access, dispatch, tagged arithmetic, barriers, safepoints,
atomics, calls. Each compiles to a binary and is timed. These numbers are the
crate's contract. A regression in them is a regression in the machine layer; a
slow program whose probe numbers are unchanged is a language-layer problem.

Benchmarks are release-profile only. A number from a debug build is not a number.

---

## 20. Invariants

These hold at every commit. Each is checkable, and each is intended to become a
gate.

1. No crate other than `rts-cranelift` depends on any Cranelift crate.
2. No module other than `lower/` constructs Cranelift instructions.
3. No identifier, string or comment inside `rts-cranelift` names a source-language
   construct.
4. A symbol exists only for an operation that touches the heap, the operating
   system, or global mutable state.
5. Box and unbox are emitted as pure instructions, never as calls.
6. Every live reference at a safepoint occupies a declared slot.
7. Every reference store passes through barrier insertion in lowering.
8. Singleton values have explicit documented numbering in one table, shared by
   every consumer. No consumer redefines a layout constant locally.
9. The verifier rejects arithmetic on generic operands.
10. The object write path is append-only: a slot offset already compiled is never
    invalidated by a later field addition.
11. Every module is testable with no language front-end present.
12. No structure in the crate records which language a declaration came from.
    The layer serves an unknown number of unknown clients, and cannot express
    otherwise.

---

## 21. Order of construction

Each step is usable and measurable before the next begins. The order follows the
measurements rather than the layering: the largest costs are allocation and
indirect access, and the tagged representation — the part that looks foundational
— is the smallest measurable item in the system.

1. IR and verifier — the shape of the boundary.
2. Scalar lowering — deterministic values end to end, probe numbers established.
3. Allocation contract — the single largest cost in the system today.
4. Addressing and object layout — reference to address as instructions rather
   than a call; shape and inline-cache mechanism.
5. Garbage collection — safepoints, barriers, precise root reporting with
   conservative fallback, regional placement.
6. Tagged values, tag registry, singleton table, inline guards — box and unbox as
   pure instructions, fold verified in the emitted code.
7. ABI — signatures, conventions, aggregates, multiple returns, type registry.
8. Guards and bailout — the failure path that everything above assumes.
9. Unwinding — protected regions, cleanup, handler transfer, over one frame model.
10. Suspension — the frame transformation, parking and resumption.
11. Scheduler and promises — continuations, queues, ordering, timers,
    cancellation, on top of suspension.
12. Ownership and effects — the annotations, then the optimizations they unlock.
13. Threading — the contract, then parallelization on supplied evidence.
14. Migration — the existing front-end moves across, family by family.

Faults, position mapping and code lifecycle are not a phase. They accrue from the
first step, because a machine layer that cannot report where a fault happened is
not usable for the migration that validates it.

---

## 22. The measured baseline

Every claim in this document that concerns cost is anchored to measurements that
already exist in this repository, obtained from a standalone probe that replicates
the real constants, the real object layout and the real allocator without linking
the engine. They are recorded here because a design that re-derives them wastes
the work, and a design that contradicts them is wrong.

**The representation is not the problem.** Isolating the value representation and
nothing else shows the current tagged form within a nanosecond of every
alternative, including a two-word form that cannot be adopted without an invasive
change, and including an untagged floating-point value that is a ceiling rather
than an alternative since it cannot hold a reference. Replacing the
representation buys the smallest measurable item in the system.

**Allocation is the problem.** Constructing an object is dominated almost
entirely by allocation through a lock-protected slab — not one cost among
several, but effectively the whole operation. Every other large number decomposes
into the same three causes: an opaque call where a load would do, a lock taken
per access, and a copy or allocation that was not necessary.

**Indirect access is the second problem.** Computing an object's address from its
reference with instructions, instead of calling out to do it, is worth more than
any representational change measured. Collapsing the shard routing to a single
region is worth more again — which is the empirical case for regional placement.

**Moving objects is free, conditionally.** The indirection that lets a collector
relocate a block costs nothing measurable — but only when allocation is
compacting and contiguous. Measured against scattered blocks the same design
costs several times more, from cache behaviour alone. Regional collection is
therefore compacting by requirement, not by preference.

**The cheapest improvement requires no analysis.** An inline check on the generic
path recovers most of the distance to the proven floor without any proof, shape
or analysis machinery. Anything more sophisticated is competing for the
remainder.

Three caveats are recorded with the measurements and are inherited here: they are
single-threaded, so a lock cost is not a promise about contention; they run
without a collector, so allocation figures exclude reclamation; and the fastest
access rows assume a memory-access relaxation that real lowering may not be
entitled to, making them an upper bound rather than a target.

**What is not yet verified.** These claims are load-bearing and unproven, and are
listed so that no one builds on them as if they were settled. Each needs a probe
fixture before it is relied on, and the fixture is cheap next to the cost of
discovering the answer during implementation.

- Whether returning more than a couple of values in registers is legalized end to
  end on every target this project ships. The signature type permits any number;
  the register budget and its overflow behaviour were read, but not exercised.
  Until a fixture proves a count on a target, that target uses the out-pointer
  form, which is unambiguously supported.
- Whether the exception-table call's ordinary path is genuinely as cheap as a
  plain call in this compiler version. The instruction exists; whether its
  instruction selection is mature is a different question, and the entire
  argument for adopting it rests on the ordinary path being free.
- Whether that call interacts correctly with the tail-call convention, and
  whether the multiple-return register budget differs under it.
- Whether the aggregate classification rules assumed for the non-primary
  architecture match what the compiler actually does there.
- What a foreign-call trampoline actually costs. It is asserted to be cheap; that
  is a design claim, not a measurement, unlike every number in the sections
  above.

**What exists and what does not.** Root finding is conservative: the stack is
scanned word by word and any word whose generation field is non-zero is treated
as a reference. The precise stack-map machinery is fully wired and carries
nothing — no producer exists, and the consumer has no callers. The producer is
not missing from the compiler; two declarations for it exist, one for a value and
one for a mutable local, and this repository calls neither. There is no write
barrier anywhere in the collector. Collection is skipped entirely on
non-x86-64 targets, because sweeping without marking would free live objects.
None of this is a defect to be patched at the edges; it is the reason the
collector contract belongs in the machine layer, where the producer of a root set
is the same component that emits the code.
