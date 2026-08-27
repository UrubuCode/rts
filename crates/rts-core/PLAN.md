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

---

## The primordial surface

The phases above built what a program calls. This is what it *names*, and it is
ordered by what a real program stops on first rather than by size. How a class is
declared, and what `#[rtse::class]` derives from that, is
`docs/engine/authoring-natives.md`.

### P1 — `Error` and the subclass family. **DONE.**

`throw new Error(…)` is how every program raises, and the name did not resolve
before this: the emitter refused it as unbound, so the one statement a failing
program is written with did not compile.

`Error`, `TypeError`, `RangeError`, `SyntaxError`, `ReferenceError`, `EvalError`
and `URIError`, each through the attribute, each inheriting the family's
`toString`. What it pinned: `Error("x")` and `new Error("x")` are the same
operation, `name` is read through the ordinary property path so a program that
replaces it is answered, and `class Mine extends Error` reaches `Mine.prototype`
because the native constructor asks `new.target` rather than linking to its own.

`throw` still ends the program — finding a handler in a *caller* needs an
exception table and a personality routine — but it now reports `"Error: boom"`
from a property read rather than `"an object"`.

### P2 — `Math`, `Number`, `Boolean`. **DONE.**

`Math` is an object rather than a compile-time lowering, and the reason is in the
engine document: a lowering is not observably equivalent, because the property is
writable. Thirty-one members and eight constants, none of them written twice.

`Number` is statics and constants; `Number.prototype` is empty and that is a gap
in property access rather than in the class — a primitive number has no cell for
the chain walk to substitute against, so a `toFixed` would be a method no
expression can reach.

### P3 — `Function.prototype`: `call`, `apply` and `bind`. **DONE.**

Substituted by the chain walk, like `String.prototype`. `bind` is the one of the
three that has to *keep* something — a receiver and a list of arguments beside a
cell — and that is a table of values the collector cannot see.

It landed anyway, and the reason is in `function_proto.rs`: **nothing collects
yet**, and the accessor table and the array element table are already values an
eventual collector has to learn about. So this is the same bet those two make
rather than a new one, and it is one more table to trace the day there is a
tracer. This line said `bind` was still waiting long after it was written.

### P4 — `JSON` and `Symbol`. **DONE.**

**A symbol's key is a name in a reserved space** — `"@@iterator"`, `"@@sym:7"` —
rather than a third `Key` variant, which is the engine being replaced's own
encoding ported rather than reinvented. `docs/engine/authoring-natives.md` §4
carries the argument and the one cost: a program writing `o["@@iterator"]` has
written the symbol slot.

`JSON` diverges in three places, each documented where it is paid: a cycle
answers `null` and a parse error `undefined` where the specification throws;
recursion is capped at 200 in both directions, because an `extern "C"` frame
cannot survive a stack overflow; and `toJSON` is not consulted.

### P4b — the iteration protocol. **DONE, and it made a divergence live.**

`iterate` reads `@@iterator`, calls it, and drives `next()`. What was
hypothetical is now reachable: the walk materialises, so an infinite iterable
does not terminate even under `break`, `return()` is never called, and side
effects all happen before the body runs. No cap is imposed — a limit would turn
a program that hangs into one that quietly walks part of a sequence. The fix is
a lazy cursor in the emitter.

### P5 — `Map` / `Set` / `WeakMap` / `WeakSet`. **DONE.**

Insertion order is the source of truth and the hash index only removes the
linear scan, because the specification requires every walk to be in insertion
order. `delete` shifts and rehashes, deliberately the rare slow path. Equality is
SameValueZero through the one `value::same_value_zero`, so `NaN` is a usable key.
A reference hashes from its live slot: the slot is stable while the collection
traces the key, so a moving collector changes neither identity nor reachability.

The weak pair is **strong**, said rather than faked — real weakness needs the
`(slot, generation)` pair C1 describes. What they do enforce is the part that is
not about lifetime: a primitive key is refused.

### P6 — `Promise`. **DONE except the half the compiler owes.**

The machine drives it: `PromiseTable` holds every state and waiter list,
`Scheduler::settle` wakes, `Queues` decides order, and there is no second queue.
What does not fit is `Scheduler::park`, which takes a parked frame and a
`ResumeLabel` — a `.then` callback has neither. That shape is exactly the `await`
half, and `await` is refused by the emitter, so this lands reachable only through
`.then`, `.catch`, `.finally` and the combinators.

The host drains at the end of the turn, in `Compiled::run`. A reaction must not
run in the entry point that queued it, and a rejection is only unhandled once
nothing more can attach to it.

### P7 — the rest of the core list. **Mostly done.**

