# Shape identity remediation — plan

**Status:** proposed, not implemented. **Date:** 2026-07-29.

This document is the outcome of a 4-round adversarial design review (20 agents)
of the object / class / array representation. It records:

1. the **five defects reproduced** against `bun` on the current `main`,
2. the **implementation plan** for each, ordered by measured value,
3. the **designs that were refuted**, so they are not re-proposed.

Everything below that claims a behaviour was *measured* was run on the release
binary (`target/release/rts.exe run-new`) against `bun 1.3.14`. Nothing here is
inferred from reading code alone unless explicitly labelled UNVERIFIED.

---

## 1. Measurements that frame the work

| Benchmark (1M iterations) | RTS | bun | ratio |
|---|---|---|---|
| allocate `{x,y}` in a loop | 427 ms | 6 ms | 71× |
| read `o.x` where `o: any` | 436 ms | 2 ms | 218× |
| read `p.x` where `p: P` (nominal class) | **13 ms** | 3 ms | **4.3×** |
| `globalThis["k"+i]=i`, n=2000 | 777 ms | 1 ms | 777× |

Read this carefully, because it inverts the intuitive priority:

- **The proven-shape path is already good** (4.3× off bun).
- **The same work through an untyped receiver is 33× slower than through a
  nominally-typed one** (436 ms vs 13 ms). This is not a growth effect — it is a
  fixed 2-key object, 1M reads, paying a linear key scan under a process-global
  mutex on every access, because `crates/rts-codegen-new/src/ic.rs` **does not
  exist** despite the design doc §8.2/§8.3 listing inline caches as implemented.
- `rts ir` confirms the split: a nominally-typed param lowers to
  `call fn0(recv, 1)` (constant slot); an `any` param **and an `arr.map(q => q.x)`
  callback param** both lower to a string-keyed dynamic lookup. Callback params
  are ordinary JS, not an edge case.
- bun allocating 1M objects in 6 ms is bun **eliminating the allocation** (escape
  analysis / scalar replacement).
- The `globalThis` quadratic is a **tail** pattern. The repository's own corpus
  does not contain it: 3 hits in `tests/` for computed-key literals, none of them
  the grow-by-N-dynamic-keys shape; 0 hits in `bench/`.

---

## 2. Reproduced defects

### D1 — Plain objects acquire a class's identity and execute its methods (soundness)

```ts
class Point {
  x: number; y: number;
  constructor(a: number, b: number) { this.x = a; this.y = b; }
  greet() { return "Point method x=" + this.x; }
}
const bare: any = {}; bare.x = 42; bare.y = 7;
bare instanceof Point   // RTS: true         bun: false
bare.greet()            // RTS: "Point method x=42"   bun: TypeError
```

**Mechanism.** `intern_class_shape` (`crates/rts-engine/src/heap/shapes.rs:60-66`)
publishes the class's shape id into the content-addressed `by_keys` map:

```rust
let id = GLOBAL_SHAPE_BASE + reg.keys.len() as GlobalShapeId;
reg.keys.push(keys.to_vec());
reg.by_keys.entry(keys.to_vec()).or_insert(id);   // <-- the defect
```

Any later `intern_global_shape(keys)` (`shapes.rs:46-55`) with the same **ordered**
key list receives the class's id. `vdispatch.rs:385-403` then compares the
receiver's slot-0 shape word against each candidate class's baked
`global_shape_id` with a flat integer compare and **no constructor check**, so a
method compiled assuming constructor-initialised fields runs against an object
that never ran the constructor.

**Routes reproduced** (all RTS `true` / bun `false`):

| route | reproduced |
|---|---|
| dynamic adds `bare.x=1; bare.y=2` | yes |
| object literal `{x:1, y:2}` (`front/run/obj.rs:102`) | yes |
| `{...instance}` spread | yes |
| `Object.assign({}, instance)` | yes |
| **`JSON.parse('{"x":1,"y":2}')`** — external data executes class methods | yes |
| subclass flattened field list | yes |
| reverse key order (`y` then `x`) | **no** — dedup is by *ordered* list |
| prelude classes (`{message:"x"}` vs `Error`) | **no** — error classes use a separate registry |
| ES5 fn-constructor (`function C(){this.x=1}`) | **no** — `ctorfn.rs` lifts it to its own class |
| `Object.create(C.prototype)` | correct today, different mechanism |

