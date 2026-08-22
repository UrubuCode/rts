# Is the context behind every entry point why the runtime costs 16–30 ns?

**No. It is worth 0.53 ns of them.** Settled 2026-08-21, before any engine code
was written, by `bench/isolated/src/bin/entry_tax.rs`.

This is the first document in this tree and it says no on purpose. Rule 1 of
`README.md` exists because of what this investigation would have cost if it had
been done in the usual order.

---

## The hypothesis

`bench/analytic.ts` has a band of rows that a machine performs with a load and a
compare, all landing between 16 and 30 ns while `bun` and `node` sit at 0.5:

| action | rts | bun | node |
|---|---:|---:|---:|
| `array index read` | 16.63 | 0.71 | 0.70 |
| `array index write` | 16.88 | 0.52 | 0.47 |
| `prop typeof` | 22.79 | 0.54 | 0.38 |
| `binary Uint8Array read` | 24.65 | 0.49 | 0.52 |
| `call closure var read` | 26.00 | 0.67 | 0.40 |
| `binary Float64Array rw` | 27.66 | 0.35 | 1.90 |
| `call method` | 29.35 | 0.52 | 0.38 |

Six unrelated operations, one number. A band that tight across operations with
nothing in common is a **shared cost**, and there is an obvious candidate: all of
them are things compiled code cannot do inline, so all of them cross into
`rts-core`, and every entry point in `rts-core` reaches the heap through one
function.

`crates/rts-core/src/entry/current.rs:218`:

```rust
pub(crate) fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CONTEXTS.with(|stack| {
        let mut borrowed = stack.borrow_mut();
        let Some(context) = borrowed.last_mut() else {
            eprintln!("rts: an entry point ran with no context installed on this thread");
            std::process::abort();
        };
        body(context)
    })
}
```

over `static CONTEXTS: RefCell<Vec<Context>>` at line 162.

Counted by eye that is a thread-local access, a `RefCell` borrow (a store, a
compare, and a second store when the guard drops), a `Vec` pointer load, a length
load, an emptiness check, and a scaled index — call it a dozen instructions,
paid by every runtime operation in the engine. The hypothesis writes itself: put
a raw pointer to the current context in a thread-local `Cell` and the band
collapses.

**It does not.** The dozen instructions are there. They are worth half a
nanosecond.

---

## The experiment

`bench/isolated/src/bin/entry_tax.rs`. Six shapes, each doing identical visible
work — read a counter out of a context, add to it, store it back — so the only
difference between rows is how the context was reached. Each is called through
`#[inline(never)] extern "C"`, because that is what an entry point is: the
optimiser cannot see into it, cannot hoist the thread-local access out of the
caller's loop, and must treat caller-saved registers as clobbered.

The `Context` stand-in is 384 bytes with a `u64` first field, so that
`last_mut()`'s scaled index is a real multiply rather than a shift, as it is in
the engine.

```
Experiment 1 - reaching the context from an entry point
shape                                             ns/op   vs first
----------------------------------------------------------------------
1. RefCell<Vec<Context>>  (engine today)          2.337      1.00x
2. RefCell<Context>       (no stack)              1.852      0.79x
3. Cell<*mut Context>     (memo of the top)       1.837      0.79x
4. &mut Context passed in (the floor)             1.809      0.77x
5. nothing reached        (call + loop only)      1.192      0.51x
6. shape 3 + throw-pending check                  1.900      0.81x
```

Four runs, release, 2026-08-21, Windows 11 Pro 26200. Spread across runs: shape 1
2.295–2.393, shape 3 1.837–1.848, shape 5 1.187–1.234. The differences below are
several times the spread, so they are differences and not noise.

---

## What the rows say

Subtract downwards and the whole cost decomposes:

| | ns | what it is |
|---|---:|---|
| shape 5 | 1.19 | the loop and one non-inlinable `extern "C"` call. **Nothing about the context can remove this.** |
| shape 4 − shape 5 | 0.61 | the field read-modify-write itself, shared by every shape |
| shape 3 − shape 4 | 0.035 | reaching via a thread-local pointer — *free* |
| shape 2 − shape 4 | 0.04 | reaching via a thread-local `RefCell` — also free |
| **shape 1 − shape 4** | **0.53** | reaching the way the engine does |

Three findings, and the second is the one worth remembering.

**The saving available is 0.53 ns per entry-point call.** Against a 16.63 ns row
that is 3.2%. Against the 200 ns string cluster it is 0.3%. It is real, it is
reproducible, and it is not the answer to anything in the table.

**It is not the `RefCell`, and it is not the thread-local.** This is the part
that was guessed wrong and matters most for the next investigation. Shape 2 keeps
the `RefCell` and the TLS access and drops only the `Vec` — and lands within
0.04 ns of passing a raw pointer as an argument. So a `RefCell` borrow costs
nothing measurable, a `thread_local!` with `const` initialisation costs nothing
measurable, and **the entire 0.53 ns is the `Vec`**: load the pointer, load the
length, check it is non-zero, multiply by 384, add.

