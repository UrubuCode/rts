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

Measured against test262's `test/language`, 23 724 files: **98.8 % read
correctly** (2026-08-16, `cargo test -p rts-codegen --test test262 -- --ignored`).
That is a reading rate, not a pass rate — nothing runs. `PLAN.md` L9 has the
full table and what each column means.

It was 95.5 % that same morning, and what moved it was `check/` rather than the
bridge: **855 invalid programs we accepted became 73**, with the wrongly-
rejected count unchanged at 215 across every step and the LOST list empty at
each one. The 215 are mostly not ours — 151 of them are one SWC lexer defect,
a reserved word with an escape in the middle of it (`break`), which is
refused as a property key it is legal in.

`RTS_TEST262_REPORT=<path>` writes one verdict per file, which is what makes
that a per-file comparison rather than a net one; `RTS_TEST262_ONLY=<fragment>`
runs a subset, because the whole corpus is minutes in a debug build.

`emit/` turns that tree into the machine's IR. Done: literals — including
strings and templates — locals, declarations, blocks, `return`, control flow,
object literals and property access, functions, calls, `this`, recursion,
closures, `typeof`, classes, tagged templates, regular expressions — the literal and `new RegExp(…)`,
which are one operation reached two ways — and **every operator the
language spells**: arithmetic,
relational, both equalities, bitwise, shifts and `**`.

A value nothing proved is `Repr::Tagged` and its operators are calls, which is
rule 5 rather than a shortcut — `a + b` decides between concatenation and
arithmetic from its operands at run time, so a proven instruction would be wrong
for `"a" + 1` and wrong silently. Where the operands *are* proven doubles, or
where a guard establishes it, the instruction is emitted instead.

Classes land in the same shape: a constructor function, an object on its
`prototype` holding the methods, and two links for `extends` — a lowering rather
than a feature, so nothing in the runtime knows what a class is. A derived
constructor holds its `this` in an environment rather than being handed one,
because the object is the base of the chain's to make — which is what lets
`class Mine extends RegExp {}` produce something with a compiled pattern.

Everything not yet emitted is refused by name, and that list is the work queue.
`PLAN.md` §3b. Iteration and `await` came off it — spread, `for-of` and `async`
functions all emit — and what remains at the top is **generators**: 38 of the
818 files in `tests/`, the largest single entry by a wide margin.

The machine's half of generators is already built and tested.
`rts_cranelift::frame::resumable_form` rewrites a suspending function into one
that takes its frame and answers whether it finished, with a dispatch on a resume
label; `crates/rts-cranelift/tests/frame_transform.rs` verifies, lowers, compiles
and RUNS the result. Nothing calls it yet. What is missing is this side and the
two around it: `function*` emitting a body that suspends, a call creating a
generator object rather than running, and the runtime holding the parked frame.

## Measured, and by what

`cargo test -p rts-host --test suite_coverage -- --ignored`, over the 818
`.test.ts` files in `tests/`, plus `crates/rts-host/examples/suite_run.rs`,
which RUNS each one in its own process because two failure modes take the process
with them.

**2026-08-08: 723 of 818 compile (88.4 %), and 535 of 818 pass (65.4 %).** 179
fail, 9 die. The first number is this crate's floor — a file it cannot compile
cannot pass — and the second is the one that matters, because it is the only one
that ran anything.

Two came off that list and each is worth the sentence. Accessor properties
arrived as a pair kept beside the cell, deliberately absent from the layout, so
that a cached read misses and the runtime is what calls a getter. The argument
vector arrived as an ordinary array the **runtime** holds for the activation —
so a call past the convention keeps its arguments and `...rest` reads them.
What is still fixed at four is the **declaration**: a fifth parameter has no
slot to arrive in, and is refused rather than reading `undefined` forever.

There **is** a global object now: `globalThis` is a real object a program can
reach and write to, an assignment to an undeclared name creates a property on
it, and `typeof undeclared` answers `"undefined"` rather than failing — the
exemption the specification gives `typeof` for taking a reference rather than a
value. The runtime supplied three names when that was written — `Object`,
`RegExp`, `String` — and supplies twenty-one now, including the `Error` family,
`Math`, `Number`, `Boolean`, `JSON`, `Map`, `Set`, `Promise`, `Date` and
`Symbol`. `rts-node` adds what Node makes global without an import:
`process`, `Buffer`, `setTimeout` and `URL`.

One thing is deliberately **stricter than the language**. Reading a name that
neither the runtime provides nor the program ever assigns is refused at compile
time, where the language throws a `ReferenceError` — an error this engine cannot
raise anywhere a handler could catch it. That refusal is wrong only for a
program meaning to catch the error, and the alternative, answering `undefined`,
is wrong for every program with a typo in it. Which names a program creates is
answered by `emit/sloppy.rs`, once over the whole tree, because the read can be
emitted before the assignment that creates it is reached.

A known divergence that **is now closed**, and the entry stays because what it
predicted is what the fix cost. `let` in a loop is a fresh binding per pass; this
engine's environment was per function **activation**, so every pass wrote the
same slot and two closures made in different passes saw the same value. It
affected every loop and arrived with E5.

The paragraph that stood here said the fix "needs an environment created inside
the loop and chained to the function's", and that is exactly what it is:
`loops::open_iteration` builds one at the top of each pass, links its
`__rts_outer` to the environment in force, and `Scope::enter_environment` binds
the pass's names at zero hops while pushing **every other binding one hop
further out**. That last clause is the whole difficulty — `hops` is a
compile-time number, so inserting a link means every binding past it counts
differently, and getting it wrong reads another activation's variable.

Two things are worth keeping written down about the shape.

The set is **two sets**, because they answer different questions. Head names
(`for (let i = …)`) arrive with a value and leave with one, so the update steps
a counter the body may already have moved. Names the **body** declares need no
copy — a `let` in a block is a new binding per pass by definition — but they
must be bound at zero hops all the same, because a captured name is *declared*
into whatever environment is current and a read resolving one hop further would
look in the function's for something written in the pass's.

`var` is in neither, and needs no special case to stay out. A `var` was hoisted,
so reaching its line is a WRITE to a binding that already exists, which resolves
through the chain to where it was hoisted. Only `let`/`const` reach the
declaration path that lands in the pass's own record.

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

The **checker** exists, in `check/`, and the paragraph that stood here said it
did not. It is one walk with every rule riding it — the ones about a *set* (the
names a scope declares, the members a class has, the names a module exports) and
the ones about a *context* (where `await` may name something, where `super`
reaches anything, whether the code is strict). It had two walks until 2026-08-16
and the split had a hole exactly the size of an expression: the set rules never
entered one, so no function *expression* and no class method was ever asked.

`check/regexp.rs` is the odd member and says why in its own header: the pattern
between the slashes has a grammar of its own, and the only other module in the
repository that looks inside one hands the text to the `regex` crate — which
answers with its own language rather than this one's.

What is left is in `PLAN.md` L10, and most of it now needs something the tree
does not carry: the raw text of a literal, or a lexer that reports an escape.

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
`KeyRegistry`, and so does `rts-core`'s runtime interner. That is deliberate
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
  check/    early errors — what a program may not be, said once
  emit/     the tree onto the machine's IR
```

Planned and absent: `scope/` (bindings and what a name resolves to), `types/`
(what a claim is worth and what checks it).

The tree is deliberately shaped for what has to be decided rather than for what
was typed. `a.b` and `a[e]` are different nodes because one has a key and the
other has an expression that must be evaluated first, and a tree that made them
the same would push that difference into every place that reads one.
