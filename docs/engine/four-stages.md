# Four stages, not two

The pipeline is `AST → machine IR`. It needs to be
`AST → resolved AST → MIR → machine IR`, and the two missing stages are not
refinements: every optimisation this engine has is written as a proof over the
*spelling* of the whole program because there is no stage where a dataflow fact
could be held instead.

This document is the architecture the missing two imply, why the order between
them is forced rather than preferred, and what it does not buy. It does not
restate what `architecture.md` says about the machine/language boundary — that
boundary is correct and nothing here moves it.

---

## What is already right, and is not touched

Named first because it decides what can be built on top:

- **`repr/mod.rs`** — a representation is carried as *data*, with a total merge
  rule. The reasoning in its header holds.
- **`types/registry`** — field offsets in one place, with `ref_offsets`
  precomputed at declaration. That is half of a precise trace already done.
- **`shape/mod.rs`** — a shape *is* an aggregate arrived at incrementally, and
  reading a property introduces no second way to reach a field.
- **`ir/`** — SSA with block parameters.
- **`gc/mod.rs`** — `describe_frames` derives the root set from liveness, and
  there is deliberately no entry point through which a client could report a set
  of its own.
- **`frame/`** — spilling live state into a record with a resume position,
  computed from liveness. See `deopt-lateral.md`: this is most of a
  deoptimiser, built for generators.
- **`rts prove`** — the only reason any of this can be discussed in numbers.

---

## The finding: a name is a spelling

`names/mod.rs:23` is `pub struct Name(u32)`, and the `u32` identifies an
**interned spelling**. No stage turns a spelling into a binding:

- `check/scope.rs` computes the lexical and var declaration sets, but for the
  early-error rules only. It answers *is this a redeclaration*, never *which
  binding is this*.
- `emit/scope.rs` is a scope chain maintained **during emission**, keyed by
  `Name`, i.e. by spelling.

Everything downstream reconstructs binding identity by counting spellings over
the whole program: `inline::declarations_of`, `Inlinable::free_proved`,
`primordial::untouched`, `omit`, `receiver`, `escape` (keyed by name, which is
why `Ctx::flattens` has to be consulted at every substitution site), `proven`,
`settled`.

Each is an attempt to recover, by counting, information a resolution pass would
have handed over for free. They are honest and they are systematically weaker
than they need to be **in both directions at once**.

### Weaker in the refusing direction

`declarations_of` over-counts on purpose: a parameter, a `catch` binding and a
loop target all count. So a helper that reads its own loop variable is refused
in every program that has two loops, because both spell it `i`. Measured
2026-08-30, release, min of 9:

```text
for (let i   = …) { const q = (x) => x + i;   a = q(a) | 0; }   233.67 ns
for (let zwq = …) { const q = (x) => x + zwq; a = q(a) | 0; }    46.33 ns
```

Five times, and the only difference is spelling.

### Weaker in the accepting direction

Measured 2026-09-20 against `target/release/rts.exe`:

```js
function main() {
  let i = 1;
  const q = (x) => x + i;
  let s = "" + q(10);
  { let i = 100; s += "," + q(10); s += "," + i; }
  return s;
}
console.log(main());
```

```text
rts:  11,110,100
node: 11,11,100
```

A silent wrong answer with exit zero — the worst class this repository names.
Two ablations isolate it: spelling the inner binding `j` answers correctly, and
adding `const hold = q` (which makes the helper read as a value, so
`omit::omittable` no longer applies) also answers correctly.

The cause is `emit/inline.rs`'s locality argument. `i` is declared twice in the
program, so `free_proved` is false, and the site accepts on `ctx.omits` instead.
That proof establishes that the caller *is* the declarer — no other **function**
is between the two — and is presented as stronger than the count. It is not: a
`{ let i }` **block** inside the declarer reintroduces the binding, and the
substituted body resolves against it. The count refused this case; locality
admits it.

Seven documented failures in that one file are the same failure: substitution by
spelling, with no renaming pass. The header says so outright — *"each of those
needs a renaming pass this does not have"*.

### And it is not only the inliner

`tests/closure-capture-loop-shadow.test.ts` is pinned and **fails today**. A
closure over an outer `i` reads a `for (let i …)` of the enclosing function
instead:

```text
rts:  0:0 1:1 2:2
node: 7:0 7:1 7:2
```

The ablation says it is not the inliner — with the helper read as a value, so
nothing may be substituted, the answer is unchanged. The mechanism is the same
root in a second place: a captured local is a **property of an environment
object**, `Binding::InEnvironment { hops, name }` keys that slot by the name, and
`emit/binding.rs`'s declaration path stores into it whenever
`scope.is_captured(name)` holds. The captured set is keyed by spelling as well,
so the loop's own binding is taken for the captured one and writes the outer
binding's slot.

