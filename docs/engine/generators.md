# Generators, and where each half of one lives

**Built. A program that yields runs**, and the suite went from 535 to 551 of 818
the day it landed. What follows is the design as it was decided, with the two
places the built thing differs from it marked — kept rather than rewritten,
because the reasoning is what a later change needs, and a document that only
describes the current code cannot say what was rejected.

Two defects surfaced while wiring it, both older than the work and both in the
machine: the frame rewrite carried a constant's NUMBER into a pool where it named
the resume label, and lowering walked blocks in creation order rather than by
dominance — which agree until a pass rewrites control flow, which is exactly what
this one does.

## What already exists, and is not called by anything

`rts_cranelift::frame::resumable_form(func, &mut types) -> Resumable` rewrites a
function whose signature says `may_suspend` into one that can be parked and
picked up again. `crates/rts-cranelift/tests/frame_transform.rs` verifies the
result, lowers it, compiles it and runs it — seven tests, so this is proven
rather than promised.

The contract it produces, exactly:

| | |
|---|---|
| the rewritten function | `extern "C" fn(frame: i64) -> i64` — one argument, one answer |
| the answer | whether it FINISHED. Not the value |
| `layout.ty` | the aggregate the frame is an instance of; `ObjectLayout::of` gives its size and field offsets |
| `layout.label_field` | the resume label. Zero means "not started"; each suspension point has a number |
| `layout.resumed_field` | written by whoever resumes, read by the frame. One slot, because only one suspension can be outstanding |
| `layout.mode_field` | HOW this resumption was made — see `frame::ResumeMode` |
| `layout.return_fields` | where the results are left |

`mode_field` is what makes `gen.return(v)` and `gen.throw(e)` re-enter the body
rather than abandon the frame, and it is a MACHINE capability rather than the
emitted operation this document originally proposed. The reason is the `return`
half: it is a *terminator* inside the region the `yield` was written in, not a
call that could be emitted after the suspension — so what runs on the way out is
whatever those regions owe. The rewrite already owns every suspension point, so
deriving the triage there rather than offering it as something a client
remembers is rule 8 of the machine layer.

`resumable_form` therefore takes the throw TAG as a parameter: the machine
compares tags and does not interpret them (rule 2), so it cannot choose one.

Everything a value needs to survive a suspension is in that record, because the
rewrite stores such values where they are DEFINED rather than reloading them
where they are used — there is no definition to reconstruct, which is what makes
the rewrite sound over arbitrary control flow.

## The four pieces, and which crate owns each

**The language** (`rts-codegen`). `function*` stops being refused: the body is
emitted with `may_suspend` set, and `yield x` becomes two operations rather than
one — a call that hands `x` to the runtime, then `Inst::Suspend`, whose RESULT is
the value the next resumption delivers. There is deliberately no `Yield`
instruction carrying a value: the machine's suspension is generic on purpose, and
teaching it what a generator yields would put a language fact in the machine.

**The machine** — nothing. This is the point of writing the document: the
temptation is a new instruction, and the answer is that `Suspend` plus an
ordinary call already spells it.

**The host** (`rts-host`). After emission, a function whose signature says it
may suspend is put through `resumable_form` and the REWRITTEN function is placed.
The host then registers, for that function's code address, the frame's type and
the three field offsets. Keyed by ADDRESS rather than by name or index, because
that is the one thing the runtime already holds about a compiled function.

**The runtime** (`rts-core`), in `entry/generator.rs`. A generator object holds the frame cell and
whether it is finished. `next(v)` writes `v` into `resumed_field`, calls the code
with the frame's address, reads the finished flag, and answers `{ value, done }`.
A second difference from what this proposed: `yield` was to reach the generator
being resumed through a STACK in `Context`, the shape `functions::invoke` uses
for callees. It is **one slot**. The value is written by `GeneratorYield` and
taken by whoever resumed the instant the call returns, so a generator advanced
from inside another's body has already had its own taken — the nesting is a stack
because the calls are. A stack would hold the same value for longer and would
need a discipline to stay in step with control flow a `throw` can leave.

