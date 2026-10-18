# Lazy per-function compilation — design plan (C1)

**Status:** design, NOT implemented. 2026-10-18. The single largest remaining
RAM lever for importing a real npm library (baileys, WhatsApp).

Grounded in: `.docs-dotnet/02-compilation/12-jit-vs-r2r-vs-native-aot.md`
(the JIT Tier0-on-first-call model), issues #2881 / #2884 (C1) / #2911, and the
rts compile pipeline read at `crates/rts-codegen/src/emit/mod.rs::emit_modules`.

---

## 1. The problem (measured in the issues)

`emit_modules` (`emit/mod.rs:1481`) **lowers every unit, then emits every unit
into ONE compilation/region** before anything runs. So importing a library
compiles **all** of its functions — issue #2911 measured 25k+ functions, ~4 GB
/ ~4.5 min; #2881 measured 549 MB / 97 s of CPU for a program whose actual run
was 272 ms. For a real library, **~5 % of the functions ever execute** (#2884
C1). The other ~95 % are lowered, optimized, and emitted to native code that is
never called — pure RAM and compile time.

This is the biggest single contributor, alongside #2883 (now fixed), to rts
using ~10 GB on baileys.

## 2. What .NET does, and what transfers

- **JIT (RyuJIT)**: compiles a method **on its first call** (Tier0 = fast
  codegen). The working set is "memory for the JIT and the code that ran" — a
  method never called is never compiled. Tiered compilation promotes hot
  methods to Tier1 later.
- **ReadyToRun (crossgen2)**: precompiles at build, but the UNIT is still the
  method, and the JIT recompiles hot ones. Keeps IL as a fallback.
- **Native AOT (ILC)**: compiles the **whole program** at build, **no JIT** at
  runtime (`RuntimeFeature.IsDynamicCodeSupported => false`). It keeps size
  down by **trimming to what is statically reachable**, not by laziness.

**The transferable idea for the JIT/run path:** compile a function's **body on
first call**, not at load — exactly `global_get_unbound`'s shape (a *miss* asks
the runtime to supply, then proceeds) applied one level up, to code.

**The transferable idea for the AOT path:** there is no runtime codegen, so
laziness does not apply; the AOT equivalent is **reachability trimming** (area
08 tree-shaking) — compile only functions the closed world can reach. That is a
separate plan; this document is about the JIT/`rts run` path.

## 3. The mechanism to reuse

`crates/rts-core/src/entry/global.rs`: `global_get` answers a *miss* by calling
`supply(context, name)` (`:61`, `:81`), which resolves and installs a global the
compiler never saw, then returns it. `global_get_unbound` (`:301`) is the same
for the scope-chain tail. **Lazy compile is this shape for function bodies:**

