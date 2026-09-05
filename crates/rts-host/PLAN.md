# rts-host — plan

Read `README.md` first. Its rules are binding; this is only the order.

## H0 — a program runs. DONE

`compile(source)` reads a function body, emits IR, verifies it, places it in
this process's memory with the runtime's addresses supplied, and returns
something callable. Seven tests, each of which runs the program rather than
inspecting it.

Three things it found on the first day, all of them defects that three phases of
verifier-checked work had not:

- **`return 1 === 1` returned a machine boolean where its signature declared a
  tagged value.** The runtime proves a `Repr::Bool`, which is what lets a branch
  consume one without a guard — but `a === b` in expression position is a
  JavaScript value, and the widening back was missing. The caller read tag 0, an
  inline integer.
- **The host was not verifying.** The check existed and had always existed; the
  first version of `compile` went from emission to the code generator without
  asking. That is what let the above reach a caller instead of a diagnostic.
- **`return` at the top level of a script is a syntax error**, so a host that
  compiled scripts could not compile a program that produced anything. It
  compiles a function body, and the completion value — what a script really
  answers, and what `eval` returns — is named as not implemented rather than
  approximated by "the last statement".

## H1 — the heap. DONE, the wiring half

`rts-core::heap::Region` — one contiguous allocation, 64-byte cells, each a
word of header and seven inline slots. `rts_alloc` implements the machine's own
entry point, and the host hands the machine `RegionBases::single(base, stride)`.

**One region per compiled program, owned by it.** Its base is a NUMBER inside
the code, so the program and its heap are one thing. The first version of this
wiring got it wrong in the way the comment beside it warned about: the host
built a region for the base and the runtime context built another when the
program ran, so compiled code would have addressed one while the allocator
filled the other. `Context::over` exists because of it.

What is NOT done: nothing allocates through it yet. Objects still live in the
slab and property access is still a call. Moving them is the rest of
`docs/engine/objects-are-aggregates.md`, and it needs the collector — a cell is
never reclaimed today, so a program that allocates enough gets index 0 back and
a wrong object. That is recorded at the entry point rather than hidden.

## H2 — the object-file destination. DONE

`object/` places a program — one file, or a module GRAPH — into a relocatable
object, and `rts-runtime` is the archive its undefined names resolve against.
The prediction above held exactly: linking is what proves the two independent
statements of the entry-point set agree, by failing when they do not.

What the graph half needed was a machine capability rather than a host one, and
it is worth recording which: three of the runtime's seed tables are keyed by a
CODE ADDRESS — which module bodies run before the entry, what a parked frame
looks like, what each function is called — and an address does not exist until a
linker places the object. So the object ASKS the linker, through
`rts_cranelift::target::AddressTable`. Until then all three shipped empty, which
refused every multi-file program, raised on every `async` function, and answered
`undefined` for `f.name` and `f.length` **with no error at all**.

## H3 — faults

A compiled program that traps takes the process with it. The machine has
`fault::FaultTable` and `MachineModule` already carries it; nothing here reads
it.

## H4 — what emission gains next

The host does not need changing for most of it. Objects, property access and
closures are `rts-codegen` phases, and each arrives here as more programs that
run. The exception is calls between compiled functions, which needs more than
one function placed — the batch interface already takes a list for this reason.

## H5 — `.html` as an entry, in `rts-cli`. DONE

Not a phase of this crate's own object-emission work — `object::page`/
`object::html_scripts` (H2) are unchanged — but the line this PLAN's own
table asks for. `rts compile pagina.html`/`rts run pagina.html` need no
`.ts` file: `rts-cli`'s `cli::html_entry` writes the `app.ts` window loop as
generated TypeScript source and hands it to the SAME `compile`/`run` path an
ordinary `.ts` entry takes, so this crate sees one more string of source —
never a second entry point, which is exactly what the section above refuses.
`docs/engine/aot-page-scripts.md` has the shell's shape and the one thing
that differs between the two commands (HTML embedded as a build-time literal
for `compile`, read from disk at run time for `run`).

Measured at close (2026-09-05, release): `rts compile
scripts/rts_vs_electron/app/index.html ReactApp` — 3 page `<script>`s
precompiled, a 39.8 MB `.exe` that opens the window under the page's own
`<title>` and logs `scripts da pagina corridos: 3`; `tests/aot/claude-pagina-entrada.html`
compiles and runs (1 script) and `rts run` opens it in JIT; `cargo test
--release -p rts-cli --test html_entry`: 5 passed. One fix at close, found by
that measurement and not by the small fixture: an `.html` entry never compiles
as a graph — the embedded bundle's own text carries `require(`/`import(` and
fooled the textual import scan into reading the `.html` off disk as
TypeScript (`c079c7ba3`).

## What is deliberately not planned

**A second way to run.** One `compile`, and the object-file path when it comes
will produce the same program. Two entry points that diverge is how a host stops
being able to say what it compiled.

---

## Where this stands

A JavaScript program compiles into this process and runs. `tests/running.rs` has
24 of them and every one runs the program rather than inspecting it.

What executes: numbers, locals, arithmetic, comparisons, `===`, `if`, `while`,
`do`/`while`, `for`, `break`/`continue`, object literals, `o.x` and `o.x = v`.

What is refused **by name**, which is the list to work down: calls, functions,
closures, strings as values, `**`, the bitwise operators and shifts, `==`, `in`,
`instanceof`, computed keys, destructuring, `switch`, labels, `throw`/`try`,
`await`, `yield`, classes, modules.

### The three agreements this crate holds, and why they are here

Each is two crates that must say the same thing and cannot check each other.

1. **The entry-point symbols.** `rts-codegen`'s `RuntimeOp` names what it emits
   calls to; `rts-core` exports names derived from its Rust functions.
   `address_of` is where a disagreement becomes a refusal instead of a call to
   whatever the linker found.
2. **The singleton numbering.** Both sides number `undefined` and `null`
   independently, and a disagreement would be quiet and total.
3. **The property-key numbering.** Resolved while compiling and crossing as a
   number, so `o.a` compiled by one side must be `o.a` read by the other.

A fourth is coming and is recorded before it is needed: the `TypeId` a shape
arrives at. The runtime derives them today and compiled code does not name one —
`cached_get` exists precisely so a site need not. The day something guards a
type by name, the two registries have to agree.

### Known defects, written down rather than discovered

- **A cell is never reclaimed.** There is no collector. A program that allocates
  past the region's capacity gets cell zero back, which is a real object and the
  wrong one. `entry/alloc.rs` records why the signature cannot say no.
- **The region never grows**, for the same reason: its base is a constant inside
  the compiled code.
- **No prototype.** A region cell has no field for one, so the chain is empty
  and every inherited property is absent.
- **`null.x` answers `undefined`** where the language throws. Throwing needs the
  machine's protected regions, and nothing emits those.
- **A property past the seventh is refused**, which is where the overflow
  indirection goes.