`Reflect`, `Date`, `ArrayBuffer`/`DataView`/the eight typed arrays,
`Object.prototype`, `Function.prototype` including `bind`, `Number.prototype`,
`structuredClone`, the URI functions, and the `String.prototype` /
`Array.prototype` gaps.

**Still absent, each for a reason rather than by omission:** `Proxy`;
`%TypedArray%` as a shared prototype; the `Iterator` helpers; `AggregateError`,
which `Promise.any` therefore rejects with a plain object carrying `name`,
`message` and `errors`; and `DataView`'s `getBigInt64` / `setBigInt64`, which
would need the numeric-versus-bigint split at each of sixteen methods where the
class-shaped spelling already reaches the same bytes.

### P8 — the two primitives the machine does not define. **DONE.**

`Symbol` and `BigInt` are **kinds**, on two of the four tags
`TagRegistry::declare_kind` leaves to a client — the same shape `undefined` has
as a singleton number, and for the same reason: the language declares them and
the runtime is told, so nothing in `rts-core` names either.

`Symbol` **was** a cell, and that was wrong in the way an implementation detail
is wrong when it is observable. `typeof` was patched to say `"symbol"` and every
other question answered "an object" — correctly for the encoding and wrongly for
the language. As a tag, `s.x = 1` writes nothing, `s instanceof Object` is false,
`Object.keys(s)` is empty, and `Symbol("a") !== Symbol("a")` falls out of
comparing two words.

`BigInt` keeps its digits in a slab, which does **not** make it a reference:
`typeof` answers from the tag and `1n === 1n` compares the DIGITS, exactly as two
string cells with equal text are `===`. Arbitrary-precision arithmetic is
`src/bigint/`, base 2^32 in sign-magnitude, with the two's-complement
interpretation the bitwise operators need.

Two facts this forced, both of which were latent bugs:

- `Value::kind` answered `Reference` for every tag it did not recognise, so the
  first client kind ever declared would have handed its payload to the region.
  It is exhaustive now, and adding a kind is a compile error at every site that
  has to decide.
- `-x` was emitted as `x * -1`, which is right for a double and wrong for a
  bigint: `-1` is a *number*, so the product was a mixed operation the language
  refuses. Every negative bigint literal was `NaN`. It is its own entry point.

**A typed array is not a primitive.** It is an object, and the grouping is worth
correcting because it decides the encoding: an object has properties, an identity
and a prototype, and none of the three fits in a tag.

### P9 — the two typed arrays whose elements are bigints. **DONE.**

`BigInt64Array` and `BigUint64Array` were the one place the bigint work left
unfinished, and the reason was real: the element codec spoke in `f64`. Sixty-four
bits is exactly the width where that stops working — every other integer kind
fits inside 2^53, so a double could carry it, and 2^63 − 1 cannot.

So the codec speaks in **words** now, with the double as one face over them.
`gathered` already produced a `u64` and the write loop already consumed one; what
changed is that both are exposed, and `read`/`write` sit on top for the nine
numeric kinds. The byte order stays written once, which was the constraint: two
codecs would be two places for it, and a typed array and a `DataView` disagreeing
about byte three is invisible until it is not.

`typed.rs` went from `Vec<f64>` to `Vec<u64>` for the same reason — a bulk copy
through a double would round in the range the classes exist for.

**Coercion is refused in both directions**, which the language spells as a
`TypeError` each way. This engine drops the write instead: an element-at-a-time
one leaves the old value, and a bulk one writes zero, because it is building
bytes that did not exist. The second direction is the one an implementation
forgets — the numeric path has a conversion that answers for anything, and
`as_number` of a bigint is `NaN`, which stores as zero. A `Uint8Array` given `9n`
would have been quietly zeroed. It is one function for both families now, because
"does this value belong in this element" is one question.

---

## What the primordials cost in divergence

Collected here rather than scattered, because a reader wants the list in one
place and each entry already carries its argument where it is implemented.

- **No `RangeError`, `TypeError` or `SyntaxError` is ever thrown.** Every
  operation that should throw answers a value instead — `undefined`, `NaN`,
  `null`, or a clamp. `entry/throw.rs` says why: a throw that no handler in the
  throwing function catches ends the program, and finding one in a caller needs
  an exception table and a personality routine.
- **A wrapper object is never made.** `new Number(5)`, `new String("a")` and
  `new Boolean(true)` answer the plain object `construct` allocated, because a
  primitive is not an object and a constructor returning one does not win.
- **`keys`/`values`/`entries` answer arrays**, on `Map`, `Set` and `Array` alike,
  rather than iterator objects.
- **`size`, `length`, `byteLength` and friends are own data properties**, not
  prototype accessors — the trade array `length` already makes, and the reason is
  that compiled code reading one never asks the runtime.
- **`Date` is entirely UTC**, and its time value is a real property visible to
  `Object.keys`.
