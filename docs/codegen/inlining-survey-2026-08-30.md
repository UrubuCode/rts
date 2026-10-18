# What can be inlined, and what a survey of it found instead

2026-08-30. A survey of every place RTS could inline in the sense a systems
language means it — `#[inline]`, cross-crate and link-time inlining,
monomorphisation, avoiding indirect calls, small-buffer optimisation — rather
than in the sense a JavaScript JIT means it.

**The two most valuable results are defects in code committed the same day, and
neither was found by a benchmark.** They are first because they are the reason
to read the rest.

---

## 0. Two defects in HEAD, found by reading

### 0a. A nested guard clause returns from the CALLER, and the program exits zero

Introduced by `da2f371b`, live at `420267cd`.

```js
function classify(x) {
  if (x > 0) {
    if (x > 10) { return 99; }   // a guard, one level down
  }
  return x;
}
console.log(classify(5), classify(50), classify(-1));
```

    node : 5 99 -1
    rts  : (nothing at all, exit code 0)

`straight_line`'s `_ if guard_return(statement).is_some() => true` arm fires at
ARBITRARY DEPTH, because `straight_line` recurses into `Block` and `If`. But
`emit_substituted` intercepts guard clauses only over the TOP LEVEL of
`candidate.statements`. Anything deeper falls through to `stmt::emit_stmt`,
whose `StmtKind::Return` arm is `builder.ret(&[result])` — **a return from the
caller**. The substituted body returns out of the calling function, then out of
the module body, and nothing after the call runs.

The shape that was tested — a guard at the top level — works. The shape one
level down does not, and it produces no output rather than a wrong value, with a
successful exit status. That is the worst class this repository has a name for.

**The gates did not catch it**: 51 repo tests, 773 corpus files and 1295 Rust
tests all pass at `420267cd`. Every guard-clause test written for `da2f371b`
puts the guard at the top level, because that is the shape the change was
designed around, and a test written from a design tests the design.

### 0b. The `try`/`catch` admission in `420267cd` is inert, and the 4% it reported was noise

`straight_line` has no `Try` arm and no `Throw` arm. The commit says it does.

The patch script that added them died on its second hunk; the shell's `&&`
correctly prevented the write, so hunks one and three never landed and only the
`declared_names` hunk — reached exclusively through statements `straight_line`
has already passed — was applied. Its arms are unreachable code.

The commit then reported `56.25 → 53.75`, a 4% win, measured on `--release` with
three alternations and a flat control. **The measurement was real and the
mechanism was absent.** A 4% reading with no mechanism is what layout noise looks
like, and it was believed because a small win was what the change was expected
to produce.

The lesson is the one `entry-tax.md` part nine already states from the other
direction — *the position of a `match` arm is load-bearing, and no test can see a
pass that silently refuses* — with a second half now attached to it:

> **Check that the mechanism is present before believing the number.** `rts ir`
> answers in seconds and would have said so both times.

---

## 1. The method

Ten independent angles, each an agent that had to quote what it read. Fifty
candidates. Each then faced three refuters — correctness, architecture,
measurement — instructed to default to refuted. A candidate needed two of three
to survive.

**Thirteen survived. Thirty-seven were refuted.** 161 agents, 4 389 file reads,
about 100 minutes.

The refutations are the more valuable half and are listed in full below, because
each is a day of work that does not have to be spent.

### The universe, counted independently before reading any result

| | |
|---|---:|
| `RuntimeOp` variants | **95** |
| `CoreEntry` numbers | 95 — they agree, so no drift |
| IR instructions the machine has | **71** |
| operations exempt from the throw check | 21 |
| `#[inline]` attributes in `rts-core` | **4**, against 792 public functions |
| `#[inline]` in `rts-cranelift` / `rts-codegen` | **0** / 0 |

---

## 2. What the survey confirmed about the shape of the problem

The established constant is that **a runtime crossing costs about 15.7 ns and the
Rust body of a typical operator costs about 1.5** — so roughly ninety per cent of
what an "operation" costs is the door. An operation the machine emits as an
instruction costs **0.00 ns** over an empty loop.

