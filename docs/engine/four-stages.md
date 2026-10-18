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

**SUPERSEDED by the table near the end of this document, taken the same day after the
frame block.** Kept because the prose under it is about the finding it produced, not
about the counts.

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
landed on one mechanism: **a constant of the LANGUAGE's table**, which the IR carries
as an index and `domain::JsConst` interprets. Two reads of one property therefore
compare equal by number, so a pass asking whether two accesses touch one field
never compares names.

The table after it:

| refusals | reason |
|---:|---|
| 85 | a call through a member |
| 38 | an object literal |
| 37 | an expression kind |
| 32 | a binding read before its declaration |

**And the top item is a design question rather than a construct.** `o.m(x)` is now
expressible as a read and a call — except that a method call passes `o` as the
receiver and `rts_mir::cfg::Call` has no receiver. Making argument zero the receiver
by convention is a decision about the CALLING CONVENTION, which belongs to the
machine layer, and it is the kind of agreement `deopt-lateral.md` warns about: a
convention held in two places drifts. Named here rather than settled in passing.

### The object literal: 66 → 76, and why it mints no shape

Per file, one gain and none lost. It lowers as pairs of a declared key and a value
in **source order** — which is a semantic and not tidiness: the order properties are
added is what decides the layout, so reordering the pairs would mint a different
shape at run time and nothing would report it.

**It claims no shape, and `reuse-check` is why.** The machine already owns shapes:
`rts_cranelift::shape::ShapeTree` has `transition`, `slot_of` and `layout`, and
`names::Names` already mints its property keys from the same `KeyRegistry`. So
nothing new was written — and the search also found the thing that settles the
design: **the `ShapeTree` that decides layouts lives in `rts-core`'s `Context`.** It
is a run-time structure, and a shape is minted as a program adds properties. The
compiler holds none.

So an object literal cannot answer *"this is shape 7"* without the compiler and the
runtime agreeing about a number only one of them mints. `rts-codegen`'s rule 1 names
that as the exact failure it exists for — *"a second shape tree disagreeing with the
compiler's about which slot is which property"* — and rule 2 forbids it by saying
where a field sits is never decided there.

`Type::Shaped` therefore stays **unreachable by construction**, with the finding in
its own doc comment rather than as a hopeful variant. What would make it reachable is
the pattern the keys already use: `Names::keyed_texts` exists so the host can install
the compiler's keys into the runtime, and shapes minted at compile time and installed
the same way would give both sides one numbering. That is a design change across
three crates — `docs/engine/deopt-lateral.md` D1 has nothing to assert about until it
exists, so **the first guard is waiting on this and not on the guard machinery.**

### The receiver: 76 → 97 in `bench/`, 127 → 167 in `tests/`

The biggest single step of the campaign, and the decision it rested on is one word:
**the receiver is a FIELD of `Op::Call`, not its first argument.**

Every real calling convention passes it as argument zero, and that is the wrong shape
at this layer. A convention is an agreement held in two places — the front end that
packs it, the lowering that unpacks it — and this document's sibling records what
those do: they drift, and the drift compiles. A field cannot drift, and the proof
arrived immediately: adding it broke two `match` arms that had not mentioned it, in
the two modules that would have silently passed a receiver as an argument.

It also puts the machine question where it belongs. How a receiver reaches a callee
IS the calling convention, so `rts-mir/lower/` refuses one by name —
`NeedsReceiverConvention`, apart from `NeedsCallee` because a callee needs a registry
and a receiver needs a convention, and counting them together would hide which a
corpus waits on.

`o.m(x)` therefore lowers as: read the receiver **once**, read the callee from it,
call with the receiver travelling as itself. Once is the semantic — `o.m()` evaluates
`o` a single time, so reading it twice would run a getter twice, which is the same
mistake `a[i()] += 1` is refused for.

**And a call through a binding that holds no function of the module is no longer a
refusal either.** It reaches whatever the value is, which is `Callee::Dynamic` and
was expressible all along: the 218 refusals in `tests/` were about the MIR having no
receiver, not about that shape.

### A refusal that named the wrong work

`read_binding` reported three different things as a temporal dead zone: a binding
declared later in the function (which is one), a binding declared OUTSIDE it (a
module binding or a captured one — not a dead zone at all), and a function of the
module read as a value (which needs a closure). 66 refusals across both corpora wore
the wrong name, so the surveys were pointing at a sentinel-and-throw that almost none
of them needed.

Told apart, the top of both tables becomes the same real item — **a binding declared
outside this function**, 912 in `tests/` and 54 in `bench/` — which is the
environment this stage does not build, and it is now the biggest single piece left.

### The outer binding: 97 → 127 in `bench/`, 167 → 1017 in `tests/`

The dominant item taken, and the largest move of the campaign by a wide margin —
59% of the `tests/` corpus now lowers, from 10%. Per file in `bench/`: five gains,
none lost.

**And it was expressible all along, because of rule 2.** A binding declared outside
the function is a cell somewhere — a module record, an environment object, a slot an
enclosing activation holds — and *where it lives* is a machine question, which
`rts-codegen`'s rule 2 says is never decided in the language layer. So the lowering
says **which binding** and stops: `outerread(@total)`, with the binding named by a
constant of the language's own table.

That also makes two accesses to one outer binding carry ONE index, which is what a
pass hoisting a load out of a loop compares — comparing `BindingId`s inside the
lowering would have given a pass reading the finished graph nothing.

Closure conversion is the other answer and was rejected for now: making every free
binding an extra parameter needs each call site to supply it, and a `Callee::Dynamic`
site does not know the callee's free set — so it would refuse exactly the calls that
most need it.

The effect is `READS|THROWS` and deliberately not `CALLS_USER`: a binding in its
temporal dead zone throws, and a binding is not a property, so no getter is reachable
through one. A write is `WRITES|THROWS`.

### Naming the bucket, and the bitwise row: 127 → 161 in `bench/`

Per file, one gain and none lost. Two changes, and the first is not a feature at all.

**`expression_name` had a fall-through arm reading "an expression kind", and it was
the biggest single bucket in `bench/` — 43 of them.** A refusal that does not name
itself is worth the same as no refusal: the whole reason `Unsupported` has one variant
per reason is that the survey IS the work queue, and a bucket cannot be queued. Every
variant is listed now, so a node added to the tree tomorrow fails to compile there
rather than joining a bucket. The 43 turned out to be a construction (31), a type
assertion (8) and a handful of others — none of which anybody would have guessed.

**The bitwise row is one row for five operators**, because they agree about everything
the table records: each coerces with `ToInt32`, each answers a value that fits in an
`i32`, and none reaches code the program wrote once the operands are not objects.
What they disagree about is which machine instruction they become, which is the
machine lowering's question.

`>>>` is deliberately NOT in it. It answers `ToUint32`, so `-1 >>> 0` is 4294967295 —
a number an `i32` cannot hold, and the one bitwise operator whose answer is not an
`Int32`. Giving it the row would be wrong at exactly the value that distinguishes it.

And the row pays for itself twice, visibly:

```text
v2 = bitwiseint32(v0, v1)   ; calls|throws     ← x | 0, x unknown
v4 = bitwiseint32(v2, v3)                      ← n & 255, PURE
```

`x | 0` is how a program makes a number provably narrow, and every bitwise operator
after it is pure because the first one proved its operand. That is the type domain
paying for itself on a shape real code writes constantly.

### `this` and the unary operators: 161 → 188 in `bench/`, 1021 → 1082 in `tests/`

Per file in `bench/`: five gains, none lost.

**`this` got the same answer the outer binding did, for the same reason.** Where the
receiver of an activation lives — an extra parameter, a register the convention
reserves, a slot the frame holds — is the machine's calling convention, so the
lowering emits `thisvalue()` and stops. It is also the other end of the receiver
field on a call: one says a receiver travels, the other says the callee reads it, and
neither packs it into an argument list. `rts-mir/lower` refuses both under the same
`NeedsReceiverConvention`, which is where that decision belongs.

Three of the unary operators are rows, and each of the other four is a decision:

- **unary plus is `ToNumber`** and gets no row. `+a` and the coercion an increment
  performs are the same operation; two rows would let a pass fold one and miss the
  other.
- **`-a` is not a subtraction from zero.** `-0` is `-0` and `0 - 0` is `+0`, and the
  two are distinguishable by `Object.is` and by division. It also answers a number
  and never an `Int32`, because negating the most negative one does not fit.
- **`void a` evaluates its operand** and answers `undefined`. Dropping the operand
  would drop its effects.
- **`delete` keeps its refusal**: it removes a property, so its operand is a PLACE,
  and lowering the operand first would evaluate what is about to be deleted.
- **`UnaryOp::IteratorResult` keeps its own refusal**, named. It is not an operator a
  program can write — a `for`-`of` expansion mints it so that raising the loop's
  `TypeError` needs no binding a program could shadow. Refusing it silently would
  refuse `for`-`of`; lowering it as a no-op would drop the check.

### The list, worked through: 188 → 309 in `bench/`, 1082 → 1489 in `tests/`

Five items planned together rather than one per session, each measured, none losing a
file. `bench/` is 80% and `tests/` is 86%, from 49% and 63%.

| item | refusals it closed | what it decided |
|---|---:|---|
| a type assertion | 8 | an annotation is evidence, not proof |
| a global read | 75 | resolved through the global object, like the language does |
| a construction | 31 | not a call: it allocates and runs a body |
| a function value | 376 | names which function; the environment stays below |
| three comparisons | 15+ | rows of their own, never a swap |

**Four of the five are the same answer**, and it is rule 2 each time: where a thing
lives is a machine question, so the lowering says WHICH and stops. A global is a
property of an object nobody named; a receiver is a slot the convention picked; an
outer binding is a cell somewhere; a closure is code plus an environment. In every
case the operation carries an identity and the storage decision stays below.

The closure is the one worth reading twice. `makeclosure(f1)` takes one argument — the
function — and no captured list, because the callee's OWN graph reads its free
bindings through `outerread`. So the captured set is derivable from the graph, which
is the one place it cannot drift from:

```text
fn outer                        fn step
  v1 = f1                         v1 = @n
  v2 = makeclosure(v1)            v2 = outerread(v1)
  v6 = call f1(v5)                v3 = add(v0, v2)
```

**And the comparisons are a refusal that was right about the wrong thing.** They were
refused for three commits because `a > b` is not `b < a` — the two coerce their
operands in opposite orders, which is observable. That argument refuses the REWRITE
and never the operation, and giving each its own table row costs one entry and keeps
the order the program wrote.

Six tests were replaced rather than relaxed, and two of them had already followed the
work once: a global call was asserted refused, then asserted refused while the member
call lowered, and now both lower — so what it pins is that they lower to DIFFERENT
shapes, one with a receiver and one without.

### The choices: 309 → 318 in `bench/`, 1489 → 1511 in `tests/`

Per file, one gain and none lost. `?:`, `&&`, `||` and `??` are **one** shape with two
knobs — what the condition is, and what the arm that does not evaluate the right side
answers — so they are one file and one join. Four copies of a join is where three of
them stop agreeing.

Each knob is a semantic:

- **`a && b` answers `a` ITSELF** when `a` is falsy, not `false`. `0 && 1` is `0` and
  `"" && 1` is `""`, and five of the seven falsy values are not `false` — so a
  lowering that answered a boolean would be wrong for every one of them.
- **`a ?? b` is not a truth test at all.** `0 ?? 1` is `0` where `0 || 1` is `1`, which
  is the whole reason the operator exists. Its condition is `IsNullish`, a row of its
  own, because building it on `Truthy` would be wrong for those same five values.

