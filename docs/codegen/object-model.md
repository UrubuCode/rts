# The object model: keep it, mend the surroundings

**The question this answers**, asked 2026-08-22: *"para reduzir ainda mais, o
hermes é ruim ou tem como melhorar?"* — to reduce further, is Hermes bad, or can
it be improved? The framing behind it: the object model follows Hermes for
objects and classes, `new` costs too many nanoseconds, and the plan was either to
**replace** the model or **fix** it to get objects close to machine potential.

**What produced this.** Six investigators, one per part of what `new` costs, each
required to cite a `file:line` they read; then every one of their 46 findings
handed to an independent verifier instructed to refute it. Where a finding was
refuted, the corrected version is carried and labelled. Section 5 lists the seven
places where investigators who could not see each other's work agreed — the
strongest signal in the data.

**Its numbers carry the caveat section 6 item 0 states**: `measurements.md` is
dated to `97f66385` and two of its rows are stale by one write barrier per cached
store. Every subtraction here is arithmetic on those rows. Re-measure before
quoting an absolute.

---

# The object model: keep it, mend the surroundings

*docs/codegen/object-model.md — 2026-08-22. Six investigations, every finding adversarially checked. Where a finding was refuted, the corrected version is carried and labelled.*

---

## 1. The verdict

**KEEP the hidden-class model. The model's own removable cost is 3–8 ns of a ~90 ns row; the money is in the collector, the call protocol, and the fixed-stride cell — none of which a different object model would change.**

The single strongest piece of evidence: `RTS_TIMING=1` over 200 000 iterations of `new Callee(); a += o.v` reports **8 resolver entries in total**. `context.resolves` is incremented at `crates/rts-core/src/entry/cache.rs:77`, the first statement of `resolve`, before any early return, so that is every entry into the cache resolver for the whole program — startup included. `ShapeTree::transition` (`crates/rts-cranelift/src/shape/tree.rs:118-145`) is never reached in the loop at all. Whatever the 90.89 ns is, it is not shape lookup.

Corroborated four more ways, by four investigators working independently: `RTS_CACHE_WHY=1` prints exactly 3 sampled `get` lines against a sampler gated on `resolves <= 3 || resolves % 20_000 == 0` (`cache.rs:177`), which bounds total resolves under 20 000; the transitions map is memoised globally at `tree.rs:137`; each site remembers a `(before-header → offset, after-header)` triple at `cache.rs:243-267`; and `cache.rs:204-206` records what that triple bought when it landed — 1 419 894 misses on `bench/analytic.ts` became ~0, of which 1 135 617 were this one site.

**Before anything else, reframe the comparison the question rests on.** `alloc object literal 2` = 1.22 ns is not a cheap allocation, it is a *deleted* one: `crates/rts-codegen/src/emit/escape.rs` scalar-replaces the object and `rts ir` shows the loop body reduced to one `FloatArith(Add)` with no `ObjectNew`. 1.22 against a 1.27 ns floor is a row that measures nothing about objects. The honest comparison is `alloc class instance` 90.89 against bun 0.53 and node 0.38 (`docs/codegen/measurements.md:94`), and against `alloc add prop after` 54.12 (`:96`), which does allocate.

---

## 2. What the row actually is

`bench/analytic.ts:126-131` — `class Callee { v: number = 1; m(x) { return x + this.v } }` — and `:259-265`, `const o = new Callee(); a += o.v`. **One field, not five**; `docs/codegen/plan.md:211`'s parenthetical "five `CachedSet`s in a five-field constructor, verified" describes a different class and must be corrected.

`rts ir` on that exact shape, per iteration:

| in the loop | in `FuncId(2) Callee` |
|---|---|
| `Guard` + `CachedGet Key(2)` — reading the binding `Callee` out of the enclosing scope object | `Guard` |
| `Call FuncId(8) __rts_construct(v8, undef, undef, undef, undef)` | `CachedSet Key(0)` — `this.v = 1`, a **growing** store |
| `WordLoad`/`Compare`/`Branch` — the throw check | |
| `Guard` + `CachedGet Key(3)` — `o.v` | |

**Three cached operations, not two.** The scope read is a hidden-class operation too — this engine reaches captured variables through shaped cells (`RTS_CACHE_WHY` shows the scope object resolving as `ty 1798 shape ShapeId(1029) holds ["__rts_outer","run"] slot Some(1)`). All three are steady-state hits.

Inside `__rts_construct` (`crates/rts-core/src/entry/functions.rs:619-692`): eight `with_current` crossings, four `Vec` push/pop pairs, and one transmuted jump into the compiled constructor at `:538`. `allocate_for_target` (`:866-916`) — verified by reading it — does `well_known("prototype")`, an `objects::read_property` loop, `typed_as`, `alloc_after_collecting`, `set_prototype`.

**`alloc_after_collecting` is called at `functions.rs:911`, INSIDE `allocate_for_target`.** Two verifiers caught the same double-count independently: any figure for "`allocate_for_target`" already contains the collector, and the collector cannot be listed beside it.

**The steady state, confirmed six times by four investigators.** `RTS_GC_DEBUG=1` over the 200 000-iteration loop prints three cycles, each `live ~1794 freed ~63 66x`. The region is 65 536 cells (`crates/rts-host/src/run.rs:1095`), the built-in world holds ~1 800. So **~0.955 `collect_cycle::release` per allocation** and ~1 `Region::each_live` visit. The independent runs landed at freed 63 662 / 63 666 / 63 667 / 63 669 / 63 672. This is the most-confirmed fact in the dataset.

**Two rows in the table of record are stale, and every subtraction below inherits it.** `docs/codegen/measurements.md:83` still reads `prop write own` **9.61** while `docs/codegen/hot-path-hygiene.md:228` records that row moving 9.37 → **4.59** when a write barrier that could never report anything was removed. The barrier was per *cached store*. `alloc class instance` contains one; `alloc add prop after` contains two (verified in IR). So the real rows today are ESTIMATE **~86** and **~44.5**, and nothing has been re-measured. That is the first item on the work list.

