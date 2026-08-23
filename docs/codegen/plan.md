# The performance work list, 2026-08-21

**What produced this.** Ten investigators, one per cost area of the engine, each
reading the code against the measured table in [`measurements.md`](measurements.md)
and required to cite a `file:line` for every claim. Their 79 candidates were then
handed one at a time to independent verifiers whose instruction was to *refute*
them. **Fifty-five were refuted.** What follows is the twenty-four that survived,
merged where two areas found the same change from different directions, and
ranked by nanoseconds-removed × confidence ÷ effort.

**How to read it.** Agreement between independent areas is the strongest signal
in the data and is marked where it happened — one item was found by five areas
from five directions. The refutations are as valuable as the survivors and the
section "What I am NOT proposing" carries them, so that a good-sounding idea
that is already settled against does not get re-opened.

**Its numbers are the `97f66385` numbers.** Every `rts ir` observation was taken
against a binary built from that commit. Rows that have since moved are recorded
in [`hot-path-hygiene.md`](hot-path-hygiene.md) and one item — the `ElementLoad`
revival — was attempted, broke programs, and is written up in
[`element-load.md`](element-load.md). Re-measure before acting on a row.

**It is a plan, and plans go stale.** `docs/README.md` rule 1 applies: when an
item here is done or disproved, edit it here in the same change, or delete it.
The one thing not to do is leave it standing as though nobody had looked.

---

The tree has moved under the investigation. Before the ranking, the finding that reorders it.

---

# PLAN OF RECORD — RTS performance work, 2026‑08‑21

## 0. Read this first: five of the surviving candidates are already written, unmeasured, and uncommitted

Every `rts ir` observation in the ten investigations was taken against `target/release/rts.exe`, which `docs/codegen/measurements.md:11` states was **built from the tree at `97f66385`**. The working tree is now 19 files ahead of that commit, and the diff contains the implementations of five candidates the investigators independently proposed:

| already written (uncommitted) | proposed by | where |
|---|---|---|
| env‑var switches asked once | calls `hoist-cache-why-var-os` + natives `env-var-in-put` | `crates/rts-core/src/entry/switches.rs` (untracked), consumed at `objects.rs:546`, `cache.rs:177`, `cache.rs:484` |
| write barrier elided for a one‑region heap | entry‑tax + objects + props (**three areas**) | `gc/barrier.rs:52` `crossing_is_possible`, `lower/body.rs:1072,1187` `regions_can_cross` |
| `Inst::ElementLoad` unblocked | arrays `revive-element-load` | `RuntimeOp::ElementsCount` (`runtime/mod.rs:355`), `array.rs:665` `elements_count`, gate removed at `foreach.rs:415` |
| escape.rs template kill‑switch deleted | escape `drop-stale-template-kill-switch` | `emit/escape.rs`, module doc rewritten |
| `len_of` stops cloning the element vector | arrays `len-of-clone` | `array_proto/iterate.rs`, `staged` → `borrowed` |

Two consequences, and they are not small.

**The ElementLoad fix took the corrected form, not the proposed one.** `foreach.rs` no longer tests `repr_of(bound) == Repr::F64`; it asks the runtime for the run's own length through a new entry point. That is exactly what the arrays verdict specified as the correction ("have that same crossing answer the run's length… deleting the `repr_of(bound) == F64` test at `foreach.rs:393` outright") and it is the more correct bound as well — `array.rs:665`'s doc states that `set_length` writes the property *from* the element count, so the elements are the source and the property is derived.

**Several investigation claims are now stale.** "ElementLoad is dead code with no producer", "a single backtick disables scalar replacement for the whole body", and "`std::env::var_os` runs on every shape‑transition write" describe `97f66385`, not the tree. Do not re‑file them.

**Phase 0, before any item below.** Per `docs/codegen/README.md` rule 2 (isolation is a gate, not a forecast) and rule 5 (compared per row, never net):

```bash
cargo build --release && cp target/release/rts.exe target/baseline97f6.exe   # the committed tree
# stash nothing — the baseline is a binary, not a stash
cargo build --release                                                        # the working tree
./target/baseline97f6.exe run bench/analytic.ts > before.txt
./target/release/rts.exe   run bench/analytic.ts > after.txt
RTS_BIN=target/release/rts.exe REPORT_FILE=now.json bash scripts/cross_runtime_check.sh
```

Compare **per row**, and run a program ruler too (`bench/objbench.ts`, `monte_carlo_pi.ts`) — rule 4 forbids shipping on an `analytic.ts` row alone. The barrier elision in particular needs `target/release/rts.exe test` because it touches the GC contract, and `crates/rts-cranelift/README.md` is modified in the same diff, which is the RULE 0 obligation being met.

The rest of this document assumes the numbers in the brief are the **`97f66385`** numbers and that four rows (`prop write own`, `alloc add prop after`, `array for-of 16`, `regex exec+group`) plus startup will have moved before any new work starts.

---

## 1. How this was ranked

Score is (nanoseconds removed across the table) × (confidence) ÷ (effort), with agreement between independent areas treated as the strongest confidence signal available. Two structural facts dominate the result:

- **The largest number in the table is not any row — it is a tax under thirty of them.** `call method` 29.35, `prop proto method call` 27.79, `call closure var read` 26.00 cluster within 3 ns of each other against bun's 0.38–0.66. Every `coll` row (43–67), every array method row, every string method row and every native row sits on top of that ~28 ns. No investigator decomposed it correctly, and one tried and got a wrong answer (see §7.2).
- **A repeated defect class beat every individual candidate.** Five areas independently found the same bug shape: a hot `Context::well_known(name)` whose name is not in `CACHED_KEYS`, so `context.rs:388` never writes the memo (`if let Some(at) = held`) and the name is re‑allocated and re‑hashed **on every call, forever**. Five areas finding one defect from five directions is the single strongest signal in the dataset.

`bench/isolated/` (untracked, 7 binaries, ~1.5 s release build, no workspace) is the established harness for the "before the engine is touched" requirement, and `docs/codegen/README.md` rule 1 already makes it mandatory. Every experiment below is expressed in one of three forms, in order of preference:

- **(I)** a new `bench/isolated/src/bin/*.rs` — plain Rust, models a shape, no engine, ~1.5 s
- **(C)** a `crates/rts-core/examples/*.rs` — real runtime objects, `cargo run --release -p rts-core --example X`, builds rts‑core only, not the CLI/node/UI
- **(S)** structural — `rts ir`, `RTS_CACHE_CENSUS`, `RTS_GC_DEBUG`, or a counter in a debug build (counts are profile‑independent)

---

## 2. SYSTEMIC — one fix, many rows

### S1. The ~28 ns dynamic‑call tax: decompose it, then land the one piece two areas agree on ★ rank 1

**Agreement:** entry‑tax (`call-name-as-a-store`, cause confirmed) + calls (`fold-set-call-name-into-the-call`, not refuted) + calls (`direct-call-for-a-proven-callee`, refuted but its corrected cause arrives at the same place). Three candidates, two areas, one conclusion.

**Confirmed cause.** `crates/rts-codegen/src/emit/call.rs:437-448` emits a whole `RuntimeOp::SetCallName` crossing before every call whose callee has a spelling (`callee_spelling`, `call.rs:226-243`, returns `Some` for every `Ident` and every `Member`), and `emit/expr.rs:588-591` exempts only `Thrown`/`TakeThrown` from `check_for_throw`, so it carries a throw check it can never need. The entry's whole body is one field write — `crates/rts-core/src/entry/functions.rs:553-559`, one `with_current`, one `literals.get`, one store — and the only reader is the failure path at `functions.rs:493` / `:519-526`, used solely to word a `TypeError`. Verified in IR by two agents independently: `block40: Call FuncId(10) __rts_set_call_name` + `WordLoad`/`Compare`/`Branch`, then `block44: Call FuncId(11) __rts_call_counted`.

**What to build.** Widen `call_counted` (already 7 params, `functions.rs:409`) and `call_with_args` with the literal index as an operand, delete `RuntimeOp::SetCallName`, `CoreEntry::SetCallName` and `pending_call_name`. Resolve the literal to a value only on the failure path.

**Do NOT build the variant entry‑tax proposed** (a `WordStore` to a per‑activation address beside `thrown_address`). `thrown_address` works because `THROWN` is its own thread‑local (`entry/current.rs:138-143`); `pending_call_name` is a field of `Context`, and `Context` lives inline in `static CONTEXTS: RefCell<Vec<Context>>` (`current.rs:184`), which `with_context` pushes for `node:vm`/`repl`/`evaluate`. A `Vec` push reallocs, so a cached per‑activation address is a **use‑after‑free write from compiled code**. It also excludes every generator and async body, because `function.rs:757` only asks for the flag address `if !body_suspends(body)`.

**Rows.** `call method` 29.35, `prop proto method call` 27.79, `call closure var read` 26.00 (~20–23% each), plus every built‑in method row: `coll Map.get` 52.10, `Map.set` 66.82, `Map.has` 50.86, `Set.has` 43.49, `array push+pop` 148.78, all the string methods, `regex test` 117.28. **Not** `call free function` 3.21 or `call arrow` 3.11 — those are inlined and contain no call (§7.1). **Not** `call closure make+call` (0.35%) or `call varargs 3` (2.3%) — drop them from any claim.

**Size.** ~5.9 ns per named call, ESTIMATE, derived by two independent routes from the table: `prop typeof alone` 13.10 over a 1.27 floor with two verified crossings, and `prop typeof` 22.79 − `prop typeof alone` 13.10 = 9.69 for `StringConst`+check *and* `StrictEquals`+check. Both land the crossing+check at 4–6 ns.

**Cost.** `call_counted` goes to 8 parameters; Windows fastcall gives four integer argument registers (`cranelift-codegen 0.131 src/isa/x64/abi.rs:1034-1037`), so the 8th crosses through memory — ~0.3–0.6 ns, well inside the win. `CoreEntry` numbering is dense and asserted (`table.rs:838-845`, `ALL.len() == CORE_ENTRY_COUNT`); `table.rs:32-37` forbids renumbering on removal, so the removal needs the same treatment the table already prescribes.

**THE EXPERIMENT (do this first, and it also answers §S1a):**

- **(I)** `bench/isolated/src/bin/call_crossing.rs`. Three shapes, harness‑calibrated, first row = the engine today: (a) one `#[inline(never)] extern "C"` call taking 7 `u64` and doing a field write, followed by a load/compare/branch on a thread‑local flag, then a second 7‑arg call — the shape today; (b) one 8‑arg call and one flag check — the shape proposed; (c) one 8‑arg call, no flag check. The (a)−(b) delta is the win's isolated bound. Quote `functions.rs:409` and `emit/call.rs:437` in the module doc, per this tree's rule for isolated experiments.
- **(S)** `rts ir` on a loop of `o.m(1)` and count `__rts_set_call_name` against `__rts_call_counted` in the callee legend — they are 1:1, which is the claim that the cost is per *call* and not per *site*.

