# What has to exist before `rts-host`

An audit of `rts-cranelift`, `rts-codegen` and `rts-core-rwk`, asking one
question: is there anything missing that `rts-host` would be built on top of?

Answer: yes, two things, and one of them is the reason `rts-host` should not be
started yet.

---

## The state of each crate

| crate | `todo!()` | stubs | what it does |
|---|---|---|---|
| `rts-cranelift` | none | none | IR, representations, shapes, frames, calls, unwinding, scheduling, two destinations |
| `rts-codegen` | none | none | the tree, and a parser bridge that fills it |
| `rts-core-rwk` | none | none | values, heap, text, objects, coercion, collection, promise values |

No unimplemented markers anywhere. What is absent is absent *structurally* — it
is not that something was stubbed, it is that a connection was never made.

---

## Gap 1 — nothing turns a tree into instructions

```
source ──parse──▶ tree ──???──▶ IR ──▶ machine code
                        ^^^^^
```

`rts-codegen` has `syntax/` and `parse/`. It has no `lower/`. Nothing anywhere
constructs a `Function` — `FuncBuilder` has no caller outside `rts-cranelift`'s
own tests.

So today a program can be read and cannot be run. Which matters here directly:
**`rts-host` is entirely entry points, and an entry point that nothing can call
is a library with no clients.** Files, sockets and time are only useful once
something executes and reaches for them.

This is the larger of the two, and it is the next real phase of work.

## Gap 2 — the new runtime is unreachable from compiled code

`rts-core-rwk` exports plain Rust functions. `strict_equals`, `add`,
`ordinary_get` have no symbol and no index, and the baker does not scan the
crate — `SCANNED_CRATES` lists twelve crates and this is not one.

So even with a lowering, compiled code could not call any of it.

The path already exists and is the designed one: `#[rtse::abi]` declares, and
`rts-symbol-baker` renders the declarations twice — by name into
`symbol_table.rs`, by index into `entries.rs`. Annotating core's entry points and
adding it to the scan list puts them in both.

Two notes on doing that:

- `rts-macro` is a proc macro, so it is a build-time dependency and adds nothing
  at run time. It does not violate the crate's rule about dependencies being paid
  on every target.
- **Not every function in core is an entry point.** `Value::kind` is a method
  compiled code has no reason to call; `add` is. Declaring the whole surface
  would put hundreds of rows in a table whose whole argument is that a small
  closed set beats a large open one. The rule the machine already uses applies:
  *an entry point exists if and only if the operation touches the heap, the
  operating system, or global mutable state.*

## Why gap 2 comes first even though gap 1 is bigger

`rts-host` is the second crate that will need an entry-point path, and core is
the first. Establishing it once, with core as the proof, means `rts-host` is
mechanical when it arrives. Building `rts-host` first would mean discovering the
path twice — and the second discovery usually disagrees with the first.

---

## Fixed while auditing

`ir/mod.rs` said "There are no call instructions yet" while `Inst::Call` and
`Inst::CallIndirect` both exist. The statement was true when written — calls were
deliberately deferred until the convention, the safepoint and the suspension form
existed — and stopped being true when they arrived.

Corrected rather than deleted, because the reason they were deferred is worth
keeping: a call node added before those three would have chosen their answers
implicitly.

This is `docs/README.md` rule 1 in action, and it is worth noting the shape of
the failure. Nothing caught it: the code compiled, the tests passed, and the only
thing wrong was a paragraph that a reader would have believed.

---

## Recommended order

1. **Entry points for `rts-core-rwk`** — annotate the operations that qualify,
   add the crate to the baker's scan, re-bake. Small, and it settles how a
   new-world crate becomes reachable.
2. **The lowering** — `rts-codegen/src/lower/`, tree to IR. The large one, and
   the one that makes everything else observable.
3. **`rts-host`** — mechanical once a program can run and call out.
