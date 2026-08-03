# rts-cranelift — the machine layer

**Read this file in full before changing anything in this crate.** The rules
below are not style preferences; each exists because the alternative was tried,
or measured, or produced a bug this repository has already paid for. If a change
requires breaking one of them, change the rule here first, with the reason, and
get that agreed — do not leave a rule in place that the code contradicts.

The direction this crate implements is `RTS_CRANELIFT.md` at the repository root.
That document is the design; this one is the working agreement.

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
derived from what a field is and where its object lives — there is no flag on a
store and no place to pass `false`, because a missed barrier produces a
use-after-free that reproduces rarely and explains nothing.

### 9. Effects are declared, never inferred

An operation that allocates receives the safepoint that implies. An operation
that stores a reference receives its barrier. The caller does not remember to
ask, and the caller cannot decline.

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
            constants, functions, the builder
  gc/       the collector's contract: liveness, safepoints, roots, barriers
  unwind/   protected regions, cleanup chains, handler search
  frame/    suspension: what a parked frame preserves, and where it resumes
  sched/    promises, continuations, queues, and the order they run in
  abi/      types, conventions, aggregate classification, multiple returns
  verify/   the rules, and what it reports
```

`gc/`, `unwind/` and `frame/` are three modules over **one** record. A point that
can collect, inside a protected region, inside a function that may park its
frame, is one program point in all three concerns; three tables keyed by it would
be kept in agreement by hand, which is the bug that record exists to prevent.

Planned and deliberately absent: `lower/` and `target/` (the code generator and
the two output paths), `guard/` (bailout), `fault/`, `observe/`, `symbols/`,
`probe/`.

Calls exist now, and they arrived last on purpose. A call is inseparable from the
convention it uses, from the safepoint it implies, and from where the frame is
when control leaves — so every one of those was built and tested first. Choosing
those answers implicitly, then rediscovering them, is the failure this ordering
avoided, and one rule was in fact written and then withdrawn once the suspension
model made it false.

---

## Where to look first

- `RTS_CRANELIFT.md` (repository root) — the design, including section 22, which
  separates what has been measured from what is load-bearing but still unverified.
- `src/lib.rs` — the crate's own statement of scope.
- `tests/invariants.rs` — the rules above, as executable claims.
