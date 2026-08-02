# RTS_ENVS.md — every environment variable the engine reads

**Status:** written 2026-08-02 by reading the tree, not from memory. Every row
below names the file that reads the variable, so a stale row is checkable. If you
add, rename or flip a knob, update the row in the same commit — a lying default
in this table is worse than no table.

Scope: the `RTS_*` variables RTS itself defines, plus the small set of foreign
variables it honours. Not covered: variables a user program reads through
`process.env` / `env.get_var` — those are data, not engine configuration.

## How to read the columns

- **Default** — what the engine does when the variable is UNSET.
- **Accepted** — the exact parse. Most knobs are strict: a typo does not warn, it
  silently falls back to the default. Two parse conventions exist and are not
  interchangeable:
  - *opt-in*: `=1` (only) enables. Anything else is off.
  - *opt-out*: any value other than `0` keeps the feature on; `=0` disables.
  A few use a third, looser form (`is_ok()` — the variable merely has to EXIST).
- **Cached** — read once into a `OnceLock` at first use. A mid-process
  `set_var` then has no effect. This is deliberate for anything on a hot path or
  anything whose choice must stay stable for the life of the process (a shard
  that started chunked cannot be re-read as a `Vec`).

---

## §1 Summary — what is off that a reader might expect to be on

Answering the question directly: **eight knobs are OFF by default, and none of
them are off by oversight.** Each has a recorded reason, and in six cases the
reason is that the measurement to justify flipping it has not been run or came
back negative. Ranked by how close each is to becoming a default:

| Variable | Off because | What unblocks it |
|---|---|---|
| `RTS_REGIONS` | Exposes a pre-existing **re-entrant shard lock**. Measured win is real: `new P()` 531 → 359 ns/iter. | Fix the re-entrant lock. This is the closest to shipping and the largest measured win of the eight. |
| `RTS_JIT_CACHE` | Wins on ONE program (`rts run` 100 → 73 ms) but is a **net loss on a batch** (TS suite 38 s → 82 s warm) and costs **1017 MB for 805 files**. Plus an open hang (`tests/node_url.test.ts` under replay). | Cache per MODULE instead of a whole-program manifest, so the prelude is one shared slot instead of 805 copies. Then fix the hang. |
| `RTS_INT_OVERFLOW` | **Semantic** knob, and the ON arm is the CORRECT one — it is the only entry here where the default is knowingly wrong for JS numbers. Off because it costs 6.5x on ordinary int loops AND changes the result REPRESENTATION even where nothing overflows (int `a+b` yields a float word), which breaks a serialization golden test. | A representation-preserving merge: the overflow edge must re-tighten to an int word when the result fits, the way `opguard::emit_number_result` already does. |
| `RTS_SLAB` | Storage-representation change under the hot allocation path; the old `Vec` path must stay default until the A/B lands. | Run the A/B. Also gates `RTS_CLASS_IMPLEMENTATION.md` C5 (the slot-table base can only be baked as an immediate when addresses are stable). |
| `RTS_BUMP` | Changes the MEMORY profile (pooled capacity is retained) as well as the speed profile; both halves must be measured. | Measure both halves on one binary. |
| `RTS_CLIF_CACHE` | Cranelift incremental cache; unmeasured as a default, and an uncapped store once reached 171 MB across a suite (now capped at 32 MB). | Measure. |
| `RTS_ALL_THUNKS` | Not a candidate — the analysis it skips buys JIT startup, and AOT already forces it ON internally. | Nothing. Correct as-is. |
| `RTS_CLIF_VERIFIER` | Measurement lever, not a feature. Costs 4.8 ms of the AOT phase / 5.5 ms of the JIT phase. Debug builds keep the verifier anyway. | Nothing. Correct as-is. |

**One knob is ON by default and worth knowing about:** `RTS_ASYNC_SM` (async via
cooperative state machine). It flipped to default-ON; ineligible shapes
(try/catch around `await`, nested `await`) fall back automatically, which is what
made the flip safe.

**A trap that costs A/B runs:** `prelude_cache` keys only on the prelude TEXT plus
its cache version. Any A/B of a LOWERING-time knob (`RTS_COLD_BLOCKS`,
`RTS_OP_GUARD`, `RTS_POW_FOLD`, `RTS_REM_GUARD`, `RTS_ESCAPE`) must set
`RTS_NO_PRELUDE_CACHE=1` **on both arms** — otherwise both arms replay one cached
lowering and the comparison measures nothing.

---

## §2 Optimization knobs — ON by default

These are A/B levers, not correctness fallbacks: both arms must print the same
thing for every program. A behaviour difference between arms is a bug in the
pass, not a tuning result.

