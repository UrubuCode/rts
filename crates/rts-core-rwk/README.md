# rts-core-rwk — the runtime every target has

**Read this file in full before changing anything in this crate.** The rules
here are binding for changes inside it. If a change requires breaking one, change
the rule first, with the reason.

`PLAN.md` has the phases, what each must decide, and which of the language
plan's traps land in it.

---

## What this crate is

What a compiled program calls that is not an instruction: values, objects, text,
memory, scheduling. Present on **every** target, including wasm — which is what
decides membership, and the only thing that does.

## What it is not

Not a compiler, not a code generator, not a host. It never emits an instruction
and never opens a file.

---

## The rules

### 1. A crate boundary answers "does this exist here?"

Not "may I mention this?". The layering being replaced used the second question —
the engine could not name a class it could not depend on — and a codegen that
reaches its runtime by index cannot name a class at all, so that boundary has no
job left.

What remains is **availability**, which is a build fact. Anything needing an
operating system goes to `rts-host`; anything a browser provides goes to
`rts-browser`; neither is a dependency of this.

### 2. Ask whether the machine already answers it

**Any crate may call `rts-cranelift`.** What is forbidden is reaching past it to
Cranelift. So before writing something here, the question is whether
`rts-cranelift` already answers it — and twice in this crate's first three
phases, it did:

- The value encoding. `tags::encode_double`, `decode_double`, `is_encoded`,
  `encode`, `tag_of`, `payload_of` exist; `Value` calls every one rather than
  re-deriving the arithmetic. The first draft did re-derive it, and
  canonicalised `NaN` as `f64::NAN.to_bits()` where the machine uses
  `CANONICAL_NAN` — a rule written twice is a rule that will be written
  differently.
- Shapes. `shape::ShapeTree` has transitions, memoised slot lookup, a `Repr` per
  key and `layout()`. A second one was half-written and deleted, and it would
  not have been mere redundancy: the compiler emits fixed-offset loads from the
  machine's tree, so a runtime resolving from another would disagree about which
  slot is which property.

### 3. One number space, however many tables feed it

Keys come from the machine's `KeyRegistry` — **the same one the compiler mints
from**. This crate's `Interner` remembers which string got which key; it does
not invent numbers.

Two tables exist and that is correct: the compiler's maps source identifiers, and
lives for a compilation; this one maps strings computed while running, which the
first has never seen. Different lifetimes, different contents, **one numbering**.
Two numberings would be two shape trees one level up.

### 4. This crate holds no language knowledge it can be told instead

`Value` does not know that singleton 0 is `undefined`. The caller passes a
`Singletons`, because a second language on this machine numbers its own
differently.

Where the language's meaning genuinely lives here — the three equalities, the
falsy set, what an array index is — it is because those are operations *over*
values rather than facts about one, and the machine has no opinion about them.

### 5. The machine is reached through `rts-cranelift`, never around it

That is what this rule is about, and it was written as a dependency count —
"`rts-cranelift`, and nothing else" — which read as *few dependencies* and is a
different rule. The count was never the point. **Cranelift is.**

This crate depends on `rts-cranelift` and must not name Cranelift, its IR, or
anything else below that boundary. The reason is rule 2's, from the other
direction: the encoding, the shape tree and the layouts are agreements between
this crate and the compiler, and there is exactly one place they are stated. A
runtime reaching past `rts-cranelift` would be restating one of them — and a
rule written twice is a rule that will be written differently, which here means
a value read as the wrong kind or a property found in the wrong slot.

An ordinary library — `regex` and `fancy-regex` are the ones here, for what a
regular expression literal needs — is not what the rule is about. It decides
nothing the machine decides, so it cannot disagree with the machine about
anything. What it must still satisfy is **availability**, the same build fact
rule 1 names: everything here exists on wasm too, which those two do, being pure
Rust over no operating system.

They are here rather than behind a crate of their own, which is what the engine
being replaced did: `rts-primitives` could not name `rts-shared` without
inverting the crate graph, so it reached the matcher by **link-time symbol
name**. That inversion does not exist here — nothing this crate depends on wants
a regular expression — so the indirection would buy a boundary with nothing on
the other side of it.

### 6. Files stop at 500 lines

Same ceiling as the rest of the workspace outside the two engine crates. A file
approaching it splits into a folder of cohesive modules.

### 7. Documentation says *why*, and names the alternative

A comment restating the code is worth nothing — the code says it already, and
says it correctly. What the code cannot say is what was rejected and for what
reason. UTF-8 was rejected for strings; the reason is in the module, and it is
the point of the module's first page.

### 8. No dead code

`#![deny(dead_code)]` is on. A structure with no producer is a gap, not a
feature. One function was written and deleted before its first commit for exactly
this — it comes back with its caller, in the same change.

---

## Layout

```
src/
  value/    what a value is: kinds, the three equalities, heap-free coercion
  heap/     a table of slots, addressed by index
  text/     strings as UTF-16 code units, in two layouts, and interning
  object/   shapes from the machine, prototypes, and enumeration order
  coerce/   ToPrimitive's protocol, `+`, relational order, number ↔ string
  collect/  mark and sweep over the slot table
  schedule/ what a promise settled with, and whether a rejection was noticed
  entry/    how compiled code reaches all of the above
```

All seven phases are in. What is deliberately absent, each with its reason in the
plan: the write barrier's runtime side (waits for regions), loose equality and
`ToPrimitive` resolution (wait for something that can call), and indexed storage
(waits for arrays).

---

## The `-rwk` suffix

Temporary. This replaces `rts-primitives` and the portable half of `rts-shared`,
neither of which can be removed while references to them remain, and cargo will
not have two crates with one name.

A phase is not finished because the new code exists — it is finished when the old
code is gone, and until then the suffix says which is which.
