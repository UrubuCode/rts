# The array fast path that already exists, and why turning it on broke programs

**Attempted and reverted, 2026-08-21.** The machine layer has a bounded scaled
load. The language layer has a producer for it. The producer's admission test can
never be true, so the whole path has never once been emitted. Making the test
satisfiable took an afternoon, moved `array for-of 16` by **−15.3%**, and
produced **wrong answers in a program**.

This document exists because the next person to read `Inst::ElementLoad` will
have the same idea, and the reason it fails is not visible from either side of
the code.

---

## What is there

`crates/rts-cranelift/src/ir/inst.rs` declares it:

```rust
ElementLoad { base: ValueId, index: ValueId, length: ValueId }
```

`crates/rts-cranelift/src/lower/body.rs:367` lowers it to six instructions — an
unsigned bound compare (one test rejects a negative index and a past-the-end one
at once), a `trapnz`, a zero-extend, a scale by eight, an add, and a load.
`crates/rts-core/src/entry/array.rs` provides `elements_base`, which hands out
the address of the run. `crates/rts-codegen/src/emit/foreach.rs` calls it once
per loop and stores the pair in `ctx.set_element_run`.

All of it is written, documented, and lowered. **None of it is ever emitted.**

## Why it is dead

`foreach.rs` gates the whole thing:

```rust
let hoistable = !super::suspends::body_suspends(std::slice::from_ref(body))
    && builder.repr_of(bound) == rts_cranelift::repr::Repr::F64;
```

`bound` comes from `property::emit_read`, and that function's join block
parameter is `UNPROVEN` — `crates/rts-codegen/src/emit/property.rs:87`. Every
property read in this compiler produces a `Repr::Tagged`, however obviously it
holds a number. So the second conjunct is false for every loop that has ever
been compiled, `hoistable` is always false, and `rts ir` on any `for`-`of`
confirms it: no `ElementLoad`, no `__rts_elements_base`.

The *reasoning* behind that gate is correct and unchanged — `to_int32` takes a
proven double, asking it for a generic is `WrongDomain` at emission, and rule 5
of `rts-codegen/README.md` says what cannot be proven becomes generic visibly.
What was wrong was the conclusion. The proof was reachable; it just could not
come from a property read.

## What was tried

`RuntimeOp::ElementsCount` / `CoreEntry::ElementsCount` — a sibling of
`ElementsBase` answering the run's own `Vec::len()` as an `f64`, so `to_int32`
accepts it and `hoistable` reduces to `!body_suspends(body)`. Bounding by the
run rather than by the `length` property is also the more correct of the two,
since `array::set_length` writes the property *from* the element count.

It compiled, passed every unit test in four crates, and moved the table:

| row | before | after |
|---|---:|---:|
| `array for-of 16` | 55.95 | **47.41** (−15.3%) |

## How it fails

`bench/analytic.ts` stopped working. Twenty of its cases reported

```
undefined undefined   UNAVAILABLE  TypeError: c.run is not a function
```

— from its own harness, `for (const c of CASES) rows.push(measure(c))`.

Reduced:

```js
const objs = [];
for (let i = 0; i < 60; i++) objs.push({ v: i });
let bad = 0;
for (const o of objs) {
  for (let j = 0; j < 20000; j++) { const t = { x: j }; }   // allocate
  if (typeof o.v !== "number") bad++;
}
console.log(bad);        // 53 with the change, 0 without
```

The discriminating probe is what names the cause. An array of **numbers** walks
correctly however hard the body allocates. An array of **references** — objects,
or closures — comes back as garbage:

| array holds | body allocates | wrong elements |
|---|---|---:|
| numbers | nothing | 0 |
| numbers | objects, 20 000 per pass | 0 |
| numbers | arrays, 5 000 per pass | 0 |
| **objects** | objects, 20 000 per pass | **53 of 60** |
| **closures** | objects, 20 000 per pass | **57 of 60** |

## The cause

**The fast path drops the only root of the array it is reading.**

`for`-`of` walks the copy `iterate` made — a `Vec<u64>` in `context.arrays`,
reached from an array cell. On the slow path, every element is
`__rts_element_at(enumerated, i)`, so the array value is passed to the runtime on
every iteration and stays live for the whole loop. On the fast path, `enumerated`
is used **once**, in the preamble, to obtain a base address and a count — and
then never again.

After that preamble the array is dead. Nothing in the loop refers to it, so a
conservative scan has nothing to find, the cell is collected, its `Vec<u64>` is
dropped, and `base` points at freed memory. Elements that are numbers survive
because the words are self-describing; elements that are references decode to
cells that have been reused.

`elements_base`'s own documentation states the contract it needs — *"That the run
does not MOVE while it holds this"* — and argues it is met because "the array is
the copy `iterate` just made, no program can name it, and the loop only reads."
Both halves of that are true. **The contract it does not state is that the array
must stay reachable**, and the optimisation that would use the address is exactly
the one that makes it unreachable.

It is the same hazard `array_proto/iterate.rs` already names from the other side,
about `Rooted`: *"What this has produced so far would otherwise live only in a
`Vec` on the Rust heap, which no scan of ours reaches — measured as nine of three
hundred rounds answering wrong data rather than failing."* Nine of three hundred
there; fifty-three of sixty here.

## Why it was reverted rather than patched

Because every available patch is a design decision, not a fix:

- **Root the copy for the loop's duration.** Correct, and it needs somewhere to
  put the root that a scan reaches — `entry::rooted` is the mechanism and it is
  a runtime-side stack, so the loop would have to push and pop it, including on
  `break`, `return`, `throw` and a labelled break out of an enclosing loop. That
  is the same set of exits `foreach.rs` already documents `IteratorClose` as not
  covering.
- **Keep the array value live artificially** — pass it to something per
  iteration. That reintroduces the crossing the optimisation removes.
- **Re-read the base per iteration.** Same.
- **Make the elements addressable from the cell** so no Rust-side pointer is held
  at all. This is the real answer and it is a large one: the array's storage
  stops being a `Vec<u64>` behind a slab and becomes something the machine can
  address, which changes how arrays grow, what the collector walks, and what
  `push` does.

None of those is an afternoon, and shipping the version that measured −15.3% and
answered wrongly is what the honesty floor exists to forbid.

## What this leaves

The instruction, the lowering, `elements_base` and the `ctx.set_element_run`
plumbing are all still there, still dead, exactly as they were before. That is
deliberate: they are a machine capability whose *correct* client has not been
written, not code that stopped being reached.

**The work item is not "enable the gate".** It is: give an array's elements a
representation compiled code can address, so that walking one holds no pointer
into Rust memory. Then the gate can go away entirely rather than becoming
satisfiable.

## What it cost to find out

Nothing that a full release build and one run of `bench/analytic.ts` would not
have cost anyway — which is the argument for rule 2 of this tree's `README.md`
stated from the failing side. The isolated experiment (`element_access.rs`) would
have said the fast path is worth 5–8× on the load itself, and it would have been
right; what no isolated experiment can see is that the surrounding program stops
keeping something alive. **A win in isolation is a licence to build, and the
engine measurement is still the gate.**
