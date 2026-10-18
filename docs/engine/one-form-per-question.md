# One form per question: the canonical answer, and which actions use it

`principles.md` says what the rules are. This says which form is canonical for
each question the runtime asks more than one way, which actions must use it, and
why the other forms existed — because a form removed without that last part comes
back.

Every entry has the same four lines, and the third is the one that matters: a
form is canonical **for a stated set of actions**, not in general. Two forms are
correct when they answer two questions; they are a defect when they answer one.

Audited 2026-10-01. Where a row says *to be built*, the canonical form does not
exist yet and the row is the specification for it.

---

## "Can this be a machine instruction"

Three things get asked as one question and they have three different homes. The
confusion is cheap to have and expensive to act on, so the taxonomy is here
rather than in anyone's head.

| | what it is | who owns it | how it is reached |
|---|---|---|---|
| **an instruction** | what the CPU does: `sqrt`, `popcnt`, `bswap`, a SIMD lane, a fence | `rts-cranelift` | `rts-codegen` names the operation, the machine emits it |
| **an OS or platform call** | a notification, the clipboard, the registry, a file dialog | the platform — **not the machine** | a foreign call: the *convention* is the machine's, the *symbol and library* are the target's. `rts-napi` already reaches one (it loads a real `.node` addon); a direct `rts:ffi` is the general form |
| **an `rts:*` module** | `rts:audio`, `rts:egui`, `rts:dom`, a future `rts:notify` | its own crate, if it wants one | `rts-runtime-boot` installs it — it already calls `rts_ui::install` and `rts_dom_bridge::install` |

**A Windows notification is not "a caller in the machine".** It is a foreign call,
and the only machine involvement is the calling convention. Nothing about it
belongs in `rts-cranelift`, and nothing about it needs to be in `rts-std`: the
availability rule constrains **`rts-core`**, which must exist on every target
including wasm, and says nothing about a module crate that is simply not installed
where it cannot run.

**And the triage rule, which decides whether any of this is worth doing.** A
notification costs milliseconds inside the OS, so the call overhead around it is
noise; turning it into anything cheaper is work with no effect. Converting an
operation into an instruction pays **only where the operation is arithmetic and
sits in a loop**. So triage by where the time goes, never by whether something
*could* be an instruction.

By that rule the live case is `rts:num`: twenty-one operations, almost every one a
single CPU instruction (`popcnt`, `lzcnt`, `tzcnt`, `rol`, `bswap`, an add with the
overflow flag, a `movq` between a float and an integer register), and every one
costing a property read plus a call today — while `Math.clz32`, the same kind of
thing, already lowers to an instruction. One category, two treatments.

---

## Where per-value state lives

**Canonical: in the cell when every value of that kind has it; in ONE indirection
when it is rare or genuinely foreign.** The dividing line is **frequency**, not
mechanism — and today the split follows neither, it follows what was quickest to
write.

**Why there is a line at all, rather than "everything in the cell".** A slot
reserved in every cell for something almost no value has costs eight bytes per
object across the whole heap. .NET makes exactly this trade: rare per-object
state — the lock, the hash code — lives in a *sync block* reached by an index in
the header, because putting it in every object is worse. Hermes does the same.
So some indirection is correct, and the question is which.

**Why the line is not where it is today.** The structural reason the payload was
outside expired on 2026-10-01: the region holds tagged words and the tracer
walked all of them, so a Rust `Vec` in a cell would have been followed as a
reference. With `gc::traces_field` and an honest layout, **a slot declared
non-GC-relevant is skipped**, which is what made the payload admissible. What
remains is not a technical reason — an `Aside<T>` costs one line in `Context`,
and that is how there came to be twenty-three of them.

Counted 2026-10-01: **23 `Aside` plus 4 `Slab` = 27 places that name a cell.**
By what they hold:

