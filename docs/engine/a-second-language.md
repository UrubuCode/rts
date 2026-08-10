# A second language on this machine

The boundary between `rts-codegen` and `rts-cranelift` is stated as a rule about
knowledge: the language knows no machine, the machine knows no language. A second
front end is the only thing that tests whether that rule is real or merely
written down, because a boundary with one client on each side is indistinguishable
from no boundary at all.

This document records what a second language would actually cost, which parts of
the runtime are neutral **by construction** rather than by accident, and why the
ordering that looks obvious is wrong. It is not a plan and nothing here is
scheduled; `crates/rts-codegen/PLAN.md` and `crates/rts-core/PLAN.md` are
where work is queued.

Lua is used throughout as the concrete case, because it is far enough from
JavaScript to be informative and close enough to be plausible. Nothing below is
specific to it beyond the examples.

---

## The neutrality is deliberate, and it is stated

Two places in `rts-core` name a second language as the reason for a design,
rather than as a hypothetical benefit:

- `README.md` rule 4 — `Value` does not know that singleton 0 is `undefined`; the
  caller passes a `Singletons`, *because a second language on this machine
  numbers its own differently*.
- `PLAN.md`, under what decides membership — hardcoding JavaScript's singleton
  numbering is "the knowledge this crate exists without".

That neutrality is exercised rather than aspirational. `Symbol` and `BigInt` are
**kinds declared by the language**, on two of the four tags
`TagRegistry::declare_kind` leaves to a client, and P8 records that nothing in
`rts-core` names either one. The mechanism a second language would use is the
mechanism the first language already uses for two of its own primitives.

So the parameterised surface is: the singleton numbering, the kind tags, the key
numbering (one `KeyRegistry`, minted by the machine), and the shape tree
(`rts_cranelift::shape::ShapeTree`, the machine's, deliberately not a second
one). None of these need changing for a second language.

## What the same rule confesses is not neutral

The second half of rule 4 is the important half:

> Where the language's meaning genuinely lives here — the three equalities, the
> falsy set, what an array index is — it is because those are operations *over*
> values rather than facts about one.

Three named items. The equalities survive a change of language almost intact —
`NaN` unequal to itself, references by identity, strings by content are Lua's
answers too. The other two do not:

| | JavaScript | Lua |
|---|---|---|
| falsy | seven cases | two: `nil`, `false`. `0` and `""` are true |
| array index | `0` to `2³²−2` | 1-based |

`CoreEntry::ToBoolean` documents itself as existing "for one falsy case out of
seven: the empty string" — precisely the case the second language does not have.

The list is short and the items are small. That is the finding: the runtime's
JavaScript content is concentrated in named places rather than diffused, which is
what makes the question answerable at all.

## Reuse is by naming, not by selection

The obvious mechanism — a configuration listing which entry points the second
front end inherits — is wrong, and the reason is a property of the checking that
already exists.

`rts-host/src/entries.rs` is the only place the compiler's statement of the
entry-point set and the runtime's definition of it are both visible, and its
`resolve` match makes each ABI shape a cast so that a signature change on either
side is a type error. What it compares is **shape**. Two languages' answers to
"convert this number to a string" have identical shapes and different values:
`tostring(1.0)` is `"1.0"` where `String(1.0)` is `"1"`, because Lua 5.3 and
later distinguish an integer from a float and JavaScript has no distinction to
preserve. A reuse list that omitted an entry would inherit JavaScript's answer,
pass every check, and be wrong at run time with nothing to report it.

The mechanism that works is the one already chosen. `rts-codegen`'s `RuntimeOp`
links **by name**, and the module says why: the set is "assembled in two places
by construction", so a disagreement is an unresolved symbol at link time rather
than a call to the wrong function with plausible arguments. A second front end
therefore states its own operation set from empty and names a symbol per
operation. Reuse becomes an affirmative act — writing `rts_string_concat` in a
Lua arm asserts that JavaScript's answer *is* Lua's answer, and is reviewable as
one line. Omission fails loudly.

The runtime grows `lua_*` entries beside the existing ones, through the same
`#[rtse::entry]`. No entry point acquires a dialect parameter, which is the
failure this avoids: a function branching on language is a function deciding
language meaning, in the crate whose whole definition is that it does not.