That one is not fixable by a narrower test at the site, because whether a
block-scoped declaration needs environment storage of its own depends on whether
an inner closure captures *it* — `catch (c) { const read = () => c; }` is the
shape that does. Deciding it needs binding identity. With slots keyed by
identity, the two `i`s are two slots and the question does not arise.

`emit/scope.rs::for_function` already carries a filter added 2026-08-21 for the
mirror image of this bug, where a nested block's captured name shadowed the
correct outer binding at zero hops. Two bugs, opposite directions, one cause.

---

## The defect class, stated once

**Decisions that require a fixed point are being taken in a single
syntax-directed pass, while emitting.** A fixed point cannot be iterated to
during emission, so every such decision is instead approximated by a
whole-program syntactic proof.

That is one statement covering both the wrong answer above and the ceiling on
speed. It is also why `Repr::I8`, `I16` and `F32` exist in the machine's lattice
with zero producers, and why the instruction set has no integer-width
conversion: those need a type domain that survives past one traversal, and there
is nowhere to put one.

---

## The four stages

**E1 — parse → AST.** Exists.

**E2 — resolve.** `Name` (spelling) → `BindingId` (binding identity). Every
`let`, `const`, `var`, parameter, `catch` binding, function-expression name and
loop target gets an identity; every identifier points at one. This is what Go's
type-check stage does before its inliner runs, and it is the cheapest of the
three missing pieces.

What it unlocks immediately: inlining with renaming by construction (the
seven-failure family stops being expressible); escape analysis keyed by binding
rather than by name; deleting `declarations_of` / `free_proved` / `omits` and the
over-counting that costs the 5× above; `var`-versus-`let` and capture answered
once instead of at each site.

**E3 — MIR.** Per function: SSA, explicit CFG, and four things the machine IR
cannot carry because they are language facts.

1. **A type domain per value**, with inference by abstract interpretation **to a
   fixed point** — which is what a loop requires and a syntactic pass cannot
   give.
2. **Guards as values in the dataflow**, not as ad-hoc emission. Only then can
   CSE, hoisting and LICM reach them. Ten guards becoming one is the result of
   passes, not of better emission.
3. **An effect summary per instruction** — reads/writes the heap, may allocate,
   may call user code, may throw. It is the precondition for moving anything
   soundly. Today "may collect" exists on the machine side only.
4. **Explicit safepoints**, so the precise root set is computed from MIR
   liveness and meets what `gc::describe_frames` already does.

Passes, in the order they unlock each other: inference → guard dedup and
hoisting → devirtualisation → inlining → escape analysis → scalar replacement →
unboxing → GVN/LICM.

**E4 — lower → `rts_cranelift::ir` → machine.** Exists and is good. The 50-odd
files of `emit/` shrink to two translators with no decisions in them:
resolved-AST→MIR and MIR→machine IR.

---

## The MIR is its own crate, because there are two languages

`a-second-language.md` records that the boundary is only real if it has a client
on each side. A MIR inside `rts-codegen` would force a second front end to
depend on the JavaScript crate or to reimplement the CFG — which is exactly the
argument that put `ShapeTree` in the machine rather than in the language.

But only half of a MIR is neutral, and the halves must be cut apart before the
crate is drawn.

**Neutral by construction.** CFG, SSA, block parameters, dominators, liveness,
effect summaries, safepoints, `DeoptPoint`/`PointId`, the two tiers, and every
pass that needs nothing but effects and dominance: GVN, DCE, guard CSE and
hoisting, LICM, inlining (by `BindingId`), scalar replacement.

**Not neutral**, and `a-second-language.md` already measured these:

| | JavaScript | Lua |
|---|---|---|
| numeric tower | one `number`; `int32` is an optimisation | integer and float are **distinct types** |
| falsy | seven cases | two: `nil`, `false` |
| array index | `0` to `2³²−2` | 1-based |

A shared lattice would be the union of the two — each language paying for the
other's cases — or it would be JavaScript's under a neutral name.
`Guard(IsTruthy)` does not mean the same thing on both sides.

**So the type domain is declared, not selected.** That is the mechanism this
repository already uses: `Value` does not know that singleton 0 is `undefined`,
and `Symbol` and `BigInt` are kinds *declared by the language* on tags the
machine leaves to a client. `rts-mir` owns the structure and takes the domain as
a declared vocabulary; inference is abstract interpretation, which is
domain-parametric in its ordinary form rather than by a trick.