Class-vs-class collision does **not** occur: two classes with identical field
sequences dispatch correctly, because `intern_class_shape` always mints a fresh
id.

**Note.** `Object.getPrototypeOf(lit) === Object.getPrototypeOf(new Point(1,2))`
is already `false` on RTS, matching bun — a correct identity oracle exists, it is
simply not consulted by `instanceof`/`vdispatch`.

### D2 — Array holes do not exist

```ts
const a = [1, 2, 3]; a[6] = 7;
a[4]              // RTS: 0                       bun: undefined
4 in a            // RTS: true                    bun: false
JSON.stringify(a) // RTS: [1,2,3,0,0,0,7]         bun: [1,2,3,null,null,null,7]
Object.keys(a)    // RTS: ["0".."6"]              bun: ["0","1","2","6"]
```

`vec_set_by_payload` (`crates/rts-engine/src/heap/payload_ops.rs:97-107`) grows
with `resize(i + 1, 0)` instead of the `PolyValue::hole()` sentinel that
`emit_hole_to_undef` (`value/emit_marshal.rs:327-338`) expects on the read side.

### D3 — `Object.keys` reads a stale compile-time key list

```ts
const obj = { a: 1, b: 2 };
function poison(o: any, k: string) { o[k] = 1; }
poison(obj, "c");
Object.keys(obj)     // RTS: ["a","b"]            bun: ["a","b","c"]
JSON.stringify(obj)  // RTS: {"a":1,"b":2,"c":1}  (correct)
```

Scope is **narrow**: 4 call sites in
`crates/rts-codegen-new/src/front/run/objstatic.rs`
(`Object.keys` / `values` / `entries` / `getOwnPropertyNames`), which bake the key
list when the receiver's shape is a compiler-proven local. The ~15 runtime
consumers of `global_shape_keys` all resolve the live shape id from slot 0 and are
**correct**. The trigger is a proven-shape local escaping as an argument to a
function that mutates it; intra-function mutation (even with computed keys) is
tracked correctly.

### D4 — `Object.setPrototypeOf` is a no-op

```ts
Object.setPrototypeOf(a, B.prototype);
a instanceof B   // RTS: false   bun: true
a instanceof A   // RTS: true    bun: false
```

`instanceof` reads a baked shape-id immediate, never the mutable per-instance
prototype link.

### D5 — `Entry::Rtse.props` is not traced by the GC (structural, NOT reproduced)

`Traceable::trace_children` (`crates/rts-engine/src/heap/handles.rs:798-856`) has
arms for `Function | Map | Vec | Proxy | GenState | Backend` and then `_ => {}`.
`Entry::Rtse { props: Box<IndexMap<String, i64>> }` (`handles.rs:477`) falls into
the catch-all, so JS properties set on an rtse-class instance that hold handles
are never traced.

**Honesty note:** an attempt to observe corruption (6M allocations across isolated
frames, with a `WeakRef` control proving the GC ran) **did not** reproduce it. The
gap is confirmed by reading the match; the behaviour is not confirmed. Report it
as a gap, not as a demonstrated bug.

---

## 3. Implementation plan

### P1 — Fix D1 (class identity). Ships first; it is a soundness defect.

**Change 1 — one call site, no format change.** In `intern_global_shape`
(`crates/rts-engine/src/heap/shapes.rs:46-55`), reject a class-owned id on a
`by_keys` hit:

```rust
if let Some(&id) = reg.by_keys.get(keys) {
    // A CLASS id must never be reachable by content: nominal identity does not
    // dedup by key sequence. Treat it as a miss and mint an id of our own.
    if !class_shapes().lock().map(|t| t.by_id.contains_key(&id)).unwrap_or(false) {
        return id;
    }
}
```