| | how many | examples | belongs |
|---|---:|---|---|
| one or two words | ~13 | `prototypes: u64`, `callables: (u64,u64,bool)`, `proxies`, `boxed`, `array_elements: Slot`, `detached: bool`, `derived`, `chain_links`, `foreign: usize` | **slots in the cell**, with the layout declaring each repr |
| a variable-length list of VALUES | ~9 | `arrays: Vec<u64>`, `accessors`, `attributes`, `collections::Table` (five `Vec`s), `generators::State` | **a spanning cell in the region** — `alloc_spanning` exists and `spill_set` already grows one by copying |
| an opaque Rust object | ~4 | `regexes: Regexp` (a compiled `regex::Regex`), `bigints`, `cells: Slab<Str>`, `foreign` | **one** indirection, keyed by cell |

Applying the frequency rule to the first two columns splits them again, and the
split is by KIND rather than by object: `callables` is on every callable,
`array_elements` on every array, `collections` on every `Map` and `Set`,
`views`/`detached` on every typed array, `generators` on every generator — those
are "always, for that kind" and belong in the cell. `accessors`, `attributes`,
`integrity`, `bound`, `cursors` and `proto_types` are carried by almost nothing
and belong in a table.

**What this buys, and it is three things rather than one.** A mover goes from ~27
`relocate` arms to about three, which is the cost `principles.md` P3 says Go and
C# do not pay. The `lost-roots.md` class shrinks from "every new table is a fresh
chance to be missing from a list" to "there is one table". And the two cold cache
lines per string — 36 ns of the 70 measured on 2026-09-30 — become one.

**And it is what makes `gc::traces_field` load-bearing.** It skips one or two
declared words per cell today, which measured neutral; with the payload inside
and declared, it skips fifteen.

---

## Obtaining a cell

**Canonical: to be built** — one `allocate(context, Want) -> u32` where `Want`
carries the shape wanted (narrow, wide by a slot count, spanning), which side
tables to register, and whether the values handed in are already rooted. It dies
on exhaustion, uniformly; the overflow slot's `+1` and the growth policy are
computed inside it.

**Used by: every allocation in `rts-core`.** No exceptions — `global.rs`'s direct
`region.alloc` is the one left and it is the defect that was removed from seven
other sites.

**Why the others existed.** `alloc_or_die` versus `alloc_after_collecting` is a
failure-mode adapter; `plain` / `built_in` / `allocate_array_cell` are
zero-argument sugar over a `_with_room` twin; `object_new_in`, `object_new_wide(_,
0)` and `modules::make_object` are three independent spellings of "an empty object
at the root shape" that differ only in which of `layout_of`/`empty_layout` they
ask. All of that is convenience and can be `#[inline]` one-liners over the
canonical form.

**What is NOT convenience and must become a field of `Want`:** the collector
contract. `built_in_with_room` inserts the element store *before* allocating the
cell; `built_in_rooted` and `built_in_from` deliberately invert that so the values
stay reachable across an allocation that can collect. Today a caller picks between
them by name, and picking `built_in` with values in a bare `Vec` reproduces the
corruption `rooted.rs` was written for. A contract chosen by a name is a contract
nobody checks (`principles.md` P2).

---

## "Is this an array"

There are three real questions here and seven forms answering them.

**Canonical, for "does this cell hold its own elements" — `context.elements_at`.**
Used by: every fast path that is about to read or write the element vector, and
every runtime operation deciding whether the element store exists.

**Canonical, for the language's `IsArray` — `array_proto::construct::array_in`.**
Used by: every place the specification says `IsArray`, which is `Array.isArray`,
`JSON.stringify`'s choice of shape, `Object.prototype.toString`'s tag, the array
branch of structured clone, and error messages that name what a value is. It
pierces a proxy to its target and answers `None` for a revoked one, which is what
the specification requires and what the marker form cannot see.

**Canonical, for "is this cell at the array layout" — the header compare against
`context.array_layout`, and it is a question about the FAST PATH, not about the
value.** Used by: exactly the two sites that need to know whether a cached slot
offset applies. It answers false for `Array.prototype` itself and for any array
that gained a property, and both are correct answers to *that* question and wrong
answers to the other two.

**Why the others existed.** `modules::is_array_in` is a host-surface wrapper on
the marker; `extends_class(cell, "Array")` answers "does it inherit from
`Array.prototype`", which is a third question again (true for
`Object.create(Array.prototype)`, false for an array whose prototype was
replaced); and `object_tag`'s proxy walk is a second copy of `array_in`'s loop
**without** the revoked-handler check, which is a divergence and not a question.

