# The language plan

What this crate has to be able to say, measured against the standard rather than
against intuition, and the order in which it will come to say it.

> This document includes material copied from or derived from the ECMAScript®
> 2027 Language Specification https://tc39.es/ecma262/.
> Copyright © Ecma International.
>
> The full copyright notice and licence is in `THIRD-PARTY-NOTICES.md` at the
> repository root. That licence permits copying, modification and derivative
> works for any purpose without fee, on the attribution conditions reproduced
> there. It governs the specification's *text*; implementing the language it
> describes is not a use of that text.

---

## 0. Where these facts come from

Every construct listed in §2 was extracted from the ECMA-262 source itself, not
from a summary of it and not from memory:

```
repository  https://github.com/tc39/ecma262
commit      994b48ed0c0940edaa0e4ce4d9e358fa3ba91edb   (2026-07-16)
file        spec.html                                   (3 143 846 bytes)
extraction  every `<emu-production name="...">` inside the Annex A subsections
```

This matters, and it is written down because the first attempt failed in a way
that would have been invisible in the result. Fetching the published spec through
a summarizing reader returned either the table of contents or a reconstruction —
plausible, mostly right, and impossible to distinguish from the real thing by
reading it. A grammar audit built on a reconstruction is an audit of what someone
remembers the language to be.

Two facts already came out different from what the reconstruction claimed: the
current draft contains `UsingDeclaration` and `AwaitUsingDeclaration`
(explicit resource management, which the reconstruction omitted), and contains no
decorator production at all. Both are checkable against the file above.

Cross-checked against ESTree (`estree/estree`, the `es5.md`…`es2025.md` deltas,
read raw) for one purpose only: the operator value sets, where an omission is
easy and silent. ESTree is **not** the model for the tree — its
`MemberExpression { computed: bool }` is exactly the collapse this crate refuses —
it is used as a second, independent list to be missing things from.

The clone is a working copy outside this repository and `spec.html` is
deliberately not vendored — see `THIRD-PARTY-NOTICES.md` for why, and for the
rule that an operation is cited by name and section id rather than reproduced.

What is **not** verified: the runtime-semantics notes in §5 come from knowledge
of the spec, not from an extraction. They are marked, and each will be checked
against `spec.html` at the point it is implemented, not before.

---

## 1. What this plan is for

Two failures are available here, and they look nothing alike.

The first is a **missing construct**: the tree cannot represent `for (const x of
xs)`, so no program using it compiles. Loud, immediate, found by the first user.

The second is a **wrong construct**: the tree can represent it and represents it
subtly incorrectly — `a += b` desugared to `a = a + b`, `+0 === -0` compiled to a
bit compare, a prototype walk that drops the receiver. Silent. Found by nobody,
until it produces a wrong number in someone's program a year from now.

The inventory below exists to make the first kind countable. §5 exists because
the second kind is the one that actually costs.

---

## 2. The complete inventory

Production names verbatim from Annex A. `✓` present, `·` absent, `~` partial
(present in a form that will need revision).

### A.2 Expressions

| Production | State | Note |
|---|---|---|
| `IdentifierReference` / `BindingIdentifier` / `LabelIdentifier` / `Identifier` | ✓ | one `Name`; the roles differ in what is legal, which is a check and not a shape |
| `PrimaryExpression` — `this` | ✓ | a node, not a name |
| `PrimaryExpression` — `Literal` | ✓ | |
| `ArrayLiteral` / `ElementList` / `Elision` | ✓ | holes modelled as `Option<Expr>` |
| `SpreadElement` | ✓ | `Spreadable` in arrays/calls/`new`, `Property::Spread` in objects |
| `ObjectLiteral` / `PropertyDefinitionList` / `PropertyDefinition` | ✓ | five spellings; `__proto__` is a variant, not a key check |
| `PropertyName` / `LiteralPropertyName` / `ComputedPropertyName` | ✓ | `PropertyKey::Named` / `Computed` |
| `CoverInitializedName` | n/a | a cover grammar, not a node: `{a = 1}` is an error as a literal and an `Element` with a default once reinterpreted |
| `Initializer` | ✓ | |
| `TemplateLiteral` / `SubstitutionTemplate` / `TemplateSpans` / `TemplateMiddleList` | ✓ | `TemplatePart` keeps raw beside an optional cooked |
| `MemberExpression` | ✓ | `Member` (static) / `Index` (computed), deliberately split |
| `SuperProperty` | ✓ | reads the home object prototype, keeps `this` |
| `MetaProperty` / `NewTarget` / `ImportMeta` | ✓ | |
| `NewExpression` | ✓ | separate node, not a call flag |
| `CallExpression` | ✓ | |
| `SuperCall` | ✓ | not a call — it binds `this` in a derived constructor |
| `ImportCall` | ✓ | specifier + options |
| `Arguments` / `ArgumentList` | ✓ | `Spreadable::count_is_static` |
| `OptionalExpression` / `OptionalChain` | ✓ | `ExprKind::Chain` is the boundary the short circuit reaches to |
| `UpdateExpression` | ✓ | own node, both positions — not an assignment of a constant |
| `UnaryExpression` | ✓ | all seven; `typeof` and `delete` take a reference, not a value |
| `AwaitExpression` | ✓ | the one expression that can end a frame residence |
| `ExponentiationExpression` | ✓ | right-associative; unparenthesised unary left operand does not parse |
| `MultiplicativeExpression` | ✓ | `* / %` |
| `AdditiveExpression` | ✓ | `+ -` |
| `ShiftExpression` | ✓ | `>>>` is the one whose result outgrows a signed 32-bit value |
| `RelationalExpression` | ✓ | ordering + `in` + `instanceof` + `#x in o` |
| `EqualityExpression` | ✓ | all four |
| `BitwiseAND/XOR/ORExpression` | ✓ | all three |
| `LogicalAND/ORExpression` / `CoalesceExpression` / `ShortCircuitExpression` | ✓ | `LogicalOp`, kept off `BinaryOp` |
| `ConditionalExpression` | ✓ | |
| `AssignmentExpression` / `AssignmentOperator` | ✓ | every operator incl. the three short-circuiting `&&= ||= ??=`, as `AssignOp`; targets as `AssignTarget` |
| `AssignmentPattern` and its whole subtree | ✓ | `AssignTarget::Pattern`; leaf is `Pattern::Target`, an arbitrary place |
| `Expression` (comma) | ✓ | flat operand list |
| `YieldExpression` | ✓ | bare, valued, and delegating |
| `PrivateIdentifier` | ✓ | `ClassKey::Private`, and `ExprKind::PrivateName` for `#x in o` |