And a detail worth keeping: the jumps into the join are written **after** both arms
are lowered, from `builder.current()`. An arm that nests another choice moves where
building is, so terminating the block the arm *started* in would terminate the wrong
one — which `a ? (b ? 1 : 2) : c` is the test for.

The two literals that still refuse are now named apart — a regular expression is an
object the runtime builds, a bigint is a second numeric tower — because a survey
counting them together says neither.

### The table rows and `switch`: 318 → 326 in `bench/`, 1511 → 1536 in `tests/`

Per file, three gains and none lost. Four operators became rows, two became a
negation, and `switch` became a chain with a merge.

**`!==` and `!=` are a negation, and this is where rewriting IS legal** — which is
worth stating beside the comparisons, where it was not. `a > b` could not become
`b < a` because the two coerce in opposite orders and that is observable. `a !== b`
becoming `!(a === b)` changes nothing: the same operands, in the same order, by the
same operation, with the negation applied to a boolean it already produced. One costs a
table row; the other costs two instructions a pass can fold.

`==` is a row apart from `===` rather than a laxer spelling of it: it may call
`valueOf` where strict equality calls nothing, so the two have different EFFECTS over
the same operands. `docs/codegen/entry-tax.md` part five is about this operator, and
the finding travels with the row — `x == null` ran `ToPrimitive` twice per comparison
where the specification calls it zero times, and answered correctly the whole time at
180 times the cost.

`instanceof` and `in` both read the heap and both may reach user code: one through
`Symbol.hasInstance`, which replaces the whole algorithm, the other through a proxy's
`has` trap.

### A `switch` merges, and skipping that was a wrong answer

The first version carried no bindings, reasoning that a switch has no back edge. It
has no back edge and it has several paths into one exit, which is a different
question — and the graph said so at once:

```text
b2:
  ; from b1, b5
  v8 = add(v6, v7)     ← v6 is defined in b1 only
b4:
  return v9            ← v9 is the default clause's value, on every path
```

**`rts_mir::verify` did not catch it**, and its own header says why: within a block it
checks order, across blocks only existence. Dominance is the real rule, and this is
the first thing that would have been caught by it — recorded there as the check to add
when a pass starts reordering blocks.

And one thing the frames had to learn: `break` leaves the innermost loop OR switch,
while `continue` names a loop. So a frame carries which construct it is, and a
`continue` inside a `switch` inside a loop walks past the switch. Without that the two
stacks are one and the `continue` leaves the loop instead of taking its next pass —
a wrong answer that compiles, with a graph that looks perfectly well formed.

### Object destructuring: 326 → 327 in `bench/`, 1536 → 1536 in `tests/`

**One file gained and the totals barely moved, which is the honest headline.** What
the change bought was not coverage but a NAME: the 23 refusals reading "a
destructuring target" became 15 that say *an array pattern steps the iteration
protocol*, and the other eight got past the declaration and stopped somewhere else.
The `tests/` total is unchanged because a function refused for one thing is usually
refused for the next as well.

**`const [a] = xs` is not `a = xs[0]`**, and that is the finding worth the file. Array
destructuring steps the iterator protocol — it reads `xs[Symbol.iterator]`, calls it,
and calls `next()` per element — so it works on a `Set`, on a generator and on
anything with a `next`, and it does NOT work on an object with numeric keys and no
iterator. A lowering that indexed would be wrong in both directions at once: accepting
what the language refuses and refusing what it accepts. So it keeps a refusal, under
the same name as `for`-`of`, which is the one piece that answers both.

**A pattern default is a branch, not a coalesce.** It runs only when the value read was
`undefined` — `{ a = 1 }` over `{ a: null }` binds `null`, because `null` is a value
that was there — and it is evaluated only when needed, so `{ a = f() }` over an object
that has `a` never calls `f`. `IsNullish` would be wrong for the first and an argument
would be wrong for the second.

A nested pattern, a computed key and an object rest each keep their own refusal. The
rest is the interesting one: it collects the own enumerable properties *not already
named*, which needs the key set at run time and is not something an ordinary read can
stand in for.

### The protected region: 327 → 328 in `bench/`, 1536 → 1541 in `tests/`

Per file, one gain and none lost. A small move for a large piece, and the piece is the
point: **`rts-mir` has regions now**, which is the first structural addition to the
shared IR since it was written.

A region is neutral and belongs there. Every language with exceptions needs to say
*these instructions are protected, and control goes THERE when one raises*, and none of
them needs it said differently. What is NOT neutral is what a handler catches — one
language catches everything with one clause, another matches a type, a third has a tag
per raise site — so the tag is the language's and arrives through `MachineOps`, which is
why the machine lowering refuses a region by name (`NeedsHandlerTag`) rather than
inventing one.

Membership is per BLOCK, not per instruction: a region is a span of control, and a block
is either inside the `try` or it is not. Marking instructions would let one block hold
two answers, and the first thing that breaks is a call in the middle of one.

### The restriction, which is a finding rather than a shortcut

**A binding the protected body assigns cannot reach the handler as an SSA value**, and
the first draft of the lowering tried to pass one as a block parameter. That is wrong in
a way that compiles: *nothing jumps to a handler*. Control arrives along an exception
edge from an unknown point of the body, so there is no jump to carry an argument and no
single value to carry — the raise may happen before the assignment or after it.

```js
let x = 1;
try { x = 2; mayThrow(); } catch { use(x); }   // x is 2 here
```

The answer real compilers give is memory: such a binding lives in a cell and the handler
reads it. That is the machinery an outer binding already uses, so what this waits on is
a LOCAL MOVED INTO A CELL — escape analysis in reverse, and a pass rather than a
lowering. Until then the shape is refused by name.

`finally` is refused for its own reason: it runs on every way out — falling off the end,
`return`, a raise the handler did not take, a `break` leaving the region — so it is not a
block reached from one place. A block placed after the `try` would run it on the
falling-off path and silently skip it on the other three.

And `rts mir` prints which region protects a block, because an exception edge has no jump
to print: without that line a protected block looks as though nothing can leave it except
through its terminator, which is the one thing it does not do.

### The class and the first entry point: 328 → 334 in `bench/`, 1541 → 1551 in `tests/`

Per file, six gains and none lost. Two independent items, and each one settled a
question rather than adding a case.

**A class is three things once the sugar is gone**: a constructor function, an object to
hold the methods, and the link between them. All three were already expressible — a
closure, an object, a property write — so nothing new was needed:

```text
v1 = makeclosure(f1)        ← the constructor
v2 = newobject()            ← the prototype
v5 = makeclosure(f2)        ← the method
v6 = fieldwrite(v2, .twice, v5)
v8 = fieldwrite(v1, .prototype, v2)
```

`extends` is refused with its three reasons: `super()` must run before `this` exists in
a derived constructor, `super.m()` reads from the home object rather than from the
receiver, and the chain has two links to set rather than one. A class that inherited
without them would compile and would get `super` wrong.

The constructor test goes through `Method::is_constructor`, which is the tree's own — it
already says that a static member and an accessor are never the constructor however they
are spelled, and a second copy of that rule is a second place for it to drift. That is
why the lowering now carries `&Names`: the question is about TEXT, not about a binding.

And the prototype key is a `JsConst::WellKnown` rather than an interned name, because
the program never wrote the string `prototype` — asking the interner for it would need a
mutable interner in the lowering for text that is not the program's.

**And the regular expression made `Callee::Entry` reachable for the first time.**
Compiling a pattern, allocating the object and installing its `lastIndex` are not things
a lowering can express as instructions, and they are one thing the runtime *does* —
which is what an entry point is. So `domain` has an entry table beside its primitive
table, for the same reason and with the same rule: the index is opaque to the IR.

```text
v0 = "ab+c"
v1 = "gi"
v2 = call regexnew(v0, v1)   ; calls|throws
```

An index nothing implements yet is honest: the graph says which operation it wants, and
the machine boundary refuses until the entry exists. `rts-host/src/entries.rs` is where
the name and the ABI shape get agreed.

### What is left, re-measured after the frame block

**The corpora are named because the two columns do not share one.** `bench/` is every
file, 14 of them, 397 functions. `tests/` is every FIFTH file — 491 files, 3 473
functions, 2 978 lowered — because `rts mir` over 2 451 files one process at a time is
an hour and the shape of what is missing does not need the hour. A share taken from a
sample is not comparable with one taken from the whole, which is why neither is quoted
here as a share.

| `bench/` | | `tests/`, every fifth file | |
|---:|---|---:|---|
| 6 | a class field | 67 | a bigint literal |
| 5 | a call through neither a name nor a property | 34 | a template literal |
| 4 | a template literal | 33 | a call through neither a name nor a property |
| 4 | an object literal with a method | 31 | an object literal with a method |
| 3 | a class with no constructor written | 22 | a rest parameter |
| 2 | an optional chain | 21 | a function of this module read as a value |
| 1 | `**` has no row | 20 | a super call |
| 1 | `>>>` has no row | 18 | a `finally` that completes abruptly |

**Re-measured after the gathering half: `bench/` 357 of 397, `tests/` 3 089 of 3 473.**
The two columns above are unchanged row for row — what left was `an array literal with
a hole or a spread`, which was the ninth row of the right-hand column and is not in the
eight shown. Recorded rather than silently dropped, because a table that only ever
loses its visible rows would make the work look like it had stopped.

**`an iteration protocol` was the TOP row of the left-hand column and is gone from both**
— 8 of 14 files in `bench/` and 27 of the sample. The section at the end of this
document has it. `a throw` and a plain `finally` left the same way one block earlier.

**`bench/` has run out of one-line entries.** Its whole column is now eight rows
totalling 26 refusals over 397 functions, and the last two are single operators with no
row in the primitive table. That is a different kind of list from the one this started
as: no entry in it is a mechanism any more.

**`a throw` and `a finally` were rows two and three of the right-hand column and are
gone**, which is the cleanup-chain section at the end of this document. What is left of
that group is 18 `finally` bodies that can complete abruptly, which are a different
SHAPE rather than a missing feature.

**A generator and an async function do not appear at all**, which is what the block
above did: they were the first and second rows of the `tests/` column and they are gone
rather than reduced.

**What moved to the top was a redirection, and it was taken.** A `throw` and a
`finally` were one piece — raising is an entry point and a cleanup chain is what runs
on every way out — and together they were 106 of this sample, the largest thing on the
list by a distance. Both are done; the section at the end has them.

**That redirection was taken too, and it contained an error of mine.** `for`-`of` and
the array pattern step the iterator and close it, on the cleanup chain the block before
built. The gathering half — an array literal with a spread, and an array rest target —
is done as well, on `array_append` and `array_append_all`.

**A rest PARAMETER was in that list and does not belong to it.** `function f(...r)`
gathers the call's ARGUMENTS, not an iterable: it needs the argument count at run time,
which is a calling convention and therefore the machine's. It is refused as a function
SHAPE and always was; what was wrong is the grouping, not the refusal. 22 of the sample
sat under the wrong heading because two things both use the word "rest".

`yield*` is still in the stepping group.

A bigint literal is a second numeric tower and is nobody's next hour, whatever its
count says.

**A generator and an async function were one piece, and that entry is DONE** — both
rows are off this list. The section at the end of this document has it; what the entry
got wrong is worth keeping, because it said both were waiting on
`rts_cranelift::frame` and only the CALLER's half was. The body of either lowers, and
`yield*` turned out to belong to the iteration-protocol row below rather than to this
one.

