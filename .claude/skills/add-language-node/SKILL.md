---
name: add-language-node
description: Emit a JavaScript or TypeScript construct in rts-codegen — the language layer. Use when a program is refused by name at compile time, when a syntax node parses but nothing lowers it, or when a semantic (an operator, a scope rule, a coercion) needs to be decided.
---

# Emitting a language construct

Read `crates/rts-codegen/README.md` in full first — 10 rules — plus `PLAN.md` §3b,
which is the work queue: everything not yet emitted is refused **by name**, and
the refusal names what it waits for.

Run `reuse-check` first.

## 0. What is actually missing

Three different situations, three different fixes:

| symptom | it is |
|---|---|
| the tree has no node for it | a `syntax/` gap — check `PLAN.md` §2 first, the inventory came from the specification |
| the node exists, emission refuses by name | your case — continue below |
| emission wants to know a machine fact | a **missing capability below**. Fix it in `rts-cranelift` (`add-ir-instruction`), never work around it here |

That last row is the one that goes wrong. Where a field sits, how a reference
becomes an address, which convention a call uses, where a barrier goes, what is
live at a collection — none of those are answered here, and none are worked
around here. Reaching for a workaround is how a language layer grows a second,
worse machine layer inside itself.

## 1. Decide the semantic once

`a + b` has **one** definition — not one in a binary expression, another in a
compound assignment, and a third in constant folding. Before writing, find where
the rule already lives. If it does not exist yet, decide where it belongs and put
it there; every other site calls it.

## 2. Proven or generic — say which, and where it stopped being proven

A value nothing established is `Repr::Tagged`, and its operators are calls. That
is the rule, not a shortcut: `a + b` chooses between concatenation and arithmetic
from its operands at run time, so a proven instruction would be wrong for
`"a" + 1` and wrong **silently**.

A TypeScript annotation is **evidence, not proof**. It may prove a representation
where the language can check it, and it becomes a **guard** where it cannot. Any
place a claim becomes a proof must say what checked it.

Where a value goes generic, that happens at the point it stopped being proven —
not three lowerings later where a reader cannot tell why.

## 3. Write it

- emission lives in `crates/rts-codegen/src/emit/` — one focused module, never
  appended to something already large
- if it calls the runtime, the operation must exist as a `RuntimeOp`: use
  `add-entry-point`
- named `emit` and not `lower` on purpose: `rts_cranelift::lower` claims to be
  the only module constructing code-generator instructions, and two things called
  lowering would make that claim uncheckable

## 4. Test against the language

A test that asserts our lowering does what our lowering does is worth nothing.
The claim is always about what **JavaScript** means, and the test name says which
behaviour of the language it pins.

```bash
cargo check -p rts-codegen
cargo test -p rts-codegen <filter>
cargo test -p rts-host running     # the test that runs the program
```

## 5. Coverage is measured, never claimed

If the change moves a number, state the number, what produced it, and the date.
"Mostly works" is not a measurement, and a number quoted from memory is worse
than no number.

## Known divergences — do not "fix" these by accident

- `let` in a loop is a fresh binding per pass in the language; this engine's
  environment is per function **activation**, so every pass writes the same slot.
  Pinned by a test that asserts what the engine does, so fixing it fails that
  test — update the test in the same change.
- Reading a name that neither the runtime provides nor the program assigns is
  refused at **compile** time, where the language throws a `ReferenceError`.
  Deliberate, and stricter than the language.
