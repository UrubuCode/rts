# Specifications and Technical Notes

Index of design documents, feature specifications, and architectural
decisions.

**The canonical direction for the engine is
[`rts-codegen-new-design.md`](rts-codegen-new-design.md)** — the ground-up
redesign (PolyValue NaN-box, Repr lattice, shapes + data ICs, single
HIR→Cranelift lowering, data-driven dispatch). Read it before any engine work.
The old engine (`crates/rts-codegen-old/`) and the MIR tier are **DELETED** — the
cutover happened. Docs describing their logic were removed with them.

**Doc hygiene pass, 2026-07-28.** Deleted as obsolete: `main-divergence.md` (a
pre-cutover merge note for an operation already performed), `gpu3d-scene-pass.md`
(the namespace was superseded by `egui.*`), `ui-stack-status.md` (a second,
unindexed UI plan competing with the frozen html-engine roadmap), and the
8-file `node-format/` study (its verdict shipped as `crates/rts-napi` — see
`napi-implementation.md`). `namespace-creation-guide.md` was rewritten from
scratch: the old one taught hand-written `abi.rs` MEMBERS tables under
`src/abi/`, a path that no longer exists and a pattern the single-source-of-truth
rule forbids. Stale test numbers were stripped from `js-parity-epic-226.md`.
**Never quote a remembered parity number — re-measure.**

## Canonical

- [Single source of truth for symbols](rts-macro-single-source.md) — **`rts-macro`
  (ORCHESTRATOR: declares, types, organizes — `SymbolDesc` derived from the Rust
  signature so drift is unrepresentable) + `rts-symbol-baker` (LINKER: bakes ONE
  static, name-ordered table that is both the JIT vtable and the AOT symbol set).**
  Kills `rt.rs`, the hand-declared symbol system and `adapter_symbols/`. Records
  the 2026-07-28 removal of the crate-dependency-direction bans (they were what
  manufactured the 527 rows of mirror tables). Phases F0→F13.
- [Engine redesign (rts-codegen-new)](rts-codegen-new-design.md) — **the
  canonical direction.** PolyValue, Repr lattice, shapes + data ICs, single
  lowering, data-driven dispatch + generated ABI. Migration phases P0→P5.
- [New-engine GC](gc-generational-design.md) — weak phase (`#217`, bounded, on the
  current mark+sweep) now; generational copying (nursery) as the long-term leap,
  deferred until ~90% cross-runtime. The advantage of handle indirection (moving ≈
  free).
- [Plan — new GC + modernized gc API/ABI](gc-new-api-plan.md) — EXECUTION
  PLAN: remove the entire old-engine gc API (manual string pool
  `gc.string_*` etc.), migrate the ~110 legacy fixtures to native strings,
  audit/trim `Entry`, redo the gc ABI PolyValue-native, and prepare the ground
  for the weak phase + the generational GC. Goal: better GC + better API/ABI, zero
  old-engine legacy.

- [Future optimization — closing the gap to native Rust](FUTURE_OPTIMIZATION.md)
  — **Phase 0 LANDED + measured** (`RTS_REPR_STATS=1` → BOX/UNBOX/TAGGED-BINDING/
  RUNTIME-CALL histogram with the engine `file:line`, in-loop split, alloc sites).
  First measurement: `bench/objbench.ts` = 17 extern calls per iteration, 1024×
  native Rust; Monte Carlo = 0 in-loop boxes/calls, faster than Bun. Confirms the
  gap is NOT Cranelift but heap traffic + generic ops on slot loads. Phases 1–6
  (escape analysis, per-slot Repr, Tagged widenings, bump alloc, inlining) are plan.
- [Standard rts:* surface (redesign)](rts-std-surface.md) — **canonical map of the
  new surface**: JS/Web globals + camelCase `rts:<ns>` modules (Rust's std
  exported), what dies/renames/moves, bytes = TypedArrays, comptime
  (`includeBytes`/`rts:build`), `exportC` + `rts compile --lib`, relocation of
  primitives to rts-primitives, phases F0→F8.
- [Engine threading model](rts-threading-model.md) — multithreading in the
  engine: per-thread regions + shared heap with promotion on publication;
  why PolyValue (slot-index, shards, 64-bit word) accommodates it; blockers
  (thread-local gcells, ICs, string pool) and phases T0→T5.

- [Method-dispatch engine (registration half)](rts-engine-dispatch.md) — the
  `rts-engine` builder/Registry design the live code references (§4/§9.5/§10
  external modules); the dispatch half was superseded by the codegen design
  doc §10 (see the status note at the top of the file).

## Active guides

- [Cross-runtime parity testing](cross-runtime-testing.md) — System that validates
  RTS vs Bun vs Node on standalone TS fixtures. Line-by-line stdout diff.
- [Cross-runtime coverage roadmap](cross-runtime-roadmap.md) — Living list of
  planned fixtures.