**An array pattern, `for`-`of` and a rest parameter are one piece**: all three step the
iteration protocol, and all three owe the iterator its `return()` on an early exit —
which is the cleanup chain a `finally` needs, so it is really the same piece as that.

**A method in an object literal and a class field are one piece**: both are installed
rather than assigned — a method with a home object, a field per instance as the
constructor runs.

---

## Has any of this changed how fast RTS runs?

Asked on 2026-09-20, and the answer has two halves.

### The MIR: nothing, and that is verifiable rather than argued

The only path into the stage is `rts mir` → `rts_host::describe::describe_mir` →
`rts_codegen::mir_dump`. `run`, `test` and `compile` never reach it — checked by
searching the workspace for callers of `lower_module`, `lower_with` and `rts_mir::`
outside the stage itself and its tests. So no program compiles differently because the
MIR exists, and no benchmark can have moved.

What did change is the build: one crate and some thousands of lines more to compile.
That is a cost paid by whoever builds, not by anything that runs.

### One commit DID touch the running path, and it costs speed on one shape

`fix(codegen): an omitted helper's free name could resolve to a block of its own
declarer` changed `emit/omit.rs`, which is the old emitter — the one every program goes
through. It refuses strictly more, and the commit said so and said the clock had not
been read.

The decision it changes is now named exactly. `rts prove` over the two programs below
differs in whether the helper exists as a compiled function at all:

```js
let i = 7;   const q = (x) => x + i;     for (let i = 0; …) s = q(s) | 0;   // q EXISTS
let zwq = 7; const q = (x) => x + zwq;   for (let i = 0; …) s = q(s) | 0;   // q is gone
```

In the second the call is substituted and the closure omitted, so `q` is not in the
report. In the first the guard refuses the omission, so `q` is compiled, its closure is
built, and the loop makes a real call. **The only difference is the spelling** — which
is the same sentence the 233.67-against-46.33 ns measurement of 2026-08-30 carries,
about this same shape.

**What is NOT claimed: a fresh number.** That would need a release build and a kept
baseline, and neither was taken. What is established is the structural change, which a
debug binary answers honestly, plus the repository's own earlier measurement of the
identical shape.

### And this is the argument for E2 in one paragraph

The guard is the correct answer available to a compiler that identifies bindings by
SPELLING: it must refuse whenever two declarations share one, because it cannot tell
which the helper read. The cost is real and it is the price of the wrong answer it
removed — `11,110,100` where node says `11,11,100`.

A compiler that identifies bindings by IDENTITY has neither the cost nor the wrong
answer: the two `i`s are two bindings, the helper reads the one it was written against,
and the substitution is legal. That is what `names::resolve` answers and what the MIR
stage is built on — so the 5× is not a trade this design makes, it is a trade the OLD
stage cannot avoid.


---

## The frame block: a generator and an asynchronous function are ONE thing

This was on the list as the architectural piece, and the piece turned out to be
smaller than the entry beside it said — because the entry was wrong about where
the missing part was.

Both kinds used to be turned away at the function, each with its own line: *"an
async function parks its frame"*, *"a generator parks its frame"*. Both lines are
gone, and not because parking got approximated. **The body of one was never the
missing piece.** A generator's body is a graph with a suspension in it; so is an
asynchronous function's. What neither body contains is the thing that is actually
absent: *calling* one runs no body — it answers a generator object or a promise —
and that is the CALLER's sequence, which follows from the callee's flag. Rule 2
puts it on the machine, and the machine refuses it there by name.

### What the shared IR gained, and what it deliberately did not

One instruction and one effect flag. `Op::Suspend { value }` hands a value out
and answers what comes back; `Effect::SUSPENDS` says the frame may be parked.

The flag is where the design decision is. Parking could have been a primitive the
language declares, and that is the wrong place for exactly the reason rule 2
draws the line by: **a language's table is what a consumer is allowed not to
understand**, and every consumer has to respect a suspension. Nothing may cross
one in either direction — between its two halves, whoever resumes decides when,
and the program keeps going meanwhile — and nothing may be placed after one on the
assumption control arrives, because `gen.throw(e)` and a rejected promise both
resume the frame *by raising at that point*.

`Func::may_suspend` is **derived by the builder from the effect**, never passed
in. The alternative — asking a lowering to push the suspension and also set the
flag — is one fact in two places, and the drift it produces is not a compile
error: it is a function the machine compiles with an ordinary frame and then
tries to leave. `verify` checks the agreement anyway, which is only reachable for
a `Func` assembled by hand, and that is what it is for.

Reading it from the EFFECT rather than from `Op::Suspend` is the part worth
keeping written down. A language that parks inside a primitive of its own is
covered without this crate naming that primitive — which is rule 1 holding under
a feature that looks like it needs an exception.

### Why it is an instruction and not a terminator

Because the CFG does not have to split. `rts_cranelift::frame` transforms the
whole function into a resumable form from **liveness** — it spills what is live
across each suspension into a record with a resume position — so a graph that
split its blocks at every suspension would be doing that transform's work badly,
and twice. The graph's job is to say that control leaves, and the effect says it.

### What a suspension answers, which is the one unsound narrowing available here

The top of the lattice. `next(x)` chooses what comes back, and so does a promise
settling; this function computed neither. The natural mistake is to narrow the
result from the operand, because the operand is right there and the types look
like they should agree — and what that produces is a type a later pass trusts
with nothing checking it. `yield 1` answers `Anything`, and a test pins it.

### The reuse-check finding: an entry point that exists and is not called

`rts-core` already has the entry point a compiled `await` calls —
`entry::promise::promise_await`, `RtEntry::PromiseAwait` — and this lowering does
not name it. Its own callers say why.
`crates/rts-core/src/entry/array_proto/more/from_async.rs` records that *"`await`
here DRAINS rather than suspending — the awaiting frame keeps the stack"*, and
that *"when `Inst::Suspend` lands, this changes with every other `await`"*.

So the existing entry point is today's shape, and today's shape is the one this
stage exists to replace. Calling it from the new graph would have compiled, would
have passed, and would have written "await means drain" into the IR whose reason
for existing is that await means park. The suspension is emitted instead, and the
machine refuses it by name until `frame::resumable_form` is wired.

**`Unlowerable::NeedsFrameTransform` is the only refusal in `lower/` that is
about the MACHINE** rather than about something the language has not declared.
Every other one names a missing declaration; this one names a graph that is
entirely well formed and a capability the machine has not been asked for. Worth
distinguishing, because the two are fixed by different people doing different
work.

### What stays refused, and it is not what it looks like

`yield*`. It is not a suspension — it is a loop around one, forwarding `next`,
`throw` and `return` to an inner iterator and yielding whatever that yields. So
it needs the **iteration protocol**, which is the same piece the array pattern,
`for`-`of` and the rest parameter are all waiting on, and it joins that group
rather than this one. A `yield*` lowered as one suspension of the inner *iterable*
would compile and hand out the wrong value.

That regrouping is the useful part of the finding: what looked like one gap
("generators and async") was two, and the halves belong to different blocks.

### Measured, per file, same denominators

A debug binary of `a4f030ca0` against the tree with the block in it, one process
per file, `rts mir` counting functions lowered:

| corpus | before | after | of | files | LOST |
|---|---:|---:|---:|---:|---:|
| `bench/` | 345 | 347 | 397 | 14 | **0** |
| `tests/` | 14 323 | 14 778 | 17 194 | 2 451 | **0** |

455 functions gained across 163 files, the denominator unmoved on both sides, and
the LOST list empty — which is the only form the claim "no regression" takes here.
The baseline was a separate `git worktree` at `HEAD` rather than a stash, so the
working tree never moved to produce it.

**No number about speed is claimed, and the section above says why**: nothing that
runs reaches this stage. `run`, `test` and `compile` do not call it.

---

## The cleanup chain: `throw`, then `finally`, and both refusals were machine answers

The measurement above picked this block rather than a plan doing it, and it picked
it for the right reason: `throw` and `finally` were 106 of a 491-file sample and
they are one mechanism.

**Both refusals were wrong in the same way, and it is worth naming the shape of
the mistake.** Each described something true about the RUNTIME and let it stand
for the graph:

- *"a throw raises, which is an entry point rather than control flow"* — recording
  the value is an entry point, and where control goes afterwards is the region
  tree, which is control flow this lowering already built.
- *"a finally runs on every way out, so it is not a block reached from one
  place"* — it does run on every way out, and routing those paths was never this
  lowering's job.

Neither statement was false. Both answered a machine question in order to turn a
statement away, which is the failure rule 2 exists to prevent read in the other
direction: a lowering may not decide a machine question, and refusing on the
strength of one is deciding it.

### What the shared IR gained

Two terminators, and both are neutral forms of something the machine already had.

`Terminator::Raise(value)` **names no successor**, and that is a claim rather than
an omission. It is the same thing `region.rs` already said about a handler:
nothing jumps to one, so nothing carries arguments to one, so its predecessors are
empty. A raise naming its handler as a successor would make that false, and every
pass reading the graph as a CFG would then expect an argument list nothing can
supply. Where it lands is `region_of` and `Region::parent` outward from there —
the search `plan_unwind` computes.

It carries **no tag**, because a tag says which handlers match and *what may be
thrown* is the one question `unwind`'s own header refuses to answer for a
language. So it is refused at the machine with `NeedsHandlerTag`, named as the
same missing declaration a region waits on rather than as a second thing.

`Terminator::CleanupDone` takes **no continuation parameter**, and the machine's
own doc records why that alternative lost: it *"would make every cleanup able to
reach every continuation, which is an edge in the graph for every pair and no
useful analysis afterwards"*, and the representation has no indirect branch to
lower it to. A cleanup is a **piece** rather than a block — it may branch and
merge inside itself and end this way in several of its blocks, still one exit
because they all leave to the same place.

`Unreachable` gained one line saying it is not either of these. It is a trap, and
a language that lowered `throw` to it would get an abort where the program expects
a catchable value.

### The trap in checking it, which the checker would have fallen into

`verify` refuses a `CleanupDone` that no region owns, and it finds the piece by
walking outward from each region's cleanup **entry** — *not* by asking
`region_of` about the block. That distinction is the whole thing: a cleanup block
is created BEFORE its region opens, so it belongs to whatever encloses the region
and never to the region whose cleanup it is. The natural check is the wrong one,
and it would have rejected every correct cleanup.

The same fact is load-bearing in the lowering for a different reason: inside its
own region, a `finally` that threw would re-enter its own `catch`.

### Two shapes for one keyword, and only one is built here

A `finally` that can complete **abruptly** is not a cleanup at all. `try { return
"t" } finally { return "f" }` answers `"f"` — an abrupt completion in the
`finally` replaces the pending one — and a `return` inside a copied cleanup is a
terminator with no successor, which is a copy left through a path the unwind knows
nothing about. The machine's verifier already names it: `CleanupDoesNotEnd`. The
correct shape for that case is a catch-all **handler**, where a return is an
ordinary return and re-raising on fall-off puts the pending throw back.

`emit/protect.rs` builds both and chooses. This lowering builds the cleanup and
refuses the handler shape by name, which is 18 of the sample.

**The predicate is shared with that file rather than written again.**
`leaves_abruptly` over-approximates in the safe direction, says in prose which
direction that is, and counts a `yield` for a reason one step downstream — the
frame transform turns each suspension into a return. A second copy is a second
place for the safe direction to be got backwards. Its home is `syntax/`, beside
the walkers, and it goes there when they move.

