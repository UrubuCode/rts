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

## Where the standard library lives

There are two reasons to draw a crate boundary, and only one of them is a good
one. The old layering used the bad one, and the correction is not to stop
splitting — it is to split on the other question.

> **A crate boundary answers "does this exist here?", not "may I mention this?"**

| reason to split | verdict |
|---|---|
| enforce a **permission** — "the engine may not name `Map`" | **no.** See below: the linkage makes it unnecessary |
| encode **availability** — "`node:fs` does not exist in a browser" | **yes.** It is a build fact, and the crate graph is exactly the mechanism for one |

### Why the permission split is over

The old engine split the runtime into `rts-engine` ← `rts-primitives` +
`rts-shared` ← `rts-std` ← `rts-runtime` to enforce a doctrine through the graph:
the engine may name only the primordial classes, so make the rest unreachable by
dependency. A hardcoded class name in codegen control flow was the recurring
regression, and the graph was the guard against it.

A codegen that reaches its runtime **by index cannot name a class at all** —
there is no string to hardcode. The property the split was defending is now a
property of the linkage, so that particular boundary has no job left.

`rts-primitives` is the crate this actually removes: it exists to hold the
classes the engine is permitted to name, and nothing needs permission any more.

### Why the availability split stays, and matters more

A build for the browser has no `node:fs`. A CLI build has no DOM. A
cross-compile has to know what exists on the target *before* it links. Those are
compile-time facts about a target, and a crate graph with cargo features is the
right and checkable way to state them — unlike a permission, which the linkage
now enforces for free.

```
rts-core      value, object, text, memory, scheduling
              present on every target, including wasm
rts-host      the operating system: files, sockets, process, time  — not in a browser
rts-node      the Node compatibility surface                       — optional
rts-browser   DOM and web APIs                                     — optional
rts-napi      a foreign ABI                                        — optional, and permanent
```

Inside each of those, modules are organised by **what the code does**, which is
stable, rather than by class taxonomy:

```
rts-core/
  value/      what a value is: encoding, conversion, equality, hashing
  object/     shapes, properties, prototypes, the operations on them
  text/       strings, and the operations that genuinely copy
  memory/     allocation, barriers, the collector's contract
  schedule/   promises, the queue, parking and resuming
```

`String.prototype.indexOf` and `Array.prototype.indexOf` do related work on
different representations. Filing them apart because one is "a string class" and
the other "an array class" is filing by taxonomy rather than by what a reader is
looking for — so they live beside each other, in one crate that exists on every
target.

### The language layer does not grow with any of this

The stdlib is never linked into `rts-codegen`. `import { readFile } from
"node:fs"` resolves **name → index while compiling**, and the call site holds the
index. How many modules exist changes the *table*, not the compiler.

Which gives the availability split a second job it did not have before: **the set
of entries built for a target IS that target's capability set.** A browser build
and a CLI build produce different tables, therefore different `TABLE_HASH`
values — so linking code compiled for one against the runtime of the other is
caught at startup instead of becoming a silent call into the wrong slot.

### What survives from the old doctrine

The machinery goes; the insight underneath it does not:

> **Native syntax means the engine lowers it directly. No native syntax means it
> is a library the engine reaches through data.**

`""`, `123`, `[]`, `{}`, `/re/`, `` ` ` ``, `function`, `class` have syntax, so
the language layer lowers them. `Map`, `Date`, `URL`, `fetch` do not, so they are
entry points resolved from an import — and a resolver turning a name into an
index while compiling is smaller and more checkable than a registry consulted
while running.

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

### CommonJS is not a third population

`require`, `module`, `exports`, `__filename` and `__dirname` are bound in every
module that mentions them, and a file may use them beside `import` and `export`.
There is no per-file decision between the two systems — no extension rule, no
`"type"` in a `package.json`, no refusal.

That is affordable here for a reason specific to this engine, and it would not
be in Node. The split exists because the two systems disagree about
*evaluation*: an ES module is linked and hoisted before it runs, a CommonJS one
executes at the first `require`. `rts-host`'s `graph.rs` already collects the
whole graph and emits every file into ONE compilation, dependencies first — so
there is one evaluation model, and the thing the split protects against does not
arise.

What is left of the difference is where a module's exports come from, and both
answers live on the one specifier table: `export` publishes names into the
namespace, `module.exports` publishes a VALUE beside it. `require` answers the
value when there is one and the namespace otherwise, which is what makes
`require` of an ES module and `import` of a CommonJS one both work. See
`rts-core/src/entry/common_js.rs` and `rts-codegen/src/emit/common_js.rs`.

**The divergence to know about**: a UMD bundle's `typeof module !== "undefined"`
sniff now takes the CommonJS branch in every file, where an ES module elsewhere
would fall through to the global one. It is the branch that works here —
`module.exports` is published — and it is the answer Node gives the same file.

---

## Reading order

1. `crates/rts-cranelift/README.md` — the machine's 13 rules. Binding.
2. `crates/rts-codegen/README.md` — the language's 10 rules. Binding.
3. `crates/rts-codegen/PLAN.md` — the phases, the measured number, what is left.
4. This document — how the pieces fit.

The first two are preconditions for editing their crates, not background reading.
