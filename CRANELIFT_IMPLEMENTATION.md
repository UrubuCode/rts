# CRANELIFT_IMPLEMENTATION.md — the Cranelift API surface RTS uses, and the one it does not

**Status:** §7's plan is IMPLEMENTED as of 2026-08-01 — see **§10 Results**, which
records what each item was actually worth, including the two that came back
negative and the one that is correct but cannot be turned on yet. Every number is
**[M]** measured on this machine, **[S]** sourced with a link, or **[E]** an
estimate — treat **[E]** as a hypothesis to test.

> **This file replaces a deleted document of the same name.** The old one argued
> "everything the engine can express as Cranelift instructions should be emitted
> as IR". `RTS_OPTIMIZATION.md` §1e refuted that premise by measurement (§1e.3:
> Cranelift's inline emission already ties the native ceiling; §1e.5: the rule is
> *symbolize the body, inline the operator*). This document does not re-argue it.
> Its subject is narrower and factual: **which parts of the Cranelift API RTS
> calls, which it does not, and what the unused ones are worth.**

## How this relates to the other documents

| Document | What it owns |
|---|---|
| `RTS_OPTIMIZATION.md` | The measured cost ladder, the refuted premises, the Tier 0–5 plan |
| `docs/specs/FUTURE_OPTIMIZATION.md` | The phase plan (Phase 0–6) and `RTS_REPR_STATS` |
| `OPTIMIZATIONS.md` | The startup/compile-time campaign |
| **this file** | The Cranelift **API surface**: used vs unused, and the compile-time numbers |

Measurement conditions for every **[M]** below: `cargo build --release`, 16 cores,
`RTS_TIMING=1`, median of 3–5 runs. `tiny.ts` is `const x = 1;` — a program that
does nothing, chosen so the numbers are the engine's floor and not the program's.

---

## §1 What RTS already does — parallel compilation

`crates/rts-codegen-new/src/front/run/parcompile.rs` already runs the expensive
phase across cores. The split it exists for:

```text
serial    build the CLIF IR for every function      (needs &mut Module)
PARALLEL  ctx.compile(isa)                          (regalloc, egraph, emission)
serial    module.define_function_bytes(id, …)       (needs &mut Module)
```

This is sound because `ctx.compile` is a pure function of the `Context` and the
ISA, and `TargetIsa` is `Send + Sync`. Functions are defined into the module in
the ORIGINAL order regardless of which worker compiled them, so module layout is
byte-identical to the serial path. A dedicated rayon pool (big stack, separate
from the global pool) keeps this phase from competing with the `parallel`
namespace. `RTS_CODEGEN_JOBS=1` forces the serial path.

**What it is worth [M]:**

| | ms |
|---|---|
| `tiny.ts`, `RTS_CODEGEN_JOBS=1` | 149 |
| `tiny.ts`, 16 cores (default) | **102** |
| machine-compile phase, serial | 66.30 |
| machine-compile phase, 16 cores | **16.47** |

**4.0× on the phase, 1.46× on total wall.** Efficiency is 25% of linear on 16
cores — 425 functions, most of them tiny, so per-task overhead and allocator
contention dominate. Chunking small functions per worker instead of one task each
is the obvious next step there **[E]**.

### The Amdahl ceiling, stated up front

Full phase breakdown of `tiny.ts` (16 cores, cache off) **[M]**:

| phase | ms | parallel? |
|---|---|---|
| registry+prelude-text | 2.39 | — |
| prelude parse+lower | 4.16 | no |
| prune prelude | 4.24 | no |
| merge programs | 7.77 | no |
| sigs+declares | 0.87 | no |
| build fn IR + main | 10.76 | **no** — needs `&mut Module` |
| build thunk IR | 1.26 | no |
| **machine-compile** | **16.47** | **yes** |
| define_function_bytes | 0.70 | no |
| finalize_definitions | 0.49 | no |

