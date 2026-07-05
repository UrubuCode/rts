# Plan — new GC + modernized gc API/ABI (new engine, zero legacy)

> **Status:** EXECUTION PLAN. To be executed in a dedicated session. Twofold,
> clear objective: (1) deliver the BEST GC for RTS's native environment;
> (2) deliver the BEST gc API/ABI for the new engine (`rts-codegen-new`), with
> NO baggage from the old engine. There is power to redo the entire ABI — do not
> be limited by the current communication form (extern-C-per-function,
> handles-as-i64); adapt freely to the new engine's more upgradable logic.
>
> Reading prerequisite: [`gc-generational-design.md`](gc-generational-design.md)
> (the collector's design — weak phase + generational copying nursery) and
> [`rts-codegen-new-design.md`](rts-codegen-new-design.md) §5 (PolyValue / GC).

## 0. Why now (what is getting in the way)

The current `gc.*` API is mostly from the OLD ENGINE (`i64` handles where a
string handle DOUBLED as the string itself). In the new engine strings are
native `PolyValue` `TAG_STR` — the **manual string pool** API became garbage
that actively breaks:

- `gc.string_from_i64/f64/concat/new/from_static` return a Handle with
  `ts_signature: number` → the new engine reboxes the result as a raw NUMBER
  (`ret_is_string_handle = ts_returns_string(ts_sig)`), so
  `print(gc.string_from_i64(v))` prints the handle's number, not `"v"`.
- **~110 legacy fixtures** use the manual pattern
  `const h = gc.string_from_i64(v); print(h); gc.string_free(h);` — pure
  old-engine pool dance. The new engine does this NATIVELY: `print(String(v))` /
  `` print(`${v}`) `` / `print("" + v)` (all verified working).
- The `rts:test` harness (BUNDLE_TS) does NOT use `gc.*` — only the fixtures.
  So migrating is safe and isolated.

Conclusion: do NOT "fix" `gc.string_*` (it is not the new engine's form).
REMOVE the manual pool API + MIGRATE the fixtures to native strings + REDESIGN
the remaining `gc.*` surface to be PolyValue-native.

## 1. Audit of the current surface

### 1.1 The collector (engine)
- `crates/rts-engine/src/heap/handles.rs` — **2078 lines**, `enum Entry` with
  ~40 variants (a mix: `String`/`Buffer`/`Vec(Box<Vec<i64>>)`/`Map(Box<IndexMap>)`
  + backend `Tcp*`/`Tls*`/`Udp*`/`Sync*`/`Atomic*`/`JoinHandle` + new-engine
  `Instance`/`Function`/`Symbol`/`WeakMap`/`Proxy`/`FinalizationRegistry`).
- `crates/rts-std/src/collector/` — `collector.rs` (165, mark+sweep + gcells),
  `string_pool.rs` (1284), `stack.rs`, `generator.rs`, `error.rs`, `mod.rs`.
- Current GC: precise mark+sweep (UserStackMap + conservative scanner), `GCELL_*`,
  scanner recognizes NaN-boxed PolyValue words (design §5.4).

### 1.2 The `gc.*` API (members — classify)
`string_from_i64 string_from_f64 string_concat string_eq string_cmp
string_from_static string_new string_len string_ptr string_free handle_len
env_alloc env_get env_set env_free closure_alloc closure_fn_ptr closure_env
instance_new instance_class instance_free instance_load_i64 instance_store_i64
instance_load_i32 instance_store_i32 instance_load_f64 instance_store_f64
collect collect_vec live_count` + internals `POLY_TO_HANDLE POLY_FROM_HANDLE
GCELL_GET GCELL_SET COLLECT COLLECT_DEBT`.

Classification:
- **LEGACY / REMOVE** (old-engine manual string pool — replaced by
  PolyValue TAG_STR + String()/template):
  `string_from_i64 string_from_f64 string_concat string_eq string_cmp
  string_from_static string_new string_len string_ptr string_free`.
  (The POOL itself — `Entry::String` + intern — STAYS; it is where TAG_STR
  lives. What goes is the `gc.string_*` SURFACE exposed to the user/fixtures.)
- **LEGACY / REMOVE** if the new engine does not use them: `instance_load_i32/store_i32`,
  `instance_load_f64/store_f64`, `handle_len`, `collect_vec` — confirm 0 usage in
  `rts-codegen-new` + migrate the few fixtures.
- **EVALUATE / MIGRATE to PolyValue-native**: `env_*` (closures #195 use a
  PolyValue env now — check whether the `env_alloc(i32)/env_get/set` API still
  fits, or is replaced by the current cell/Vec mechanism), `closure_*`, `instance_*`
  (class instances are keyed `Entry::Vec` now — `instance_new/class/load/store`
  may be duplicating the `__rtsadp_obj_*` path).
- **KEEP (new-engine core)**: `POLY_TO_HANDLE POLY_FROM_HANDLE GCELL_GET
  GCELL_SET COLLECT live_count` (+ the `__rtsadp_obj_*` that ALREADY are the
  PolyValue-native object API).

### 1.3 `enum Entry` — what the new engine actually needs
FROM THE NEW ENGINE: `String` (TAG_STR), `Vec(Box<Vec<i64>>)` = keyed
object/array (slot0 shape + PolyValue values), `Function`, `Instance` (evaluate
whether still distinct from `Vec`), `Symbol`, `WeakMap/WeakSet/WeakRef/FinalizationRegistry`,
`Proxy`, `Closure`/`Env`, `ErrorObj`, `DateMs`, `Regex`, `Json`, `BigFixed`,
`Promise*`, `Buffer`, and the backend ones (`Tcp*/Tls*/Udp*/Sync*/Atomic*/Http*/Events*/
JoinHandle/ProcessChild/Hasher/CString/OsString`) — these backend ones stay
(they are real resources of active namespaces), but must be AUDITED: any one
without a live path in the new engine is garbage.

## 2. Part A — The GC (best collector for the RTS environment)

No architecture rewrite now — follow the phased path of
[`gc-generational-design.md`](gc-generational-design.md):

- **A1. Weak phase (small, #217):** REAL WeakMap/WeakSet/WeakRef/FinalizationRegistry
  via a phase between mark and sweep in the current mark+sweep. Does not rewrite
  the GC. Today they are interim strong-ref `.ts`/Entry.
- **A2. Generational copying (nursery) (big, DEFERRED until ~90% cross-runtime):**
  young bump-alloc + minor GC copies survivors + write barrier + remembered
  set + per-thread TLAB. The handle indirection (PolyValue = slot index) makes
  MOVING ≈ free (updates only slot→address, no pointer-patching). Old gen
  mark-compact, runs rarely.

**This API update is what PREPARES the ground for A1/A2:** a lean `Entry` + a
PolyValue-native ABI + correct child tracing (NaN-boxed words) are
prerequisites of a clean moving collector.

## 3. Part B — HandleTable / `Entry` redesign

- **B1. Audit every `Entry` variant** for live usage in the new engine + active
  namespaces. Remove every dead variant (garbage). Document the child tracing
  of each surviving variant (what mark/copy must visit) — a prerequisite of the
  generational.
- **B2. Break up `handles.rs` (2078 lines)** into submodules < 500 (layout rule)
  — `entry/` by category (primordial / collection / backend / weak), centralized
  tracing.
- **B3. Confirm the PolyValue↔handle contract** in a single place: `POLY_FROM_HANDLE`
  (64-bit handle → 48-bit slot payload) and `POLY_TO_HANDLE` (payload → handle,
  generation read from the slab). Today the `& PAYLOAD_MASK` rule was a gotcha
  (see Proxy #218) — centralize and test.
- **B4. Keyed object = `Entry::Vec`** of PolyValue words (slot0 = shape-id).
  Evaluate merging `Instance` into `Vec` (a class instance is already a keyed
  object). A single object type simplifies the generational's tracing.

## 4. Part C — `gc.*` API/ABI redesign (zero legacy)

- **C1. REMOVE the manual string-pool surface** (`gc.string_from_i64/
  f64/concat/eq/cmp/new/from_static/len/ptr/free`) from the spec (`collector/mod.rs`),
  from the new engine's `runtime_link.rs`/`abi_sig.rs`, and from the JIT
  symbols. Strings are TAG_STR; conversion is native `String()`/template/`+`;
  comparison is native `===`/`<`.
- **C2. REMOVE `instance_load/store_i32/f64`, `handle_len`, `collect_vec`** and
  any other member without a live path in the new engine (confirm via grep in
  `rts-codegen-new`).
- **C3. MODERNIZE the communication form** (full power to redo the ABI):
  - The remaining `gc.*` surface is INTERNAL (engine↔runtime), not user-facing
    TS. It needs neither one `extern "C"` per trivial operation nor a lying
    `ts_signature`. Define a minimal PolyValue-native ABI: `collect()`, `live_count()`,
    `poly_to_handle`/`poly_from_handle`, `gcell_get/set`, and the `__rtsadp_obj_*`
    (get/set/has/delete/keys/values) as THE canonical object API.
  - Where it makes sense, replace multiple externs with a data-driven path
    (aligned with the design's §10 — ABI derived from SPECS). Do NOT carry over
    the old pattern of "one manual symbol per operation".
- **C4. `env_*`/`closure_*`/`instance_*`**: reconcile with the new engine's
  current mechanisms (per-invocation cell #195, `__rtsadp_obj_*`, closure env
  as a Vec of PolyValue). Remove what duplicates; keep a single path.

## 5. Part D — Migration of the ~110 legacy fixtures

- **D1. Rewrite the pattern** `const h = gc.string_from_i64(v); print(h);
  gc.string_free(h);` → `` print(`${v}`) `` (or `print(String(v))`). Ditto for
  `string_from_f64`/`string_from_static`/`string_concat`. Migration script
  (regex) + manual review of the composite cases.
- **D2. Fixtures that test the gc API ITSELF** (`alloc_*`, `gc_instance_*`,
  low-level `env_*`): decide case by case — if the API was removed, the fixture
  goes with it (it tested the old engine) OR it is rewritten for the new
  mechanism. The honesty floor: never delete a fixture to inflate the number;
  delete only what tested an API removed by design (explicit, justified
  regression).
- **D3. Re-measure** real correctness (assertion-level, not just run-exit-0 —
  see [`project_measure_metric`] in memory: `measure_new.sh` counts execution
  coverage, not correctness). Expected: a big CORRECTNESS jump when the ~110
  fixtures stop printing handle numbers.

## 6. Execution phases (order)

1. **C1 + D1 first** (immediate unblock, low risk): remove `gc.string_*`
   + migrate fixtures to native strings. This alone unblocks the ~110 fixtures
   and clears the biggest "getting in the way". Measure correctness before/after.
2. **C2/C3/C4 + B1**: drain the rest of the legacy API + audit `Entry`. Each
   removal: zero-usage grep in the new engine, green suite, gate with no hard
   violation.
3. **B2/B3/B4**: refactor `handles.rs` into submodules < 500 + centralize the
   PolyValue↔handle contract + merge Instance/Vec.
4. **A1 (weak phase / #217)**: with the lean Entry + documented tracing, the
   weak phase lands clean.
5. **A2 (generational)**: dedicated project, DEFERRED until ~90% cross-runtime
   (do not swap the GC while the engine is still filling in semantics — it only
   adds an unstable variable on the critical path).

## 7. Invariants / floor (never yields)

- **Build compiles; suite known at every step; regression only explicit and
  justified** (the REGRESS-WHEN-NECESSARY rule).
- **PRIMORDIAL-vs-Registry doctrine**: the engine names only primordials; `gc`
  is an internal namespace (not a non-primordial class), but the way it is
  called follows the new engine's ABI, with no non-primordial class hardcode in
  the front.
- **Layout**: no engine file > 500 lines (break up `handles.rs`).
- **Metric honesty**: use a CORRECTNESS measure (parse `✗` from `run-new`),
  not just run-exit-0. Never delete a fixture to inflate; fixture removal only
  when it tested an API removed by design.
- **GC**: nothing that crashes/hangs committed as "pass". The scanner
  recognizes NaN-boxed PolyValue words (design §5.4); keep that invariant in
  any `Entry` change.

## 8. Definition of done

- `gc.string_*` and every legacy member REMOVED; `grep` for them in the repo = 0
  (outside history).
- Fixtures migrated to native strings; correctness (assertion-level) measured
  and rising.
- `handles.rs` broken into submodules < 500; `Entry` audited (zero dead
  variants); PolyValue↔handle contract centralized + tested.
- The remaining `gc.*` ABI is PolyValue-native, minimal, with no lying
  `ts_signature`, aligned with the design's data-driven direction.
- (Phase A1) real WeakMap/WeakSet/WeakRef/FinalizationRegistry via the weak phase.
- (Phase A2, deferred) generational copying nursery — dedicated project post-90%.
