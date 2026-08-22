# The documentation

Five places, and a rule for each. The rule is the point: this tree was thirty
loose spec files with overlapping and partly-stale content, and the failure was
not any one document — it was that nothing said where a new one belonged, so
every new one went next to the others and the pile grew.

```
docs/
  engine/      how the compiler works, and why it is built that way
  guides/      how to do a thing
  reference/   surfaces we implement against, that we do not own
  ui/          the graphical engine
  codegen/     what an action costs, and what was tried about it
```

---

## Which one is it

**`engine/`** — a decision about the compiler that outlives the change that made
it. Not a plan, not a to-do list, not a status report. If deleting it would make
someone re-derive something from scratch, it belongs here.

**`guides/`** — how to do a thing. Reading Cranelift IR, adding a Node module,
running the corpus. Written for someone who has the task in front of them and
wants the steps.

**`reference/`** — a surface someone else defined that we implement against.
Node's module APIs. We do not own these and cannot change them; the document
records what the surface *is*, not what we decided.

**`codegen/`** — a question about **speed** that was settled, with the experiment
that settled it. Kept apart from `engine/` by one test: a document that would
still be true with every number in it deleted belongs there, and a document that
is *about* the numbers belongs here. Its own `README.md` carries the rules that
make a speed claim admissible — the first of which is that an optimization is
proven in `bench/isolated/` **before** the engine is touched — and they are
binding for any change justified by speed, in any crate.

A refuted candidate gets a document here exactly like a successful one, and for
a better reason: the successful one is visible in the code, and the refuted one
is invisible, so without the document the next person spends the same day
rediscovering that the answer is no.

**`ui/`** — the graphical engine, which has its own frozen plan and its own
phases. Kept apart because its direction is decided separately.

---

## The rules

### 1. A document that lies is fixed or deleted in the change that made it lie

Not flagged, not deferred. A stale document is worse than an absent one: an
absent one sends you to the code, and a stale one sends you somewhere wrong with
confidence. This tree previously carried specs describing a design two rewrites
old, and the cost was not the disk space.

### 1b. The one document that lived outside these four has expired, as written

`RTS_RWK_IMPLEMENT.md` sat at the repository root — how natives are authored for
the new engine — because the work had not started, and splitting a direction
across two homes before anyone has followed it is how both go stale.

It carried the condition that ends it: **split or deleted when the first three
classes land through the attribute**. Seven did, so its decisions are
`engine/authoring-natives.md` and its queue is `crates/rts-core/PLAN.md`,
which is what rule 2 says.

It is recorded here rather than quietly removed, because the condition working is
the whole difference between that file and the root `RTS_*.md` files this tree
replaced: those had no owner and no end, which is why they outlived the designs
they described.

### 2. Plans live with the thing they plan

A phase list belongs in the crate it is a plan for — `crates/rts-codegen/PLAN.md`
is the model. Not here, because a plan goes stale the moment work starts and the
person who would notice is the person editing that crate.

What is here is what stays true after the work is finished.

### 3. One question, one document

If two documents answer the same question they will answer it differently, and
the first person to notice will be someone who read the wrong one. Merge, or
delete one.

### 4. Say why, name the alternative

A document restating what the code does is worth nothing — the code says that
already, and says it correctly. What the code cannot say is what was rejected
and for what reason. That is the whole value.

### 5. Measured numbers carry their date and their source

A percentage with neither is a rumour. State what produced it and when, or leave
it out.

### 6. The crate is the first place to look

Every crate's `README.md` states its own rules and is binding for changes inside
it. `crates/rts-cranelift/README.md` and `crates/rts-codegen/README.md` are the
two that matter most, and both must be read in full before editing their crate.

Documents here explain how things fit together. They do not repeat what a crate
README already says, because that would be two answers to one question — see
rule 3.

---

## Where the engine's direction is written

The compiler is two crates, and the boundary between them is the design:

| | |
|---|---|
| `crates/rts-cranelift/README.md` | the machine. 13 binding rules. Knows no language. |
| `crates/rts-codegen/README.md` | the language. 10 binding rules. Knows no machine. |
| `crates/rts-codegen/PLAN.md` | the phases, the measured coverage, and what is left |
| `docs/engine/` | how the pieces fit, and the decisions behind them |

Either rule alone is a preference. Both at once is a boundary.
