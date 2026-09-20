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

### What is left, measured 2026-09-20

| `bench/` | | `tests/` | |
|---:|---|---:|---|
| 8 | an iteration protocol | 61 | a generator |
| 6 | a class field | 15 | an async function |
| 5 | a call through neither a name nor a property | 15 | an array pattern |
| 4 | an object literal with a method | 10 | a rest parameter |
| 4 | a template literal | 8 | an object literal with a method |

**A generator and an async function are one piece**: both park a frame, so both wait
on `rts_cranelift::frame` — the same machinery `deopt-lateral.md` D3 needs.

**An array pattern, `for`-`of` and a rest parameter are one piece**: all three step the
iteration protocol, and all three owe the iterator its `return()` on an early exit —
which is the cleanup chain a `finally` needs, so it is really the same piece as that.

**A method in an object literal and a class field are one piece**: both are installed
rather than assigned — a method with a home object, a field per instance as the
constructor runs.
