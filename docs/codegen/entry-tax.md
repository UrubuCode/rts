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

---

# Part three: the emitter speculating against its own constant

Found 2026-08-29, chasing why a defaulted parameter cost 24.5 ns where the same
call without one cost 8.

## What it was

`emit_binary_inner` speculates. Given any operator a pair of doubles settles, it
emits a guard on each side, the machine instruction, and the runtime call as the
slow path — a good bet when nothing is known about the operands.

It is not a bet when one operand is a constant the emitter itself just
materialised. `x === undefined` guards `undefined` for being a double. That
guard fails on **every** pass, by construction, because the constant is a
singleton word and will never be anything else. So the shape that ran was: two
guards nobody can pass, an instruction nothing reaches, five blocks, and
`__rts_strict_equals` — a full runtime crossing — to answer whether one machine
word equals another.

`FuncBuilder::is_singleton` had existed since the nullish work and answered
exactly that question in one comparison. Its own doc comment said the old shape
cost every optional chain, every nullish coalesce and every defaulted parameter
two calls — and the defaulted parameter had never been moved over. The comment
described the intent and was read as the state.

## What it is now

`expr::singleton_equality` runs before the speculation and settles strict
equality and inequality against `undefined` and `null` as one
`Inst::IsSingleton`. The IR for a defaulted parameter went from a guard pair
plus a call to two instructions:

    v9: Bool = IsSingleton { value: v3, singleton: SingletonId(0) }
    Branch { cond: v9, ... }

Measured, `--release`, min of 9, three alternations, controls in the same run:

| | base | now |
|---|---:|---:|
| a defaulted parameter | 23.75 | **19.75  (-16.8%)** |
| `x !== null` | 12.00 | **8.75  (-27.1%)** |
| a destructuring default | 40.25 | **36.50  (-9.3%)** |
| CONTROL a plain call | 8.00 | 8.00 |
| CONTROL an addition | 3.75 | 3.75 |

## Why the operand and not the syntax

`undefined` is an identifier in JavaScript and a program may shadow it. Decided
on the emitted VALUE, that case needs no thought at all: a shadowed `undefined`
is a scope read rather than a constant, so `is_constant_singleton` answers
false and the comparison stays a call — correct, and for free. Decided on the
tree, it would have needed the scope, and would have been wrong the first time
someone wrote a local named `undefined`.

It also catches the comparisons no program wrote. `bind_parameters` synthesises
one comparison per defaulted parameter, and `destructure` emitted its own
`StrictEquals` call directly; a syntax test would have had to be told about
both, and was told about neither.

## THIS IS A CLASS, and two more members are already named

The defect is not "defaults were slow". It is **the emitter speculating that an
operand is a double while holding the constant that proves it is not**, and the
same shape was found in two more places within the hour:

- **`x === true` and `x === false`.** A boolean constant is a unique word too,
  by the same argument `Inst::IsSingleton` gives for a singleton — and it takes
  the identical useless guard pair and the identical call. The machine has no
  instruction for it, because `IsSingleton` deliberately takes a singleton
  number, so this one needs a machine capability and not only an emitter change.
- **`typeof x === "string"`.** THREE crossings — `TypeOf` to build a string,
  `StringConst` to build the other one, `StrictEquals` to compare their text —
  plus a throw check, to answer a question about `x`'s tag. The runtime already
  computes a `TypeName` discriminant and then turns it into a string purely so
  that the comparison has something to compare.

**Expect more.** Anywhere the emitter materialises a constant and then hands it
to a path that assumes it knows nothing about its operands, the assumption is
already false at the moment it is made. The check that finds the next one is to
read the IR of a construct rather than its source: a `Guard` expecting `F64`
whose input is a `Const` on the line above is the whole signature, and `rts ir`
prints both.

## The two rows that read worse, and how they were dismissed

`bench/analytic.ts` put `string number->string` at +14.7% and
`string toUpperCase 16` at +8.2%. A dedicated probe (min of 11, three
alternations) put them at +5.7% and +2.1%, so both are real and small.

