# rts-profile — what a site saw, and how often

The record between two runs of a program. `rts-core` writes it from the path a
cached access already falls down; `rts-codegen` reads it before lowering and
turns a frequency into an assumption its own `Domain` can interpret.

It exists because the guard, the two tiers and the lateral fall are all built
and **none of them knows what to bet on**.
`docs/engine/profile-oracle.md` is the design, including the three levels of
that question and why only two of them are neutral.

These rules are binding for changes inside this crate. RULE 0 of the root
`CLAUDE.md` applies: read this file in full first.

---

## 1. This crate never names a language

Not JavaScript, not Lua. No type, no truth rule, no index base. The record holds
names it does not interpret and counts it does not judge.

Checked by `tests/neutrality.rs`, which **failed on this crate's own first run**
— a sentence in `lib.rs` naming a language while explaining that it must not.
That is the argument for the test rather than the grep: `rts-mir` states the same
rule as a grep a person runs and is clean, `rts-cranelift` had no grep and had
five leaks, and this crate wrote one while being written.

## 2. It counts; it does not type

There is no `should_speculate`, no threshold, no "is this monomorphic". A
[`Majority`] is a count beside a total and the division is the reader's.

Why: what a 90% majority authorises is a judgement about one language's
semantics — how cheap its fall is, how much its specialised tier gains — and a
judgement taken here would be one every client inherited. It is the mistake
`rts_cranelift::observe` declines to make about sampling policy, in its own
words: *"being a profiler is not a machine-level capability."*

The mechanical form of the rule: **if a field could only be filled in by a
language, the field is in the wrong crate.** `tests/toy_oracle.rs` holds the
threshold and the meaning *in the test*, which is what makes that checkable.

## 3. A key is never a number this workspace mints

`CacheId` is a dense table index. `KeyRegistry` issues keys in the order one
compilation asks. A shape id is created as the program grows. A record keyed by
any of them is valid only for the compilation that wrote it — and reads back
**pointing at other sites** rather than failing, with every guard still passing.

So a site is a `ModuleId` and a `Position`, because a position is already
defined as *"a number the client gave us"* that the machine never interprets,
and the client is the only party able to make it stable. Reusing it also keeps
this crate from being a second numbering of one space.

## 4. An edited module has no record

`ModuleId` is derived from the source text, so an edit answers a different
identity and `Profile::knows` says no. The cost is stated in that type's own
documentation: editing one line discards that module's observations entirely.

That is taken deliberately. A smaller unit would have to decide which positions
still mean what they meant, which is the question with no answer, and refusing
is the only behaviour that cannot mislead.

## 5. A record is deterministic and a person can read it

Sites in key order, witnesses by descending count with ties broken by the
witness. Two runs that saw the same thing write byte-identical records — the
same rule `rts-cranelift` states as its rule 13, for the same reason: a diff
that changes for no reason is a diff nobody reads.

Text rather than binary because the first thing anyone does with a profile is
disbelieve it, and a binary record needs a tool before it can be doubted.

## 6. What cannot be written is refused where it is written

A name containing whitespace is a `FormatError::UnrepresentableName` from
`write`, not an escaping rule invented under the reader. A version that is not
`FORMAT_VERSION` is refused by name, never interpreted as best it can be.

Why: an escaping convention is a second thing two implementations must agree
about, and a client whose names can contain whitespace needs a decision taken
here rather than a quoting rule guessed at.

## 7. Files ≤ 500 lines

Everything outside the two engine crates, per the root `CLAUDE.md`.

---

## What is NOT here, and is the next question

**Nothing writes a record yet, and nothing reads one.** This crate is the format
and its invariants; the two ends are named work and not done:

- the **writer** — a `Recorder` reached from the cached-access miss path in
  `rts-core`, where the name looked for and the layout found are already in
  hand. That is the reason no instrumented tier is needed, and it is also the
  reason this is cheap: the cost is a counter at a point the runtime is already
  standing on.
- the **reader** — in `rts-codegen`, producing `Assertion`s for `rts-mir`'s
  guards from a majority plus whatever static evidence that crate already has.

**The measurement has been taken, and it says not yet.** `rts prove` over the
thirteen programs of `bench/` on 2026-10-03: of the 3 895 runtime operations on
the settled path, 2 971 — 76% — are a string literal, a call that is not direct,
a post-call throw check, an allocation and a global lookup. None of those is a
question an observation answers. What a `Domain` would narrow is about 5.3%, and
a static type answers most of it.

And `calls 0 direct, 0 through a value` in all thirteen: 831 calls, none direct.
The static producer comes first and is strictly larger.

So this crate is parked as a finished format rather than built out, and
`docs/engine/profile-oracle.md` carries the figures and the limit on reading
them — `prove` counts occurrences, not executions. It becomes worth wiring when
a report is dominated by genuinely polymorphic sites, and the same command
answers that.

## Working on this crate

`cargo check -p rts-profile`, `cargo test -p rts-profile`. Both are seconds, and
the crate has no front end above it by construction — rule 3 of
`rts-cranelift`'s README, applied here because it is the property that makes the
neutrality claim testable at all.