And the floor under all of it: **`rts --version` is 48 ms [M]** — process start,
before a single `.ts` byte is read. So of `tiny.ts`'s 102 ms, 48 ms is not
compilation at all, ~37 ms is serial front-end, and 16.5 ms is the parallel
phase. **Driving machine-compile to zero lands at ~86 ms.** Parallelism divides
work; it does not delete it.

---

## §2 THE FINDING — Cranelift ships a per-function incremental cache, RTS does not enable it

`cranelift-codegen 0.131.0` has an incremental compilation cache behind the
`incremental-cache` feature (which pulls `enable-serde`, `postcard`, `sha2`).
Verified in the vendored source under `~/.cargo/registry`:
`src/incremental_cache.rs`, gated at `src/lib.rs:108`.

**RTS enables only `enable-serde`** (`crates/rts-codegen-new/Cargo.toml:19`), so
the module is not even compiled in.

```rust
Context::compile_with_cache(&mut self, isa, &mut dyn CacheKvStore, ctrl_plane)
    -> CompileResult<(&CompiledCode, bool)>      // the bool is "was a hit"

pub trait CacheKvStore {
    fn get(&self, key: &[u8]) -> Option<Cow<'_, [u8]>>;
    fn insert(&mut self, key: &[u8], val: Vec<u8>);
}

pub fn compute_cache_key(isa: &dyn TargetIsa, func: &Function) -> CacheKeyHash;
```

`CacheKeyHash` is a SHA-256 digest over the **`FunctionStencil`** plus the
architecture name, target triple, compiler flags and ISA flags **[S]**. The store
is a plain `Vec<u8>` → `Vec<u8>` key/value interface, so the backing policy
(memory, disk, eviction) is entirely the embedder's.

### Why this is the right shape for RTS specifically

The design splits the IR into a **stencil** and **parameters**, such that
compilation depends only on the stencil. **Function-reference relocations and
debug source locations are parameters, not stencil** **[S]** — they change often
without affecting the generated code. So a function whose callees were assigned
different `FuncId`s in this run **still hits the cache**.

That is precisely the problem RTS solved by hand in
`front/run/bake.rs::symbolize` — rewriting a run-specific `ModuleReloc` into a
name-symbolic `SymReloc` so a later run can remap it. Cranelift does the same
thing upstream, maintained and fuzzed (the feature is fuzzed by mutating a
function and comparing incremental against from-scratch compilation **[S]**), and
**per function instead of per program**.

The consequence for the cache-granularity problem: editing one source file would
invalidate only the functions whose IR changed — **including inside
`node_modules`, and without the module-system refactor being done first.** The
763-function prelude produces an identical stencil on every run, so it would hit
at 100%.

### The honest caveats — measure before adopting

1. **It covers only the machine-compile phase** — 16.47 ms of `tiny.ts`'s 102 ms.
   The front-end still runs, because you need the IR built in order to hash it.
   It cannot skip parse+lower (4.16), prune (4.24), merge (7.77) or build-fn-IR
   (10.76).
2. **It is not free.** SHA-256 over every function's IR plus a `postcard`
   serialize/deserialize per function. For 425 mostly-tiny functions the hashing
   may cost more than the compilation it avoids **[E]** — this is the number the
   spike has to produce.
3. **It complements the whole-program bake, it does not replace it.** The bake
   (`bake.rs` + `progcache.rs`) skips the ENTIRE front-end on a hit; the
   incremental cache skips only compilation but is self-invalidating and
   fine-grained. The combination — per-module bake for the front-end, incremental
   cache for what changed — is better than either alone.

---

## §3 Cranelift features with ZERO uses in the tree

Each verified by grep over `crates/` on 2026-08-01.

