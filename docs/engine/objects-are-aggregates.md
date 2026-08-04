# An object is a struct, and the machine already says so

Raised as a question: could objects be stored the way Hermes does — an object is
technically a struct, since `this`, a getter and a setter all reach fields at
known positions?

**Yes, and `rts-cranelift` was built that way.** E4 did not use it. This records
what the machine provides, what E4 built instead, what that costs, and what
using it requires.

---

## What the machine already models

`shape/mod.rs` states the design in its own words, and it is the Hermes model:

> A shape **is** an aggregate — it is a way of arriving at one incrementally. So
> the object header still holds the identity of a layout, reading a field is
> still an offset the layout decided, the type guard still compares one number,
> and the collector still traces what the aggregate says to trace. None of those
> learned anything.
>
> That is why reading a property needs no new instruction. Where the layout is
> known, the position is known and it is an ordinary field read. Where it is
> not, a type guard establishes the layout and then it is an ordinary field
> read. **A shape answers *which field*; it does not introduce a second way to
> reach one.**

Four pieces, all present and all connected:

| piece | what it is |
|---|---|
| `mem::HeaderLayout` | 8 bytes, one word: the `TypeId`. Nothing else — no size (the type gives it), no mark bit, no lock |
| `mem::ObjectLayout::of` | header + fields at byte offsets the type registry decided |
| `lower::memory::address_of` | reference → address, as **arithmetic**: `base + index × stride` |
| `ir::FuncBuilder::field_load` / `guard_type` | read at a constant offset; establish which layout first |

The reference inside a value is deliberately **an index, not an address** —
which is what makes conservative scanning safe and a moving collector possible.
`address_of`'s own comment records that this was measured:

> doing it here rather than through a runtime function was worth more than any
> change to the value representation.

Allocation is the one part that stays a call: `Inst::Alloc` lowers to
`RtEntry::Alloc(size, type)`. The runtime hands back a reference; everything
after that is instructions.

---

## What E4 built instead

```rust
enum Cell { Object(Object), Text(Str) }
struct Object { shape: ShapeId, slots: Vec<Value>, prototype: Option<Slot>, accessors: HashMap<…> }
```

A `Slab<Cell>` of Rust values. Every property access is a **call** into Rust that
looks the key up in a hash map and indexes a `Vec`.

None of the four pieces above is used. Not one field is read at a constant
offset, no layout is guarded, and the `TypeId` a shape arrives at is never asked
for.

## What that costs, measured

| | ns per access |
|---|---:|
| E4 as built | **94.8** |
| a bare runtime call, for reference | ~24 |
| the machine's path | a compare, a branch, a load |

The 94.8 is not the cost of dynamic property access. It is the cost of doing it
in Rust behind a call when the design's answer is two instructions.

---

## Why it happened, since that is the part worth not repeating

E4 was scoped as *make property access correct, leave it slow, add the inline
cache after a measurement.* The scoping was right. The implementation ignored
rule 2 of the runtime's own README — **ask whether the machine already answers
it** — which is the same rule that had already caught a duplicated shape tree
and a re-derived value encoding in that crate.

The tell was available and unread: `ShapeTree::layout(shape, types) -> TypeId`
exists, is documented as *"what a shape arrives at is an ordinary aggregate"*,
and E4 never called it.

---

## What using it requires

Not a tweak. The obstacle is real and worth stating precisely.

The machine addresses an object as `base + index × stride` — a **region of
fixed-stride cells**. `rts-core-rwk`'s heap is a `Slab<Cell>` where a cell is a
Rust enum holding a `Vec`. Indices are compatible in spirit; the storage is not.

1. **The runtime's heap becomes the machine's heap.** A region with a stride,
   each cell being a header and inline slots. `RegionBases::single(base, stride)`
   already describes it.
2. **`RtEntry::Alloc` gets an implementation** in `rts-core-rwk`, and the host
   hands the machine the region's base.
3. **An object that outgrows its inline slots needs somewhere to go.** A prior
   measurement in this repository put a bag of overflow at 0.25 ns, so the shape
   of the answer is known.
4. **Then the emitter can use `guard_type` and `field_load`,** and `cached_get`
   gets its first caller.

Steps 1 and 2 are the GC contract, which is the largest single thing the new
runtime has not built. That is the honest size of this.

## What does not change

The correctness E4 established: shape transitions, two objects built the same way
sharing one `ShapeId`, an absent property reading as `undefined`, a property
holding `undefined` shadowing an inherited one. Those are about the object model
rather than about where the bytes live, and they are tested.

## The conclusion, stated plainly

**E4's slowness is not "pending an inline cache". It is pending the object model
the machine already has.** A cache over the current storage would cache a hash
lookup — faster, and still the wrong shape. The cache belongs on top of
`guard_type` and `field_load`, which is what it was designed for.

So the next step is not E4b as previously written. It is the heap.