Semantic operations are not MIR nodes either. A `ToBoolean` is a call to an
entry point the language **names** — the same "reuse is by naming, not by
selection" that `rts-host/src/entries.rs` enforces by comparing ABI *shape*,
because two languages have the same shape and different answers:
`tostring(1.0)` is `"1.0"` where `String(1.0)` is `"1"`.

### The crates

```
rts-cranelift        the machine: low IR, repr, layout, frame, gc, deopt
      ▲
rts-mir              NEW — the shared IR
  cfg/               blocks, SSA, dominators, liveness
  effect/            heap read/write, allocates, calls user code, throws
  guard/             guard(cond) else deopt(PointId); the two tiers; pair.rs
  domain/            the trait: types, join/meet, what a guard may assert
  profile/           a profile's format and its consumption (PGO)
  passes/            generic over the domain
  lower/             MIR → rts_cranelift::ir; language operations by NAME
      ▲                        ▲
rts-codegen (JS)           a second front end, later
  syntax/ parse/ check/      syntax/
  names/    BindingId        names/
  domain.rs int32|double|    domain.rs integer|float|…
            string|shape|…
  lower.rs  AST → MIR        lower.rs
```

Direction: `rts-codegen → rts-mir → rts-cranelift`. Nothing points back, and
`rts-mir` never names JavaScript. A deoptimiser that makes `rts-cranelift`
depend on `rts-codegen` has put rematerialisation in the wrong layer.

Named `rts-mir` and not `rts-ir` because `rts_cranelift::ir` already exists and
is the low one.

### The anti-drift mechanism, because one client is no boundary

Building `rts-mir` with only JavaScript above it is the reliable way to bake
JavaScript into it by accident. The mitigation is the one already applied to
`Symbol` and `BigInt`: **instantiate a toy second domain in the tests** — three
types, a different falsy rule, integer distinct from float — and run the generic
passes over it. It is not a second language; it is the proof that the
parameterisation is real rather than decorative.

---

## The order is forced

1. **E2, resolve.** Nothing below is sound without binding identity, and it is
   the only step that closes a defect open today.
2. **Handles in `rts-core`.** The Rust half of precise roots — the half no
   compiler pass can supply. Independent of the rest, so it can run in parallel.
3. **`rts-mir`'s skeleton**: CFG, SSA, effects, safepoints. No passes yet; an
   `rts mir` that prints, and `prove` reading from it.
4. **Inference and guards as values.** This is where the numbers appear.
5. **Precise roots wired**: MIR safepoints plus `describe_frames`.
6. **Unboxing, `ElementLoad`, integer widths** — legal only now.

Swapping 5 and 6 is the one exchange that produces silent wrong answers:
`the-unwired-keystone.md` records an `ElementLoad` fast path that was 15.3%
faster and wrong in 53 of 60 cases, because a machine-typed derivative of a
reference is invisible to a collector that recognises references by bit pattern.

---

## What the stage covers, measured

**2026-09-20, `rts mir` over every fifth file of `tests/`: 182 files, 1 725
functions, 127 lowered.** Not a share to be proud of and not the point — what the
measurement is for is the *shape* of what is missing, and that turned out to
contradict the plan it replaced.

| refusals | reason |
|---:|---|
| **1 074** | a call |
| 82 | an assignment (compound, or to a pattern) |
| 61 | a generator |
| 52 | a nested definition |
| 41 | a function expression |
| 38 | a binding read before its declaration |
| 35 | a property access |
| 34 | an assignment to a property |
| 23 | a string literal |
| 21 | an object literal |
| 20 | a destructuring target |
| 10 | **a `do`-`while` or a `for`** |

A call is **thirteen times** the next item and two thirds of every refusal — *on
this corpus*, and that qualifier turned out to be the finding.

### The corpus decides the answer, and one corpus is one claim

The same instrument over `bench/` — 386 functions of programs written to be *run*
rather than to assert — answers something else entirely:

| refusals | reason |
|---:|---|
| **131** | a `do`-`while` or a `for` |
| 36 | an array literal |
| 27 | a nested definition |
| 27 | an object literal |
| 19 | a string literal |
| 16 | a property access |
| 14 | a call through a member |

`for` goes from ten to a hundred and thirty-one and from last to first; a call goes
from first to seventh. Neither table is wrong and neither is the answer: a test file
is a chain of `expect(…).toBe(…)`, so almost every call in it is a method on a
value or an import of the harness, while a benchmark is a loop over an array.

So the sentence this section first carried — *"the intuition was wrong by two orders
of magnitude"* — was itself a claim about `tests/` wearing a measurement's clothes.
The intuition that put `for` next was right about the programs the engine exists to
run. **A survey names its corpus or it says nothing**, which is the same rule the
merge gate states about a suite number and the same one `README.md`'s generated
block states about a share.

### The `for` was taken, and here is what it was worth

