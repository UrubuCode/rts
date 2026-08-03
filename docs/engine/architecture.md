# The engine

Two crates and a boundary. Everything else follows from where the boundary is.

```
rts-codegen      the language.  JavaScript and TypeScript semantics.
                                Knows no machine.
      │
      ▼
rts-cranelift    the machine.   IR, representations, GC contract, frames,
                                calls, unwinding, scheduling.
                                Knows no language.
      │
      ▼
Cranelift        instruction selection, registers, object emission.
```

The rule that makes it a boundary rather than a preference: **each side's rule is
written in its own README, and neither is allowed to reach past the other.**
`rts-codegen` never names Cranelift. `rts-cranelift` never names JavaScript. One
of those alone is a style; both at once means a decision has exactly one place it
can be made.

The test of whether the line is in the right place is a second client. The
machine layer was built to serve a language it has never seen, and it will be
wrong somewhere — the value is that *where* it is wrong will be discoverable
rather than arguable.

---

## What is decided on each side

| Question | Answered by |
|---|---|
| what `a + b` means | language |
| whether `a + b` compiles to `iadd` or a call | language, from what it proved |
| what an `iadd` becomes | machine |
| what a value *is* to a program | language |
| how a value is encoded in 64 bits | machine |
| which fields an object has | language |
| where a field sits | machine |
| when a collection may run | machine |
| what is alive when it does | machine, derived from liveness |
| whether a call may suspend | language declares, machine implements |

The pattern: the language says **what**, the machine says **how**, and neither
asks the other to do its half. When the language layer is tempted to decide a
machine question, a capability is missing below and the fix belongs below —
reaching for a workaround is how a language layer grows a second, worse machine
layer inside itself.

---

## The runtime is a third thing, and it is not a crate the compiler talks *through*

A compiler emits code. Some operations cannot be emitted as instructions —
allocation, a write barrier's slow path, parking a frame, throwing — because
they touch the heap, the operating system, or global mutable state. Those become
calls into a runtime.

The rule that decides membership is grepable and it is the machine layer's own:

> An entry point exists if and only if the operation touches the heap, the
> operating system, or global mutable state. Pure computation is instructions.

At the size that rule produces — a list a person can hold in one screen — an
explicitly numbered enum in source is the right mechanism. That is
`rts_cranelift::symbols::RtEntry`, and it is deliberately not generated.

### Reaching the runtime: by index, not by name

A name is a **linker** concept. It exists so something *outside* can find the
thing, and three populations hide inside what used to be one table:

| population | needs a name |
|---|---|
| helpers only the codegen calls | **no** — the address is known when code is emitted |
| the AOT link surface | one per relocation target, not one per entry |
| a foreign contract (N-API) | **yes, permanently** — the name *is* the interface |

Only the third genuinely needs one. For the first, a string is a detour:
`"io.print"` → hash → address, when the address was already known. Worse, the
cost lands per *call site* rather than per entry, so it scales with how often
something is called rather than with how many things exist.

So: `rts-symbol-baker` renders the same declarations twice. `symbol_table.rs`
addresses them by name; `entries.rs` addresses them by index, with a
`TABLE_HASH` covering every index, name and signature.

**The hash is not optional.** Names fail loudly — codegen and runtime disagree,
the link fails, nothing runs. Indices fail *quietly*: a caller built against one
table dispatches into another's function, same address space, wrong signature,
arguments read as the wrong types. Comparing the hash once at startup turns that
back into a loud failure. Indices as linkage without a version check exchanges a
loud bug for a quiet one, which is the opposite of the point.

The name stays in every row — as **diagnostics**. A backtrace naming `rts_alloc`
is readable and one naming index 47 is not. Keeping the name as metadata rather
than as the mechanism is the entire distinction.

---

## Where the standard library lives, and why it is not a crate per layer

The old engine split the runtime into `rts-engine` ← `rts-primitives` +
`rts-shared` ← `rts-std` ← `rts-runtime`, with a doctrine about which classes the
engine was allowed to *name*. The split existed to enforce that doctrine through
the crate graph: a layer could not name what it could not depend on.

**That is not the right shape for this engine, and the reason is that the
doctrine it enforced is no longer needed.**

The old rule said the engine may name only the primordial classes and everything
else must resolve through a registry, because a hardcoded class name in codegen
control flow was the recurring regression. But a codegen that reaches its runtime
by **index** cannot name a class at all — there is no string to hardcode. The
property the crate split was defending is now a property of the linkage.

What replaces it:

### One surface, described as data, organised by what it *is*

A standard library is a set of **entry points with signatures**, plus `.ts` for
what is genuinely written in TypeScript. Not a hierarchy of crates whose edges
encode a permission system.

```
runtime/
  value/      what a value is: encoding, conversion, equality, hashing
  object/     shapes, properties, prototypes, the operations on them
  text/       strings, and the operations that genuinely copy
  memory/     allocation, barriers, the collector's contract
  schedule/   promises, the queue, parking and resuming
  host/       the operating system: files, sockets, time, process
```

Organised by **what the code does**, which is stable, rather than by **who is
allowed to call it**, which was the old split and which changed every time the
doctrine did.

### Answering the question directly: no, do not rebuild `rts-primitives`

It exists to hold the classes the engine is permitted to name. The new engine
names nothing, so the crate has no job.

What its *contents* are — `String`, `Object`, `Array`, `Number` and the rest —
still has to exist, and it moves to `runtime/value/` and `runtime/object/` above,
grouped by the operation rather than by the class. `String.prototype.indexOf` and
`Array.prototype.indexOf` are two entry points that do related work on different
representations; filing them under two crates because one is "a string class" and
the other "an array class" is filing by taxonomy rather than by what a reader is
looking for.

### The one thing that must survive the reorganisation

The old doctrine had a real insight underneath the crate-graph machinery, and it
should be kept while the machinery is dropped:

> **Native syntax means the engine handles it directly. No native syntax means it
> is a library the engine reaches through data.**

`""`, `123`, `[]`, `{}`, `/re/`, `` ` ` ``, `function`, `class` have syntax, so
the language layer lowers them. `Map`, `Date`, `URL`, `fetch` do not, so they are
entry points resolved from an import — and a resolver that turns a name into an
index at compile time is a smaller and more checkable thing than a registry
consulted at run time.

---

## Modules

Two populations, and conflating them is what produced a table of thousands.

**User modules** (`./thing.ts`) — we compile them. They become functions we
define; a call is direct, resolved by the module the compiler is already
building. No runtime table is involved at all.

**Builtin modules** (`rts:fs`, `node:path`, and the globals) — they live in the
runtime archive and are known before the program runs. `import { readFile } from
"rts:fs"` resolves **name → index at compile time**, and the call site holds the
index.

The name is matched once, while compiling. It never appears on the path that
emits code, and never in the emitted code at all except as debug information.

---

## Reading order

1. `crates/rts-cranelift/README.md` — the machine's 13 rules. Binding.
2. `crates/rts-codegen/README.md` — the language's 10 rules. Binding.
3. `crates/rts-codegen/PLAN.md` — the phases, the measured number, what is left.
4. This document — how the pieces fit.

The first two are preconditions for editing their crates, not background reading.
