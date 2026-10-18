# Multiple regions — region-per-module, the .NET ALC model (design plan)

**Status:** design, NOT implemented. 2026-10-18. The enabler under lazy
compilation (`lazy-compile-plan.md`), dynamic `import()`/`require()`, freeing
code, and per-region size classes. Prompted by: "there must be a way to split
into several regions — see how .NET does it."

Grounded in: `crates/rts-cranelift/src/mem/address.rs` (the addressing forms
rts already has), `crates/rts-host/src/live.rs` (the single-region run path),
`crates/rts-node/src/module/require.rs:20-35` (the cross-region wall), and
`.docs-dotnet/01-module-resolution/{01,08,12}` (AssemblyLoadContext +
LoaderAllocator).

---

## 1. The key fact: rts ALREADY has multi-region addressing

`mem/address.rs` has TWO addressing forms, and the choice is "a property of the
heap rather than of a call site":

- `Addressing::Single { base, stride }` — one contiguous region, address =
  `base + index*stride`, **two instructions**. What the single-region run path
  uses today.
- `Addressing::Sharded { table, selector_bits, stride }` — **several regions,
  selected by the low bits of the reference**; one extra load + one mask per
  access. Built because "a heap shared by several threads cannot be one
  contiguous region without synchronizing every allocation."

The GC already crosses regions: `lower/body.rs` computes a write barrier with
`barrier_for(..., region, ...)` and guards it with
`crossing_is_possible(bases.regions())`. And `compile_for(n > 1)` already emits
sharded code.

So the barrier is NOT the addressing — it exists. It is that the **JIT/run path
picks `Single`** and `live.rs` therefore "refuses rather than placing a second
compilation under the single-region addressing, which would build references
the first program reads as other cells." The references a region hands out are
**region-local slot indices with no region tag**, which is exactly why
`require.rs` says a loaded file's "exports would be cells this program cannot
touch."

**Splitting into several regions = running on `Sharded` and giving each module
(or each dynamically loaded / lazily compiled unit) its own region, so a
reference carries which region it belongs to.** The primitive is there; the
work is to use it on the run path and to make cross-region references first
class.

## 2. How .NET does it, point for point

.NET never bakes the whole program into one address space. Each assembly loads
into an **AssemblyLoadContext (ALC)**; cross-ALC references are resolved by the
binder, lazily, at JIT time (metadata token → address on first use).

The unit of memory is the **`LoaderAllocator`** (doc 08): for a collectible ALC,
**all** context-specific memory — type data (`MethodTable`/`EEClass`), JITed
code, stubs/thunks, the binding cache — comes from one `AssemblyLoaderAllocator`.
That is, one owner per region of memory. This is **exactly rts's "a region per
compilation"** (`live.rs`), named the same way.

