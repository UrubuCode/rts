# rts-mir — the shared mid-level IR

The stage between a language's tree and the machine. `rts-codegen` lowers
JavaScript into it, a second front end would lower its own language into the
same thing, and `lower/` turns it into `rts_cranelift::ir`.

It exists because the pipeline was `AST → machine IR` and every decision that
needs a **fixed point** was therefore being taken in a single syntax-directed
pass, while emitting. `docs/engine/four-stages.md` has the measured consequences.

These rules are binding for changes inside this crate. RULE 0 of the root
`CLAUDE.md` applies: read this file in full first.

---

## 1. This crate never names a language

Not JavaScript, not Lua, not TypeScript. No `ToBoolean`, no `undefined`, no
`int32`, no seven-case falsy set. If a name here would have to be spelled
differently for a second language, it is in the wrong crate.

The test is mechanical: `grep -ri "javascript\|undefined\|nan\|prototype"` over
`src/` should find nothing but this README's own sentence saying so.

## 2. What is neutral is here; what is semantic is DECLARED

The structure is neutral: blocks, SSA, block parameters, dominators, liveness,
effects, safepoints, guards, deoptimisation points, the two tiers, and every pass
that needs nothing but effects and dominance.

Everything else arrives through [`Domain`] and the primitive table. A language
declares its types, its join, what each of its primitives computes, and what a
guard narrows a type to. This crate composes those; it does not know them.

That is the same mechanism `rts-core` already uses — `Value` does not know that
singleton 0 is `undefined`, and `Symbol` and `BigInt` are kinds declared by the
language. `docs/engine/a-second-language.md` is why.

## 3. One module may name the machine, and it is `lower/`

Everything else in this crate manipulates this crate's own representation.
`lower/` is where a `Prim` becomes instructions or a call, and where a
`DeoptPoint` becomes a `ResumeLabel`. It is the only module that may mention a
`Repr`, a `TypeId` or an offset.

`rts-cranelift` states the mirror of this rule about its own `lower/`. Two
layers, the same discipline, for the same reason: a boundary with an exception
everywhere is not a boundary.

## 4. A primitive is an index, never an operation this crate understands

`Prim(u32)` is opaque. Its effect summary and its transfer function come from the
tables the language registered. A pass that special-cases a particular `Prim`
value has put language knowledge here, and the next language's table numbers it
differently anyway.

## 5. Effects gate motion, and absence of an effect is a claim

Nothing may be moved, merged or eliminated except by reading [`Effect`]. A
primitive registered with the wrong summary produces a silently wrong program
rather than a failure — a pure-marked operation that allocates will be hoisted
out of the loop that was keeping its result alive.

So `Effect::PURE` is a claim about the runtime's implementation, and the language
that declares it owes a test that the implementation still matches.

## 6. A guard is a value in the dataflow

Not emission, not metadata. A guard is an instruction with a result, a narrowed
type and a `PointId`, so that CSE, hoisting and LICM reach it like anything else.
Ten guards becoming one is the result of passes; a pass cannot optimise what it
cannot see.

## 7. Two tiers, from ONE traversal

A function is lowered twice — specialised and generic — from the same MIR. Point
pairing is therefore by construction, and `guard::pair` is the net rather than
the mechanism. Two emitters that agree by convention drift; the check exists
because that is the failure this design is built to make impossible, not
unlikely.

## 8. The fall is a branch, never an unwind

There is no interpreter and no baseline tier. A guard that fails reaches the
generic body of the same function, in the same binary.
`docs/engine/deopt-lateral.md` is the design and its cost.

**Two forms, and only the second needs a frame reconstructed.** This rule said
"suspends and resumes", which describes one of them and is what the code now
contradicts — amended here rather than left standing, because a rule the code
disagrees with is worse than no rule.

- **A guard at the entry**, with nothing but guards before it, has no local state
  behind it: the live set *is* the parameters. Falling from one is a CALL to the
  generic body with the same arguments, and resuming at its entry is resuming at
  the point, because the point is the entry. Nothing is reconstructed because
  nothing was built. This is what `MachineOps::fall` is asked for and what
  `rts-codegen` answers.
- **A guard anywhere else** does need the frame reconstructed, and that is the
  rest of D3. `lower` refuses it by position — structurally, because this crate
  has no liveness pass and a condition it can check exactly is worth more than
  one it would approximate.

A fall hands over the **original** operands, never the narrowed ones. The generic
body is reached precisely when a speculation did not hold, so passing the value a
failed guard claimed to have produced would pass on the very thing that was wrong.

## 9. Every structural invariant is checked, and the checker runs in tests

`verify` is the statement of what a well-formed function is: every value defined
before use, every block terminated exactly once, every jump's arguments matching
the target's parameters, every `PointId` declared. A pass that breaks one of
those must fail a test rather than produce a program.

## 10. A second domain exists in the tests, and it is not decorative

`a-second-language.md`: *"a boundary with one client on each side is
indistinguishable from no boundary at all"*. With only JavaScript above it, this
crate will acquire JavaScript by accident. `tests/toy_domain.rs` instantiates a
three-type domain with integer distinct from float and a two-case truth rule, and
runs the generic passes over it.

When a pass cannot be written against the toy domain, that is the finding.

## 11. Files ≤ 500 lines

The root `CLAUDE.md` gives the two engine crates 1000 and everything else 500.
This crate takes 500 — it is new, so there is nothing to grandfather, and the
ceiling is easiest to hold from the start. A file that would pass it becomes a
folder of cohesive modules.

## 12. A suspension is a fact about the graph, and the flag is derived

Parking a frame belongs here and not in a language's table, for the reason rule 2
draws the line by: a table is exactly what a consumer is allowed not to
understand, and every consumer has to respect a suspension. So `Effect::SUSPENDS`
carries it, `commutes_with` answers `false` against anything in either direction,
and `falls_through` answers `false` — `gen.throw(e)` and a rejected promise both
resume the frame **by raising at that point**.

`Func::may_suspend` is **derived by `FuncBuilder` from the effect** and never
passed in. One fact in two places drifts, and the drift here is a function the
machine compiles with an ordinary frame and then tries to leave; `verify` checks
the agreement anyway, for a `Func` assembled by hand. Read from the effect and
not from `Op::Suspend`, so a language that parks inside a primitive of its own is
covered without this crate naming it.

It is an instruction and not a terminator, because `frame::resumable_form`
transforms the whole function from liveness. A graph that split its blocks at
every suspension would be doing that transform's work badly, and twice.

What it **answers** is the top of the lattice. `next(x)` chooses it and so does a
promise settling, so this function computed neither; narrowing it from the
operand is the natural mistake — the operand is right there — and it hands a
later pass a type nothing checks.

**Why this is rule 12 and not rule 9.** It belongs beside rule 8, and the rules
after it are referred to by number from elsewhere — `CLAUDE.md` and
`docs/engine/four-stages.md` both name rule 10 by its number. Renumbering to put
this in its logical place would silently repoint those, which is a worse cost
than a rule sitting at the end.