**`bench/`: 41 of 386 before, 46 of 386 after** — per file, 11 files both times,
one gained 38 → 43 of 349 and **none lost**, which is the only form the claim "no
regression" takes here. The 131 loop refusals are gone from the table entirely; the
count moved by five, because a function refused for a `for` is usually also refused
for an array literal or a property access. That is the ordinary shape of this work
and the reason the *distribution* is what a survey is read for, never the total.

The refusals that topped the table next were what a benchmark is made of — a
compound assignment (64) and an array literal (39) — and both were taken:
**46 → 54 of 386**, per file one gain of 43 → 51 and none lost.

### A refusal count that RISES is progress

After those two, the table reads:

| refusals | reason | was |
|---:|---|---:|
| 57 | a call through a member | 24 |
| 51 | a property access | 16 |
| 38 | an object literal | 33 |
| 27 | a nested definition | 27 |

The first two more than doubled, and nothing got worse. The survey records **one**
refusal per function — the first one the lowering hits — so a function that used to
stop at its `for` now gets as far as `arr[i]`. A row growing means work arriving at
it, and only the *lowered* count and a per-file comparison can say whether anything
regressed. Reading these tables as a scoreboard would have this session's best two
commits looking like its worst.

**And the measurement caught a crash before it was recorded as a result.** The
first reading after the loops landed said *3 of 37* where the corpus holds 386
functions: `self.values[binding]` panicked on a carried binding that holds nothing
yet, two files died, and a process that dies writes no line at all. A denominator
that falls by ten times is not a result — `Lowering::carried_now` is the fix and
carries the account.

### Property access and the string literal: 54 → 66

Per file, two gains and none lost. Reads, writes, indexed access and a string all
landed on one mechanism: **a constant of the LANGUAGE table**, which the IR carries
as an index and  interprets. Two reads of one property therefore
compare equal by number, so a pass asking whether two accesses touch one field
never compares names.

The table after it:

| refusals | reason |
|---:|---|
| 85 | a call through a member |
| 38 | an object literal |
| 37 | an expression kind |
| 32 | a binding read before its declaration |

**And the top item is a design question rather than a construct.**  is now
expressible as a read and a call -- except that a method call passes  as the
receiver and  has no receiver. Making argument zero the receiver
by convention is a decision about the CALLING CONVENTION, which is the machine
layer, and it is the kind of thing  warns about leaking: a
convention agreed in two places drifts. It is named here rather than settled in
passing.

### What each table asks for next

- `bench/`: the classic `for` and `do`-`while`, which are the `while` shape once the
  header is decided — and the header machinery exists.
- `tests/`: a member call, which needs a receiver proof (`emit/receiver.rs` is the
  existing one), and an imported binding, which needs an entry point.

**And the top item of that table was not a statement kind to lower — it was a
structural change.** A call needs to name a callee, `Callee::Func(FuncId)` means a
registry of the program's functions with ids, and a lowering that takes one function
at a time has nothing for a call to name. `lower_module` is that registry, and it is
built: functions numbered in source order, one domain shared, a callee map keyed by
`BindingId` so that two functions spelled alike are two entries.

**What it yielded on `tests/` was nothing, and that is stated rather than buried:
127 of 1 725 before and 127 of 1 725 after.** What changed is that the 1 074 calls
split into 844 through a member and 218 to a binding holding no function of this
module — which is what named the next two pieces of work. A function refused for one
call is usually refused for several, so resolving one kind of callee moved no
function into the lowered column.

Two defects in the instrument were found by running it, and both had made it
measure nothing:

- it parsed a **script**, and every file of the corpus imports `rts:test` — so the
  first survey answered `PARSE_FAIL` for 182 of 182;
- it looked only at **top-level declarations**, and a corpus file puts its code
  inside `describe(…, () => { … })` — so what it could parse, it reported as
  holding no functions.

That is the honesty floor's "verify the input, not just the output" landing on a
tool built in this same session: a 0% coverage reading and a 7% one look equally
plausible, and only the input said which was real.

## What this does not buy

It does not give JavaScript machine speed. It gives near-native speed to
**monomorphic** code — which is the shape well-typed TypeScript tends to have —
and everything else falls to the generic tier and stays where it is. Genuinely
polymorphic code, `eval`, and an object that changes shape at run time have no
fast version in a compiler without a deoptimiser, and no amount of analysis
changes that. `deopt-lateral.md` is how much of that ceiling is recoverable and
what it costs.

What the architecture does guarantee is that the **boundary** between the two
cases is visible and measurable, instead of being a surprise in a benchmark.

Every step of it is measured with `release`, per file against a kept binary. A
`fast` binary answers "is it correct", never "how fast".
