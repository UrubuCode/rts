---
name: reuse-check
description: Mechanical search for an existing answer before writing new code in the new engine (rts-cranelift, rts-codegen, rts-core, rts-host, rts-macro). Run it BEFORE writing a value encoding, a layout, a numbering, a signature, a queue, a barrier, an interner, or anything that looks like machine bookkeeping. Also run it when a change feels like it needs a table, a registry, or a second source of a number.
---

# Does something already answer this?

The layering already burned this twice inside `rts-core`'s first three phases:
the value encoding was re-derived (and canonicalised `NaN` differently), and a
second `ShapeTree` was half-written before deletion. Both would have compiled.

The rule that forbids it lives in the crate READMEs. This is the search that
makes it mechanical.

## 1. Search the machine first

`rts-cranelift` owns everything true of the machine. Search it by concern, not by
name — the name you would pick is rarely the name it has.

| about to write | search for | in |
|---|---|---|
| tagging, NaN-boxing, "is this a double" | `encode_double`, `payload_of`, `tag_of`, `CANONICAL_NAN` | `src/tags/` |
| a property → slot map, transitions | `ShapeTree`, `ShapeId`, `KeyRegistry` | `src/shape/` |
| a field offset, a struct layout | `TypeRegistry`, `TypeId` | `src/types/` |
| a signature, a calling convention, a return | `AbiType`, `EntryDesc`, `Signature`, `Convention` | `src/abi/` |
| roots, safepoints, write barriers | `BarrierKind`, liveness | `src/gc/` |
| promises, continuations, run order | `SchedulerId`, `Delivery`, `ContinuationId` | `src/sched/` |
| suspending or resuming a frame | frame record | `src/frame/` |
| try/catch regions, cleanup chains | protected region, handler search | `src/unwind/` |
| a reference → an address | | `src/mem/` |
| a runtime function the machine itself emits | `RtEntry` | `src/symbols/` |
| what an operation costs | | `src/probe/` |

If the machine answers it, **call it**. Depending on `rts-cranelift` is the
design, not a concession.

## 2. Search the crate you are in

Second copies land inside one crate too. Search for the concept, then for the
`Aside<T>` pattern specifically — state beside a cell is the established shape in
`rts-core`, and a new field on `Context` for the same job is a duplicate.

- `crates/rts-core/src/entry/mod.rs` — the `Context` struct is the inventory
  of everything already held beside a cell. Read it before adding a field.
- `crates/rts-core/src/entry/table.rs` — every numbered entry point.
- `crates/rts-codegen/src/runtime/mod.rs` — every operation the language calls.

## 3. Two tables of one number are a bug, not redundancy

Some duplication is correct and some is fatal, and they look alike. The test is
whether the two sides must **agree about a number**:

- Correct: `rts-codegen`'s `Names` and `rts-core`'s `Interner` are two tables
  — different lifetimes, different contents — that mint from **one**
  `KeyRegistry`.
- Fatal: two things minting their own numbers for one space. Two numberings are
  two shape trees one level up.

If your new table hands out numbers, find whose registry it must mint from.

## 4. Report what you found

State the answer before writing: *"the machine already has X, I am calling it"*,
or *"nothing answers this; the nearest is Y, which differs because Z"*. That
sentence goes in the doc comment of what you write.

## Then

Read the target crate's `README.md` in full (RULE 0) and continue.
