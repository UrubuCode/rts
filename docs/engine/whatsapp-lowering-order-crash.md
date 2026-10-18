# WhatsApp Web crashed the compiler: a use lowered before its definition

**Status:** FIXED, 2026-10-18. Was diagnosed to the wrong cause first; this
records both, because the wrong one is instructive.

/ Status: CORRIGIDO. Diagnosticado primeiro à causa errada.

WhatsApp Web did not render because the **compiler** aborted while compiling the
page's JavaScript. The crash was a machine-layer (`rts-cranelift`) bug in the
ORDER blocks are lowered in — not, as first recorded, the frame transform or
invalid SSA.

---

## Symptom

```
[RTS PANIC] no entry found for key
  at crates/rts-cranelift/src/lower/body.rs:1458   (Body::value → self.values[&id])
```

`Body::value` looks a value up in the map lowering fills as it walks the blocks
in `Function::control_order()`. The panic means a block was lowered that reads a
value whose defining block had not been lowered yet — a use emitted before its
definition.

## Reproduction

```bash
cargo build --profile fast --bin rts
target/fast/rts.exe run target/wa/s8.js
```

`s8.js` is Meta's `AbortController` / event polyfill, ~300 KB, mixing heavy
`try` / `catch` / `finally` (92 `try`, 21 `finally`) with its own control flow.
After the fix it compiles and runs; it then stops at `ReferenceError: self is
not defined`, which is the browser global the offline runner does not provide —
a runtime fact about the program, not a compiler fault.

## The real cause (measured)

The crashing function is **not** a generator and does **not** go through
`frame::resumable_form`: `may_suspend=false`, 290 blocks, two protected regions
(a `try`/`catch`/`finally`). Its SSA is valid — a def-dominates-use check over
the augmented graph lowering walks reported zero violations. What was wrong was
the ORDER two places put blocks in. Both are the same defect — ordering by
`BlockId` (creation order) instead of by reverse postorder (control order), so a
definition created after a block that reads it comes out second:

1. **The copy of a cleanup** (`lower/cleanup.rs::cleanup_piece`). A `finally` is
   copied into each path that leaves its region; the copy was lowered in
   block-id order. A `finally` whose body has its own control flow defines a
   value in a block created late and reads it in a block created early, so the
   copy emitted the read first. Measured: `v133`, a call result defined in
   `block105`, read by a `Widen` in `block13`.

2. **Dead code** (`ir/func.rs::control_order`). Blocks the entry cannot reach
   are still lowered (the backend removes them), but were appended in creation
   order. A dead region with the same def-after-use shape then lowered the read
   first. Measured: `v149`, defined in a dead `block112`, read in a dead
   `block8` and `block114`, with `block8` created before `block112`.

## Why the first diagnosis was wrong, and the lesson

The first record (this file's previous contents, and commit `828bdf3b6`) blamed
`frame::resumable_form` for producing invalid SSA — a value used in a `finally`
merge its definition did not dominate — and proposed threading the value as a
block parameter or spilling it to the frame.

Every step of that was refuted by measurement:

- The crashing function is not a generator, so `resumable_form` never touches it.
- Its SSA is valid: nothing is used where its definition fails to dominate it.
  The block the earlier note named as the undominated merge (`block266`) was a
  `Return([v450])` using only its own parameter.
- The value that actually went missing (`v133`, then `v149`) was defined in the
  SAME block that branched on it, or in a dead block — a lowering-ORDER problem,
  not a dominance one.

The lesson is the one `PLAN.md` §0 and `rts-cranelift/README.md` rule 4 already
state: a diagnosis reasoned from what the code *must* be doing, rather than
measured, points confidently at the wrong layer. The fix here is four lines of
ordering in two functions; the proposed frame-transform surgery would have been
large, in the wrong crate, and would not have helped.

## The fix

Both order functions now produce a reverse postorder, so a definition precedes
every block that reads it:

- `cleanup_piece` walks the piece from its entry and returns reverse postorder
  rather than sorting by block id.
- `control_order` walks each dead subgraph from its own entry — a dead block no
  other dead block leads to — and appends its reverse postorder, rather than
  appending dead blocks in creation order. The reachable prefix is unchanged.

Pinned by `crates/rts-cranelift/tests/cleanup.rs`
(`a_cleanup_copy_emits_a_definition_before_its_use_regardless_of_block_id`,
which panics with exactly this error before the fix) and by the unit test
`dead_code_is_ordered_so_a_definition_precedes_its_uses` in `ir/func.rs`.

## Validation

```bash
cargo test -p rts-cranelift                 # the machine layer's invariants
target/release/rts.exe test                 # the *.test.ts suite (honesty floor)
target/fast/rts.exe run target/wa/s8.js     # compiles; stops at `self is not defined`
```
