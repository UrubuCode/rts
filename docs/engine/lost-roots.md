# A live reference nothing enumerates

*Three of these were found and fixed on 2026-08-29, and the fourth is still
out there. This document is about the CLASS, not about the three — its whole
purpose is that the next one costs an afternoon instead of a week.*

---

## What the class is

The collector is **conservative over the machine stack and precise over
everything else**. `roots::scan_stack` walks `stack_low..stack_high` and keeps
every word that decodes as a reference; `roots::context_roots` enumerates, by
hand, every field of `Context` that holds one; `trace::edges_of` walks, by hand,
every side table a marked cell reaches through.

Both hand-written halves are lists. **A list is a place a thing can be missing
from**, and when something is missing from one of them the program does not
crash at the mistake — it crashes, or answers wrongly, at a collection that may
be minutes later and in another module entirely.

Every instance of this class has the same shape:

> A reference is live, and the only thing naming it is something the collector
> does not look at.

The four hiding places, in the order they have actually bitten:

| where the reference is | why the scan misses it |
|---|---|
| a **side table** keyed by cell (`Aside<T>`) | `trace` walks eleven of them by name; a twelfth is not walked because nobody added the arm |
| a **Rust local** holding a bare `u32` cell index | the scan recognises an encoded `Value`; a raw index is not one |
| a **`Vec<u64>`** a native is building | its buffer is on the Rust heap, which the stack walk does not cover |
| a **callee-saved register** | it is at no address in the scanned range at all |

The third has a guard already — `entry::rooted::Rooted` — and the fourth has
one too, `entry::registers`. The first two are the ones with no mechanism, and
both of 2026-08-29's silent-wrong-answer bugs were in them.

---

## The three, as evidence rather than as history

### An iterator's source — a `for`-`of` that ENDED EARLY and said nothing

`Context::cursors` holds `(listed, at)`: the array an array or string iterator
steps, or the Map/Set whose table a collection cursor walks. `listed` is an
encoded `Value`. `trace` visited `prototypes`, `callables`, `proxies`,
`spill_of`, `array_elements`, `bound`, `views`, `collections`, `generators`,
`helpers`, `accessors` and `boxed` — and not `cursors`.

```js
function arrIter() { return [101, 102, 103][Symbol.iterator](); }
const it = arrIter();          // the array is now named by the cursor ALONE
it.next();                     // {value: 101, done: false}
// …300 000 allocations, one collection…
it.next();                     // {done: true}    — node says {value: 102}
```

**The failure is silence.** A `for`-`of` over such an iterator ends early, the
loop body runs fewer times than it should, and nothing anywhere reports it. The
same held for a Set and a Map.

What makes it worth reading twice is that `roots.rs` had already *classified*
`cursors` correctly — it is in the list of "heap content, not a root", with the
reason "reachable once the owning cell is marked, through that cell's `Trace`
implementation". The classification was right. The implementation it referred
to did not exist. **A correct sentence about a mechanism that was never built
reads exactly like a correct sentence about one that was.**

### A parsed object — a segfault that was a wrong answer first

`json::materialise` allocates the object with `native::plain` and then fills it,
holding it as a bare `u32`. The first allocation inside that window is the
spill: `set_slot_value` reaches `spill_set` reaches `alloc_spanning_or_die`,
which may collect. The cell was swept and handed out again while the loop went
on writing into it.

```js
let src = "{"; for (let i = 0; i < 20; i++) { if (i) src += ","; src += '"k'+i+'":'+i; } src += "}";
for (let i = 0; i < 60000; i++) JSON.parse(src);
```

**Twenty keys, sixty thousand parses, two objects come back with
`Object.keys(o).length === 0`, and the process exits ZERO.** The access
violation everyone remembers is what happens further along, once enough
recycled cells have been written through — so a clean exit proves nothing, and
a small object is not safe.

An object grown the ordinary way never had this. `const o = {}; o.k = v` holds
the object in a machine slot as an encoded value, which the scan does see. **It
is specifically a NATIVE building a cell out of a Rust local that is exposed**,
and the fix is one line: push `Value::from_slot(cell).bits()` onto the `Rooted`
guard the values were already using.

### A remembered key — the mirror image, and it killed the program

The other direction is just as expensive. `Context::remembered_keys` roots the
key string a keyed inline-cache site compares against, and the comment beside it
justified the retention as *"bounded by the program's property names"*.