The machine needs no change for any of this. It already refuses to choose:

```text
Inst::Generic(..) => Err(LowerError::NotYetLowered { needs: Capability::Calls })
```

It knows a generic operation is a call and declines to say *which* call, because
which one is a fact about a language. That refusal is the boundary working, and
it is what makes a second client possible.

## What is genuinely expensive, and none of it is front-end work

Four things. The first three are not about the second language at all — each is
owed to JavaScript too, which is what decides the ordering.

**Nothing throws.** `rts-core`'s divergence list opens with it: every
operation that should raise answers a value instead — `undefined`, `NaN`, `null`,
or a clamp — because a throw no handler in the throwing function catches ends the
program, and finding one in a caller needs an exception table and a personality
routine. JavaScript tolerates the substitute, since `NaN` is frequently the
answer the program would have received. Lua cannot: `"" - 1` is an error, not a
value, so the language's semantics would be approximated wrongly rather than
implemented. This is machine work.

**Coroutines, if the second language has them.** Lua's are stackful and resume
across arbitrary frame depth, which is not the transformation `async` and
generators get. It needs real stack switching or whole-program CPS. Also machine
work, and it is the single largest item — larger than the rest of the language.

**The calling convention is per-language and is already in the right place.**
`ARGUMENT_SLOTS = 4` lives in `rts-codegen` because which convention compiled
code uses is a fact about what that crate emits, is restated by the runtime, and
is asserted equal in `rts-host`. That placement is correct and it is also the
first thing that breaks: a language with real multiple returns and varargs wants
a different convention, and the host asserts *one* equality. Two front ends with
two conventions is where `rts-host` stops being generic in fact rather than
in intent.

**Indexed storage does not exist.** `rts-core/README.md` lists it as
deliberately absent, waiting for arrays. This one is smaller than it appears: an
array here is an object with integer-index keys and a `length` that is an own
data property rather than a prototype accessor, so a Lua table and a JavaScript
array are already the same structure. The difference is which integer the index
starts at and who writes `length` — an offset and a property, not a second object
model. The enumeration order C3 owns (index keys ascending, then strings by
insertion, then symbols) is JavaScript's rule, and a language whose iteration
order is undefined simply does not use it.

## Interoperation is a larger claim than a second front end

Calling an existing JavaScript library from the second language — the case that
motivates the question in practice — is not the same project, and it commits to
something the front-end-only version does not: the second language's aggregate
must **be** a JavaScript object, same handle, same shape, same property walk.
That is affordable, and C3 makes it more so than expected. The prototype walk
carries the original receiver and stops on a descriptor rather than a value,
which is exactly the contract Lua's `__index` needs; it would be reused, not
reimplemented.

What it does not make affordable is the ecosystem. A JavaScript library of any
size is asynchronous throughout, so a coroutine-to-promise bridge stops being
optional, and the Node surface has to exist in the new engine at all. Neither is
front-end work.

One notational point, recorded because it is the natural first attempt and it
fails silently. Lua's `a:b()` desugars to `a.b(a)`, which is `[[Call]]` with a
receiver — right for a method, wrong for construction, and a class constructor
invoked without `new` raises. Redefining `:` to mean construction when the callee
is a constructor is implementable, since the test is a flag on the value, but it
has no answer for a `function` that is both callable and constructible, and would
pick one silently. The explicit spelling — index the constructor with `.`, then
call a synthesised `new` with `:`, so the constructor arrives as the receiver and
`[[Construct]]` is named rather than inferred — costs one synthesised member and
stays greppable.

## The conclusion, which inverts the obvious order

Every item on the expensive list is machine work owed to the current language
too: real `throw`, stack switching, a convention that admits more than four
slots, indexed storage. A second front end built before them targets a machine
that cannot express its semantics, and a second front end built after them is
mostly naming.

So the second language is not the way to get those capabilities, and it is also
not blocked on being scheduled: it is what becomes cheap once they exist. What it
is useful for *now* is falsification. A front end of a few operations —
enough to construct an aggregate and index it — that compiles and links without
touching `rts-cranelift` is evidence the boundary holds. If it cannot, the leak
is found at the cost of one file rather than one crate, which is the only reason
to run the experiment early.