### A.3 Statements

| Production | State | Note |
|---|---|---|
| `Block` / `StatementList` / `StatementListItem` | ✓ | |
| `LexicalDeclaration` / `LetOrConst` / `BindingList` / `LexicalBinding` | ✓ | `BindingKind` + a `Pattern` target |
| `UsingDeclaration` / `AwaitUsingDeclaration` | ✓ | its own statement — the name behaves as `const`, the difference is on the way out |
| `VariableStatement` / `VariableDeclarationList` / `VariableDeclaration` | ✓ | same shape as above |
| `BindingPattern` / `ObjectBindingPattern` / `ArrayBindingPattern` | ✓ | rest, holes, defaults, nesting; rest-last and rest-has-no-default are unrepresentable |
| `EmptyStatement` | ✓ | |
| `ExpressionStatement` | ✓ | the lookahead restrictions are the parser's, not the tree's |
| `IfStatement` | ✓ | |
| `DoWhileStatement` | ✓ | own node; `continue` jumps to the condition |
| `WhileStatement` | ✓ | |
| `ForStatement` | ✓ | `ForInit`; `copies_per_pass` names the fresh-binding rule |
| `ForInOfStatement` / `ForDeclaration` / `ForBinding` | ✓ | one node, `ForEachSource` × `ForEachTarget` |
| `ContinueStatement` / `BreakStatement` | ✓ | with labels |
| `ReturnStatement` | ✓ | `Option<Expr>` — bare and explicit-`undefined` stay distinguishable |
| `WithStatement` | ✓ | representable so it can be rejected with a reason |
| `SwitchStatement` / `CaseBlock` / `CaseClauses` / `CaseClause` / `DefaultClause` | ✓ | flat clause list, so `default` keeps its written position |
| `LabelledStatement` / `LabelledItem` | ✓ | |
| `ThrowStatement` | ✓ | |
| `TryStatement` / `Catch` / `Finally` / `CatchParameter` | ✓ | optional binding, and it is a pattern |
| `DebuggerStatement` | ✓ | |

### A.4 Functions and classes

| Production | State | Note |
|---|---|---|
| `FunctionDeclaration` / `FunctionExpression` | ✓ | |
| `FormalParameters` and subtree | ✓ | patterns, defaults, rest as its own field |
| "simple parameter list" | ✓ | `Function::has_simple_parameter_list` |
| `ArrowFunction` / `ConciseBody` / `ExpressionBody` | ✓ | `FunctionBody::Expression` |
| `AsyncArrowFunction` and subtree | ✓ | |
| `MethodDefinition` | ✓ | `Property::Method` — a home object, which is what `super` reads |
| getter / setter / `PropertySetParameterList` | ✓ | |
| `GeneratorDeclaration/Expression/Method` | ✓ | |
| `AsyncGenerator*` | ✓ | |
| `AsyncFunctionDeclaration/Expression/Method` | ✓ | |
| `ClassDeclaration` / `ClassExpression` / `ClassTail` / `ClassHeritage` | ✓ | one `Class`; heritage is an expression |
| `ClassBody` / `ClassElementList` / `ClassElement` | ✓ | source order preserved, because it is the semantics |
| `FieldDefinition` / `ClassElementName` | ✓ | `ClassKey` separates private from public |
| `ClassStaticBlock` | ✓ | |
| class element evaluation order | ✓ | `runs_at_definition` / `runs_per_instance`, documented on the module |

### A.5 Scripts and modules

Every production present.

`Script` and `Module` are recorded as a `Goal`, not derived from whether the body
has an `import`. Three things differ before a statement is read — module code is
always strict, top-level `this` is `undefined`, top-level `await` is legal — and
a module with no imports is still a module.

Imports are their own node rather than declarations with a source attached,
because an imported name is unlike every other binding a scope holds: it tracks
the binding in the other module rather than copying it, and it is immutable here.

