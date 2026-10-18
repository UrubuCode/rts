# WhatsApp Web crashes the compiler: invalid SSA from the frame transform

**Status:** diagnosed to the line, NOT fixed. 2026-10-18.

/ Status: diagnosticado à linha, NÃO corrigido.

This records why WhatsApp Web does not render in the engine today, the exact
root cause, how to reproduce it, and the two-step fix plan. It is a
machine-layer (`rts-cranelift`) correctness bug at the intersection of
suspensions (generators / `async`) and exceptions (`try`/`catch`/`finally`) —
the most delicate corner of the engine — so it is written down rather than
rushed.

---

## Symptom

Running WhatsApp Web aborts the **compiler** (it never reaches rendering):

```
[RTS PANIC] no entry found for key
  at crates/rts-cranelift/src/lower/body.rs:1458   (Body::value → self.values[&id])
```

It is not a layout or paint problem. The page loads (14 MB read), then the
panic happens while compiling the page's JavaScript.

## Reproduction

The composed page (scripts already embedded) is cached at
`target/wa/pagina-chrome.html`. The 42 scripts are also split out as
`target/wa/sN.js`.

```bash
# the whole page — crashes
target/release/rts.exe run target/wa/wa-ver.ts        # viewer: reads the cache, opens the egui window

# bisection with the offline driver (runs scripts 0..WA_LIMIT, no window)
WA_LIMIT=7 target/release/rts.exe run target/wa/wa-offline.ts   # OK  (ran 6)
WA_LIMIT=8 target/release/rts.exe run target/wa/wa-offline.ts   # CRASH
WA_SKIP=8 WA_LIMIT=42 target/release/rts.exe run target/wa/wa-offline.ts   # OK (ran 26) — skipping script 8 avoids it

# script 8 alone — the minimal reproducer
target/release/rts.exe run target/wa/s8.js            # CRASH

# accurate backtrace + line numbers need a DEBUG build; release LTO
# mis-symbolises the frames (shows quicksort/OnceLock, which are wrong):
cargo build --bin rts && target/debug/rts.exe run target/wa/s8.js
```

`s8.js` is Meta's `AbortController` / event polyfill, ~300 KB. It mixes
`function*` / `yield` / `async` (suspensions) with heavy `try` / `catch` /
`finally` (92 `try`, 21 `finally`).

## Root cause (measured, not guessed)

Instrumenting `Body::value` to report the missing id, its origin, and the
control-order positions of the defining and using blocks gave:

```
value MISSING id=v133 origin=InstResult(inst92)
  def_block=block105  def_pos=250
  CUR_block=block266  cur_pos=242   cur_preds=[block264, block267]
  total_blocks=290
```

Reading it:

- `v133` is an instruction result defined in **block105**.
- It is **used in block266**, a MERGE (two predecessors, `block264` and
  `block267`), lowered at control-order position **242 — before** block105 at
  **250**.
- Every reverse-postorder places a dominator before the node it dominates.
  block105 appearing *after* block266 therefore proves **block105 does not
  dominate block266**.
- So `block266` uses `v133` without its definition dominating the use: the
  output of `frame::resumable_form` (`frame/transform.rs`) is **invalid SSA**.
  `lower::body::Body::value` then indexes `self.values` with a value that was
  never defined on the path that reaches it, and panics.

`block266` is a transform-created block (high number); its two-predecessor
merge is the shape of a **`finally`** continuation — the block that runs on both
the normal and the exceptional path. A value (`v133`) defined before the `try`
and live past the `finally` must be threaded into that merge as a **block
parameter** (or spilled to the frame across the suspension), and the transform
does neither for this case.

## Why nothing caught it earlier

`verify/` has a dominance checker (`verify/dominance.rs`) that would reject a
use not dominated by its def. But with `RTS_CL_VERIFY=1` the crash still hits
the lowering panic directly — the verifier runs over the **source** IR (which is
valid) and **not over the transform's output**. That is a gap against rule 7 of
`crates/rts-cranelift/README.md` ("invariants are enforced, not documented").

## Fix plan (two steps)

1. **Close the verifier gap** — run `verify/` (at least the dominance /
   def-dominates-use rule) over the **output** of `frame::resumable_form`, so
   this class of bug fails loudly with a precise error instead of a silent
   `no entry found for key` in lowering. This does **not** make WhatsApp render;
   it turns the panic into a clear, early rejection and guards the next step.

2. **Fix the transform** (`frame/transform.rs`) — at a `finally` /
   cleanup / handler merge, thread a value live across it as a **block
   parameter** of the merge (passed by every predecessor's `BlockCall`), or
   spill it to the frame when it also crosses a suspension. The relevant code is
   the region/cleanup handling (`copy_regions`, `dispatch_on_mode`, the
   `split_at_*` helpers and `rewrite_terminator`), and the spill set computed by
   `plan_with_arrivals` in `frame/mod.rs` from `Liveness`.

## Validation required

This touches generators, `async`/`await` and `try`/`catch`/`finally` across the
whole engine, so WPT reftests are **not** the ruler. Validate with the full
TypeScript suite and the crate's own tests:

```bash
cargo test -p rts-cranelift                 # the machine layer's invariants
target/release/rts.exe test                 # the 888 *.test.ts files (per CLAUDE.md honesty floor)
WA_SKIP=-1 target/release/rts.exe run target/wa/s8.js   # the reproducer must now compile
```

Per `RULE 0`, read `crates/rts-cranelift/README.md` in full (13 rules) before
editing the crate; the fix for the verifier gap is itself an instance of rule 7.

## Pointers

- `crates/rts-cranelift/src/lower/body.rs` — `Body::value` (the panic site) and
  `create_blocks` (block params are pre-seeded; the entry order is
  `func.control_order()`).
- `crates/rts-cranelift/src/frame/transform.rs` — `resumable_form`, the rewrite.
- `crates/rts-cranelift/src/frame/mod.rs` — `plan_with_arrivals` (the spill set
  from liveness).
- `crates/rts-cranelift/src/verify/dominance.rs` — the dominance checker to run
  over the transform output.
- `target/wa/` — the cached page, the per-script `sN.js`, `wa-offline.ts`
  (bisection), `wa-ver.ts` (viewer window). Not versioned.