Checking on **read** rather than on write is what makes this one change
sufficient. It subsumes both the `intern_class_shape` insert (`shapes.rs:65`) and
`seed_global_shapes`'s `by_keys` rebuild (`shapes.rs:210-215`), which republishes
class ids after seeding. `ClassShapes { by_name, by_id }` (`shapes.rs:115-118`)
already round-trips through `export_class_shapes`/`seed_class_shapes`, the AOT
seed blob, and the `bake.rs` manifest — so **no new per-row flag, no blob format
change, no manifest change** is required.

Lock order `registry()` → `class_shapes()` matches the order already established
by `reset_global_shapes` (`shapes.rs:71-84`); no deadlock risk.

Cost: one extra lock on the intern **hit** path only. Not hot —
`alloc_shaped_object_with_id` uses a per-call-site cached id; interning fires once
per distinct static shape.

**Change 2 — bump two cache versions, same commit.** Both caches persist in the
temp dir with no TTL and survive `cargo build`, and both replay **baked machine
code** whose shape-id immediates were produced by the pre-fix engine:

- `crates/rts-codegen-new/src/front/run/progcache.rs:28` — `CACHE_VERSION`. Key is
  `version + program source + prelude`, with no engine hash. AOT reuses this cache
  (`module_entry.rs:114-124`), so a stale hit reproduces the defect in a shipped
  binary.
- `crates/rts-codegen-new/src/front/run/prelude_cache.rs:27` — `CACHE_VERSION`.
  On by default (`RTS_NO_PRELUDE_CACHE` disables).

Without these bumps the fix silently does not apply on any machine that ran the
pre-fix binary once.

**Change 3 — regression tests (currently absent in both directions).** The 15
relevant existing test files pass 76/76 on `main` and none of them assert the
buggy sharing, so regression risk is low — but none assert the *correct*
behaviour either. Add a test asserting `instanceof === false` for: a plain object
built by dynamic adds, an object literal, `{...instance}`,
`Object.assign({}, instance)`, and `JSON.parse` output — each against a class with
the same ordered field list.

**Gate before commit:** `bash scripts/read_before_commit.sh`, plus
`tests/instanceof_operator`, `instanceof_ctor_fn`, `instanceof_cast_rhs`,
`instanceof_globals`, `instanceof_in_callback`, `structured_clone`,
`structured_clone_deep`, `structuredclone_type_preserve`, `claude-pickle`,
`claude-pickle-golden`, `object_create_basic`, `class_proto_hoist_gate`,
`registry_instance_proto`, `object_assign_field_types`, `spread_obj_field_types`.

### P2 — Fix D2 (array holes)

`payload_ops.rs:97-107` must grow with the `hole()` sentinel, not `0`. This is a
prerequisite for any future dense/sparse array work: "degrade to sparse when a
hole is created" is meaningless while hole creation does not produce a hole.
Gate on array tests plus `JSON.stringify`/`Object.keys`/`in` behaviour.

### P3 — Fix D3 (`Object.keys` self-validating fast path)

At the 4 `objstatic.rs` call sites, emit a comparison of the receiver's live
slot-0 shape id against the shape id the compile-time key list was baked from —
one load plus one `icmp` — and fall through to the existing
`dynamic_obj_enum`/`__rtsadp_obj_keys` trampoline on mismatch. No interprocedural
escape analysis needed.

### P4 — Inline caches (`PropIcCell`)

This is the largest *measured, broadly-applicable* win: the 33× gap between an
untyped and a nominally-typed read of the same field, hit by every `any` param and
every `.map(q => q.x)` callback.

Design (already specified in `docs/specs/rts-codegen-new-design.md` §8.2/§8.3,
never built): one writable data cell per dynamic-access call site holding
`(shape_id, slot)`. Emitted code compares the receiver's slot-0 word against the
cell's `shape`; on hit it does a constant-offset load, on miss it calls the
existing slow path and stores the resolved pair back.

AOT-safe by construction — it is *data*, not self-modifying code. Mutable data
cells already have precedent in the tree: `module_aot.rs:96-146` and
`aot_str.rs:96-105` declare and reference module data;
`cranelift_module::Module::declare_data(.., writable: true, ..)` is already
available on the shared `FnCtx.module: &mut dyn Module`.

