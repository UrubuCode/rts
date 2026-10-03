# The observability family is finished, and nothing calls it

`rts-cranelift` can answer three questions about a running program, and no
crate outside it asks any of them. Verified 2026-08-28 by searching the
workspace for consumers:

| capability | built where | who reads it |
|---|---|---|
| `gc::describe_frames` — which slots of a frame are live | on demand from liveness | **nothing** |
| `observe::CodeMap` / `MachineModule::place` — which function an address is in | on demand | **its own tests** |
| `observe::PositionMap` — which source position an address is at | eagerly, per function, `target/mod.rs:605` | **its own tests** |

The last is the sharpest: the map is *already built for every function the
engine compiles*, kept in `MachineModule`, reachable through a public getter —
and the only caller of that getter is `tests/observability.rs`.

This document exists because the three read as three separate pieces of unbuilt
work in three separate places, and they are one piece of *unwired* work that
several other things are waiting on.

---

## What each one is blocking

### 1. Precise roots block every machine-typed derivative of a reference

`docs/codegen/element-load.md` records a fast path that was 15.3% faster and
gave wrong answers in 53 of 60 cases: `Inst::ElementLoad` turns an array
reference into a base address, and the moment it does, the array stops looking
like a reference and the conservative scan stops finding it. The array is
collected while the loop is still reading it.

That document lists five candidate patches and refuses all five. The refusals
are correct **and they are all the same refusal**, which is only visible once
the three rows above are read together: a value that is a machine-typed
derivative of a heap reference — a base address, an unboxed field, a narrow
element — is invisible to a collector that recognises references by their bit
pattern.

It is also why `Repr::I8`, `I16` and `F32` exist in the machine's lattice with
**zero producers in the language layer**, and why the instruction set has no
integer-width conversion at all. The whole family shares one precondition.

`gc/mod.rs` already states the alternative and that it is not taken:

> the compiler underneath already supports the precise alternative … Two such
> declarations exist and **this repository calls neither**.

**And a third thing is blocked that this list did not name: the END of a whole
class of correctness bug.** With references recognised by bit pattern, what is
live is decided by two HAND-WRITTEN lists — `roots::context_roots` and
`trace::edges_of` — and a list is a place a thing can be missing from. Three
were missing on 2026-08-29 and two of them answered WRONGLY in silence: a
`for`-`of` that ended early, and a `JSON.parse` that returned objects with no
properties while the process exited zero.

`docs/engine/lost-roots.md` is the class, with the four checks that find the
next one and the reason to expect one. It is worth reading beside this document
rather than instead of it, because the two are the same precondition from
opposite sides: precise roots are usually argued for as a way to go FASTER — the
nine dead capabilities above — and they are equally a way to stop maintaining a
list by hand. Until then the class is policed rather than closed, and every new
side table, native and cache is a fresh chance to be missing from one.

### 2. Machine-derived stack traces block the direct call

`docs/codegen/native-call-floor.md` measures a JavaScript call at ~23 ns
against the 1.1 ns the machine charges for its own, and ranks a direct call to
a statically known callee as the largest remaining item at ~16 ns. Everything it
needs exists: `emit/inline.rs` already proves *which* function a name denotes,
the machine already lowers `Inst::Call`, and the justification that used to
forbid it expired.

What stops it is one line in `entry/throw.rs`: a stack trace is built by walking
`context.callees`, the runtime-side list every call pushes and pops. A direct
call that skips that push is a frame missing from `new Error().stack`, which is
a visible regression rather than an optimisation.

So the direct call needs stack traces to come from the machine stack instead —
which is `CodeMap` plus frame descriptors, both of which exist.

The same list is what `docs/codegen/native-call-floor.md` §3a prices at 7.3–10.2
ns per call and §5b records a refuted attempt at removing. **`callees` is not
removable while it is the only thing that knows what is running.**

### 3. And it is a missing user-visible feature, not only a performance one

`CLAUDE.md` states the gap in its own words:

> **No line numbers yet** — the machine records a source position per
> instruction and nothing maps an address back to one at run time, which is
> `rts_cranelift::observe`'s question.

`entry/throw.rs` states the same from the other side, where the code is:

> **What it does not carry is a POSITION**, and the caller must say so rather
> than fill one in … A zero here would be a line number a program could act on.

Both are describing `PositionMap`, which is built for every compiled function
and read by nobody.

---

## Why it went this way, which is worth more than the list

Nothing is broken and nobody was careless. Each capability was built to the
crate's own standard — documented, tested, and correct — and each was built
*before* a client existed for it, which is the order `rts-cranelift`'s README
argues for at length and which produced a working machine layer.

