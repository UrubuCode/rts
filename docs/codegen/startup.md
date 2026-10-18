# Where the 19.9 ms of `rts run empty.ts` goes

The target for this is a Rust binary's ceiling — about 12 ms measured the same
way. This document is what stands between the two, and the first thing it does is
correct the arithmetic everyone has been using.

**Current state, measured 2026-08-21**, PowerShell `Measure-Command`, median of
15, machine quiet:

| | med | above the floor |
|---|---:|---:|
| `cmd /c exit` — what PowerShell charges to spawn anything | 11.80 | — |
| `rts run empty.ts`, at `97f66385` | 20.82 | 9.02 |
| **`rts run empty.ts`, now** | **18.35** | **6.55** |
| `rts run hello.ts`, at `97f66385` | 22.53 | 10.73 |
| **`rts run hello.ts`, now** | **19.32** | **7.52** |

**−2.47 ms, which is −27% of the engine's own startup work.** None of it came
from making anything faster: all three fixes below remove work that nobody asked
for. What remains — 6.55 ms — is the subject of the rest of this document.

---

## The arithmetic in the old reading was wrong

`RTS_TIMING=1` prints one row per phase and the rows **nest**. `probe::Phase`
reports on `Drop` (`crates/rts-cranelift/src/probe/phase.rs:80`), so the inner
phase prints first and every parent's number already contains its children:

```
emit                          ⊂ front-end
plan, lower+compile, define   ⊂ place
install-std/node/physics/dom  ⊂ seed-context ⊂ run
```

Summing the printed rows therefore double-counts. The **top-level** total is

```
front-end 0.871 + prepare 0.027 + place 0.599 + run 5.101 = 6.598 ms
```

against a measured span of 19.9 − 12.8 = **7.1 ms** between the process being
loaded and the program being finished. So the "2–3 ms unaccounted for" that the
first reading produced **does not exist**. The real residual is **0.502 ms**, and
it holds the first `Region` construction — `crates/rts-host/src/run.rs:1097`,
outside every phase — plus `canonicalize`, `.env` and `read_to_string`.

That correction matters because it moves the target. There is no large
unmeasured block outside `run`; the work is inside it, and most of it is inside
`seed-context`.

## Where it actually is

| | ms | |
|---|---:|---|
| `seed-context` | **4.380** | 86% of `run` |
| ↳ `install-node` | 1.809 | |
| ↳ `install-std` | 0.909 | |
| ↳ `install-dom` | 0.067 | |
| ↳ `install-physics` | 0.008 | |
| ↳ **unnamed** | **1.587** | see below |
| `place` | 0.599 | |
| `front-end` | 0.871 | |
| `lower+compile` | 0.391 | |

**Building the built-in world costs seventeen times what compiling and placing
the program costs.** That is the shape of the problem: `rts` is not slow to
compile, it is slow to have a JavaScript world ready to compile *into*.

### What the 1.587 ms inside `seed-context` is

Not the four timed installs — those sum to 2.793. The remainder covers
`Context::over` (`run.rs:326`), the `declare_*` block (`run.rs:336-394`), and
**two installs with no `Phase` at all**: `crate::stack::install` (`run.rs:331`)
and `rts_ui::install` (`run.rs:416`). `rts-host/Cargo.toml` sets
`default = ["physics", "ui"]`, and the UI surface is confirmed present in the
shipped binary — `rts:egui` has 36 members, `rts:input` 20, `rts:gpu` 12.

**The first change here is an instrument, not an optimization**: put a `Phase`
around those two and around the tail, so the subtraction stops. Two cautions
recorded with it — do not wrap `Context::over` (it is ~70 `Vec::new`/`None`
initialisers and the one expensive thing in it is already gone, below), and do
not put a `Drop`-reported phase around the `with_context` block, because
`run.rs:479` calls `std::process::exit(1)` inside it and an uncaught throw would
print nothing.

## What the seed is actually building

Walked from inside a running program, at depth ≤ 6: **126 plain objects, 491
native functions, 1 497 own property definitions**, all before the program's
first statement.

Per member, `native::install` (`crates/rts-core/src/entry/native.rs:55`) does one
callable cell, one `.name` string — which is a `Slab<Str>` insert *plus* a `Vec`
malloc *plus* a second region cell, because `Context::intern_value` does not
intern despite its name — two key interns, two `objects::put` shape transitions,
and two attribute records.

