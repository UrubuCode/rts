# RTS crate reorganization — `rts-natives` and the end of `__rtsa_`

**Status:** plan, not yet executed. **Owner directive, 2026-07-31:** *"the engine
MANAGES rts, not how it works internally"* — that sentence is the whole
partition rule, and everything below is its consequence.

Every number here was **measured**, not estimated. Re-measure before trusting
any of them.

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

`Scope::Abi` exists in `rts_abi::scope` and has **never been used**. It also
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
rts-engine/collector/stack_map_registry.rs   only RECEIVES the PCs
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
rts-natives    HOW IT WORKS INSIDE — the extend of Cranelift.
               heap + HandleTable            6186
               GC, unified + stack maps       935 + 206
               generator/async state machines 1210
               error slot / unwind             245
               string pool                     376
               gcells                          165
               trace stack                      73
               shapes, poly (NaN-box)         (inside heap/)
               → every __rtsn_ symbol lives here
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

**N0 — create `rts-natives`, empty, wired into the graph.**
Crate + `Cargo.toml` + a place in the workspace. Nothing moves yet.

**N1 — move the heap.** `rts-engine/src/heap/*` → `rts-natives`. Biggest single
move (6186 lines) but purely mechanical: it is a leaf of the engine, and
`rts-engine` re-exports during the move so nothing above notices.

**N2 — unify the GC.** `rts-engine/src/collector/*` and
`rts-std/src/collector/{collector,gcells,stack,error,string_pool}.rs` both land
in `rts-natives`. **Kill `GC_COLLECT_HOOK`** — with both halves in one crate the
call is direct. Delete the four `gc_surface.rs`.

**N3 — move the state machines.** `rts-std/src/collector/generator.rs` (1210,
38 `__rtsn_` symbols) → `rts-natives`.

**N4 — delete `Scope::Abi`.** Remove the variant, the `abi` attribute argument,
and the `__rtsa_` branch from `symbol_for`; re-point the ~50 mapped symbols at
`__rtsn_`. Re-bake.

**N5 — the 868-symbol rename** (the campaign already mapped, in
`docs/specs/no-mangle-drain.md`): `__RTS_FN_*` → `__rtsm_`/`__rtsn_`.
Independent of N0–N4 and can run before or after; keeping it separate keeps the
diagnosis clean if something breaks.

**N6 — wire the precise scan.** Call `declare_value_needs_stack_map` for
`Repr::Ref(_)` / `Repr::Tagged` values, consume the registered PCs, and make the
scanner precise. **Fix `CLAUDE.md` and `.claude/rules/02-runtime.md`, which
currently claim this already works.** Note the scratch-module hazard first
(`docs/specs/no-mangle-drain.md` §7): `PENDING`/`REGISTRY` are process-global
while `bake.rs::capture_compiled` populates a separate `JITModule` with its own
`FuncId` numbering.

**N7 — audit the dead.** ~150 symbols have no consumer at all (all the buffer
`ATOMICS_*`, 4 `JSON_STRINGIFY_*`, ~104 of `collections` such as `SET_UNION` /
`VEC_TO_SPLICED`, `THIS_GET`, `STRING_FREE`). Own campaign, own verification —
deliberately not mixed into a rename, so a breakage has one candidate cause.

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
   `rts-runtime` references `__rtsadp_*` symbols that live above it
   (`rts-shared` 36 unresolved, `rts-primitives` 35, `rts-napi` 36). Run per
   crate; do not read this as a regression.

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
