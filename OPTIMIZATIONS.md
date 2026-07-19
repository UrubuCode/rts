# OPTIMIZATIONS — startup and compile-time campaign

> Written in English per the documentation-language rule (`CLAUDE.md` §
> Conventions, owner decision 2026-07-05). Working language stays Portuguese.

This document is the plan for the RTS startup/compile-time campaign opened on
2026-07-18. It records **what was measured**, **why each item is on the list**,
and the **one-item-per-commit protocol** every item follows.

Every number below is either tagged **measured** (produced by `RTS_TIMING=1` on
this machine) or **estimated** (a projection that the item's own commit must
confirm or refute). Do not quote an estimate as a result.

---

## 1. The finding that started this

`rts run` on an EMPTY program (`const x = 1;`) cost **890 ms** — measured. The
same Monte Carlo benchmark the README reports at 543 ms JIT spends only ~70 ms in
its 10M-iteration loop; everything else is fixed startup.

Phase breakdown of those 890 ms (measured, `RTS_TIMING=1`):

| Phase | ms | Avoidable? |
|---|---:|---|
| parse + lower the embedded `.ts` prelude (252 KB, ~5k lines) | 69 | yes — precompute |
| **Cranelift: 831 prelude function bodies** | **578** | **yes — prune / precompile** |
| Cranelift: 831 uniform-ABI thunks | 89 | yes — same |
| merge + sigs + symbol table + finalize | ~13 | no |
| process start, registry build, runtime init | ~80 | partly |

### Why this is the right thing to attack

The engine's embedded prelude (Error/Object/String/Number/console/Map/Set/JSON/
web-api/streams/node:stream/`rts:test`/…) is **fixed at build time**. It was
parsed, lowered and machine-compiled on *every* process, whether or not the
program touched any of it. That is not a value-model problem — `PolyValue` and
the Repr lattice are nowhere near this critical path — it is pure redundant work.

It also multiplies: `rts test` spawns **one process per test file** (724 files),
so the suite paid this fixed cost 724 times. Startup dominates the suite's wall
clock far more than any test does.

---

## 2. The protocol (binding for this campaign)

One item per commit. For each:

1. **Measure before.** `RTS_TIMING=1 rts run <empty.ts>` for the phase table,
   plus the item's own specific metric.
2. **Implement one item only.** No drive-by changes — a commit that moves two
   things cannot attribute its own result.
3. **Measure after**, same command, same machine, and put both numbers in the
   commit body.
4. **Run the suite** (`target/release/rts.exe test`) and compare against the
   recorded baseline. The honesty floor from `CLAUDE.md` never lifts: a
   regression is acceptable only when intentional AND stated explicitly.
5. **Update this file** — move the item to its measured result, replacing the
   estimate. An estimate left standing after the work landed is a lying doc.

### Suite baseline (2026-07-18, before any item)

```
Files  709 passed, 15 failed, 724 total     (serial runner, ~10 min)
Tests  2376 passed, 16 failed, 2392 total
```

Those 15 failing files pre-date this campaign.

**The suite is not deterministic.** A second serial run of the same tree gave
**16** failing files, not 15. So a ±1 file swing is noise, not signal, and no
item may be judged on a one-file difference alone. When a run differs, diff the
FAILING FILE LISTS rather than comparing counts:

```bash
extract() { sed 's/\x1b\[[0-9;]*m//g' "$1" \
  | awk '/^tests[\\\/].*\.(test|spec)\.ts$/{f=$0} /✗/{if(f!="")print f}' | sort -u; }
comm -13 before.txt after.txt   # failures the change INTRODUCED
comm -23 before.txt after.txt   # failures the change FIXED
```

---

## 3. Items

### Item 1 — Prelude reachability pruning ✅ done

**What.** `crates/rts-codegen-new/src/front/run/prune.rs`: drop the prelude
functions the program cannot reach, before Cranelift sees them.

