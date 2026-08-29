# What a runtime crossing costs, and why the context is not it

**A crossing is ~5 ns, and the context behind every entry point is about 1.0 of
them.** Part one asked whether `with_current` was the 16-30 ns band and
answered NO on 2026-08-21, before any engine code was written, by
`bench/isolated/src/bin/entry_tax.rs`. It said 0.53; part two re-measures it
inside the call shape the engine actually emits and gets ~1.0, prices the
rest of the door, and refutes three more candidates — including one that was
implemented in full and reverted.

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

## So where are the 16-30 ns? — asked here, ANSWERED in part two

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

---

# Part two — the crossing, priced whole

*2026-08-29. This document asked one question and answered it correctly. What it
could not say, because nobody had measured it, is what the 16–30 ns IS. That is
below, along with a correction to its own number and a second refutation of the
same hypothesis from the other end.*

---

## The correction: 0.53 becomes ~1.0, and it does not change the verdict

`entry_tax.rs` measured a **direct** `call rel32`. The engine emits an
**indirect** one — entry points are `Linkage::Import`, so `colocated` is false
and cranelift materialises the address and calls a register.
`bench/isolated/src/bin/crossing_price.rs` re-runs this document's own
shape-1-minus-shape-4 subtraction inside that shape, and `with_current` comes
out at **1.03 ns** on one run and 0.93 on another — call it **~1.0**, not 0.53.
The two runs are the same bin on the same machine minutes apart, and quoting
either alone would be a precision this experiment does not have.

The verdict above stands unchanged: 1.0 is still not 16-30. What changes is the
share — 1.0 is a THIRD of the 3.10 ns floor measured below, which makes it the
largest single non-body part, and this document had it at half that.

## The floor: one crossing is ~5 ns, and it is purely per-crossing

Measured in the ENGINE, `target/release/rts.exe`, a ladder whose crossing count
was counted in `rts ir` rather than assumed:

| crossings in the loop body | ns/op |
|---:|---:|
| 0 (`a = a + 1.5`, proven double) | 0.80 |
| 1 (`arr[i & 1023]`) | 14.60 |
| 2 | 28.40 |
| 4 | 56.00 |

**13.8 ns per crossing, three ways, intercept zero.** There is no shared fixed
cost to amortise, which is what makes dividing a row by its counted crossings
legitimate.

But that is `get_indexed`, a crossing with a real body. The DOOR alone, measured
with the cheapest entry point the language can reach — `if (o)`, which lowers to
one `__rts_to_boolean` and nothing else, no throw check, no result guard:

| | ns net of the 0.80 floor |
|---|---:|
| a machine operation | 0.0 |
| **a cached own property read** (no crossing at all) | **3.2** |
| **the door alone** | **~5** |
| door + `get_indexed`'s body | 14.0 |
| door + a typed array's body | 20.4 |
| door + `Set.has` | 30.9 |
| door + `Map.get` | 35.2 |

The parts, from `crossing_price.rs` — each row is the one above it plus one
thing, so every part is a subtraction and not an estimate:

| | ns | the part it adds |
|---|---:|---:|
| the loop, no call | 0.236 | |
| a DIRECT call, empty body | 1.154 | 0.92 |
| an INDIRECT call, empty body | 1.154 | **0.00** |
| + the throw check | 1.159 | **0.005** |
| + a `&mut Context` passed in | 1.171 | 0.01 |
| + `with_current` instead | 2.201 | **1.03** |
| + a real (trivial) body and its check | 3.338 | 1.14 |

**A crossing with a trivial body is 3.10 ns**, which is the same floor
`NumberRemainder` reaches in the engine. The engine's ~5 is that plus the box
and the guard on the result.

Two of those rows are refutations on their own. **An indirect call costs exactly
what a direct one costs** — 1.154 both — which is the premise
`crossing_price.rs` was written to test, and it came back no. And **the throw
check is 0.005 ns**, which is the third instrument to say so.

`with_current` at 1.03 is the largest single part, and roughly double what part
one recorded, for the reason at the top of this section.

**Register pressure is NOT settled, and this document will not claim it.** Six
values live across the call add 1.09 ns (row 6 of `crossing_price`), but
`verify_regs.rs` — which subtracts the cost of holding the same six across NO
call — puts the call's own share at **-0.24**, negative. Two instruments
disagree in sign, so the honest range is 0 to 0.6 and the number is not usable.
It is written down because the first draft of this section quoted 1.04 as fact
from one instrument alone, which is exactly the error `README.md` rule 2 is
about.

## Three things this priced at zero, so nobody prices them again

**The throw check.** 0.00 ns by isolated subtraction and at most 0.11 by the
tightest in-engine bound. Independently: `__rts_to_boolean` is on
`raising.rs`'s no-check list and emits no `WordLoad`/`Compare`/`Branch`, while
`__rts_type_of` emits all three — and typeof's crossing is the CHEAPER of the
two. The −1.0% that `native-call-floor.md` §5 collected for the
ToBoolean/StrictEquals exemption was the 44 removed BLOCKS, not the removed
loads. Extending `raising.rs` buys code size, not time.

