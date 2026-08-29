# rts-core — the runtime every target has

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

### 8. A native that calls user code asks whether it threw

Before looking at the answer. `functions::call` answers `undefined` for a call
that did not run, and `undefined` is a VALUE — so a native that carries on
produces effects the language says never happen, or never stops: a spread over
an iterator whose `next` threw filled a vector until the process died, because
`done` read `undefined` and `undefined` is never true.

`throw::in_flight()` asks without clearing, for a native that PROPAGATES: it
returns early and the compiled call site above re-raises. `throw::caught()`
takes, for the one native that HANDLES — the promise drain, where a handler that
threw rejects the derived promise instead of resolving it.

Where the question is unavoidable, the type asks it: `collections::invoke` and
`array_proto::iterate::visit` answer `Option<u64>`, so a caller has to decide
rather than inherit the wrong answer.

Exempt, and it is worth saying why so it does not read as an oversight:
`function_proto.rs` and `reflect.rs` are pure forwarders whose caller is
compiled code, which already checks.

This rule is why a native may raise at all. Raising without it turns one silent
wrong answer into a hang, which is what happened the first time it was tried —
and why the raise and the checks are one change rather than two.

### 9. No dead code

`#![deny(dead_code)]` is on. A structure with no producer is a gap, not a
feature. One function was written and deleted before its first commit for exactly
this — it comes back with its caller, in the same change.

### 10. A reference this crate holds is a reference the collector is told about

Two hand-written lists say what is live: `roots::context_roots` enumerates the
fields of `Context` that hold one, and `trace::edges_of` walks the side tables a
marked cell reaches through. **A list is a place a thing can be missing from**,
and a reference missing from one does not fail where the mistake is — it fails
at a collection, later, somewhere else.

So, when adding anything:

- **A new `Aside<T>` that can hold a `Value` gets an arm in `edges_of`, or a
  line in its closing comment saying why it holds no reference.** In neither
  list is the bug. `cursors` sat in exactly that state and a `for`-`of` over a
  Set ENDED EARLY in silence.
- **A native that builds a cell roots it before anything else can allocate.**
  `cell` is a bare `u32` in a Rust frame and the stack scan recognises an
  encoded `Value`, not an index; `Rooted` is the guard, and `alloc_or_die`,
  `intern_value`, `object_new*` and any `put` that grows a shape or a spill all
  end the safe window. `json::materialise` did not, and twenty-key objects came
  back EMPTY with the process exiting zero.
- **Values in a `Vec<u64>` go in a `Rooted`.** Its header carries the
  measurement that made it necessary.
- **A root source is BOUNDED, and the bound is about the right thing.**
  `remembered_keys` claimed to be bounded by the program's property names while
  being keyed by the cell, and a computed key in a loop exhausted the heap.
  Over-rooting is the same defect as under-rooting, wearing the coat that fails
  loudly.

`docs/engine/lost-roots.md` is the why: the four hiding places, the three
instances with their reproductions, and the four mechanical checks that find
the next one. **There will be a next one** — every new table, native and cache
is a fresh chance to be missing from a hand-written list, and only
`docs/engine/the-unwired-keystone.md` ends the class rather than policing it.

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

All seven phases are in.

`ToPrimitive` resolution and loose equality were listed here as absent, "waiting
for something that can call". That thing was already in the crate: `functions::call`
is how `Array.prototype.map`, `Set.forEach` and every promise reaction run. What
is true is narrower — a call may not happen INSIDE a `with_current` borrow — so
`entry/primitive.rs` holds none across one, and the protocol `coerce` states is
wired at `+`, the four relational operators, `==`, `String()`, `Number()` and
`join`.

What is still deliberately absent: the write barrier's runtime side (waits for
regions).

`Symbol.toPrimitive` was listed here as the second absence and is not one:
`entry/primitive.rs` consults it ahead of `valueOf`/`toString` for every hint,
and — since 2026-08-16 — an object answered by the hook is the `TypeError` the
language says it is, rather than a fall-through to the ordinary pair. So is an
object whose two methods both answer objects. That raise is rule 8 applied from
the raising side, and it is what `symbol/codex3_012_toprimitive_nonprimitive_throws`
measures.

**A collector is no longer among them**, and this paragraph said it was: `collect/`
marks and sweeps, `entry/collect_cycle.rs` runs a cycle from `entry/alloc.rs`
when the region fills, and `entry/roots.rs` answers what is live — the context's
own fields, a conservative scan of the machine stack, and (since `rts-napi`
needed one) what something outside the heap is holding, in `entry/external.rs`.
What is NOT there is precision: `rts_cranelift::gc::describe_frames` is finished
and has no caller, so a compiled frame cannot be asked which of its slots are
live and the stack half stays conservative.

**A native can raise a catchable error**, and the discipline that had to come
first is rule 8. `throw::type_error` builds the program's own `TypeError`, so
`e instanceof TypeError` holds; two sites raise today — calling something that is
not a function, and an object whose `Symbol.iterator` answers something with no
callable `next`.

This was tried once and reverted before commit, and the reason is worth keeping:
raising while no native ASKED turned one silent wrong answer into a hang. The
checks and the raises are one change.

---

## The `-rwk` suffix, and where it went

This crate was `rts-core-rwk` while `rts-primitives` and the portable half of
`rts-shared` still existed and cargo would not have two crates under one name.
The rule the suffix encoded was: **a phase is not finished because the new code
exists — it is finished when the old code is gone.**

Both were deleted on 2026-08-10 and the suffix came off, everywhere, in the same
change. It is worth keeping the rule written down because it is being used
again: `rts-napi` carries it today, beside the `rts-napi` it replaces.