That is why the entry-point membership rule matters more here than it reads:

> an entry point exists if and only if the operation touches the heap, the
> operating system, or global mutable state. **Pure computation is instructions.**

Every surviving candidate is an instance of a value crossing that door carrying
a proof the far side then re-derives, or not carrying one it already had.

---

## 3. The thirteen that survived

Ranked as the completeness critic ranked them, not as they were found.

### First: the proof-carrying boolean (candidates 1, 2, 4 — one change)

**Measured**, `--release`, min of 7, 10–20 M iterations, identical loops
differing in one operator:

| | ns/iteration |
|---|---:|
| `if (o.f)` | 6.35 |
| `if (!o.f)` | **10.55** |
| `if (o.f === k)` | 4.10 |
| `if (o.f !== k)` | **7.55** |

So **3.5 to 4.2 ns on every negated condition the language has** — `!x`, `!==`,
`!=`, `x !== undefined`, `typeof x !== "…"`, `x != null`.

The cause is one line. `emit_guarded` gives its join a `Repr::Bool` parameter
only when `matches!(instruction, Proven::Compare(_)) && !negated`. The fast path
for `!==` already emits `Compare(Ne)` and already produces a proven boolean —
`proven_binary` maps `StrictNotEqual` to `Proven::Compare(CmpOp::Ne)`. It is the
SLOW path that spoils it: strict equality is stated once, so `!==` has no entry
point of its own, and the runtime's answer is inverted by `choice::from_bool`,
which builds a three-block diamond of **tagged** boolean constants. The join
therefore has to be `UNPROVEN`, and `to_boolean` — which shortcuts `Repr::Bool`
and `Widen(Bool)` and nothing else — misses, so every such condition calls
`__rts_to_boolean` to recover a proof both incoming edges already had.

A census over 400 corpus files puts **1 079 `__rts_to_boolean` sites, of which
328 are this shape and 307 are the diamond** — 59% of them.

`from_bool`'s own comment says why it is a diamond:

> negating a proven boolean is arithmetic, and this module has no unary path yet

That is **stale**. `Inst::Compare` is verified with `Domain::Any`
(`verify/rules.rs:673`), `same_proven` accepts any equal non-`Tagged` pair,
`Repr::Bool` lowers to `types::I64` (`lower/types.rs:35`) and a scalar constant
of any non-float repr lowers to `iconst` (`lower/body.rs:676`). So
`Compare(Eq, value, false)` is one instruction the machine already has, and
negating a proven boolean is one `icmp`.

The one prerequisite the candidates did not state: the language layer cannot
name the operand, because a proven boolean's bit pattern is the machine's to
decide (`lower/value.rs` widens with `select(v, TAG_BOOL|1, TAG_BOOL|0)`). So
`FuncBuilder::bool_constant(bool)` has to exist first — the machine answering a
machine question, which rule 2 permits and requires.

**Verified by hand before any of it was built**: the three machine facts above
were read rather than taken from the agents.

### Second: `typeof x === "number" | "boolean" | "undefined"` as a tag test

`typeof x === "number"` measures 15.67 ns absolute against a 4.33 floor, and the
answer is decidable from the TAG with no heap access at all. `lower/value.rs`
already emits `has_tag(TAG_BOOL)`; `undefined` is `is_singleton`, used two
functions above the site; `number` is `Kind::Float | Kind::Int`, the same
two-test disjunction `test` already gives.

**Scoped deliberately.** `symbol` and `bigint` are also tag-decidable, and are
NOT free: their tag numbers are runtime values (`context.kinds.symbol`), so the
emitter would need a new compile-time agreement asserted in `rts-host` — the
singleton-numbering shape of work — for two spellings almost nobody writes.
`string`, `object` and `function` genuinely need the cell header and stay a call.

Corpus frequency, counted: `function` 49, `number` 45, `object` 43, `string` 22,
`undefined` 12, `bigint` 8, `symbol` 6, `boolean` 3. So the tag-decidable three
are 60 of 188 occurrences, and the two that need new machinery are 14.