That inverts the fix. "Replace `RefCell` with something cheaper" would have been
a rewrite of the borrow discipline for no gain. What the measurement points at is
much smaller: keep the stack, keep the `RefCell`, and stop *indexing* on the hot
path.

**The throw-pending check is free.** Shape 6 adds the read compiled code performs
after every call that can raise, and it costs 0.063 ns.
`crates/rts-core/src/entry/current.rs:41` claims that moving `THROWN` out of the
`Context` and into its own thread-local made the check cheap and was worth 3–6%
on a loop whose body is one array element read. The first half is confirmed
here. Nothing about the check is worth revisiting.

---

## So where are the 16–30 ns?

Not established here, and this document does not guess. What it does establish is
the budget the rest of an entry point has to fit into, and it is the whole thing:

```
  16.63 ns   array index read
−  1.19 ns   the call and the loop
−  0.53 ns   reaching the context, as done today
= 14.91 ns   everything else — 90% of the row
```

The IR is where the next look goes, and one dump of it already contradicts the
"crossing is the cost" framing. For `a += arr[i & 1023]`,
`rts ir` emits, per iteration:

```
ToInt32(i) ; ToInt32(1023.0)         a constant, converted at run time
Bitwise(And) ; ToF64 ; Widen         unbox, mask, re-box into a Tagged value
Call __rts_get_indexed(arr, <boxed>) the index arrives NaN-boxed
WordLoad(thrown) ; Compare ; Branch
Guard(a, F64) ; Guard(result, F64)   the accumulator is Tagged across the back edge
FloatArith(Add) ; Widen              and re-boxed for it
```

The call is one line of that. The boxing round trips, the un-folded constant
conversion, and a runtime call receiving a *tagged double* where it wants an
integer index are the other five, and they are questions for the language layer
and the machine layer rather than for `rts-core`. They get their own documents.

---

## What to do with the 0.53 ns

Not nothing — 3% of the busiest band in the engine, for a change with a
contained blast radius — but not first, and not on its own. The honest framing is
that it is a **finishing** change: worth making once the 14.91 ns is gone, when
0.53 is 20% of a row rather than 3%, and worth making then in the smallest form
the measurement supports.

The smallest form, given that the `Vec` is the whole cost:

- Keep `CONTEXTS: RefCell<Vec<Context>>` exactly as it is. It is the source of
  truth, `with_context` still pushes and pops it, and the nesting that
  `node:vm` and `node:repl` need (documented at `current.rs:162`) is untouched.
- Add `CURRENT: Cell<*mut Context>` beside it, written in `with_context` on push
  and restored on pop — two writes per *program*, not per call.
- `with_current` reads the pointer instead of indexing the stack.

The cost, stated per rule 6: a second way to name the same context, which is
precisely the "two answers to one question" that `CLAUDE.md` and
`current.rs:41` both refuse — and `current.rs` refuses it *by name*, about
`THROWN`, for being "a cached flag beside the real slot … kept in step by hand".
The difference that makes it admissible here is that the memo has exactly two
writers, both inside `with_context`, and a debug assertion can compare it against
`last_mut()` on every call. The difference that makes it *cost* something is
that the assertion is not free in release, so in release the two really are
independent and a future third writer of the stack would desynchronise them
silently.

**And there is a second cost, larger than the first, found by the review that
followed this document rather than by it.** `RefCell` does not merely borrow — it
*checks*. A re-entrant borrow is what `authoring-natives.md` warns every native
author about and what eight `rts-core` modules are shaped around (the two-stage
"collect, drop the borrow, call user code, re-borrow" pattern that
`array_proto/iterate.rs` and `string/pattern.rs` both open with). Today a native
that gets it wrong **aborts loudly at the moment it does it**. Behind a raw
pointer the same mistake is two `&mut Context` alive at once, which is undefined
behaviour, silent in release, and detectable only by a debug assertion that
release builds do not run.

So the real trade is not "0.53 ns against a second source of one fact". It is
**0.53 ns against turning a checked invariant into an unchecked one**, in a crate
whose whole discipline around borrows exists because that mistake is easy to
make. That is not worth it now and it is unlikely to become worth it; if the
14.91 ns above ever goes, the thing to reach for first is not this.

It is written down so that whoever proposes it next does not have to re-derive
any of it.

---

## Re-running this

```bash
cd bench/isolated
cargo run --release --bin entry_tax
```

About a second and a half from cold. If the engine's `Context` grows past a
power-of-two size boundary, or `with_current` changes shape, re-run it and update
the numbers here rather than reasoning from these.
