# rts-host-rwk — plan

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

## H1 — the object-file destination

Rule 3 says both destinations or neither, and today there is one. The machine
has both. What is missing here is the archive: an object file's undefined
`__rts_add` is the linker's to resolve, and nothing yet builds a runtime archive
for it to resolve against.

Worth doing early, because it is the path that needs no addresses handed over —
so it is the one that proves the two independent statements of the entry-point
set agree, by failing to link when they do not.

## H2 — faults

A compiled program that traps takes the process with it. The machine has
`fault::FaultTable` and `MachineModule` already carries it; nothing here reads
it.

## H3 — what emission gains next

The host does not need changing for most of it. Objects, property access and
closures are `rts-codegen` phases, and each arrives here as more programs that
run. The exception is calls between compiled functions, which needs more than
one function placed — the batch interface already takes a list for this reason.

## What is deliberately not planned

**A second way to run.** One `compile`, and the object-file path when it comes
will produce the same program. Two entry points that diverge is how a host stops
being able to say what it compiled.
