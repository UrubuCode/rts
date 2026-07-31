# RTS crate reorganization — `rts-natives` and the end of `__rtsa_`

**Status:** N0–N4 EXECUTED (2026-07-31). N2b (new), N5, N6, N7 open — and each is
bigger than this document originally claimed; **see §8**, which records what
execution measured and this plan got wrong. §2 is left as WRITTEN, not corrected
in place: it is the diagnosis that motivated the work, and §8 says where it was
mistaken. **Owner directive, 2026-07-31:**
*"the engine MANAGES rts, not how it works internally"* — that sentence is the
whole partition rule, and everything below is its consequence.

Every number here was **measured**, not estimated. Re-measure before trusting
any of them — including the ones execution has since corrected.

---

## 1. The rule

> **`rts-engine` manages. `rts-natives` is how it works inside.**

An item belongs in `rts-natives` when EITHER clause holds:

- **(a) The Cranelift IR cannot express it**, so generated code has to call out:
  coroutines, exceptions, a garbage collector, a mutable shared cell, a trace
  stack.
- **(b) It IS the runtime value representation, or it has to know that
  representation from the inside**: the heap, `Entry`, the HandleTable, hidden
  classes, the NaN-box — and anything that pattern-matches `Entry` exhaustively.

It belongs in `rts-engine` when it **decides dispatch**: the Registry, the
builder, the member/spec vocabulary.

Clause (b) is what places the heap, `shapes/` and `poly.rs`: hidden classes and
`PolyValue` are the *form* a value has at runtime, which is exactly "how it
works inside". Forcing them under clause (a) would be a stretch — the IR can
express a struct load; what it cannot express is which struct.

---

## 2. What is measured today

### 2.1 `rts-engine` is two crates in a trenchcoat

```
heap/        6186   HandleTable, shapes, poly, string_pool, pickle, fixed
collector/    935   HALF of the GC
builder/      914   Registry builder
registry.rs   331   Registry
member/sig    208   member + signature vocabulary
loop_sources   103
runtime_ci     139
gc_surface.rs  74   re-export + extern blocks (0 own functions)
```

~7.1k lines of value machinery against ~1.45k of dispatch vocabulary.

### 2.2 A third of the machinery escaped upward into the BACKEND

`rts-std` is the backend crate (io / net / tokio), the top of the stack. It
currently holds:

```
rts-std/src/collector/generator.rs    1210   generator + async state machines
rts-std/src/collector/string_pool.rs   376
rts-std/src/collector/error.rs         245   the error slot (unwind)
rts-std/src/collector/collector.rs     206   the OTHER half of the GC
rts-std/src/collector/gcells.rs        165   mutable closure cells
rts-std/src/collector/stack.rs          73   trace stack
                                      ----
                                      2275
```

None of that is backend. Coroutine state machines are not io/net/tokio.

### 2.3 The symptoms this produces — all of them cicatrices of the same wound

**A function pointer installed at startup so the engine can call upward.**
`rts-std/src/collector/collector.rs:6` says it outright:

> `finish_cycle` via the `GC_COLLECT_HOOK` (installed by `runtime_init` at
> startup — *the engine can't name `rts-std`'s `finish_cycle` directly*)

So one GC cycle crosses the crate boundary **twice**: engine fires the hook →
`finish_cycle` (std) → `scan_all_roots` (engine again).

**Four `gc_surface.rs` files with zero functions of their own:**

| file | lines | `pub use` | `extern` blocks | own fns |
|---|---:|---:|---:|---:|
| `rts-engine/src/gc_surface.rs` | 74 | 2 | 2 | **0** |
| `rts-std/src/gc_surface.rs` | 47 | 3 | 1 | **0** |
| `rts-primitives/src/gc_surface.rs` | 6 | 1 | 0 | **0** |
| `rts-shared/src/gc_surface.rs` | 6 | 1 | 0 | **0** |