Two things that are **not** the cost, checked rather than assumed:

- **`Intl` is free at startup.** It is a `supply` arm (`entry/global.rs:120`),
  and ICU4X's data is baked `const` into `.rdata` by the `icu_*_data` crates —
  no parse, no file read. What it costs is image size.
- **`place` spawns no thread pool for an empty program.** `PARALLEL_THRESHOLD`
  is 2 and an empty file emits one body, so the serial arm is taken and the
  pool — N workers × 32 MiB stacks — is never constructed.

And one that is: **`process.env` materialises all 77 environment variables into
one object at startup** (`crates/rts-node/src/process/info.rs:164`), which is why
`process` alone accounts for 144 of the 1 497 definitions.

## Fixed: an 8 MiB region built and thrown away

`Context::over` ended with `..Context::new(singletons, kinds)`, and Rust
evaluates that base expression in full before moving the fields it keeps — so
every run reserved 64 MiB of address space, zero-filled 8 MiB of it, and freed
it, in addition to the region the host had already handed in. See
[`hot-path-hygiene.md`](hot-path-hygiene.md) §4 for the fix and why the
regression is now unrepresentable rather than merely documented.

**The 4.380 ms above was measured with that bug present.** It has not been
re-measured since, and doing so is the remaining work on this line.

## Fixed: the region was zero-filling memory the OS had just zeroed

`Region::sharded` claimed the heap like this:

```rust
let mut words = Vec::new();
words.reserve_exact(words_for(reserved));   // 64 MiB of address space
words.resize(words_for(cells), 0);          // 8 MiB, WRITTEN
```

`Vec::resize` with a zero fill is a `memset`. It was running over eight megabytes
that the operating system had just handed over — which is necessarily already
zero, or one process could read another's.

`bench/isolated/src/bin/region_start.rs`, release, 2026-08-21, per construction:

| | ns |
|---|---:|
| `reserve_exact` + `resize(start, 0)` — what it did | **1 515 547** |
| `vec![0; reserved]` + `truncate(start)` | **37 814** |
| `reserve_exact` + `resize(one cell, 0)` | 22 139 |
| `reserve_exact` alone, no fill | 22 356 |
| `spanned_interior: vec![false; cells]` | 967 |

**1.5 ms of every `rts run`**, against a whole-process budget of about 7 ms above
the shell's spawn floor. `vec![0; n]` is specialised to `alloc_zeroed`, which for
a block this size asks for demand-zero pages and never writes them; `truncate`
lowers the length without moving or freeing anything, so `Region::base` — an
immediate in the compiled code, which may never move — is exactly what the
allocation returned.

Nothing about the reservation changed, and the memory is not free now either: an
untouched reserved page still has no physical page behind it, and a page the
program allocates into is faulted in on first touch. What went is the
*redundant* write.

**But the faults did move, and this document said they did not.** Before the
change, the `resize` memset touched the first 8 MiB, so those pages were resident
before the program started. Now they are not, and a program that allocates pays
a fault per page that startup used to have paid in one blocking run.

That transfer had to be priced rather than assumed, because it is exactly the
shape of an optimisation that moves a cost instead of removing one. Measured
2026-08-22, same session, alternated, three million `new Callee()`:

```
without the change (97f66385)   112.41   103.57   106.37 ns/alloc
with it                         104.99   103.11    98.51
```

**Level or faster.** A fault taken one page at a time inside a program that is
doing other work costs less than eight megabytes of memset taken all at once
before anything can start. The startup keeps its 1.5 ms and the program pays
nothing for it.

The invariant that makes this safe was already pinned:
`growing_does_not_move_the_base_compiled_code_was_given` asserts `region.base()`
is unchanged across a `grow`, and `grow` itself asserts it.

**Measured in the engine, and this is the gate working.** The isolated
experiment predicted 1 515 547 − 37 814 = **1.478 ms**. `rts run empty.ts` went
from 19.86 ms (with the other five changes) to **18.35 ms** — **1.51 ms**. A
prediction from a standalone Rust file and a measurement of a 33 MB engine
agreeing to 2% is the strongest evidence this tree has that its rule 2 is worth
following.

**And this is the second half of the same memset.** Before the constructor merge
above, `Context::over` did it **twice** — once for the region the host handed in
and once for the one `..Context::new()` built and dropped. That one was cheaper
(about 0.9 ms rather than 1.5) because the second allocation reuses the block the
first just freed, whose pages are already resident, so it pays bandwidth rather
than page faults. The two together are the bulk of the 2.47 ms above.

