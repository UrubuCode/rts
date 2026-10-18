# What has to exist before `rts-host`

An audit of `rts-cranelift`, `rts-codegen` and `rts-core`, asking one
question: is there anything missing that `rts-host` would be built on top of?

Answer: yes, two things, and one of them is the reason `rts-host` should not be
started yet.

---

## The state of each crate

| crate | `todo!()` | stubs | what it does |
|---|---|---|---|
| `rts-cranelift` | none | none | IR, representations, shapes, frames, calls, unwinding, scheduling, two destinations |
| `rts-codegen` | none | none | the tree, and a parser bridge that fills it |
| `rts-core` | none | none | values, heap, text, objects, coercion, collection, promise values |

No unimplemented markers anywhere. What is absent is absent *structurally* — it
is not that something was stubbed, it is that a connection was never made.

**Pre-existing, and not on the path to `rts-host`:** `rts-macro`'s integration
tests do not compile. Four of them — `ctor_handle_escape`, `class_param`,
`functioncall`, `type_record` — fail on a missing
`__rtsm_global_handlector_new`, which looks like a naming bug in the `#[rtse::class]`
constructor path. Confirmed against an earlier commit in a scratch worktree
rather than assumed, because "it was already broken" is the easiest thing to say
and the easiest to be wrong about.

They are old-world: `#[rtse::class]` builds Registry members for the engine being
replaced. Worth fixing, and not a blocker for anything below.

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

`rts-core` exports plain Rust functions. `strict_equals`, `add`,
`ordinary_get` have no symbol and no index, and the baker does not scan the
crate — `SCANNED_CRATES` lists twelve crates and this is not one.

So even with a lowering, compiled code could not call any of it.

**This section first recommended `#[rtse::abi]` and the baker, and that was
wrong.** Corrected here rather than quietly, because the reason matters more
than the conclusion.

`#[rtse::abi]` emits an `rts_abi::SymbolDesc`, and `rts-abi` is the interface
`rts-cranelift::abi` **replaced**. The machine's own module says why it was
rebuilt rather than extended: *"entirely scalar: no aggregate, no structure, a
return position holding zero or one machine slot, and a string that cannot be
returned at all… It is not a foundation."* Declaring a new crate through it would
tie the new engine to the one being removed — a regression, and one that only
shows up later as work to undo.

It is also the wrong mechanism twice over: the baker scans and matches **names**,
which is exactly the linkage the index table exists to replace.

The right path was already in the repository. `rts_cranelift::symbols::RtEntry`
is an explicitly numbered enum, and its documentation states when that is the
right mechanism and when it is not:

> At that size, an explicitly numbered list in source is the right mechanism, and
> the same list at several hundred entries would not be… A closed set a reviewer
> can read in one screen is not the failure mode that motivated generation; an
> open-ended one is.

Core is the small side of that line, and stays there because of the membership
rule: **not every function is an entry point.** `Value::kind` is a method
compiled code has no reason to call; `add` is one because joining two strings
allocates. `to_int32` is not one at all — it is arithmetic, and belongs in what
the lowering emits.

Done as `CoreEntry`: four entries, numbered in source, each carrying a
`rts_cranelift::abi::Signature` so the compiler emitting a call and the runtime
defining it read the same value.

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

1. **Entry points for `rts-core`** — **done**, as `CoreEntry`. An explicitly
   numbered enum, not the baker: see above for why the first recommendation here
   was a regression.
2. **The lowering** — `rts-codegen/src/lower/`, tree to IR. The large one, and
   the one that makes everything else observable.
3. **`rts-host`** — mechanical once a program can run and call out.