That is the bound on distinct **keys**. The table is keyed by the **cell** — and
the paragraph two lines above already said why the two differ: *"two different
string cells can spell one key, and it is the cell a site was actually handed
that has to survive."* A fresh cell per pass is a fresh **permanent root** per
pass:

```js
const o = { abcdefg: 1 }; const s = "abcdefghij";
for (let i = 0; i < 2_000_000; i++) a += o[s.slice(0, 7)];
```

`roots 63355 live 65396 freed 5`, and the program died of heap exhaustion over
seven characters that never changed. It is a count now — a site remembers
exactly one key, so the site's previous key is released as its new one is taken
— which bounds the table by the number of keyed SITES.

**Over-rooting is the same bug wearing the other coat.** Under-rooting frees
something live; over-rooting never frees anything. Both are "the root set does
not describe the program", and the second is not the safe direction — it is
merely the direction that fails loudly.

---

## How to find the next one

Four questions, and they are mechanical.

**1. Does every `Aside<T>` that can hold a `Value` have an arm in
`trace::edges_of`?** This is a two-command check and it is the one that found
`cursors`:

```bash
grep -nE '^\s+[a-z_]+: Aside<' crates/rts-core/src/entry/mod.rs
# then, for each field, look for `context.<field>` in trace.rs
```

A field that is deliberately not traced must say so in `edges_of`'s closing
comment, which already lists `regexes`, `integrity`, `attributes`, `derived`,
`buffer_of`, `foreign`, `detached` and `proto_types` and gives the reason for
each. **A field in neither list is the bug** — that is exactly the state
`cursors` was in.

**2. Does any native hold a cell index across something that can allocate?**
The pattern is a bare `u32` or `Slot` in a Rust local, and the allocations that
end the window are `alloc_or_die`, `alloc_spanning_or_die`, `intern_value`,
`object_new*`, `native::plain`, `array::built_in`, and any `put` that can grow a
shape or a spill. `entry::functions` already carries a comment naming this
hazard for its own case; `json::materialise` did not.

**3. Does any native hold values in a `Vec<u64>` without a `Rooted` guard?**
`entry::rooted` exists for exactly this and its header carries the measurement
that made it necessary — nine wrong results in three hundred `map` rounds.

**4. Is any root source BOUNDED?** For every entry in `context_roots`, ask what
limits its size. `literals`, `type_names`, `single_unit_texts` and
`key_texts_as_values` are bounded by the program's text. `remembered_keys` was
not, and the comment claiming it was is the shape to look for: a bound stated
about the wrong thing.

---

## There will be more, and that is the point of this document

**This class is not closed.** Three were found in one afternoon by a sweep that
was not even looking for the second and third, and the sweep did not cover:

- the **other twenty-odd crates** — `rts-std`, `rts-node`, `rts-napi`, `rts-ui`
  and the DOM all build cells from Rust locals, and none was read;
- **`Context::foreign`**, which holds one opaque word beside an object for a
  client, documented as "not a reference: nothing marks it and nothing follows
  it" — true for today's clients and a promise about tomorrow's;
- **a `for`-`of` over a `Set` inside a nested loop**, which still dies with
  `TypeError: __rts_of_it.next is not a function` on the first collection. It is
  narrowed and not fixed: the hand-written `it.next()` protocol over the same
  Set never fails, so it is the `for`-`of` LOWERING and not the iterator; no
  environment object is involved; and it is deterministic per binary but moves
  when an unrelated variable is RENAMED, which is the register allocator's
  signature. `docs/codegen/what-a-property-costs-2026-08-29.md` has the full
  bisection.

The reason to expect more is structural rather than pessimistic. **Every new
side table, every new native that builds a cell, and every new cached value is
a new opportunity to be missing from a hand-written list**, and nothing in the
type system says otherwise. `rts-core`'s rule 10 is what turns that from a
warning into a precondition, and `docs/engine/the-unwired-keystone.md` is what
would end the class rather than police it: with precise roots from a frame
descriptor, the two hand-written lists stop being the thing a reference can be
missing from.

Until then, the honest statement is the one at the top of `roots.rs` and it is
worth repeating here, because it was written about the register half and it is
true of all four: **absence of a failing case is not a proof.**