**Why.** For most programs the overwhelming majority of the 831 prelude functions
are unreachable. Compiling them is work whose result is never read.

**How it stays sound.** Dispatch is not always static — a method can be reached
through a shape/`__rts_class` runtime table, an accessor through property syntax,
a function through a value reify. So reachability is computed over **names, not
call edges**: every name a reachable body mentions (identifier, method, property,
object key, `new C`, and every string literal — a dynamic `obj[k]` can only name
a member the program spells out somewhere) is treated as a potential edge. A
matched class name keeps the whole class plus its ancestors. User functions and
`__rtsn_main` are always roots and are never pruned.

The statement and expression walkers are written out variant by variant with **no
`_ =>` arm**, on purpose: a name the walk misses is a function pruned while still
being called, so a new HIR node must break the build rather than silently produce
a broken program.

**The computed-member-name question, resolved by testing rather than assumed.**
The premise "a dynamic lookup can only reach a member the program spells out"
would break for a name ASSEMBLED at runtime (`obj["to" + "String"]()`). Measured
outcome:

- **Object receivers are already safe.** An instance can only exist if some kept
  body named its class, and naming a class keeps its ENTIRE member surface.
  Verified: `(o as any)["gre"+"et"]()` on an object literal and
  `(c as any)["hel"+"lo"]()` on a user class both work with the pass on.
- **A PRIMITIVE receiver is covered by seeding the intrinsic wrappers.** When
  this pass landed, `(s as any)["to"+"UpperCase"]()` did not work in the engine
  at all — it failed identically with `RTS_NO_PRUNE=1`, so the pass was not what
  broke it. That engine limitation has since been FIXED (autoboxing onto the
  primordial wrapper prototypes, `crates/rts-adapters/src/value/protos.rs`),
  which made the gap live: a primitive autoboxes by TAG, so nothing spells
  `String`/`Number`/`Boolean` and neither edge kind keeps them. They are now
  seeded as roots unconditionally — measured cost **280 → 293** kept functions,
  Cranelift phase unchanged. Verified: without the seed, the
  `computed_member_primitive` fixture fails with `toString is not a function`.
- **A widening was implemented, measured, and REJECTED.** Keeping the full
  surface of every class touched by a computed access re-expanded the kept set
  from **280 → 673** functions, because ordinary numeric `arr[i]` indexing trips
  the same flag and the prelude is full of it. Paying most of the win to close a
  hole nothing can currently reach is a bad trade; the reasoning is recorded in
  `prune.rs` so it is not rediscovered and re-adopted.

**Escape hatch.** `RTS_NO_PRUNE=1` disables the pass — this also lets one binary
produce both sides of an A/B comparison, and confirms or clears this pass as the
suspect for any misbehaving program in a single run.

**Result (measured).** 831 → 280 functions kept; empty-program startup
**890 ms → 390 ms (2.3×)**. Cranelift bodies 578 → 196 ms, thunks 89 → 24 ms;
the pass itself costs 9 ms. Whole suite 1m41s → **50 s**.

**Correctness (measured).** Against the same binary with `RTS_NO_PRUNE=1`:
**zero new failures, 8 files FIXED** (16 failing → 8).

**Why pruning FIXES tests — the part that matters more than the speed.** The
prelude and the user program compile as ONE unit, so a single prelude function
hitting an unsupported construct aborted the whole program, however unrelated:

```
error: unsupported in the numeric subset: in fn `__fsWebStreamOf`:
       literal slot `v` has no synthesized fn
```

`tests/object_method_chain.test.ts` never touches fs web streams, but
`__fsWebStreamOf` failing to lower killed it. Pruning removes the unreachable
function, so the bail disappears. Eight files were failing for exactly this
reason. **Reachability pruning is therefore also a robustness fix**: an
unsupported construct in an unused corner of the stdlib no longer takes the
user's program down with it.

**Status.** Implemented and validated.