### One combination refused for a reason neither half has

A cleanup **beside** a handler that assigns. The cleanup is copied into the body's
exit path and into the handler's, those two disagree about what the binding holds,
and so one copy would read a value the other path defined. Each half lowers
alone, and a test pins that it is the combination — because a refusal that names
a combination is the kind that gets over-generalised into a refusal of both.

### Where the sample stands now

Same instrument, same stride, three measurements of the `tests/` corpus at every
fifth file — 491 files, 3 473 functions:

| | lowered | `throw` | plain `finally` |
|---|---:|---:|---:|
| before the frame block | 2 978 | 61 | 45 |
| after `Raise` | — | **0** | 45 |
| after `CleanupDone` | **3 053** | **0** | **0** |

What is left of the group is the 18 abrupt `finally` bodies, which are the handler
shape.

**And the next mechanism is already named by the same table.** The iteration
protocol — `for`-`of` at 27, a rest parameter at 22, a destructuring target at 14,
an array literal with a spread at 12 — is one piece, and it is the piece that owes
the iterator its `return()` on an early exit. That is the cleanup chain again,
from the other side: `yield*` joined this group when it left the frame block, and
the chain it needs is the one that now exists.

Per file against a kept binary at each step, `bench/` and `tests/`, LOST empty
every time.

---

## The iteration protocol, and the entry point that was right for somebody else

The table pointed here and this is what it found: the operation that looks like the
answer already exists, and using it would have been wrong.

`rts_core::entry::iterate` turns an iterable into an array, is reached by
`CoreEntry::Iterate`, and is one call where this is a loop. Its own header says who
it is for — *"what still arrives here is everything that must consume the WHOLE
sequence to answer at all"* — and a `for`-`of` is not that. The old emitter measured
the three costs of pretending otherwise, against Bun, before rewriting itself:

- a `break` never reaches `return()` on the iterator — a leak;
- a `Map` or `Set` the body mutates is walked as it was BEFORE the body ran — a
  wrong answer;
- a source that never reports `done` is drained forever instead of ending the pass
  it was told to — a program that stops terminating.

So the reuse-check finding here is the *inverse* of the usual one. The usual finding
is "this exists, call it". This one is "this exists, and calling it is the defect".

### Three ways out, and only one of them is a jump

| leaving by | closes? | how |
|---|---|---|
| `done` | **no** | the sequence ended itself; nothing is owed |
| `break` | yes | a block between the loop and the exit |
| `return`, or a raise | yes | the region's **cleanup**, copied in by the machine |

`return` and a raise are not jumps out of the loop — they leave the function — so
no block this lowering writes could be on their path. That is exactly what the
cleanup piece is for, and it is why the `finally` work had to come first: this
block consumes it.

**The `done` path must not close**, and that is the reason the close is not simply
the cleanup for all three. A cleanup runs on every way out of a region, and
`it.return()` after `done` is an observable extra call on a user's iterator. So the
region is entered for the BODY and left before the exit, and `break` carries its
own closing block.

The obligation itself is not decided here. `ForEachSource::owes_iterator_close` was
already in the tree, stating it once.

### `return` is optional, and that is what `CleanupDone` said it allowed

An iterator need not have one, and calling `undefined` would raise where the
specification says do nothing. So the close reads the key, asks whether it is
nullish, and calls only if it is not — a piece that **branches inside itself**,
which is precisely the shape `Terminator::CleanupDone`'s own doc says a cleanup may
take. The feature was written for this and used by it two commits later.

### Four well-known keys, which the enum predicted

`WellKnown`'s first version carried one variant and said: *"a second is expected —
the iterator key a `for`-`of` reads is the same shape of thing"*. It reads four:
`Symbol.iterator`, `next`, `done`, `value`, plus `return` for the close.

They are here rather than asked of the interner because **the program never wrote
them**. `for (const x of xs)` contains no `next` and no `done`, so there is no
spelling in the source to have a `Name` for, and minting one during lowering would
need a mutable interner here — the same reason `class.rs` gives for `prototype`.

And they are KEYS, not operations. Reading `done` off a step result is an ordinary
property read; a `Prim` for it would be this language claiming the read is special
when only the key is. `done` is read with `Truthy` and never compared against
`true`, because the specification says ToBoolean: an iterator answering `done: 1`
ends the loop.

### The specialisation that is deliberately absent

`emit/foreach.rs` carries **two arms in one loop** — an indexed walk for an array or
a string, the stepped protocol for everything else — because stepping costs a
`{ value, done }` allocation per element, including for `for (const x of anArray)`,
which is the common case the indexed walk exists to avoid. That is a real cost and
the dual arm is a real answer to it.

It is not reproduced here, and the reason is about *where the decision belongs*
rather than whether it is worth making. The old emitter had to decide it while
emitting, from syntax, which is why it emits both arms and lets one be dead. Here
the question is *"is this value an array"* — a type and a guard, which is
`rts_mir`'s own machinery applied by a pass over a graph that already says what it
is doing. Writing the dual arm into the lowering would spend the thing this stage
exists to provide.

**So this emits one honest loop, and the specialisation is a pass. What it must not
do is emit one honest loop and call it fast** — which is why no number about speed
appears anywhere in this section.

### The array pattern, on the same three helpers

A fixed number of steps rather than a loop over them, and it found one thing worth
writing down. **A slot past the end binds `undefined`, not the step's own `value`.**
`{ done: true, value: 42 }` is a legal answer from a hand-written iterator, and the
specification says the slot is `undefined`. Reading `value` unconditionally is
correct for every well-behaved iterator and silently wrong for that one — so each
slot is a step, a truth test, and a join whose two arms are the value and the
singleton.

A **hole** still takes a step and binds nothing, which is why the tree keeps it as
an absent element rather than omitting it: dropping one would shift every element
after it onto the wrong value. A test pins that the hole *costs* a step, by counting
against the same pattern without it.

The close is owed only when the pattern stopped first, which the last slot's `done`
test decides. A pattern with no elements at all closes unconditionally, because it
stepped nothing and so never reached `done`.

### What was refused, and each names a different missing thing

| refused | what it waits on |
|---|---|
| `for`-`in` | nothing here — it walks the prototype chain, which is not this protocol |
| `for await` | every suspension sits inside the region that owes the close, and the frame transform has a measured bug class exactly there |
| an array **rest** target | it drains from the CURRENT position, so `iterate` is wrong for it too; it needs an append |
| a nested pattern in a slot | the same refusal the object form gives, under the same name |

The `for await` one is worth reading twice, because it is the frame block and this
block meeting: `frame/transform.rs` records that a `Return` left inside a region ran
its `finally` **three times** for `try { yield 1; yield 2 } finally { … }`. A `for
await` is that shape by construction.

### One real bug, and it was the same bug twice

The per-pass binding lives in the loop head's own scope, and without entering it the
target is not found at all and reads as a **global**. That is precisely what the
`catch` clause reported the first time it ran, and it took the same fix. The
resolver had already opened the scope; only the lowering did not step into it.

Worth noting as a pattern rather than an incident: this stage has now got the same
thing wrong twice, in two files, because a scope that the resolver opens is invisible
to a lowering that does not ask for it. Nothing structural prevents a third.

### Where the sample stands

`tests/`, every fifth file — 491 files, 3 473 functions — across this whole stretch:

| measured after | lowered |
|---|---:|
| the suspension | 2 978 |
| `CleanupDone` | 3 053 |
| the array pattern | **3 078** |

Three rows and not five, because the sample was taken three times. `Raise` and
`for`-`of` were each measured PER FILE over the whole corpus and not over this sample,
and inventing a row for them from the full-corpus delta would be a number nobody took.

And per file against a kept binary at every step, on both corpora, with the
denominator unmoved: `bench/` 345 → **356** of 397, `tests/` 14 323 → **15 170** of
17 194. **The LOST list is empty at each of the five.**

---

## The gathering half, and the word that grouped two different things

Two entry points already existed — `array_append` and `array_append_all`, both
answering the array so calls chain — so this half was declaring them in the
language's table and using them.

### A literal with a spread is BUILT, not counted

The refusal said *"the element count would stop being the count written"*. True,
and the count is not what `NewArray` has to be given: it needs the **elements**.
So one empty array and an append per element, in source order.

A literal with no spread is unchanged, and a test pins that it emits **no call at
all** — `[a, b, c]` must not start paying for a feature it does not use. Keeping
both shapes is not two answers to one question: it is one answer whose input
differs, and which one applies is settled by the syntax rather than guessed from
the graph.

A **hole** is still refused, now under its own name rather than sharing the
spread's. `[, 1]` has a hole some operations skip and others read as `undefined`,
and collapsing them loses that.

### A rest target is a LOOP, and the drain that looks right is not

`ArrayAppendAll` drains an iterable from the **start**; a rest target gathers what
this iterator has **left**, and the two differ by however many slots came before
it.

Handing the iterator to the drain would have compiled, and would be right for every
**built-in** iterator — those answer themselves from `Symbol.iterator`. A
hand-written one need not, and then the drain restarts the source or raises. That is
the same class of defect as reading `value` past `done`: **correct until someone
writes their own iterator**, which is the second time this block has met it.

So the loop is built, from the same step the slots use and one `ArrayAppend` per
pass. It carries one value across its back edge — the array — and it carries the
**append's answer** rather than the array it was handed. They are the same object
today and the entry point answers one deliberately, so passing the operand instead
would be reading a value whose definition does not dominate the next pass.

Gathering **cannot close**, and that is not an omission: it runs until the iterator
reports `done`, which is the one way out that owes nothing. A test pins it by
counting the nullish test — one for a pattern that stops early, zero for one with a
rest target.

### The error in the entry above, which was mine

A rest **parameter** was grouped with these. It does not belong: `function f(...r)`
gathers the call's **arguments**, not an iterable. It needs the argument count at
run time, which is a calling convention and therefore the machine's, and it is
refused as a function SHAPE — as it always was. The refusal was right and the
grouping was wrong.

22 of the sample sat under the wrong heading because two different things are both
spelled "rest". Worth recording as its own kind of mistake: every other correction
in this document is a refusal that turned out to be answerable, and this one is a
**work list that pointed at the wrong crate**. A plan can be wrong in that
direction too, and nothing measured would have caught it — the count was real, the
grouping was not.

### Where it stands

| measured after | `bench/` | `tests/`, every fifth file |
|---|---:|---:|
| the suspension | 347 of 397 | 2 978 of 3 473 |
| `CleanupDone` | 349 | 3 053 |
| the array pattern | 356 | 3 078 |
| the gathering half | **357** | **3 089** |

Per file against a kept binary at every step, both corpora, denominator unmoved,
**LOST empty at all six**.

---

## What is left of this stage, measured rather than estimated

Asked directly, and answered by searching rather than by recalling. **It is one thing,
and the other three are downstream of it.**

| missing | what the search found |
|---|---|
| **a producer at the machine boundary** | `MachineOps` had **zero** implementations outside a toy with one primitive in `rts-mir`'s own tests |
| **E2 on the path that runs** | `resolve_module` is reached only from `mir_dump`, which is only reached from `rts mir` |
| **a guard, therefore a second tier** | `Op::Guard` and `Terminator::Fall`: **zero** producers. Nothing asks for `Tier::Specialised` |
| **passes** | one, `refine_effects` |

So the stage lowered 15 223 of 17 194 functions to a graph and **no graph became
code**. That is a harsher reading than the share suggests, and it is the accurate one.

### The first producer, and the hole that explains why there had been none