`rts-engine/src/gc_surface.rs` declares `extern "C"` blocks for symbols that
live **above** it, in `rts-std`. That is an inverted dependency wearing a
forward-declaration as a disguise.

### 2.4 `__rtsa_` is an empty drawer

```
__rtsa_     0 symbols
__rtsn_    63 symbols
__rtsadp_ 1035 symbols   (value model, bare/Verbatim form — uses no scope)
```

`Scope::Abi` exists in `rts_abi::scope` and names **zero rows of the baked
table**. Its only users in source were the 8 `ta_ctor!` TypedArray constructors,
which the baker cannot see at all (they are emitted from a `macro_rules!` body —
see `docs/specs/no-mangle-drain.md` §1); they are now `native`. It also
collides conceptually with the crate named `rts-abi`, which holds the *contract*
(`AbiType`, `SymbolDesc`, the naming rule) and not a single `extern "C"`
function. Two names, opposite meanings, same repo.

The 63 `__rtsn_` are, by category: coroutine state machines 35, exception slot 5,
vec-by-payload 5, iteration protocol 4, GC 3, trace 3, formatting 3,
PolyValue↔handle 2, gcell 2, event loop 1. Every one of them is "the IR cannot
express this". There is no line separating them from the ~50 `__RTS_FN_RT_*`
still to be converted — `invoke_auto` and `gen_sm_next` are the same kind of
thing.

### 2.5 The precise GC scan is documented as working and is NOT

`CLAUDE.md` and `.claude/rules/02-runtime.md` both describe the GC as *"precise
mark+sweep using Cranelift `UserStackMap`"*. Measured:

```
declare_value_needs_stack_map   4 occurrences, ALL of them comments
UserStackMap extracted by       rts-codegen-new/{module_jit,parcompile}.rs
rts-natives/collector/stack_map_registry.rs  only RECEIVES the PCs (and
                                            `lookup` has ZERO callers)
```

The transport exists; nothing feeds it. The scan is conservative. **This is a
defect to fix, not a reason to leave the GC where it is** — the GC is supposed
to talk to Cranelift on both ends, and having it split across a crate boundary
is part of why it never got wired.

---

## 3. Target

```
rts-abi        the CONTRACT. Zero dependencies, bottom of the graph.
               AbiType / SymbolDesc / scope.rs (the naming rule) / tymap / table.
               STAYS. rts-macro (proc-macro) and rts-symbol-baker (binary) both
               derive the same symbol from it; removing it forces the baker to
               reimplement symbol_for, which is the exact drift the single
               source of truth exists to kill.
   ↑
rts-natives    HOW IT WORKS INSIDE — the extent of Cranelift.
               heap + HandleTable            6186   ✅ N1
               GC, unified + stack maps       935 + 206   ✅ N2
               generator STATE MACHINE       ~900   ✅ N3 (not 1210 — the async
                                                    DRIVER stayed in rts-std)
               error slot / unwind             245   ✅ N2
               gcells                          165   ✅ N2
               trace/depth guard                73   ✅ N2
               shapes, poly (NaN-box)         (inside heap/)
               → every __rtsn_ symbol lives here

               NOT here, against this plan's original inventory:
               string pool                     376   stays in rts-std — it asks
                                                    the CLASS layer what a handle
                                                    is (see §8)
               async driver (async_sm_*/agen_*) ~310  stays in rts-std — timers,
                                                    tokio, microtasks: scheduling
   ↑
rts-engine     MANAGES — decides dispatch.
               Registry + builder + member + sig + loop_sources   ~1.45k
   ↑
rts-primitives → rts-shared → rts-std (real backend: io/net/tokio)
   ↑
rts-runtime    facade + adapters (value model, __rtsadp_*)
```

Current dependency graph, for reference:

```
rts-abi          -> (nothing)
rts-engine       -> rts-abi, rtse
rts-primitives   -> rts-engine, rtse
rts-shared       -> rts-engine, rts-primitives, rtse
rts-std          -> rts-engine, rts-primitives, rts-shared, rtse
rts-runtime      -> all of the above + node/napi/dom/render/input/egui
rts-codegen-new  -> rts-parser, rts-hir, rts-ast, rts-runtime, rts-engine, rtse
rts-macro        -> rts-abi
rts-symbol-baker -> rts-abi
```

`rts-natives` slots between `rts-abi` and `rts-engine`. `rts-engine` then depends
on `rts-natives` (the Registry stores handles), which is the direction it already
wants — today it fakes it with `extern` blocks.

### Naming

**`__rtsn_` becomes the only snake_case engine prefix. `Scope::Abi` and the
`__rtsa_` spelling are deleted from `rts_abi::scope`.** One drawer, one rule, no
per-site judgement. `__rtsadp_*` (value model) is unaffected — it uses the bare
`Verbatim` form.

Everything else keeps the case-preserving rule landed in `98d8d385`:
`__rtsm_<module>_<value>` / `__rtsm_global_<Class>_<value>`, every segment
verbatim.

---

## 4. What this deletes

| thing | why it existed |
|---|---|
| `Scope::Abi` + `__rtsa_` | a second drawer for what `__rtsn_` already is |
| 4× `gc_surface.rs` | the engine reaching for what escaped into the backend |
| `extern "C"` blocks in `rts-engine/src/gc_surface.rs` | inverted dependency in disguise |
| `GC_COLLECT_HOOK` | a fn pointer to call upward across a crate boundary |
| the engine↔std round trip per GC cycle | the split itself |

No compatibility shims, no deprecated aliases: the project is pre-production
(owner directive), so every move is a hard cutover.

---

## 5. Phases

Each phase must end green — `cargo check --workspace`, the baker's `--check`,
and the TS suite showing the same failing SET as the measured baseline.

**N0 — create `rts-natives`, empty, wired into the graph. ✅ DONE (2026-07-31).**
Crate + `Cargo.toml` + workspace member + a row in the baker's `SCANNED_CRATES`.

**N1 — move the heap. ✅ DONE (2026-07-31).** `rts-engine/src/heap/*` →
`rts-natives`, and `collector/*` + `numfmt.rs` went WITH it rather than waiting
for N2: they are mutually coupled (`heap` calls `crate::Traceable` and
`collector::debug`), so splitting them across the phase boundary would not have
compiled. Purely mechanical otherwise — measured, the dispatch half
(builder/member/registry/sig) referenced them in exactly ONE line, the `pub use`
in `lib.rs` — and `rts-engine` re-exports so nothing above notices.

What left with them, unplanned and worth recording: `regex`, `fancy-regex`,
`serde_json`, `sha2`, `rustls`, `indexmap`. Every use of those in the old
`rts-engine` was inside `heap/` or `collector/`; they were `Entry`-payload
dependencies. A crate that only decides dispatch no longer links a TLS stack.

**N2 — unify the GC. ✅ DONE (2026-07-31), with two corrections.**
`rts-std/src/collector/{collector→cycle,gcells,stack,error}.rs` landed in
`rts-natives`. **`GC_COLLECT_HOOK` is dead** — with both halves in one crate
`alloc_entry` calls `finish_cycle` directly; what survives is a plain
`GC_ARMED: AtomicBool`, because before `runtime_init` the process may still be
COMPILING and the codegen holds interned handles in Rust collections rather than
as words on the scanned stack.

Corrections, both found by the move failing to compile:

- **`string_pool.rs` did not move.** It reads and formats `Entry` values and
  spreads iterables, and to do that it asks the CLASS layer above
  (`rts_shared::collections::map::{handle_is_map_kind, handle_is_set_kind,
  MAP_VALUES}`) what a handle is. That fails both clauses of §1 — it is not
  machinery, so it stays in `rts-std`. §2.2's inventory counted it as escaped
  machinery; it is not.
