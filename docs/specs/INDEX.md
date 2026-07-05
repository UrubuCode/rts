# Specifications and Technical Notes

Index of design documents, feature specifications, and architectural
decisions.

**The canonical direction for the engine is
[`rts-codegen-new-design.md`](rts-codegen-new-design.md)** — the ground-up
redesign (PolyValue NaN-box, Repr lattice, shapes + data ICs, single
HIR→Cranelift lowering, data-driven dispatch). Read it before any engine work.
The old engine (`crates/rts-codegen-old/`) is FROZEN and gets deleted at cutover;
docs that described its logic (silent parallelism, pre-ABI hot-path, the 100%
parity plan on the old engine, the old core-engine, app-features) were REMOVED —
they were not a guide for the new engine.

## Canonical

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
- [How to create a namespace](namespace-creation-guide.md) — Process based on
  `rts-engine::abi` (centralized SPECS, `__RTS_FN_NS_*` symbols, `AbiType`).
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
  the SEMANTICS the new engine must cover (the implementation migrates to the
  PolyValue/shapes model; do not take the PR list as the new engine's path).
- [Reflect + Proxy](reflect-proxy.md) — Reference design of the Reflect API (13
  methods) + Proxy (13 traps) with `Entry::Proxy { target, handler }`. Target of
  `#218` in the new engine via the callback-from-runtime bridge.
- [async / Promise / Function](async-promise-function.md) — The old engine's async/
  await + Promise + Function class subsystem (reference). **The new engine
  has interim SYNCHRONOUS async** (event loop / real suspension are a clean redesign,
  `#207`) — this doc describes the previous Promise-centric model.
- [`.node` support (Node native addons)](node-format/README.md) — Study of the
  N-API → `HandleTable` ABI without V8. Implemented in `crates/rts-napi/`.
- [N-API then-chained crash study](napi-then-chained-crash-study.md) — Technical
  note on a crash in chained `.then` with N-API.
- [N-API implementation](napi-implementation.md) — Spec of the N-API implementation
  (159 fns, loader, HandleTable bridge).
- [Cranelift — explanations](cranelift-explications.md) — Notes on the Cranelift
  backend (egraph, stack maps, callconv).
- [main ↔ cutover divergence](main-divergence.md) — Note on the divergence at the
  P5 cutover (deletion of the old engine + MIR tier).

## Historical / pending rewrite

Historical reference; do not take as a guide for new code.

- [rtslib-external-namespaces.md](rtslib-external-namespaces.md) — Design of
  external `.rtslib` packages. Depends on the ABI stabilizing before being resumed.

## Binding rules

Process rules live in [`.claude/rules/`](../../.claude/rules/)
(`00-meta` → `05-codegen-notes`), each binding. `CLAUDE.md` at the root is the
meta-index.