### Third: `switch` bypasses every settlement `emit_binary` has

**Measured**: `switch (typeof o.f)` with two labels, 44.2 ns/iteration, against
the same test written as an `if` chain at 32.4 — and the `if` chain evaluates
`typeof` **twice** and still wins by 11.8 ns.

`switch.rs` calls `strict_equals_proof`, which consults `proven_binary` and
nothing else. `settled::singleton_equality` and `settled::typeof_equals_literal`
are reachable only from `emit_binary_inner`. A `switch` label is `===` with no
coercion — exactly what both already implement — so routing the chain through
them is the whole change.

**And this is one instance of a general omission, not a finding of its own.**
The same shape appears at `delegate.rs:92-98`, `delegate.rs:125-130` and
`destructure/array.rs:218-226`. The fact to write down once is:

> a comparison not built by `emit_binary_inner` is unsettled

Expect more: nothing prevents the next desugaring from doing it again.

### The rest, in brief

| # | what | measured or estimated |
|---|---|---|
| 5 | `while (true)` calls `__rts_to_boolean` on a constant, every iteration | **+3.6 ns/iter measured** (5.6 vs 2.0 for `for(;;)`) |
| 8 | a nested `function` declaration forces its enclosing function to build an environment object even when nothing recurses | **~20 ns per call measured** |
| 3 | `type_of_is` re-derives a compile-time-constant string comparison | 5.6–7.8 ns, and see the caveat below |
| 10 | a void helper — no trailing `return` — is refused outright; 11% of named functions | ~16 ns per call |
| 11 | `closed_over` refuses assignment to a body LOCAL, and its justifying comment is false for locals | ~16 ns, and it is the prerequisite for every accumulator and every loop body |
| 13 | a property more than two links up refuses the chain cache permanently, then pays TWO crossings | 5–8 ns per access |
| 12 | the statement budget of 8 bounds the wrong quantity | not a run-time number; 99.4% of bodies are ≤ 7 statements, so the budget is inert |

**Candidate 3 is the one most likely to be wrong.** Its 7.8 ns is derived as
`11.52 − 3.68`, but the two sides differ by more than the string comparison:
`type_of_is` also does three side-table lookups. And `settled.rs`'s own
2026-08-29 measurement puts bare `typeof` at **8.3 ns, not 3.68** — nobody
reconciled the two. Build candidate 9 first, then re-measure 3 before believing
it.

---

## 4. The thirty-seven that were refuted

Listed because each is work that does not have to be done. The vote is out of
three.

**Refuted on measurement — the number is already known, or is zero**

- (3/3) **A direct call instead of an indirect one.** Already measured at exactly
  zero: `entry-tax.md:293-305`, "a DIRECT call, empty body 1.154" against "an
  INDIRECT call, empty body 1.154".
- (3/3) **`string_const` as a `WordLoad`.** Struck once already at 0.33 ns, and
  the refutation adds the reason the earlier one missed: the shape it proposes
  is guarded by `if !body_suspends(body)`, because `frame::resumable_form`
  rewrites a suspending function around every suspension — which cost 37
  generator files in one run when `flag` learned it.
- (3/3) **A cached STORE past the fifteenth property.** Census: 3 fires in 2
  files over 400 tests, none in a loop, zero in either benchmark. Total measured
  exposure across the suite is about a microsecond, once.
- (3/3) **Marking the AOT heap-base load `readonly`.** Proof of inertness rather
  than a risk of it: Cranelift gates the entire egraph pass on
  `opt_level != None`, and RTS sets `opt_level` only from an env var.

**Refuted on correctness — it changes what the program does**

- (3/3) **Turning on Cranelift's optimiser for AOT.** Enabling the egraph mid-end
  changes which SSA values are live at which point, and this engine finds
  compiled-frame GC roots **by bit pattern** from a conservative stack scan. The
  worst class this repository has.
