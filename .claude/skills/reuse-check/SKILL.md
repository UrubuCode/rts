---
name: reuse-check
description: Mechanical search for an existing answer before writing new code OR a new CONCEPT in the new engine (rts-cranelift, rts-codegen, rts-core, rts-host, rts-macro). Run it BEFORE writing a value encoding, a layout, a numbering, a signature, a queue, a barrier, an interner, or anything that looks like machine bookkeeping — and equally before adding a PREDICATE, a type test, a fast path, a side table, a failure mode, or a second way to ask something the tree already asks. Duplication here is usually not textual: the same question answered in two shapes compiles, passes, and diverges later.
---

# Does something already answer this?

The layering already burned this twice inside `rts-core`'s first three phases:
the value encoding was re-derived (and canonicalised `NaN` differently), and a
second `ShapeTree` was half-written before deletion. Both would have compiled.

**And the expensive kind is not textual.** An audit on 2026-10-01 found eight
families where one question had several answers and no two of them looked alike:
fifteen ways to obtain a cell with three different failure modes, seven ways to
ask "is this an array", three readers of one property, and `f.call` decided by
three predicates that check *different things*. One had already produced a wrong
answer against Node — `JSON.stringify(new Proxy([1,2], {}))` — because one site
asked the array marker where the specification asks `IsArray`. None of the eight
was written carelessly; each was the shortest path at the moment. So this search
is not only for "has someone written this function" but for **"does this question
already have an answer, under another name".** Section 0 is that question and it
comes first.

The rule that forbids it lives in the crate READMEs, and the two documents that
decide the shape are `docs/engine/principles.md` (P1 and P5) and
`docs/engine/one-form-per-question.md`. This is the search that
makes it mechanical.

## 0. Name the question, then search for the question

Before searching for a function, write the question in one sentence — *"is this
cell an array", "may this store skip the barrier", "is this the language's
`Function.prototype.call`", "where does this value's state live"*. Then search for
**the question**, because the existing answer is rarely spelled the way you would
spell it.

`docs/engine/one-form-per-question.md` is the list of questions already settled
and which form is canonical for each. **If your question is in it, you are done —
call that form.** If it is a question nobody has written down, these four tell you
whether it is new or a disguise:

| ask | if yes |
|---|---|
| Can two places answer this differently, and would anything notice? | it is one question, so one predicate — P1 |
| Is this a FAST PATH? Then: taken wrongly, is the result slow or wrong? | wrong means it needs a proof or a guard with a landing — P5 |
| Does this need per-value state? Is it state that every value of that kind has? | in the cell if always, one indirection if rare — `one-form-per-question.md` |
| Does this hand out numbers? | find whose registry it mints from — P6, and section 3 below |

**The tell that you are about to duplicate a concept** is the sentence *"the
existing one does not fit because my caller has different data in hand"*. That is
an argument for changing the existing one's parameters, not for a second one. The
worked example is `gc::traces_field`: it first took a registry and two indices,
measured slower than what it replaced, and became a function of one `FieldLayout`
— same rule, one statement, called by both the trace side and the barrier side.

---

## 1. Search the machine first

`rts-cranelift` owns everything true of the machine. Search it by concern, not by
name — the name you would pick is rarely the name it has.

| about to write | search for | in |
|---|---|---|
| tagging, NaN-boxing, "is this a double" | `encode_double`, `payload_of`, `tag_of`, `CANONICAL_NAN` | `src/tags/` |
| a property → slot map, transitions | `ShapeTree`, `ShapeId`, `KeyRegistry` | `src/shape/` |
| a field offset, a struct layout | `TypeRegistry`, `TypeId` | `src/types/` |
| a signature, a calling convention, a return | `AbiType`, `EntryDesc`, `Signature`, `Convention` | `src/abi/` |
| roots, safepoints, write barriers | `BarrierKind`, liveness | `src/gc/` |
| promises, continuations, run order | `SchedulerId`, `Delivery`, `ContinuationId` | `src/sched/` |
| suspending or resuming a frame | frame record | `src/frame/` |
| try/catch regions, cleanup chains | protected region, handler search | `src/unwind/` |
| a reference → an address | | `src/mem/` |
| a runtime function the machine itself emits | `RtEntry` | `src/symbols/` |
| what an operation costs | | `src/probe/` |

If the machine answers it, **call it**. Depending on `rts-cranelift` is the
design, not a concession.

## 2. Search the crate you are in

Second copies land inside one crate too. Search for the concept, then for the
`Aside<T>` pattern specifically — state beside a cell is the established shape in
`rts-core`, and a new field on `Context` for the same job is a duplicate.

- `crates/rts-core/src/entry/mod.rs` — the `Context` struct is the inventory
  of everything already held beside a cell. Read it before adding a field.
- `crates/rts-core/src/entry/table.rs` — every numbered entry point.
- `crates/rts-codegen/src/runtime/mod.rs` — every operation the language calls.

## 3. Two tables of one number are a bug, not redundancy

Some duplication is correct and some is fatal, and they look alike. The test is
whether the two sides must **agree about a number**:

- Correct: `rts-codegen`'s `Names` and `rts-core`'s `Interner` are two tables
  — different lifetimes, different contents — that mint from **one**
  `KeyRegistry`.
- Fatal: two things minting their own numbers for one space. Two numberings are
  two shape trees one level up.

If your new table hands out numbers, find whose registry it must mint from.

## 4. Report what you found

State the answer before writing: *"the machine already has X, I am calling it"*,
or *"nothing answers this; the nearest is Y, which differs because Z"*. That
sentence goes in the doc comment of what you write.

**And when the answer is a new form of an old question, the report has a second
half**: say which actions the new form is for and which the old one keeps. A form
is canonical for a *stated set of actions*, never in general — two forms are
correct when they answer two questions and a defect when they answer one. If the
new form replaces the old one everywhere, the old one goes in the same change;
leaving both is how the eight families got there.

## 5. If you had to add one, add what refuses a disagreement

A rule stated in one place and called from several is still only a convention
until something rejects a caller who disagrees with it. Both of 2026-10-01's
cases carried that:

- `trace::edges_of` skips a slot the layout declares non-GC — and a debug
  assertion refuses a slot that holds a reference anyway. It fired on the first
  `cargo test` and found five wrong declarations, one of which would have freed a
  suspended generator's locals.
- `side_tables::SideTable` is a total `match`, so **the compiler refuses the
  crate** until a new table is classified.

Pick whichever fits: the builder refuses it at construction, a total `match`
refuses to compile, or an assertion refuses it where the damage would happen.
`rts-cranelift/README.md` rule 7 is the binding form — *invariants are enforced,
not documented* — and a form with nothing enforcing it will be re-duplicated by
whoever next has data in a different shape.

## Then

Read the target crate's `README.md` in full (RULE 0), plus
`docs/engine/principles.md` and `docs/engine/one-form-per-question.md` where the
change touches creation, allocation, identity, tracing, movement, a predicate or
a fast path — the RULE 0 table says which. Then continue.