**The defect this row is for.** `json/write/shape.rs` uses the marker where the
specification asks `IsArray`, so `JSON.stringify(new Proxy([1, 2], {}))` answers
`{"0":1,"1":2}` where Node answers `[1,2]`. Measured 2026-10-01.

---

## "Is this a string"

**Canonical: `Context::text_at`** — it resolves the slab, which is what every
caller that is about to read characters needs.

**`is_text_at` is canonical for one action only: deciding a cell's KIND without
touching the slab**, which is what a cache resolver and a sweep need. Its one
divergence from `text_at` is a cell whose header says text and whose slab slot
cannot be read, and that cell is a bug elsewhere, not a string.

**`shape_of(ty).is_none()` is canonical for nothing.** It means "this cell has no
property shape", which is true of a string **and** of a callable, and reading it
as "is a string" is a conflation with no diagnostic. Where a caller wants "not an
ordinary object", that is the name it should ask by.

**A wrapper is not a string.** `new String("x")` answers false to all of the
above and that is correct; `primitive_proto::unwrap` is the form for "does this
object stand for a primitive", and only `JSON` and coercion want it.

---

## Reading a text ARGUMENT

The entry above is about a CELL — whether this thing is a string. This is about a
CALL: a native has a `u64` in an argument slot and wants text out of it. Three
questions, and the module that reads an argument has to know which one it is
asking, because the answers differ for `undefined` specifically.

**Canonical, for a parameter WebIDL declares as `USVString` and required —
`entry::usv_text_of`, with no absent check in front of it.** Used by: `URL`'s
`url`, every `name`/`value` of `URLSearchParams`, `TextEncoder#encode`, `Blob`'s
parts, and anything else whose IDL says `USVString`. `ToString` runs on whatever
arrives, so `undefined` is the five-character string `"undefined"` —
`new URL(undefined, "http://h/")` is `"http://h/undefined"` in every engine — and
an unpaired surrogate becomes `U+FFFD` rather than a refusal.

**Canonical, for a parameter that must BE a string — `entry::string_in`, and the
caller RAISES `ERR_INVALID_ARG_TYPE` naming the argument.** Used by: every path
argument of `node:path`, `querystring.parse`'s subject, and any `node:` surface
whose documented contract is a type check rather than a conversion. Node's
message names the argument and not always `"path"`: `relative` says `"from"` and
`"to"`, `basename`'s second parameter is `"suffix"`. A message naming the wrong
argument sends a reader to the wrong line.

**Canonical, for a parameter with a DEFAULT — the absent check BEFORE the
coercion, then either.** Used by: `console.time`/`count`/`group`'s label,
`querystring`'s `sep` and `eq`, `basename`'s `suffix`, `URL`'s `base`. The
language fires a default parameter on `undefined` specifically rather than on
"the argument was omitted" — `function f(x = 1)` defaults for an explicit
`f(undefined)` — so the check is `value == undefined_value()`, and it has to come
first.

**`entry::text_of` is canonical for nothing by itself.** It is `ToString` with a
refusal for objects, which is the right MECHANISM for two of the three questions
and the right ANSWER to none of them on its own: it never returns `None` for
`undefined`, so a `text_of(x).unwrap_or(default)` is a default that cannot run.
There were 26 of those in this workspace.

**Why the others existed.** Three modules wrote the same private helper —
`url/mod.rs::text`, `path.rs::text`, `querystring.rs::argument_text` — each
spelling "`undefined` is absent", and `url/mod.rs`'s own comment says it is "the
same convention" as the other two. One convention answering three questions, and
the duplication made it look settled.

**Does a native know whether an argument was PASSED?** Yes, and this was got
wrong in #2850's first write-up. `functions::call_counted` carries the count as
an operand from compiled code, `called` leaves it in `context.pending_counts`,
and `arguments::with_arguments_at` reads it — which is what makes
`[].push(undefined)` push one. The `None` that `functions::call` pushes means
"a native called this, and that native had no count to declare", not "unknown in
general". Note that `arguments_at`'s own doc still describes the pre-count state
and names `a.push(undefined)` as a live divergence; it is not one. Measure before
believing a comment.

