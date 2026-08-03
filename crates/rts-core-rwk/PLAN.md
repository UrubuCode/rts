# The core runtime plan

What a compiled program calls that is not an instruction, and the order it gets
built in.

---

## What decides membership

Two questions, and only the second draws a crate boundary:

> **Does this exist on every target?** If no, it belongs in `rts-host` or
> `rts-browser`. If yes, it belongs here.
>
> **Is this an instruction?** If yes, it belongs in `rts-cranelift`. If no —
> because it touches the heap, the operating system, or global mutable state —
> it is an entry point, and the ones that exist everywhere are here.

Nothing here may name a language construct. `Value` does not know that singleton
0 is `undefined`; the caller passes that in. A second language on this machine
numbers its own singletons differently, and hardcoding JavaScript's numbering is
the knowledge this crate exists without.

## What it depends on, and why only that

`rts-cranelift`, for the value encoding, and nothing else.

Not for convenience. This crate must agree with the machine's bit layout
exactly, and it agrees by **reading the same constants** rather than copying
them — a change to `BOX_BASE` or a tag number is a compile error here instead of
a value silently read as the wrong kind.

Every dependency added here is a dependency on **every** target, wasm included.
That is the bar a new one has to clear.

---

## Before writing anything: ask whether the machine answers it

Any crate may call `rts-cranelift`; what is forbidden is reaching past it to
Cranelift. So the first question about a new module here is whether
`rts-cranelift` already has it — and in the first three phases the answer was
yes twice:

- **The encoding.** `tags::encode_double` and friends exist. C0 re-derived them
  and canonicalised `NaN` differently; both spellings agree on this target and
  nothing makes them have to.
- **Shapes.** `shape::ShapeTree` is more complete than what C3 began writing,
  including the `remove` the new file said it was deliberately omitting.

What survived the same question, and why: the slot table (the machine's `gc/`
and `mem/` are compile-time — frame descriptors, liveness, how emitted code
computes an address; a runtime table is the other side of that contract),
strings (the machine holds no text), and the equalities (the machine has no
opinion about whether `NaN` equals `NaN`, and should not acquire one).

## The phases

Ordered by what the next one cannot be built without. Each lands as its own
commit, with tests naming the behaviour they pin.

### C0 — values. **DONE.**

The encoding, the three equalities, and the conversions that need no heap.

16 tests. What it pinned: `NaN` canonicalisation (an arithmetic `NaN` must not
read back as a reference), the two cells where `===`, `SameValue` and
`SameValueZero` disagree, `ToInt32` wrapping where a cast would saturate, and
`to_number` refusing a reference rather than guessing zero.

### C1 — the heap. **DONE.**


Everything below needs somewhere to put things.

A slot table, not an allocator handing out addresses. The machine's 48-bit
payload is a **slot index**, which is what makes the NaN-box safe to scan
conservatively: a payload can never be mistaken for a pointer because it is not
one.

Decisions this phase has to make and write down:

- **Where the generation lives.** Sixteen bits of generation do not fit beside
  48 bits of slot in a 48-bit payload. So a live `Value` carries only the slot,
  and the generation is checked slab-side — which is sound only because a live
  value keeps its slot reachable. `WeakRef` and `FinalizationRegistry` are the
  exception and need the full `(slot, generation)` pair, so they get a wider
  handle rather than the boxed one.
- **What a slot holds.** One tagged union, or one per kind. The first costs a
  branch on every read; the second costs a table per kind and a way to tell them
  apart from the payload alone.
- **Growth.** A slab that moves invalidates nothing, because indices are
  indices — which is the property that later makes a moving collector possible
  without rewriting every reference.

### C2 — text. **DONE.**


Strings, and one decision that is very hard to reverse.

**A JavaScript string is a sequence of UTF-16 code units.** `length`, `charAt`,
indexing and every surrogate-pair oddity follow from that. Storing UTF-8 makes
`s[i]` a scan and `length` a lie; storing UTF-16 doubles the memory of ASCII
text, which is most text. The usual answer is to store both shapes and remember
which — one-byte latin1 when every code unit fits, two-byte otherwise — and that
is a decision to make deliberately here rather than discover later.