### S1a. Decompose the rest of the ~28 ns — an investigation, not a change ★ rank 2

This is the highest‑value unresolved question in the dataset and it costs one experiment. `call method` 29.35 is 28 ns over the floor for a call whose callee body is `return x + this.v`. The named components: `SetCallName` + check (~6 ns, S1), six `with_current` round trips in `called`/`invoke` (`functions.rs:442, 458, 470, 483, 540` + `set_call_name`) at the tree's own measured ~1 ns each, `resolve`'s `Aside::copied`, **three `Vec` push/pop pairs that can realloc**, the transmuted indirect call (`functions.rs:538`), and the callee's own prologue.

That sums to roughly 10–12 ns. **Sixteen nanoseconds are unattributed**, and every method row in the table pays them.

The per‑borrow constant is settled and should not be re‑litigated: `docs/engine/new-engine-speed.md:544` puts `type_of` at 2.50 ns from Rust including the crossing; commit `487004d6` measured one `with_current` removal at ~1 ns; `crates/rts-core/src/entry/throw.rs:81-101` is a two‑binary A/B putting one round trip at 0.6–1.3 ns. The 5 ns/round‑trip figure two candidates derived from `class_support.rs:187-189` is **wrong** — that 16 ns covers `to_primitive` + `is_symbol` + `as_number`, three function bodies, not three borrows.

**THE EXPERIMENT:**
- **(C)** extend `crates/rts-core/examples/entry_cost.rs` (which already installs a context and loops an entry point with a checksum): loop `entry::call(callee, undefined, …)` on a native and on a compiled‑shaped callable, 2×10⁷ iterations. That prices `called`+`invoke` with no crossing and no compiled code in the picture. The difference between it and the 29.35 row is the crossing plus the callee.
- **(I)** `bench/isolated/src/bin/vec_markers.rs` — three `Vec` push/pop pairs per iteration against none, to price the bookkeeping `functions.rs:458-473` does.

Deliverable is a `docs/codegen/*.md` with the decomposition, per rule 7 (one question, one document).

### S2. The `CACHED_KEYS` audit — five areas found the same defect ★ rank 3

**Agreement: five areas.** calls (`"constructor"`, `"name"` — `functions.rs:132,154`), codequality (`"value"`, `"done"` — `generator/mod.rs:396-404`, per `.next()`), natives (`"detached"` — `buffers/mod.rs:250`, per buffer), strings (`"@@split"`, `"@@replace"`, `"@@match"` — `symbol.rs:302-305`, which additionally `format!`s the name), props (`"@@hasInstance"` — `functions.rs:999`).

**Confirmed cause.** `crates/rts-core/src/entry/mod.rs:213-221` lists exactly six names. `Context::well_known` (`context.rs:366-392`) writes the memo only at `context.rs:388`, `if let Some(at) = held` — so a name **not on the list is never memoised**. Every call pays `Str::from_str` (a `Vec` malloc, `text/mod.rs:162-165`) plus `Interner::intern`, which is `units_hash` (a `DefaultHasher` write per code unit, `text/intern.rs:83-94`) plus a `HashMap<u64, Vec<Key>>` probe plus a `HashMap<Key, Str>` probe plus `same_units` — **three SipHashes on a hit**.

**Rows.** `flow generator next` 805.09 (two names per `.next()`), `call closure make+call` 1672.46 (two per closure), `binary alloc Uint8Array 64` 944.93 and `binary TextEncoder 16` 1250.23 (`"detached"` per buffer), `string split 16` 4799.01 and `regex replace` 1503.12 (`format!` + intern per call), `prop instanceof` 240.80, and — through `generator::result`, which `list_iterator.rs:51-72`, `array_proto/cursor.rs:117-120` and `collections/cursor.rs:167-176` all share — `array for-of 16`, `array map 16`, `array filter 16` and the Map/Set iteration paths.

**This is not a one‑line change and must not be filed as one.** Three obligations:

1. `context.rs:375-379` documents the linear scan as a deliberate choice over a `HashMap` ("which would hash the name to avoid hashing the name"). Lengthening the list lengthens every **miss** — and `regex/mod.rs:445` calls `well_known(name)` with the *user's* capture‑group name, a guaranteed miss per named group per match. The scan's cost curve has to be measured, not assumed.
2. The `@@` names must not be `format!`ed at the call site. `symbol.rs:83` already has the right shape — `const HAS_INSTANCE: &str = concat!(prefix!(), "hasInstance")` — and `method_of` should take the prefixed const.
3. `crates/rts-core/src/entry/mod.rs` is 1091 lines against rule 6's 500‑line ceiling. New code does not get appended to it (CLAUDE.md).

**THE EXPERIMENT:**
- **(I)** `bench/isolated/src/bin/well_known.rs`: (a) a linear scan of 6 short `&str` + an array index — the hit today; (b) the same scan of 12; (c) `Str::from_str` + a `DefaultHasher` over 11 code units + two `HashMap` probes — the miss today. Answers both halves at once: what a name is worth moving onto the list, and what each extra name costs every miss.
- **(S)** a `#[cfg(debug_assertions)]` `AtomicU64` in `well_known` counting misses per name, printed at exit. Run each bench body under `cargo run -q -p rts-host --example run_fixture`. **Counts are profile‑independent**, so this needs no release build, and it turns "five areas think this is hot" into a frequency table.

### S3. The ~68 ns fixed tax under every `String.prototype` method ★ rank 4