- [How to add a namespace or a class](namespace-creation-guide.md) — **Rewritten
  2026-07-28** for the single-source-of-truth flow: declare with `#[rtse::*]`
  (or the `e.module(…)` builder), symbol name DERIVED by `rts_abi::scope`
  (`__rtsm_`/`__rtsn_`/`__rtsa_`), one row in `REGISTER`, then re-bake with
  `cargo run -p rts-symbol-baker`. Never hand-write a symbol or signature row.
- [Immediate-mode GUI via egui — `rts-egui` crate + `ui` namespace](egui-ui-crate-design.md)
  — Design of the cross-platform GUI: egui (immediate-mode, no FLTK) in a new crate,
  primitives in Rust + high-level lib in TS, TS-driven loop, wgpu
  primary. Rendering foundation aiming at games/browser in the future.
- [HTML render engine (operational roadmap + north-star)](html-engine/README.md)
  — **DECIDED (2026-06-23):** evolve the light retained HTML engine already on main
  (tree DOM + data-driven block allocator in TS + mutation by NodeId, in
  `rts-egui`) IN-PLACE; the 5-tree `rts-html` crate will NOT be created. Living
  operational plan: [`rts-html-roadmap.md`](html-engine/rts-html-roadmap.md)
  (strategy, 10 decisions, 6 invariants, pixel-early phases F0-F5, kill-gates,
  first slice ≤1 day). The old 5-tree plan (DOM→Style→Layout→Display
  list→Paint, new crate, universal absolute paint) was demoted to a frozen
  north-star: [`rts-html-north-star.md`](html-engine/rts-html-north-star.md).
  Includes the 4 base analyses + architecture + adversarial critique.
- [Epic #226 — JS/TS parity](js-parity-epic-226.md) — Catalog of the ~60 JS
  APIs (Array/Object/Math/String/URL/Date/Boolean/parseInt/destructuring). Defines
  the SEMANTICS the new engine must cover. **Status/test numbers were stripped
  2026-07-28** — they were the deleted old engine's; always re-measure.
- [Reflect + Proxy](reflect-proxy.md) — Reference design of the Reflect API (13
  methods) + Proxy (13 traps) with `Entry::Proxy { target, handler }`. Target of
  `#218` in the new engine via the callback-from-runtime bridge.
- [async / Promise / Function](async-promise-function.md) — The old engine's async/
  await + Promise + Function class subsystem (reference). **The new engine
  has interim SYNCHRONOUS async** (event loop / real suspension are a clean redesign,
  `#207`) — this doc describes the previous Promise-centric model.
- [`rts:serde` — deep binary serialization (the RTS pickle)](serde-pickle.md) —
  RTSP v1 wire format (memo back-references: cycles + shared identity), class
  instances / Map/Set / functions-by-reference, ExtCodec + revive hooks,
  golden-file format freeze. Shipped (PRs #2008/#2009).
- [N-API then-chained crash study](napi-then-chained-crash-study.md) — Technical
  note on a crash in chained `.then` with N-API.
- [N-API implementation](napi-implementation.md) — Spec of the N-API implementation
  (159 fns, loader, HandleTable bridge).
- [Cranelift — explanations](cranelift-explications.md) — Notes on the Cranelift
  backend (egraph, stack maps, callconv).

### UI stack (referenced from the code — read with the html-engine roadmap)

These four are cited by live source files, so they are specs, not history. The
CANONICAL UI plan is still the frozen [html-engine roadmap](html-engine/README.md)
(F0–F5) — read it first; these describe pieces under it.

- [`rts-dom` crate design](rts-dom-crate-design.md) — the headless retained DOM
  (HTML parser + tree + query/mutation, no UI). Cited by
  `crates/rts-dom/Cargo.toml` and `rts-runtime/src/namespaces/mod.rs`.
- [DOM/layout in TS, dumb egui canvas](dom-in-ts-architecture.md) — cited by
  `crates/rts-egui/src/canvas.rs`.
- [Abstract render/input interfaces](dom-render-input-interfaces.md) — how the
  engine never names the concrete backend. Cited by
  `rts-runtime/src/namespaces/mod.rs`.
- [Input system design](input-system-design.md) — the 3 input phases + key-code
  table. Cited by `crates/rts-input/src/abi.rs` and `lib.rs`.
- [Engine limits found building the UI](engine-limits-found-building-ui.md) —
  engine gaps hit in practice while building the UI; a work-item source, not a plan.

## Historical / pending rewrite

Historical reference; do not take as a guide for new code.

- [rtslib-external-namespaces.md](rtslib-external-namespaces.md) — Design of
  external `.rtslib` packages. Depends on the ABI stabilizing before being resumed.

## Binding rules

Process rules live in [`.claude/rules/`](../../.claude/rules/)
(`00-meta` → `05-codegen-notes`), each binding. `CLAUDE.md` at the root is the
meta-index.