What has no owner is the **join**. `rts-cranelift` cannot wire it, because the
consumer is a collector and a throw path that live in `rts-core`, and rule 2
forbids the machine knowing a source language. `rts-core` cannot wire it,
because a frame descriptor is a fact about emitted code that only the emitter
has. `rts-host` is the one crate permitted to name both — its README rule 2 is
"make the agreements between the three explicit" — and this agreement was never
made.

That is the same shape as the four expired sentences
`docs/codegen/native-call-floor.md` §7 found in two crates that cannot see each
other. The boundary is the design and it is worth keeping; what it costs is that
some facts are true only in the space between the crates, and nothing lives
there to notice.

---

## The constraint that decides how the walk is written

*Added 2026-08-28, before writing one.* Frame pointers are preserved —
`isa_with` sets `preserve_frame_pointers` and says why — so a chain of compiled
frames is walkable by following `rbp`. `entry/registers.rs` already captures
`rbp` for the collector, and `rts-host/src/stack.rs` installs
`Context::stack_high` from the OS, so a walk has both a start and a bound.

**And that is not enough, because the chain is not all ours.** A throw is raised
inside `rts-core`, and the frames between one compiled function and the next are
Rust: `call_counted`, `called`, `invoke`, and whatever native is running. Rust
and LLVM do not promise to keep `rbp` as a frame pointer in a function that does
not need one, so following the chain through them can stop early or land on a
word that is not a frame at all.

Two consequences, and the second is the design decision:

- **The walk must SKIP rather than stop** at an address the map does not
  attribute. A Rust frame is not the end of the program; it is the middle of
  one call. A walker that ended at the first unattributed address would report
  exactly one compiled frame.
- **The chain itself has to come from unwind information, not from `rbp`**, if
  it is to cross those frames reliably. On Windows that is
  `RtlCaptureStackBackTrace` / `RtlVirtualUnwind`, which reads the unwind tables
  every x64 function is required to have; the machine layer's `unwind/` already
  produces ours. Elsewhere it is the platform's equivalent.

That puts the walker in `rts-host`, beside `stack.rs`, which is already the
crate that asks the OS about this thread's stack and is already written per
platform. Not in `rts-core`, which has no business naming an OS API, and not in
`rts-cranelift`, whose rule 2 forbids it knowing who is asking.

**This is why no walker is written here.** An `rbp` chain is ten lines and would
work in the cases anyone would first test — a compiled function calling a
compiled function — and would silently truncate the moment a native sat between
them, which is most real traces. Writing it that way to have something is how a
trace that is quietly wrong ships.

## What the conservative scan actually costs, measured (2026-10-03)

Everything in this document argues that a moving collector needs precise roots,
and the argument is right. What it never had is a NUMBER, so both 6.4 and 6.5
were justified by principle rather than measurement — which is what P8 forbids.

`RTS_GC_PRECISION` marks each cycle twice, once with every root and once with
only the declared ones (`context_roots` plus the saved registers):

```
rts-gc precision: 2 of 2814 live cells are pinned by the conservative
stack scan (221 declared roots, 235 with the scan)
```

**Two cells of 2814 — 0.07 %**, stable across every cycle of two different
programs.

Three things follow, and they reorder this document's own advice:

- **6.4 does not wait for 6.5.** Mostly-copying — Bartlett, and what the CLR
  does with pinned handles — pins what the conservative stack reaches and
  evacuates the rest. The residual fragmentation that costs was the unknown
  price of that design, and it is 0.07 %.
- **6.5 drops in priority.** A shadow stack recovers those two cells. Its value
  is moving WITHOUT pinning, and at two cells, pinning is already near free.
- **And the hard part of 6.4 is not the stack at all.** It is that **23 side
  tables are indexed by cell index**, so moving a cell means moving 23 entries.
  That sounded like the blocker until `side_tables/release.rs` was read: the
  `tables!` macro already generates a total `release_tables`, so a
  `relocate_tables(from, to)` is the same shape and is **unforgettable by
  construction** — the property P3 praises about `SideTable`.

So what 6.4 still needs, stated so the size is not underestimated again:
**forwarding**. Copying a cell is easy; finding every reference that names it
and rewriting them is the work, and the classic answer (a forwarding pointer in
the old cell, then a pass that updates every reference) has to agree with
`trace::edges_of` about what a reference IS — which is exactly the question
`gc::traces_field` answers and `Repr::Payload` exists to keep honest.

## The alternative the section above does not consider: a shadow stack