---

### Item 2 — Parallel test-suite runner ✅ done

**What.** `crates/rts-cli/src/cli/test_cmd.rs` ran 724 subprocesses in a serial
`for` loop, each blocking on `.output()`. Now a worker pool sized to the CPU
count, with results written into per-file slots so the report order is the input
order regardless of scheduling. `RTS_TEST_JOBS` overrides the worker count
(`1` = the old serial behavior).

**Why it is first in effort/reward.** The processes are **already isolated by
design** (#314 — a segfaulting fixture must not abort the suite), so running them
concurrently changes no semantics whatsoever. It only stops leaving 7 of 8 cores
idle. It also compounds with everything else: every validation cycle in this
campaign gets ~6× faster.

**The flakiness it exposed, and the fix.** The first parallel run reported 17
failing files against 16 for the serial run of the same binary. The extra one
was `promise_rejection_basic.test.ts`, which passes when run alone: a
timing-sensitive fixture losing a race because N children were competing for the
CPU. Rather than book that as an accepted regression, the runner now **re-runs
every failing file alone, serially**, and keeps the retry's result. A genuinely
broken test fails both times, so this removes load-induced noise without hiding
a real failure — and the files that needed a retry are NAMED in a `Flaky` summary
line, so the flakiness is reported instead of laundered.

**Result (measured).** Suite **~10 min → 1m41s (~6×)**, of which the serial
retry pass costs ~20 s. Failing-file list **identical to the serial run of the
same binary** (16 files, `comm` diff empty both ways).

---

### Item 3 — Parallel Cranelift compilation (estimated 220 ms → ~40 ms)

**What.** The 196 ms of function bodies + 24 ms of thunks is per-function
independent work. Split the pipeline:

```
serial:    build the clif IR for each function      (cheap)
parallel:  ctx.compile(isa)                          (regalloc + egraph + emit — the expensive part)
serial:    module.define_function_bytes(id, bytes, relocs)
```

**Why it is safe to attempt.** `define_function_bytes` exists in
`cranelift_module` for exactly this split, and rayon is already a workspace
dependency (the `parallel/` namespace). This is pure infrastructure: it either
emits the same machine code or it fails — there is no way for it to change
program behavior subtly.

**The real difficulty.** Mapping relocations (`FuncRef` → `FuncId`) across the
parallel boundary, and the fact that lowering currently holds `&mut dyn Module`
(it declares functions/data mid-lowering). The declaration side has to be hoisted
out of the parallel region.

**Why it survives Item 5.** Even with a precompiled prelude, user code and
dependencies still go through this path — so this is not made redundant by the
embed.

---

### Item 4 — Parallel parsing (estimated 69 ms → ~15 ms, and the node_modules lever)

**What.** The prelude is 30 separate sources concatenated into one string and
parsed as a single program. Parse the 30 in parallel and concatenate the item
lists (they are all top-level declarations, so this is equivalent).

The same applies with much more force to the module graph: `front/modules/
graph.rs` BFS calls `load_one` **serially** per module. Parallelizing per BFS
level is what keeps a large dependency from dominating compile time once
node_modules works (Item 7).

**Care required.** `set_fnprop_ns` is global state shared across parses.

---

### Item 5 — Precompile and embed the prelude (the real fix)

**What.** Compile the embedded `.ts` prelude at BUILD time into an object plus a
serialized metadata blob, embed both in `rts.exe`, and stop compiling it per
process. Target: the remaining ~390 ms drops to near the process floor, since
this removes the 69 ms of parsing too.

**Why it is legitimate.** `build_program_for_prelude` reads only a static table
of `&'static str` (`registry_build.rs` `PRELUDE_TS`) — no user input reaches it.
The prelude's lowering is genuinely user-independent and therefore precomputable.

**The blockers are all downstream**, in four whole-program passes that currently
straddle the prelude/user boundary:

1. **Linkage.** Prelude functions are emitted `Linkage::Local`
   (`module_jit.rs:224`) — invisible to a separately compiled user object. They
   must become `Export`, with the user build declaring them `Import`. Mechanical.
2. **gcell ids are not stable.** `funcval::module_globals` numbers sequentially,
   but only over names passing its `promote` test, which is computed over the
   MERGED program. A different user program can stop a prelude global from being
   promoted, shifting every later id. Fix: force-promote the whole prelude set
   unconditionally so its id prefix is frozen.
3. **TCO changes a prelude function's calling convention based on user code.**
   `tco::compute_tail_set` (`module_jit.rs:209`) flips `CallConv::Tail` on
   prelude functions according to the user's call edges. A precompiled function
   cannot have a per-program ABI — the tail set must be frozen for prelude
   functions before they can be precompiled.
4. **Shape ids are process state, and AOT already loses them.**
   `rts-engine/src/heap/shapes.rs` documents `error_class_info` returning `None`
   "when the prelude class was not lowered in this process — e.g. an AOT binary".
   This is a **pre-existing bug** that this item must fix regardless: shape ids
   have to be serialized and replayed at startup, not merely linked.

**Plus one semantic case.** A user program may declare `class Map` /
`function describe` and override the prelude (last-wins, a supported feature). A
precompiled prelude cannot serve that program — it needs a fallback to full
recompilation. The check is cheap: compare post-parse top-level names against
`prelude_fn_names` + the prelude `ClassTable` keys.

**Relationship to Item 1.** They are complementary, not alternatives. Embedding
ships all ~830 functions in every binary; pruning keeps ~280. For JIT the size is
irrelevant (they are already in `rts.exe`), so embed wins there. For AOT output
size, pruning still matters. Keep both.

---

### Item 6 — Bring back the compilation cache (`.o` + metadata sidecar)

**What.** The OLD engine had content-hashed object reuse and it was deleted with
the old engine. Stale artifacts prove the format existed:

```
tests/cross-runtime/misc-platform/node_modules/.rts/obj/<sha256>/output.ometa
{
  "source_checksum":      "2d003bb3f715…",
  "rts_version":          "0.1.0",
  "target":               "x86_64-pc-windows-msvc",
  "compiler_fingerprint": "7d7751fb648b…",
  "used_namespaces":      ["collections","gc","io","trace"]
}
```

`rts clean` (`cli/clean.rs:15`) still deletes `node_modules/.rts/` — a cleaner
for a cache nothing writes anymore. Nothing in the current tree reads or writes
`.ometa` (`grep -rn ometa crates/ src/` → zero hits).

**Why the old format is not good enough to restore as-is.** Its
`source_checksum` covered the **whole flattened program**, so it only helped when
re-running an unchanged program — editing any one file invalidated everything.
That is the least valuable case.

**Proposed format — per-module, not per-program.** Key each cache entry by:

```
sha256( module source
      + resolved import graph edges of that module
      + compiler_fingerprint     // rts build id — invalidates on engine change
      + target triple
      + codegen flags )          // opt_level, PIC, etc.
```

Store per entry: the object bytes **plus a metadata sidecar** carrying the
compile-time state a dependent module needs to link against it without
re-lowering — the `ClassTable` slice it exports, function signatures, shape ids,
gcell id range. Editing one file then recompiles one module.

**Why it shares infrastructure with Item 5.** Serializing "the compile-time
interface of an already-compiled unit" is *exactly* the problem the prelude embed
solves. The prelude is simply the special case where the unit is fixed at build
time. Build the serialization once and both items use it — doing Item 5 first and
Item 6 second is the cheaper order.

**Keep from the old format:** `compiler_fingerprint` (a stale cache surviving an
engine change is a debugging nightmare), `target`, and `used_namespaces` (it
drives AOT use-slicing).

---

### Item 7 — Reactivate node_modules

**What.** Bare non-builtin specifiers are refused today:

```rust
// front/modules/resolve.rs:76-79
// Bare non-builtin specifier (npm/workspace) — honest bail, out of M1 scope.
Ok(Target::Unsupported { specifier: specifier.to_string() })
```

`flatten.rs:266` turns that into a hard error. There is no `node_modules` walk
and no `package.json` `main`/`exports` handling. The two `node_modules/`
directories under `tests/cross-runtime/` are empty.

Note the installer half **is** live: `cli/registers/npm.rs` fetches tarballs into
`~/.rts/register/npm/<name>/<version>/` and `install.rs` materializes
`node_modules/`. So packages can be installed but not imported.

**Scope.**
1. Node resolution for bare specifiers: `node_modules` walk up the directory
   tree, `package.json` `exports` then `main`, index fallback, scoped packages.
2. Feed resolved packages through the same graph/flatten path.
3. **Land it together with Items 4 and 6** — a dependency graph resolved
   serially, compiled whole, and never cached is exactly the 890 ms problem again
   at a larger scale. Better to build it right than to retrofit.

---

### Item 8 — Asymmetric `opt_level` (rule-breaking; NOT recommended as-is)

**What.** Everything compiles at `opt_level=speed`, including prelude functions
that in most programs never execute. Compiling the prelude at `opt_level=none`
would likely push the 196 ms well under 100 ms.

**Why it is listed but not recommended.** The cost is real: when the stdlib *is*
hot (`String.split` in a loop), it gets slow — which directly contradicts the
project's "faster than Bun by default" goal. It is only correct as real tiering
(compile cold, recompile hot), which is the same machinery as lazy compilation.

**Where it might be acceptable:** `rts test`, where suite throughput is the only
thing that matters. Behind a flag, never as the default for `rts run`.

**Recorded here so the option is not rediscovered and adopted carelessly.**

---

### Item 10 — Cranelift settings RTS never set ✅ partly done

**What was found.** RTS sets only `opt_level=speed`, `preserve_frame_pointers`
and (AOT) `is_pic`. Everything else runs on Cranelift's defaults — and one of
those defaults is expensive:

- **`enable_verifier` defaults to TRUE.** Its own doc: *"makes compilation slower
  but catches many bugs. The verifier is always enabled by default, which is
  useful during development."* It ran at several points per function on every
  release compile. **Done:** kept in debug builds (where a malformed lowering
  must be caught loudly, and where `cargo run -- run` iteration lives), dropped
  in release. Measured: Cranelift phase 221 → 171 ms, startup 390 → **346 ms**,
  suite failing-file list unchanged.

**Still unexamined, worth measuring:**

- **`regalloc_algorithm`** — `backtracking` (default, better code) vs
  `single_pass` ("quick compilation but results in code with more register
  spills and moves"). A direct compile-time/run-time dial. Same tiering argument
  as Item 8: attractive for the prelude and for `rts test`, wrong as a blanket
  default for `rts run`.
- **`enable_alias_analysis`** (default true) — redundant-load removal. Costs
  compile time, buys run time. Measure before touching.
- **`enable_heap_access_spectre_mitigation`** / `enable_table_access_spectre_mitigation`
  (default true) — these guard Cranelift *heaps* and *tables*. RTS uses neither
  (it addresses through HandleTable slots and raw pointers), so they are likely
  inert here; confirm before assuming a win.
- **`machine_code_cfg_info`**, `enable_probestack`, `enable_nan_canonicalization`
  — check whether any default costs us something we do not use.

---

### Item 11 — `opt-level` on the compile path and the runtime ✅ done

**What.** The release profile built EVERYTHING at `opt-level = "z"` (optimize for
size). Two per-package overrides to `3`, for two different reasons:

- **Compile path** (`rts-codegen-new`, `rts-adapters`, `rts-parser`, `rts-hir`,
  `rts-engine`, `cranelift-*`, `regalloc2`, `swc_*`) — runs on every `rts run` /
  `rts compile`, so size-optimizing it taxes every invocation.
- **Runtime** (`rts-primitives`, `rts-shared`, `rts-std`, `rts-runtime`) — this is
  what the COMPILED PROGRAM calls, so size-optimizing it taxes the user's
  program, which is where the project's speed claim is actually measured.

`z` stays the default for everything else: wgpu/naga/egui/actix are the bulk of
the binary and none of them runs during a compile.

**Result (measured).** Startup 244 → **188 ms**; parse+lower 65 → 54 ms;
machine-compile 32 → 20 ms; string-heavy workload **1.08 s → 0.32 s**; Monte
Carlo 10M JIT 298 → 266 ms. Binary 63 → **65.6 MB**.

**Why scoped rather than global.** A blanket `opt-level = 3` reached the same
startup (190 ms) for **+6 MB**; the scoped version costs **+2.6 MB**. Bare
process startup (`rts --version`) is ~78 ms either way, so no load time is being
traded for run time.

**Two consequences, handled.** opt-3 inlines more and so uses deeper frames: a
full unit-test run overflowed the Rust harness's 2 MiB per-test thread stack (the
main thread gets 8 MiB, so real usage is unaffected) — fixed by
`.cargo/config.toml` raising `RUST_MIN_STACK` for test threads. And the per-child
test timeout dropped 120 s → 30 s, because `node_child_process_full` hangs under
parallel load often enough that the old budget cost the suite ~90 s whenever it
tripped.

---

### Item 12 — The GUI DLL load ✅ done (opengl32 only)

**The finding.** Bare process startup (`rts --version`, no compilation at all) was
**78 ms**. For comparison, on the same machine:

| | startup | binary |
|---|---:|---:|
| Bun | 14 ms | 94 MB |
| Deno | 15 ms | 122 MB |
| Node | 21 ms | 91 MB |
| **RTS** | **78 ms** | 65 MB |

Bun is *larger* and starts 5.5× faster, so binary size was never the cause. The
import table was: the binary links wgpu/egui, so it statically imports
`opengl32.dll` (GPU driver stack), `mmdevapi.dll` (audio devices), `dwmapi`,
`uxtheme`, `imm32`, `setupapi` — and **Windows loads every statically imported
DLL before `main` runs**. A compile-only run paid for the whole graphics and
audio stack.

**Which DLL, isolated by measurement.** Delay-loading exactly one at a time and timing `rts --version`: nothing delayed 78 ms; only `mmdevapi` (audio) 78 ms — audio costs ~0; only `opengl32` **14 ms**. So ~64 of the ~65 ms is `opengl32` alone. It is imported because `rts-runtime` enables rts-egui's `glow-backend` beside `wgpu-backend` — glow is the GL COMPATIBILITY FALLBACK for old GPUs, so every invocation paid 64 ms for a path almost nothing takes.

**Shipped: delay-load that ONE DLL.** `rts --version` 78 -> **12 ms**, empty program 188 -> **128 ms**, Monte Carlo 266 -> **202 ms**. Stable: baseline 825/4 across two full unit runs, TS failing-file list byte-identical, both window examples and the audio example verified.

**The wider list stays rejected.** Delaying more DLLs buys almost nothing on top
(78 → 11 ms instead of 12 ms) and destabilizes the process:

- Wide list (incl. `crypt32`, `shell32`, `iphlpapi`): the unit-test binary died
  with `STATUS_STACK_OVERFLOW` on one run and `STATUS_ILLEGAL_INSTRUCTION` on the
  next. Two different hard crashes from one change is corruption, not tuning.
- Narrow list (GUI/audio only): no crashes, but results stopped being
  reproducible — **824/5 then 823/6, with a different extra failure each time**,
  each of which passes in isolation. The same tree without delay-load gives
  **825/4 three runs running**.

One extra millisecond is not worth a compiler whose results vary between runs.

**What is still on the table — the plugin split.** Delay-load defers the load;
it does not remove the dependency, and the CLI still ships wgpu/naga/egui (the
bulk of its 65 MB). The engine knows at compile time whether a program touches
GUI, but that knowledge arrives *after* the OS has loaded the imports, so acting
on it means the load must stop being static:

`rts-render` and `rts-input` already define backend **traits** that `rts-egui`
implements and installs — the seam exists. Build `rts-egui` as a `cdylib` and
load it with `libloading` (the pattern `rts-napi` already uses for `.node`
addons) on the first call that needs a backend (`openWindow`, `rts:canvas`,
`rts:dom`). Then a compile-only run has no GPU/audio import at all, a graphical
program loads it exactly when it opens a window, and the CLI binary sheds most of
its 65 MB (wgpu/naga/egui are the bulk).

Cost to weigh: distribution becomes two artifacts, AOT must decide whether to
embed or reference the plugin, and a "plugin missing" error class appears that
cannot exist today.

---

### Item 9 — Lazy compilation (stub-on-first-call)

**What.** Compile a function on its first call rather than up front. Structurally
the largest win for both the prelude and large dependency graphs: you pay only
for what actually executes.

**Why it is last.** Highest complexity of anything here, and Items 1/5 already
capture most of the prelude win. It becomes the important item once node_modules
(Item 7) makes dependency graphs large — at that point static reachability
(Item 1) stops being enough, because a big dependency is *reachable* without
being *executed*.

---

## 4. Measuring

```bash
# phase table for any program
RTS_TIMING=1 target/release/rts.exe run file.ts

# A/B the pruning pass with one binary
RTS_NO_PRUNE=1 target/release/rts.exe run file.ts

# suite (compare against the baseline in §2)
target/release/rts.exe test
```

`crates/rts-codegen-new/src/timing.rs` is the instrumentation. It is zero-cost
when `RTS_TIMING` is unset (a `OnceLock<bool>` read) and every wrapped phase runs
unchanged either way.

---

## 4a. ⚠️ THE UNIT-TEST BINARY IS NOT STABLE — read before judging any item

`cargo test -p rts-codegen-new --lib` **crashes intermittently**, with
`STATUS_STACK_OVERFLOW` or `STATUS_ILLEGAL_INSTRUCTION`, and when it does
complete its failure count varies (825/4 most runs, 824/5 sometimes). Runs that
report an extra failure name a *different* test each time, and every one of them
passes in isolation.

**This is NOT caused by any item in this document.** Bisected: the same
intermittent crash reproduces at commit `2749ccdb` — the GC-pin-leak fix —
*before* parallel Cranelift, before the `opt-level` change, before the delay-load.
Raising `RUST_MIN_STACK` to 64 MiB does not fix it (the failure just changes to
ILLEGAL_INSTRUCTION), so it is not a stack-size problem.

The likely origin is the GC fix itself, and the reason is structural: before it
the tests **never finished at all**, so this crate has never had a stable
baseline to compare against. It went from "always hangs" to "usually completes,
sometimes crashes" — a real improvement, but not a clean signal.

### What this invalidates

Two changes in this campaign were REJECTED on evidence that this instability
explains just as well:

- **The narrow delay-load list** (Item 12) — rejected for "824/5 then 823/6".
- **The parallel prelude parse** (Item 4) — rejected for one anomalous run in
  three, then a stack overflow. The overflow reproduced *after the revert*, on a
  clean tree.

Both may be fine. Neither should be re-adopted until the instability is
understood — a green run proves nothing while the binary crashes at random, in
either direction. **Finding this must come before any further item**: it is
currently impossible to validate engine work at the unit-test level.

The TS suite (`rts test`) has stayed reproducible throughout — failing-file lists
byte-identical across every item — so it remains the trustworthy gate for now.

---

## 4b. Adjacent fixes this campaign uncovered

Not startup items, but found by this work and worth recording — two of them
change the performance picture more than some items on the list.

### The GC pin leak (fixed)

`abi_adapter::intern_poly` pinned every string it interned as a permanent GC
root. Correct for a compile-time literal (the JIT splices the handle as an
`iconst` immediate the scanner cannot see), wrong for the ~146 runtime
trampolines that shared the helper — every transient string became a permanent
root, ~9.3k per program, monotonic.

Two failure modes fell out: a quadratic `Vec::contains` in `pin_handle` (spin),
and — once the live count crossed `GC_LIVE_FLOOR` — a collector running every
256 allocations that marked (taking shard locks) *while a thread was suspended*
(deadlock). That is why `cargo test -p rts-codegen-new --lib` never finished.

- Unit tests: never completed → **825 passed / 4 failed, 219 s**
- String-heavy workload: **4.09 s → 1.08 s (3.8×)** — this was taxing every
  program, not just tests

### The hang was hiding a regression of mine

With the unit tests running again, two failures surfaced that the TS suite never
caught: `typeof Object(null)` → *"call to unknown function `Object`"*. The
lowering rewrites `Object(x)` to the prelude's `ObjectFactory(x)`, a name no user
source spells, so no mention edge kept it and **Item 1 pruned it**.

Fixed by seeding `ENGINE_CALLED_PRELUDE_FNS`, with a test that fails if the
prelude renames one. The lesson is worth more than the fix: a dark test layer let
a real regression through a full green suite. Restoring that layer is part of why
this campaign is worth its cost.

### Computed member access on primitives (fixed)

`(s as any)["to"+"UpperCase"]()` threw. Fixed by autoboxing onto the primordial
wrapper prototypes in the value model. Found on the way: every key on a string
receiver was `ToNumber`-coerced, so `s["length"]` silently returned `"a"` instead
of `3` — a wrong value, not an error.

---

## 5. Status board

| # | Item | Expected | Measured | State |
|---|---|---|---|---|
| 1 | Prelude reachability pruning | — | **890 → 390 ms**, +8 files fixed, 0 regressions | ✅ done |
| 2 | Parallel test runner | ~6-8× suite | **10 min → 1m41s**, same failing list | ✅ done |
| 3 | Parallel Cranelift | 220 → ~40 ms | **142.9 → 34.5 ms (4.1×)**; startup 356 → 244 ms | ✅ done |
| 4 | Parallel parsing | 69 → ~15 ms | parse 26 → 11 ms, but reverted — see §4a | ⏸ blocked on test instability |
| 5 | Precompiled embedded prelude | → process floor | — | not started (4 blockers documented) |
| 6 | Compilation cache (`.o` + sidecar) | — | — | not started |
| 7 | node_modules reactivation | — | — | not started |
| 8 | Asymmetric `opt_level` | 196 → <100 ms | — | recorded, not recommended |
| 9 | Lazy compilation | — | — | deferred |
| 10 | Cranelift settings (verifier off) | — | **390 → 346 ms** | ✅ verifier done; regalloc/alias unexamined |
| 11 | opt-level 3 on compile path + runtime | — | **244 → 188 ms**; string workload **1.08 s → 0.32 s** | ✅ done |
| 12 | GUI DLL load (78 ms of bare startup) | 78 → 11 ms | **`rts --version` 78 → 12 ms**; empty 188 → 128 ms; Monte Carlo 266 → 202 ms | ✅ done (opengl32 only; wider list rejected as unstable) |

**Combined so far:** empty-program startup **890 → 128 ms (7.0×)**; bare
`rts --version` **78 → 12 ms**; Monte Carlo 10M JIT **960 → 202 ms**; full suite
**~10 min → 37 s** (~16×); unit tests from never-finishing to 87 s; 8
previously-failing files fixed; no regression.
