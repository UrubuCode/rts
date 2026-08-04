# rts-codegen — the language layer

**Read this file in full before changing anything in this crate.** It is the
mirror of `crates/rts-cranelift/README.md`, and the two are only useful together:
that one says the machine knows nothing about a language, this one says the
language knows nothing about a machine. Either rule alone is a preference. Both
at once is a boundary.

If a change requires breaking a rule, change the rule here first, with the
reason. Do not leave a rule in place that the code contradicts.

---

## What this crate is

JavaScript and TypeScript, as semantics. What `a + b` means, what a scope is,
when a name is in one, what a type annotation is evidence of, what happens when
it turns out to be wrong.

It is a client of the machine layer, and the only one so far. A second — the
language the machine layer was designed to also serve — is the test of whether
that boundary was drawn in the right place.

## What this crate is not

It is not a compiler backend and it is not a runtime. It does not know what a
register is, where a field sits, which calling convention anything uses, when a
collector can run, or that Cranelift exists. If it did, those decisions would be
back at the point of use, taken slightly differently each time, which is the
disease the pair of crates was built to cure.

---

## The rules

### 1. This crate never touches Cranelift

It depends on `rts-cranelift` and on nothing below it. Not for convenience, not
for one case. The moment a language reaches past the machine layer, the machine
layer stops being able to change anything, and the boundary becomes a comment.

**Calling `rts-cranelift` is the point, not a concession.** It carries real work
— the representation lattice, shapes, frames, the collector contract, the
verifier — and reaching around it to do that work again is the failure, not
depending on it. Before writing anything here, ask whether the machine already
answers it. In the neighbouring crate the answer was yes twice in three phases,
and one of the two would have produced a second shape tree disagreeing with the
compiler's about which slot is which property.

### 2. Never decide a machine question

Where a field sits, how a reference becomes an address, which convention a call
uses, where a barrier goes, what is live at a collection — none of those are
answered here, and none are worked around here.

**When this crate is tempted to decide one, a capability is missing below and the
fix belongs below.** Reaching for a workaround instead is how a language layer
grows a second, worse machine layer inside itself.

### 3. A semantic rule is stated once, where it is decided

`a + b` has one definition. Not one in the lowering of a binary expression,
another in the lowering of a compound assignment, and a third in constant
folding. A rule written twice is a rule that will be written differently.

### 4. A type annotation is evidence, not proof

TypeScript types are erased and unchecked at every boundary the program does not
control. So an annotation is treated as a **claim**: it may be used to prove a
representation where the language can check it, and it becomes a guard where it
cannot.

Treating a claim as a proof is the single most tempting mistake available here,
because it makes generated code faster and the program wrong. Any place a claim
becomes a proof must say what checked it.

### 5. What cannot be proven becomes generic, visibly

A value whose representation the language could not establish is generic, and the
place it stopped being proven is where that happens — not three lowerings later
where someone reading the code cannot tell why.

### 6. Semantics are tested against the language, not against ourselves

A test that asserts our lowering does what our lowering does is worth nothing.
The claim is always about what the *language* means, and a test says which
behaviour of the language it pins.

### 7. Coverage is measured, never claimed

What fraction of the language works is a number produced by running something,
and it is stated with the date and the thing that produced it. "Mostly works" is
not a measurement, and a number quoted from memory is worse than no number.

### 8. Files stop at 1000 lines

Same ceiling as the machine layer, same reason: a file approaching it is split
into a folder of cohesive modules, and new code lands in a small focused one
rather than being appended to something already large.

### 9. Documentation is explicit, and says *why*

`#![deny(missing_docs)]` is on, and that is the floor. Where a decision was made
against an alternative, name the alternative and the reason. Where a semantic is
subtle, say what the language actually does — a comment restating the code is
worthless, and a comment restating the code *wrongly* is worse.

### 10. No dead code

`#![deny(dead_code)]` is on. Code no live path reaches is deleted in the change
that stopped reaching it, and a structure with no producer is a gap rather than a
feature. That last one is not hypothetical: the machine layer shipped three of
them and each was found by looking, not by the build.

---

## Working on this crate

`cargo check -p rts-codegen`, `cargo test -p rts-codegen`. Both are seconds. Do
not build the workspace in release to check this crate.

Tests name the behaviour of the language they pin, not the function they call.

---

## State

The tree is complete against ECMA-262 Annex A: every production a tree can hold,
it holds. `PLAN.md` §2 is the inventory, extracted from the specification source
rather than remembered, and §2b lists the rules the grammar enforces through
production shapes and early errors instead of through nodes.