*Added 2026-10-03, and it changes the recommendation rather than qualifying it.*

Everything above is about **reconstructing** which values are live by reading
the machine stack, and every difficulty in it comes from that one choice: the
chain is not all ours, Rust frames may keep no frame pointer, so the walk needs
unwind information and an OS API, and it must skip rather than stop.

A shadow stack does not have those difficulties, because **it crosses nothing.**
Compiled code DECLARES its live references into a stack the runtime owns —
pushed where a value becomes live, popped at the end of the frame — and the
collector reads that stack. The Rust frames between two compiled ones are not
walked, not attributed, not skipped: they are irrelevant, because nobody is
walking anything.

**And this is not a new mechanism, which is the strongest argument for it.**
`entry::rooted::Rooted` is already a shadow stack, for the Rust side, and this
repository's own comments are full of it: *"the values interned so far are
exposed between the steps of the very loop that makes them"*. So today two
mechanisms answer "what is live" — `Rooted` for Rust and a conservative scan for
compiled code — which is **P1 with the answer in the wrong shape**: two places
can disagree, and `lost-roots.md` is the record of what that has already cost.
A shadow stack makes it one mechanism with two callers.

### What it costs, stated before it is built

Not free, and the honest comparison is this:

| | unwind walker | shadow stack |
|---|---|---|
| cost while nothing collects | **zero** | a store per live reference |
| crosses Rust frames | needs unwind info + an OS API | **nothing to cross** |
| per platform | Windows / Linux / macOS each | **one** |
| JIT and AOT | two paths to make findable | **the same code** |
| what `describe_frames` is for | map a return address to a frame | say where to push |

So the walker is cheaper when the collector is idle and the shadow stack is
cheaper to be CORRECT. For this engine the second wins, and the reason is the
measured one: the conservative scan is why `examples/entry_probe` cannot run an
allocating row past two thousand iterations, why `region.alloc` costs 8.65 ns
instead of a bump, and why a cell is 128 bytes whatever its layout declares. A
store per reference buys all three.

### Why this belongs to a machine language specifically

A browser engine can afford to reconstruct: it has one platform per port, a
large team per port, and no AOT target. This compiles whole programs ahead of
time to native code on several targets, and `docs/engine/a-second-language.md`
says the machine layer must work with no front end present. A mechanism that
needs `RtlVirtualUnwind` on one target and something else on the next is a
mechanism with a port per target; one that emits two instructions is the same
on all of them.

**P3 is what this is really for**, and P3 says so: *"Go and C# relocate objects
because the pointer map is derived; a conservative scan plus hand-written lists
cannot, because a word that merely looks like a reference must not be
rewritten. So P3 is not hygiene — it is the precondition for the whole of
6.4/6.5."*

### The order, revised

1. **The shadow stack itself**, with the conservative scan still running beside
   it and nothing depending on it yet. Then the two can be compared on a real
   program: every cell the precise set finds must be in the conservative set,
   and the difference is what a moving collector would gain.
2. **`roots::scan_stack` reads it** and the conservative scan is kept behind a
   switch rather than deleted, because this is the class
   `docs/engine/lost-roots.md` is about and a wrong answer here is a
   use-after-free rather than a slow program.
3. **Then 6.4/6.6** — move, bump, exact sizes.

`throw::stack_text` is NOT on this path and should not be made to be. It wants
the frames that are RUNNING, which is a different question from which values
are live, and the previous section's ordering advice — stack traces first
because they do not touch the collector — applies to the walker it was written
about, not to this.

## What wiring the WALKER would involve, kept for the record

Not attempted here, and stated so the size is not underestimated:

- **A frame table in the artifact.** `describe_frames` answers per function at
  compile time; the answer has to survive into the running program, for the JIT
  and for an object file, and be findable from a return address.
- **A stack walker**, and the section above settles which kind: unwind
  information rather than an `rbp` chain, in `rts-host` beside `stack.rs`,
  because the frames between two compiled ones are Rust and may keep no frame
  pointer at all.
- **Two consumers switched over.** `collect_cycle` stops scanning conservatively
  where a frame is described; `throw::stack_text` stops reading `callees`.
- **And the conservative scan stays.** `roots.rs` (B) is explicit that a Rust or
  foreign frame calling into the runtime can never have a descriptor. The
  precise path replaces it for compiled frames only.

The order that follows from the three sections above: the stack-trace consumer
first, because it is the one that unblocks the largest measured item and does
not touch the collector; the collector second, because it is the one that can
produce a use-after-free if it is wrong.
