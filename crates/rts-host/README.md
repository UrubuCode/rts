# rts-host — where a compiler and a runtime meet

**Read this file in full before changing anything in this crate.**

Every other crate in the new engine is deliberately half of something.
`rts-codegen` knows JavaScript and no machine. `rts-cranelift` knows the machine
and no language. `rts-core` implements what the language calls out for and
never decides what to call. None of them can run a program, and that is by
construction rather than by omission.

This is the crate that may name all three at once, so it is the crate where a
program runs.

---

## Why it did not exist until now

It was deferred on purpose, with two stated preconditions: an entry-point path
in the runtime, and a lowering in the language. *"An entry point that nothing
can call is a library with no clients"*, and a host built before either would
have discovered the connection twice.

Both landed, and the third reason arrived on its own: three phases of emission
had been measured against the verifier, which answers *"is this well formed"*
and not *"can this be compiled"*. Reading the first as the second let two phases
ship IR that no destination could accept. A host is what makes that question
answerable at all.

---

## The rules

### 1. This crate holds no semantics

Not one. If something here decides what JavaScript means, it belongs in
`rts-codegen`; if it decides what a machine does, it belongs in `rts-cranelift`;
if it is what an operation does at run time, it belongs in `rts-core`.

The temptation is real and specific: this is the only place where a bug in any
of the three is visible at once, so it is the easiest place to work around one.
A workaround here hides the defect from the crate that owns it, and hides it in
the one file nobody re-reads.

### 2. Make the agreements between the three explicit

Two independent facts have to line up before a compiled program is correct, and
neither is checked by a type:

- **The runtime symbols.** `rts-codegen`'s `RuntimeOp` states the names it
  emits calls to; `rts-core` exports names derived from its Rust function
  names. Nothing ties them together at compile time.
- **The singleton numbering.** `ValueModel::declare` numbers `undefined` and
  `null` from the machine's tag registry; the runtime holds its own
  `Singletons`. If they disagree, `null` compiled by one side is read as
  something else by the other.

Both are wired here, and both are asserted here, because this is the first place
either can be observed.

### 3. This crate does not depend on `cranelift-*` either

The machine's rule 1 says "no other crate in the workspace may depend on
`cranelift-*`", and this crate is not an exception to it. The first version of
this file claimed it was — "this crate may depend on Cranelift where the other
three may not" — and that sentence was written to justify a manifest, not read
from any rule.

What made it look necessary was a leak in the machine's own surface: placement
took a `cranelift_module::Linkage` and handed back a `cranelift_jit::JITModule`,
so the rule and the API contradicted each other and no host could obey both.
That was fixed in the machine, which now says placement in its own vocabulary.

The general form is worth keeping: **when a rule appears to require an
exception, check whether the rule is wrong before assuming it is.** Here it was
not the rule.

### 4. Both destinations, or neither

A program compiled into memory and the same program compiled into an object file
must be the same program. Where the two paths differ, the difference is stated
and is about the destination — not about what was compiled.

They genuinely differ in one thing, and it is not a defect: an object file's
undefined `__rts_add` is resolved by a linker against the runtime archive, while
executable memory has no linker and must be handed the address.

### 5. A test here runs the program

That is the entire reason the crate exists. A test that stops at "it compiled"
belongs in one of the other three, where the cost of running is a dependency
they may not have.

### 6. Files stop at 500 lines

The engine ceiling. This crate is glue, and glue that grows is usually a
semantic decision that drifted in — see rule 1.

---

## What it does not do yet

- **No object-file path.** Only executable memory. The machine has both
  destinations and this uses one, which makes rule 3 a rule about the future.
- **No modules, no functions, no objects.** A host runs what `emit` can produce,
  and `emit` is at E3.
- **No fault handling.** A compiled program that traps takes the process with
  it.