Covered: all six import clause shapes, namespace import, string-named specifiers
(`import { "a b" as c }`), import attributes (`with { type: "json" }`, part of
the request's identity), all four export shapes including re-export and
`export * as ns`, and `export default` in both its declaration and expression
forms — which differ in liveness, and so are different variants.

### A.1 Lexical

The parser's territory, listed because the tree must be able to hold what it
produces: BigInt literals (`123n`), regular-expression literals with flags,
numeric separators, every string escape form (including the legacy octal and
non-octal-decimal ones, sloppy-mode only), template cooked-vs-raw, hashbang, and
automatic semicolon insertion — where the restricted productions (`return`,
`throw`, `break`, `continue`, postfix `++`/`--`, `=>`, `yield`) change what
parses, so ASI is a correctness feature and not a convenience.

### Count

Present: **93**. Absent: **0**. One row is marked n/a: `CoverInitializedName`
is a cover grammar rather than a node.

Every production in Annex A that a tree can hold, the tree holds. What is left
is not shape — it is the parser that fills it (ASI included, since ASI decides
what parses) and the passes that read it.

Was 31 / 63 when this document was written.

L1 moved five rows — update, exponentiation, shift, the three bitwise, the comma
operator — and finished two that were partial. Every operator the language has,
except `#x in o`, which waits on private names.

L2 moved seven: both declaration forms, the binding patterns, the assignment
patterns, the parameter list, the simple-parameter-list rule, and the `catch`
parameter.

What remains absent is classes, modules, every loop but `while`, `switch`,
templates, generators, `this`, spread, and the object-literal forms —
structure rather than vocabulary.

---

## 2b. Rules that are not nodes

Facts the grammar enforces *structurally* — through the shape of a production or
an early error — rather than through a node anyone can point at. They are listed
because each one is a decision the tree or the parser has to embody, and none of
them will show up as a missing `ExprKind` variant to remind us.

**Precedence encoded as a production shape.** The left operand of `**` is an
`UpdateExpression`, not a `UnaryExpression` — so `-2 ** 2` is a *SyntaxError*,
not `(-2) ** 2` and not `-(2 ** 2)`. The grammar refuses to guess, and so must
we. Similarly `??` has no production path that mixes it with `&&` or `||`:
`a ?? b || c` does not parse. Both are cases where the standard chose an error
over a precedence anyone would have to memorise.

**Optional chains have a boundary, and things are banned at it.**
`` a?.`tpl` `` is an early error, and so is `new a?.b()` — a chain cannot be
constructed through. This is the second reason (after short-circuit scope) that a
per-node `optional` flag is the wrong model: it has nowhere to put a rule about
the chain.

**Assignment targets are three different things.**
- `=` accepts a destructuring pattern or any simple target.
- A compound operator (`+=`, `**=`, …) accepts a simple target only —
  `[a, b] += c` is a SyntaxError.
- `&&=`, `||=`, `??=` accept a *simple* target only, and additionally
  short-circuit, so they neither read nor write when the left side decides.

And a destructuring target is a full `LeftHandSideExpression`, not an identifier:
`({ a: obj.x } = src)` is legal. A pattern type that only holds names cannot say
this.

**`__proto__` in an object literal is not a property.** In the plain
`__proto__: value` form (not shorthand, not computed, not a method) it sets the
prototype, and writing it twice is an early error. The other three spellings are
ordinary properties named `__proto__`. Four syntaxes, two meanings.

**Array literal length is not the element count.** `[1, 2, ]` has length 2 —
trailing comma is a separator. `[1, 2, , ]` has length 3 — that one is a hole.

**Array destructuring uses the iterator protocol, not indexing.** `[a, b] = xs`
calls `xs[Symbol.iterator]()`, which is why it works on a `Set` and why it has to
close the iterator afterwards (§5.6).

**`typeof` is the one read that cannot fail.** On an unresolvable reference it
answers `"undefined"` rather than throwing — the only place in the language where
touching an undeclared name is not a `ReferenceError`.

**`delete` has two early errors.** On an unqualified identifier in strict code,
and on a private field (`delete this.#x`) in any code.

**`CoverInitializedName` is legal only after reinterpretation.** `{a = 1}` is an
error as an object literal and correct as a destructuring pattern — the parser
cannot know which until it sees what follows. A cover grammar, not a node.

**ASI is a parser rule with semantic force.** The restricted productions —
`return`, `throw`, `break`, `continue`, postfix `++`/`--`, `=>`, `yield`, `async
function`, `using` — change what parses when a newline appears. And ASI never
fires inside a `for(;;)` header.

---

## 3. Phases

Ordered by what unblocks the most, not by what is easiest. Each lands as its own
commit with tests naming the language fact it pins.

**L1 — complete the operators. — DONE.** `**`, the three shifts, the three bitwise, `in`,
`instanceof`; unary `+`, `~`, `delete`; `++`/`--` as their own node in both
positions; the comma operator; every compound assignment. `&&=`/`||=`/`??=` go
with `LogicalOp`, not `BinaryOp` — they short-circuit, and a compound assignment
that does not evaluate its right side is not the same node as one that does.
*Cheap, and it stops the tree lying about how much of the language it holds.*

**L2 — patterns. — DONE.** One `Pattern` type for both destructuring roles, with the
distinction the spec makes: a **binding** pattern introduces names, an
**assignment** pattern writes to arbitrary targets (`[a.b] = xs` is legal). Used
by declarations, parameters, `catch`, and `for`-heads. Unblocks the rest of L3.

**L3 — the remaining control flow. — DONE.** Three-part `for`, `for-in`, `for-of`,
`for await-of`, `do-while`, `switch` with its single shared scope, labels,
labelled `break`/`continue`, `debugger`. `for-of` carries the iteration protocol,
which is where `IteratorClose` lives (§5.6).

**L4 — objects and functions in full. — DONE.** Object-literal shorthand, methods,
getters, setters, spread, `__proto__`; spread in calls and `new`; concise arrow
bodies; `this`; template literals with raw text; tagged templates.

**L5 — classes. — DONE.** Declaration and expression, `extends`, constructor, instance
and static methods, fields, static blocks, private names and `#x in o`, `super.m`
and `super()`. Evaluation order is part of the deliverable, not a follow-up: this
is where the machine layer's shapes get their first real client, and where the
old engine's inheritance work (parent-first ordering, flattened members,
shape-keyed dispatch) is re-earned rather than copied.

**L6 — modules. — DONE.** (dynamic `import()` and `import.meta` landed with L5.) `Script` and `Module` as distinct goals. Every import and
export form, import attributes, live bindings, hoisting. Dynamic `import()` and
`import.meta`.