**Agreement:** strings, two candidates converging (`text-cell-method-cache` refuted‑but‑real; `intern-value-cell-cost`'s corrected cause (b)).

**The number is a subtraction inside the table and it is stark.** `string charCodeAt` 97.78 against `call method` 29.35, for a native body that is one `Str::unit_at` (`string/mod.rs:278-282` → `text/mod.rs:237`, constant time, allocates nothing). **68 ns before any work happens.** It is why `toUpperCase 16` (202.29), `slice 16` (206.59) and `indexOf 256` (207.45) sit within 5 ns of each other despite bodies that differ by orders of magnitude.

**The leading hypothesis, and it is well‑founded.** A text cell has no shape and no recorded prototype — `objects.rs:651-653` *substitutes* `String.prototype` when `inherited_from` reaches one — so `cache_resolve_indirect` refuses the site at `cache.rs:577-579` ("the receiver has no recorded link") and writes only the `REFUSED` marker into word 3 (`cache.rs:554-558`). The machine compares word 0 against the header (`lower/body.rs:839-841`), so the guard never recognises anything and **the site calls the resolver on every execution, forever**, then falls to `__rts_get_property` (three `with_current` at `objects.rs:129,143,153`) and a two‑link chain walk into a 49‑property prototype. `s.length` escapes only because it was special‑cased into the cache at `cache.rs:143-158`, and `entry/mod.rs:202-204` records what the road it left cost: **"99 ns against 4.8 for an ordinary property."**

**THE EXPERIMENT — the best one in this dataset, and it needs no engine change at all:**

- **(S)** Time `s16.toUpperCase()` against `w.toUpperCase()` where `const w = new String("abcdefghijklmnop")`. A wrapper is an ordinary cell **with** a shape and a recorded prototype (`primitive_proto.rs`), so its site *can* arm while the primitive's cannot; the native body is byte‑identical because both go through `coerce_receiver`/`receiver` (`string/mod.rs:240-242`). **The difference between those two rows is the whole of this candidate, measured by the instrument of record.** Add both to `bench/analytic.ts` and run once.
- **(S)** `RTS_CHAIN_DEBUG=1 ./target/release/rts.exe run f.ts` on the primitive loop prints the refusal by name.

**Only then decide the fix**, because the obvious one is narrower than it looks: arming the indirect cache for text receivers helps only the **14** `String.prototype` properties in inline slots 0–13 (`cache_resolve_indirect` has no overflow path — `cache.rs:633-640` refuses `slot >= holder_width`, and `Object.getOwnPropertyNames(String.prototype)` returns 49 names in install order, so `split` at index 21 is unreachable). It also requires refusing `setPrototypeOf` on a text cell first, because every text cell shares one header word and `chain.rs:108-122` currently lets `Reflect.setPrototypeOf("abc", …)` succeed — verified: `"abc".foo` answers 42 after it. And the closest in‑tree analogue (commit `78e62f54`, arming text `length`) measured **12.8 ns**, not 68.

### S4. Array construction costs ~215 ns and eight rows pay it ★ rank 5

**Agreement:** calls (`rest-arguments…` corrected cause), arrays (`set-length-fast-path` corrected cause), objects (`stop-zeroing…` corrected cause). Three areas, arriving from rest parameters, from `push`/`pop`, and from GC.

**The decomposition that made this visible.** `alloc array literal 4` = 231.34; `array index read` = 16.63 accounts for the read; so ~215 ns is `array::built_in` (`array.rs:113-168`). `call varargs 3` = 253.36 = `call method` 29.35 + ~215 + <10 rest‑specific — which is why the calls verdict concluded rest parameters are not the story, array construction is.

**What is in the 215 ns**, all read from source, none of it measured apart:
- the elements `Vec` malloc (`array.rs:50` `to_vec`) and the `Slab::insert` (`array.rs:115`)
- `alloc_or_die` (`array.rs:166`) — and on the free‑list path `Region::alloc` writes **fifteen zero words** per cell (`region/mod.rs:452-454`)
- `set_length` (`array.rs:683-707`): `refuses_key_write` **three times** (`objects.rs:462`, `:486`, `array.rs:700`), each an `integrity_at` + an `attributes_at` linear find; `reconcile_length` (`objects.rs:1137-1163`) re‑deriving the length key and re‑reaching the elements to compare a length the caller just read; a shape `slot_of`; and a `set_attributes` whose first record mallocs a `Vec` (`integrity.rs:198-202`)
- **the sweep**: `collect_cycle::release` (`collect_cycle.rs:139-235`) runs **22** `Aside::remove` calls (counted, not estimated) plus `weak::clear_freed`, `finalize::queue_freed` and `region.free` **per freed cell**. `RTS_GC_DEBUG=1` over 200 000 allocations reports 3–6 cycles at `freed ~63 670` each — so in steady state there is one `release` per free‑list `alloc`, and it is several times the work of the allocation.

**Rows.** `alloc array literal 4` 231.34, `call varargs 3` 253.36, `coll Object.keys 4` 308.09, `binary subarray 64` 294.13, `array map 16` / `filter 16` (a fresh array per call), `string split 16` 4799.01 (eight pieces), `regex exec+group` 2268.25 (`array_new` + the match object), `json stringify small` 5014.88 (`own_keys` builds one per object).

**THE EXPERIMENT — decompose before choosing a sub‑item:**
- **(C)** `crates/rts-core/examples/array_build.rs`: one `Context`, four timed loops of 10⁶ — (i) `array::built_in(context, vec![0;4])`, (ii) the same with `set_length` elided, (iii) `collect_cycle::release` on a dead array cell, (iv) `region.alloc(STRIDE, ty)` alone. Four numbers decide which of allocation / `set_length` / sweep is the target. Nothing here needs TypeScript or a workspace build.
- **(I)** `bench/isolated/src/bin/region_reuse.rs` (a sibling of the existing `region_start.rs`): the 15‑store zeroing loop against `fill(0)` against nothing, on the free‑list path.

**Two things not to do inside it.** Do not bound `trace::edges_of` by the shape: `trace.rs:141-158` walks `width−1` because the last slot holds the overflow block's **address**, and `shape_of` is unreliable as a "has a shape" predicate because `context.rs:108-111` grows `shape_of_type` with `resize(…, shape)`, filling intervening indices with the shape being recorded. Both make the collector follow a non‑value. Do not move `length`'s non‑enumerability out of `set_length`: it is the single funnel that records it (`array.rs:688-691`, `object_global/arrays.rs:174-176`, `array_proto/mod.rs:584-590` all say so), and bypassing it makes `Object.keys(arr)` and `for‑in` wrong.

### S5. Declare which entry points cannot raise ★ rank 6 — **LANDED 2026-08-22**

**Done, and the ranking above was derived from the wrong number.** This was rank
6 because the site-level prize is 0.1–0.6 ns per removed check. Nobody had
counted what the check costs the **compiler**, and that is where it is large.

Measured over `bench/analytic.ts` by `rts ir`, per file, before and after:

| | before | after | |
|---|---:|---:|---|
| basic blocks | 6 164 | 5 456 | −11.5% |
| throw checks | 1 423 | 1 069 | **−24.9%** |
| IR lines | 26 839 | 23 641 | −11.9% |

Eight operations removed a quarter of every throw check in the file. The reason
that matters more than the nanoseconds: `RTS_TIMING=1` on the same file puts
135.8 ms in `machine-compile` against 19.7 ms in this layer's lowering, so **87%
of compiling a program is the code generator**, and what it is handed is blocks.

The list is `rts-codegen/src/runtime/raising.rs` — a new module rather than an
append, because `runtime/mod.rs` is 1 407 lines and already past the 1 000-line
ceiling. It holds two lists kept deliberately apart: `CANNOT_RAISE` (eight
operations, each naming the `rts-core` body it was read against) and
`IS_THE_CHECK` (`Thrown`/`TakeThrown`, exempt from *asking*, not from raising).

Beyond the three the plan named, five more were verified closed by reading:
`RunningFunction`, `SetCallName`, `MarkDerived`, `MarkClassConstructor`,
`ElementsBase`. `KeyNumber` was examined and **rejected** — it calls
`to_primitive`, which runs a user `toString`.

Pinned three ways: unit tests in `raising.rs`, two assertions in
`rts-host/src/entries.rs` (each exempt operation still resolves and still agrees
about its shape; the two lists cannot merge), and
`rts-host/tests/remainder.rs::the_unboxed_remainder_carries_no_thrown_value_check`,
which reads the callee number out of the IR legend and asserts no `WordLoad`
follows that call. **What none of them can check** is whether the runtime body on
the other end still refrains from throwing — that is a human reading eight
bodies, which is why the list is short.

The original entry follows, unchanged.


**Agreement:** codequality (`nonraising-entry-points`, not refuted) + strings (`string-literal-without-a-call`, whose corrected cause independently observes that `StringConst` carries a throw check it can never need).

**Cause.** `emit/expr.rs:586-591` exempts exactly `RuntimeOp::Thrown | TakeThrown` and emits `WordLoad` + `Compare` + `Branch` + two basic blocks after everything else. The tree contradicts itself about this in three places already — `runtime/mod.rs:170-174` ("because it cannot run user code and therefore cannot throw — none of the `__rts_take_thrown` check that follows every generic operator"), `emit/expr.rs:1437-1439`, and `crates/rts-host/tests/remainder.rs:200` — all asserting an exemption the code does not implement. Under CLAUDE.md's "never leave a rule the code contradicts", that is a defect independent of any nanosecond.

**Exempt set**, each confirmed against its rts‑core definition: `NumberRemainder` (`operators.rs:174-191`, "takes and answers unboxed doubles, touches no context, and cannot throw"), `MathRandom` (`math.rs:461` → a thread‑local xorshift), `StringConst` (`text.rs:184-190`, a `literals.get`). Audit for more; do **not** exempt the generic relational entries — they route through `operators.rs:203-215` → `primitive::operands`, which can run a user `valueOf`.

**Where the fact lives.** Not in `#[rtse::entry]`: `crates/rts-codegen/Cargo.toml:11-23` depends on rts‑cranelift and swc only, and rule 1 of `crates/rts-codegen/README.md:36-40` forbids adding rts‑core. The only crate that may name `RuntimeOp` and `rts_core::entry` together is `crates/rts-host/src/entries.rs:83-84`, which already holds the mapping — the assertion belongs there, with `can_raise()` beside `signature()` in `runtime/mod.rs:1069`.

**Rows.** `arith int mod` 5.23, `arith Math.random` 5.01, and every row containing a string literal in expression position — `string equals` 10.66, `prop typeof` 22.79, `prop in operator` 85.53, `string concat 2` 141.88, and the regex/JSON rows. 0.1–0.6 ns per removed check (the tree's own bound, `expr.rs:568-578`), small per site and very wide.

**THE EXPERIMENT:**
- **(S)** `rts ir` on `a += i % 7` and count blocks in the loop body: today `Call`, `WordLoad`, `Const`, `Compare`, `Branch`, plus a re‑raise block and a continue block; after, `Call` and nothing else. Pin as a codegen test. This is a structural assertion and needs no timing.
- **(I)** `bench/isolated/src/bin/throw_check.rs`: a call plus a thread‑local load/compare/never‑taken‑branch against a bare call, on and off a dependency chain.

**Cost, stated plainly:** a false `can_raise() == false` is a **swallowed throw** — a caught exception silently becoming a wrong answer. The two facts must be asserted against each other in rts‑host, never hand‑written twice.

### S5d. The throw check's zero, and S6 — **LANDED 2026-08-23**

Two changes that look alike and are not: one is IR size, the other is the fast
path. Landed together and measured apart, because a single number covering both
would have hidden that each moved a different ruler.

**(a) The zero every throw check compares against was declared per site.**
`bench/analytic.ts` emitted **1 066 `Inst::Const` of the integer zero**, a third
of every constant in the file. `Function::push_const` already collapses the pool
to one row, but each site still materialized its own — a value must dominate its
uses, so a pool row cannot be shared. The entry block dominates every block, so
one value there can be. It sits in `BodyState::zero`, under the same condition as
`flag`: **absent for a body that PARKS**, because `frame::resumable_form` rewrites
a suspending function around every suspension and a constant is as much an SSA
value of the pre-rewrite function as an address is.

| `bench/analytic.ts` | before | after | |
|---|---:|---:|---|
| `Inst::Const` | 3 074 | 2 100 | −31.7% |
| IR lines | 17 407 | 16 435 | −5.6% |

**What it bought in wall clock: at most ~2%, and probably nothing.** Twelve
interleaved pairs, `RTS_CRANELIFT_JOBS=1`, 101 bodies: `place` medians 37.935 →
37.256 ms, **9 of 12 pairs** in that direction. The spread inside each binary is
larger than the difference; the pair count is the only thing that makes it
reportable at all. Consistent with what S5b already established — removing cheap
`iconst`s does not move Cranelift much.

**(b) S6, and it moves a different ruler.** `emit/expr.rs` computed
`tagged(a)`/`tagged(b)` **before** the guards, in the block the FAST path runs
through, for a value whose only consumer is the runtime call on the slow path.
Widening an `F64` is a bitcast, an `iconst(CANONICAL_NAN)`, an `fcmp` and a
`select` — a GPR/XMM domain crossing and a cmov, per iteration. Nothing removes
it: `opt_level = none` gates out the whole egraph mid-end, so there is no GVN, no
LICM and no sinking.

The plan's structural experiment is what it promised: `rts ir` on
`function f(n){let a=0;for(let i=0;i<n;i++)a+=1;return a}` — **one `Widen` in the
loop header before, zero after**, with the widening now in the slow block.

**It changes no IR line count** (403 → 403 — moved, not removed), and the plan's
own prediction that it was "high confidence free, low confidence it moves a row"
is half right. Measured per program, interleaved:

| | HEAD | after | |
|---|---:|---:|---|
| `bench/monte_carlo_pi.ts` | 815 ms | 796 ms | **−2.3%, 5 of 5 pairs, ranges disjoint** |
| `bench/objbench.ts` | 314 ms | 297 ms | 2 of 3, noisy |
| `bench/pi_machin.ts` | 54 ms | 55 ms | flat — see below |

`monte_carlo_pi.ts` is the right ruler and not a lucky one: this function's own
comment already names it as the file whose operands arrive unproven, because
`rngState` is module-scoped. Every `novo` run beat every `head` run, which is a
stronger claim than the medians.

`pi_machin.ts` measures nothing here — 54 ms is mostly process start, so a loop
effect cannot show through it. Stated rather than counted as a null result.

**And passing the raw operand to `guard` is not a second change.**
`FuncBuilder::guard` calls `fold::guard_answer` before widening and
`widen_if_needed` after, so the widening still happens where it is needed. What
changes is that the fold now sees the operand instead of a `Widen` of it — the
same "indirection that hid a constant from a layer built to look at constants"
this function's own comment names further up.

### S5c. One re-raise block per region, and the emitter bug it uncovered — **LANDED 2026-08-22**

Two changes and one bug, and the bug is the part worth reading.

**(a) The re-raise block was built per SITE.** Every check that follows a call
that can raise branches to a block holding three lines — a header, a call to
`__rts_take_thrown`, and a `Throw`. `bench/analytic.ts` held **1 069 identical
copies**, 20% of every basic block in the file.

Sound to share because the block reads NOTHING from the site: no parameters, and
its only instruction is a call with no arguments. The key is the **region**,
because where a `Throw` lands is decided by the region its block is in — the
reason `raise_if_thrown` always created the block with the region still open.
`emit/body_state.rs` holds that argument. 1 069 → **96**, one per function.

**(b) Three `hit` blocks in `property.rs` were forwarding a value to
themselves.** `cached_get` prepends the value it found to whatever the hit
`BlockCall` carries — which is why the hit target must have `Repr::Tagged` first.
The emitter still built a block whose only parameter was that value and whose
only instruction was a jump handing it to `join`. Pointing the terminator at
`join` removes 403 of the 1 072 blocks that held nothing but a `Jump`.

| `bench/analytic.ts` | before | after | |
|---|---:|---:|---|
| IR lines | 21 434 | 17 407 | −18.8% |
| basic blocks | 5 456 | 4 010 | −26.5% |
| re-raise blocks | 1 069 | 96 | **−91%** |

Against the figures before S5 and S5b: **26 839 → 17 407 lines and 6 164 → 4 010
blocks, −35% each.**

#### The bug, which is worth more than the block count

Sharing the re-raise block shifted block numbering, and **a program HEAD compiles
stopped compiling**: `['a','b'].forEach(x => { s = s + x; if (x === 'b') throw … })`
inside a `try`. Two programs that HEAD *cannot* compile started working. None of
those three was evidence about the change. All three were one pre-existing
defect.

`Ctx::last_captured_write` memoises a captured binding's last written value so
that `s = s + x` followed by a read of `s` does not go back to the heap — worth
69.6 ns an iteration on `bench/monte_carlo_pi.ts`, and its window is deliberately
one block wide. It holds a `ValueId` **and** a `BlockId`, both handles into one
`FuncBuilder`, and **it was never saved or restored around a nested function.**

So a write inside an arrow left a memo naming the arrow's block and the arrow's
value, and emission carried on in the enclosing body still holding it. The guard
is "same block, nothing emitted here", and `BlockId`s are per function — a
collision is not exotic, it is one number matching another. What comes out is a
read answering a value from a function it is not in, and the symptom is
`Place(Lower(CannotWiden { from: I64 }))`, which `emit/binding.rs` already
records as this field's failure mode for a *different* mistake.

It is the **third** instance of one defect class in this file's history: the
`thrown_flag` that made every generator load a callee's address, the
`finally_jumps` that made the builder panic with "block belongs to this
function", and this. The fix is not a fourth save-and-restore site to keep in
step by hand — it is `emit/body_state.rs`, one field holding every fact that
belongs to one `FuncBuilder`, taken and restored as a unit.

**It fixes a test**: `running.rs::a_throw_from_a_callback_stops_the_native_that_called_it`
went from FAILED to ok, and it is a fix rather than a shift — the cause was
found, and the program compiles with the memo enabled.

**Still open, and this is the honest edge**: the memo's window is per block and
nothing says how many blocks may collide *within* one function. Nothing found
such a case, and nothing looked for one.

#### What the IR still holds that nobody has removed

**744 blocks still contain nothing but a `Jump`**, 572 of them reached from a
`Branch`. They are the `carrying_on` of a throw check that turned out to have
nothing after it — which is not knowable when the block is created, so it cannot
be fixed the way (b) was. Removing them is a CFG pass that threads edges past
empty forwarding blocks and then compacts the block list, and compacting means
renumbering `BlockId`s — which `RegionTree` also stores, for handlers and
cleanups. A mistake there is a silently uncaught throw. It is a separate change
with its own gate.

**334 `Const` instructions are duplicates within one block.** Small. Sharing more
than that needs dominance, and the entry block dominates everything — but a
constant materialized once in the entry block is live across the whole function,
which trades IR size for register pressure, and `opt_level = none` means
Cranelift will not undo it. Not attempted; not measured either way.

### S5b. The constant pool was 94% duplicates — **LANDED 2026-08-22**

**Not on any list before this, and found by counting rather than by reasoning.**
`bench/analytic.ts` declared **3 417 constant-pool rows holding 202 distinct
values**. A single trivial body carried eleven separate declarations of the
integer `0`. `Function::push_const` appended unconditionally, so every call site
that wanted a zero minted its own row.

Nothing downstream merged them either, and `target/mod.rs:1065-1067` says why in
its own words: Cranelift's default `opt_level` is `none`, which gates out the
whole egraph mid-end — **there is no GVN**. A duplicate declared here stays a
duplicate all the way to the register allocator.

The fix is a `HashMap<ConstDecl, ConstId>` beside the pool in
`rts-cranelift/src/ir/func.rs`, under machine-layer rule 8 — derive what a client
would otherwise have to remember. The alternative was a memo per emitter, and
`emit/` has nineteen `declare_const` call sites that would each have kept a
different one. The pool stays a `Vec` in declaration order because rule 13 says
what a person diffs is ordered deterministically; the map is the lookup, not a
second source of the content.

| `bench/analytic.ts` | before | after | |
|---|---:|---:|---|
| pool rows | 3 417 | 856 | −75% |
| `Inst::Const` | 3 428 | 3 074 | −10.3% |
| IR lines, with S5 | 26 839 | 21 434 | **−20.1%** |

**856 and not 202, and the difference is not a shortfall.** `ConstId` indexes a
*function's* pool, so dedup is necessarily per function and the file has 94 of
them. 202 was the count of distinct values across the whole compilation, which no
per-function handle could ever reach.

**What it does NOT do**, stated because the row above invites the wrong reading:
it does not collapse the instructions. Each `use_const` still emits its own
`Inst::Const`, and it must — a value has to dominate its uses, so one
materialization cannot be shared across blocks that do not dominate each other.
The `Inst::Const` count fell 10.3% only because S5 deleted the blocks those
constants lived in. Sharing materializations is a dominance analysis and a
separate change.

### What S5 and S5b together are worth in wall clock, and it is not 20%

That paragraph said this was unmeasured. It is measured now, and the answer is
**much smaller than the IR reduction**, which is the number worth carrying
because it is the one that would have been guessed wrong.

The instrument had to be built first: `bench/analytic.ts` runs for 27 seconds, so
its compile time is invisible beside it, and `rts ir` never machine-compiles at
all. What was measured instead is 101 generated bodies (`a * j + b) % 7` in a
short loop, so the program compiles a lot and runs in nothing), with
**`RTS_CRANELIFT_JOBS=1`** — the parallel pool's scheduling noise is larger than
the whole effect, and with it on, five pairs disagreed about the sign.

Serial, five interleaved pairs, medians, same machine, 2026-08-22:

| | baseline | after | |
|---|---:|---:|---|
| `cpu lowering` (this layer) | 6.031 ms | 5.688 ms | −5.7% |
| `machine-compile` (Cranelift) | 44.562 ms | 42.789 ms | −4.0% |
| **`place` (the phase)** | **51.841 ms** | **50.081 ms** | **−3.4%** |

**All five pairs moved the same way on `place`**, which is what makes a 3.4%
claim reportable at all — the medians alone would not be, at this spread.

**So the hypothesis that motivated the work was wrong in its size.** The
reasoning was: 87% of placement is the code generator, the code generator is
handed blocks, therefore removing 11.5% of the blocks and 20% of the IR buys
something near that. It buys 3.4%. The blocks removed are the smallest ones in
the program — two instructions and a terminator — and a deduplicated constant is
an `iconst` the register allocator was already rematerialising. **Cranelift's
cost is not linear in block count**, and this is the measurement that says so.

Which does not make the change wrong; it makes the *ranking argument* wrong. What
S5 actually fixed is a rule the code contradicted in three places, and it did it
without costing a nanosecond anywhere. The 3.4% is a bonus, and it is stated as
3.4% rather than as "20% less IR" precisely because the second sentence is true
and would be read as the first.

On a single small body — `hello.ts`, one function, no pool — the same pair
measures `machine-compile` 0.764 → 0.689 ms and `place` 1.214 → 1.113 ms over
nine interleaved pairs. Same direction, same order of magnitude.

### S6. Sink the dead `Widen` off the fast path of every guarded operator ★ rank 7

**Agreement:** codequality, three candidates converging — `sink-widen-to-slow` (not refuted), `entry-guard-parameters` (refuted; its corrected cause names this as "the half that is on the chain"), `cranelift-opt-level` (refuted; its corrected cause names it as the real defect).

**Cause.** `emit/expr.rs:1594-1595` computes `widened_a`/`widened_b` **before** the guards, in the block the fast path runs through; the only consumer is the runtime call on the slow path at `expr.rs:1685`. Widening an F64 lowers to bitcast + `iconst(CANONICAL_NAN)` + `fcmp(x,x)` + `select` (`lower/value.rs:59-69`) — a GPR/XMM domain crossing and a cmov, per iteration. Nothing removes it: `target/mod.rs:1065-1067` records that Cranelift's default `opt_level` is `none`, so there is no GVN and no LICM. Confirmed in IR: `block1(v9: F64, v10: F64): v13: Tagged = Widen(v10)` read only in the guard‑failure block — and on `arith compare int` the **same** widening of the **same** SSA value is emitted twice, because there is no GVN to share it.

**Corrections to carry:** it is at most **one** operand, never both (`expr.rs:1590` returns early when both are proven; `builder.rs:1115-1121` returns the value unchanged when it is already Tagged). And the proposed "keep a widening at the guard sites" is unnecessary — `FuncBuilder::guard` calls `fold::guard_answer` before widening (`builder.rs:778`) and `widen_if_needed` after (`builder.rs:795`), so passing the raw operand emits byte‑identical IR.

**THE EXPERIMENT:** **(S)** pure IR, no timing. `rts ir` on `function f(n){let a=0;for(let i=0;i<n;i++)a+=1;return a}`, count `Inst::Widen` in the loop header: one before, zero after. Pin it as a rts‑codegen test so it cannot come back. **The size must then be measured per row** — the tree's own record is that a same‑class hoist (a loop‑invariant `CachedGet`) measured as noise (`docs/engine/new-engine-speed.md:577-590`), so this is high confidence that it is free and low confidence that it moves a row. Effort **S**, one file — worth doing on those terms and not on a ns claim.

### S7. The inline‑cache fast path: one guard per literal, and a non‑growth store ★ rank 8

Two independent halves, both surfaced as *corrections* to refuted candidates, both agreed by two areas.

**(a) The third word and its branch on every cached read.** `lower_cached_get` (`lower/body.rs:645-748`) is not one compare and one load: `address_of`, then **four dependent loads** before the value — header at `:647`, `cell[0]` at `:655`, `cell[8]` at `:704`, `cell[16]` at `:710`, then the value at `:743` — plus two branches (`:665`, `:722`). Word two and its branch exist only for objects that have **overflowed** (`body.rs:687-702`) and are paid by every read. `prop read own` is 4.97 against bun 0.54. (props, `cache-cell-half-line` corrected cause.)

**(b) The non‑growth cached store.** `lower_cached_set` unconditionally re‑stores the header through a `select` (`body.rs:1050-1071`) on every hit, including the overwhelming majority that change nothing; and an escaping object literal re‑emits `Guard { expect: Ref(Opaque) }` **per property** on the same receiver (visible in IR: the same receiver guarded again in block2 and block6). Reusing one proven‑reference guard across a whole literal, and a terminator that omits the word‑2 load/select/header store when the site provably never grows, are the two changes named. (objects `hand-object-new-the-shape-not-the-count` corrected cause + props `no-barrier-single-region` second half — two areas.)

**Rows.** (a) every cached read: `prop read own` 4.97, `prop read 4 fields` 4.18, `prop proto method call`, `string length` 5.41. (b) `prop write own` 9.61, `alloc add prop after` 54.12, `alloc class instance` 90.89 (five `CachedSet`s in a five‑field constructor, verified).

**THE EXPERIMENT:** **(I)** `bench/isolated/src/bin/cache_shape.rs` — model the two sequences over a 128‑byte cell array: (i) header load + cell[0] + cell[8] + cell[16] + two branches + value load; (ii) the same without cell[16] and its branch; (iii) the store form with the select and header write against one without. Dependent‑chain and independent variants, since `docs/engine/new-engine-speed.md:555-573` shows this machine hides ~7 independent ops for 0.34 ns.

---

## 3. THE OBJECT MODEL — asked for specifically

**What the ten investigations established, and it reframes the question.**

The engine **does** have hidden classes, and they work. `ShapeTree::transitions` is memoised globally (`shape/tree.rs:136-138`) and each site remembers a (before‑header → offset, after‑header) triple (`cache.rs:243-267`). `RTS_CACHE_CENSUS=1` over 200 000 iterations of both `new Callee()` and `{x:i}; o.y=i` reports **0 misses, 0 sites**. `RTS_CACHE_WHY=1` over a 200 000‑iteration escaping‑literal loop prints **three** lines, one per key — so `ShapeTree::transition` and its linear `repr_of` walk run once per *site* for the life of the program, not per object. **None of the 90.9 ns or the 54.1 ns is cache miss**, and the candidate that assumed it was (`hand-object-new-the-shape-not-the-count`) is refuted on exactly that.

**Object literals are not cheap — they are deleted.** `alloc object literal 2` measures 1.22 and `8` measures 1.24 against a **1.27 ns floor**. `rts ir` shows the loop body reduced to one `FloatArith(Add)` with no `ObjectNew` at all; `RTS_ESCAPE_STATS=1` prints `1 candidate, 1 replaced`. Those two rows say nothing about allocation and must stop being quoted as if they did.

**Where the object model's cost really is**, in order:

1. **`allocate_for_target` resolves the prototype on every `new`** — `functions.rs:889-902`: `well_known("prototype")` (a six‑name scan, `context.rs:380`), `objects::read_property` (`objects.rs:585-618`), then `typed_as` (`context.rs:71-72`), then `alloc_after_collecting`, then `set_prototype`. The file's own comment records `new C()` at **597 ns** before an earlier round of this work (`functions.rs:882-887`). → **L3** below.

   **Corrected 2026-08-22, twice and independently** (`docs/codegen/object-model.md` §5): this line said "a full prototype‑chain property read … ~6 FxHashMap probes". It does **not** walk the chain. `closure_new` eagerly `put`s a `prototype` on every function (`functions.rs:113-115`), so it is an **own** property found on the first link — one `own_property`, **3** probes. Verified from the other side too: `Object.getOwnPropertyNames(Callee)` answers `["prototype","name","length"]`. And `typed_as` is not a "linear find" in any meaningful sense — the prototype is in hand from the line above, so `known.iter().find` scans a list of one, which its own comment at `context.rs:905-909` says. Both corrections **shrink** the prize this item was written to justify, which is why they are recorded rather than quietly dropped.
2. **Allocation and sweep bookkeeping** — §S4. `release` runs 22 `Aside::remove` per freed cell, once per free‑list `alloc` in steady state.
3. **The write barrier** — already fixed in the tree, needs measuring (§0).
4. **The per‑property guard and the header re‑store** — §S7(b).

**What is settled *against*, so nobody re‑opens it:**

- **Move the prototype into the cell.** Refuted (objects). `Context::typed_as` already encodes the link **in the type number** via `proto_types` (`context.rs:62-94`), and `cache.rs:455-464` states it: "the receiver's type is discriminated by its link… so recognising the type proves the link is the cell whose address was remembered" — for zero extra loads, out of a header word that must be loaded anyway. A cell copy adds a load, a compare and a second source of truth. It also costs a property slot on every object, and `region/mod.rs:153-178` **measured** that boundary: 15 → 31 slots made a scene engine's frame 30% *worse* (0.86 → 1.12 ms) on cache density alone.
- **Precompute a class's shape from its declared fields.** Refuted (objects). The field list would come from `emit/types/classes.rs:40`, whose own header (`:18-30`) says the resolution is unsound — `interface Foo` and `class Foo` collide in one `Name` — and closes "Slower, never wrong." Seeding a *shape* from it converts a slower‑never‑wrong guess into instances born with another declaration's properties. Separately, `Repr` is chosen by **observing the value** (`objects.rs:514-518`), and a pre‑seeded key never transitions (`objects.rs:481`), so the invented repr would be permanent. And a seeded object claims every key with value `0.0` until each write lands, because zero decodes as the double 0.0 (`objects.rs:1005-1007`).
- **Scalar‑replace `o.y = v` where `y` is not a literal key.** Refuted (escape), **on semantics, with a demonstration**. `o.y = v` is an ordinary `[[Set]]` that walks the prototype chain: it may call an inherited setter, may be a silent no‑op against an inherited getter‑only property, and for `__proto__` creates no own key at all. Verified against both engines: with `Object.defineProperty(Object.prototype,"y",{set,get})`, `o.y=7; o.y` answers `99` and calls the setter in **rts and node alike**. Flattening answers `7` and calls nothing. `alloc add prop after` = 54.12 is the honest price of a conformant `[[Set]]`.
- **Fewer `with_current` per `new`.** Largely refuted. It is eight crossings, not nine, and only two merge without breaking the `construct`/`construct_inner` split that `construct_with_args` depends on (`functions.rs:636-639`, `:851-852` — a marker pushed on top would hide the vector from the callee it was made for, breaking rest parameters). At ~1 ns each that is 1–4 ns of 90.89.

**Named long‑term, not scheduled.** Array elements live in `arrays: Slab<Vec<u64>>` (`entry/mod.rs:752`) reached through `array_elements: Aside<Slot>` (`:706`) — outside the addressable heap, which is why `a[i]` must be a call. Moving them into the region is the same change the property overflow already took (`objects.rs:1008-1048` records that a `Vec` in a slab made `cache_resolve` return −1 forever, 84 484 013 misses on `analytic.ts`, fixed by `alloc_spanning_or_die` plus the block's address in a reserved slot). It is XL, it needs an `ElementStore` with a **miss edge** rather than a trap (`ir/inst.rs:305-309` — `xs[9]` must answer `undefined`, not `TrapCode::OutOfBounds`), a runtime arrayness discriminator the header word does not carry (`{length:4}` and `[1,2,3,4]` reach the same shape and the same type), and it must handle `Object.freeze` (`computed/access.rs:132-142` records that exact bug), holes, and detachment. Write it down; do not start it before §S4 says where the 215 ns is.

---

## 4. STARTUP — asked for specifically

**First, the arithmetic in the brief is wrong and it changes what is missing.** `probe::Phase` reports on `Drop` (`probe/phase.rs:80-86`), so the printed rows **nest**: `emit` ⊂ `front-end`; `plan`/`lower+compile`/`define` ⊂ `place`; the four installs ⊂ `seed-context` ⊂ `run`. Top‑level is front‑end 0.871 + prepare 0.027 + place 0.599 + run 5.101 = **6.598 ms**, against a 12.8 → 19.9 ms span of 7.1 ms. The "2–3 ms unaccounted" does not exist; the residual is **0.502 ms**, and it holds the first `Region` construction (`run.rs:1097`, outside every phase) plus `canonicalize`/`.env`/`read_to_string`.

### ST1. Instrument the 1.587 ms inside `seed-context` that no row explains ★ startup rank 1

`seed-context` is 4.380 ms; the four timed installs sum to 2.793 ms. The remaining **1.587 ms** covers `Context::over` (`run.rs:326`), the `declare_*` block (`:336-394`), and **two installs with no `Phase` at all** — `crate::stack::install` (`:331`) and `rts_ui::install` (`:416-417`). `rts-host/Cargo.toml:73` sets `default = ["physics","ui"]` and the UI surface is confirmed present in the shipped binary (36 members on `rts:egui`, 20 on `rts:input`, 12 on `rts:gpu`). A further **0.721 ms** sits between `seed-context` and `run` with no row.

**Change:** add `Phase::start` around `install-ui` (`:416-417`) and `declare` (`:331-394`), and one covering the tail (`rts_ui::shutdown` at `:490-491`, `census_report` at `:494`) so the subtraction stops. Do **not** wrap `Context::over` — reading it (`entry/mod.rs:940-1058`) shows ~70 `Vec::new`/`Slab::new`/`None` initialisers, and the one thing that made it expensive is already gone (see ST2). Do **not** add a `Drop`‑reported phase around the `with_context` block: `run.rs:479` calls `std::process::exit(1)` inside it, so an uncaught throw would print nothing.

**Cost, stated:** each nested phase prints an **unbuffered** `eprintln!` (`phase.rs:96-101`) while the parent clock still runs, so children will still not sum to the parent, and part of the 1.587 ms is the instrument itself.

**THE EXPERIMENT:** it *is* the experiment. Verify by `RTS_TIMING=1 rts run empty.ts` and checking the new rows are non‑zero and roughly close the gap.

### ST2. `Context::over` no longer builds a second 8 MiB region — re‑measure, do not re‑do

Already fixed in the working tree: `over` (`entry/mod.rs:940-1060`) now holds the whole field list and `new` (`:1084-1089`) delegates into it. `entry/mod.rs:1068-1083` records the bug in the past tense — "every `Context::over` built a whole second region and dropped it — 8 MiB reserved, zero‑filled, and freed, on every `rts run`". The 4.380 ms of record was measured **with** that bug. **Nobody has measured what removing it bought.** That measurement is the whole of the remaining work here.

### ST3. Three LAZY node modules are forced eager by install itself ★ startup rank 2

`crates/rts-node/src/lib.rs:105/119/126` register `assert`, `dns` and `path` in `LAZY` (`:207-209`, `declare_module_lazy`), and then `:379/:383/:395` call `module_at_name` on all three to read `assert.strict`, `dns.promises` and `path.posix|win32` — and `modules.rs:152-156` **builds** a module when it is named. 87 objects / 149 property definitions built at startup despite being declared deferred.

**Narrow it to that.** Do **not** fold in making `fs`/`inspector`/`util` lazy: `lib.rs:183-184` states that as a decision, `lib.rs:317-323` derives `builtinModules` from the same array (and `crates/rts-host/tests/node_modules.rs:717` asserts `isBuiltin("node:fs")`), and `fs/streams.rs:270-281` + `fs/utf8stream.rs:212-217` both justify an unlinked prototype on the stated fact that install calls `fs::namespace` first. `events` and `stream` must stay eager — `lib.rs:176-183` records a real test failure from prototype‑registration order.

**One hazard the candidate missed:** register both specifiers of a secondary name in **one** `declare_module_lazy` call. `modules.rs:165` uses `std::ptr::fn_addr_eq` to decide identity, and two independent coercions of one `fn` item are not guaranteed equal — two separate registrations would break `require('sys') === require('util')`, the invariant `lib.rs:167-172` protects.

**A free finding beside it — LANDED 2026-08-22, and it was worse than this line said.** `events::namespace` is **not memoised** (it allocates a fresh namespace and installs three natives per call), and there are **three** callers, not two: `lib.rs`, `process/mod.rs` and `readline.rs`. So `node:events` was built three times on every startup.

The prototype half already *was* memoised — `make_prototype` records by name and returns what it recorded — and the prototype is the only thing the two extra callers wanted; both walked `namespace` → `EventEmitter` → `prototype` to reach it. So the fix is not a memo on `namespace` but `events::emitter_prototype`, the memoised half, called directly by the two.

That form is also **more** robust than what it replaced, which is why it was taken. `process/mod.rs` refuses `make_prototype(context, "EventEmitter", &[])` for a stated reason: an empty member list wins the name and registers a prototype with no methods, so `node:events` finds it later, never installs its own, and every emitter in the program silently loses `on`. `emitter_prototype` passes the **real** table — the same one `namespace` passes — so whichever of the two runs first installs the same surface and the other gets it back. The comment's request to be "independent of that order" is now satisfied rather than merely ordered correctly today.

**THE EXPERIMENT:** **(S)** a temporary `eprintln!` in `Context::module_at` (`modules.rs:156`) naming the specifier it builds. `rts run empty.ts` must print `assert`, `dns`, `path` and must **not** print `os`, `crypto`, `http2`. Then count the win the same way it was counted: a `.ts` file walking `process.getBuiltinModule("node:assert")` with `Object.getOwnPropertyNames`, before and after.

### ST4. Host globals are eager where the runtime's own are lazy ★ startup rank 3

rts‑core's globals are already built on first read (`entry/global.rs:81-184`, `supply`); the host's are not (`modules.rs:570-576`, `declare_global` puts a fully built value). 47 own properties on `globalThis` at program start, 45 of them host‑installed.

**Scope it honestly.** Of install‑std's 0.909 ms only `console::install` and `globals::install` (`rts-std/src/lib.rs:98-99`) are in scope — the other four steps build **modules**. Of install‑node's 1.809 ms, `crates/rts-node/src/lib.rs:203-206` attributes 1.4 ms to nine eager namespaces, five of which must stay eager for reasons stated there; only `process`/`timers`/`url`/`perf_hooks` are removable.

**Blocker to fix first.** `global_get_unbound` (`global.rs:235-269`) reads the own property and throws `ReferenceError` **without ever calling `supply`**. Exactly one installed global is outside `PROVIDED` (`emit/globals.rs:66-257`): `write` (`rts-std/src/globals/output.rs:23`) — verified live, `write("x")` compiles to `__rts_global_get_unbound`. Making it lazy without touching that path turns a working program into a `ReferenceError`.

**Cost:** `Object.getOwnPropertyNames(globalThis)` stops listing an unbuilt global. That divergence already exists for rts‑core's ~40; this widens it to 45 more, and `console` is the one global nearly every program touches, so it is deferral rather than removal (`empty.ts` 19.9 vs `hello.ts` 20.4).

### ST5. Free, already written: the env‑var switches are worth ~0.3 ms of startup

`RTS_CACHE_WHY=1 rts run empty.ts` emits **1957** `rts-why put` lines — the full count of growth `put`s during install — and `switches.rs` measured `env::var_os` for an absent name at **172.2 ns** (`bench/isolated/src/bin/env_probe.rs`, release, 2026‑08‑21). 1957 × 172.2 ≈ **337 µs**, ~1.7% of the 19.9 ms. Falls out of Phase 0's measurement; nothing to write.

### Startup, settled against

- **`alloc_zeroed` instead of the 8 MiB memset.** The memset is real (`region/mod.rs:317-319`, 65 536 × 128 B) but happens **once** per run (ST2), sits in `assemble` (`run.rs:1096`) strictly before `run`/`seed-context`, and moves demand‑zero faults rather than removing them — pages the process touches are paid either way, and `seed-context` gets marginally *slower*. Ceiling is under 0.502 ms, once, and **zero** on every ns/op row.
- **Dropping `ui` from the default build.** Seven of the nine window DLLs (`gdi32`, `imm32`, `ole32`, `oleaut32`, `setupapi`, `shell32`, `user32`) are in `HKLM\…\Session Manager\KnownDLLs` — mapped from a boot‑time section object, no path search, no rebasing. `.cargo/config.toml:47-53` already records that a wider `/DELAYLOAD` list crashed the unit‑test binary with `STATUS_STACK_OVERFLOW` then `STATUS_ILLEGAL_INSTRUCTION`, and there is **no delay‑load directory in the current PE at all** — the `opengl32` entry is a no‑op today. `rts-host/Cargo.toml:52-72` names the reversal condition (a wasm target), and it is not startup. What survives: `rts_ui::install` is ~0.14 ms (68 members at install‑dom's measured 0.0021 ms/member) and should get a `Phase` (ST1).
- **Snapshotting the seeded heap.** The region is base‑independent, but not self‑contained: `run.rs:336` interns the **program's** key texts first (`text.rs:324-330`, "interning is what mints the numbers"), so every built‑in name's `Key` — and therefore every shape node (`tree.rs:46-49`) and every `TypeId` compiled code guards on — depends on which names the program mentions. Cell references are absolute indices and program literals occupy the low ones (`run.rs:341`, `:365`). Three relocations, not one.
- **`opt-level = 3` for rts-std/rts-node.** The per‑member install work executes in `native::install` (`native.rs:55-63`) and `context.rs:387-388` — rts‑**core**, already at 3 (`Cargo.toml:188-189`). `lib.rs:203-206` measures the rts‑node‑side registration loop at **14 µs**. A profile flag on 45 000 lines of native bodies to speed a 250‑line function whose work is elsewhere.

---

## 5. LOCAL — one fix, one row (ranked)

**L1. `call closure make+call` 1672.46 — the `closure_new` diet.** Four candidates, two areas (calls ×3 + escape `non-escaping-closure`'s corrected cause). All in `crates/rts-core/src/entry/functions.rs:71-179`. (a) ~4 × 172 ns of `var_os` — **already fixed**, measure it. (b) An arrow gets a whole `prototype` object it must not have: a cell (`:106`), two `put`s (`:114`, `:133`), two `hidden`s, a `hold`/`release` pair whose `release` is `position()` + `Vec::remove` (`external.rs:53-56`). This is also a **correctness bug** (§8). (c) `.name` costs a third region cell because `intern_value` does not intern (`context.rs:309-333`, and its own doc at `:398-400` says so) — one line: use the existing memo `Context::key_text_value` (`context.rs:298-306`), which `roots.rs:176` already roots. (d) `functions.rs:148-152` is a **linear scan** of `context.function_names`, O(functions in the program), ending in `name.clone()`. (e) `"constructor"`/`"name"` → §S3. **Semi‑systemic:** `array map 16` and `filter 16` allocate a closure per `map()` call inside the bench loop, ~1672 of ~3534 ns (§7.3), so this moves those rows too. Effort **M**. **Experiment:** (C) `crates/rts-core/examples/entry_probe.rs` already has a `closure_new` row but runs with `function_names` **empty**, so it takes the cheap branch and measures the wrong thing — seed the table with a few hundred entries first; that fix is step one and makes the row a real before/after.

**L2. `coll Object.keys 4` 308.09 + `json stringify small` 5014.88 — one enumeration, two rows.** natives ×2. `json/write.rs:308` calls `array::own_keys` per object — which builds a heap JavaScript array of interned key **cells** (`array.rs:553-585`) only for `write.rs:311-316` to clone them into a Rust `Vec` and `write.rs:339` to convert each **back** to a `Str`. Call `array::key_texts` directly (already `pub(in crate::entry)`, `array.rs:361`) — that keeps exactly **one** ordering answer, which is what `write.rs:273-277` refuses a second walk for. Separately `array.rs:730` calls `text.to_rust()` per key to ask whether the spelling is an integer, duplicating `object::as_array_index` (`object/key.rs:53-83`), which `array.rs:214` already uses for that question; boundaries verified identical. Also real and uncounted: ~4 `HashMap` probes per key, a shape‑tree walk that allocates its own `Vec` (`tree.rs:229-241`), and `attributes_at`'s **linear scan per key** — O(keys²) (`integrity.rs:108-112`). Effort **M**. **Experiment:** (I) `bench/isolated/src/bin/key_order.rs` — `to_rust()` + `parse::<u32>()` + `to_string()` against a code‑unit index test, 4 short keys.

**L3. `alloc class instance` 90.89 — memoise the constructor's prototype and type.** objects, corrected. An `Aside<(u64, u32)>` keyed by the **resolved target cell** (the `cell` computed at `functions.rs:873` from `new_targets.last()`), never the callee — `functions.rs:772` calls `allocate_for_target(parent)` from `super_construct_inner`, and `functions.rs:745-749` names filing under the parent as "exactly the bug that makes `new B()` produce something inheriting from `A.prototype`". Invalidate from `objects::put` (`objects.rs:446`) on the `prototype` key, which also covers `Object.defineProperty` (`object_global/descriptor.rs:319`). Removes per `new`: one `well_known` scan, one chain walk (~6 map probes), one linear `typed_as` find. Effort **M**. **Experiment:** (S) a counter in `allocate_for_target` printed beside the timing line — it must equal the number of constructions exactly, which is the claim the memo removes; then (C) an `examples/` loop over `entry::construct`.

**L4. `string template literal` 477.50 — one buffer, one intern.** strings, not refuted, and its correction makes it larger. `text::template_join` (`text.rs:141-179`) allocates ~9 times and two region cells for a 5‑character answer: `Rooted::new()` boxes a `Vec` and pushes a thread‑local (`rooted.rs:91-96`) with an `rposition` scan on `Drop`; `string_of` **interns a whole cell** for the number's text (`:119`); `pieces.clone()` copies a `Vec<u32>` per evaluation (`:157`); `to_text` at `:172` **clones the `Str` `string_of` just built**; and `joined.concat(…)` at `:169`/`:175` allocates a fresh `Vec` per piece — under a comment at `:160-162` that says "One buffer, grown once and written through", which is currently **false**. Both cells are removable (the candidate wrongly conceded one): `to_primitive` takes and returns a `u64` and never needs the text as a value (`primitive.rs:67-70`), so the intermediate intern is pure ABI tax. With no intern, the `Rooted` dies too. Effort **M**. **Experiment:** (S) three `.ts` rows — `` `${i}` ``, `` `v=${i}!` ``, `` `v=${i}!${i}?` `` — the slope is the per‑piece concat and the intercept is the fixed machinery, measured with today's binary.

**L5. `prop instanceof` 240.80.** props + objects (`prototype-in-the-cell`'s corrected cause independently agrees the cost is the preamble). `functions.rs:999-1011` runs five `with_current` and a full **computed** `get_indexed` for `@@hasInstance` before any work — walking Base → Function.prototype → Object.prototype to answer Absent, and interning the 13‑unit key text every time. Fix: `HAS_INSTANCE` into `CACHED_KEYS` (§S3, kills the hash) plus resolve the hook with `accessor::resolve(context, callee_slot, key)` **inside the borrow the walk already takes** (`functions.rs:1012`). Preserve the two paths that must leave the borrow: `Found::Getter`, and `context.proxy_at(callee).is_some()` — a Proxy `get` trap can supply `Symbol.hasInstance` and does today (verified: answers `true`). Do **not** use the proposed `has_instance_hooks` counter — it cannot see a proxy trap, and it becomes permanently non‑zero the day `Function.prototype[Symbol.hasInstance]` is installed (§8). Effort **M**.

**L6. `arith int mul` 7.55 — `| 0` on the loop‑carried chain.** codequality. `Proven::Bits` (`expr.rs:1518-1523`) emits `to_int32` on **both** operands; `Inst::ToInt32` lowers to nine branch‑free instructions (`lower/body.rs:315-328`) and `ToF64` adds one. In `a = (a*3)|0` the whole chain is loop‑carried — which is why the same operator costs 2.62 ns in `int div`, where it sits off the chain, and `lower/body.rs:301-303` records the same shape at 11.6 ns on the chain against 2.98 off it. The IR also runs it **twice**: `v28: I32 = ToInt32(v26)` where `v26 = Const(F64 0.0)` — nothing folds it, and `Bitwise(Or, x, 0)` is the identity, and `ToF64(ToInt32(x))` round‑trips. **Do the fold first** — it is safe, and the fold must live in **one** place: `ToInt32` is already stated twice in Rust (`value/convert.rs:128`, `emit/fold.rs:167-176`, whose doc at `:148-160` says a fold disagreeing with the runtime is worse than no fold), so a third copy is the defect. The two‑instruction fast path (`fcvt_to_sint_sat(I64)` + `ireduce`, guarded on |x| < 2⁶³) is sound but overturns `ir/inst.rs:196-197`, which sells branch‑freedom as the design point — RULE 0 requires that comment change first, with the reason. Scope the claim to `int mul`; `int div` (1.35 ns of headroom) and `switch 8-way` (2.95 ns) are already throughput‑hidden. Effort **M**. **Experiment:** (I) `bench/isolated/src/bin/toint32.rs` — the nine‑op sequence against `(x as i64) as i32 as f64`, on and off a dependency chain.

**L7. `flow try/catch no throw` 10.31 — give the join block parameters.** codequality, corrected, and the correction inverts the fix. The region itself is free (`; protected by RegionId(0)`, zero instructions at entry; `unwind/plan.rs:3-6` — the answer is compiled in, not searched). The cost is a `CachedGet`/`CachedSet` pair per iteration because `assigned_under_protection` (`capture.rs:292`) puts every name assigned in a `try` into the environment object. **But it is not a handler‑reachability guard** — `protect.rs:249` snapshots the scope, `:269-270` leaves the body with `builder.jump(join, &[])` (an **empty** argument list), and `:410-411` restores the snapshot at the join. Memory is the only channel by which the try body's assignments reach the code after the `try` **on the path where nothing is thrown**. Making the demotion conditional makes the loop return 0 — the identical bug is already documented at `destructure/array.rs:378-386`. The correct change is in `protect.rs`: give `join` block parameters for the names the body assigns and pass them from the fall‑through edge. Ordinary SSA, no throwability analysis, no capture/proven fixpoint. Effort **M–L**. **Experiment:** (S) the IR diff is the test — `CachedGet`/`CachedSet` count in the loop, plus a fixture `try { a += 1; f(); a += 1 } catch {}` where `f` throws, proving the handler still sees the first assignment.

**L8. `alloc array literal 4` 231.34 — array literals as escape candidates.** escape + arrays (**two areas**, both not refuted). `escape.rs:336` admits only `ExprKind::Object`; and `xs[0]` is an `ExprKind::Index`, which `scan_expr` does not model, so it reaches the catch‑all kill at `escape.rs:719-721`. Note the arrays verdict's correction: `literal_name`'s digit rule is **irrelevant** — `expr.rs:474-476` returns `None` for a number literal seven lines before the digit test. Three obligations the proposals missed: `proven.rs:680-687` and `:531-542` answer about a replaced property for `Member` only, so without an `Index` arm the row lands far short of the floor; a read‑only `length` key cannot be expressed by `note_use`, which branches on `is_write` only for depth (so `xs.length = 2` would silently write a binding); and **`escape.rs` is 1049 lines against the 1000‑line ceiling** (`crates/rts-codegen/README.md:95-99`) — the array key space goes in a sibling module. Effort **L**. **Experiment:** (S) hand‑flattened twin row in `analytic.ts` (`const x0=i,x1=i,x2=i,x3=i; a += x0;`) against the existing row — the gap is the entire ceiling, measured today. **Keep an escaping array row in the bench**, or the last real array‑allocation instrument disappears with the allocation (§7).

**L9. `flow generator next` 805.09.** codequality, corrected. `"value"`/`"done"` are not in `CACHED_KEYS` (§S3) so `generator::result` (`mod.rs:396-404`) does two `Str::from_str` allocations and four hash probes **per `.next()`**; `made()` (`mod.rs:411-421`) does `class_support::prototype(context, "Generator")`, a **linear scan comparing `&str`** (`class_support.rs:62-67`), per call; and `native::plain` takes a region cell per `.next()`. Shared with `list_iterator`, `array_proto/cursor` and `collections/cursor`, so it reaches the for‑of and Map/Set iteration rows too. Do **not** reuse the `{value,done}` object — `CreateIterResultObject` must be fresh and the entry point has no escape information.

**L10. `string parseInt` 138.48 / `parseFloat` 167.20 / `string index []` 123.03.** strings, not refuted. (a) `number::parse::leading` (`parse.rs:83-85`) does `to_text(...).and_then(to_rust)` — a `Str` clone **and** a `String` allocation; a `narrow()` + `is_ascii()` fast path yields a `&str` with no allocation, and that exact shape is already written at `basic.rs:492-509` and `text/normalize.rs:81`. Note the correction: ~50–65 ns of that row is five entry crossings the change cannot touch, so quote ~30–45 ns, not 40–90. (b) `string::text::string_element` (`text.rs:96-104`) allocates a malloc, a `Slab` slot and a 128‑byte cell **per character**; a `[Option<u64>; 256]` cache is sound (`context.rs:293-297`: a JS string is immutable and has no observable identity) — and **must be added to `roots.rs`** beside `type_names`/`well_known_texts`/`key_texts_as_values` (`roots.rs:165,170,175`), each of which carries a comment naming the exact use‑after‑collect this would otherwise reproduce. `roots.rs` is missing from the proposal's file list. (c) `string number->string` is on neither path — drop it from the claim. Effort **M**.

**L11. `regex exec+group` 2268.25 / `regex test` 117.28.** natives, corrected. The target is **per‑match bookkeeping**, not the subject copy: `pattern.rs:426-441` allocates a `Spans` vec, a `groups: Vec<Option<String>>` with a `String` per group, and `compile.rs:105-118` `names()` builds its own `Vec<Option<String>>` with `to_owned` per capture name — every match. `compile.rs:80-86` already records the shape: "280 ns for a three‑character subject, of which only 85 more appear when the subject grows to 251 — so the cost was per CALL and not per character." Cheap wins beside it: hoist `text_of` above the `search` call (`methods.rs:275` duplicates `:177`) — pure deletion; make `named_groups` iterate without collecting. **Do not** borrow the subject: `methods.rs:196` returns the owned text *out* of the borrow and `:207-210` says why ("interning ALLOCATES and an allocation collects"); `replace.rs:74-84` then runs user JavaScript; and `search.rs:70-82` records this exact transformation **measured slower** (886.7 → 990.9 ms) and reverted.

**L12. Buffers.** natives, corrected. `install_bytes` (`buffers/mod.rs:241-251`) writes `byteLength` and `detached` through two separate `objects::put` growth transitions where `attach` (`:277-338`) shows the one‑shape‑walk form and `:279-291` names what the two `put`s waste. `"detached"` → `CACHED_KEYS` (§S3). And an unclaimed third: `to_number`'s fast path (`class_support.rs:198-200`) does **not** fire for an encoded Int, so `u8.subarray(0, 64)` takes **five** `with_current` before any work. Correction to the candidate: `new Uint8Array(n)` and `subarray` each do **one** class‑name scan, not two — only `make_bytes` (the TextEncoder path) does both. And `context.classes` is short, because registration is lazy (`global.rs:118-176`), so count it before optimising it.

**L13. `array join 16` 151.56 and four siblings.** arrays, corrected to its sound half only: `joining.rs:94-105` allocates one `Vec<u16>` **per element** purely to concatenate at `:109-114`; `joined.extend(text.units())` removes n+1 allocations with no snapshot change and no extra borrow — the technique already exists at `json/write.rs:430`. The same `Vec<Vec<u16>>` shape is at `array_proto/more/mod.rs:259,270`, `more/sorting.rs:280`, `eval.rs:188`, `json/mod.rs:136`. **Do not** re‑read per index: it costs N `with_current` and changes observable behaviour, and `joining.rs:43-46` documents the snapshot as deliberate.

**L14. `coll Map.*` 43–67.** natives, corrected to a 4‑line change: both `has` methods throw away the cell `branded` already returned (`map.rs:86`, `set.rs:90` call `.is_none()`) and `held` re‑derives it. Sub‑nanosecond. The residual in those rows is the prototype method read (§S1) and `Table::slot`'s per‑step `same_key` → `context.same_text`.

---

## 6. Investigations to open (deliverable is a number, not a diff)

- **§S1a**, the ~16 ns of the call tax nobody has attributed.
- **§S3's frequency table** — which `well_known` names are actually hot, from a counter, not from five people's intuitions.
- **§S4's four‑number decomposition** of array construction.
- **§S3/§S8: `crates/rts-core/src/entry/collect_cycle.rs:139-235`.** Nothing measured `release`. In steady state it runs once per allocation, does 22 `Aside::remove` calls plus `weak::clear_freed` and `finalize::queue_freed`, and it is amortised into every allocating row in the table. It was found three times as a *correction* and never as a candidate. **(C)** an `examples/` loop is enough to price it.

---

## 7. The instrument lies in six places — fix `bench/analytic.ts` before quoting it again

This is a finding in its own right and it is required by the honesty floor.

1. **`alloc object literal 2` (1.22) and `8` (1.24) are below the 1.27 ns floor.** The literal is scalar‑replaced; the IR contains no `ObjectNew`. Those rows measure nothing about allocation. Add an escaping twin.
2. **`call free function` (3.21) and `call arrow` (3.11) contain no call.** `emit/inline.rs` substitutes the body; the loop is `FloatArith(Add, v, 1.0)`. Confirmed by three separate agents' IR dumps (and `inline.rs:158-180` accepts `const f = (x) => …`, which is why the two rows are identical). **One verdict claimed the opposite and is wrong** — its test file used a function that did not qualify for inlining. Any per‑call cost derived by differencing against these rows is invalid.
3. **`array map 16` (220.89) and `filter 16` (223.51) mostly measure `closure_new`.** The bench allocates the callback **inside** the loop: `rts ir` shows `Call { callee: __rts_closure_new }` in block34, inside the back edge. `ops: 16` means a call is ~3534 ns, of which ~1672 is the closure. Re‑measure with the callback hoisted before attributing anything to `map`.
4. **`string split 16` regressed 2.7× and nothing recorded it.** `crates/rts-core/src/entry/string/split.rs:26-28` records this exact row at **1755.86 / 1791.70 / 1756.84 ns** on 2026‑08‑13; it is 4799.01 now. `bench/analytic.ts` is byte‑identical between those points (last changed `9c057939`, 08‑11). The suspects are `split.rs:70` (`pattern::hooked`, added `77b6ce6c`, 08‑15) and `split.rs:64`/`:77-78` (`coerce_receiver`, `to_primitive`, added `da9ddd21`, 08‑16). `string slice 16` moved 118–122 → 206.59 over the same window, so ~1.7× is systemic and split's remaining ~1.6× is its own. **Bisect this.** CLAUDE.md: "regress explicitly, never silently."
5. **Per‑op divisors are not uniform and three verdicts got them wrong.** `analytic.ts:610` computes `best*1e6/(n*ops)`; `ops` is 16 for map/filter/join/for‑of, 4 for `Object.keys`/`read 4 fields`, 2 for `push+pop`, 1 elsewhere. State it beside the table.
6. **`prop typeof alone` (13.10) is not cleanly "two crossings"** — the subject is a module‑level `const` read out of the environment, so the row includes a property read and two throw checks. Per‑crossing costs derived by dividing it by two are soft by roughly a factor of two. `docs/codegen/entry-tax.md` and `docs/engine/new-engine-speed.md:544` (2.50 ns) are the better rulers.

Also worth a line in `measurements.md`: **`arith int add` (1.22) is below the floor**, and five rows sit at or under it, where the harness dominates.

---

## 8. Correctness findings — file these as bugs, not as perf

Found while checking performance claims. None of them is a slow program; all of them are a wrong one.

1. **Arrows, methods and async functions carry a `prototype` they must not have.** `functions.rs:104-136` builds one for every callable, justified at `:99-102` by "every function gets a `prototype`, because `new` reads one" — false for three of the six kinds. Verified: `Object.getOwnPropertyNames((x)=>x)` answers `prototype,name,length` where node answers `length,name`, and `new (arrow)()` answers an object instead of throwing.
2. **`Function.prototype[Symbol.hasInstance]` is missing.** A spec‑required intrinsic. `tests/cross-runtime/classes/claude-instanceof-bound-and-ordinary-has-instance.ts:16-18` already asserts it and dies at line 18 with `TypeError: Cannot read properties of undefined (reading 'writable')`.
3. **`Array.prototype.join` does not re‑read per index.** `const mut=[1,2,3]; mut[0]={toString(){mut.length=1;return "S"}}; mut.join("-")` answers `"S-2-3"` here and `"S--"` on node. `joining.rs:43-46` documents the snapshot as deliberate; the spec says `Get` per index. Fixing it **costs** time — file it as conformance, not perf.
4. **`join` does not honour rule 8.** `joining.rs:103` uses `.unwrap_or_default()` and never asks `throw::in_flight()`, so it keeps running later elements' `toString` after one has thrown. Not a wrong answer today (the call site's post‑call check propagates), but any rewrite of that loop must carry the check.
5. **`af.name` and `gen.name` are `undefined`** for async functions and generators: `emit/wrap.rs` never pushes `ctx.function_names`, so `functions.rs:148-153` finds no row. This also means any "carry the function's kind in `function_names`" design silently misses exactly the two kinds that need it.
6. **`Math.sqrt(g())` evaluates `g` twice** when the argument's repr refuses the machine operation: `call.rs:211` emits the argument **before** the refusal at `:212-214`, and `call.rs:88-90` then re‑emits the whole call. Verified: prints `2 2` here, `2 1` on node and bun. `inline.rs:136-138` has the same shape for the spread refusal. **Must be fixed before any change to `machine_operation`.**
7. **`Reflect.setPrototypeOf("abc", …)` succeeds** and `"abc".foo` then answers the injected property, because `chain.rs:108-122` accepts any cell `Value::as_slot` accepts and the invalidation at `:121-129` is gated on `shape_of(ty)`, which is `None` for text. This is a prerequisite for §S3's string‑cache work and a divergence on its own.

**Documentation that the code contradicts** (RULE 0: change the rule first, with the reason): `runtime/mod.rs:170-174` + `expr.rs:1437-1439` + `tests/remainder.rs:200` all claim `NumberRemainder` carries no throw check; `text.rs:160-162` claims template_join uses "one buffer, grown once"; `array.rs:611`'s comment about `iterate` copying verbatim is stale (`iterate.rs:79-84` now maps through `visible`); `target/mod.rs:1074-1078` quotes an empty‑loop floor of 7.2 ns from a tree whose floor is now 1.27; `context.rs:375` says "five short `&str`s" for a list of six.

---

## 9. What I am NOT proposing, and why

Each of these is an idea a reasonable engineer would try. Each was refuted with evidence; the evidence is the deliverable.

- **Replace `RefCell<Vec<Context>>` with `Cell<*mut Context>`.** The experiment was already built and run: `bench/isolated/src/bin/entry_tax.rs`, six shapes, and `docs/codegen/entry-tax.md` opens "**No. It is worth 0.53 ns of them.**" The whole of it is the `Vec` — a `RefCell` borrow and a `const`‑initialised `thread_local!` each cost nothing measurable. It would also demote a **checked invariant** (no re‑entrant borrow, which `authoring-natives.md:65-75` and eight rts‑core modules are shaped around) to a debug assertion, i.e. silent UB in release for 0.53 ns.
- **Merge `with_current` borrows generally.** `docs/engine/new-engine-speed.md:629-633`: one such merge was written, measured, and dropped — "no measurable win on any call row, and cost 7.6 points on `instanceof`." `json/write.rs:330-337` records a second: collapsing borrows moved `{a:1}` from 1942 to **2084** ns. Two independent measured negatives.
- **Turn on Cranelift's mid-end (`RTS_CL_OPT=speed`).** `Priority::CodeQuality` has one call site — `target/destination.rs:71`, the **AOT** object‑file path; `rts run`/`test` go through `host_isa()`, hard‑wired to `CompileTime`. The mid‑end also cannot move a `Guard`, which is a terminator lowering to `brif`. And `target/mod.rs:1017-1020` measured `speed` at +27% placement time.
- **Change the calling convention, or `Convention::Tail`.** ~0.3–0.6 ns by instruction count. Decisively: there is no direct‑call path at all (`emit/call.rs:501` ends every call in `RuntimeOp::Call`), and `call free function` and `call method` emit byte‑identical seven‑operand sequences — so the 26 ns between them is provably not the convention. `CallConv::Tail` also shrinks the callee‑saved set and the caller is `rustc`‑compiled `extern "C"`, so a trampoline stores and reloads the same two words.
- **Scalar‑replace class instances / inline constructors.** `escape::analyse` is per‑function‑body with no `Ctx` and no module (`function.rs:627`); `class::with_fields` is an emit‑time function needing `&mut Ctx` and evaluated computed‑key `ValueId`s; after it the prologue is `this.k = "@@rts_field_initialiser"(e)`, and that marker exists because `new.target` differs inside it (`class.rs:568-575`). The bench's own class has a method, so it fails the proposal's own gate.
- **Substitute a local arrow at its call sites.** `Scope::lookup` resolves innermost‑layer‑of‑the‑**call‑site** first (`scope.rs:554-565`). Demonstrated: `function g(){let i=1; const c=()=>i; let out=0; {let i=99; out=c();} return out;}` answers 1 today and would answer 99. `function` expressions additionally break `this`.
- **Widen escape analysis to a key the literal did not write.** §3, refuted on semantics with a program that agrees between rts and node today and would stop agreeing.
- **Per‑kind inline‑cache cell sizing.** `target/mod.rs:757-763` records this being tried and reversed: a store resolver writes six words and a read site sized for three landed beside it — memory corruption, not a wrong answer. Also, `frame/transform.rs:161-170` declares `source.cache_count()` sites before any terminator exists, so a kind‑walk finds no kind for some of them.
- **An environment‑object census.** Refuted: `rts ir | grep` already answers it more finely at zero cost, and the two existing counters (`RTS_ESCAPE_STATS`, `types/census.rs`) exist precisely because their subject is *invisible* in the output — an emitted `ObjectNew` is the trace itself.
- **Removing the escape template kill‑switch as a perf change.** Already done in the tree, and measured at **exactly zero** replacements gained over 400 files: all six recovered candidates die under an independent rule. It is a correct docs/refactor change and must ship as `refactor:`, not `perf:`.
- **Hoisting the loop‑invariant parameter guard.** The guard is off the loop‑carried dependency chain, `new-engine-speed.md:577-590` measured the equivalent hoist as noise, and the only representable non‑duplicating form coerces on the fail path — which breaks BigInt (`0 < 1n` is legal) and moves a `valueOf` side effect.
- **`fs`/`inspector`/`util` lazy, and snapshotting the heap.** §4.

---

## 10. Suggested order of work

1. **Phase 0** — build, keep the baseline, measure the five in‑flight changes per row, run a program ruler and the corpus, commit them with their numbers (`docs/codegen/README.md` rule 8: a landed optimization says what it actually moved).
2. **§7.4** — bisect the `string split 16` regression. It is a 2.7× on a recorded number and the honesty floor makes it the first thing that is *owed*.
3. **§7** — fix the six instrument defects, so the table means what the next twenty decisions will assume it means.
4. **§S1a + §S3's counter + §S4's decomposition** — three experiments, all runnable in `bench/isolated/` or `crates/rts-core/examples/`, none needing a workspace release build. Together they price the three largest unexplained blocks in the table.
5. **§S1 (SetCallName), §S5 (non-raising), §S6 (Widen)** — the three cheapest systemic changes, each with a structural test that needs no timing.
6. **§8.6 and §8.1** — the two correctness bugs that block later perf work (`machine_operation` double evaluation; the arrow `prototype`, which is also L1's largest item).
7. Then the locals, in the order listed, each gated on its own experiment.