Estimated ~150 LOC in a new `crates/rts-codegen-new/src/ic.rs` plus call-site
wiring.

**Do not build shapes work ahead of this.** Boa's own PR #2723 measured shapes
alone at 28.1 → 33.6 and only reached 42.9 once inline caching landed in the same
release; shapes without ICs is a bad trade.

### P5 — `Entry::Array` discriminant

Give arrays their own `Entry` variant, byte-identical to `Entry::Vec` (same `i64`
slots, same `vec_get_by_payload`/`vec_set_by_payload`), differing only in the enum
discriminant. This deletes the array-vs-object heuristic documented at
`shapes.rs:37-42` (which today distinguishes them by checking whether
`global_shape_keys(slot0)` length matches) and removes the reason
`GLOBAL_SHAPE_BASE = 0x4000_0000` exists.

Blast radius: the `Entry` enum plus the 3-4 discrimination sites in `shapes.rs`.
**Zero** codegen call sites change; the ABI is untouched.

Add an exhaustiveness guard on `Traceable::trace_children` in the same change
(replace the `_ => {}` catch-all, or add a test asserting every handle-bearing
variant has a trace arm) — D5 exists precisely because that catch-all silently
swallows a new variant.

### P6 — Allocation front (own project, scope before coding)

Every `alloc_entry` (`crates/rts-engine/src/heap/handles.rs:1297`) takes a shard
`Mutex`, bumps the contended global `LIVE_HANDLES` atomic, and may `Vec::push`
(reallocating). A thread-local bump/free-list front would attack the measured 71×.

**Decide before writing code:** handle-indexed arena vs raw-pointer arena. The
raw-pointer variant requires a second root-recognition class in the conservative
stack scanner and breaks the "handle indirection ⇒ moving is ≈ free" invariant.

### P7 — Escape analysis

The real answer to the 71× (it is what bun does). No infrastructure exists: grep
for `escape`/`stack_alloc`/`non_escaping` in `crates/rts-codegen-new/` finds
nothing relevant. Needs its own scoping pass; do not commit LOC estimates yet.

---

## 4. Designs that were REFUTED — do not re-propose without reading this

| Design | Why it fails |
|---|---|
| Boa-style transition tree | 5-10× oversized for the measured defect. |
| Compile-time dictionary hint at the allocation site | The engine has no alias/points-to analysis; the mutation site is never the allocation site in the real cases. Best case it collapses to a runtime bit checked on every access — i.e. the runtime trigger it claimed to replace. |
| Two shape-id bands for AOT | Already exists (`export_seed_blob`/`seed_from_blob`, runtime ids mint above the seeded range). |
| Converting a live object `Entry::Vec` → `Entry::Map` in place | **Unsound.** `payload_ops.rs:78-87` returns `0` and silently **drops writes** on an Entry-kind mismatch, so every already-compiled constant-slot access to that object breaks silently. `const o = {a:1}; …; o.a` does bake a constant slot. |
| Overflow dictionary + shape frozen at K | Refuted from three angles. `pickle/encode.rs:296-328` validates completeness with `keys.len() + 1 == slots.len()`, which **still passes** with overflow keys — so `structuredClone` silently drops every key past K. ~15 consumers of `global_shape_keys` would each need a merge. A literal with more than K fields would force the front to route *statically known* fields through the dictionary. And it does not remove the O(N) mint cost *below* K, so it is the minimal fix **plus** extra machinery, never less. |
| Lazy-parent rows `{parent, key}` | Turns a contiguous scan into an O(depth) pointer chase under the global mutex on every dynamic read, and makes all ~20 `global_shape_keys` consumers walk *and* clone. Requires a materialisation cache, which recreates the O(N²) it set out to remove — and a derived index over this registry was already tried and reverted for diverging under parallel suite execution (`shapes.rs:383-385`). It would also create two dedup domains (literal vs dynamic), breaking today's "same key sequence ⇒ same id regardless of construction path". |
| Boa-style `UniqueShape` (per-object key list mutated in place) | **Unsound here.** Today `reg.keys[i]` is immutable once minted, so every clone already handed to a reader stays valid forever; in-place mutation makes handed-out snapshots stale — the same hazard class as the reverted index. Ownership is also unenforceable: a shape id is a bare `u32` with no owner or refcount, and `pickle/decode.rs:188-196` deliberately assigns the same class shape id to N freshly allocated objects. |
| Dense type-specialised array storage (`DenseI32`/`DenseF64`) | `PolyValue` already stores an f64 inline and unboxed, so `DenseF64` saves nothing at the ABI. `DenseI32` needs an element-type proof the front-end does not produce: `rts-hir/src/type_refine.rs:84` types **every** array literal as `Array(Any)`, including `const a: number[]`, and `rts ir` confirms `number[]` and `any[]` lower identically. There is **no deopt/guard-failure machinery anywhere in the engine** (grep is empty), so speculative storage cannot be made sound. ~20 codegen files affected. |