---

## 3. Where the 90.89 ns goes

Provenance tags: **IN-ENGINE** (measured on the real binary), **BENCH** (a row of `measurements.md`), **PROBE** (`crates/rts-core/examples/*.rs`, real engine code but no calling convention, no inline caches, tiny working set — its own header calls its rows a floor), **ISOLATED** (`bench/isolated/`, a model of a shape of Rust), **DERIVED**, **ESTIMATE**.

| part | ns | provenance | model or surroundings |
|---|---:|---|---|
| one non-inlinable call — convention, argument pad, throw check, callee prologue and frame, return; loop floor included once | **~20.7** | BENCH. `crates/rts-codegen/src/emit/inline.rs:5-8`: "`f(a)` costs 20.7 ns in `bench/analytic.ts` when `f` is `function f(x) { return x + 1 }`". A constructor uses `this` and is never inlinable (`inline.rs:19-25`) | surroundings — calling convention |
| amortised collector: one `release` (22 `Aside::remove`, `weak::clear_freed`, `finalize::queue_freed`, `Region::free`) plus one `each_live` visit | **10–14** | **IN-ENGINE, twice, independently.** `crates/rts-core/examples/alloc_cost.rs`, `sweeping − pre-freed`: 33.83 − 23.72 = **10.11** and 26.73 − 12.80 = **13.93**. Loaded machine, so an upper bound | surroundings |
| `Region::alloc` free-list path: read link, write header, write fifteen zero words (`crates/rts-core/src/heap/region/mod.rs:467-489`) | 4–8 | ISOLATED, disputed. `object_floor.rs` rows 0b−0a and 4−6 give 4.1–8.5 with non-overlapping brackets | surroundings — fixed stride |
| the two `CachedGet`s (scope `Callee`, then `o.v`) with their guards | ~3.7 | DERIVED from `prop read own` 4.97 − floor 1.27; that row's own loop is also two cached gets, so ~1.85 apiece | **model — delivering** |
| the constructor's one growing `CachedSet` | ≥3.3 | DERIVED from `prop write own` 4.59 − 1.27. A lower bound: a growing store also re-stores the header | **model — delivering** |
| `allocate_for_target`'s lookups: `well_known("prototype")`, one `own_property`, `typed_as` | 3–8 | ESTIMATE — corrected consensus of five investigations, never measured in-engine | **model — its uncached slow path** |
| `construct`/`construct_inner` over a plain call: three net `with_current`, four `Vec` push/pop, `callable_at`, `is_derived`, `is_object_in` | 2–3 | PROBE, `construct(derived) − call(plain fn)`, three runs: 2.28 / 2.50 / 3.14 | surroundings |
| `set_prototype` — one `Aside::set` (a `Vec<Option<u64>>` store) | ~1 | ESTIMATE | model — the side-table choice |
| **named** | **~48–61** | | |
| **residual** | **~30–43 of 90.89, ~25–38 of a re-measured ~86** | | |

### The residual is not new, and it is half already ranked

`docs/codegen/plan.md:110-116` (§S1a, rank 2) decomposes `call method` = 29.35 and closes: *"That sums to roughly 10–12 ns. **Sixteen nanoseconds are unattributed**, and every method row in the table pays them."* `new Callee()` goes through the same `called`/`invoke` machinery, so it pays those sixteen too. So ~16 of the residual belongs to an open question the plan already owns, and ~14–27 is specific to `new`.

The second instrument puts that remainder in one place. `crates/rts-core/examples/construct_probe.rs` isolates `allocate_for_target` exactly — `construct_inner:665` hands a *derived* constructor `undefined` for `this` and never calls it, so `construct(empty) − construct(derived)` is that function and nothing else. It measures **44.5–47.4 ns**, against ~24 for a sum of its named parts. So both routes point at the same place: **`allocate_for_target` costs about twice what its named operations cost, and nobody knows why.**

Three honest caveats, none of which the data resolves:

- The probe's absolute scale is uncalibrated against the bench. Its `construct(field) warm` = 115.07 for what the bench measures at 90.89 — because `construct_probe.rs:92-99` calls the `set_property` *entry point* where compiled code uses a `CachedSet` whose hit path never calls it. The one row that would calibrate the two instruments (`set_property(o,v)`, which exists at `construct_probe.rs:192-198`) was never reported.
- The probe's rows moved by up to 2.2× between runs on this loaded machine (`object_new(0)` cold 25.4 / 55.3 / 43.0; warm 58.6 / 38.1 / 31.3; spreads 95–172%). It cannot support a 20 ns discrimination.
- Two named parts are themselves floors: `construct_probe.rs:44-48` states outright that its rows are floors, and `construct_probe.rs:138-139` pins `stack_high` to a local of `main`, so `roots::scan_stack` (`roots.rs:284-300`, which materialises a `Vec` of every word between the stack bounds) walks a handful of frames instead of a real thread stack, and `trace::mark` walks four closures instead of 1 794 live cells.

**So the honest statement is a band, not a number**, and the experiment that closes it is cheap: report the probe's `set_property` row, run it on a quiet machine with `stack_high` from the host's real thread base and a live set seeded to a few thousand cells, and report per-run pairings instead of a min and a spread. Until then, do not quote a residual to two decimals.

### Model-intrinsic share

Add up the model rows: the three cached operations (~7 ns) are the model **delivering** — they are the same 4.97 / 4.59 that bun does in 0.5, and they are 8% of the row. The model's *removable* cost is the `allocate_for_target` lookups plus `set_prototype`: **4–9 ns of ~86–91, i.e. 4–10%.**