Neither is attributable, and the check that says so is not an argument: `rts ir`
for the `number->string` loop is **byte-identical** between the two binaries. The
emission did not change, so what moved is where the runtime's own code landed in
the linked image. That is this tree's documented layout floor, measured rather
than asserted for once — and it is the cheapest check of the three this file
records, because it needs no second build.

---

# Part four: `typeof x === "string"` — three crossings for a tag

The second member of part three's class, closed the same day, and the larger of
the two.

## What it was

    v8  = Call TypeOf(v2)          // build one string
    v13 = Call StringConst(0)      // build the other
    ... Guard F64 / Guard F64 ...  // the double speculation, on two strings
    v18 = Call StrictEquals(v8, v13)
    ... WordLoad / Compare / Branch / Throw block ...   // the throw check

Eight blocks and three runtime crossings to answer a question decided by a tag
and a cell header. The runtime already computed the answer as a `TypeName`
discriminant and then turned it into a string **purely so that the comparison
would have something to compare**.

## What it is now

    v9 = Call TypeOfIs(v2, 0)
    Return

One crossing, one block. The throw check is gone as well, and not by
assumption: `TypeOfIs` is on `raising::CANNOT_RAISE`, and the one objection that
kept `TypeOf` off that list — *it allocates its answer string* — is precisely
what this does not do. `raising.rs` records the count change from eleven to
twelve with that reasoning, because the list refuses to grow silently.

Measured, `--release`, min of 9 in process, three alternations:

| | base | now |
|---|---:|---:|
| `typeof x === "number"` | 22.33 | **15.67  (-30%)** |
| `typeof x === "string"` | 25.33 | **19.33  (-24%)** |
| `typeof x === "object"` | 23.67 | **17.33  (-27%)** |
| `typeof x === "function"` (no match) | 19.33 | **9.67  (-50%)** |
| `x === true` (the class member NOT done) | 9.33 | 9.33 |
| CONTROL an addition | 3.67 | 3.67 |

`bench/analytic.ts`'s own `prop typeof` row reads -32.5%, and it is the one row
that moved by the same amount in every run.

## Where each half of the decision lives

`type_name_of` is new and is the whole point of the split: `type_of` turns its
answer into a string, `type_of_is` compares it against a literal. A second copy
of that match is how `typeof x` and `typeof x === "…"` would come to disagree
about a value nobody tested.

On the emitter's side the recognition is on the TREE and not on the values —
the opposite of `singleton_equality` and for the opposite reason. What has to be
seen is that the left operand is a `typeof` APPLICATION, and a `ValueId`
holding `"string"` cannot say whether it came from `typeof` or from a string the
program computed.

The literal reaches the runtime as its INDEX, from the same table
`string_const` reads. A number naming one of the nine `typeof` answers would
have been a second numbering of them — rule 3 of `crates/rts-core/README.md`,
and the drift `TypeName` already exists to prevent one level down.

## What analytic.ts could NOT say, and why it is written here anyway

Two runs of the same two binaries, on the same machine, an hour apart:

| | geomean | rows >8% better | rows >8% worse |
|---|---:|---:|---:|
| first (taken with a build running) | **+7.08%** | 2 | 25 |
| second (machine otherwise idle) | **-6.29%** | 23 | 1 |

They disagree in SIGN on more than twenty rows. `arith negate` read +124% in the
first and did not move in the second; `array push+pop` read -44% in the second
and did not move in the first. So no geomean from that table is claimable for
this change in either direction, and the honest report is that the instrument
could not answer.

**Nothing was netted away by choosing the friendlier run.** What survives both
is the one row the change targets — `prop typeof`, -32.5% in both — plus the
dedicated probe above, plus the real programs: `monte_carlo_pi` 377 to 375 ms,
`objbench` 302 to 295, and an empty program 60 to 59, all min of five, which is
what says the change is not paying for itself somewhere else.

