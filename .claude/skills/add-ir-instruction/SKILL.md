---
name: add-ir-instruction
description: Add or change an instruction, a representation, or a machine capability in rts-cranelift — the machine layer. Use when a lowering in rts-codegen cannot express something and the missing capability is a machine question (a layout, a convention, a barrier, a guard, a frame, an effect), or when the verifier refuses IR that should be legal.
---

# Adding a machine capability

Read `crates/rts-cranelift/README.md` in full first — 13 rules, all binding.
Run `reuse-check` first.

## 0. Is it a machine question?

If it decides what a **source language** means, it does not belong here. No
language name, no language concept, no per-client namespace — not in an
identifier, not in a comment.

If `rts-codegen` is tempted to decide a machine question, the fix is here rather
than there. That is the correct reason to be in this file.

## 1. The order, and why it is this order

Work outward from the representation. Each step is checkable before the next
exists.

1. **`src/repr/`** — if a new representation is involved, the merge rule first.
2. **`src/types/`**, **`src/shape/`**, **`src/mem/`** — if it changes layout or
   how a reference becomes an address.
3. **`src/ir/inst.rs`** — the `Inst` variant. Operands are `ValueId`s; the doc
   comment says what it guarantees.
4. **`src/ir/builder.rs`** — the constructor. The builder is where a mistake is
   made awkward to write and reported where it was made.
5. **`src/verify/`** — the rule that rejects the malformed form. Both the builder
   and the verifier, preferably: the verifier also catches representations that
   did not come from the builder.
6. **`src/lower/`** — the only module that may construct code-generator
   instructions.
7. **`tests/invariants.rs`** — the claim, in the crate's public vocabulary.

## 2. The rules this step usually trips

- **Effects are declared, never inferred.** An operation that allocates receives
  the safepoint that implies; one that stores a reference receives its barrier.
  There is no flag to pass, and no place to pass `false`.
- **Derive what a client would otherwise have to remember.** Root sets come from
  liveness; barriers come from what a field is and where its object lives. Do not
  add an API through which a client declares either.
- **No operation accepts both a proven and a generic operand.** Proven and
  generic are separate entry points with different names and different costs. A
  convenience that inspects operands and branches is the disease.
- **Widening is automatic, narrowing never is.** Narrowing is reachable only
  through a guard, and the guard is a terminator so its failure path cannot be
  omitted.
- **Unproven behaviour fails safely.** Not verified against the code generator
  yet? The conservative form is the default; raising it is explicit and per
  target, named for what it requires.
- **Determinism where a human reads the output.** Root sets, descriptor tables —
  ordered, never left to hash iteration.

## 3. One record, three concerns

`gc/`, `unwind/` and `frame/` are three modules over **one** record. A point that
can collect, inside a protected region, inside a function that may park its
frame, is one program point in all three. If your change adds a table keyed by a
program point, it belongs in that record.

## 4. Verify

```bash
cargo check -p rts-cranelift
cargo test -p rts-cranelift <filter>
cargo test -p rts-cranelift --test invariants
```

Both are seconds. Do not build the workspace in release to check this crate.

The code generator's own verifier answers *"is this well formed"* and **not**
*"can this be compiled"*. Reading the first as the second already let two phases
ship IR no destination could accept. A capability is proven when
`crates/rts-host-rwk/tests/running.rs` runs a program through it.

## 5. What is not a machine question

A structure with no producer is a gap, not a feature — this crate has shipped
three. If you add a variant, add what constructs it and what lowers it in the
same change, or add `todo!()` and say when it arrives.