That is the verdict number.

---

## 4. Is Hermes bad? Mechanism by mechanism

**A warning that applies to this whole section.** There is no Hermes source in this tree. `find . -iname "*hermes*"` returns nothing; `grep -rn "Hermes"` over `docs/` and `crates/` returns two incidental mentions (`docs/engine/objects-are-aggregates.md:3,15`, `crates/rts-core/src/entry/function_proto.rs:85`), neither about `parent_`, `rootClazzes_` or GetById. **Every Hermes claim below is UNVERIFIED — recalled, not read.** The RTS side of every row was read and is cited.

| mechanism | Hermes (UNVERIFIED) | RTS (cited) | verdict |
|---|---|---|---|
| **transition tree** | `HiddenClass` chain, `Transition{SymbolID, PropertyFlags}` | `ShapeTree`, keyed `(parent, key, Repr)`, `tree.rs:88,137,149` | **RTS slightly worse** — see below |
| **inline cache, direct read** | `PropertyCacheEntry{clazz, slot}`, one way, `if (cacheEntry->clazz == clazzPtr)` | one triple per site, header compare + constant-offset load, `lower/body.rs:647-661` | **same.** Both monomorphic |
| **inline slots + overflow** | `DIRECT_PROPERTY_SLOTS = 5` + `GCPointer<PropStorage>` | `INLINE_SLOTS = 15` (`region/mod.rs:179`) + `spill_set` (`objects.rs:995-1046`) | **same shape**, one asymmetry below |
| **prototype read on every `new`** | `Callable::createThisForConstruct_RJS` → `getNamed_RJS(prototype)`, no cache | `allocate_for_target` → `well_known` + `read_property`, no cache | **same — and copying it is the defect.** Hermes amortises it against interpreter dispatch; RTS pays it in a 90 ns budget |
| **caching a *transition*** | none — PutById's fast path fires only when `hasOwnProp` | `(before-header → offset, after-header)` at `cache.rs:214-266`; the machine writes the after-header itself | **RTS ahead**, and measured: 1 419 894 misses removed |
| **where the prototype lives** | `GCPointer<JSObject> parent_` in the object; parent re-derived at the access | side table (`aside.rs:19`, `context.rs:185-192`); link encoded in the type number by `typed_as` (`context.rs:62-94`) | **RTS worse on polymorphism, unmeasured** — see below |
| **dictionary mode, bounded tree** | `kDictionaryThreshold = 64`, weak transition map | none; the tree only grows | **different, and RTS's is defensible** — see below |
| **cache throttle** | `PROPERTY_CACHING_DISABLED`, `isDictionaryNoCache()` | the *chain* resolver has one (`REFUSED`, `cache.rs:433`, checked at `:551-553`); the *direct* one does not | **same-ish; copying it here would be wrong** |
| **slot lookup** | one probe of `propertyMap_` | three probes: `contains_key`, `Index`, then the inner `get` (`tree.rs:212-214`, `:280-291`) | **RTS worse by 1–2 probes** |

### Where RTS is worse, in order of size

**1. Polymorphism tolerance (unmeasured, and the only genuine model-level argument against KEEP).** Because the prototype lives in an `Aside` that compiled code cannot reach, the only way to prove a chain-cache sound is to encode the link in the type number — `context.rs:40-53` states it: a cache "has to distinguish two objects that hold the same fields and inherit from different places, and the only thing it compares is the type number". The consequence: two classes with identical fields carry different `TypeId`s and thrash a monomorphic site, where Hermes (re-deriving the parent from the object) would serve both from one entry. Measured shape of the cost, from `RTS_TIMING=1`: a three-shape site over 200 000 iterations reports **200 022** resolves against **8** for the monomorphic form. Whether that matters is unknown — all twelve `bench/*.ts` show under 20 000 resolves per entire run, and the repo's one app-shaped TS program cannot be run here (`rts:buffer` is missing). Measure a real program before deciding anything.

**2. `Repr` in the transition key.** RTS keys transitions on the value's representation; Hermes does not. Confirmed with a control the original finding did not run: number/string alternation at one write site → 13 sampled resolve lines (≥200 000 resolves); number/number → 1; string/boolean → 1. So `Repr` is the discriminator. But the *scope* is narrow, and this was the correction that mattered: compiled writes all grow the shape with `Repr::Tagged` (`cache.rs:230`), so `new C()`, object literals and literal-key `o.x = v` **cannot** split (3, 2 and 3 lines, identical to their monomorphic twins). Only a write routed through `objects::put` (a computed key or a native) followed by a cached by-name read pays it. Two of six runtime transition sites choose `F64` from an observed value — `objects.rs:514-518` and `array.rs:151` — and nothing consumes the difference: both reprs are 64 bits (`repr/mod.rs:108`), the collector walks slots conservatively (`trace.rs:152-162`), and `traced_offsets` has no consumer outside `mem/layout.rs`. Worse, the repr is not an invariant: `objects.rs:480-491` stores any value into an existing slot without checking, so a shape can claim `F64` while holding a pointer. `crates/rts-cranelift/README.md` rule 11 says widening is automatic and narrowing never is; `objects.rs:514-518` narrows from one observed value at run time. **Make both observing sites write `Tagged`.** Dropping `Repr` from the key entirely is a separate machine-layer decision that first needs a check on whether the array `length` layout may merge with a plain object's. Already stated in words at `cache.rs:207-213`.