Also this phase: interning, because a property key is a string and comparing
keys by content is what interning exists to stop.

Deliberately **not** here: `Number::toString`. It needs the shortest
round-tripping decimal, which is its own problem (§5.5 of the language plan) and
belongs with conversion.

### C3 — objects. **DONE.**

The shape tree is `rts_cranelift::shape::ShapeTree`, **not one of ours**. A
second was half-written here and deleted: the compiler emits fixed-offset loads
from the machine's tree, so a runtime resolving a dynamic property from another
would disagree about which slot is which property. What this phase owns is the
part the machine refuses to know — that a key can be an integer index, which is
what enumeration order turns on.


The phase where the machine layer's shape support got its first real client —
and where a class turned out to need nothing new. A class declares its fields,
so the shape is known before any instance exists: walk the transitions once at
definition time, keep the `ShapeId`, and `layout()` hands compiled code one
aggregate with fixed offsets for every instance. `shape_for` is that walk.

Three traps from the language plan land here, and each is a test:

- **The prototype walk carries the original receiver** (§5.8). Recursing with
  the parent as receiver breaks `this` in every inherited getter.
- **The walk stops on a descriptor, not on a value** (§5.8). An own property
  explicitly set to `undefined` shadows the parent; stopping on
  `value != undefined` falls through to the wrong one.
- **Enumeration order is not insertion order** (§5.9). Array-index keys first in
  ascending numeric order, then the other strings in insertion order, then
  symbols. A single insertion-ordered slot list — the obvious backing for a
  shape — is wrong for any object mixing the two, which is most arrays.

**Left open, deliberately.** Whether a property is data or an accessor is held
on the object rather than in the shape. It belongs in the shape — two objects
differing in it differ in how a read behaves, which is what a shape is for — and
it is not there because the machine's shape carries a `Repr` per key and no
notion of kind. Adding one is a change to the machine, which is where the fix
belongs; inventing a second layout notion here to avoid asking is the failure
the READMEs name. Cost until then: a map per object, empty for nearly all.

### C4 — coercion. **DONE.**

Nothing here calls anything. `ToPrimitive` on an object runs user code, so an
operation handed one returns `Needs` — which operand, which hint — and the
caller resolves it and asks again. Same shape as `Found` in C3, same reason:
what is easy to get wrong is not performing the conversion but performing it on
the right operand in the right order.

**The states that are not digits are owned elsewhere, and this crate holds only
what it uses.** `NaN` and the infinities are `f64` values whose bit patterns the
machine owns; their two *spellings* are constants in `coerce::number`, beside
the two functions that write and read them, because a constant written in two
directions must not become two literals that drift.

`undefined` and `null` are singletons the **language** declares —
`TagRegistry::new` says the singleton space "is entirely the client's" and
numbers nothing itself. Their numbers arrive as `Singletons`, passed in and never
assumed, and their spellings live with the declarer (`Singleton::type_of` in
`rts-codegen`). A first attempt put them here too, in a file of their own; they
were used by nothing but their own tests, and `deny(dead_code)` does not catch a
`pub` constant. **This crate does not know that "undefined" names a singleton,
and must not learn.**

Still absent: `ToPropertyKey` beyond what `object::key_of` already does, and
loose equality, which needs `ToPrimitive` resolved and therefore a caller that
can call. Both land with the first client rather than being written blind.


Now that a heap and strings exist, the conversions that need them.

`ToPrimitive` and its hints, `ToString`, `ToNumber` on a string, `ToPropertyKey`,
the polymorphic `+`, and loose equality.

Four more traps, and they are the expensive kind — each has an implementation
that passes the obvious test and is wrong:

- **`+` decides after coercion, not before** (§5.1). `[] + {}` concatenates
  though neither operand is a string.
- **`a <= b` coerces its right operand first** (§5.3), because it is specified
  as `!(b < a)`.
- **`Number("")` is `0`** (§5.4), and hex/octal/binary string forms reject a
  sign.