**L7 — suspension. — DONE.** `await`, `yield`, `yield*`, `for await-of`, top-level
`await`. The machine layer already has the frame transformation and the
scheduler; this is the language deciding where a suspension point is.

**L8 — the last corners. — DONE** except ASI, which belongs to the parser and lands with it. `with` (parse and reject outside sloppy mode rather
than fail to parse), `using`/`await using`, `new.target`, ASI in the parser,
the sloppy-mode Annex B forms.

**L8.5 — the parser bridge. NOT IN THE ORIGINAL PLAN, and L9 cannot start
without it.**

Writing L1–L8 surfaced a dependency this document did not list: **nothing fills
this tree.** Every phase above built what a program can be represented *as*, and
none of them built the thing that turns source text into one. That was an
omission here, not a discovery about the language, and it is recorded rather than
quietly folded into L9 because a plan that hides its own missing step is worse
than one with a gap in it.

The work is not a new parser. `rts-parser` already exists — ~8000 lines lowering
SWC's AST into `rts-ast`, the *old* tree. The bridge is a second lowering of the
same SWC output into this one, and it is comparable in size to the first.

Two things make it more than mechanical, and both are the reason it is a phase
rather than an afternoon:

- **ASI.** Listed under L8 as if it were a corner. It is not: it decides what
  parses, so it belongs to whatever produces the tree. SWC implements it, which
  is an argument for the bridge over a hand-written parser.
- **The cover grammars.** `{a = 1}` is an error as an object literal and an
  `Element` with a default as a pattern; `(a, b)` is either a parenthesised comma
  or an arrow's parameters. The tree has one shape for each *outcome*, so the
  bridge must reinterpret rather than translate — and reinterpreting into the
  wrong role is exactly what `Pattern::is_valid_binding` was written to catch.

**L9 — measured coverage.** test262's `test/language/` tree, filtered by the
`features` frontmatter to what is claimed implemented. A number produced by
running the standard's own tests, which is the only kind this crate's rule 7
permits.

**DONE, and here is the number.** `tests/test262.rs`, run against
`test/language` — 23 724 files after excluding fixtures:

```
read correctly     21 712   91.5 %
refused, named        549    2.3 %   our gaps, each naming a construct
wrongly rejected      214    0.9 %   valid programs we refused
wrongly accepted    1 249    5.3 %   invalid programs we did not refuse
```

**What that measures, precisely.** Whether the front end *reads* each program
correctly: accepts what the corpus says is valid, refuses what it says is not.
Nothing runs. This is not a pass rate, and calling it one would be false — a
program that parses can still be compiled wrongly. It is the floor underneath
everything else, because a front end that mis-reads a program cannot compile it
correctly.

The first number published here was **92.3 %, and it was wrong** — measured
against a corpus missing 503 of its 24 007 files. On Windows some test262 paths
exceed the 260-character limit; `git sparse-checkout` *warns* rather than fails,
skips them, and everything downstream looks healthy. The missing files were
concentrated in `import/import-defer/…`, so they were disproportionately ones we
get wrong: recovering them added 279 correct and **220 incorrect**. The corpus
must be cloned with `-c core.longpaths=true`, and `check_checkout_is_complete`
now asks git what should be on disk and refuses to report a score if anything is
absent.

Three more findings the number would have hidden, and the harness was fixed for
each before it was believed:

- An early run read the corpus with **TypeScript** syntax. TypeScript is a
  superset, so it accepts programs JavaScript rejects — 240 false accepts were
  that alone. `Dialect` now exists because of this measurement, and a `.js` file
  gets `Dialect::JavaScript`.
- test262 runs an `onlyStrict` file with a strict prologue prepended. Without it,
  a test whose entire point is a strict-mode error is handed to a sloppy parse,
  which correctly accepts it — and the harness recorded our correct answer as a
  defect.

It also found a real bug in our own code: `strip_shebang` looked only for `\n`,
and JavaScript has four line terminators. `comments/hashbang/line-terminator-*`
caught it. That is what a corpus is for.

**L10 — early errors.** The 1 249 false accepts are not a parser gap. Sampled:
they are redeclaration tests — `let x; function x() {}` in one scope — plus
duplicate `__proto__`, `delete` of a name in strict code, invalid assignment
targets. Static semantics: rules that no grammar production encodes and no node
can hold, which is exactly the class §2b named and said would need checking
rather than shaping.

So the tree is finished and the *checker* has not started. That is one phase, it
is measurable from the day it begins — the 1 249 is its scoreboard — and it is
where the next work goes.


## 3b. Phase E — the tree, in the machine's representation

**L1–L10 are all front end.** Every one of them is about what a program can be
represented as, or about whether it was read correctly. None of them produces a
single instruction, and the plan did not say so — which is the second omission
of this kind after L8.5, and is recorded the same way rather than folded in
quietly.

So today a program can be read and cannot be run. `FuncBuilder` has no caller
outside the machine's own tests.

### The name

`emit`, not `lower`. `rts-cranelift::lower` is IR → machine code and says it is
"the only module permitted to construct code-generator instructions". Calling
both steps "lowering" would make that claim uncheckable in conversation:

```text
source ──parse──▶ tree ──emit──▶ IR ──lower──▶ machine code
                        (here)     (rts-cranelift)
```

### Everything is `Tagged`, on purpose

No type pass exists, so every value is `Repr::Tagged` and every operator is a
`GenericOp`. That is rule 5 rather than a shortcut: `a + b` in JavaScript is not
addition — it converts both operands to primitives and *then* decides between
concatenation and arithmetic from what came back. A first version emitting
`arith` because most numbers are doubles would be fast, wrong for `"a" + 1`, and
wrong **silently**.

