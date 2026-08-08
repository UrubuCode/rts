# Generators, and where each half of one lives

A decision, not a plan: the shape below is what the pieces already in the tree
force, and writing it down is what stops the next attempt from re-deriving it —
or, worse, from building a second suspension mechanism beside the one that is
already tested.

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
| `layout.return_fields` | where the results are left |

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

**The host** (`rts-host-rwk`). After emission, a function whose signature says it
may suspend is put through `resumable_form` and the REWRITTEN function is placed.
The host then registers, for that function's code address, the frame's type and
the three field offsets. Keyed by ADDRESS rather than by name or index, because
that is the one thing the runtime already holds about a compiled function.

**The runtime** (`rts-core-rwk`). A generator object holds the frame cell and
whether it is finished. `next(v)` writes `v` into `resumed_field`, calls the code
with the frame's address, reads the finished flag, and answers `{ value, done }`.
`yield` reaches the generator currently being resumed through a stack in
`Context`, the same shape `functions::invoke` already uses to record which
callable is running.

## The one decision that is not obvious

**A call does not need to know it is calling a generator.** Calling a generator
function must not run the body — it must make an object — and the tempting fix is
a flag threaded from the language through `ClosureNew` to the call site.

It is not needed. `functions::invoke` already resolves a closure's code address
before jumping, and the host has registered exactly those addresses. So `invoke`
asks the generator table first: a hit makes the object, a miss jumps as it always
did. The language emits an ordinary closure, the call site emits an ordinary
call, and the fact that lives in one place is registered in one place.

## The open question, and why it is written here rather than guessed at

**Where the frame lives, and how its address reaches the rewritten function.**

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
the wrong words, which is the exact shape of defect this engine's rules exist to
prevent — so it is the first thing to answer, with a test that RUNS a rewritten
function against a runtime-allocated frame, before the other three pieces are
built on top of it.

## What this costs that a state-machine desugaring would not

The rewrite is per function and the frame is a heap cell, so a generator that is
created and never advanced still allocates. A desugaring in the language layer —
the body rewritten into a switch over a state, locals lifted into an object —
would avoid the machine entirely and would also be a second suspension mechanism
living beside a tested one, disagreeing with it about what "parked" means the
first time either learned something. That is the trade, and it was taken in
favour of the machine's.