### The result nobody refuted

For N sequential key additions, "one shape per key-set" mints N distinct shapes.
If every shape stores a materialised flat key list, memory is Σi = **O(N²)**,
unavoidable. To get O(N) you must not store flat lists per shape, which forces a
parent chain and O(depth) reads. A materialisation cache restores the O(N²).

**No representation of the shape table yields O(N) memory and O(1) reads at the
same time.** The only escape is not to mint N shapes — and every mechanism for
that (in-place conversion, overflow, unique shapes) was refuted above on
soundness grounds. Combined with the measurement that this pattern does not occur
in the repository's own corpus, the correct decision is to **leave the
`globalThis` quadratic alone** and spend the engineering on P4/P6/P7.

### The invariant that governs any future proposal

The current write path is **append-only**, and that is its soundness property: an
already-compiled constant-slot offset `1 + slot` is never invalidated by a later
append. Any design that breaks append-monotonicity corrupts silently, because
`payload_ops.rs:78-87` fails by returning `0` and dropping writes rather than by
trapping.

---

## 5. Out of scope, recorded

- `structuredClone` of a real instance **revives the class by name**
  (`pickle/decode.rs:188-196`), deliberately. bun strips the prototype per the
  HTML structured-clone algorithm. This is an intentional RTS divergence,
  unrelated to D1, but it is a divergence.
- Fixing D4 properly means `instanceof` consulting the prototype chain instead of
  a baked immediate — that trades an `icmp` against a constant for a load plus a
  walk. It is the change that retires the "one integer, two meanings" defect for
  good, but it is a dispatch redesign, not a bug fix, and must not be bundled with
  P1.

---

## 6. External engines read (cloned, not summarised second-hand)

**Boa** (`boa-dev/boa`). `SharedShape` is `{ forward_transitions, property_count,
previous, property_table }` — it does **not** store a key list per shape; the
whole transition chain shares one `PropertyTable` and each shape records how many
of its entries are its own. Forward transitions are `WeakGc` and pruned every 256
insertions. `TRANSITION_COUNT_MAX = 1024` degrades a shared shape to a
`UniqueShape`. `SlotAttributes` packs inline-cache bits (`PROTOTYPE`, `FOUND`,
`NOT_CACHEABLE`) into the same `u8` as the property attributes.

**Boa's pruning is illegal in RTS.** Nested compiles (`eval` / `new Function` /
dynamic import) require `intern_*` to be idempotent and append-only
(`front/run/mod.rs:236-244`) because the outer program already has `iconst <id>`
baked into compiled code. Boa can prune because it bakes nothing.

**Perry** (`PerryTS/perry`, a native TS → LLVM AOT compiler). It embeds **raw
pointers** in its NaN-box and consequently needs per-platform heap-address
heuristics (`value/addr_class.rs::is_handle_band`, a 2 TB floor on macOS vs
`0x1000` on Linux/Android/iOS); their own source comments tie this to a bug where
`.length` collapsed to 0 on iOS. **This is a strong argument for RTS keeping the
handle-index payload** rather than a raw pointer. Perry also ships a generational
copying GC with `gc/verify.rs` (heap-invariant walker) and `gc/heap_snapshot.rs`,
neither of which RTS has — a `verify.rs` equivalent is cheap and would have caught
the class of bug RTS hit in PR #400.