| Variable | Default | Accepted | Reads |
|---|---|---|---|
| `RTS_COLD_BLOCKS` | **ON** | opt-out (`=0`) | `rts-codegen-new/src/front/run/clifflags.rs` |
| `RTS_OP_GUARD` | **ON** | opt-out (`=0`) | same |
| `RTS_POW_FOLD` | **ON** | opt-out (`=0`) | same |
| `RTS_REM_GUARD` | **ON** | opt-out (`=0`) | same |
| `RTS_ESCAPE` | **ON** | opt-out (`=0`) | same |
| `RTS_INLINE` | **ON** | opt-out (`=0`) | `front/run/inliner.rs` |
| `RTS_LAZY_SHAPE` | **ON** | opt-out (`=0`) | `rts-runtime/src/adapters/value/objops.rs` |
| `RTS_ASYNC_SM` | **ON** | off on `0`/`off`/`false`/`none` | `rts-parser/src/lowering_items.rs` |

All cached in a `OnceLock`.

- **`RTS_COLD_BLOCKS`** — mark MISS / BAIL / ERROR / THROW blocks cold so
  Cranelift sinks them to the end of the function and the hot path stays
  contiguous in the I-cache. ONE knob deliberately covers the whole set (error
  edges, catch/finally, operator and `%` guard misses, the overflow arm, the IC
  miss, the typed-array OOB arm, the not-a-function throw arm) so the set stays
  measurable as a unit. **No A/B has been run** — the claim that the guard numbers
  are "likely conservative" without it is an expectation, not a result.
- **`RTS_OP_GUARD`** — inline tag guard in front of the generic operator
  trampolines. OFF lowers to the same `__rtsadp_*` call it always did. Expectation
  2–4x, measured on a probe, not on this emission.
- **`RTS_POW_FOLD`** — fold `x ** 2` into a native `fmul`. Deliberately a
  SEPARATE knob from `RTS_OP_GUARD` so two Tier items that landed together stay
  independently attributable. Expectation 11.5x (probe).
- **`RTS_REM_GUARD`** — run-time integer guard in front of `%`, so a native `srem`
  fires when the three preconditions that make integer remainder equal the JS one
  are proved. Expectation 1.45x (probe).
- **`RTS_ESCAPE`** — scalar-replace a provably non-escaping `new C(..)`. The
  138x figure in the probe is the provably-local BEST CASE, not an expectation.
- **`RTS_INLINE`** — the only one here with a measurement OF THIS EMISSION:
  release, median of 7, a 5M-call benchmark 47 → 40 ms (15%), a prelude-heavy
  benchmark 157 → 154 ms, a call-free loop unchanged, machine-compile time
  unchanged, TS suite byte-identical both ways.
- **`RTS_LAZY_SHAPE`** — put the shaped own-slot read AHEAD of the dictionary and
  `Entry::Rtse` probes in `rtsadp_obj_get`. `=0` restores the pre-Tier-3.3 order.
  Lives in the runtime, not `clifflags.rs`, because the lowering emits the
  identical call either way.
- **`RTS_ASYNC_SM`** — async lowered to a cooperative state machine instead of
  the thread-blocking fallback.

---

## §3 Optimization knobs — OFF by default

| Variable | Default | Accepted | Reads |
|---|---|---|---|
| `RTS_REGIONS` | OFF | opt-in (`=1`) | `rts-natives/src/heap/regions.rs` |
| `RTS_SLAB` | OFF | opt-in (`=1`) | `rts-natives/src/heap/slab/mod.rs` |
| `RTS_BUMP` | OFF | opt-in (`=1`) | `rts-natives/src/heap/bump.rs` |
| `RTS_INT_OVERFLOW` | OFF | opt-in (`=1`) | `front/run/clifflags.rs` |
| `RTS_JIT_CACHE` | OFF | `1` or `true` | `front/run/progcache.rs` |
| `RTS_CLIF_CACHE` | OFF | opt-in (`=1`) | `front/run/clifcache.rs` |
| `RTS_ALL_THUNKS` | OFF (forced ON for AOT) | opt-in (`=1`) | `front/run/thunk.rs` |

Full reasoning per row is in §1. Additional detail:

- **`RTS_REGIONS`** — deterministic thread→region allocation affinity instead of
  smearing each thread's objects across all 32 shards. Beyond lock contention it
  is the dependency of `RTS_CLASS_IMPLEMENTATION.md` C5: with one region per
  thread the slot-table base stops being a `load [SLOT_TABLES + shard*8]` and
  becomes an `iconst`. Unset or `0` reproduces the historical global round-robin
  exactly — same counter, same modulus.