**The convention.** `native-call-floor.md` §7a's framing — six `u64` and nothing
else expressible, so every operand makes a widen/guard round trip — is true and
priced at **zero for a JS call**: a call with four arguments costs the same as
one with none. The box is worth 1.0–2.7 ns on a two-operand CROSSING, and
`NumberRemainder` proves the floor it implies is already reachable: it is
`(F64,F64)->F64`, cannot raise, its loop body in `rts ir` is literally one
`Call`, and it costs **3.0 ns end to end**.

**Narrowing the signature table.** Audited: 62 of ~90 rows carry an `UNPROVEN`,
and all but eleven genuinely accept or produce an arbitrary JavaScript value.
The eleven are nine returns and two params, every one a return-side `Ref` — an
ABI change to a `#[rtse::entry]` body for ~1 ns at the consuming site. Nothing
in the 11–40 ns band reaches single digits this way. And there is no emitter
blocker to remove: `emit/expr.rs` has honoured per-parameter reprs since the
property-key fix, and honours narrow returns too.

## REFUTED, and implemented in full first: fusing the two borrows in `get_indexed`

`get_indexed` and `set_indexed` each took **two** `with_current` on the hot
path — `opened` takes one to run `resolved`, and the body immediately takes
another. Since `resolved` already takes `&mut Context`, the two can be fused
without duplicating the shared decision that `opened` exists to keep single.

Predicted at ~0.9 ns, which would have been 7% of `a[i]`. It was written,
built, and measured. **It bought nothing:**

| | base | fused | |
|---|---:|---:|---:|
| `array index read` | 15.00 | 15.00 | 0.0% |
| `array index write` | 14.80 | 15.40 | +4.1% |
| `Uint8Array read` | 21.80 | 21.60 | −0.9% |
| `Uint8Array write` | 28.40 | 29.00 | +2.1% |
| `Float64Array rw` | 52.00 | 52.67 | +1.3% |
| `DataView getU32` | 32.00 | 31.33 | −2.1% |
| `string index []` | 25.67 | 25.67 | 0.0% |
| *control* `o.a` cached write | 4.20 | 4.00 | **−4.8%** |

Mixed signs, and **the control moved as much as the targets**. Reverted.

The mechanism, stated so it is not re-proposed: `with_current` costs ~1.0 ns
the FIRST time, when the thread-local slot and the `RefCell` flag are cold and
the branch is unpredicted. The second one in the same operation touches lines
already in L1 and a branch already predicted, so its marginal cost is far below
the first's — and a decomposition that prices a repeat at the same rate as a
first is a decomposition that will be wrong in exactly this direction.

That is the third independent negative on rearranging this crate's hot paths,
after `native-call-floor.md` §3a-i and §5b, and it is why
`crates/rts-core/README.md` and `docs/codegen/what-a-property-costs-2026-08-29.md`
both say to prefer a change that REMOVES work.

## So where the 16–30 ns actually is, now that it is measured

Two answers, and they are for two different halves of the table.

**A row that contains a JS CALL** — `call method 21.05`, `call closure var
read 18.16`, and every row whose body calls one — is the door, and the door is
worth **19.9 of its 21 ns**. The measurement that says so is the engine's own
existing proof: `emit/inline.rs` identifies a callee whole-program, and an
inlined `inl(x){return x+1}` costs **1.5 ns** at a site where the same call
uninlined costs 20.5. A direct `Inst::Call` to a statically-proven callee is the
only item in this document that turns a 20 ns row into a single-digit one.

Its blocker is `Context::callees`, and it is **four consumers rather than one
line**: `throw::stack_text` walks it for `.stack`, `roots.rs` feeds it to the
collector, `function_proto` reads `last()` for a bound function's identity, and
`functions.rs` reads `len()` as the activation depth `new.target` matches. Only
the first is user-visible, and `docs/engine/the-unwired-keystone.md` §2 already
names its replacement. The other three are answerable statically for exactly the
class `inline.rs` already proves.

**A row that is a crossing plus a body** — `array index read 14.48`,
`Map.get 43.81` — is mostly BODY. ~5 is the door and the rest is work, and no
amount of making the door cheaper reaches single digits: even a FREE door leaves
`Map.get` at 30. Those rows need the crossing removed, not shortened, and the
machine already holds the instructions that would do it — `Inst::ElementLoad`,
`CallIndirect`, `IntArith` and `Alloc` are declared and lowered with **zero
producers** — behind the one precondition `the-unwired-keystone.md` names.

One correction to that precondition, and it is new: **a typed array's elements
are bytes, never references**, so the collector objection that blocks
`ElementLoad` for ordinary arrays does not apply to `Uint8Array read 20.61`,
`Uint8Array write 27.97`, `Float64Array rw 25.49` and `DataView getU32 30.52`.
What blocks those four is the storage representation instead — the bytes live in
a Rust `Vec<u8>` reached through a side table — which is a different and smaller
problem than precise roots.

## Re-running part two

```bash
cd bench/isolated
cargo run --release --bin crossing_price   # the parts of the door
cargo run --release --bin verify_regs      # register pressure, on its own
```

The in-engine ladder is a `.ts` file and is not checked in; its shape is in the
table above and it is three lines of loop per row.