`MachineOps::prim` received the machine values and nothing else, while `param_repr`
directly above it takes a `ValueId` and says the answer *"comes from what a pass proved
about the value"*. So a front end could set a parameter's representation from the
lattice and then have **no way** to lower `-` as a machine instruction, because it
could not ask what its operands were proved to be.

The type domain arrived at the boundary and was unusable there. That is a fair account
of why nothing had implemented the trait, and it is the kind of gap that does not
appear in any count: every test passed, the graph was well formed, and the one thing
missing was a parameter.

It now takes the whole `Inst` — one parameter instead of three, and the other two earn
their place: the result is what a representation is a fact *about*, and `at` is what a
fault record needs.

### Three things the machine and the lattice taught the file, none of them guessed

**The first draft lowered integer arithmetic and every arm of it was dead.**
`Subtract`, `Multiply`, `Divide` and `Remainder` answer `Double` whatever their
operands were, because the result may not fit in an `i32` — which `domain/tables.rs`
records about `+` and is true of all four. A guard on `result == Int32` is a condition
nothing satisfies. So the slice works in the double domain, and what found this was
reading the lattice rather than reasoning about it.

**`%` has no form here, and `arith` is what said so**: `UnsafeRemainder { found: F64 }`,
because `NumOp::Rem` is integer-domain only. Nor is that a gap in the machine —
JavaScript's `%` over doubles is `fmod`, which keeps the sign of the left operand, so a
float instruction that *did* exist would have been the wrong one to reach for.

**A text operand is refused at the CONSTANT, not at the operation**, the other way
round from what the test first asserted: the literal is its own instruction, lowered
before the subtraction that reads it. Recorded because the wrong guess is the natural
one — the interesting refusal is the one about the operand, and it is not the one a
reader gets.

### The bound it hits, which is the headline

**A parameter is `Anything`.** Rule 4 of `crates/rts-codegen/README.md` says a type
annotation is evidence and not proof, so `function f(a: number)` proves nothing about
`a`, and every primitive over one is refused at this boundary. A test pins that an
annotation and no annotation are refused **identically** — rule 4 as a test rather than
as prose, and a tripwire for the day something starts trusting a declaration the
language does not check.

What would turn `Anything` into `Int32` is a **guard**, and nothing emits one.

So the order of the remaining work is settled by this rather than chosen: **a guard
producer comes next**, because without it the boundary is correct and has nothing to
work on for any function that takes an argument. Then the second tier, which is what a
guard falls to. Then passes, including the array specialisation `lower/iterate.rs`
explicitly deferred to one.

### One more finding, about the tests rather than the code

The boundary test builds the signature from what the language answers — and it supplied
the RETURN half by hand at first. A comparison caught it in the same minute: `7 < 3`
answers a boolean against a declared `F64`, and the machine's verifier said
`ReturnRepr { expected: F64, found: Bool }`.

**A principle stated and half applied** is the shape of mistake this whole stage exists
against, and it turned up inside the file whose header states the principle. Worth
keeping for that reason alone.

Measured per file against a kept binary: `bench/` 357 of 397 and `tests/` 15 223 of
17 194, both **unchanged**, LOST empty. Expected, and stated rather than omitted: this
adds a consumer of the graph and changes nothing that produces one.

---

## A claim becomes a guard, and the section above was wrong about the rule

The entry before this one said *"a parameter is `Anything`"* and treated it as what
rule 4 requires. It is not. Rule 4 says it in one sentence and the second half is the
part that matters:

> An annotation is treated as a **claim**: it may be used to prove a representation
> where the language can check it, and **it becomes a guard where it cannot**.

The second half had no implementation, and the commit that reported the gap pinned it
as a test — *"an annotation changes nothing at this boundary"*. That is a **gap wearing
a rule's clothes**, and it is a failure mode worth naming on its own: a test that
asserts current behaviour reads exactly like a test that asserts intended behaviour,
and the only thing distinguishing them is whether someone checked the rule.

### What a guard makes true, and why it is not trusting the annotation

Nothing believes TypeScript. The annotation is not evidence that the value **is** a
number — it is evidence that **assuming so is worth a check**. The guard performs the
check, and after it the narrowing is a proof in the ordinary way: `Op::Guard`'s result
is the same value with a narrowed type, which is why `rts-mir` rule 6 makes a guard a
value in the dataflow rather than emission.

So rule 4's *"any place a claim becomes a proof must say what checked it"* is answered
**structurally**. What checked it is the instruction standing between them, and a
narrowed type with no guard above it is unrepresentable rather than discouraged.

### The measurable result is that the refusal changed sides

For one source, `function f(a: number, b: number) { return a - b; }`:

| tier | refused by | saying |
|---|---|---|
| generic | the **language** | `Subtract over operands that were not proved numeric` |
| specialised | the **machine** | `NeedsSideExit(PointId(0))` |

That is the whole proof the chain works — claim → guard → narrowed → an instruction
the language can emit — and it is a better proof than a passing lowering would have
been, because it says exactly which layer the work moved to. What remains is
`deopt-lateral.md` D3, named rather than described.

A test pins both halves over the same string, so the day one of them changes the
other is right there to compare against.

### `: number` asserts a DOUBLE

A JavaScript number is a double, and `Int32` is a subset the lattice tracks separately.
Asserting `IsInt32` from `: number` would be a check the annotation does not support —
`f(1.5)` is a perfectly good call — so it would fall on ordinary input and the
specialised tier would be dead weight.

A test pins that the guard's **result** is `Double` while the value it **guards** is
still `Anything`. That pair is what makes the guard the thing that changed the answer,
rather than the annotation.

### Only the specialised tier, and why the builder had to be asked

A guard needs somewhere to fall, and the generic tier is that somewhere — a guard there
is a check whose failure has no destination, which is what `guard.rs` says from its own
side. So the generic body emits none and is slower on purpose.

`FuncBuilder` gained `tier()` for it. The decision turns on the tier in a **different
crate** from the one that chose it, and a client keeping its own copy is two places for
one answer, whose drift is a guard with nowhere to fall.

### What produces no guard, and each is a different reason

| claim | why not |
|---|---|
| `boolean`, `undefined`, `null`, an object, an array | the assertion table has no row |
| a union | `Claim::is_definite` already said it: *"a claim that has to be examined before it answers is a claim that did not answer"* |
| `any`, `unknown` | `Claim::Unknown` names nothing to check |

The union is the interesting one. Guarding `number | string` needs two assertions and
two falls for one parameter, which is a **different shape** and not a bigger version of
this one. Rule 5 is satisfied by all of these being visible in `lower/claim.rs` rather
than three lowerings later.

### One numbering, not two

The deoptimisation points are counted on the lowering rather than per guard, because
the number has to be the **same in both tiers**: a fall from point three of the
specialised body lands at point three of the generic one. Two counters that happen to
agree is the shape of a bug that only appears once a fall actually fires.

### Where this leaves the order of work

The guard producer was named as next by the section above, and it is done. What is left
is unchanged in kind and shorter by one:

1. **the side exit** — `deopt-lateral.md` D3, which is what `NeedsSideExit` waits on
   and what makes a guard's failure survivable;
2. **E2 on the path that runs**, which is still reached only from `rts mir`;
3. **passes**, still one.

Measured per file against a kept binary: both corpora unchanged, LOST empty. `rts mir`
prints the generic tier, so nothing it surveys emits a guard — stated rather than left
as a surprise for whoever runs it next.

---

## The side exit, and what TypeScript is actually for here

`function f(a: number, b: number) { return a - b; }`, in the specialised tier:

```text
block0(v0: Tagged, v1: Tagged)   Guard v0 expect F64   -> block1 / block2
block1(v2: F64)                  Guard v1 expect F64   -> block3 / block4
block2                           Call generic(v0, v1); Return
block3(v4: F64)                  FloatArith(Sub, v2, v4); Return
block4                           Call generic(v0, v1); Return
```

Two guards, two side exits, one native instruction, and the machine's own verifier
accepts it. **That is the sentence the four stages exist for**, and this is the first
time anything in this document could print it.

It also settles what the annotations are for. They are not a type system this compiler
enforces and they are not documentation — they are **where the speculation comes from**.
`: number` says which assumption is worth a check; the guard makes it sound; the fall
makes it survivable. Nothing in the chain believes TypeScript at any point, and the
un-annotated form of the same function still reaches no instruction.

### Why it is buildable now and was not, and it is one observation

A guard in the **entry block with nothing but guards before it** has no local state
behind it. So the live set *is* the parameters, and falling from there needs no frame
reconstructed — it needs the same arguments handed to the other tier.

That is the whole of it. The rest of `deopt-lateral.md` D3 — reconstructing a frame for
a guard in the middle of a body — is still absent and still refused by the same name it
always was.

**The condition is checked structurally, by position.** `rts-mir` has no liveness pass,
and a condition it can check *exactly* is worth more than one it would approximate. Both
halves are pinned: a guard at the entry lowers, and the same guard with one ordinary
instruction in front of it is refused.

### Two questions the boundary grew, and why each belongs to the language

`asserted_repr` — what an assertion narrows to in machine terms. `IsStr` answers
**None**, because a string is a reference to a heap value whose layout is the runtime's
shape tree, and inventing a `Ref` of something would be a guard that narrowed to the
wrong thing **and passed**.

`fall` — what happens when a guard fails. Where a fall lands is an arrangement between
two bodies of one function, and which two those are is not something a graph knows.
`rts-host` is the crate that may name all three layers, so it is where the pairing is
agreed — exactly as the entry-point symbols and the singleton numbering are.

### The property the whole arrangement rests on

**Every fall hands over the original arguments, never the narrowed ones.** A test pins
it across both falls, *including the second* — where the first guard had held and a
narrowed `a` existed.

The generic body is reached precisely when a speculation did **not** hold. Handing it
the value the failed guard claimed to have produced would pass on the very thing that
was wrong, and it would be subtly wrong in the way nothing else catches: the types
line up, the verifier is content, and the program answers using a value that was never
established.

A tail call would save the frame and is not available — `unwind`'s header says a call in
tail position discards its frame before control transfers, so it cannot also be the call
a handler is installed around. So the fall calls and returns, costing one frame on the
path taken when a speculation failed, which is the path whose cost the arrangement is
willing to pay.

### A justification that stopped being true, kept because that is the lesson

The boundary test built its registries fresh wherever one was needed, on an argument
written into the file: `lower` takes them immutably, so nothing it does can add a row,
and two empty registries built the same way are the same registry.

Declaring the generic twin **mutates** the function registry. So the verifier read a
fresh one, found no such callee, and said `UnknownCallee` — the right answer to the
wrong registry. The argument was sound when written and false two commits later, and
nothing about it looked stale.

**A justification is only as good as the last thing that changed under it.** That is
the third time this campaign has produced a variant of the same finding, after a test
that pinned a gap as a rule and a work list that pointed at the wrong crate.

### What is left

1. **frame reconstruction** — a guard that is not at the entry, which is the remainder
   of D3;
2. **E2 on the path that runs**, still reached only from `rts mir`;
3. **passes**, still one — and `rts mir` surveys the generic tier, so nothing it prints
   shows a guard. Worth fixing next if only so the work is visible to the instrument
   that measures it.

Measured per file against a kept binary at every step: both corpora unchanged by this
commit, LOST empty.

---

## How far from running anything: 2.8%, and the road is now known precisely

Asked directly — is the new stage at or above what `main` passes? — and the answer needed
an instrument that did not exist, because the number being quoted was the wrong one.

**Two shares, and they had been read as one.**