| API | uses | what it would do here |
|---|---|---|
| `FunctionBuilder::set_cold_block` | **0** | sink guard-fail / error-slot blocks to the end of the function, keeping hot-path I-cache lines dense. `RTS_OPTIMIZATION.md` §5 item 1.5 |
| `sadd_overflow` / `ssub_overflow` / `smul_overflow` | **0** | int32 arithmetic with a correct fallback. RTS currently emits no overflow check at all; the fallback would be an inline widen to `f64`, not a deopt |
| `Context::inline` / `trait Inline` | **0** | inlining landed 2025-07 as a separate pass, embedder-driven **by design** — the module's own docs say it "does not attempt to define heuristics" |
| `declare_value_needs_stack_map` | **0 real** | precise GC roots. 5 textual occurrences in the tree, **all of them comments** (`module_jit.rs:159`, `parcompile.rs:72`, `parcompile.rs:147`, `stack_map_registry.rs:4`, `:14`). The transport is wired end to end and carries an empty set |

### The inlining note deserves its own paragraph

Wasmtime keeps the inlining pass **off by default** and its "enable inlining by
default" PR was closed unmerged **[S]**. The reason is specific to Wasmtime:
*its* input arrives already inlined by LLVM, so the compile-time cost buys
nothing. **Neither premise holds for RTS** — the input is TypeScript, nothing has
inlined it, and the `__rtsadp_*` trampolines are exactly the opaque boundary
`RTS_OPTIMIZATION.md` §1e.2 measured at 12.2×.

One caveat bounds it: the pass inlines CLIF into CLIF, so a Rust-compiled
`__rtsadp_*` is **not** a candidate. The reachable version is emitting the fast
path as CLIF in the first place.

**IMPLEMENTED 2026-08-02, and the argument held [M].** `front/run/inliner.rs`
drives the pass with a size heuristic, ON by default (`RTS_INLINE=0` disables):

| benchmark | off | on |
|---|---|---|
| 5M direct calls | 47 ms | **40 ms** (15%) |
| prelude-heavy array/string | 157 ms | 154 ms |
| call-free integer loop | 23 ms | 23 ms |

Machine-compile time is unchanged, because the pass runs INSIDE the existing
parallel region — `populate_module` has already built every body into
`Vec<Pending>` before anything is compiled, so the callee map is in hand exactly
where `ctx.compile` runs. Only building that map is serial.

**And a safety constraint that only measurement found.** Cranelift's inliner
**panics** — it does not bail — when the callee body contains a `global_value`
instruction (`inline.rs:402`, "callee must already be legalized"). RTS emits
those everywhere: IC cells, AOT string data, module globals. The first version
passed the whole TS suite and the AOT smoke with a 24-instruction ceiling purely
because that ceiling happened to exclude every such body; raising it to 48 crashed
the compile on the first program tried. **A size ceiling is not a safety property
here** — the `global_value` filter is, and with it in place 24/48/96/256 all
measure the same, which is itself the evidence that size was never doing the work.

---

## §4 A free item — the AOT path runs the verifier

`front/run/module_jit.rs:84` sets `enable_verifier = "false"`.
`front/run/module_aot.rs` **does not** — despite its own comment claiming "the
EXACT same flags as the JIT".

Measured on the same program **[M]**:

| | machine-compile |
|---|---|
| JIT (`rts run`) | 17.8 ms |
| AOT (`rts compile`) | **33.9 ms** |

Roughly 2×, **consistent with** the verifier running — but not proof of it. The
AOT path also lowers string literals as data objects plus a runtime
`string_from_static` call instead of the JIT's compile-time-baked handle, which
adds real IR. One line settles it, and the measurement is the test.

Note the context before over-valuing it: a full `rts compile` is ~1019 ms **[M]**
and the object is 565 KB, so `rust-lld` dominates. This is ~16 ms of that.

---

## §5 What the other Cranelift consumers do — and one negative result

