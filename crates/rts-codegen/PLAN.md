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
| `IdentifierReference` / `BindingIdentifier` / `LabelIdentifier` / `Identifier` | ~ | one `Name`; the three roles differ in what is legal, not in shape |
| `PrimaryExpression` — `this` | ✓ | a node, not a name |
| `PrimaryExpression` — `Literal` | ✓ | |
| `ArrayLiteral` / `ElementList` / `Elision` | ✓ | holes modelled as `Option<Expr>` |
| `SpreadElement` | ✓ | `Spreadable` in arrays/calls/`new`, `Property::Spread` in objects |
| `ObjectLiteral` / `PropertyDefinitionList` / `PropertyDefinition` | ✓ | five spellings; `__proto__` is a variant, not a key check |
| `PropertyName` / `LiteralPropertyName` / `ComputedPropertyName` | ✓ | `PropertyKey::Named` / `Computed` |
| `CoverInitializedName` | · | `{a = 1}` — only legal once reinterpreted as a pattern |
| `Initializer` | ✓ | |
| `TemplateLiteral` / `SubstitutionTemplate` / `TemplateSpans` / `TemplateMiddleList` | ✓ | `TemplatePart` keeps raw beside an optional cooked |
| `MemberExpression` | ✓ | `Member` (static) / `Index` (computed), deliberately split |
| `SuperProperty` | · | `super.x`, `super[e]` |
| `MetaProperty` / `NewTarget` / `ImportMeta` | · | |
| `NewExpression` | ✓ | separate node, not a call flag |
| `CallExpression` | ✓ | |
| `SuperCall` | · | not a call — it binds `this` in a derived constructor |
| `ImportCall` | · | `import(spec, options)` |
| `Arguments` / `ArgumentList` | ✓ | `Spreadable::count_is_static` |
| `OptionalExpression` / `OptionalChain` | ✓ | `ExprKind::Chain` is the boundary the short circuit reaches to |
| `UpdateExpression` | ✓ | own node, both positions — not an assignment of a constant |
| `UnaryExpression` | ✓ | all seven; `typeof` and `delete` take a reference, not a value |
| `AwaitExpression` | · | |
| `ExponentiationExpression` | ✓ | right-associative; unparenthesised unary left operand does not parse |
| `MultiplicativeExpression` | ✓ | `* / %` |
| `AdditiveExpression` | ✓ | `+ -` |
| `ShiftExpression` | ✓ | `>>>` is the one whose result outgrows a signed 32-bit value |
| `RelationalExpression` | ~ | ordering + `in` + `instanceof`; `#x in o` waits on private names |
| `EqualityExpression` | ✓ | all four |
| `BitwiseAND/XOR/ORExpression` | ✓ | all three |
| `LogicalAND/ORExpression` / `CoalesceExpression` / `ShortCircuitExpression` | ✓ | `LogicalOp`, kept off `BinaryOp` |
| `ConditionalExpression` | ✓ | |
| `AssignmentExpression` / `AssignmentOperator` | ✓ | every operator incl. the three short-circuiting `&&= ||= ??=`, as `AssignOp`; targets as `AssignTarget` |
| `AssignmentPattern` and its whole subtree | ✓ | `AssignTarget::Pattern`; leaf is `Pattern::Target`, an arbitrary place |
| `Expression` (comma) | ✓ | flat operand list |
| `YieldExpression` | · | `yield`, `yield*` |
| `PrivateIdentifier` | · | |

### A.3 Statements

| Production | State | Note |
|---|---|---|
| `Block` / `StatementList` / `StatementListItem` | ✓ | |
| `LexicalDeclaration` / `LetOrConst` / `BindingList` / `LexicalBinding` | ✓ | `BindingKind` + a `Pattern` target |
| `UsingDeclaration` / `AwaitUsingDeclaration` | · | in the current draft; disposal runs on scope exit |
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
| `WithStatement` | · | normative (14.11), forbidden in strict code |
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
| `AsyncArrowFunction` and subtree | ~ | shape complete; `await` waits on L7 |
| `MethodDefinition` | ✓ | `Property::Method` — a home object, which is what `super` reads |
| getter / setter / `PropertySetParameterList` | ✓ | |
| `GeneratorDeclaration/Expression/Method` | ~ | `is_generator` flag; no `yield` |
| `AsyncGenerator*` | ~ | both flags; no `await`, no `yield` |
| `AsyncFunctionDeclaration/Expression/Method` | ~ | `is_async`; no `await` |
| `ClassDeclaration` / `ClassExpression` / `ClassTail` / `ClassHeritage` | · | |
| `ClassBody` / `ClassElementList` / `ClassElement` | · | |
| `FieldDefinition` / `ClassElementName` | · | instance and static fields |
| `ClassStaticBlock` | · | |
| class element evaluation order | · | fields in source order after `super()`; statics once at definition; private installation before any initializer |

### A.5 Scripts and modules

Every production absent. `Script` and `Module` are different goal symbols, not a
flag: module code is always strict, top-level `this` is `undefined`, and
top-level `await` is legal. `Program` currently records none of it.

Absent: `ImportDeclaration` and all six clause shapes, `NameSpaceImport`,
`NamedImports`, `ImportSpecifier` (including the string-named form),
`WithClause`/`WithEntries`/`AttributeKey` (import attributes, spelled `with {}`),
`ExportDeclaration` in all seven shapes, `ExportFromClause`, `NamedExports`,
`ExportSpecifier`, `ModuleExportName`.

### A.1 Lexical

The parser's territory, listed because the tree must be able to hold what it
produces: BigInt literals (`123n`), regular-expression literals with flags,
numeric separators, every string escape form (including the legacy octal and
non-octal-decimal ones, sloppy-mode only), template cooked-vs-raw, hashbang, and
automatic semicolon insertion — where the restricted productions (`return`,
`throw`, `break`, `continue`, postfix `++`/`--`, `=>`, `yield`) change what
parses, so ASI is a correctness feature and not a convenience.

### Count

Present or partial: **60**. Absent: **34**.

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

**L5 — classes.** Declaration and expression, `extends`, constructor, instance
and static methods, fields, static blocks, private names and `#x in o`, `super.m`
and `super()`. Evaluation order is part of the deliverable, not a follow-up: this
is where the machine layer's shapes get their first real client, and where the
old engine's inheritance work (parent-first ordering, flattened members,
shape-keyed dispatch) is re-earned rather than copied.

**L6 — modules.** `Script` and `Module` as distinct goals. Every import and
export form, import attributes, live bindings, hoisting. Dynamic `import()` and
`import.meta`.

**L7 — suspension.** `await`, `yield`, `yield*`, `for await-of`, top-level
`await`. The machine layer already has the frame transformation and the
scheduler; this is the language deciding where a suspension point is.

**L8 — the last corners.** `with` (parse and reject outside sloppy mode rather
than fail to parse), `using`/`await using`, `new.target`, ASI in the parser,
the sloppy-mode Annex B forms.

**L9 — measured coverage.** test262's `test/language/` tree, filtered by the
`features` frontmatter to what is claimed implemented. A number produced by
running the standard's own tests, which is the only kind this crate's rule 7
permits.

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