| | over `bench/` | what it means |
|---|---:|---|
| functions that produce a GRAPH | 88.5% | it lowered |
| functions that reach the MACHINE | **2.8%** | it becomes code |

`rts mir --machine` is the second one, and it is part of this work rather than a script:
an instrument that can only be run by `cargo test` is an instrument nobody runs. Over
`tests/` at one file in twenty it is 4.3%.

**And `main`'s 833 of 888 does not pass through any of this.** That number is produced
entirely by the old `emit/` path, which this branch does not change. The new stage has no
pass rate on the suite because no `*.test.ts` file executes through it. Quoting 88.5%
beside 94% would have compared two rulers and overstated the stage by more than an order
of magnitude.

### Two defects the instrument found immediately, and every test passed with both

**`Truthy` over a proved boolean was refused.** It is the identity — this language's truth
rule applied to something already a truth value asks nothing — and the slice turned it
away for failing a numeric test it never had. `truthy` is the second most common operation
in `bench/` at 362, because every `if (a < b)` writes one over a comparison's boolean, so
every branch in the corpus stopped at its condition.

**An edge carried a representation the target did not declare**, and that one was mine. A
block parameter's representation comes from what a pass PROVED; an argument's comes from
whatever instruction produced it. `let x = n; if (c) { x = 2; }` joins a guarded double
with an integer literal: the lattice proves `Double` and the literal arrives as an
integer. The machine refused and was right to — changing a representation is never
implicit there. 133 of roughly 350 refusals over `bench/` were that one error.

`MachineOps::coerce` is the question that was missing, and it belongs to the language
because which conversions are sound is a fact about its lattice. The refusing half is the
one worth stating: tagged into anything is a NARROWING, and a coercion that performed it
would be a guard nobody wrote and nobody checks.

**Neither fix moved the count**, and that is stated rather than dressed up: 11 of 398
before and after. What moved is which wall is in front — and knowing that is the point of
measuring rather than estimating.

### What is in front now, in order, and why the order is not a preference

The wall is `LessThan over operands that were not proved numeric: v4 is Double, v0 is
Anything` — 119 of them. `v0` is an **unannotated parameter**, and `bench/` has 75
annotations against `tests/`'s 2107 because its files run unmodified under Node and Bun,
so they can carry no types at all.

The obvious answer is to speculate on the OPERATION rather than on the annotation: assume
`<` has numeric operands, guard, fall if not. That is what a real engine does, and it is
**blocked by something else**: such a guard is mid-body, and `lower` refuses any guard that
is not at the entry because the live set there is not the parameters. So operation-level
speculation waits on frame reconstruction, which is the rest of D3.

That leaves everything else, and every remaining item is an agreement somebody else has to
state:

| stopped by | count | whose |
|---|---:|---|
| a declared constant needs the runtime's numbering | 37 `bench/`, 195 `tests/` | the **host**'s: where a text, a key or a binding lives |
| `ThisValue` | 18 | the **machine**'s receiver convention |
| `NewArray` over N operands | 19 | the **host**'s or the machine's: an array's layout |
| `IsStr` narrows to no representation | 9 `bench/`, 48 `tests/` | the **machine**'s: a string is a reference whose layout is the shape tree |
| `Add` has no machine form | 31 | **decided**, and deliberately: over anything but two numbers it may concatenate |

**So the language side of this slice is done.** That is the real finding, and it is more
useful than the percentage: raising the machine share from here is not more lowering
work — it is frame reconstruction, plus the host stating where an entry point lives.

### A refusal whose name is wrong, and it is worth correcting the expectation

`NeedsHandlerTag` reads as though the language owes a tag. It does not owe much: JavaScript
has exactly ONE tag, because one `catch` catches everything, so the declaration is
`Tag(0)` and a line of code.

What is actually missing is that `lower` creates **every block before any region is
opened**, and a block's region is fixed at creation — `FuncBuilder::create_block` assigns
whatever region is open, which is deliberate and documented on it. So lowering a region
means interleaving `open_region`/`close_region` with block creation, walking the region
tree rather than the block list.

Recorded rather than done, because it is a restructuring of `lower`'s two-pass block
creation and this section would otherwise claim a piece that is not finished. The name
will keep suggesting a one-line fix to whoever reads it next; it is not one.

## The environment: 645 → 827 of 8 985, and the wall behind it is one piece

Measured 2026-09-24 over **every** file of `tests/` and `bench/`: 931 files, 8 985
functions, `mir_dump::describe_machine` on each, compared per function against a binary of
the tree before the change. **182 gained, none lost.** `tests/` goes 626 → 806 of 8 598,
`bench/` 19 → 21 of 387. The sampled figures above (one file in twenty) are not comparable
with these and are not replaced by them.

**What stood in front was not a machine question, and it was not only a boundary one.**
The refusal said a captured binding needed "the environment layout -- how many
`__rts_outer` links to walk -- which is escape analysis", and that was true of the reader.
The declarer was worse off and nothing said so: it kept the captured local in SSA while the
closure asked for it somewhere else. Both sides compiled. So this is a soundness fix on one
end and a lowering on the other.

- **E2 answers capture.** `names::resolve::captured` records every use with the scope it was
  written in and resolves them once the tree is complete -- a use can reach a `var` declared
  further down. From that: which bindings are captured, which activations build an
  environment, which ones reach past themselves, and how many links separate a use from the
  owner. The running engine answers the same question in `emit/escape.rs`, by spelling.
- **The graph says the layout.** `OuterRead` / `OuterWrite` named a binding and left where it
  lived to the machine; they are gone, with `JsConst::Binding`. In their place five
  operations over an ordinary object: `EnclosingEnvironment` (parameter 0 of the convention),
  `EnvNew`, `EnvOuter`, `EnvRead`, `EnvWrite`. A `MakeClosure` now carries the environment it
  closes over as its second operand, so the one thing a closure is made of that was nowhere
  in the graph is in it.
- **The boundary lowers all five** through what it already had: the cached read `o.x` uses,
  and a cached define whose slow path is `DefineField` rather than `[[Set]]` -- a binding
  spelled like an `Object.prototype` accessor must land as data. Every key is defined at
  creation, to `undefined`, so a read never falls through to the prototype chain.

**Refused by name rather than folded**, each a different missing piece: a captured binding
of a scope that is fresh per loop pass (15 functions -- the per-iteration environment the
running engine builds); two captured bindings of one activation with one spelling; a
function expression's own name, captured. And one gap stated rather than hidden: a `let`
read by a closure before its declaration runs answers `undefined` where the language throws.

**What is in front now is one piece, counted as one.** Of the 4 745 functions the capture
stopped, 3 564 now stop at `NeedsCallee`, and every builder of an environment stops at
making its closure -- "a function value needs the machine id of fN". Both are the same
missing thing: the boundary compiles one function at a time, so no other function of the
module has a machine id. The message for the second was rewritten to say so, because
counted apart they read as two pieces of work.

## The plain call: 643 → 4 210 of 8 985, and the instrument now asks the verifier

**`NeedsCallee` was two refusals under one name, and the larger one needed nothing.**
`rts_mir::lower` refused a call to a numbered function AND a call through a value with no
receiver, while `MachineOps::call_value` had taken its receiver as optional from the day it
was written. So `f(x)` over a value -- the most common call there is -- stood behind a
function registry it never asked for. Routed to `call_value` with `None`, it takes the same
door a method call takes, `RuntimeOp::Call` with the receiver `undefined`.

**The instrument did not ask the machine's verifier, and it does now.** `reaches_machine`
counted a function as reaching the machine when the builder accepted every instruction, and a
builder accepting instructions one by one is not the verifier accepting the function. Asked,
it refused two functions the old count had included -- so the base is re-measured under the
stricter ruler, 645 → 643, and every number in this section is the verifier's. Both were one
defect: a body returning a value on one path and falling off the end on another produced a
`ret` with nothing, against a signature of one. `return;` and falling off the end now answer
`undefined` in the graph, because every function of this language returns a value and that
is the language's to say.

Measured 2026-09-24, every file of `tests/` and `bench/`, per function against the tree
before the environment work, both sides verified: **643 → 4 210, none lost.** `tests/` 624 →
4 188 of 8 598; `bench/` 19 → 22 of 387.

**What the number does not say.** No `*.test.ts` file executes through this stage yet, so
4 210 is how many functions become code the machine accepts, not how many answer correctly.
`NeedsCallee` still refuses 203 calls to a function of the module by number, which the
module's machine numbering is what unlocks.

**Two wrong answers the count was hiding, found by reading what reached it, and refused
now.** An arrow's `this` read the receiver the arrow was CALLED with, where the language
fixes it where the arrow was written; and `arguments`, declared by no scope, was read
through the global object. Each compiled, verified, and would have answered wrongly. Both
are refused by name, and that is a REGRESSION in the count, stated rather than netted: 4 210
→ 4 189, the 21 being exactly 17 `arguments` and 4 arrow `this` -- and 20 of them were
counted as reaching the machine in the base. They were wrong answers wearing a pass; the
LOST list against the base is those 20 and nothing else. A third suspicion was checked and is not one: a function reads `this`
as the raw receiver with no global substitution, which is what the running engine does
too -- it compiles a program strict and only `eval` and `Function` text sloppy.

## The convention and the module's numbering: 4 189 → 5 384

**A closure is a code address, and an address exists only for a function the machine's
registry declared.** So every builder of an environment stopped at making its closure,
~1 000 functions. `Shared::number_module` declares one machine function per function of the
module before any is lowered, and a function value becomes `func_addr` handed to
`ClosureNew` beside the environment -- the call `emit/function.rs` makes.

**Numbering first was only possible because the signature stopped depending on the body.**
`reaches_machine` declared each function from what the lattice proved about its parameters
and its return, which is a signature nothing in this engine can call: the runtime enters a
function through `invoke`, on the terms `emit/function.rs` states -- environment, receiver,
four argument slots, one result, all tagged. A function declared any other way cannot be the
code of a closure, so the instrument had been measuring functions that could never run. It
is `emit::convention()` now, reused rather than restated, and a proved return is widened on
the way out through a new question on the boundary, `MachineOps::returned`, whose default
hands the value back unchanged -- the toy domain's answer. A fifth parameter is refused, as
`emit/function.rs` refuses it.

Measured 2026-09-24 per function against the previous commit: **4 189 → 5 384, none lost.**
`tests/` 5 363 of 8 598, `bench/` 21 of 387.

**What stays refused at the call, deliberately.** 377 calls name a function of the module
by number, and a direct machine call is not what the running engine makes -- every call
there goes through `invoke`, which keeps the call stack a trace is built from and the
argument count a callee can observe. Calling past it would be faster and would answer
`new Error().stack` differently. And the lowering names a callee by number without asking
whether its binding is ever reassigned, which would call the old function after
`f = other`. Both are reasons to keep `NeedsCallee` until something proves the binding and
decides the trace, and neither is a reason to guess.

## The generic arm: 5 384 → 7 218, and three things the lattice or the table had wrong

**An operation over values nothing proved is a CALL**, which is `README.md` rule 5 and what
`emit/expr.rs` has always done: `a + b` over tagged values is the runtime's `Add`, because
which of concatenation and arithmetic it is depends on the operands at run time.
`machine/generic.rs` names, per row, the `RuntimeOp` the running engine calls for the same
operator -- so the runtime's implementation stays the one definition. Proved numbers still
take the instruction, and that set grew: `+`, `==`, and `-x` (the sign bit, which `0 - x`
is not, for `-0`), with `%` as the unboxed `NumberRemainder` rather than the integer `Rem`
the machine had refused.