## The one decision that is not obvious, and what was built instead

**A call does not need to know it is calling a generator.** Calling a generator
function must not run the body — it must make an object — and the tempting fix is
a flag threaded from the language through `ClosureNew` to the call site.

This document proposed asking a table in `functions::invoke`, which already
resolves a closure's code address before jumping: a hit makes the object, a miss
jumps as it always did. **That is not what was built, and the reason is where the
cost lands.** The table is consulted before EVERY call in the program, so every
ordinary call pays a lookup for a fact that is true at one site — the definition.

What was built is a **wrapper**, the shape `wrap_async` already uses for the same
reason: `function*` emits its body plus an ordinary function that hands the
body's ADDRESS to `GeneratorNew` and answers the object. The closure, the call
and the caller are unchanged, `invoke` is untouched, and the fact lives at the
one site where it is true. The table that remains is the frame's SHAPE by code
address, which the runtime needs regardless — it cannot allocate a frame without
knowing how big it is.

## Where the frame lives — answered by running one

**The question was: does the rewrite's byte-offset addressing describe the same
memory the runtime's cells do?**

The rewritten function takes `Repr::Ref(RefKind::Aggregate(ty))` and reaches its
fields by BYTE OFFSET, through `ObjectLayout::field_offset` and the addressing
`lower::memory::address_of` emits. The transform's own test sidesteps the
question: it leaks a byte buffer the size of the layout and declares it as a
region of one object with `RegionBases::single`.

The runtime's heap is not that. `Region::alloc(size, ty)` answers a cell, and
cells are read and written by SLOT — `field(reference, slot)` — over a fixed
stride. So before any of this is written, one thing has to be established rather
than assumed: whether a frame can be an ordinary cell whose slots line up with
the aggregate's fields, or whether it needs its own region.

Getting that wrong is not a compile error. It is a generator that runs and reads
the wrong words — so it was answered first, by
`crates/rts-host/tests/generator_frame.rs`, which RUNS a rewritten function
against a frame the runtime's own heap handed out, under the addressing
`compile_for` builds rather than one shaped to suit a test.

The answer, in three parts:

- **A frame that fits a cell needs nothing new.** The region's reference is the
  argument unchanged, and field N is slot N.
- **That correspondence holds only because every field is a machine word.** The
  type registry packs by natural alignment, so a frame holding a narrower spill
  would put field N somewhere other than slot N, and the runtime would have to
  read by byte offset. A test pins this rather than leaving it implied.
- **A frame wider than a cell lives in the cells after it.**
  `Region::alloc_spanning` takes consecutive cells and answers the first one's
  reference; `spanning_field` / `set_spanning_field` are the runtime's side,
  bounded by the caller's layout. Nothing about the addressing changes — a
  region's cells are consecutive words of one allocation, so an object crossing
  a boundary is still contiguous and `base + reference × stride` still reaches
  it. Both the allocation and the access stay O(1) and cost the two instructions
  one cell costs.

Six parameters and a return is nine words against a cell's seven, so this is not
an exotic case. `Region::alloc` still refuses an oversized object: a caller that
spans has to say so, and everything else keeps the property that one reference is
one cell. The rejected alternatives were a region of its own — not expressible,
since a compiled program has ONE addressing and therefore one stride — and an
out-of-line spill like the property overflow, which the rewrite cannot use
because it reaches its fields by byte offset from the frame's address.

## What this costs that a state-machine desugaring would not

The rewrite is per function and the frame is a heap cell, so a generator that is
created and never advanced still allocates. A desugaring in the language layer —
the body rewritten into a switch over a state, locals lifted into an object —
would avoid the machine entirely and would also be a second suspension mechanism
living beside a tested one, disagreeing with it about what "parked" means the
first time either learned something. That is the trade, and it was taken in
favour of the machine's.