- **`RTS_SLAB`** — chunked, stable-address slot storage per shard. Must be stable
  for the process lifetime, hence the `OnceLock`. When OFF, the chunk-table base
  address accessor returns `0` deliberately, so codegen that skips the check gets
  a null-deref (loud) rather than a wrong pointer (silent).
- **`RTS_BUMP`** — per-thread recycling of object payload buffers. Pool classes
  are 4/8/16/32 words, sized from the real shaped-object layout (`1 + field_count`
  words). Wider allocations are collections, not instances, and fall through to
  the global allocator.
- **`RTS_ALL_THUNKS`** — give EVERY function a thunk, skipping the address-taken
  analysis. **Forced ON for AOT regardless of the variable**, because an AOT object
  carries relocations emitted outside the lowering path (prelude class new-thunks
  and statics) and a missed mark there is `undefined symbol` at link time, not a
  slower start. The analysis only buys JIT startup, so the trade is only made
  where it pays.
- **`RTS_CLIF_CACHE_DIR`** — where the Cranelift incremental store lives.
  Default `.rts`. Store capped at 32 MB (past it, reads still hit, writes drop);
  under 4 MB the whole data file is slurped once per process.

---

## §4 Escape hatches — a feature is ON, this turns it OFF

| Variable | Effect | Accepted | Reads |
|---|---|---|---|
| `RTS_NO_PRELUDE_CACHE` | Disable the prelude lowering cache | exists (any value) | `front/run/prelude_cache.rs` |
| `RTS_NO_PRUNE` | Disable dead-prelude-function pruning | any non-empty, non-`0` | `front/run/prune.rs` |
| `RTS_GC_DISABLE` | Turn off the periodic collector | opt-in (`=1`) | `rts-natives/src/heap/live.rs` |