**`+` over two proved numbers was refused on purpose, and that decision is reversed.** The
concern was lowering it "beside the four on the strength of the operands looking alike",
and it was right about unproved operands. Behind the `all_numeric` gate nothing looks like
anything -- both operands were proved by a literal or a guard -- so the refusal was one gate
too wide. An unproved `+` still cannot reach the instruction; the boundary test pins both
halves.

Three things were wrong before any of this could be sound, and each was invisible while the
boundary refused the operations they described:

- **The lattice ignored BigInt.** Every ToNumeric row answered `Double` whatever its
  operands, and `10n - 1n` is `9n`. It answers a number now only where one operand rules a
  BigInt out -- mixing the two kinds throws, so `x - 1` and `x | 0` keep their proofs --
  and `Anything` over two unknowns.
- **`FieldWrite` typed its answer as the KEY**, the second operand, so `(o.x = 5) + 1` read as
  a concatenation.
- **`Compare` was three operators under one row**, with a note saying which one was "the
  machine lowering's question" -- which the machine lowering could not answer, because the
  graph did not say. All 79 were over proved numbers. It is three rows.

Measured 2026-09-24 per function against the previous commit: **5 384 → 7 218, none lost.**
`tests/` 7 027 of 8 598; `bench/` 191 of 387, from 21.

Still refused by the generic arm, each for a stated reason in `generic.rs`: `BitAnd`
(five operators under one row, the fault `Compare` had), and `ToNumber` over an unproved
value (`i++` applies ToNumeric, and the call that exists, `UnaryPlus`, throws on a BigInt).

## The protected region: 7 218 → 7 292, and the order that was the whole problem

The entry above called `NeedsHandlerTag` a name that "will keep suggesting a one-line fix
to whoever reads it next". It was right. The tag was a line -- `MachineOps::exception_tag`,
answered with the one tag the running engine throws and catches with -- and the rest was
ORDER: the machine places a block in a region when the block is made, and `rts_mir::lower`
made every block before opening any region.

- **`rts_mir::lower::regions` makes the machine's blocks in the region tree's order**: a
  level's own blocks first (a region's handler and cleanup live outside it, and the machine
  requires them made before it opens), then each region inside, opened around the blocks it
  holds. The machine's `open_region` also places the block being BUILT, so each region
  opens on an anchor -- its first block, counting the regions inside it -- which is what
  leaves the first block of `try { try { … } }` in the inner one.
- **A block made while emitting has to inherit its region, and the machine now derives
  it.** Every continuation the boundary makes -- the halves of a cached read, the path past
  a raising call -- was made with no region open and so belonged to NOTHING: a throw from
  one would have left the function past the handler written to catch it, and the verifier
  has no way to know that was not meant. `FuncBuilder::inherit_block_regions` is a mode, not
  a parameter, for rts-cranelift's rule 8; its invariant test shows both halves.
- **The re-raise block is one per region**, as its own note said it would have to become.
- **A call inside a `finally` takes no re-raise check**, which is `emit/expr.rs`'s decision
  for the same reason: the verifier refuses a cleanup that leaves by a throw. The gap is the
  running engine's too, and stated there.

`rts-mir/src/lower.rs` was already past that crate's ceiling of 500 lines; it is a folder
now, `mod.rs` the walk, `ops.rs` the questions a language answers, `regions.rs` the order.

Measured 2026-09-24 per function against the previous commit: **7 218 → 7 292, none lost**,
and `NeedsHandlerTag` is gone from the list. The boundary test asks the machine to plan
every throw inside a `try` and checks none of them leaves the function.

## The language's own keys, `== null` and `!`: 7 292 → 7 388

**A key the language fixed was refused as needing "the runtime's numbering", and it needed
none.** `for`-`of` reads `@@iterator`, `next`, `done`, `value` and `return`, and a class
writes `prototype` -- keys the program never spelled, held as `JsConst::WellKnown`. A key is
a number from the one registry every key comes from, and a symbol key is an interned name
in a space no program can write, which is `rts_core::entry::symbol`'s design. So
`WellKnown::spelled` is the one place the six spellings live, and `key_of_constant` is the
one place a constant becomes a `shape::Key` -- the operand and the cached access recover it
the same way.

That moved 50 functions to the wall behind it, `IsNullish` -- the `x == null` a loop asks
before it closes an iterator -- and `Not`. Both are branches over what the machine already
answers, which is `emit/choice.rs`'s shape: two singleton tests and a join, and a branch on
the truth value. Over a proved operand `x == null` is the constant `false`, answered here
because the machine refuses the question rather than answering a constant.

Measured 2026-09-24 per function against the previous commit: **7 292 → 7 388, none lost.**
`tests/` 7 188 of 8 598, `bench/` 200 of 387.

## The template literal: 7 388 → 7 467

A template is the first text, then each substitution converted to a string and joined, then
the text after it. The conversion is `RuntimeOp::StringOf` -- ToString, the STRING hint --
and not the `+` that joins, because `+` asks `valueOf` first and a substitution asks
`toString` first: `emit/template.rs` recorded that no spelling of `+` repairs it. It is the
shape that file itself falls back to; `TemplateJoin` is its faster form over declared sites
and stays a pass's to choose.

Measured 2026-09-24 per function against the previous commit: **7 388 → 7 467, none lost.**
`tests/` 7 263 of 8 598, `bench/` 204 of 387.

### What stands in front now

Counted over the same 931 files, and each is a different piece:

| stopped by | functions | what it is |
|---|---:|---|
| `NeedsCallee` | 504 | a call to a function of the module by NUMBER -- kept refused on purpose: the running engine never calls past `invoke`, which keeps the trace and the argument count, and the lowering does not prove the callee's binding is never reassigned |
| `NeedsFrameTransform` | 173 | a generator or `async` body: `Op::Suspend` to `into.suspend()`, the host's `frame::resumable_form`, and the wrapper that answers a generator object or a promise |
| `NeedsSideExit` | ~210 | a guard behind something observable: frame reconstruction, the rest of D3 |
| the lowering's own refusals | ~550 | a callee that is neither a name nor a property, an object literal with a method, `yield*`, parameter defaults, `super` calls, rest parameters, destructuring targets, `arguments` -- each a named line of `rts mir` |

## The stage RUNS: the suite, per file, with the door open

Everything above measured what COMPILES. `emit/through_mir.rs` is where a function of a
running program is compiled through this stage instead of the running emitter, and it is
on by default; `RTS_MIR=0` shuts it for measuring the running emitter alone on the same
binary, and `RTS_MIR_TRACE=why` names every function taken and every reason one was not.

Measured 2026-09-24 on this container (4 cores), `--profile fast`, one process per file,
`target/debug/rts` moved aside -- six `*_err.test.ts` files spawn it when it exists and
compare a different binary than the one under test:

| binary | passes |
|---|---:|
| `origin/main` | 880 of 905 |
| this branch, door SHUT | 883 of 907 |
| this branch, door OPEN | **883 of 907** |

LOST with the door open, against `main` and against the same binary with it shut: **empty**.
The two gained over `main` are this branch's own, not the door's.

**What running found that compiling never could**, each a wrong answer or a crash on a
function every static check accepted -- the verifier included:

- the machine lowered handler and cleanup blocks in creation order, because a throw has no
  successor; exceptional edges are successors in the ordering now;
- `if`, `try`, `switch` and every loop handed the next statement the map ONE path left;
  `settle_after` is one rule for all of them;
- `var x;` rebound `x` to `undefined`;
- an array rest took one element too few -- the tree's trailing placeholder;
- `try/catch/finally` ran the `finally` before the `catch`, and skipped it on the ordinary
  way out;
- deep recursion overflowed without `TailCall`, and `return c ? f() : g()` hid its calls
  behind a join;
- `Number.isFinite(42)` was `false`, and `new Date(0)`, `BigInt(1)` and numeric `Intl`
  options misread an integer, because the runtime read numbers with `as_f64`, which knows
  one of the two encodings. The running emitter widens integers too, so this was a runtime
  defect the stage exposed rather than one it caused.

**How much of the program this is.** Over the suite, a first count had 2 830 functions
taken and 7 454 declined -- and the largest reason was an instrument error of mine: the
scope tree was built without the IMPORTS, so every imported name (`expect` alone, 3 822)
read as an unplaced global. With them, the first 60 files take 292 and decline 117. What
declines most now is a function that makes a closure -- `test(() => …)` inside
`describe(() => …)` -- because a closure made here would be laid out by this stage and read
by the running emitter's code.

### Closures made here, and what the Rust tests saw that the suite did not

A function this door takes now MAKES closures: the functions written directly inside are
emitted by the running emitter, in the scope the taken function sits in -- which is what
they reach, since a taken function builds no environment of its own -- and the closure is
made here from the id that answers. A class inside, or an arrow reading the enclosing
`this`, still declines. Over the first 60 files that moved the count from 292 taken to 363.

Same suite, same conditions as above: **883 of 907, LOST empty** against `main` and against
the door shut. But the suite was the weaker ruler this time. `cargo test -p rts-host` had
seven failures only with the door open, and three of them are the kind a green suite
cannot see:

- **a disabled optimisation.** The generic tier answered `5 + i` with a call where the
  running engine guards `i` and adds; `literal_guard_gate.rs` counts the guards and read
  zero. `machine/guarded.rs` is the running engine's shape at the boundary -- guard, the
  instruction, and the very call the generic arm makes on the failing edge -- which is a
  LOWERING rather than a speculation, since nothing is reconstructed when the guess is
  wrong. `% 2^k` asks the machine for its exact sequence first, as `remainder.rs` pins;
- **an encoding.** A literal the lattice proved `Int32` left the proved domain boxed under
  `TAG_INT32`, where the running engine boxes every literal as a double. Both are legal
  words; every native still reading `as_f64` is not ready for the first. `coerced` goes
  through the double now, so a number leaves this stage in the running engine's encoding;
- **a name.** `const f = () => …` names the arrow `f`, and the running emitter carries that
  from the declaration to the function. The door emitted the body without it, so `f.name`
  was `""`. It carries the names of the three sites a taken function can hold.

And one defect of the scope tree itself, which both stages read: a declaring pattern's
DEFAULTS were never walked, so `function inner({ a = seen })` recorded no use of `seen`
and the capture analysis called it a register. The running emitter was never affected --
it answers capture with its own walk -- which is why only the door found it.

`rts ir` did not open the door at all: the graph path, `emit_modules`, never built the scope
tree, so the one command whose job is to show what runs showed the other stage's output.
It builds one per unit now.

### An unplaced global, and an environment built here: 83% → 90% of the suite's functions

Counted over every `*.test.ts` with `RTS_MIR_TRACE=why`: 8 534 functions taken and 1 779
declined before these two steps; **9 342 and 1 003** after.

- **A name nothing placed** -- `dom`, `DomTimers`, `DomScope`, `engine`, which the host
  installs where no compile-time list sees them -- was 640 of the refusals. The running
  emitter reads such a name with `UnboundGlobalGet`, which raises `ReferenceError` when it
  RUNS; the boundary now does the same for exactly those reads. A page script, whose
  sibling scripts write its window, and a read under `typeof`, which the language exempts
  from the error, still decline.
- **A function that builds its own environment** was 308. The environment built here is
  the running emitter's shape exactly -- an object, a slot per spelling, `__rts_outer` --
  so a function the running emitter compiles inside this one reads it through one more
  layer of ITS scope at hops zero, built from the same `environment_of` list the
  lowering builds the object from. The one thing it cost: a read of a binding the
  ENCLOSING layout holds counts from the environment this function was made in, and one
  built here stands one link in front of it -- 21 files failed until that link was
  walked.