**The defect this row is for.** Ten of thirteen text-argument readings diverged
from node 22, in BOTH directions: `URL`, `URLSearchParams`, `querystring.escape`
and the global `console`'s label refused where Node coerces; `path.join`,
`path.basename`, `path.resolve` and `querystring.parse` coerced where Node
refuses. `path.join(undefined)` answered `"."` because the variadic collector
`filter_map`ed the `None` away — the count was right and the reader erased the
argument. Measured 2026-10-02, fixed the same day;
`tests/claude-text-argument-reading.test.ts` is the ruler.

---

## "Is this callable"

**Canonical: `Context::callable_at`**, for every action that is about to call
something it already knows is a plain callable.

**Canonical for the language's `IsCallable` — the form in `functions.rs` that also
accepts a proxy and a bound function.** Used by: `typeof`, `instanceof`, the
thenable protocol, `Array.prototype`'s callback checks, and any refusal message.
A callable proxy and a bound function are callable in the language and the short
form misses the second.

**`is_class_constructor` is the only form that knows `C()` must throw**, and the
actions that must ask it are every call site that did not come from `new`.

**Why the others existed.** `callable_at` is spelled out by hand in five separate
files; that is five copies of one predicate, not five questions.

---

## Reading a property

**Canonical: one `read(context, cell, key, Accessors)`** where `Accessors::Run`
resolves getters and `Accessors::Skip` does not — *to be built* as the merge of
`objects::read_property` and `accessor::resolve`, which walk the same chain and
differ only in that one sees accessors.

**`Run` is the default and `Skip` requires a reason at the call site**, because
the failure direction is asymmetric: skipping a getter that exists answers
`undefined` for a property the program defined, silently. Two call sites in
`array_proto` already carry comments saying they picked wrong.

**Why `Skip` must still exist.** A getter is user code, and user code cannot run
inside the `RefCell` borrow that found it. A caller holding that borrow genuinely
cannot run one, which is why `accessor::resolve` hands the getter back *unrun*
rather than calling it — and that is the shape to keep: the borrow ends, then the
call happens.

**`own_property` is canonical for one action: "own-ness is the question"** —
`hasOwnProperty`, `getOwnPropertyDescriptor`, the first half of the accessor
walk. There are three implementations of it today and two are copies.

**The four cache doors are not duplicates.** `cache_resolve`,
`cache_resolve_indirect`, `cache_resolve_store` and `cache_resolve_keyed` answer
four different questions — an own slot, one prototype link out, a write, a key
that is a value — and each is a different machine shape at the site. They stay.

---

## Reaching a built-in method

Four mechanisms, in the order a call site should try them, and all four stay:

1. **A compile-time tree rewrite**, where the whole call can be erased
   (`class_layout`). Cheapest, and only available when the receiver provably never
   exists.
2. **A typed door** (`RuntimeOp::…Direct`), where the receiver's kind is proven
   and the arity matches. Removes the property read and the call.
3. **The inline cache plus `call_counted`** — the default, and correct for
   everything.
4. **A hand-rolled fast path inside the native**, which is the *fallback* of a
   door: the door was taken, the receiver turned out not to qualify, and the
   native re-routes through the member.

**What is canonical is not the mechanism, it is the PREDICATE.** Each claim a
mechanism rests on — "this is the language's `Function.prototype.call`", "this
receiver is an ordinary array", "this function is plain" — is stated once and
called by every layer. Today `f.call` rests on three different predicates that
check different things, which is the defect; `gc::traces_field` is the shape of
the fix.

---

## Which words a collector follows

**Canonical: `gc::traces_field`** over the field the cell's layout declares, with
`true` — follow it — for anything undeclared.

**Used by: both sides.** `gc::barrier_for` for a store and `trace::edges_of` for
a walk, and the debug assertion in `edges_of` refuses a field whose declaration
disagrees with its contents. This row is the one already finished, on
2026-10-01, and it is in the document as the worked example of what the others
should look like.

**Why there was no other form.** There was no form at all: the tracer pushed
every slot of every cell, and the machine had already answered the question for
the store side alone.
