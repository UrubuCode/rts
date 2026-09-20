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

There is no interpreter and no baseline tier. A guard that fails suspends and
resumes the generic body of the same function, in the same binary.
`docs/engine/deopt-lateral.md` is the design and its cost.

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