The type pass is a separate phase with its own evidence. What it adds is the
right to emit `arith`; it does not change what is correct here.

### The phases

**E1 — expressions and straight-line statements. DONE.** Literals, locals,
arithmetic and relational operators, sequence, assignment including compound
forms, `let`/`const`/`var` with a plain name, blocks, `return`, falling off the
end. Ten tests.

A binding is a `ValueId`, not a stack slot. The slot-per-local implementation is
the obvious one and it is wrong to start with: undoing it later is a rewrite
rather than an optimisation, because every read has become a memory operation
for a subsequent pass to prove away. Pinned by a test asserting nothing
allocates.

Everything else is refused **by name** — `EmitError::Unsupported { construct }`.
The list of names in `expr.rs` and `stmt.rs` is the work queue below, and it is
readable without running anything.

**E2 and E3 were listed in the wrong order, and the code said so.**

The plan had control flow before calls. Reading the machine's builder before
writing either showed that it cannot be:

```rust
pub fn branch(&mut self, cond: ValueId, …) -> BuildResult<()> {
    if self.func.repr_of(cond) != Repr::Bool {
        return Err(BuildError::WrongDomain { operation: "branch", … });
    }
```

A branch takes a **proven** boolean, and the route from a tagged JavaScript
value to one is `ToBoolean` — which is a call, because six of the seven falsy
values a comparison settles and the seventh is the empty string, whose emptiness
is read from the heap.

The same reading found something larger. The machine refuses to lower a generic
operation at all:

```rust
Inst::Generic(..) => Err(LowerError::NotYetLowered { needs: Capability::Calls })
```

So **E1's own output cannot become machine code** without calls either. Calls
are not third in the order; everything routes through them. The order below is
the corrected one, and the correction is recorded rather than quietly applied
because the original ordering was an assumption presented as a plan.

**E2 — calls, and the first control flow. DONE.** `runtime/` holds the
operations the language performs by calling out — `Add`, `StrictEquals`,
`ToBoolean`, `NumberToString` — each with a symbol and a signature, declared on
demand so a program that never concatenates carries no relocation to the string
path. `===` and `if` are emitted; the arms merge through block parameters.

Linkage here is **by name**, which is not a departure from the engine's index
rule. Index linkage is right for a set one side numbers and the other reads,
where a skew fails quietly. This set is stated independently in two crates that
never see each other's source, and a disagreement between them should be an
unresolved symbol at link time rather than a call to the wrong function with
plausible arguments.

**E3 — loops. DONE.** `while`, `do`/`while`, three-part `for`, and unlabelled
`break`/`continue`. `switch` and labels remain refused by name.

A loop header is a join whose second predecessor does not exist when its
parameters must be decided — the one thing `if` did not have to solve. Two ways
out, and the second is taken: giving every live local a parameter is correct and
makes every loop pay for every variable in scope, which is the same trap as the
stack-slot-per-local. Asking the tree which names the body *assigns* is a
syntactic question with a syntactic answer, and it over-approximates only in the
safe direction — an assignment in a branch that never runs still counts.

`break` and `continue` merge through the same mechanism: a `continue` is a back
edge and a `break` is an extra predecessor of the exit, so both carry the same
names.

### Can any of this run yet? No, and the blocker was measured

Asked directly, and worth the paragraph because the answer was not the expected
one. The machine executes: a dozen of its tests compile into this process's
memory and call the result. The plumbing is proven.

What could not run was ours. E1 emitted `Inst::Generic` for every operator, and
the machine refuses to lower a generic operation **unconditionally**:

```rust
Inst::Generic(..) => Err(LowerError::NotYetLowered { needs: Capability::Calls })
```

So E1 and E2 produced IR that passes the verifier and can never become machine
code — and no test caught it, because every test stopped at the verifier. A
verifier answers "is this well formed", not "can this be compiled", and the two
questions are not the same one.

Fixed by the boundary's own logic: which symbol a generic addition dials is a
fact about JavaScript, so the language emits the call. Nothing this crate emits
is a generic operation any more, and a test asserts that.

**The regression that came with it, stated rather than quiet:** `-`, `*`, `/`
and the four relational operators are now refused. They were accepted before as
generic operations that could not be compiled. They return when the runtime
defines them — inventing symbols here that `rts-core-rwk` does not export is
exactly the drift the audit named, and nothing links the two sides yet to catch
it.

### Where the executable test has to live, and why it is not here

Not in this crate. Compiling and running needs `Linkage`, `MachineModule` and a
JIT module, and rule 1 is unambiguous: *"This crate never touches Cranelift…
Not for convenience, not for one case."* A dev-dependency on `cranelift-module`
to write one test would be the concession that rule refuses.

There is also a real gap underneath it. The two destinations are not equal for
this:

- **object file** — an undefined `__rts_add` is resolved by the linker against
  the runtime archive. This path works.
- **executable memory** — `executable_memory()` builds and consumes its
  `JITBuilder`, so nothing can register the address of a runtime symbol. The
  machine has `EntryTable` for exactly this, and it serves `RtEntry` — the
  machine's own entries, not the language's.

So AOT could execute today and JIT could not. Both belong to the crate that may
name a compiler and a runtime at once, which is `rts-host`. Its two stated
preconditions — core's entry path, and a lowering — are now met.

**E4 — objects and property access. DONE, the correct half.** Object literals,
`o.x` and `o.x = v`, through three runtime entry points. A property key is
resolved while compiling and crosses as a **number**, which makes a second
agreement between the crates — the compiler's key registry and the runtime's
must have issued the same ones — wired and asserted in `rts-host-rwk` beside the
singleton numbering.