The lesson for the next measurement is the cheaper half: **the first table was
taken while `cargo` was running**, and a benchmark sharing the machine with a
linker is not a benchmark. Min-of-three was not enough to hide it either —
`objbench` read +2.3% at min-of-three and -2.3% at min-of-five.

## A member of the class that was REFUTED before it was built

`__rts_string_const` is the second-most-emitted crossing in `analytic.ts` — 230
call sites, behind only `get_property`'s 362 — and it reads a table entry that
never changes after startup. The obvious next move was to read it as a
`WordLoad` from the table's base, which is exactly what `Inst::WordLoad`'s own
doc comment describes itself as existing for.

Measured first, and it does not pay:

| | ns |
|---|---:|
| a string literal, then `.length` | 9.00 |
| the same string HELD in a `const`, then `.length` | 8.67 |
| difference | **0.33** |

The 230 are sites EMITTED, which is a compile-time cost, not a run-time one —
the call is hoisted out of the loop by the time it matters. So the ranked item
is struck, and with it a design that would have cached a base pointer into a
`Vec` that `eval` and `new Function` can still grow: a stale pointer read as a
value, which is the silent-wrong-answer class `docs/engine/lost-roots.md`
catalogues.

**A census counts sites; only a clock counts nanoseconds.** That is the third
time in this file a ranked item died to a measurement that cost ten minutes.

---

# Part five: the rule that was applied in the wrong ORDER

The third member of part three's class, and the one that turned out not to be a
performance defect at all.

## What was measured

`x == null` — one of the most written idioms in JavaScript — emitted the worst
shape in this file: the double speculation's two guards, of which the one on the
constant `null` fails on every pass by construction; a full crossing to
`__rts_loose_equals`; and the throw check that crossing implies, because `==` in
general runs `ToPrimitive` and `ToPrimitive` runs user code.

The emitter fix is the same one part three describes — `x == null` is true
exactly when `x` is nullish, so it is `choice::branch_on_nullish` in a value's
clothing. What was not expected is the size of it:

| | base | now |
|---|---:|---:|
| `x == null`, `x` an OBJECT | **1 456.67** | **8.00  (182x)** |
| `x == null`, `x` undefined | 10.67 | 8.67 |
| `x != null` | 1 470.33 | 8.00 |
| `x == undefined` | 1 502.00 | 8.00 |
| the guard idiom plus a property read | 1 443.33 | 9.67 |
| CONTROL `x === null` (part three's) | 8.00 | 8.67 |
| CONTROL an addition | 3.33 | 3.67 |

A crossing costs about 14 ns. 1 456 is not a crossing, and the gap between the
object row and the undefined row — 1 456 against 10.67, same operator, same
constant — is what said the operand was being CONVERTED.

## What it actually was

    let (left_object, right_object) = with_current(…);      // ask
    let (left, right) = match (left_object, right_object) {
        (true, false) => (to_primitive(left, hint), right), // CONVERT
        …
    };
    with_current(|context| {
        …
        if absent(left) || absent(right) { … }              // the null rule
    })

The `null`/`undefined` rule was applied **after** the conversion. The
specification puts it at steps 2 to 4 of `IsLooselyEqual` and `ToPrimitive` at
step 10, and the order is not decoration: `({ valueOf() { … } }) == null` must
not run the `valueOf` at all.

**And that is observable, not merely wasted.** Counted against node:

    obj == null   ->  false, valueOf/toString called 2 times   (node: 0)
    obj == 0      ->  true,  called 3 times cumulative         (node: 1)

So a program whose conversion counts, logs, or fetches behaved differently here
than in every other engine, and the ANSWER was right the whole time. Nothing in
the corpus caught it, because every test asserted the answer.

## Both halves shipped, and why one was not enough

The emitter change removes the crossing wherever a literal `null` or `undefined`
is at the site — which is most real code, and more of it than expected, because
it **composes with the inliner**: a helper like `cmp(a, b) { return a == b }`
called as `cmp(x, null)` has its body substituted, and the literal then arrives
in operand position at a site that had none.

That is a mask, not a fix. A callee the inliner refuses still reaches
`__rts_loose_equals`, and the probe that proves it is a helper declared twice in
one program, so `declarations_of` is 2 and the pass refuses both:

    via a refused callee:  false, calls: 1     (node: 0)

The runtime arm is therefore reordered as well, in the same change. The rule is
now asked in the borrow that already existed for `is_object_in`, before the
conversion — and it is still asked a second time afterwards, which is NOT dead
code: `ToPrimitive` can answer `undefined`, from a `valueOf` that returns
nothing, and the specification re-enters the comparison with the converted value
rather than continuing down the table.

## THE CLASS THIS ADDS, and it is not the one part three named

Part three's class is *the emitter speculating against its own constant*. This is
a different one and a worse one:

> **A rule applied in the wrong order is invisible to every test that asserts an
> answer.**

The answer was correct. The cost was 180x. The only thing that could see the
defect was a counter on a side effect, and the only reason anyone looked was
that a benchmark row read 1 456 ns where the model said 14.

**Where to expect more.** Anywhere this runtime converts before it dispatches —
`ToPrimitive`, `ToNumber`, `ToString`, `ToPropertyKey` — the specification
almost always has cheap arms ahead of the conversion, and putting the conversion
first is both the natural way to write the function and undetectable by an
assertion on the result. The check is not a test; it is a COUNTER on the
conversion, and `tests/loose_null.test.ts` is the shape: give the operand a
`valueOf` that increments, then assert the count as well as the answer.

A test that asserts only the answer proves the answer. It says nothing about
what was run to get there.

### The two nearest neighbours were checked and are CLEAN

Written down so the next reader does not re-read them:

- **`primitive::to_primitive`** asks the cheap questions first and is correct.
  A non-reference returns immediately without borrowing the context at all, and
  a reference that is not an object returns before `Symbol.toPrimitive` is
  looked up. Its own comment says why.
- **`functions::instance_of`** cannot skip its `Symbol.hasInstance` probe — the
  specification puts it at step 2 of the operator, ahead of everything,
  including the "`V` is not an object, return false" arm that looks skippable.
  What it CAN do is answer the probe without a crossing, and it already does, as
  of the same day's work.

One instance found, one instance fixed. The class is stated because the next one
will be somewhere nobody has read, not because a sweep found several.

---

# Part six: two helpers, one converting, and the caller that picked the other

Part five named the class as *a rule applied in the wrong order*. Running its own
check — trace what an operation RUNS on its operands, not what it answers —
across the whole language found the class again, and this time the shape is
sharper and the instances are many.

## The check, and what it is

One object with a counting `valueOf` and `toString`, one line per operation,
compared against node:

    o & 1     RTS: (nothing)  -> 0      NODE: a.valueOf -> 1
    o | 0     RTS: (nothing)  -> 0      NODE: a.valueOf -> 3
    o << 1    RTS: (nothing)  -> 0      NODE: a.valueOf -> 6
    o ** 2    RTS: (nothing)  -> NaN    NODE: a.valueOf -> 9

Forty-five rows, forty-two identical to node. This is the INVERSE of part five's
defect: there the conversion ran and should not have; here it does not run and
should. And unlike part five, the answer is wrong, so an ordinary assertion could
have caught it — no test in the corpus had one.

**`[7] & 15` answered 0 where the language says 7, and `[7] ** 2` answered NaN
where it says 49.** Nothing exotic is required: a one-element array inherits
`valueOf` from `Object.prototype`, which answers the array, so `toString`
produces `"7"` and `ToNumber` produces 7.

## The mechanism, which is the part worth remembering

**There are two functions called `operands`.**

- `primitive::operands` converts. It runs `ToPrimitive` and therefore user code,
  so it must be called OUTSIDE a context borrow.
- `operators::operands` reads. It is `as_number(…).unwrap_or(NAN)`, pure, and
  must be called INSIDE one.

The arithmetic operators call both, in that order, and are correct. Every
operator in `entry/bitwise.rs` called only the second — and `bitwise.rs`'s own
module header states the rule correctly, `ToInt32(ToNumber(a))`, above code that
does not implement it. The doc comment on `number_exponent` went further and
described `operands` as running `ToPrimitive` and possibly a user `valueOf`,
which was a true sentence about the other function of that name.

## And it is not one pair

The same shape, audited with the same probe over every method that takes a
numeric argument, found **fourteen more** — and a THIRD spelling of the
non-converting read:

| where | the non-converting read |
|---|---|
| `entry/bitwise.rs`, 8 operators | `operators::operands` |
| `entry/string/*`, 23 call sites | `string::integer_arg` |
| `entry/array_proto/*` | `Value(x).numeric().unwrap_or(0.0)`, inline |

`string/mod.rs` is the honest one: `integer_arg`'s doc says *"An object answers
`NaN` and therefore zero, because `ToNumber` on one runs user code and this is
inside a borrow. **The stated gap**"*, and `integer_outside` sits directly below
it as *"the same conversion, performed OUTSIDE any borrow so an object
converts."* The pair is documented, the gap is documented, and the callers still
picked the wrong one.

Measured against node, an object argument that converts to a number:

    substr(n1,n2)            ""            node "bc"
    codePointAt(n2)          97            node 99
    startsWith('b',n1)       false         node true
    endsWith('b',n2)         false         node true
    padStart(n5)             "ab"          node "   ab"
    padEnd(n5)               "ab"          node "ab   "
    arr.lastIndexOf(3,n4)    -1            node 2
    arr.fill(0,n2)           [0,0,0,0]     node [1,2,0,0]
    arr.fill(0,n1,n3)        [1,2,3,4]     node [1,0,0,4]
    arr.copyWithin(n0,n2)    [1,2,3,4]     node [3,4,3,4]
    arr.splice(n1,n2)        []            node [2,3]
    arr.with(n1,9)           undefined     node [1,9,3,4,5,6]
    arr.length = n2          [1,2,3]       node [1,2]
    [3, n2, 1].sort()        [{},1,3]      node [1,{},3]

`with` is implemented and correct for a plain number; every row here is the
argument, not the method.

## Why this keeps happening, and the only thing that stops it

A borrow of the context cannot call user code — that is a hard constraint of
this runtime, and it is correct. So every conversion has to be lifted above the
borrow, which means every argument-reading site has TWO shapes available and
only one of them is right. The wrong one is shorter, reads naturally, compiles,
and answers a plausible number.

Naming the pair did not prevent it. Documenting the gap did not prevent it. What
FINDS it is the counter, and what would END it is one converting helper that the
non-converting read is not reachable around — a single `ToIntegerOrInfinity`
taken above the borrow, rather than three spellings of a numeric read that each
caller must remember to lift.

Until that exists, the check is `scripts/`-able and cheap: give the argument a
counting `valueOf`, run the operation, and compare the trace to node. A test
that asserts the answer for `fill(0, 2)` passes on every build in this table.

---

# Part seven: what an operator actually costs, and one allocation removed

Asked directly: why are the operators heavy, when they should cost nothing?

**They do cost nothing.** What costs is the door.

| | ns | over the floor |
|---|---:|---:|
| an empty loop | 4.33 | — |
| `i & 15`, compiled to an INSTRUCTION | 3.67 | **0.00** |
| `obj.x`, a cached property read | 10.33 | 6.0 |
| `typeof x === "object"` — exactly ONE crossing | 20.00 | **15.7** |
| `null - 1` — one crossing plus a body | 21.33 | 17.0 |
| `null >>> 1` — one crossing plus a body | 22.67 | 18.3 |

The Rust body of a bitwise or arithmetic operator costs about **1.5 ns**. The
crossing costs about **15.7**. Ninety per cent of what an operator "costs" is
leaving compiled code, and when the machine can prove its operands it does not
leave at all — which is what the zero on the second row means.

That figure also corrects one from earlier in this file. `Math.abs(1)` was used
as a stand-in for "one crossing" at 5.7 ns over the floor; it is not one.
`Math.floor(1.5)` measures 5.3 and `bench/analytic.ts` reads `arith Math.floor`
at 3.0 — both are compiled, not called. The honest single-crossing control is
`typeof x === "…"`, because part four made it exactly one, and it agrees with
the 13.8 ns per crossing part two measured independently.

## The one thing that was removed

`coerce::string_to_number` began with `text.to_rust()`, unconditionally. For the
Latin-1 form that is `String::from_utf8(bytes.to_vec())` — a `Vec`, a copy and a
revalidating scan — so `"7" - 1` allocated a one-byte `String` in order to parse
one digit.

ASCII bytes ARE UTF-8 bytes, so the common case needs no copy. Measured,
`--release`, min of 11, three alternations, two controls in the same run:

| | base | now |
|---|---:|---:|
| `"7" - 1` | 81.50 | **42.00  (-48%)** |
| `"7" & 15` | 103.50 | **56.00  (-46%)** |
| `+"7"` | 94.00 | **46.50  (-47%)** |
| `"1234.5" - 1` | 92.50 | **55.00  (-41%)** |
| `"7" < 8` | 144.50 | **96.50  (-33%)** |
| `Number("7")` | 131.00 | **90.00  (-31%)** |
| `" 7" - 1` — the slow path, kept | 100.50 | 100.00 |
| CONTROL `null - 1` | 22.00 | 21.50 |
| CONTROL `obj.x` | 10.00 | 10.00 |

Thirty-eight nanoseconds off `"7" - 1`. That is the allocation.

**There is still exactly one parser.** The bytes are BORROWED into the same
`&str` the built `String` would have produced, so nothing below the first four
lines changed. A second parser for narrow text would be a second statement of
what a number literal is, and this file exists because of spellings — `inf`,
`NaN`, `1_0`, `0x` with a sign — that two statements would disagree about.

The boundary is `is_ascii` and not `str::from_utf8`, and the case that makes the
distinction necessary rather than cosmetic is pinned in a test: `U+00A0` is a
no-break space, it is string whitespace, and it FITS in Latin-1 — so `narrow()`
answers bytes for it that are not UTF-8. Letting the UTF-8 validator decide
would reject that one, and would accept a Latin-1 pair like `0xC3 0xA9` as a
DIFFERENT character.

## Three hypotheses that did not survive

Written down because each looked obviously right and cost a build to refute.

- **"Each `with_current` costs about 7 ns."** Derived from `null - 1` at 20.33
  against `null & 15` at 27.33, the two differing only in that `&` takes a
  second borrow for the bigint ask. `&`, `|` and `^` can never answer
  `Err(Refused)` — only `<<`, `>>` and `**` can — so the second borrow was
  merged away for those three. Measured: `&` unchanged, `|` and `^` **1.5 ns
  worse**, and the untouched `>>>` control drifted as far as either. Reverted.
  Whatever separates `-` from `&` is not the borrow.
- **"The thread-local lacks a `const` initializer."** It has one already.
- **"`bigint_class::binary` probes the digit side table twice per operation."**
  It does not. `digits_of` is `as_client(kinds.bigint)?` — a tag comparison that
  returns immediately for anything else, and the table is never touched.

The first is the fifth rearrangement refuted in this campaign against zero that
shipped. **A change that removes work is worth building; a change that moves
work is worth measuring before believing.**

## What the clock could NOT say this time

The whole-program and `analytic.ts` numbers for this change were taken on a
loaded machine — an empty program measured 79-88 ms where it measures 58, and
`objbench` bounced between 409 and 451 between two alternations of the same
binary. They support no claim in either direction and none is made.

The probe above is not affected by that, and this is why it is built the way it
is: **the two controls are in the same run as the targets.** Load that moves a
target moves a control with it. `obj.x` reading 10.00 on both binaries is what
licenses reading 81.50 against 42.00 as a real difference.