- (3/3) **A direct `Inst::Call` between two emitted JS functions**, and (3/3)
  **`CallIndirect` with a monomorphic call-site cache**, and (3/3) **C++-style
  function versioning**. All three fail the same way: `entry::called`/`invoke`
  is not a door, it maintains three per-activation stacks JavaScript observes —
  `context.callees` IS the call stack that `.stack` prints, `new.target` is
  decided by `depth + 1 == callees.len()`, and `RunningFunction` answers
  `callees.last()`. A call that skips them is not a faster call, it is a
  different language.
- (3/3) **Emitting a body twice (IPA-CP cloning).** Emission is not a pure
  function of the AST — `Ctx::template` deliberately mints a fresh identity per
  site, because the specification gives each tagged template its own strings
  object. Two emissions of one body are two observable identities.
- (3/3) **Omitting a `const f = <function>` whose every use is substituted.** The
  refusal set is not in one file: `emit_substituted` is gated from OUTSIDE by
  `ctx.with_objects.is_empty()`, which a remedy written against `inline.rs`
  structurally cannot see.
- (3/3) **Deciding `prototype` by a whole-program question rather than source
  form.** The class lowering reads the flag's product with no syntax to scan for.
- (3/3) **Declaring `Ref(Opaque)` returns on operations that can only answer a
  reference.** `entries::agree()` compares signatures by equality and the host
  rejects the program.
- (3/3) **A self-contained loop in an inlinable body.** Refuted because it widens
  the miscompile of §0a rather than for its own sake — which is how that defect
  came to light.

**Refuted on architecture or on price**

- (3/3) `opt-level = "z"` for everything but `rts-core`: the facts are right and
  the edit is not behaviour-neutral, because this engine's GC has no precise
  roots and an optimisation level changes what the conservative scan sees.
- (3/3) A separate tag for strings: the larger half of its estimate was already
  collected by `Context::is_text_at` in part eight of `entry-tax.md`.
- (3/3) `#[inline]`/`#[inline(never)]` splits of `to_primitive` and `as_number`,
  and `#[cold]` on `with_current`'s abort arm. The release profile is
  `lto = "thin"` with `codegen-units = 1`, so LLVM already has what the attribute
  would give it.
- (3/3) A general dead-code elimination pass. **The interesting refutation.** A
  census found 11 908 calls with all results unused — and 5 253 are
  `__rts_call_counted` (every expression statement `foo();`), 2 088
  `__rts_set_indexed`, 1 611 `__rts_define_method`. Deleting any of them changes
  the program. Liveness was never the missing ingredient.

---

## 5. The angle nobody ran, and it is the machine-language answer

Every candidate above removes ONE crossing, at emit time, by hand, one syntactic
construct at a time. That is a programme of special cases over a corpus of 1 710
functions.

**The C and C++ answer for a call you cannot inline is not to inline it. It is to
DESCRIBE it** — `__attribute__((const))`, `__attribute__((pure))`, LLVM's
`readnone` / `readonly` / `speculatable` / `willreturn`. You do not inline
`strlen` at `-O2` in a non-LTO build; you annotate it, and the caller's mid-end
then CSEs it, hoists it out of loops, sinks it past branches, and deletes it when
the result is unused.

That is exactly this engine's situation: a fixed `extern "C"`-shaped door and a
body the compiler will never see.

**The repository diagnosed the disease and filed the wrong prescription.**
`crates/rts-cranelift/src/target/mod.rs:1065-1080` says:

> Cranelift's default is `none`, which gates out the WHOLE egraph mid-end: no
> GVN, no LICM, no redundant-load elimination… The mid-end cannot optimize across
> an opaque call, and this engine's IR is mostly opaque calls, so there is little
> for it to see.

It concluded *the knob is worthless* instead of *the calls are undescribed*.

Evidence it is absent rather than merely unmentioned:

- `runtime/raising.rs` is the **only** per-operation property list in the engine,
  and it describes a CONTROL effect ("can this fill the thrown slot"). There is
  no memory-effect list — no `READS_NOTHING`, no `WRITES_NOTHING`, no
  idempotence.