Shapes get their first real client, through the runtime rather than through
compiled code: a property write transitions the object's layout, so two objects
built the same way share one `ShapeId`. That is the property an inline cache
depends on, and it is tested directly.

**Deliberately not done: the fast path.** `cached_get` and `guard_type` still
have no caller. Property access is a call that looks a key up in a layout, which
is correct and slow. A cache built before there was something correct to cache
would be a cache over a guess — and the type pass showed what the measured
version of this work looks like, so the fast half waits for a measurement rather
than an intention.

**E4b — the fast path. DONE, and it went further than planned.** `guard_type`,
`cached_get` and `cached_set` all have callers; the last did not exist in the
machine and was added. A property read is ~0.9 ns and a write 5.4 ns, against
132.8 and 71.8 when E4 landed.

**E4c — arithmetic on what nothing proved. DONE.** The type pass proves things
about locals, and `o.n` is not one. A guard needs no such knowledge: it tests the
value it got, takes the instruction when both operands are doubles, and falls
back to the call that would have happened anyway. 24 ns per operator to ~1 ns,
with the fully proved kernel unchanged — a proved operand never reaches the
guard.

Every number here is in `docs/engine/new-engine-speed.md`, with the method, the
scaling checks, and the two occasions the measurement was wrong before the code
was.

**E4d — the operators a program is actually written with. DONE.** `!`, unary
`-` and `+`, `void`, `?:`, `&&`/`||`/`??`, `++`/`--`, `!==`, the logical
assignments on a local, and compound assignment to a property. `emit/choice.rs`
and `emit/unary.rs`; 16 tests, every one of them run rather than verified,
in `rts-host-rwk/tests/running.rs`.

Chosen as a slice because it is exactly what needs **no new runtime operation
and no new machine capability** — the whole set is branches, merges, and the
arithmetic already defined. `for (let i = 0; i < 5; i++) total += i;` compiles
and runs, which was unwritable before and is the measurement of what changed.

Two decisions worth carrying forward, both of which the obvious spelling gets
wrong:

- `-x` is emitted as `x * -1`, not `0 - x`. `-(0)` is `-0` and `0 - 0` is `+0`,
  and `1 / -0` is `-Infinity`, so the two are distinguishable. Going through `*`
  also means numeric coercion is stated once — the guard, the instruction and
  the runtime symbol are all already decided there.
- `x++` is `x - -1`, not `x + 1`. `+` may concatenate, and `"5" + 1` is `"51"`
  where `"5"++` is `6`.

Refused deliberately rather than for want of time: `&&=` on a **property**. When
the left side decides, the specification performs no write at all, which a
setter observes — so emitting the write anyway would be right for plain data and
wrong for the objects the operator exists for.

**E5 — functions and closures. THE TWO LOWER HALVES ARE IN; THE EMISSION IS
NOT.** A captured local stops being a `ValueId` and becomes heap storage, which
is why `emit::scope::Binding` is an enum with one variant today.

What was built and tested first, because emission cannot be written against
capabilities that do not exist:

- **`rts_cranelift::ir::Inst::FuncAddr { callee: FuncId }`** — the address of a
  declared function, as `Repr::I64`. The capability was genuinely absent:
  `ConstDecl::Symbol` existed with no lowering at all. Taken by `FuncId` rather
  than by symbol name, because the address is known at emission and a name would
  reintroduce string linkage for the one population that provably does not need
  it. `Repr::I64` and not a reference, so the collector is never handed a
  pointer into the text segment. Builder refuses an undeclared callee, verifier
  refuses it again for IR that did not come from the builder — rule 7 wants
  both. Two tests in `rts-cranelift/tests/calls.rs`.
- **A callable, in `rts-core-rwk::entry::functions`** — a region cell at a
  reserved layout holding two words, the code address and the environment. Not
  an object with two properties: `code` would then be a key in the registry that
  JavaScript could read and *write*, and a program storing a number there would
  name the instruction the next call jumps to. `Context::shape_of` had to learn
  about the second reserved layout, because `shape_of_type` is grown with
  `resize(n, shape)` and a callable would otherwise answer property reads by
  interpreting its code address as a field.
- **`RuntimeOp::ClosureNew` and `RuntimeOp::Call`**, wired through the host.

### The three decisions E5's emission is now committed to

**Calling is a runtime operation, not `call_indirect`.** The machine's indirect
call takes a callee *proven to be code*, and finding out whether a JavaScript
value is code reads the heap. The sharper reason: `1()` throws a `TypeError`,
throwing needs protected regions, and nothing emits those — so compiled code has
no way to fail here and the runtime does. The alternative is not a slower wrong
answer, it is a jump to an arbitrary address.

**The arity is fixed at four, and the convention is
`(env, this, a0..a3) -> value`.** JavaScript's arity is dynamic; expressing that
needs a caller-allocated argument vector, which is a stack slot this compiler
does not emit. A call with more arguments is refused **by name** rather than
truncated. `ARGUMENT_SLOTS` is stated in `rts-codegen` — the language decides its
own convention — restated in `rts-core-rwk`, and `rts-host-rwk` carries a `const`
assertion that they agree, the same shape as the singleton and key numberings.

**Capture goes in an environment object, chained by `__outer`.** A captured local
is a property of an environment object created at function entry; two closures
made in one activation get the same object, which is what makes them share the
variable. A nested function receives its defining activation's environment as
parameter 0; if it captures anything itself it creates its own and points
`__outer` at the one it received. **The hop count is static** — the compiler
knows how many links out a name is — so a read is `hops` loads and a
`GetProperty`, never a search.

