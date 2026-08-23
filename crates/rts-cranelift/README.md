# rts-cranelift — the machine layer

**Read this file in full before changing anything in this crate.** The rules
below are not style preferences; each exists because the alternative was tried,
or measured, or produced a bug this repository has already paid for. If a change
requires breaking one of them, change the rule here first, with the reason, and
get that agreed — do not leave a rule in place that the code contradicts.

This file is the working agreement **and** the design of record. It used to
point at `RTS_CRANELIFT.md` at the repository root for the second half; that
document was deleted in `e39a21b9` and the pointer stayed, which is a dangling
reference in the one file every change to this crate is required to read in full.
Recover it with `git show e39a21b9^:RTS_CRANELIFT.md` if a decision needs its
reasoning; do not restore it without an owner, because two documents that
disagree is what deleting it was for.

---

## What this crate is

The machine layer. It owns everything that is true of the machine — representations,
layouts, frames, references, calling conventions, the collector's contract,
unwinding — and nothing that is true of a source language.

It is the only crate permitted to depend on the code generator underneath. Every
other crate reaches the machine through this one.

## What this crate is not

It is not a JavaScript engine and it is not a runtime. It does not know what
`undefined` is, what a prototype is, what a metatable is, or that more than one
client exists. If an identifier, a string, or even a comment in this crate names a
source-language construct, that is a defect in the layering, not a shortcut.

---

## The rules

### 1. Only this crate touches the code generator

No other crate in the workspace may depend on `cranelift-*`. Inside this crate,
only `lower/` may construct code-generator instructions; every other module
manipulates this crate's own representation.

Why: if a client can reach one layer past the boundary, it will, precisely in the
cases that matter — and then the decisions are back at the call sites, which is
the disease this crate was built to cure.

### 2. No source-language knowledge, and no record of who is asking

No language name, no language concept, no per-client namespace. Registries take
counts and return encodings; they do not record an owner.

Why: a per-client namespace would let this layer answer "which client owns this
value", and no machine operation needs the answer. A capability that cannot
express a client's identity cannot accidentally depend on it.

### 3. Every module is testable with no client present

If a module cannot be exercised without a front-end, it is in the wrong crate.

Why: this is the property that makes performance attributable. The whole claim
being made — that after this work, slowness belongs to the layer above — is only
meaningful if this layer can be measured alone.

### 4. Documentation is explicit, and says *why*

`#![deny(missing_docs)]` is on: every public item is documented, and that is the
floor, not the goal. A doc comment that restates the signature is worthless. Say
what the thing guarantees, what breaks without it, and what was rejected.

Where a decision was made against an alternative, name the alternative and the
reason. Where a fact came from reading the code generator's source, say so, and
say what was *not* verified. A design document that cannot be checked against
reality decays into one that is wrong, and this repository has already been
burned by exactly that.

### 5. Files stop at 1000 lines

A file approaching the limit is split into a folder of cohesive modules — the
`mod.rs` plus siblings pattern already used here. New code lands in a small,
focused module, never appended to an already-large file.

### 6. No dead code

`#![deny(dead_code)]` is on. Code no live path reaches is deleted in the same
change that stopped reaching it. Never commented out, never kept "just in case".
`todo!()` is an acceptable work-in-progress marker; commented-out code is not.

A constant that is part of a contract is not dead code — make it public and
document it as the contract. That is what happened to the tag numbering, and it
is the right answer rather than a workaround for the lint.

### 7. Invariants are enforced, not documented

Anything this crate states as an invariant must have something that rejects the
program breaking it — the builder refuses it at construction, the verifier
rejects it afterwards, or both.

Both, preferably. The builder makes a mistake awkward to write and reports it
where it was made; the verifier makes it impossible to ship, including for
representations that did not come from the builder.

Documentation that is not enforced degrades into documentation that is wrong.

### 8. Derive what a client would otherwise have to remember

