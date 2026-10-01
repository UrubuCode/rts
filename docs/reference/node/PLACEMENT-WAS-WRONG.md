# The placement doctrine in this folder was written for crates that no longer exist

**2026-10-01.** Five documents here told a reader where to put code, and every
crate they named was deleted on **2026-08-10**. They are removed as of this
commit; this file is what stands in their place, because a document deleted
without a reason gets re-derived.

## What was removed, and what each one said

| removed | lines | what it instructed |
|---|---:|---|
| `architecture.md` | 385 | the `rts-engine` / `rts-primitives` split, and which of the two owns an async primitive |
| `layering.md` | 256 | "duplicate only backend-specific logic; **share cross-context logic through `rts-primitives`**; globals belong to the engine" |
| `crates.md` | 267 | a per-module dependency table routing `url`, `querystring`, `punycode` and `TextDecoder` into `rts-primitives`, with crypto "promotable" to it |
| `implementation-plan.md` | 263 | an ordered build plan over that crate graph, status "plan — not executed" |
| `rts-std-migration.md` | 204 | surgery on an `rts-std` that was itself deleted and rebuilt |

`rts-engine`, `rts-primitives`, `rts-shared`, `rts-abi`, the old `rts-std` and
eleven others went on 2026-08-10 with the whole old runtime. So the sentence
"promote this to `rts-primitives`" has no referent, and a reader following it
either invents the crate or picks the nearest-looking one — which is how a
decision gets taken by accident.

## Why these five and not the other forty-eight

Because of what `docs/README.md` says `reference/` is for: **a surface someone
else defined that we implement against.** The per-module documents — `fs.md`,
`stream.md`, `crypto.md` and the rest — are Node's API, which is still Node's
API; they are kept. The five above were never that. They were *our* doctrine
about *our* crate graph, filed in the folder for surfaces we do not own, and
that is the second reason they went stale invisibly: nothing about Node changed,
so nobody re-read them.

## What is true now

- The crate map is in `CLAUDE.md` — twenty-one crates, counted rather than
  carried forward, and a name that is not there does not exist.
- Where a piece of logic goes is `rts-core`'s rule 1: **availability decides
  membership.** Anything needing an operating system is `rts-host`'s; anything
  present on every target including wasm may be `rts-core`'s. There is no
  "cross-context" crate to promote into, and the question those documents were
  answering — which of two engines owns a primitive — does not exist with one
  engine.
- `node_completed.md` is the verified state of the `node:` surface, and `INDEX.md`
  says so itself. The per-module `Status` rows are stale and that file is the
  source of truth.

## The per-module documents still NAME those crates inline

About forty of them mention `rts-primitives`, `rts-engine` or `rts-shared` in
passing — a placement aside in the middle of an API description. Those lines are
stale in exactly the same way and are not worth rewriting forty times.

**Read a per-module document as the API surface and never as placement.** Where
one says a piece belongs in a crate, check the crate exists before believing it;
`CLAUDE.md`'s map is the list.

## The general lesson, which is why this file is in `engine`'s style and not a note

A document that describes OUR structure, filed where documents about OTHER
people's structures live, is a document nothing will ever force a re-read of.
`docs/README.md` already states the rule that would have prevented it — a
decision about the compiler that outlives the change belongs in `engine/`, and
`reference/` is for what we do not own. These five were in the wrong folder
before they were wrong.