The analysis that decides which locals those are was written and is *not* in the
tree, because dead code is not kept: it is `declared_in_this_function ∩
referenced_anywhere_inside_a_nested_function`, deliberately over-approximating
(it counts a nested function's own locals too). The direction of that error is
what makes it safe — a name wrongly in the environment costs one load, a name
wrongly out of it is two closures disagreeing about a variable.

### What is left, in order

1. `Ctx` carries the `TypeRegistry` and the functions emitted so far; `emit_body`
   becomes `emit_program`, answering a list of `(FuncId, Function)` rather than
   one function.
2. `Binding` gains `InEnvironment { hops, name }`, and every site that reads or
   writes a local goes through one pair of functions rather than matching the
   enum — `stmt.rs`, `expr.rs`, `unary.rs` and `loops.rs` each touch bindings
   today, which is four places to teach.
3. Function declarations are hoisted per block, so recursion and mutual
   recursion resolve.
4. Call sites, with `this`: `f()` passes `undefined`, `o.f()` passes `o` and
   evaluates `o` exactly once.
5. The host places every emitted function rather than one, and the script's own
   entry takes the same convention.

**E5 — functions and closures.** The emission itself, as listed above.

**E6 — the rest.** `throw`/`try` over the machine's protected regions,
`await`/`yield` over its suspension, classes, modules.

### What E-phases are measured against

Not test262's reading rate — that measures the front end and is already
reported. An emitted program either runs and produces the right value or does
not, and that needs E3 before it can be asked at all. Until then the honest
statement is a list of what is refused, which is what `Unsupported` maintains.

---

## 4. TypeScript is not a phase

It is not "L10: add types". It is a different claim about what the language is
for, and it changes what the phases above are *worth*.

### The ordinary arrangement, and why RTS is not it

A JavaScript engine receives JavaScript. Types were erased before it ever ran, so
everything it knows it learned by watching: this call site has seen one shape, so
speculate and guard; it saw a second, so widen; a third, so give up. The
speculation is good, and it is entirely a reconstruction of information the
programmer already had and the pipeline threw away.

RTS does not receive JavaScript. It receives **TypeScript, and compiles it** — so
the annotation is still there when the decision is being made. That is the whole
difference, and it points somewhere specific: TypeScript here is closer to a
machine-typed language's declaration syntax than to a linter that runs first.

The consequence the request names:

```ts
Engine.findComponent<PlayerEntity>();   // returns PlayerEntity, as data
```

For that to return data rather than a tagged word, the type argument has to reach
code generation. Which means **monomorphization**: `findComponent<PlayerEntity>`
and `findComponent<Transform>` are compiled as two functions, each with the
return representation its own type argument proves. This is what C# does for
value-type instantiations and what Rust does for every one. TypeScript's own
compiler erases instead — which is correct for a transpiler and is exactly the
information this compiler must not throw away.

### The rule this must not break

**A claim is not a proof, and the boundary is where it stops being either.**

An annotation is checkable evidence *inside* code RTS compiled, and an unverified
assertion at every edge RTS did not: an argument from a JavaScript caller, a
`JSON.parse` result, a foreign function, an `as` cast, an ambient `.d.ts`
describing a library that was never compiled here.

The old engine has this wrong today, and it is worth being precise about how,
because the mistake is attractive rather than careless. `repr_map.rs` maps an
annotated `i64` straight to an unboxed 64-bit register — no guard. Fast, and
correct exactly as long as every caller told the truth. When one does not, the
result is not a crash: it is a wrong number, computed at full speed, with nothing
in the output pointing back at the lie.

The rule that fixes it costs almost nothing where it matters:

- **Inside a proven region** — a value RTS produced from code RTS compiled —
  the annotation is a proof. Unbox, no check, full speed.
- **At an untrusted boundary** — a guard, which the machine layer already makes
  unavoidable: narrowing is only reachable through `Terminator::Guard`, and a
  terminator cannot have its failure path forgotten.

One guard at the edge, none in the loop. The measured cost is a compare and a
predictable branch, once per boundary crossing, in exchange for the whole region
behind it being honestly unboxed instead of speculatively so.

### `.d.ts` as a representation source, and the two kinds of it

A `.d.ts` describes a value's shape without describing its code — which is
exactly the input a layout decision needs. Two uses, and conflating them is how
the guarantee would be lost:

1. **A `.d.ts` RTS emitted** for code RTS compiled. The shapes in it are the
   shapes the compiler chose. Reading it back is reading its own notes, and the
   information is proof. Cross-module unboxing, cross-module field offsets,
   cross-module devirtualization all become available without inlining.
2. **A hand-written or third-party `.d.ts`** describing JavaScript nobody
   compiled. Every fact in it is a claim. Usable to *pick* a representation —
   the guess is a good one, someone wrote it deliberately — and never usable to
   skip the guard.

The gain is concentrated where the old engine currently gives up. An object whose
`.d.ts` declares `{ x: number; y: number }` needs no shape check and no boxed
slots: it is two `f64` at fixed offsets. An interface reached through a small
closed set of implementations is a switch, not a hash lookup. A generic
instantiated at a known type is a monomorphic function. In every one of those,
the NaN-box is not made cheaper — it is **not there**, which is the only form of
"faster boxing" that actually wins.

### Inheritance, and what the old engine already earned

Not to be reinvented. `class/inherit.rs` orders classes parent-before-child and
flattens members so a subclass's layout is a prefix of nothing and a superset of
its parent's; `class/vdispatch.rs` puts class identity in slot 0 and dispatches by
comparing that word against the set of candidates, with a static path when the
receiver's class is known and a dynamic one when it is not.

That is the right shape and it stays. What changes is where the candidate set
comes from. Today it is discovered from the classes in the program being
compiled. With declarations available it is the declared implementations of the
declared type — which is a smaller set, known earlier, and known across modules.
A closed set of one is a direct call, and the dispatch disappears rather than
getting faster.

### Ordering

Nothing above starts before L5. Types decide representations of things, and until
the tree can hold classes, generics, and modules, there is nothing for a type to
decide about. The phases are ordered so that when the type layer arrives it has a
language underneath it and not a subset.

---

## 5. Where a straightforward implementation is wrong

Not a list of hard things. A list of things that look done and are not — each has
a plausible implementation that passes the obvious test and is incorrect. Every
one gets a test naming it, at the phase that implements it.

**Sourcing note:** unlike §2, these come from knowledge of the spec rather than
from an extraction of it. Each will be read against `spec.html` when its phase
lands, and the section id recorded in the code comment at that point.

1. **`+` decides after coercion, not before.** `ToPrimitive` runs on both sides
   first; the string-or-number branch reads the *results*. `[] + {}` concatenates
   though neither operand is a string. Checking `typeof left === "string"` first
   is the wrong shape of the algorithm, not a shortcut of it.

2. **`+0 === -0` is true; `NaN === NaN` is false.** Both fall out wrong from a
   bit compare — which is exactly what a NaN-boxed word invites. And `Object.is`
   and `Map` keys each want a *different* one of the three equalities: strict,
   `SameValue`, `SameValueZero` differ only in those two cells, and `Map` uses
   the one where `NaN` is a usable key.

3. **`a <= b` is `!(b < a)`, coercing `b` first.** The operands evaluate left to
   right; their *coercions* do not. Two `valueOf` calls, observable order.

4. **`Number("")` is `0`.** `parseFloat("")` is `NaN`. Hex, octal and binary
   string forms reject a sign, so `Number("-0x1")` is `NaN` while `Number("-1")`
   is `-1`.

5. **Number→string is the shortest round-tripping decimal.** `0.1` prints
   `"0.1"`; `0.1 + 0.2` prints `"0.30000000000000004"`. Not a fixed precision —
   any fixed choice is wrong for one of those two. And `(-0).toString()` is
   `"0"`, though `-0` is a distinguishable value everywhere else.

6. **`IteratorClose` runs on early exit.** `break`, `throw`, and a destructuring
   that consumed fewer elements than the iterator offered. Skip it and a
   generator's `finally` never runs. The completion rules are asymmetric: an
   error thrown while closing during an unwind is discarded in favour of the
   original; the same error after a clean `break` propagates.

7. **`finally` overrides.** An abrupt completion from a `finally` block replaces
   whatever was in flight — `try { throw e } finally { return 1 }` returns `1`
   and the error is gone. The machine layer's cleanup blocks make this
   representable; the lowering has to actually mean it.

8. **The prototype walk carries the original receiver.** Recursing into the
   parent with the parent as receiver breaks `this` in every inherited getter.
   And the walk stops when a *descriptor* is found, not when a non-`undefined`
   *value* is: an own property explicitly set to `undefined` shadows the parent.

9. **Enumeration order is not insertion order.** Array-index keys first in
   ascending numeric order, then the other strings in insertion order, then
   symbols. A single insertion-ordered slot list — the obvious backing for a
   shape — is wrong for any object mixing the two.

10. **Sloppy-mode `this` substitution depends on the callee.** Whether `this`
    becomes the global object is decided by the strictness of the function being
    called, not of the code calling it.

11. **A derived constructor's `this` does not exist before `super()`.** Reading
    it is a `ReferenceError`, not `undefined`. And a constructor returning an
    object discards the instance — for a base class; a derived class returning a
    primitive is a `TypeError` instead.

12. **`?.` short-circuits the chain, not the link.** In `a?.b.c`, an absent `a`
    skips `.c` too. A per-node flag cannot express that; the chain needs a
    boundary.

13. **A property that holds `undefined` is not a missing property.** `in` and
    `hasOwnProperty` tell them apart, and so does the shape.

14. **ASI changes what parses.** A newline after `return` ends the statement.
    Not formatting — meaning.

---

## 5b. Deferred: the tree's storage

The tree is `Box`-linked today. Some of those boxes are forced — `ExprKind`
contains `PropertyKey` contains `Expr` is a cycle, and Rust needs indirection
somewhere in it — but most are just the obvious way to write a tree.

The alternative is an **arena with indices**: one `Vec<Expr>`, and `ExprId(u32)`
where a `Box<Expr>` is now. It is what rustc, Zig and Carbon do, and it is
probably right here too.

The reason is not allocation, and saying so would be claiming a measurement that
does not exist. It is **side tables**. A type pass wants to attach a `Claim` and
then a representation to every node, and with indices that is a `Vec` parallel to
the arena — no field on the node, no growth of the node, no pass rewriting the
tree to record what it learned. With boxes, each of those becomes either a field
nobody else uses or a `HashMap` keyed by address.

Not done now because it is a mechanical change that answers no open question, and
it would be done blind: no pass yet exists that needs the side table. The trigger
is §4 — when representations start being decided per node, convert first, then
write the pass.

Note this is unrelated to `PolyValue` and NaN-boxing, which live in the *other*
layer. Those encode one JavaScript value in 64 bits at **run time**. This tree is
read once by the lowering and discarded, and never exists while the compiled
program runs.

---

## 6. Deliberately out of scope

- **Decorators.** Not in the spec text at the commit above. Revisit when merged.
- **`Annex B` sloppy-mode forms**, beyond parsing them without dying. Legacy web
  compatibility; the cost is real and the benefit here is not.
- **The TypeScript *type* language as a checker.** RTS reads annotations to
  decide representations. Full inference, conditional and mapped types, variance
  — a separate problem, and modelling it here would model it twice.
- **`eval` with a live scope.** Runtime compilation exists; giving it the
  caller's bindings would make every enclosing scope unprovable, which is
  precisely the property this compiler is built to have.