Where a client could forget something and produce a program that is silently
wrong, the layer computes it instead of offering an API to declare it.

Two live examples. Root sets are derived from liveness — there is no entry point
through which a client could report its own set, because a discipline that must
hold at every allocation in every program will not hold. Write barriers are
derived from what a field is, where its object lives, and **how many regions the
heap has** — there is no flag on a store and no place to pass `false`, because a
missed barrier produces a use-after-free that reproduces rarely and explains
nothing.

The third fact is the newest and the one most likely to be mistaken for a
loophole, so it is stated here rather than only in `gc/barrier.rs`: a barrier
reports exactly one thing, a reference crossing from one region into another, and
under `mem::Addressing::Single` there is no second region for it to cross into.
The elision is therefore arithmetic and not a judgement about which stores look
safe — and it is derived in `gc::crossing_is_possible`, in the one place, so it
remains something a client cannot decline or forget. `BarrierKind` gaining a
second non-`None` variant is what invalidates it, and that function's own
documentation says so.

### 9. Effects are declared, never inferred

An operation that allocates receives the safepoint that implies. An operation
that stores a reference receives its barrier, wherever one can report anything —
see rule 8 for the heap that makes a crossing unrepresentable. The caller does
not remember to ask, and the caller cannot decline in either direction: it can no
more force a barrier than suppress one.

Corollary: no hidden flag that changes emitted behaviour underneath the
abstraction. The code generator offers one that silently rewrites a signature
when returns outgrow their registers; this crate does not set it and performs
that rewrite itself, visibly, in one place.

### 10. No operation accepts both a proven and a generic operand

Proven arithmetic and generic arithmetic are separate entry points with different
names and different costs. There is deliberately no single operation that
inspects its operands and branches.

That branch, repeated at every call site, is exactly what this crate exists to
remove. Reintroducing it as a convenience reintroduces the problem.

### 11. Widening is automatic, narrowing never is

Widening to the generic form is inserted by the layer, because it cannot fail.
Narrowing out of it can fail at run time, so it is reachable only through a guard
— and the guard is a terminator, so its failure path cannot be omitted.

### 12. Unproven behaviour fails safely

Where something is not yet verified against the code generator, the conservative
form is the default and raising it is explicit, per target, named for what it
requires.

The live example is multiple returns: the signature type permits any number, but
until a fixture demonstrates a count on a target, the limit is one and everything
else uses an out-pointer. An unproven count then costs an extra indirection
rather than producing a wrong interface.

### 13. Determinism where a human will read the output

Anything a person compares between builds — a root set, a descriptor table — is
ordered deterministically, never left to hash iteration. A diff that changes for
no reason is a diff nobody reads.

---

## Working on this crate

Compile-check with `cargo check -p rts-cranelift`; test with
`cargo test -p rts-cranelift`. Both are seconds. Do not build the workspace in
release to check this crate, and do not run the full TypeScript suite — those are
merge-time activities for the engine, and this crate is reachable from neither
yet.

Tests live in two places, on purpose. Unit tests sit beside the code they check
and cover the local rule. Integration tests in `tests/` state the invariants in
the crate's public vocabulary, as a client would meet them — which is also how
they stay honest about what is actually exposed.

Name a test after the claim it makes, not after the function it calls, and put
the reason in the assertion message. A failing test should explain what is now
untrue.

---

## Layout

```
src/
  repr/     representations and the merge rule
  types/    aggregate layout — the single source of field offsets
  tags/     the generic value encoding and its registry
  ir/       the representation clients build: entities, instructions,
            constants, functions, the builder, and the text a person reads it as
  gc/       the collector's contract: liveness, safepoints, roots, barriers
  unwind/   protected regions, cleanup chains, handler search
  frame/    suspension: the frame record, and the rewrite that produces it
  sched/    promises, continuations, queues, and the order they run in
  abi/      types, conventions, aggregate classification, multiple returns
  verify/   the rules, and what it reports
  lower/    IR -> the code generator. The ONLY module that touches Cranelift
  mem/      object layout, and how a reference becomes an address
  shape/    layouts arrived at one property at a time, and sites that remember
  symbols/  the closed set of runtime entry points
  fault/    what can stop, and where in the client's program it came from
  observe/  which part of the program an address is, and which function
  probe/    what the primitives cost, measured with no client present
  target/   where compiled code goes: executable memory, or an object file
```