- `emit_guarded` says it outright: *"Nothing removes it for us… A value computed
  on the fast path and used only on the slow one stays exactly where it was
  put."* The emitter is **hand-sinking a widen** because no sinking pass exists.
- `grep` for LICM, CSE, GVN or invariance across both engine crates returns
  nothing. Every `hoist` in `rts-codegen` is JavaScript declaration hoisting.

It also explains the dead-code refutation above rather than contradicting it: DCE
cannot delete a `Call` with an unused result unless something asserts the call
has no effect. The survey refuted the **pass** and never noticed that the missing
precondition was an **annotation**.

Where it lands without breaking the architecture: the classification is the
language's (`RuntimeOp`, beside `raising.rs` — "what does this operation do" is
exactly what that crate decides); the flag rides on `Inst::Call`; the pass lives
beside `ir/fold.rs`, because an effect is a machine question. `ToBoolean`,
`TypeOfIs`, `StringConst` and `IsSingleton` are already in the class today.

**The falsifier to write first**, before any of it:

```js
for (let i = 0; i < N; i++) if (typeof x === "string") s++;   // x invariant
```

`rts ir` must show `Call TypeOfIs` **once**, not N times. Loop-invariance is not
a property of a syntax node, so the emitter has no vantage point from which to
see this — which is the whole argument for the annotation over another special
case.

### A second un-run angle: hot/cold layout

`__builtin_expect`, `.text.unlikely`, `-freorder-blocks-and-partition` — applied
to GENERATED code rather than to Rust.
`cranelift_frontend::FunctionBuilder::set_cold_block` exists in the pinned 0.131
and this repository calls it **zero times**. Meanwhile `raising.rs` measures the
throw check alone at **1 423 of 6 164 blocks in `analytic.ts`, about 46% counting
continuations** — and every raise block, every guard slow path and every
cache-miss block is laid out inline in the hot path.

This is the dual of inlining: C++ inlines aggressively precisely because it can
then sink the cold half out of the instruction cache. Every candidate above makes
it matter more.

---

## 6. One stale comment found on the way, and it is load-bearing

`emit/inline.rs`, above `straight_line`:

> `Declare` is absent, and it was there for one build… a body with no
> declarations is the whole of what can be spliced

The code four lines below admits `StmtKind::Declare` for non-`var` single-name
bindings with an initialiser, guarded elsewhere by a one-declaration-in-the-
program check. Candidate 11's framing depends on which of the two is current.

That is the contradiction CLAUDE.md says never to leave standing, and it is the
same failure that produced §0b: **a comment that describes an intent is read as
a description of the state.** This survey found three of them — this one, the
`from_bool` "no unary path yet", and the `is_singleton` comment that claimed
defaulted parameters had been moved over when they had not.

---

## 7. What this says about method

- **Ten angles found what one would not.** The two most valuable results —
  both defects in code committed hours earlier — came from angles aimed at
  something else. §0a surfaced while refuting the loop candidate; §0b surfaced
  while auditing what `try` admits.
- **Refutation is the productive half.** Thirty-seven of fifty died, four of them
  against measurements this repository already had and had not been consulted.
- **A count is not a clock.** Every census here — 1 079 `to_boolean` sites,
  11 908 dead-result calls, 230 `string_const` sites — is a reason to measure,
  never a result. The one ranked item `entry-tax.md` struck earlier died exactly
  that way.
- **And check the mechanism before believing the number.** Both §0a and §0b would
  have been caught by one `rts ir` run costing seconds.

---

## 8. Nothing here has been built

The working tree is at `420267cd` with no uncommitted changes. The one edit that
had been started — `FuncBuilder::bool_constant` for the proof-carrying boolean —
was reverted when this document was asked for, so the first item of §3 is
designed and verified and not written.

Two items are not optimisations and should be taken first regardless of what is
decided about the rest: **§0a is a live silent miscompile** and **§0b is a commit
message that describes a mechanism the code does not contain.**
