# The profile oracle: who tells a guard what to bet on

**2026-10-03.** The guard, the two tiers and the lateral fall exist and are
neutral. The inline cache exists and is neutral. What does not exist is anything
that says **which** assumption is worth making at a given site, and this document
is about that gap: what fills it, what the filling may and may not know, and why
it is the one piece of this that earns a crate of its own.

It is not a plan. `crates/rts-codegen/PLAN.md` is where work is queued.

---

## The question this engine is actually facing

"Should RTS do what V8 does" is the wrong question, and it was asked and answered
badly once already in conversation before the code was read. The engine is not a
JavaScript engine that might grow a second client; `rts-mir` is a crate
*because* the IR belongs to neither front end, and
`docs/engine/a-second-language.md` is the measured cost of the second one. So any
answer that only works for JavaScript is not an answer.

Read that way, the comparison comes apart into three levels that are usually
discussed as one — and they have **different** answers:

| level | neutral? | where it lives |
|---|---|---|
| the **mechanism**: guard, two tiers, the fall, the cached access | yes, obligatory | `rts-mir`, `rts-cranelift` |
| the **producer**: "at this site, bet on int32" | **no, and it must not be** | each front end |
| the **oracle**: how often this site saw what | yes, obligatory | nowhere yet |

The middle row is the one that surprises. A neutral producer would be the union
of two languages' assumptions, which is exactly what `rts-mir/src/domain.rs`
refuses in its own header: *"a shared lattice would be the union of the two, so
each language pays for the other's cases, or it would be the first language's
under a neutral name. Both are worse than a parameter."* Deciding what to bet on
**is** declaring language meaning. It belongs where meaning is declared.

## The mechanism is already done, and that is a finding rather than a claim

Checked on 2026-10-03 rather than assumed:

- `rts-mir/src/guard.rs` makes `Assertion(pub u32)` **opaque**, and says why in
  the type's own doc: *"'is this an int32' is one language's question and 'is
  this an integer rather than a float' is another's"*. The Lua/JavaScript
  divergence was anticipated in the design of the guard, not retrofitted.
- `Domain::narrow` is the only thing that turns an `Assertion` into a type, and
  the trait's doc calls it *"the only thing that makes the specialised tier know
  more than the generic one"*.
- `rts-mir` rule 1 is clean mechanically: `grep -ric
  "javascript\|undefined\|nan\|prototype"` over `crates/rts-mir/src/` answers
  zero.
- `tests/toy_domain.rs` is a second domain with integer distinct from float and
  a two-case truth rule, which is rule 10's answer to *"a boundary with one
  client on each side is indistinguishable from no boundary at all"*.

And the inline cache is neutral for a reason worth stating, because it reads as
the most JavaScript-shaped thing in the engine and is not. `cached_get`,
`cached_get_keyed` and `cached_get_indirect` are built in
`rts-cranelift/src/ir/builder.rs` — **the machine** — over `shape::ShapeTree` and
`shape::Key`. `shape/mod.rs` describes its own subject as *"objects whose layout
is arrived at rather than declared… most of what a **dynamic client**
allocates"*, and `a-second-language.md` lists the shape tree and the key
numbering among the things that **need no change for a second language**.

A Lua table is precisely "a layout arrived at rather than declared". The cache
serves it already.

## So the gap is the oracle, and its first problem is the key

The obvious implementation records what the compiler already has in hand. All
three candidates are **local to one compilation**, and this was checked:

| candidate | why it cannot be the key |
|---|---|
| `CacheId` | `ir/entity.rs`: `pub struct CacheId(pub(crate) u32)` — a dense table index, and not even public |
| a key number | `shape/key.rs`: `KeyRegistry { issued: u32 }` hands them out in the order the compilation asks |
| a shape id | minted as the program grows, in that order |

Recording any of them produces a file valid only for the compilation that wrote
it, which is the same as writing nothing. **And it is the failure mode that does
not announce itself**: the file loads, the numbers match *other* sites, every
guard still passes, and the speculation is simply pointed somewhere else. Nothing
asserts an answer that would be wrong — this is the second silent class the
honesty floor names, a rule applied to the wrong thing rather than a wrong
result.

The key that works is **`(module identity, Position)`**, and the reason is
pleasing: `fault/mod.rs` already defines `Position(pub u32)` as *"a number the
client gave us. This layer never interprets it, never orders it, never renders
it"*, and explains that a position this layer understood would be a position it
could be wrong about. So **the key space already belongs to the language** — the
only party able to make it stable across compilations, and whose own space Lua
would own for itself. The machine carries the number without reading it, which
is what it already does.

## The payload is names, never numbers

Same argument one level down. A record of "shape 7" or "key 12" is meaningless
in the next compilation, so an observation carries:

- a property as its **string**, not its `Key`;
- a callee as its **stable symbol**, not its index;
- a shape as the **ordered set of property names** that defines it, not its id.

This costs bytes and is the only form that crosses two compilations. A profile
file is not a cache of the compiler's internals; it is a statement about the
program, in terms the program's source uses.

## The contract that keeps it neutral: it counts, it does not type

The oracle answers *"at this site, 9 731 of 9 740 arrivals had the properties
`x, y` in that order, and nine did not"*. It never answers *"this is a float"*.

Turning a frequency into an `Assertion` is `Domain`'s job, in the front end,
because that is a judgement about what the language's semantics make safe to
assume. The oracle does not know what it observed means, in the same way
`rts-mir` does not know what a `Prim` computes and `rts-cranelift` does not know
what a `Position` points at. One mechanism, used a third time.

This is also the line that makes the neutrality testable rather than asserted:
if a field of the record can only be filled in by a language, the field is in the
wrong crate.

## Why it is a crate, which nothing else in this conversation was