- **`finish_cycle` could not take the microtask roots with it.** The microtask
  queue lives in `rts-std/src/globals/text_encoding/instance.rs` — a 928-line
  file that has nothing to do with text encoding — and the cycle hardcoded a call
  into it. That call is now `collector::root_sources`, a registry any layer uses
  to contribute handles it holds in its own Rust containers. **This is not the
  hook wearing a new name.** `GC_COLLECT_HOOK` let the collector call *upward to
  run the cycle*, an inversion that existed only because the halves were split.
  Root contribution is the opposite direction of knowledge: the collector owns
  the cycle and always will, but it cannot own the exhaustive list of every
  container in every layer without depending on all of them.

Two other things a low layer legitimately needs from above are now explicit
rather than hidden: `error.rs` takes an installed `fn() -> String` for the
`Error.prototype.stack` text (the frames are pushed by `rts-shared`'s `trace`),
and `stack.rs` link-resolves `__RTS_FN_GL_RANGE_ERROR_NEW` for "Maximum call
stack size exceeded". Deleting the four `gc_surface.rs` is NOT part of this phase
— see N2b.

**N2b — delete the four `gc_surface.rs`.** Measured: **~40 consumers across 8
crates** (`rts-node` 14, `rts-primitives` 5, `rts-shared` 4, `rts-egui`,
`rts-dom`, `rts-input`, `rts-napi`, `rts-runtime`). Most of what it declares is
now a REAL re-export of `rts-natives` code, not an inverted extern; what is left
inverted after N3 is four symbols owned by `rts-primitives`/`rts-std`
(`__RTS_FN_RT_INVOKE_AUTO`, `__RTS_FN_GL_FUNCTION_CALL`,
`__RTS_FN_GL_PROMISE_RESOLVE`/`_REJECT`). Its own phase so a breakage in a
40-site sweep has one candidate cause.

**N3 — SPLIT the state machines, not move them.** The plan called
`rts-std/src/collector/generator.rs` (1210 lines, 38 `__rtsn_`) a relocation. It
is not: measured, it has **21 call sites into five `rts-std` modules** —
`promise_slot`, `globals::timers`, the microtask queue, `promise`, and
`runtime::async_rt`. The file holds two things at once:

- lines 1–618, the **sync generator + lazy state machine** (`iter_*`,
  `generator_*`, `gen_sm_*`) — machinery by clause (a), and its only upward
  reference is the async-generator branch of `gen_sm_drain`;
- lines 619–end, the **async driver** (`async_sm_*`, `agen_*`) — tokio guards,
  timer polling, microtask enqueue. That is backend, and it belongs where it is.

So N3 is: sync half down, async driver stays, and `gen_sm_drain`'s
`is_async_gen` branch becomes a registered delegate in the same shape as
`root_sources`.

**N4 — delete `Scope::Abi`. ✅ DONE (2026-07-31).** The variant, the `abi`
attribute argument and the `__rtsa_` branch of `symbol_for` are gone; the 8 real
users (`ta_ctor!` in `adapters/value/taops.rs`) now declare `native` and emit
`__rtsn_ta_new_*`; the 50 mapped rows in `docs/specs/symbol-rename-map/` were
re-pointed at `native`/`__rtsn_` (primitives_std 22, input_render_engine 15,
math_buffer 9, shared_b 3, node_rest 1 — re-verified: no duplicates in the map,
no collision against the baked table). Every doc, doc-comment and unit test that
described `__rtsa_` as a valid prefix was updated in the same pass;
`validate_symbol("__rtsa_…")` is now `SymbolError::MissingPrefix`. Re-bake so the
generated header stops naming `__rtsa_` as an example scope.

**N5 — the rename** (mapped in `docs/specs/symbol-rename-map/` and
`docs/specs/no-mangle-drain.md`): `__RTS_FN_*` → `__rtsm_`/`__rtsn_`. **Run it
AFTER N7**, not before or after at will — see N7 for why the two collide.

Scope, re-measured 2026-07-31 (the map is 626 rows against **943** baked
`__RTS_FN_*`):

- **317 symbols are unmapped, not the 236 this plan implied.** The two gaps §5
  named are exact — `collections` 125, `rts-node` net+dgram 111 — but there are
  three more nobody listed: **`NS_EGUI` 45**, **`NS_ENGINE` 13**, **`NS_GPU` 12**,
  plus 20 strays (`RT_NAPI`, `NS_GC_STRING_*`, `RT_MAP_*`, `RT_PROXY_RESOLVE`,
  `RT_FOR_OF_NORMALIZE`, `GL_FETCH_RESPONSE_OK`, `GL_ARRAY_FROM_VEC`). The egui
  ones are blocked anyway by the MANDATORY egui-plan rule in `CLAUDE.md`.
- The surface a rename must sweep, per symbol: the Rust item name *is* the symbol
  (`pub fn __RTS_FN_…`), so renaming it renames a Rust item and its whole `use`
  graph; **1431** bare `"__RTS_…"` registration string literals; 16
  `Member { symbol: … }` rows; `declare_function`/`call_runtime` literals in
  `rts-codegen-new` (**unchecked at compile time — these fail at RUNTIME**); 30
  files with hand-written `extern "C"` blocks; the `abi_sig.rs` rows.
- **Out of scope, and a regex will eat them:** the `__RTS_GEN_SM_*` /
  `__RTS_AGEN_*` / `__RTS_ASYNC_SM_*` family (~130 occurrences in
  `rts-parser/src/generator_sm.rs` and `rts-codegen-new`) are codegen-internal
  JIT function names, not runtime symbols. They are not in the baked table.

Partition for parallel execution, by DEFINING file: **A** collections (125) ·
**B** `rts-shared` rest (275) · **C** `rts-node` (167) · **D** UI/host —
dom/egui/input/render (186, egui blocked) · **E** primitives+std (185) ·
**F** engine core (~40). Files that straddle areas and must have exactly ONE
owner: the generated `symbol_table.rs` (nobody edits it — regenerate once, at the
end), the whole `rts-runtime/src/adapters/value/` tree (zero definitions, pure
consumer, dominated by collections → give it to A), the multi-family codegen
literals in `rts-codegen-new/src/front/run/` (one "codegen sweeper" owner), and
the `gc_surface.rs` seam (→ F).

**N6 — wire the precise scan.** Call `declare_value_needs_stack_map` for
`Repr::Ref(_)` / `Repr::Tagged` values, consume the registered PCs, and make the
scanner precise. **Fix `CLAUDE.md` and `.claude/rules/02-runtime.md`, which
currently claim this already works.** Note the scratch-module hazard first
(`docs/specs/no-mangle-drain.md` §7): `PENDING`/`REGISTRY` are process-global
while `bake.rs::capture_compiled` populates a separate `JITModule` with its own
`FuncId` numbering.

**N7 — audit the dead. AUDITED, not yet deleted; full list in
`docs/specs/dead-symbols-n7.md`.** Measured **173**, not ~150. The families named
above check out — `ATOMICS_*` 4/4, `JSON_STRINGIFY_*` 4 of 6, `THIS_GET`,
`STRING_FREE`, `SET_UNION` — **except the collections number, which is wrong in a
way that breaks the build**: 119 `NS_COLLECTIONS_*` exist, **75 are dead and 44
are live**, so deleting 104 is roughly 29 link errors.

Three things that audit established and this plan did not account for:

- **Never delete by prefix.** `VEC_TO_SPLICED` is dead but `VEC_TO_SPLICED_AUTO`
  is live; `VEC_SPLICE_REMOVE`/`_INSERT` are dead but `VEC_SPLICE_AUTO` is live.
  `grep VEC_TO_SPLICED` returns 7 hits for a symbol that has 2 — it is matching
  the live siblings.
- **370 symbols have no Rust caller and must not be deleted**: they are Registry
  members, reachable from TS by their JS name. A call-graph audit calls all of
  them dead. (All 926 `__rtsm_*` are in this class by construction.)
- **N7 must run BEFORE N5**, because 71 of the 173 corpses have rows in the
  rename map. Those rows do not merely need dropping: as written they would turn
  an unreachable symbol into an `__rtsm_global_*` registry member — N5 would
  *resurrect* the `Error`/`Reflect`/fetch families as TS surface rather than
  delete them. Decide per family before either campaign runs.

One corpse is a missing feature, not debt: `__rtsn_stack_push`/`_pop` are the
recursion-depth guard the codegen was supposed to emit at every non-tail user
function. Dead means **the codegen stopped emitting them**, and `stack.rs`'s
`RangeError` path goes with them. Confirm before deleting.

---

## 6. Verification protocol

Same discipline as `docs/specs/no-mangle-drain.md` §5, which caught three
rename traps in this campaign already:

1. **Bake the symbol table before and after; diff the NAME SETS.** A pure move
   must show zero added and zero removed. A pure re-spelling must pair
   one-to-one with zero orphans — that bijection is the proof, and reading the
   diff is not a substitute for it.
2. **Grep every old spelling across `*.rs` AND `*.ts`** before declaring a
   rename done. A `.ts` prelude, a Registry `instanceof_predicate("…")` string,
   a hand-written `extern "C"` block and a codegen `declare_function("…")` are
   all consumers that fail at LINK or at RUNTIME, never at compile time.
3. **Any "pre-existing failure" claim must be produced by running the failing
   thing on a stashed, rebuilt clean tree** — never inferred from a commit
   message.
4. Baseline as of 2026-07-31: `target/release/rts.exe test` → **772/775 files,
   2841/2853 tests**; failing set = `claude-dom-script-globals` (1),
   `claude-object-statics-como-valor` (crash),
   `claude-stringify-wrapper-objects` (10).
5. `cargo test --workspace --lib` **does not link** — every crate below
   `rts-runtime` references symbols whose bodies live above it, resolved only in
   the final link (`rts-shared` 36 unresolved `__rtsadp_*`, `rts-primitives` 35,
   `rts-napi` 36, and **`rts-natives` 2**: `__RTS_FN_GL_FUNCTION_CALL` and
   `__RTS_FN_GL_RANGE_ERROR_NEW`, the two link-resolved externs N2/N3 added by
   design). Running per crate does NOT avoid this — the failure is at LINK, not
   at compile, so `cargo test -p <crate> --lib` fails too for exactly these
   crates. Do not read it as a regression; `cargo check` is the compile signal
   and the TS suite is the behaviour signal.

---

## 7. `heap/pickle/` — resolved: `rts-natives`, by clause (b)

This was the one member the "what Cranelift cannot do" test did not settle, so it
was measured separately.

```
1023 lines (encode 385 / decode 389 / mod 249)
names 20+ of the 75 Entry variants — including BACKEND ones:
  TcpStream, TcpListener, TlsClient, UdpSocket, SyncMutex, SyncRwLock, SyncOnce
consumers in 5 crates:
  rts-codegen-new/module_jit   reset_program_fns, register_program_fn
  rts-node/v8/symbols          serialize_value / deserialize_value
  rts-runtime/adapters/errslot set_class_revive_hook
  rts-shared/serde_ns          register_ext_codec, serialize/deserialize
  rts-std/globals/storage      localStorage
```

By clause (a) pickle does **not** qualify: serialization is a feature, not an IR
gap. By clause (b) it qualifies outright — **it is the single piece of code in
the repo that knows `Entry` most exhaustively.**

Placing it above `rts-natives` would make all 75 `Entry` variants public surface
and put a 75-arm `match` on the far side of a crate boundary — a permanent drift
point, which is the exact failure class this whole campaign deletes. `rts-shared`
is the worst option: it removes a hook or two and inherits the remote match.

**Separate debt, not fixed by the move:** pickle already carries three
startup-installed function pointers (`set_class_revive_hook`,
`register_ext_codec`, `register_program_fn`) — the same inversion pattern as
`GC_COLLECT_HOOK`. It needs things that live above it. Choosing a crate does not
address that; it deserves its own item and should not be folded into this
reorganization.

---

## 8. What execution measured — and what this plan got wrong

Written after N0–N4 landed (2026-07-31). Every entry here was found by the work
failing, not by review, which is the point of recording them: the plan read as
confident in exactly the places it was wrong.

### The partition rule held. The inventory did not.

§1's two clauses decided every case cleanly, including the ones that had to be
overruled. What repeatedly failed was §2.2's list of "machinery that escaped
upward into the backend" — it was assembled by looking at where files SIT, not at
what they CALL.

| plan said | measured |
|---|---|
| `string_pool.rs` (376) is escaped machinery | It calls `rts_shared::collections::map::{handle_is_map_kind, handle_is_set_kind, MAP_VALUES}`. A helper that has to ask the CLASS layer what a handle is fails both clauses. **Stays in `rts-std`.** |
| `generator.rs` (1210) is a move | 21 call sites into five `rts-std` modules. It is a state machine AND its scheduler in one file. **Split**, ~900 down and ~310 staying. |
| `__rtsa_` names 0 symbols, `Scope::Abi` "has never been used" | 0 rows of the BAKED TABLE, but **8 real users** — `ta_ctor!` in `taops.rs`, invisible to the baker because a source scanner cannot see through `macro_rules!`. "Never used" would have led a reader to re-point nothing. |
| N7: ~150 dead, "~104 of `collections`" | **173** dead; collections is **75 of 119**, not 104. Deleting 104 is ~29 link errors. |
| N5: two areas unmapped (collections 125, node net 111) | **317** unmapped. Three more areas nobody listed: egui 45, engine 13, gpu 12, plus 20 strays. |
| N2 deletes the four `gc_surface.rs` | ~40 consumers across 8 crates, and most of what it declares is now a REAL re-export rather than an inversion. Own phase (N2b). |

### Three things the rule could not decide on its own

Each is a low layer genuinely needing something from above. None is the
`GC_COLLECT_HOOK` pattern, and the difference is worth stating because it is the
one judgement this reorganization keeps having to make:

1. **Off-stack GC roots** (`collector::root_sources`). The collector owns the
   cycle and always will; it cannot own the list of every container in every
   layer that might hold a handle. Contribution by registration is what a correct
   GC needs, not a workaround for a bad partition.
2. **The async-generator driver** (`generator::AgenDriver`). A state machine is a
   data structure with a `step`; deciding WHEN to step it is scheduling, and
   scheduling is backend.
3. **Constructing a JS value of a class defined above** — `RangeError` for stack
   overflow, `Function.prototype.call` for a user iterator's own `next`. These
   are link-resolved `extern "C"` declarations, the same mechanism `gc_surface`
   uses, and they are legitimate: the layer below owns the mechanism, the class
   above owns the value.

The test that separates these from the deleted hook: **which direction does the
KNOWLEDGE flow?** `GC_COLLECT_HOOK` had the lower layer asking the upper one to
run the lower one's own algorithm. All three above have the upper layer supplying
something only it can know.

### Verification notes for whoever runs N5/N6/N7

- The NAME-SET bijection (§6.1) caught nothing across N0–N4 because all four were
  pure moves — 2191 → 2191 every time. That is the point: it is cheap and it is
  the only proof that a "move" was a move. Run it anyway.
- A mechanical `sed` rename broke two `rts-abi/src/table.rs` test fixtures by
  making an ascending symbol list non-ascending (`__rtsn_` sorts after
  `__rtsm_`), which would have failed the contiguous-range invariant. `cargo test
  -p <crate> --lib` catches it; `cargo check` does not.
- The full TS suite ran clean against the measured baseline (772/775 files,
  2841/2853 tests) after N2 and again after N3, with the failing SET re-run
  individually rather than inferred from the counts.