**3. A cached store cannot reach the overflow; a cached read can.** `cache.rs:735` resolves stores with `Reaches::Cell`; `cache.rs:288-292` answers −1 for any slot past the cell; `lower/body.rs:1032-1044` stores at `address + offset` with no indirection. So a store to the 15th property onward resolves by name forever. The reason is correct and recorded (`cache.rs:60-66`: answering with an overflow offset would write into the receiver's own cell at another property's position — a regression that shipped once, `b9df2d9d`). Measured exposure: `RTS_CACHE_CENSUS=1` over 400 `tests/*.test.ts` fires this reason **three times in two files**, neither in a loop, and **zero times** in `bench/analytic.ts` and `bench/objbench.ts`. Dormant. If it is ever closed, use a fourth word — every cache site is already eight words / 64 bytes with two of pure padding (`target/mod.rs:761-783`) — not a sign-overloaded word two, which `plan.md:207` (§S7) wants *removed* from the read path, not given a third meaning.

**4. `slot_of` pays a redundant probe.** `index_of` does `contains_key`, then `Index` (which is `get().expect()`), and the caller then does `get` on the inner map. Confirmed not elided by LLVM: a faithful replica's asm re-executes the hash and a second full SIMD probe loop on the hit path. Worth ~1 ns per otherwise-uncached lookup. The `entry()` form does not compile (E0502 — `properties(shape)` borrows `self.nodes` while `entry()` holds `self.indexes`); the get-first form does. And since `ShapeId` is dense (`ShapeId(self.nodes.len() as u32)`, `tree.rs:142`), `indexes` can be a `Vec<Option<HashMap<Key,u32>>>`, which removes **two** of the three and is what actually reaches Hermes's one-probe `findProperty`.

### Where RTS's difference is defensible, not a defect

**No dictionary mode.** Growth-only is a *soundness* property here, not laziness. `tree.rs:150-159`: a node is shared by everything that extends it, so unlinking one would change a layout other objects are using. `context.rs:45-54` names the failure direction: a mutable tree would let `delete o.x` merge two layouts, "which makes the cache HIT where it must miss. Every other part of shape identity fails toward 'different'; that one would have failed toward 'same'." And repeated deletes cost nothing anyway — `tree.rs:137` returns the memoised child before minting; a 50 000-iteration `delete o.y` loop reports **4** resolves total.

**No throttle on the direct resolver.** Hermes can disable caching because its fallback is the ordinary interpreter path. Here the fallback is `__rts_get_property` and a chain walk, which `entry/mod.rs:202-204` prices at "99 ns against 4.8 for an ordinary property". Throttling would send a resolvable site down the expensive road. The right answer for a polymorphic site is a polymorphic *cell*, not a disabled one — and the room already exists in the five unused words of each 64-byte cache line.

**The prototype in a side table.** The cost half of the recorded refusal is mis-cited and should be corrected: `region/mod.rs:153-178` (15 → 31 slots, a scene frame 0.86 → 1.12 ms) is about cell **width**, not about spending one of fifteen slots. Putting a parent word in slot 14 changes `STRIDE` not at all; what it costs is that the uncacheable cliff moves from the 15th property to the 14th. The decision stands — `typed_as` already proves the link for zero extra loads out of a word that must be loaded anyway — but for the right reason.

### The answer to the question as asked

**Hermes is not bad, and RTS's version of it is not where the nanoseconds are.** RTS is ahead of Hermes in one place that was measured to matter (caching the transition itself), behind in three small ones (a repr in the transition key on a narrow path, a store that cannot reach the overflow in code nobody in this corpus writes, one redundant hash probe), and identical in the rest. Under every reading, the model's removable share is 4–10% of the row.

---

## 5. Where independent investigators agreed

Agreement between agents who did not see each other's work is the strongest signal in the dataset. Seven convergences:

1. **One `release` per allocation.** Four agents, six `RTS_GC_DEBUG=1` runs, freed counts within 10 cells of each other.
2. **The transition machinery is off the hot loop.** Six agents, three different switches (`RTS_CACHE_CENSUS`, `RTS_CACHE_WHY`, `RTS_TIMING`), converging on 8 resolves in 200 000 iterations.
3. **The prototype read does *not* walk the chain.** Reached twice independently — once empirically (`Object.getOwnPropertyNames(Callee)` → `["prototype","name","length"]`) and once from source (`functions.rs:99-121`: `closure_new` eagerly `put`s a `prototype` on every function). Both corrections say the same thing: `plan.md:227`'s "~6 FxHashMap probes" is **3**, and this *shrinks* the prize it was written to justify.
4. **The census counts refusals, not misses.** Found three times independently. `explain` (`cache.rs:79-104`) is called only on `return -1` paths; every successful resolve at `:157`, `:268`, `:318`, `:341` bypasses it. `cache.rs:400` prints the count as "misses". A site that re-resolves 200 000 times reports `0 misses, 0 sites`.
5. **`allocate_for_target` contains the collector.** Two agents caught the same double-count, at `functions.rs:911`.
6. **The prototype+type memo is the right fix, and the naive keying is a wrong answer.** Five investigations converge on `plan.md:302` (L3); three verifiers independently found the same failure — key by the callee and `new B()` on a bound function inherits from the wrong prototype. One of them *ran* it: `function A(){}; const B = A.bind(null); new B(); A.prototype = {tag:1}; new B().tag` answers **1** today (matching Node) and would answer `undefined` under the memo as first specified.
7. **The isolated sweep models overstate.** Four separate checks. `bench/isolated/src/bin/sweep_tax.rs`'s headline (52.93 ns/alloc) exceeds `alloc add prop after`'s *whole* 54.12 ns row, which pays exactly the same one release plus two `CachedSet`s and a `CachedGet`; a rerun produced a physically impossible row (deleting fifteen word-writes made it 1.9× slower); `release_shape.rs`/`release_pack.rs` showed the model's 22-deep accumulator serialising misses that the engine's straight-line code overlaps (167.90 → 87.50 when the answers are discarded the way the engine discards them). **The in-engine instrument beat all of them and should be the one of record.**

---

## 6. What to do, ranked

Ranking is ns removed × confidence ÷ effort. Every item names the experiment that must run *before* the engine is touched.