- **`RTS_NO_PRELUDE_CACHE`** — the cache is ON by default. It was briefly opt-in
  while a heap-corruption bug was open (the load path seeded the shape registry
  unconditionally, illegal for a NESTED compile from `new Function`/eval running
  while the outer program's shapes are live); fixed at the call site, so it is
  safe on. **Set this on both arms of any lowering-time A/B** — see §1.
- **`RTS_GC_DISABLE`** — for diagnosing a sweep that frees reachable handles
  (stack-scan / safepoint coverage bug). Cached on purpose: the collector being
  armed must not change under a running program.

---

## §5 Tuning — numbers, not switches

| Variable | Default | Reads |
|---|---|---|
| `RTS_INLINE_MAX` | `24` instructions | `front/run/inliner.rs` |
| `RTS_STACK_LIMIT` | `10000` frames | `rts-natives/src/collector/stack.rs` |
| `RTS_CODEGEN_JOBS` | `available_parallelism()`; `1` forces serial | `front/run/parcompile.rs` |
| `RTS_CODEGEN_CHUNK` | `1` (rayon's adaptive splitting) | same |
| `RTS_THREAD_POOL_SIZE` | `8` workers | `rts-std/src/thread/mod.rs` |
| `RTS_TEST_JOBS` | `available_parallelism()`, clamped to file count | `rts-cli/src/cli/test_cmd.rs` |
| `RTS_TEST_TIMEOUT` | `30` seconds per child; `0` disables | same |
| `RTS_OPT_LEVEL` | `speed` (`none` \| `speed` \| `speed_and_size`) | `rts-cli/src/compile_options.rs` |

- **`RTS_STACK_LIMIT`** — exceeding it sets the runtime error slot and raises a
  real `RangeError` ("Maximum call stack size exceeded"), catchable by user
  try/catch.
- **`RTS_TEST_TIMEOUT`** — the 30 s default is deliberately tight. It bounds a
  HANG, and every second is paid in full by a wedged file:
  `node_child_process_full` hangs under parallel load often enough that a 120 s
  default once cost the suite ~90 s (50 s → 2m16s) for a file the serial retry
  then passes.
- **`RTS_OPT_LEVEL=none`** trades slower code for faster compiles — useful for
  codegen debugging, never for a benchmark.

---

## §6 Diagnostics — output only, no behaviour change

All are OFF by default and correctly so; each prints to stderr or writes files.

| Variable | What it reports | Accepted | Reads |
|---|---|---|---|
| `RTS_TIMING` | Startup/compile phase wall times, plus clif-cache hit/miss | non-empty, non-`0` | `rts-codegen-new/src/timing.rs` |
| `RTS_AOT_TIMING` | AOT phase timing | `=1` | `rts-runtime/src/lib.rs` |
| `RTS_REPR_STATS` | BOX/UNBOX events by `file:line`, prelude counted separately | non-empty, non-`0` | `rts-codegen-new/src/stats.rs` |
| `RTS_CLIF_VERIFIER` | Force Cranelift's IR verifier ON in a RELEASE binary | `=1` | `front/run/clifflags.rs` |
| `RTS_GC_DEBUG` | GC mark/sweep events | exists | `rts-natives/src/collector/debug.rs` |
| `RTS_DEBUG_GCELL` | Global-cell events during a cycle | exists | `rts-natives/src/collector/cycle.rs` |
| `RTS_DEBUG_GEN` | Generator/delegate runtime events | exists | `rts-natives/src/collector/generator/delegate.rs` |
| `RTS_DIAG_GEN` | Generator capture-reference analysis decisions | exists | `rts-parser/src/gencapref.rs`, `lowering_items.rs` |
| `RTS_DEBUG_DESTRUCTURE` | Parameter-destructuring desugar decisions | exists | `front/run/desugar/destructure/params.rs` |
| `RTS_DEBUG_LITCAP` | Literal-capture decisions in the arrow lifter | exists | `front/run/funcval/mod.rs` |
| `RTS_DIAG_BAIL` | **Why** the arrow lifter REFUSED to extract an arrow | exists | same |
| `RTS_DIAG_UNBOUND=<name>` | Why free identifier `<name>` was not treated as a capture | must equal the identifier | same, + `stmt_assign.rs` |
| `RTS_DYNFN_DUMP=<dir>` | Write a failing `new Function` body + its error to `<dir>/dynfn_fail_N.js` | a directory path | `front/run/dynfn.rs` |

Two of these exist because the failure they diagnose is otherwise invisible:

- **`RTS_DIAG_UNBOUND`** — a wrong capture decision does not necessarily bail; it
  can silently resolve the name to something else and produce a plausible WRONG
  VALUE. That is how the top-level-shadowing bug was found. Keyed by name because
  the per-ident volume makes an unfiltered dump useless.
- **`RTS_DYNFN_DUMP`** — a dynamic function body exists only in memory (its span
  indexes no on-disk source), so without this there is no way to extract a
  failing function from a multi-megabyte bundle for a minimal repro.

---

## §7 Toolchain / linking

| Variable | Default | Reads |
|---|---|---|
| `RTS_TARGET` | host triple | `rts-linker/src/lib.rs` |
| `RTS_LINKER_BACKEND` | `auto` (`object`/`manual` \| `system`/`native`) | same |
| `RTS_WINDOWS_SUBSYSTEM` | unset (`console` \| `windows`/`gui`) | same |
| `RTS_RUNTIME_OBJECTS_DIR` | `runtime-objects` next to the `rts` binary | `src/runtime_objects.rs` |
| `RTS_BINARY` | resolve `rts` next to the current exe | `rts-std/src/runtime/mod.rs` |

`RTS_BINARY` is only honoured when the path exists and is a file; otherwise the
normal resolution runs.

---

## §8 Foreign variables RTS honours

Not ours — do not rename, do not reuse.

| Variable | Use |
|---|---|
| `RUST_BACKTRACE` | **Always set to `full`** before running `rts.exe`; the crash handler (`src/crash.rs`) needs it for full frames |
| `LIB` | Windows library search paths, read by the system linker path |
| `MACOSX_DEPLOYMENT_TARGET` | macOS minimum platform version passed to the linker |
| `HTTPS_PROXY` | HTTPS proxying |
| `PATH` | tool discovery |
| `HOME` / `USERPROFILE` / `USER` / `USERNAME` / `SHELL` | `node:os` shims |

---

## §9 Rules for adding a knob

1. **A knob is a MEASUREMENT device, not a feature flag.** It exists so a cost or
   a win is an A/B on ONE binary instead of an attribution across two builds.
2. **Cache it in a `OnceLock`** if it is read anywhere near a hot path.
   `std::env::var` locks the process environment and allocates a `String` per
   call. Anything whose choice must stay stable for the process (a storage
   representation, the collector being armed) must be cached regardless of cost.
3. **Say the default in the doc comment, and say WHY.** "OFF until measured" is a
   complete reason; "OFF" alone is not.
4. **State whether it is a SEMANTIC switch.** `RTS_INT_OVERFLOW` changes what a
   program PRINTS; `RTS_ESCAPE` must not. Conflating the two categories is how a
   correctness regression gets logged as a tuning result.
5. **Update this file in the same commit.** A stale default here is a lie the next
   session will act on.