**`rustc_codegen_cranelift`** — the interesting finding is a **negative** one.
Compiled object files are **not** stored in its incremental cache, and the
project measures that at roughly **200% worse incremental compile performance**
(tracked as its issue #760) **[S]**. The project structurally closest to RTS has
the same hole and has already priced it.

**Wasmtime** — merged the Cranelift incremental cache in 2022 **[S]** and caches
compiled artifacts at the **module** level, keyed on the compilation
configuration (target, Cranelift flags).

**Wasmer** — module-level too: `Module::serialize` / `Module::deserialize`, with
artifacts under `~/.wasmer/cache/compiled`, and a *headless* engine that contains
no compiler at all and can only run a pre-compiled artifact **[S]**.

**The number worth carrying over from Wasmer**: zero-copy deserialization gave
them **40–50% on module load time** **[S]**. That is the measured answer to "would
a custom serialization format help RTS's cache" — the win in a compiled-artifact
cache is **not copying**, not a faster codec. RTS's own numbers agree on the
scale: reading + deserializing the 1.1 MB manifest is ≤11 ms of a 71 ms cache hit
**[M]**, against a 48 ms process floor a codec cannot touch.

**Correction of a common attribution:** Cranelift is **not** Wasmer's. It is
maintained by the Bytecode Alliance, developed principally for Wasmtime. Wasmer
uses it as one backend among Singlepass and LLVM.

---

## §6 The prelude, in Cranelift terms

Why any of the above matters is set by what RTS actually asks Cranelift to
compile. Prelude = **763 functions, 278 296 bytes of `.ts`**, parsed and lowered
on every startup **[M]**.

| | funcs kept | machine-compiled | machine-compile |
|---|---|---|---|
| `RTS_NO_PRUNE=1` | 763 | 1587 | 38.65 ms |
| pruned (default) | **207** | **425** | **17.81 ms** |

Two facts fall out, both **[M]**:

**1. Half of everything compiled is a thunk, emitted unconditionally.**
425 = 207 bodies + 207 thunks + 11 class new-thunks. `module_jit.rs:486` is
`for f in funcs` with no condition — every function gets the uniform-ABI bridge,
including functions that are only ever called directly and never reified. A thunk
is needed only where something takes a `func_addr` (passed as an argument, stored,
reached by dynamic dispatch). The prune already harvests name mentions; what is
missing is distinguishing a mention in **call position** from one in **value
position**.

**2. The 207 is a floor, not a demand.** Identical for three different programs:

| program | pruned | kept | compiled |
|---|---|---|---|
| `const x = 1` | 556 | **207** | **425** |
| `console.log(1)` | 556 | **207** | **425** |
| `"a" + 1` | 556 | **207** | **425** |
| `JSON.stringify({a:1})` | 533 | 230 | 473 |

`console.log` adds **zero**. The unconditional `String`/`Number`/`Boolean`
wrapper seed is not the cause — `prune.rs` measures that seed at 13 functions
(280 → 293). The floor is prelude **top-level code**: class declarations and
singletons like `const console = new Console()` that `main` mentions, so they are
roots by existing rather than by being used. The prune is reachability from
`main`, and the prelude's `main` initialises everything.

This is the limit of "RTS knows whether it needs it": it knows what YOUR code
reaches; the prelude reaches itself at initialisation.

---

## §7 Ordered, with what each is worth

| # | item | expected | effort | confidence |
|---|---|---|---|---|
| 1 | `enable_verifier=false` in `module_aot.rs` | ~16 ms of a 1019 ms AOT compile | one line | the 2× gap is measured; the attribution is not |
| 2 | **spike `incremental-cache`** — enable the feature, put a `CacheKvStore` over `.rts/`, measure hit rate and the SHA-256+postcard cost against the 16.5 ms it disputes | unknown, possibly negative | small | the API is verified present; the economics are not |
| 3 | thunk on demand | ≤51% of the compiled COUNT; thunks are small so less than that in time **[E]** | contained — the analysis already exists in `prune.rs` | high that it shrinks the count |
| 4 | chunk small functions per rayon worker | efficiency is 25% of linear today | ~an hour | moderate |
| 5 | `set_cold_block` + the `*_overflow` family | part of `RTS_OPTIMIZATION.md` Tier 1.5 / 2.2 | small each | measured elsewhere, not here |
| 6 | declare-then-lower (parallelise IR construction) | 10.76 ms serial | medium — `aot_str::DATA_CTR` and `ic::CELL_CTR` declare data DURING lowering and would need per-worker buffers or pre-declaration | low until prototyped |

**None of these changes the ceiling.** `tiny.ts` is 102 ms of which 48 ms is
process start and ~37 ms is serial front-end. The item that changes category is
not in this table: `RTS_OPTIMIZATION.md` §1e.5's route — stdlib bodies as native
symbols — because a native symbol has no CLIF, no thunk, no prune decision and no
parse. It is already compiled into the binary. Everything above divides or trims
the work; that one deletes it.

---

## §8 What these measurements do NOT prove

1. **One machine, one program shape.** Every number is `tiny.ts` or `proj/main.ts`
   on one 16-core Windows box. The parallel efficiency in particular is
   core-count- and allocator-dependent.
2. **Median of 3–5 with no variance reported.** A real 4× and a lucky 3× look
   identical here.
3. **The AOT/JIT machine-compile gap is attributed, not isolated.** Two variables
   differ (verifier, string lowering) and only one was reasoned about.
4. **The incremental-cache economics are entirely unmeasured.** Its API is
   verified to exist; whether hashing 425 functions costs less than compiling
   them is exactly what item 2 in §7 exists to find out, and it may come back
   negative.
5. **A first run measured cold is not a measurement.** An earlier pass of the
   `RTS_CODEGEN_JOBS` sweep reported 368 ms serial vs 99 ms parallel; re-running
   with the OS file cache warm gave 149 vs 102. The first ordering charged the
   serial run for the cold start. Corrected here; noted because it is the easiest
   mistake to repeat.

---

## §9 Sources

- [Cranelift `incremental_cache.rs`](https://docs.wasmtime.dev/api/src/cranelift_codegen/incremental_cache.rs.html) — the API quoted in §2; also verified in the vendored `cranelift-codegen-0.131.0`
- [Cranelift Progress in 2022 — Bytecode Alliance](https://bytecodealliance.org/articles/cranelift-progress-2022) — the stencil/parameters design and the fuzzing approach
- [Incremental compilation cache in Cranelift — wasmtime#4155](https://github.com/bytecodealliance/wasmtime/issues/4155)
- [rustc_codegen_cranelift](https://github.com/rust-lang/rustc_codegen_cranelift) — object files absent from the incremental cache (its issue #760)
- [wasmer-cache](https://docs.rs/wasmer-cache/) — module-level artifact caching
- [Improving WebAssembly load times with Zero-Copy deserialization — Wasmer](https://wasmer.io/posts/improving-with-zero-copy-deserialization) — the 40–50% figure
- In-tree: `RTS_OPTIMIZATION.md` §1e (the backend question, settled), `crates/rts-codegen-new/src/front/run/parcompile.rs` (the parallel split and why it is behaviour-preserving)

---

## §10 RESULTS — what §7 was actually worth

Implemented 2026-08-01. Same conditions as every other **[M]** here: `cargo build
--release`, 16 logical / 8 physical cores, `RTS_TIMING=1`, `tiny.ts`. Each item
carries an env switch so the claim stays checkable with one binary instead of
being re-derived from a rebuild.

| # | item | expected (§7) | measured | state |
|---|---|---|---|---|
| 1 | `enable_verifier=false` on AOT | ~16 ms of a 1019 ms AOT compile | **4.8 ms** of the AOT machine-compile phase (41.2 → 36.4) | landed, default ON |
| 2 | `incremental-cache` spike | "unknown, possibly negative" | one program **15.6 → 10.4 ms**; cross-program hits **0% → 98%** after the determinism fix; suite break-even | landed, OPT-IN |
| 3 | thunk on demand | ≤51% of the compiled count | **59 of 218 thunks** dropped; 19.2 → 15.4 ms | landed, default ON |
| 4 | chunk per rayon worker | "efficiency is 25% of linear" | **no gain**; premise was hyperthreads | knob kept, default = rayon |
| 5a | `set_cold_block` | Tier 1.5 | **no measurable delta** (47 ms both ways) | landed, default ON |
| 5b | `*_overflow` family | Tier 2.2 | **correct**, and 6.6x slower | landed, OPT-IN |
| 6 | declare-then-lower | 10.76 ms serial | serial-forced share measured at **7%**; phase 12.5 → **11.1 ms** from the symbol memo | open, now specified |
| 7 | **inlining** (§3, not in the original list) | untested hypothesis | **15%** on a call-heavy benchmark, compile time unchanged | landed, default ON |

Switches: `RTS_CLIF_VERIFIER=1`, `RTS_CLIF_CACHE=1` (+ `RTS_CLIF_CACHE_DIR`),
`RTS_ALL_THUNKS=1`, `RTS_CODEGEN_CHUNK=N`, `RTS_COLD_BLOCKS=0`,
`RTS_INT_OVERFLOW=1`.

### The two corrections §7 forced

**§4's attribution was wrong.** The 2× JIT/AOT machine-compile gap is not the
verifier. With the verifier off on both paths AOT is still 36.4 ms against the
JIT's 21.0 ms; the verifier accounts for 4.8 of that (13%), and the rest is the
second variable §4 named and did not isolate — the AOT path lowers string
literals as data objects plus a runtime `string_from_static` call instead of the
JIT's compile-time-baked handle.

**§7 item 4's premise was wrong.** "Efficiency is 25% of linear on 16 cores" was
measuring hyperthreads: 8 workers already reach the 16-worker number (22.2 vs
22.4 ms), so the ceiling is physical cores, not per-task overhead. Forcing a
coarser grain does nothing up to `chunk=8` and costs 2× at `chunk=32`, because
the functions are not equal-cost and rayon's adaptive splitting already
work-steals around that.

### Item 2 is positive on one program and NEGATIVE on a suite — corrected

The first result here read "positive, against its own prediction", from a
`tiny.ts` measurement alone. Measuring the real workload reversed it, and the
correction is recorded rather than replaced because the reason is the useful part.

**On a repeated compile of one program it wins**, and that still holds: 15.6 →
10.5 ms of machine-compile, 203 hits, a store that stabilises at 408 KB.

**Across the TS suite it loses, badly** — 811 processes, each compiling a
different program:

| | off | on |
|---|---|---|
| suite run 1 | 38.8 s | **65.8 s** (store 138 MB) |
| suite run 2 | 37.7 s | **78.2 s** (store 171 MB) |

Two store designs were measured before concluding, because the first failure had
an obvious-looking cause that was only half of it:

1. **read-all + rewrite-all** — every process paid a full read and a full rewrite
   of a growing file, quadratic in the number of programs (39.6 → 44.1 → 57.9 →
   76.2 s).
2. **append-only, then prelude-only entries** — the rewrite is gone and writes are
   restricted to the functions that ought to repeat. Still loses, still grows.

**The cause is not the store: the prelude's stencils are PROGRAM-DEPENDENT.** A
prelude function's IR carries shape-id immediates interned per program and
IC-cell data symbols named from a per-run counter (`ic.rs::CELL_CTR`,
`aot_str.rs::DATA_CTR`), so identical source lowers to a different stencil in each
process and hashes to a different key. There is nothing to hit, and every process
pays hashing plus a bigger file to read.

§2's optimism — "the 763-function prelude produces an identical stencil on every
run, so it would hit at 100%" — is therefore **false as the engine stands**. It
would become true only if shape interning and data-symbol naming were made stable
across processes, which is a change to those two mechanisms, not to the cache.

**And then it was fixed.** The cause named above turned out to be one line of
policy, not a design limit. `ic.rs` numbered its cells from a process-wide
counter; it now names them `__rtsic_<fn>_<site>`, per owning function. A
function's IR no longer depends on how many IC sites were lowered before it, so
prelude pruning keeping a different subset per program stops changing the
stencil. Measured immediately after, compiling a DIFFERENT program against one
program's cache:

| program | hits | misses |
|---|---|---|
| `tiny.ts` (cold) | 0 | 207 |
| `numbench.ts` | **203** | 4 |
| `callbench.ts` | **116** | 93 |

Cross-program hits: **0% → 98%**. The suite stopped diverging — 37.8 s off
against 42.5/40.1 s on, with the store CONVERGED at 34.6 MB instead of climbing
past 171 MB. Suite results with the cache on are identical to off (810/811 files,
3245/3246 tests), so the replayed machine code is correct.

The store was reshaped to match: an index file (`[key][offset][len]`) plus a blob
file, with the access mode chosen by size — slurp under 4 MB, read individual
blobs by offset above it. Both directions are measured: serving ~200 hits by
individual `seek_read` costs 14.6–16.7 ms against 10.4 ms slurped, while slurping
tens of MB in each of 811 processes is the I/O that made the earlier rounds lose.

The feature stays OPT-IN and capped at 32 MB — break-even on a suite is not a
reason to turn something on by default. The determinism fix is NOT opt-in, and it
is the durable part: it also removes one of the two things blocking item 6.

### Item 5b is the one that is correct and still off

It is the only item here that is a CORRECTNESS fix rather than a speed one, and
the only one that cannot simply be switched on. §3 recorded "RTS currently emits
no overflow check at all"; the consequence is that a proven-int `+`/`*` answers
wrongly once it wraps (`4611686018427387904 * 4` gives `0`, node gives
`18446744073709552000`). The checked form matches node on every case tried.

What blocks it: **RTS has both semantics and cannot currently tell them apart.**
A JS `number` is a double, so wrapping is a wrong answer; a value declared
`i64`/`u32` is a native fixed-width integer, so wrapping is the contract the
annotation asks for. `rts-hir` types EVERY integral literal as `HirType::I64`
regardless of annotation (`lower.rs:915`/`:927`), so gating on the operand's HIR
type was implemented, measured to never fire, and removed. The prerequisite is an
"annotated" bit on the HIR binding (~95 construction sites).

The cost is why a blanket check is not an option: an int-heavy 50M-iteration loop
goes 25 → 226 ms with the two edges merging as `Float64`, and 25 → 735 ms with
them merging as `Tagged` (the first version — the Tagged repr propagates into the
loop variable and takes the whole loop off the int path, which is a useful
demonstration of Pilar 2's join rule doing what it says).

### Item 6, the one left open — now measured, with both prerequisites named

**How much of the phase is even at stake, measured.** `build fn IR + main` is
serial only because it holds `&mut dyn Module`. `timing::declare` (new; reported
under `RTS_TIMING=1` as `of which: module decls`) prices exactly that part:

| | build fn IR | of which module mutations |
|---|---|---|
| before | 12.5 ms | **1.76 ms** (10538 calls) — 14% |
| after the per-module symbol memo | **11.1 ms** | **0.74 ms** (5788 calls) — 7% |

So **93% of the phase is per-function work holding the module for no reason**,
and that is item 6's real ceiling: ~10 ms of a ~100 ms `tiny.ts`, or ~9 ms once
divided across 8 physical cores.

The memo itself was free money the accounting exposed: a runtime symbol was
import-declared at EVERY call site mentioning it, and each redeclare rebuilt the
Cranelift `Signature` (two `Vec` allocations) before `declare_function` deduped
it by name. One `name -> FuncId` map per module removed 4750 of them.

**Prerequisite 1 — `FuncId`s are baked into the IR being built.** A callee
reference is `UserExternalName { namespace: 0, index: FuncId }` in
`func.params`. Cranelift classes those as cache PARAMETERS, not stencil, which is
the hint: they can be REWRITTEN after the fact. So workers could build IR against
a per-worker virtual index space and a serial pass could remap
`func.params.user_named_funcs()` to real ids — the same move `bake.rs::symbolize`
already makes for relocations. Buffering the declares themselves does not work;
remapping the references does.

**Prerequisite 2 — shape ids are assigned by INTERNING ORDER.**
`rts-natives/src/heap/shapes/mod.rs:130` computes
`id = GLOBAL_SHAPE_BASE + reg.keys.len()`, and the lowering interns as it goes.
Under parallel lowering the id a shape receives would depend on worker
scheduling, so the immediates baked into the IR would differ run to run —
breaking the whole-program bake, the incremental cache, and the AOT shape seed,
which all assume a program compiles to the same bytes twice. The fix is a serial
PRE-INTERN pass over the HIR (deterministic by scan order), which is also what
pre-declaration of IC cells and string data wants; the alternative, content-hashed
ids, collides with the positional snapshot `seed_global_shapes` rebuilds from.

**And a third constraint the AOT regression added (#2100).** "Address taken" is
not a property of the lowering alone: an AOT object carries relocations emitted
OUTSIDE it (class new-thunks, prelude statics). Any pre-declaration pass must
cover those sites too — a missed one is a link failure, not a slow start.

### The original ceiling note

`build fn IR + main` is 10.78 ms and serial because lowering holds
`&mut dyn Module`: it declares callees, data and signatures as it goes (25 call
sites across `front/run/` and `value/`), and the returned `FuncId`/`DataId` is
used immediately in the IR being built.

The shape that would work is **pre-declaration, not buffering**: a scan pass over
the HIR collects every string literal and every IC site up front, declares their
data objects, and hands the lowering an immutable name→id map — at which point
lowering needs only `&Module` and parallelises like the machine-compile phase
already does. Buffering the declarations per worker does NOT work, because the
ids must exist while the IR that references them is built.

**Half of that is now done.** Pre-declaring IC cells required knowing a cell's
name without having lowered the sites before it, which the process-wide counter
made impossible; per-function naming (`__rtsic_<fn>_<site>`) gives every site a
name derivable from a scan of its own function. What remains is the scan pass
itself plus the string-literal side, and the fact that `declare_func_in_func`
takes `&mut Module` even for an already-declared callee (replicable by hand from
a pre-resolved `(FuncId, Signature)` snapshot, since it only calls
`func.import_function`).

**A measured near-miss worth recording, so it is not retried blindly.**
`emit_marshal::import_func` scans a function's already-imported `ext_funcs`
linearly per call site — quadratic in distinct callees, on a phase that is 10.8 ms
and serial. Replacing it with a per-function `FuncId → FuncRef` memo changed
nothing measurable (build-fn-IR 11.9 ms median, inside the noise of the 10.8–12.2
ms it already varied over), because `Lowerer::func_ref` already caches the hot
path and only `main` has enough distinct callees for the scan to matter. Reverted
rather than kept: an unmeasurable win is not worth cross-function cache state that
can only fail as a miscompile.

Ceiling: 10.78 ms of a 102 ms `tiny.ts`, of which 48 ms is process start. A
perfect result is ~9 ms, roughly 9% — for a refactor that touches every module
mutation in the lowering. That is why it is listed and not done: it is the item
in this table with the worst ratio of risk to return, and §7's closing paragraph
already names what changes category instead (stdlib bodies as native symbols,
`RTS_OPTIMIZATION.md` §1e.5 — a native symbol has no CLIF, no thunk, no prune
decision and no parse).