`gc/`, `unwind/` and `frame/` are three modules over **one** record. A point that
can collect, inside a protected region, inside a function that may park its
frame, is one program point in all three concerns; three tables keyed by it would
be kept in agreement by hand, which is the bug that record exists to prevent.

Nothing is planned and absent any more. What is left is depth: more fixtures,
more targets, and the numbers the probe produces being watched over time.

`lower/` is not finished, and it is explicit about which half. Scalar work,
control flow, widening, guards, constants and calls are emitted, checked by the
code generator's own verifier, and — through `target/` — compiled and run. Field access reads and writes a real heap; allocation, promises, awaiting and an
escaping throw reach a runtime; a throw a handler catches goes straight there,
because the destination was known while compiling. Cleanup runs on both the throwing path and the ordinary one, and a suspension is
rewritten away before lowering ever sees one. Nothing in the representation is
refused any more.

One capability was added after that paragraph was written, and it was genuinely
missing rather than merely unused: `Inst::FuncAddr` takes the address of a
declared function as a value. `ConstDecl::Symbol` was in the representation with
no lowering — one of the structures with no producer this crate has shipped
before — and it would have been the wrong mechanism anyway, since it addresses
by string what the registry already numbers.

**Five more were found the same way on 2026-08-23, by auditing every variant of
`Inst` and `ConstDecl` for a producer**, which is the search that paragraph
should have prompted and did not:

- `ConstDecl::Bytes`, `Text`, `Symbol` and `StaticRef` — none built anywhere,
  all four answered `NotYetLowered` in `lower_const`, and three carried
  documentation describing behaviour the code did not have. Deleted. What it
  buys past a smaller enum is that **lowering a constant can no longer fail**:
  `lower_const` is total and answers a `Value` rather than a `Result`.
- `Inst::Narrow` — no producer, and its verification was weaker than rule 11.
  `verify` recorded which *representation* a guard proved for a block, never
  which *value*, so a narrow of some other tagged value in that block passed and
  would have reinterpreted a pointer's bits as a double. Deleted rather than
  strengthened: `Terminator::Guard` hands the narrowed value to its success
  block as a parameter, which IS the narrowing, and a second spelling of it is
  what let the weaker check exist. `ir/inst.rs`'s own header already said
  narrowing "is only reachable through `Terminator::Guard`" — that sentence is
  now true.

The rule this leaves, and it is rule 6 read as an instruction rather than as a
prohibition: **audit for producers, do not wait for the build to find them.**
`#![deny(dead_code)]` does not fire on a public enum variant, which is why every
one of these shipped.

Calls exist now, and they arrived last on purpose. A call is inseparable from the
convention it uses, from the safepoint it implies, and from where the frame is
when control leaves — so every one of those was built and tested first. Choosing
those answers implicitly, then rediscovering them, is the failure this ordering
avoided, and one rule was in fact written and then withdrawn once the suspension
model made it false.

---

## Where to look first

- `src/lib.rs` — the crate's own statement of scope.
- `tests/invariants.rs` — the rules above, as executable claims.
- `docs/engine/authoring-natives.md` — how the layers above this one author
  what they call, and which of those decisions this crate owns.

What is **not** here any more: `RTS_CRANELIFT.md`, and with it the section that
separated what had been measured from what was load-bearing and unverified. That
separation is a real thing to keep, and it now belongs in the doc comment beside
the decision — `lower::abi_return_type` is the worked example, naming what was
read from the code generator's source and what was not.