## Fixed: ~1 500 environment-variable reads

`native::install`'s two `objects::put` calls each ran
`std::env::var_os("RTS_CACHE_WHY")` before the counter test that would have
short-circuited it — 172 ns each, measured
(`bench/isolated/src/bin/env_probe.rs`). At 1 497 property definitions that is
**~0.3 ms of startup** spent reading an environment variable nobody set. Fixed
via `crates/rts-core/src/entry/switches.rs`; same document, §1.

## The process itself: 1.6 ms over the floor, and it is DLLs

Process load costs 12.8 − 11.2 = **1.6 ms** over what PowerShell charges to spawn
anything. The PE has 5 sections, `SizeOfImage` 33 050 624, an IAT of 3 176 bytes
(~397 imported functions, negligible), and `.reloc` of 179 492 bytes — but ASLR
relocations are applied once per boot and then shared, so they are not the cost
either.

**27 statically imported DLLs are** — `user32`, `gdi32`, `imm32`, `shell32`,
`ole32`, `oleaut32`, `setupapi`, `dwmapi`, `uxtheme`, `crypt32`, `iphlpapi`,
`ws2_32` among them. Every one is mapped and `DllMain`'d by the loader before
`main` runs.

That this is the lever is not a guess: `.cargo/config.toml` already carries the
measurement. Delay-loading **`opengl32.dll` alone took `rts --version` from 78 ms
to 14 ms.**

### `opengl32` is not on that list any more, and this is what closed the question

`plan.md` recorded that "there is **no delay-load directory in the current PE at
all** — the `opengl32` entry is a no-op today", which reads as a lost 64 ms
waiting to be recovered. It is not. The PE was read directly, 2026-08-22, and
both halves matter:

| data directory | state |
|---|---|
| `DelayImport` (index 13) | **absent** |
| `Import` (index 1) | present, 560 bytes → 27 descriptors |

and `opengl32.dll` **is not among the 27**. So there is nothing to delay-load:
the linker was asked to defer a DLL this binary no longer imports statically,
found no imports from it, and emitted no directory — which is exactly what an
inert `/DELAYLOAD` looks like from outside, and is the *good* outcome rather than
the bad one. The 64 ms is already gone; it is not sitting there to be collected a
second time.

The flags themselves are being applied, which is what makes the reading
conclusive rather than ambiguous: `SizeOfStackReserve` is `0x4000000` — the 64
MiB the `/STACK` argument on the same `rustflags` line asks for. A configuration
that was not reaching the linker would have neither.

**What remains is the twelve-DLL GUI and networking set, and it is settled
against** for the reason `plan.md` states with evidence: seven of them are in
`KnownDLLs` and mapped from a boot-time section object, and a wider `/DELAYLOAD`
list crashed the unit-test binary with `STATUS_STACK_OVERFLOW` and then
`STATUS_ILLEGAL_INSTRUCTION`. That is a measured refusal, not an unexplored
lever.

## What bun proves

`bun -e ''` measures **8.5 ms**, which is *below* the 11.2 ms `cmd /c exit`
floor. So the floor is an artefact of `cmd.exe`, not a bound on a process, and
**~20 ms is not something Windows imposes.** The 8 ms between rts's process load
and its finished empty program is real, attributable work, and the table above
says where.

## Settled against

- **Snapshotting the heap.** The region is a flat array of fixed-stride cells
  with index-based references, which is unusually snapshot-friendly — but the
  seeded state is not only in the region. It is in the `Slab<Str>` behind every
  string cell, the key registry, the interner, the shape tree, and ~26 `Aside`
  tables, all of them Rust-side allocations with pointers. A snapshot that
  restores the region and rebuilds the rest restores nothing useful.
- **Making the whole `node:` surface lazy.** `entry::global::supply` already
  builds most globals on first read — `Object.getOwnPropertyNames(globalThis)`
  is 47 entries at program start, and every one of them belongs to `rts-std` or
  `rts-node`. The eager part that remains is the part something needs eagerly.

The three lazy candidates that *did* survive review are in
[`plan.md`](plan.md) §4 — three `node:` modules that are written to be lazy and
are forced eager by install itself, and the host globals that are eager where the
runtime's own are lazy.