Suite, same conditions as above: **884 of 907**, LOST empty against `main` and against the
step before. The one gained, `closure-capture-loop-shadow.test.ts`, is a block-scoped
binding captured by a closure that outlives the block, which the running emitter reads
from the wrong object -- `function f(a) { let x = a; { let y = 5; var g = () => x + y; }
return g(); }` throws `ReferenceError: y` with the door shut, and answers what Node does
with it open.

### Past the four slots, and a fixed point that was not one: 90% → 93%

9 636 taken and 705 declined over the suite, from 9 342 and 1 003.

- **More than four of anything.** A call of five arguments goes through `CallWithArgs`
  over an array built by `ArrayOf` and `ArrayAppend`, an array literal of five elements is
  that same array, and a function declaring five parameters or `...rest` reads them from
  `RestArguments` -- each the running emitter's shape for the same code. What the graph
  needed for the last one is all four slots whether or not the program named them, so a
  function that gathers declares every slot as an entry parameter; and a count the
  compiler fixes is `JsConst::Count`, a machine word, never a number of the language.
- **`rts_mir::infer` answered a type that was not a fixed point.** A change re-queued the
  successors of the block it happened in, and nothing that READ the value: a loop exit
  reached through a block that only forwarded the counter never looked again, and kept the
  entry's `Int32` for a value the back edge had made a `Double`. The machine was then asked
  to narrow a double to an integer -- 26 functions refused, and the lattice was WRONG
  rather than coarse, which is a wrong answer the day a pass trusts it. Every block reading
  a value that moved is re-queued now; `tests/toy_domain.rs` pins the graph in the toy
  language, and fails without the fix.

Suite: 884 of 907, LOST empty against `main` and the step before.

### Any callee, the comma, and a proved number held tagged: 93% → 94%

9 737 taken and 604 declined, from 9 636 and 705.

- `o[k]()` is a method call with `o` as its receiver, and any other callee -- `f()()`,
  `(a || b)(x)`, `(0, o.m)()` -- is a value called with none.
- `a, b` evaluates both in order and answers the last.
- **A value the lattice proved a number and the machine held tagged.** `x * 2` rules a
  BigInt out, so the lattice answers `Double` whatever `x` is; the guarded lowering joins
  the instruction's double with the runtime call's tagged word, so the representation did
  not follow the proof, and every consumer wanting the double refused. `as_double` now
  unboxes such a word from either of `rts-core`'s two number encodings, and TRAPS on a word
  that is neither -- which would mean the lattice was wrong, and a crash names that where a
  garbage double would not.

Suite: 884 of 907, LOST empty against `main` and the step before.

**A test target that does not settle, and it is not this work's.** `rts-host`'s
`node_modules` target timed out in two of five runs with the door OPEN and in two of five
with it SHUT, on the same binary; every one of its 51 tests passes run alone. On `main` the
same target ABORTS in six runs of six. Recorded rather than retried until green.

### Generators and async functions run through the stage: 94% → 96%

9 962 taken and 378 declined, from 9 737 and 604. The 196 generators and 87 async
functions were the largest refusal left.

- **The suspension is neutral, what is handed out is not.** `rts_mir::lower` emits the
  machine's own `suspend`, and asks the language first through `MachineOps::hand_out`;
  this language calls `GeneratorYield`, which is what `emit/expr.rs` does for a `yield` and
  for a parking `await`. The default still refuses as `NeedsFrameTransform`, so a language
  that has not said stays refused by name. The function is declared as one that may
  suspend, and gets no tail calls -- the frame is the generator's or the promise's, which
  `emit/tail.rs::permitted` refuses for the same reason. Only the BODY comes from here:
  the wrappers and `frame::resumable_form` are the running emitter's, unchanged. An async
  GENERATOR still declines, because its `await` drains where its `yield` parks and one
  `Op::Suspend` cannot say which it was.
- **`frame::resumable_form` walked blocks in creation order**, the same fault the
  lowering had, one stage later: it reads every value from the map its definition filled,
  and a block made before the ones feeding it panicked with "no entry found for key".
  `Function::control_order` is now the one ordering both use, moved out of `lower/body.rs`.
- **`continue` in a `for` skipped the update.** The MIR lowering sent it to the test, so a
  pass cut short never incremented and the loop never ended. It had been wrong since the
  loop was first lowered, and only showed once async functions with such loops came
  through; the update has a block of its own now, which `continue` reaches.

Suite: 884 of 907, LOST empty against `main` and the step before.

**Every Rust test of the four engine crates passes now, 1 088 of them**, and the two that
did not were not this stage's:

- `node_modules` timed out in about a third of the runs, with the door open or shut. Two
  real defects under it: `node:http2`'s pump could deliver an accepted session's request
  before the session had an object, so `'stream'` went nowhere and the peer waited for
  ever; and `node:tls` never relayed the inner socket's `'error'`, so a refused connection
  ended the process even when the program listened on the `TLSSocket` -- Node emits it
  there. The test that connects to a closed port now listens for it, as Node requires of
  that program. 15 runs of 15 pass.
- `aot_manifest_embedded` named the executable `.exe` and the object `.obj`, which is
  Windows; everywhere else it failed at "did not produce" beside the file it wanted.

### Parameter lists: defaults, patterns, and `+x` over anything

10 016 taken and 324 declined, from 9 962 and 378.

- **A default** is lowered as the language defines it, `if (p === void 0) p = default;`,
  through the statements that already exist -- after the environment opens, in order, so
  a default may read a captured binding and the parameters before it. `void 0` because
  `undefined` is a name a program may bind. A default holding a function still declines:
  it sits outside the body, so nothing numbered it.
- **A destructured parameter** arrives as one value and is taken apart there too, in the
  same order. With a default of its own, or past the four slots, it still declines.
- **`ToNumber` over an unknown value** is `UnaryPlus`, which is exactly what
  `emit/unary.rs::step_value` calls for `x++`. It throws on a BigInt where the language
  steps one; the running engine does the same, and this door answers what that engine
  answers rather than a different language.

Suite: 884 of 907, LOST empty against `main` and the step before.

### `arguments`, `yield*`, and a runtime bug both engines shared: 97.4%

10 079 taken and 268 declined, from 10 016 and 324.

- **`arguments`** is built from the four slots by `ArgumentsObject` where a function that
  has one mentions it, the test `emit/function.rs` makes. An arrow's is its enclosing
  function's, and still declines.
- **`yield*`** is a loop in the graph, path for path what `emit/delegate.rs` emits: the
  source's iterator where it declares one, then either the protocol -- `DelegateStep`
  until `done`, sending each resumption's value into the next step, the finished step's
  `value` answering the expression -- or what `Iterate` materialises. Forwarding
  `outer.throw(e)` and `outer.return(v)` stays the runtime's, as it was.
- **A raise from the delegated iterator escaped the `yield*`.** Comparing the two engines
  on `yield*` showed they agreed on a wrong answer: when the inner iterator's own `throw`
  or `return` raised, `rts-core` ended the outer generator and let the error reach the
  caller of `outer.throw(e)`, past the `try` the outer body had written around the
  `yield*`. Node and Bun answer from the `catch`. It is raised at the `yield*` now, the
  path an inner iterator with no `throw` already took, and
  `tests/generator_delegate_raise_at_yield.test.ts` pins both halves -- it fails on the
  binary before the fix.

Suite: **885 of 908**, LOST empty against `main` and the step before; the one more file is
that test.

### `**` and bigint literals

- **`a ** b`** has a row of its own, `JsPrim::Exponent`: numeric on `-`'s terms in the
  lattice, `NumberExponent` over two doubles -- the unboxed call the running engine makes,
  there being no instruction for `powf` -- behind the same guard as the others, and the
  runtime's `Exponent` otherwise.
- **A bigint literal** is `BigIntNew` over its digits, the running engine's one path for
  `1n` and `BigInt("1")`. The lattice has no bigint, so every operator over one is the
  runtime's, which is what the running engine does too.

`>>>` is the operator still without a row: its answer is `ToUint32`, which an `Int32`
cannot hold. The tests that used `**` as their example of a missing row use it now.

### A class written inside, as the running emitter's own: 98.0%

10 141 taken and 210 declined, from 10 079 and 268.

**The class is not lowered here.** `lower/class.rs` builds one from a closure and property
writes, which the stage can reason about and which is not the object a program observes:
methods non-enumerable with a home object, a constructor that refuses a call without `new`,
fields run in the constructor, `extends` linking two chains. So under the door a class is
`(function () { return class … })()`: a helper the running emitter compiles, numbered at the
class's position, called with this activation's receiver -- which is what its `extends`
expression and computed keys read. Inside an arrow the helper would need the enclosing
`this`, so a class there declines only when those two actually read it.

Two things the scope tree had to learn for it, both in the over-reporting direction
`captured.rs` calls safe:

- a class's `extends` expression and computed keys count as reads from the class's own
  activation, because that is where the helper evaluates them. The computed keys were not
  read at all before -- a key naming a local recorded no use of it;
- the bindings inside a class body -- its own name, its static blocks' -- are the helper's
  to lay out, so the environment built here leaves them out.

And the lattice learned a bigint: `Type::BigInt`, from `BigIntNew`, joins nothing but
itself. Typed `Anything`, `1n + i` guarded `i` where the running engine does not, and
`literal_guard_gate.rs` counted the guard.

### Object literals the stage does not build, by the same helper: 98.3%

10 171 taken and 181 declined, from 10 141 and 210.

A literal with a method, an accessor, a spread, a computed key or a prototype is the
running emitter's too, in a helper: `lower::built_elsewhere` is the one answer to "does this
stage build it", asked by the scope tree, the door and the lowering alike. What the
literal's values read is read from the helper's activation, so the scope tree counts it
that way. Three cases still decline: a literal that suspends, since a suspension cannot
move into another function; one whose values read `arguments`, `super` or `new.target`,
which the helper would answer with its own; and, inside an arrow, one that reads `this`.

### Writes to globals, and two block-scope defects found on the way: 98.4%

10 191 taken and 167 declined, from 10 171 and 181.

- **A write to a global** is `GlobalSet` under the door -- the running emitter's
  `globals::write` -- for a name its lists place, and declined otherwise.
- **The stage read a `try` body and its `finally` in the enclosing scope.** The scope tree
  opened both and recorded neither, so a lowering could not enter them: a `const` in a
  `try` body was not found and its name was taken for a global. It is recorded now, and the
  lowering enters both as it enters a `catch` clause.
- **And the running emitter had the same shape of bug one layer down, on `main` too.**
  `tests/try_block_scopes_shadow.test.ts`, written for the fix above, failed with the door
  shut: `let x = 1; try { … } finally { let x = 3; x = 5 } return x` answered 5, and the
  version with `let x` in the `try` answered 23. Two causes. `emit/protect.rs`'s
  `emit_block` never gave a `try`, `catch` or `finally` body the environment of its own an
  ordinary block gets where it shadows a name kept in memory -- and everything a `try`
  assigns is kept in memory, by spelling. And the memo that forwards the last captured
  write to the next read (`CapturedWrite`) compared the name and the depth, which two
  bindings at depth zero of two different environments share; it compares the environment
  now. Both pass on this branch with the door open and shut.

Suite: **886 of 909**, LOST empty against `main` and the step before; 885 with the door shut.