`parse/` fills it, from source text, through SWC — chosen over a hand-written
parser because ASI decides what parses and getting it wrong yields a program that
compiles and means something else.

Measured against test262's `test/language`, 23 724 files: **91.5 % read
correctly**. That is a reading rate, not a pass rate — nothing runs. `PLAN.md`
L9 has the full table and what each column means.

`emit/` turns that tree into the machine's IR. Done: literals — including
strings and templates — locals, declarations, blocks, `return`, control flow,
object literals and property access, functions, calls, `this`, recursion,
closures, `typeof`, and **every operator the language spells**: arithmetic,
relational, both equalities, bitwise, shifts and `**`.

A value nothing proved is `Repr::Tagged` and its operators are calls, which is
rule 5 rather than a shortcut — `a + b` decides between concatenation and
arithmetic from its operands at run time, so a proven instruction would be wrong
for `"a" + 1` and wrong silently. Where the operands *are* proven doubles, or
where a guard establishes it, the instruction is emitted instead.

Everything not yet emitted is refused by name, and that list is the work queue.
`PLAN.md` §3b. The three shapes a gap has today, so the next one can be placed:
it needs a **runtime operation nobody defined** (`instanceof`, which needs a
prototype) or a **mechanism** (a global object, `throw`, classes, `new`,
iteration, and the argument vector rest and spread need). Nothing is left in the
first category — a heap value this crate cannot make — now that strings and
arrays exist.

A **known divergence**, pinned by a test that asserts what the engine does so
that fixing it fails: `let` in a loop should be a fresh binding per pass, and
this engine's environment is per function **activation**. Every pass writes the
same slot, so two closures made in different passes of one loop see the same
value. It affects every loop, arrived with E5, and needs an environment created
inside the loop and chained to the function's.

One entry in that list was **wrong** and is worth recording rather than quietly
fixing: `delete` was said to be impossible because "a shape tree built from
transitions cannot perform it". `ShapeTree::remove` had existed all along, and
its own documentation explains the design — the tree only grows, so removal
rebuilds the layout without the key rather than unlinking a node other objects
share. The claim was made by reasoning about what the tree must be like instead
of reading it, which is the mistake `PLAN.md` §0 records for the grammar.

Named `emit` and not `lower` because `rts-cranelift::lower` is the other half of
the same pipeline, and it claims to be the only module that constructs
code-generator instructions. Two things called lowering would make that claim
uncheckable.

What does not exist yet is the **checker**. The 1 249 programs the corpus says
are invalid and we accept are early errors — redeclarations, duplicate
`__proto__`, `delete` of a name in strict code — rules no grammar production
encodes and no node can hold. `PLAN.md` L10.

## What this crate deliberately holds, having been audited for the opposite

Every module here was checked against `rts-cranelift` under rule 1. What
survived, and why the machine does not already answer it:

| here | the machine has | why both |
|---|---|---|
| `names/` — text ↔ `Name`, and `Name` → machine `Key` | `KeyRegistry`, which hands out opaque numbers | it holds **no text**, deliberately. This is the table that remembers what a number was called |
| `values/` — `Singleton`, `ValueModel` | `TagRegistry`, `SingletonId` | the machine numbers singletons; it does not know one of them is `undefined`, or that `typeof null` is a mistake from 1995 |
| `syntax/`, `parse/` | nothing | a machine has no tree and no parser |

Two pass-through methods were deleted in that audit: `ValueModel::word` forwarded
to `SingletonId::word`, and `ValueModel::unknown` returned a constant. A method
that only re-exports is a second name for one thing, and the second name is the
one that goes stale.

**One number space, two tables.** `Names::key` mints from the machine's
`KeyRegistry`, and so does `rts-core-rwk`'s runtime interner. That is deliberate
and load-bearing: a shape is keyed by `Key` and there is one shape tree, so if
the compiler numbered `"a"` from one counter and the runtime from another,
`obj.a` compiled to a fixed slot and `obj["a"]` resolved at run time would look
up two different keys in one tree. Two numberings are two shape trees, one level
up.

## Layout

```
src/
  syntax/   the tree a program is written in
  names/    identifiers, interned into what the machine keys layouts by
  values/   what a JavaScript value is, registered with the machine's encoding
  parse/    source text in, tree out, through SWC
```

Planned and absent: `check/` (early errors — see PLAN.md L10), `scope/` (bindings
and what a name resolves to), `lower/` (semantics onto the machine's
representation), `types/` (what a claim is worth and what checks it).

The tree is deliberately shaped for what has to be decided rather than for what
was typed. `a.b` and `a[e]` are different nodes because one has a key and the
other has an expression that must be evaluated first, and a tree that made them
the same would push that difference into every place that reads one.