### 0. Re-measure the table. Effort S. Zero engineering. Do this first.
`measurements.md` is dated to `97f66385` and still carries `prop write own` = 9.61 against `hot-path-hygiene.md:228`'s 4.59. Two allocation rows contain that barrier once and twice. **Every subtraction in this document is arithmetic on stale rows.** One quiet-machine run of `bench/analytic.ts`, plus a note beside each row saying which binary produced it. Without this the rest is not rankable.

### 1. L3 — memoise the constructor's prototype and type. 3–8 ns. Effort M.
`docs/codegen/plan.md:302`, unchanged in substance and corrected in three places by this round:
- Key by the **resolved target cell** — `functions.rs:873`, from `new_targets.last()` — **and after the bound walk at `:891-902`**, or a bound function serves a stale prototype (demonstrated above).
- Invalidate from `objects::put` (`objects.rs:446`) on the `prototype` key, which covers `Object.defineProperty` (`descriptor.rs:319`), **and from `computed::delete_own` (`computed/query.rs:192`)** — `F.prototype` is configurable here (`functions.rs:115` uses `native::hidden`) and `delete F.prototype` succeeds today, verified by running it.
- Clear it in `collect_cycle::release` — a 23rd `Aside::remove` per freed cell, on the free path this same row already pays — and root it in `roots.rs:160-176`, beside `type_names`/`well_known_texts`/`key_texts_as_values`, each of which carries a comment naming the exact use-after-collect this would otherwise reproduce.

**Experiment first (S):** a counter beside `functions.rs:889`. It must equal the construction count exactly today. Then a `construct_probe.rs` row with the pair pre-resolved, against `construct(empty) warm`. The gap is the whole prize, priced before the engine is edited.

**Cost:** a second source of one fact, admissible only because the writers are enumerable. **A missed invalidation is not a slow answer — it is `new C()` producing an object that inherits from the old prototype.** Pin it with a test that reassigns `C.prototype` mid-loop, one that deletes it, and one on `A.bind(null)`.

### 2. Hoist an emptiness check out of the sweep loop. Part of 10–14 ns. Effort S–M.
`release` (`collect_cycle.rs:151-235`) makes 22 unconditional `Aside::remove` calls — verified by `grep -c`, correcting `plan.md:165/:228/:333`'s "~26". In a `new C()` loop exactly **one** of the 22 has been grown (`prototypes`, from `functions.rs:912`); the other 21 are bounds checks against a zero-length `Vec`.

**Do NOT build the per-cell bitmask** that four investigations proposed. Three verifiers refused it and the argument is the same each time: a mask moves the exhaustiveness obligation from 22 lines in one function to 47 `Aside::set` sites, and `collect_cycle.rs:178-188` records what one missed line already cost — `detached` left behind was **corruption, not a leak**, reproducing only at 80 000 iterations once cells began coming back. In `trace::edges_of` the failure direction is worse still: a missed bit means an edge is not followed and a **live object is freed**.