- **Number → string is the shortest round-tripping decimal** (§5.5). `0.1`
  prints `"0.1"`; `0.1 + 0.2` prints `"0.30000000000000004"`. No fixed precision
  is right for both.

### C5 — collection. **DONE.**

Mark and sweep over the slot table. What the machine already decides is not
re-decided here: where a collection may happen, what is live at that point, and
whether a store needs a barrier are all derived in `rts_cranelift::gc` while
lowering. What arrives here is a list of slots.

**The write barrier's runtime side is deliberately absent.** `BarrierKind` has
two cases and the second is `CrossRegion` — a notification that one region now
refers to another. There are no regions yet, so a remembered set would be a
structure nothing writes and nothing reads. It lands with regions, not before.


Mark and sweep over the slot table, and the write barrier's slow path.

The mark phase has one requirement the old collector did not: it reads
**NaN-boxed** words. A stack word is a root when `(w & BOX_BASE) == BOX_BASE`
and its tag is a reference — which is *more* precise than a conservative scan
that treats any word resembling a handle as one, because a float that happens to
look like a handle stops being a false positive.

Written down now because it constrains C1: the slot table has to be walkable
without knowing what put anything in it.

### C7 — the entry surface. **DONE.**

How compiled code reaches this crate, and the two decisions it forced.

**The boundary is scalars**, so state cannot be a parameter: `u64`, `i64`,
`i32`, `f64`, `bool` and strings cross `extern "C"`, and a `&mut ShapeTree`
never will. An operation needing the heap therefore reaches ambient state. The
rejected alternative is threading a context pointer through every call site: it
works, costs a register everywhere, and lets a caller pass the wrong one.

**One context per thread**, not one behind a lock. A global lock would serialise
every property read, which is the opposite of what a per-region heap is for.

Membership is the machine's rule unchanged, and it does real work: `to_int32` is
not an entry point because it is arithmetic; `add` is, because joining two
strings allocates; `to_boolean` is, for one falsy case out of seven.

This phase also resolved what C1 deferred — `Cell` is the union of what a slot
holds, which arrives now because there are finally two kinds to unify.

### C6 — scheduling. **DONE — and almost all of it was the machine's.**

`rts_cranelift::sched` owns the promise state machine, waiter lists, cycle
detection on adoption, the queues, and handing woken continuations to the
scheduler that owns them. `Delivery::Elsewhere` even makes the publication
obligation explicit when a promise settles across regions. None of that is
repeated here.

What it deliberately does not hold is a **value** — a `PromiseCell` is a state, a
waiter list and an owner, and `Settlement` carries nothing. The machine models
control; which value a promise resolved with is data the language chose. So this
phase is two things: a side table from promise to value, and whether a rejection
was ever looked at.

The second is language policy with a rule that fails quietly: **a rejection is
unhandled if nothing waits on it when the TURN ends**, not when it settles.
`Promise.reject().catch(f)` attaches afterwards and is not one. Reporting at
settle time warns about correct code, which teaches people to ignore the warning.


Promise state, the microtask queue, parking and resuming a frame.

The machine layer owns the frame transformation and the scheduler contract; this
is the state machine those act on. Last because it is the only phase whose
correctness is mostly about *ordering*, and ordering is easiest to get right when
everything it orders already works.

---

## How this is measured

Not yet, and saying otherwise would be the failure the language plan spent a
commit correcting.

Unit tests pin behaviour per phase and prove nothing about coverage. The real
number needs the whole engine running — test262's `built-ins` tree is what will
produce it, and it cannot run until a program can execute end to end. Until
then: no percentage, and the trap list above is the checklist.

What *is* claimable per phase: which of the language plan's §5 traps have a test,
by number. That is a count of pinned behaviours, not of passing tests, and it
will be stated as such.

---

## The `-rwk` suffix

Temporary. This replaces `rts-primitives` and the portable half of `rts-shared`,
neither of which can be removed while references remain, and cargo will not have
two crates with one name.

It goes away when they do. A phase is not finished because the new code exists —
it is finished when the old code is gone, and until then both are in the tree and
the suffix says which is which.
