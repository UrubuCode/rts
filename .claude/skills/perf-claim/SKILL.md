---
name: perf-claim
description: Establish a performance claim before optimizing anything in this repository. Use before writing an optimization, when about to say something is slow or fast, when choosing between two designs on speed grounds, or when a benchmark number is about to be quoted, committed, or put in a document.
---

# Before you optimize

This repository's record on unmeasured performance premises is bad, and it is
recorded: object layout, method/`this` dispatch, the shape campaign, and tier 3.2
field loads were each pursued on reasoning and each **refuted by measurement**.
Four campaigns' worth of work, and in one of them all four stated premises were
false.

So the order is: falsify first, then optimize.

## 1. Write the falsifier before the optimization

State the claim as something a measurement can **kill**:

- bad: "property access is slow"
- good: "property access spends >50% of its time in the heap, not in the
  generated code — so if I remove the heap call and it does not move, the premise
  is dead"

Then find the thing that would show it false. `typeof` was the falsifier that
resolved the masked-root GC question; `rts ir` separates what we emit from what
the code generator does with it.

## 2. Measure the thing, not around it

```bash
cargo run -- run file.ts             # JIT, no release build
target/release/rts.exe ir file.ts    # what we emit, PRE-optimization
cargo run --release -p rts-cranelift --example probe_run   # what the machine's
                                     # primitives cost, with no client present
```

The third line said `cargo test -p rts-cranelift --test probe` until
2026-08-28, and **that target has never existed** — so the one command this
skill gave for attributing a cost to a layer printed a list of other test
targets and exited. It is an example rather than a test on purpose: a timing
test fails on a busy machine, and the probe is a ruler, not a gate.

`crates/rts-cranelift/src/probe/` measures machine primitives with **no client
present**. That is the property that makes a cost attributable to a layer, and
the row to read first is `call_direct`: it is the cheapest call the machine can
emit, so it is the floor under every call the layers above pay, and anything
above it belongs to them. `docs/codegen/machine-primitives.md` has the current
numbers and what they settle.

**Never benchmark a debug build.** A debug number is not a number.

**And a ruler has to be able to see.** Every probe fixture runs its primitive
inside a counted loop, because until 2026-08-28 each was one operation behind an
indirect call that costs 1.27 ns — so every row landed within noise of every
other and a field read measured *below* an addition, which is impossible. If a
measurement you are about to trust cannot separate two things you know differ,
the instrument is the first suspect, not the code.

## 3. Verify the input, not just the output

A number measured against a corpus quietly smaller than claimed is a claim
wearing a measurement's clothes. This is not hypothetical: a test262 score was
published 0.8 points high because 503 of 24 007 files silently failed to check
out.

Before quoting a number: how many inputs went in, how many were expected, and
what happened to the difference.

## 4. State it so it stays true

A performance claim carries three things or it is not a claim:

1. **what produced it** — the command, the build profile, the corpus
2. **when** — an absolute date, not "recently"
3. **what it does not say** — a measurement of `.ts` body → native symbol is not
   a measurement of "lower it into an instruction", and reading it as one is how
   a 124× number gets used to justify a different change entirely

## 5. An optimization wrong for legal programs needs a guard

And the guard is usually a whole-program fact this compiler cannot establish. If
you cannot name the guard, the optimization is not available yet — record it as a
decision at the call site for later, not as a different runtime.

## The floor

A measured number stays real. No deleting, disabling, skipping, or
input-special-casing a test to move it. A regression is allowed when it is
intentional or a necessary trade, and it is **stated** in the commit with the
reason. "It broke and I don't know why" is never acceptable.