The dependency graph decides it. Read from the manifests on 2026-10-03:

```
rts-core     → rts-cranelift
rts-codegen  → rts-cranelift, rts-mir
rts-mir      → rts-cranelift
```

The **writer** is `rts-core`, the **reader** is `rts-codegen`, and there is **no
edge between them** — they are siblings. A format both must name can therefore
live in neither.

The third option is `rts-cranelift`, which both already see, and it is refused:
the record carries property **names** and callee **symbols**, and a machine layer
holding a table of a language's property names is the half of the boundary that
`a-second-language.md` says does not relax. `tests/neutrality.rs`, added the same
day, would fail on the first fixture.

That is the same argument that made `rts-mir` a crate — it belonged to both sides
and therefore to neither — and it is the only piece here that passes it. The
guard did not need one; the cache did not need one; a producer of assertions
belongs inside a front end by definition.

## What is deliberately NOT bought

The expensive half of what V8 does is not the speculation. It is **obtaining the
profile without ever having seen the program**, and the apparatus that requires:
an instrumented tier 0, on-stack replacement so a long-running loop can be left,
and invalidation of code already installed, under threads.

None of it is taken. Two reasons, and the second is the stronger:

- **The data is already in hand at zero marginal cost.** When a cached access
  misses, the runtime is *already* on the slow path and *already* holds the name
  it looked for and the shape it found. Instrumenting is incrementing a counter
  where the cost has been paid. There is no tier 0 to build because there is
  nothing to instrument that is not already running.
- **Go reaches guarded devirtualization with none of that apparatus.** Its
  profile is a file from a previous run, read at build time; the compiler emits
  the concrete-type test, inlines, and leaves the interface call in the `else`.
  Published gains are typically single-digit percent whole-program. That is the
  same *shape* as .NET's dynamic PGO with none of its runtime machinery — and
  this engine has an AOT mode (`rts compile`) where the file form is the natural
  one.

So: the mechanism of .NET, the delivery of Go, and nothing of the V8 tier
pipeline. `docs/engine/what-the-literature-does-not-buy.md` is the precedent for
pricing a published technique against this tree rather than adopting it.

## The ordering, and what would make this not worth doing

`rts prove` already counts, per function and per tier, where the proofs stopped —
widenings, guards, cached accesses and runtime operations, split between the
armed path and the generic one. It is the measurement that decides whether an
oracle is worth building at all:

- if the widenings sit where a `Domain` fed by **static** information could
  already narrow, the oracle buys nothing and the work is in the producer;
- if they sit on genuinely polymorphic callees and property sites, that is the
  case only observation answers.

**It was run on 2026-10-03, and the answer is no — not yet.** `rts prove` over
the thirteen programs of `bench/`, 485 functions, from `target/release/rts.exe`:

```
settled   1 740 widened, 3 165 guarded, 2 074 cached, 3 895 runtime operations
fallback  3 373 runtime operations
```

What the SETTLED path — the one that runs when every speculation holds — asks
the runtime for, by count:

| | | what would remove it |
|---:|---|---|
| 962 | `__rts_string_const` | nothing observable: it is a LITERAL |
| 831 | `__rts_call_counted` | a direct call, which whole-program already knows |
| 462 | `__rts_thrown_address` | structural, after a call |
| 428 | `__rts_closure_new_light` | allocation |
| 288 | `__rts_global_get` | whole-program resolution |

**2 971 of 3 895 — 76% — in five entries, and not one of them is a question an
observation answers.** What a `Domain` would narrow sits far below: `add` 84,
`to_boolean` 61, `number_remainder` 35, `strict_equals` 27. About 5.3% together,
and a static type answers most of it without observing anything.

And the number that settles the ordering: **`calls 0 direct, 0 through a value`
in all thirteen.** 831 calls and no direct ones. Guarded devirtualization is the
right technique for exactly that traffic — but there is nothing to devirtualize
INTO while a direct call is not emitted at all, and `graph.rs` puts every file of
a program in one compilation, so a large share of those callees is statically
known. The static step comes first and is strictly larger.

The `fallback` column is dominated by `__rts_get_property` (1 325) and that is
not evidence either: the fallback runs when the cache misses, so it is not a cost
while speculation holds. **And the oracle's natural target — which layout to bet
on — is what the inline cache already obtains at run time with no profile at
all.** That is the deeper reason this is premature here: the oracle would buy
information the cache already gets by itself, and would not buy the information
that is missing.

One limit on all of the above, stated because the command states it: `prove`
counts occurrences in the IR, not executions — "counts, never costs". So 962
`string_const` means the construct appears 962 times, not that it runs 962 times.
What the figures authorise is a conclusion about **where the proofs stopped**,
which is the question asked. They authorise nothing about time.

So the crate stays as the format, with neither end wired, and the next work is
the static producer: a direct call where the callee is known, a literal
materialised without the runtime, a global resolved across the one compilation.
The oracle becomes worth building when a report like the one above is dominated
by sites that are genuinely polymorphic — and the same command answers that.

Running it before building is the honesty floor's "verify the input" applied to
a design rather than to a number. And one correction worth recording, because it
was stated the other way round earlier the same day: **a TypeScript annotation is
not the neutral source.** It is a fact about one language family — Lua has
nothing corresponding — so it enters as *one producer of assertions among
several*, inside `rts-codegen`, and never as the premise of the mechanism. An
observed frequency is the neutral source, which inverts the obvious ordering in
the same way `a-second-language.md`'s conclusion does.

## What falsifies the design

The toy domain is the model. A profile the three-type toy domain can write and
read — integer distinct from float, two-case truth — exercised in the crate's own
tests, with no front end present. If a field of the record cannot be filled
without naming a language, the leak is found at the cost of one file rather than
one crate, which is the only reason to run the experiment early.