Collectible ALCs **unload** (doc 08, two-phase): phase 1 on `Unload()` drops the
strong handle so the `LoaderAllocator` becomes collectable; phase 2, when the GC
proves no instance/type/assembly of the context is live, frees the native
allocator. So .NET **frees a whole region's code+types at once, keyed on the GC
proving the region dead** — the capability `live.rs` explicitly lacks ("Freeing
the code ... a program that compiles in an unbounded loop grows without bound").

Mapping:

| .NET | rts today | rts with this plan |
|---|---|---|
| ALC | a compilation / region | a region per module/unit |
| `LoaderAllocator` (one owner per region) | `RegionBase` + the region's cells | same, one per module |
| cross-ALC reference, binder-resolved | region-local index (no tag) → wall | tagged reference (selector bits) |
| collectible ALC, 2-phase unload | — (code never freed) | collectible region, GC-proved dead |
| NativeAOT: no unload, closed world | AOT single region | AOT stays single/closed (§6) |

## 3. What region-per-module unblocks (all RAM or capability)

1. **Dynamic `import()` / `require()` of files outside the graph** — the
   `live.rs` / `require.rs` / `node:vm` / `worker_threads` wall. Each loaded
   file compiles into its own region; its exports are tagged references the
   caller can hold. This is the capability several refused features share.
2. **Lazy per-function compilation** (`lazy-compile-plan.md`) — a function or
   module compiled on first use is placed into its own region instead of being
   refused by the single-region path.
3. **Freeing code = RAM** — a collectible region (the .NET collectible ALC) is
   freed when the collector proves nothing in it is reachable. Fixes `live.rs`'s
   unbounded growth and reclaims the code of modules/functions no longer live.
4. **Per-region size classes** (plan 11.2) — `address.rs` notes a size class "is
   naturally a shard with a stride of its own ... the reference already carries
   selector bits and the bases already come from a table, so the shape is almost
   there." A two-field object paying a 128-byte cell today is pure RAM; per-
   region size classes cut it. This rides the same mechanism.

## 4. The measured cost (stage 0 DONE, commit 762566773, 2026-10-18)

Measured in release with the probe (`fr_shard_sel` vs `fr_single_sel`, 200M
trips, min-of-5): a Sharded field read costs **+0.23 ns (~1 cycle)** over
Single with a run-time reference, and is equal with a constant reference (the
mask/shift fold). One-stride form confirmed (compile-time constant stride, one
for all shards). **So a field read is cheap enough for Sharded to be global.**

BUT the gating cost is NOT the read. `Sharded` sets `cursor()` and `runs()` to
`None` (`mem/address.rs`), which **disables inline cell-allocation and inline
run reads** — allocation would fall from the inline bump (~5-7 ns) to the
runtime door (~45 ns), a regression that violates "new method >= old".

**VERDICT: hybrid.** The base region stays `Single` (keeps inline alloc); only
loaded / dynamic / collectible regions are `Sharded`. The .NET-inspired path to
eventually-global is a **per-region allocation cursor** (the per-ALC allocation
context, a gen0 bump pointer per LoaderAllocator) so a `Sharded` region keeps
O(1) inline alloc — lifting the `cursor()==None` restriction is its own
sub-task, not a blocker for the hybrid start.

## 5. The hard parts (where the work actually is)

- **Cross-region references must carry the region.** Today a reference is a
  region-local index. Making exports usable across regions means the selector
  bits travel with every reference that can escape its region — the
  `address.rs` `Sharded` form already defines the encoding, but `rts-codegen`
  mints references without a region tag in single mode (`live.rs`: "letting a
  compilation START its numbering at an offset ... is a change to rts-codegen's
  Ctx"). This is the core change.
- **Collectible regions need the collector to track per-region liveness.** .NET
  does a two-phase unload keyed on the GC proving the ALC dead; rts's collector
  "does not know about code" (`live.rs`). Freeing a region safely needs a
  per-region reachability proof (no live reference carries that region's
  selector) — a real GC extension, the `LoaderAllocator` idea.
- **Whole-program optimizations** (the primordial fold, direct-call light
  proof, escape analysis — see `lazy-compile-plan.md` §4) assume one program.
  Cross-region calls go through a resolved handle (the door / a FuncAddr that
  the loading region fills in), like .NET's binder resolving a token — so a
  cross-region call is the dynamic door, not a direct call, unless proven.

## 6. AOT stays closed-world

Like NativeAOT (no runtime codegen, no unload), the AOT target keeps a single
closed-world region: there is nothing to load or free at runtime. Multiple
regions are a JIT/`rts run` capability. The AOT RAM lever is area-08 reachability
trimming, not regions. Keep the two paths explicit (the same split as
`lazy-compile-plan.md` §2).

## 7. Staging

0. **Measure the `Sharded` per-access cost** — DONE (commit 762566773): +0.23 ns
   / ~1 cycle per field read; verdict HYBRID because Sharded disables inline
   alloc (cursor()==None). See §4. The per-region allocation cursor (to let
   Sharded regions keep inline alloc) is a sub-task under stage 3.
1. **Region-tagged references in rts-codegen** (Ctx mints references with a
   selector; the base region is selector 0). The enabling change. M/L.
2. **Region-per-module for dynamic `import()`/`require()`** — the smallest real
   capability win; place a loaded file in its own region, hand back tagged
   exports; cross-region calls take the door. M.
3. **Collectible regions** — per-region liveness in the collector, two-phase
   free (the .NET LoaderAllocator). The RAM reclaim. L.
4. **Lazy-compile and per-region size classes** build on 1-3.

Each of 0 and 1 is independently committable; 0 gates everything (if the
per-access cost is too high globally, the whole shape becomes hybrid-only).

## Pointers

- `crates/rts-cranelift/src/mem/address.rs` — `Addressing::{Single, Sharded}`,
  the primitive that already exists.
- `crates/rts-host/src/live.rs` — the single-region run path and its three
  explicit gaps (sharded addressing refused, numbering can't start at an
  offset, code never freed).
- `crates/rts-node/src/module/require.rs:20-35` — the cross-region export wall.
- `crates/rts-cranelift/src/lower/body.rs` — `barrier_for` / `crossing_is_possible`
  (the GC already crosses regions).
- `.docs-dotnet/01-module-resolution/08-...` — collectible ALC + LoaderAllocator
  (one owner per region, two-phase unload).
- `docs/codegen/lazy-compile-plan.md` — the consumer of this capability.
