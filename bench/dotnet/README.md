# The other side of `crates/rts-core/examples/string_cost.rs`

`dotnet run -c Release` — the same rows, in C#, so "an allocation costs four
times what it costs in .NET" is a comparison rather than a recollection.

Two things this file exists to keep honest, both found by getting them wrong
first:

- **Nothing is stored.** The first version kept every object in an `object[]`
  to defeat the escape analysis .NET 9+ performs, and that store alone measured
  **8.65 ns** — a write barrier plus a covariant array-store check, the same
  order as the allocation it was meant to hold still. What proves the object
  reached the heap is the **gen0 count** printed beside each row: a stack
  allocation moves no budget, and 128-byte cells report 244 collections against
  57 for 32-byte ones, proportional to the bytes.
- **The GC mode is printed**, because workstation and server GC do not allocate
  at the same price, and a number without it is a number about an unknown.

Not in the workspace build. It needs the .NET SDK, and nothing in `rts`
depends on it.
