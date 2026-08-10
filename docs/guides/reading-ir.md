# Reading what the engine emitted

`rts ir` prints the IR for a program without running it. It is the loop an
optimization is checked in: emit, read, change the emitter, read again.

```bash
cargo run -- ir file.ts                       # a file, with its import graph
cargo run -- ir "let x = 1 + 2; console.log(x)"   # an inline snippet
cargo run -- ir file.ts > before.txt          # stdout, so redirect works
```

## What you are looking at

`rts_cranelift::ir` — the representation `rts-codegen` builds and
`rts-cranelift` lowers. **Not** Cranelift's `.clif`, which exists only inside
`lower/` and only after every decision this engine makes has already been taken.
If the question is "what did the emitter decide", this is the right side of it;
if the question is "what did the register allocator do", this cannot answer.

## The shape

```
; callees
;   FuncId(0) <anonymous>
;   FuncId(5) soma
;   FuncId(6) __rts_add

; FuncId(0) <anonymous>  ; the program's entry
function(Tagged, Tagged, Tagged, Tagged, Tagged, Tagged) -> (Tagged) [Internal], entry block0
    c0 = Scalar { repr: F64, bits: ScalarBits(4613937818241073152) }
block0(v0: Tagged, v1: Tagged, …):
    v6: F64 = Const(c0)
    v31: Tagged = Widen(v6)
    v32: Tagged = Call { callee: FuncId(6), args: [v15, v8, v31] }
    Branch { cond: v35, then_block: BlockCall { block: block11, args: [] }, … }
```

- **The legend** is the only place a `FuncId` has a name. A runtime operation
  shows its symbol (`__rts_add`); a function from the program shows what it is
  called, or `<anonymous>`. The machine's registry records no names on purpose
  (its rule 2), so nothing else can say.
- **`v3: I64 = …`** is a value and its representation. `Tagged` means the
  representation was not proven at that point.
- **`Widen`** is a proven value entering the generic form. A pair of them around
  one operation is the emitter failing to prove something it could have.
- **A block per path.** Every call is followed by a `__rts_thrown` check and a
  branch, which is what a catchable throw costs.
- **`; at 41`** on an instruction is the source position, when one was recorded.

## What to look for

| you see | it means |
|---|---|
| `Widen` then an immediate use in a generic call | a type the emitter could have proven and did not |
| `Call { callee: FuncId(n) … }` for arithmetic | the generic path — `__rts_add` rather than `IntArith` |
| the same `Const` declared several times | a literal not shared across sites |
| a `CachedGet` with a `miss` block that does real work | an access whose shape is not stable |

Two dumps diff cleanly: everything is walked in table order, never over a map
(the machine's rule 13). Take one before a change and one after, and `diff` is
the review.

## Before quoting any of it as a speed claim

Read `CLAUDE.md`'s performance rule and invoke the `perf-claim` skill. A dump
says what was emitted, not what it costs — fewer instructions is a hypothesis,
and this repository has a written history of that hypothesis being wrong.