> a call to a not-yet-compiled function MISSES, a stub asks the runtime to
> compile the body, the body is installed (the cell's code pointer is patched),
> and the call proceeds.

The placement half already exists: `rts-host/src/live.rs` compiles **into the
region that is already there** (same base/stride/literal table as the running
context) — that is how dynamic `import()` places code at runtime today. A
lazily-compiled body uses the same placement.

## 4. The hard constraint: whole-program facts

`emit_modules` compiles the whole tree up front **on purpose** — several
optimizations are proven from "I have seen the entire program". Each must be
re-examined before a body can be compiled in isolation on first call:

- **Primordial folding** (`emit/primordial.rs`; the long comment at
  `emit/mod.rs:1531-1560`). `Math.sqrt(v)` lowers to `Inst::FloatUnary` ONLY
  because no unit in the whole tree reassigns `Math.sqrt`. If a body is compiled
  before the unit that does `Math.sqrt = ...` has been seen, the fold is a
  silent wrong answer (the exact bug recorded there, and in
  `docs/engine/lost-roots.md`).
- **Direct call to a light function** (`emit/direct_call.rs`,
  `emit/light_call.rs`): lowering `f()` to a direct `FuncAddr` call needs `f`
  proven declared-once / never-reassigned / captureless — a whole-program fact.
- **Scalar replacement / escape analysis** across call boundaries
  (`emit/escape.rs`): "this object never escapes" can depend on callees.

**Resolution:** compute these facts from a **cheap up-front SCAN**, not from
full lowering. The scan is lexical/AST-level (which names are assigned anywhere;
which functions are reassigned; reachability) and is cheap next to lowering +
regalloc + emission. Where a fact genuinely needs a callee's lowered body, the
lazy body compiles **conservatively** (no fold / go through the door) — correct,
just not maximally fast, for the cold functions that were never going to be hot
anyway.

## 5. Staged design

**Stage 0 — measure (first commit).** Counters (behind `RTS_COMPILE_STATS=1`):
functions lowered+emitted vs functions actually CALLED, for a real library
(baileys / a vendored npm package / the WhatsApp page). Confirm the ~5 %
run claim and the RAM/time split. No optimization until this is on the table
(`perf-claim` discipline).

**Stage 1 — the whole-program scan.** Before compiling, a pass over all units'
ASTs that records exactly the facts §4 lists (reassigned primordials, reachable
exports, light-function candidates) into `Ctx`. This replaces "whole tree
lowered" with "whole tree scanned" for those facts. Cheap and additive — the
eager path can keep using it too.

**Stage 2 — the lazy stub.** A function value's code pointer starts as a
**compile-on-first-call trampoline** (a sibling of the existing door
`__rts_call_counted`): on entry it compiles the function's body (using Stage-1
facts), patches the function cell's `FuncAddr` to the freshly placed code
(`live.rs`-style placement into the running region), and tail-calls it.
Subsequent calls hit the real code directly. This is V8's lazy compile / .NET
Tier0-on-first-call. Root/entry functions (the module top-levels that run
immediately) are compiled eagerly as today — only nested/library functions get
a stub.

**Stage 3 — retain the source, not the native code.** Lazy compile needs each
function's AST/source kept until first call. Retaining the AST is far cheaper
than emitting native code for the 95 % never called; and it ties into area 01's
"double parse" (walk + front_end) — if the AST is kept once, both the lazy body
and the parse dedup win. Alternative: re-parse the one function on demand (slower
first call, less retained memory) — measure which wins.

## 6. Risks and validation

- **Correctness is the whole game**: a wrong whole-program fact is a *silent
  wrong answer*, not a crash (the `Math.sqrt` class). Stage 1 must be proven to
  compute every fact the eager path got for free; the safest first cut compiles
  lazily but **disables every whole-program optimization on lazy bodies**
  (door calls, no primordial fold), turning the question into "is the answer
  identical, just slower on cold paths?" — then re-enable optimizations one at a
  time, each gated on a Stage-1 fact.
- **AOT is out of scope** (no runtime codegen) — AOT gets reachability trimming
  (area 08) instead; keep the two paths explicit.
- **Validate** with the full TS suite (888 `*.test.ts`, the honesty floor),
  byte-identical output on a corpus, and RSS/compile-time before/after on a real
  library (the Stage-0 program). A regression in peak speed on a hot function is
  acceptable only if stated; the goal here is RAM and startup, and the hot path
  tiers up (Stage 2 can recompile a hot lazy body with the full optimizer, the
  .NET Tier1 move) — but that is a later stage, not required for the RAM win.

## 7. Effort

**L, architectural.** Stage 0 is S (counters). Stage 1 is M (a scan + threading
facts). Stage 2 is the L core (the stub, placement, cell patching, and the
interaction with generators/async whose frames already suspend). Stage 3 is M
and overlaps area 01. Do them in order; each of 0→1 is independently
committable and 1 improves the eager path too.

## Pointers

- `crates/rts-codegen/src/emit/mod.rs:1481` — `emit_modules`, the eager
  whole-graph loop.
- `crates/rts-codegen/src/emit/mod.rs:1531-1560` — the primordial whole-program
  proof (the constraint §4).
- `crates/rts-core/src/entry/global.rs:61,81,301` — `supply` /
  `global_get_unbound`, the miss→runtime shape to reuse.
- `crates/rts-host/src/live.rs` — placing code into the running region at
  runtime (the placement half of Stage 2).
- `crates/rts-codegen/src/emit/{direct_call,light_call,escape,primordial}.rs` —
  the whole-program optimizations to re-examine.
- Issues #2881, #2884 (C1), #2911.