**The safe form:** compute once per *cycle* which of the 22 tables have `entries.len() != 0`, and skip those removes for the whole cycle. Same prize on this workload, no new table, no set-side obligation, one source of truth (the tables' own length), and a forgotten table stays a leak instead of becoming corruption.

**Experiment:** `crates/rts-core/examples/alloc_cost.rs` already measures the whole envelope in-engine. Add a row with the hoist. Also add a `reach()` dump so the "one dense table" model is a number rather than an assumption.

### 3. `finalize::queue_freed` — a scalability cliff, 0 ns today. Effort S.
`finalize.rs:86-97` is a full linear walk of `Context::deaths` with an O(n) `Vec::remove` inside it, run **per freed cell**. Measured in-engine: **+22–40 ns per allocation at 100 registrations, +243–291 at 1000.** It is reachable from JavaScript through `FinalizationRegistry.register` (`collections/finalization.rs:187`).

Its twin `weak::clear_freed` (`weak.rs:74-80`) is the same shape and **unreachable from JavaScript** — three investigators found this independently. `WeakRef` holds its target strongly as an ordinary `"__target__"` property (`collections/weakref.rs:5-11`); the only producers of `context.weak` are `rts-napi` (`references.rs:78`, `tags.rs:83`, `wrap.rs:191`, `finalizers.rs:119`).

**Fix with a threshold, not an unconditional `Aside`.** An `Aside` grows to `cell + 1` on first write (`aside.rs:125-131`), so one registration at a high cell index materialises a 65 536-entry table that the sweep then streams per freed cell — worse than the scan for the handful-of-registrations case that is normal. Scan while small, index when large.

**Experiment:** already built. `alloc_cost.rs` has the 100- and 1000-registration rows. Run it quiet.

### 4. Stop recomputing what `own_property` just loaded. 1–2 ns. Effort S.
`objects.rs:619-625` reads `region.type_of` and `context.shape_of`, then calls `slot_value` (`:963-967`) → `owned_slots` (`:942-946`), which reads both again plus `width_of`. Pure deletion, no new source of truth. Subsumed on the `new` path if L3 lands, but it also serves every `objects::put` growth and every cache miss.

### 5. `slot_of`: two probes, then one. ~1 ns per uncached lookup. Effort S.
Get-first fast path in `slot_of` (the `entry()` form does not compile). Then, since `ShapeId` is dense, make `indexes` a `Vec<Option<_>>`. **Bundle with the duplicate found next door:** `cache.rs:216` (`slot_of(shape,key).is_none()`, gating growth) and `cache.rs:270` (`let Some(slot) = slot_of(shape,key)`) are the *same lookup* — a non-growing store resolve asks twice, six probes for one answer. Keep the `Option`.

### 6. `Region::alloc`'s zero fill as `fill(0)`. ≤2 ns, unmeasured. Effort S.
`region/mod.rs:485-487` is an indexed loop. Emitted asm, opt-level 3: **~81 instructions with 15 bounds checks and a panic frame**, against **18 instructions with 2 compares and seven `movups`** for `words.get_mut(range).fill(0)`. Same semantics, same guarantee, same line of code. This is `plan.md:171`'s experiment (I) of §S4 (`bench/isolated/src/bin/region_reuse.rs`), never written.

**Do not remove the fill and do not move it.** It is required — `region/mod.rs:481-487` plus the test at `:935`, and worse than the test says: `trace::edges_of` walks `0..owned` unconditionally, so a leftover reference-tagged word would mark or follow a dead cell. And moving it to `free` was measured neutral-to-worse and is mechanically neutral (the same lines are dirtied either way).

### 7. Fix the census. 0 ns. Effort S. Two lines.
`context.resolves` already exists (`cache.rs:77`) and `crates/rts-core/src/entry/mod.rs:720-726` already documents it as "the ONE number that separates 'the cache works' from 'the cache is a slower way of calling'". `census_report` (`cache.rs:400`) just does not print it, and labels refusals as "misses". Three investigations had to work around this instrument. Print `resolves` beside the refusal count, and rename the label.

### Correctness, off the nanosecond list, ships regardless

**Class fields are `[[Set]]`, not `CreateDataProperty`.** `class.rs:690-695` synthesises `this.k = e` as an ordinary assignment reaching `property::emit_write` → `RuntimeOp::SetProperty`. `class A { set v(x){…} get v(){…} } class B extends A { v = 1 }`: node prints `1`, bun prints `1`, rts runs the base setter and prints `99`. It is not one bug: `accessor.rs:253` (`define_method` **is** `set_property`, so a prototype setter intercepts a method definition too) and `object.rs:207` (an object literal's property) have it as well — and for literals, escape analysis already produces **two different answers for one program** depending on whether the object is flattened. The fix is one define primitive over `objects::put` (`objects.rs:446`), with the non-extensible case throwing rather than no-op'ing, used by all three call sites. `defineProperty(Object.prototype, …)` appears nowhere in `tests/`; add the fixtures first.

**`math_primordial` and `inlinable` are proved per-unit on the module path.** `mod.rs:1089`/`:1092` compute them inside `emit_unit`, once per module, while `primordial.rs:26-28` conditions the whole optimisation on the proof being **whole-program** and `:10-11` says "a wrong guess here is not slower code, it is a wrong answer". Demonstrated: module A assigns `Math.sqrt = () => 999`, module B imports a tag from A and prints `Math.sqrt(16)`. rts prints **4**; bun and node print **999**. The same source in one file prints 999 everywhere. Union the proof over every unit before the `mod.rs:1039` loop.

---

## 7. The ceiling

**ESTIMATE: `alloc class instance` becomes ~66–80 ns if every item above lands.** Against bun 0.53 and node 0.38.

Derivation, and it is deliberately pessimistic about what is known:

| | ns |
|---|---:|
| the row, re-measured after the barrier removal (one `CachedSet` × ~4.8 ns) | ~86 |
| L3, the prototype+type memo | −3 to −8 |
| the sweep's per-cycle emptiness hoist | −3 to −7 |
| `owned_slots` not recomputing | −1 to −2 |
| `slot_of`'s redundant probe | −1 |
| `fill(0)` instead of the indexed loop | −0 to −2 |
| **result** | **~66–80** |

**The list does not close the gap, and it cannot.** That is the most important sentence in this document. The remaining ~66 ns is not object-model shaped. Four structural facts own it, and none of them is a hidden-class question:

1. **The allocation is a call, not an instruction.** bun and node inline it into JIT'd code. Here `new` crosses `__rts_construct` and runs `allocate_for_target` in Rust. `crates/rts-codegen/src/emit/inline.rs:5-8` prices a non-inlinable call in this bench at **20.7 ns**, and a constructor uses `this` so it is never inlinable.
2. **The cell is 128 bytes.** `INLINE_SLOTS = 15`, `STRIDE = 128` (`region/mod.rs:179,182`), fixed for the whole heap because a reference is an index and `address = base + index × stride`. A 16-byte object gets 128 bytes. bun's and node's are 16–32.
3. **Every allocation pays a sweep.** 10–14 ns in-engine, measured twice. A generational nursery pays none for the common case.
4. **The constructor call goes through the runtime.** `invoke` (`functions.rs:482-542`) transmutes a pointer and jumps; `plan.md:373` records there is no direct-call path at all.

The path to single digits is: inline the allocation into compiled code with a fast-path bump and a slow-path call; a nursery so the ordinary allocation pays no `release`; and a construct-site cache so the type and the prototype are constants at the site rather than lookups. That is a heap-and-calling-convention programme measured in quarters, and it is **orthogonal to the object model**. Replacing hidden classes with anything — dictionaries, Hermes verbatim, V8's maps — moves none of the four.

---

## 8. The strongest argument against the verdict, answered

### The argument

*The 128-byte fixed stride is forced by index-not-address addressing, and this repository's own measurements show cell density dominating whole-program time at a rate no cache removes.* `region/mod.rs:153-178`: 15 → 31 slots made a scene engine's frame 0.86 → 1.12 ms, **30% worse**, with nothing overflowing in either configuration — the only variable was cell size. `region/mod.rs:127-141`: 7 → 15 made `bench/objbench.ts` 5.66 → 6.99 s, **+23.5%**. So halving the cell should be worth a comparable share in the other direction, and RTS cannot give a one-field object a small cell. If the object model cannot express a small object, the object model is the problem.

### The answer

**The measurement is real; the target is wrong, in four steps.**

**(a) It is a heap decision, not an object-model one.** `crates/rts-cranelift/src/mem/address.rs` and `crates/rts-core/src/heap/region/mod.rs` do not mention shapes. The stride survives any object model. The strongest form of this argument concedes it in its own closing sentence: "the index-not-address decision survives intact, which is why this is a MEND and not a REPLACE."

**(b) The tree already walked the left half of that curve and rejected it.** At seven slots a property read was **265 ns instead of 14.7** and `bench/repro/` had **1 135 690 cache misses instead of 75** (`region/mod.rs:120-138`), because `cache_resolve` answers −1 for a slot past the cell (`cache.rs:277-290`). Smaller cells move the uncacheable cliff from the 15th property to the 7th. The curve is not symmetric; it has a cliff on one side.

**(c) The fix that removes the cliff instead of moving it is already named**, at `region/mod.rs:144-151`: make the overflow addressable so a cached read — and, per §4's asymmetry, a cached *store* — can reach it. The storage exists (`spill_of` puts the block in the region at a stable address, `objects.rs:996-1010`); the machine's half does not. If that lands, seven slots becomes measurable on its merits and the whole trade re-opens honestly. That is the change this argument actually supports.

**(d) Per-shard strides are refused with numbers, not with taste.** `crates/rts-host/src/run.rs:1092-1094` states it: the sharded form "costs a mask, a load and an add on every access, and a program that never asked for a second thread must not pay them" — paid per property read (4.97) and write (4.59), which outnumber allocations by orders of magnitude. `gc/barrier.rs:52-54` re-arms the write barrier when `regions > 1`, giving back the 9.61 → 4.59 that `hot-path-hygiene.md:78-100` records. `Addressing::Sharded` holds one `stride` field and is region-per-*thread*; `Context` owns one region and `Region::decompose` refuses a foreign reference. And there is no forwarding pointer anywhere, so nothing could migrate an object between size classes when it grows past its cliff.

### The second argument, which is genuinely open

**RTS's type number encodes (shape, prototype), so two same-shaped classes thrash a monomorphic site where Hermes would not.** This *is* about the model. It is unmeasured, it does not touch the 90.89 (monomorphic), and it could matter a great deal in real code. The probe is cheap and already written twice (a two-class read loop; `poly2.ts` against `mono.ts`). **Measure it on a real program before deciding anything** — and note the mitigation is a 2-way cache inside the five words each 64-byte cache cell already wastes (`target/mod.rs:757-786`), not a model change. A 4-way cache is refused: twelve words is two cache lines per site, against the density result in (a).

---

## 9. What is not worth doing

Each of these was proposed by at least one investigator and refuted with evidence. The evidence is the deliverable.

**Model-level**

- **Replace the object model.** Removable model-intrinsic cost is 4–10% of the row.
- **Move the prototype into the cell.** `typed_as` already proves the link for zero extra loads out of a word that must be loaded anyway. *Correct the recorded reason*: the 15 → 31 measurement is about cell width; the actual cost is that the cliff moves to the 14th property.
- **Precompute a class's shape from declared fields.** `emit/types/classes.rs:18-30` says the resolution is unsound (`interface Foo` and `class Foo` share one `Name`) and closes "Slower, never wrong"; `Repr` is chosen by observing a value and a pre-seeded key never transitions; a seeded object claims every key with `0.0`.
- **Dictionary mode.** Contradicts `object/mod.rs:17-24` and `tree.rs:115-117`, and `integrity::retype` mints a fresh `TypeId` per call — in-place updates would be *more* unbounded than the tree they replace.
- **A 4-way polymorphic cache, now.** All twelve `bench/*.ts` show under 20 000 resolves per entire run. Two cache lines per site against a measured density result. Revisit only with a real program's numbers, and then 2-way inside the existing line.
- **A throttle on the direct resolver.** The fallback here is the 99 ns by-name path, not an interpreter.

**Emitter-level**

- **Scalar-replace class instances / flatten `new C()`.** It fires only where the instance is provably unused *as an object* — `escape.rs:542-543` kills any candidate reading a key the field list does not hold, `:751-753` kills any bare mention — so it removes 0 ns from every program that stores, returns, passes, or calls a method on an instance. What it removes is the *instrument*: the row would fall to ~1.27, repeating exactly what `plan.md:341` files as instrument defect #1 for the two object-literal rows, against `plan.md:312`'s explicit guard for the identical shape ("keep an escaping array row, or the last real array-allocation instrument disappears with the allocation"). It also needs a TDZ answer no cited predicate supplies, and `escape.rs` is **1134 lines** against `crates/rts-codegen/README.md:95`'s 1000.
- **Inline the constructor.** Two independent barriers: `inline::declared_function` (`inline.rs:158-180`) never yields a class, so `shape_of` and `closed_over_parameters` are never even asked; and `emit_substituted` has exactly one caller, on the `ExprKind::Call` path (`call.rs:83`), while `new C()` is `ExprKind::New` (`expr.rs:176-178`). `inline.rs:6-13` makes it doctrine: no deoptimiser, so the answer must be a fact about the whole program.

**Collector-level**

- **A per-cell bitmask of which side tables hold something.** Refused three times; see item 2 of §6.
- **Lazy sweep / cursor reuse instead of a free list.** `release` reclaims four things that are not the cell — a text slab payload (`collect_cycle.rs:161-166`), a spanning overflow block (`:168-170`), an array (`:171-173`), a buffer (`:174-176`), a generator frame (`:216-218`). Deferring to reuse never reclaims two of them, because a cursor that skips `spanned_interior` walks *past* the overflow block and the frame before reaching the owner. It also deletes `FREE_MARKER`, on which `alloc_spanning`'s only `free_runs` producer, the unconditional double-free refusal (`region/mod.rs:511-539`) and `width_of`'s `None` all depend — and `span.rs:66-75` records that free-runs failure already happening once ("64 154 cells freed by a collection and the very next spanning allocation still refused").
- **Bound `trace::edges_of` by the shape.** Already refused at `plan.md:173`, and the refusal is correct: `Context::record_shape` (`context.rs:104-111`) fills intervening indices with an unrelated shape via `resize`, so `shape_of` can answer a shape *shorter* than the cell, a live reference is never marked, and the sweep frees it. Use-after-free, not retention.
- **Move the zero fill to `free`.** Measured worse and mechanically neutral — the same lines are dirtied either way.
- **Read a live cell's slots as a slice in `edges_of`.** ~0. The two `decompose` calls are already hoisted out of the loop by LLVM; only 15 header loads survive, at ~0.3 ns per live cell and 0.028 live-cell visits per allocation.
- **Grow before collecting when the last cycle found nothing.** `GROWTH_CEILING = 8` over 65 536 cells allows three doublings *ever*, so the whole cost is a bounded one-off. The first cycle has no predecessor to read. And the heuristic as proposed latches — a skipped collection never refreshes the reading, pinning growth-first until the 64 MiB reservation is spent, which is the exact failure `alloc.rs:88-91` gives as the reason for the current order.

**Machine-level**

- **Merge `with_current` crossings.** `construct(derived) − call(plain fn)` = 2–3 ns, and most of that is `Aside` probes and `Vec` push/pops, not the crossings. `plan.md:237` already says "largely refuted"; `plan.md:373` records two *measured* negatives for merging borrows (`new-engine-speed.md:629-633`; `json/write.rs:330-337`, 1942 → 2084 ns). `functions.rs:448-452` records that holding a borrow across the jump panics on re-entry, "a deadlock this repository has already paid for once".
- **Branch instead of `select` in `lower_cached_set`.** 13 → 12 ops (you still must load `cell+16` and compare it to branch on it), and **0 ns on this row** because every constructor field write genuinely grows the shape. `plan.md:209` (§S7(b)) already owns it, with an experiment never run.
- **`Inst::AllocDynamic` as specified.** It lowers to the same `RtEntry::Alloc` call, so it removes no crossing — and `new C()` makes no `rts_alloc` call at all (`rts ir` shows one `__rts_construct` and no `Alloc`). *The direction is right and the specification is wrong*: what §7 wants is an inline bump with a slow-path call, which is a different instruction.
- **Per-shard strides / size-classed cells.** §8(d).

---

## 10. Corrections this investigation owes the tree

Not optional. `CLAUDE.md` RULE 0: never leave a rule the code contradicts.

| where | what is wrong | what it should say |
|---|---|---|
| `docs/codegen/measurements.md:83,94,96` and four more rows | stale by one write barrier per cached store | re-take, or mark each row with the binary that produced it |
| `plan.md:227` | "walking the chain (~6 FxHashMap probes)" | `prototype` is an **own** property of every closure (`functions.rs:113-115`); one `own_property`, **3** probes, no walk |
| `plan.md:211` | "five `CachedSet`s in a five-field constructor, verified" | `Callee` has one field (`bench/analytic.ts:126-131`); this describes a different program |
| `plan.md:165,228,333` | "~26 `Aside::remove`" | **22** (`grep -c 'remove(cell)'`) |
| `plan.md:376` | "`escape::analyse` … has no `Ctx`"; "the bench's own class has a method, so it fails the proposal's own gate" | `escape.rs:123` imports `Ctx` and `escape.rs:795` takes `&mut Ctx`. The method clause is a category error — `escape.rs:542-543` gates per *access*, and `analytic.ts:262` reads only `o.v`. The grounds that survive: `escape.rs:336` admits only `ExprKind::Object`; `with_fields` needs emit-time computed-key `ValueId`s; `types::Classes` cannot supply a key list and is never even built on the module path |
| `cache.rs:400` | prints refusals as "misses" | print `context.resolves` beside them, and rename |
| `objects.rs:507-511` | "a later write of something else takes a different transition" | contradicted 30 lines above at `:480-491`, which stores any value into an existing slot without checking the repr |
| `region/mod.rs:145-147` | "It moves to the sixteenth property" | the fifteenth (slot 14) — `cache.rs:283-287` computes `width = width_of − 1` for the block address |
| `entry_probe.rs:66-88` | attributes its iteration bound to conservative stack scanning, and explains a 751% spread by "one round paying for a cycle" | **no cycle can run**: nothing sets `Context::stack_high`, so `collect_cycle::collect` returns 0 at `:79-85` (pinned by the test at `:278`). The spread is `Region::grow`'s `words.resize` first-touch commit. Its allocation rows are bump-path-only. `construct_probe.rs:63-68` repeats the same wrong reason its own `:129-133` refutes |
| `docs/engine/objects-are-aggregates.md` | describes rts-core's heap as `Slab<Cell>` with `Vec` slots; lists "`RtEntry::Alloc` gets an implementation" as future work | both long done — `heap/region/mod.rs` is the fixed-stride region, `rts-host/src/entries.rs:444` wires the entry |

**And one hygiene item.** This round left six untracked binaries in `bench/isolated/src/bin/` (`sweep_tax`, `sweep_release`, `release_shape`, `release_pack`, `mark_tax`, `object_floor`) and two in `crates/rts-core/examples/` (`alloc_cost`, `construct_probe`). Four of them answer the same question — `sweep_tax.rs:1` and `sweep_release.rs:1` are *both* titled "Experiment 8" and neither cites the other. `docs/codegen/README.md:146-150` rule 7 exists for this. **Keep `alloc_cost.rs` and `construct_probe.rs`** — they are the (C)-form in-engine instruments `plan.md:302` and `:333` ask for by name, and `alloc_cost.rs` produced the only number in this document that two independent runs agreed on. Consolidate the isolated four into one, record its RESULT table in its own module doc per rule 7, and note on every row that the machine was loaded.