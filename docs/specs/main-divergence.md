# `main` ↔ `feat/rts-codegen-new` divergence (cutover) — 2026-06-19

This branch (`feat/rts-codegen-new`) DELETED the old engine (`rts-codegen-old`) +
the MIR tier (`rts-mir`) at the P5 cutover. `main` is still on the pre-strangler-
fig layout (the engine is `crates/rts-codegen`). A normal `git merge` would therefore
conflict on ~the whole engine. Reconciliation was done via `git merge -s ours main`:
`main` becomes an ancestor (unified history), this branch's TREE stays intact, and
**only N-API was ported manually** (the important part). This doc lists what `main`
had and the fate of each change.

## The 30 commits on `main` ahead of this branch

### ✅ PORTED — N-API (.node native addons) — #1545–#1555
Brought in via `crates/rts-napi/` + support in `rts-engine` (commit `201c3d4b` of this
branch). Complete crate (159 fns), `Entry::ArrayBuffer`/`BigInt`/`NapiExternal` +
finalizer queue in the engine, `rts-runtime` re-exports `napi`. Lib 59/59 green.

| Commit | Content |
|---|---|
| `413a9923` | .node base — N-API Node parity |
| `3df281c4` | Phase 2 — Buffer/Date/Symbol/wrap/classes |
| `2bcc8df7`/`fb2e1d40` | docs/tracking (#1547/#1548) |
| `090f0416` | `Entry::ArrayBuffer` stable ptr + 15 ArrayBuffer/TypedArray fns |
| `1d4a8389` | synchronous async-work + callback scopes (144/159) |
| `e3337a56` | real BigInt + threadsafe + Promise↔async (158/159) |
| `c7d06c20` | fix chained `.then()` on an N-API Promise |
| `6798aae5` | legacy `napi_module_register` — 159/159 |

**PENDING in the new engine** (documented in `napi-implementation.md`): lowering of
`require('x.node')` → `__RTS_FN_NS_NAPI_LOAD_ADDON` (the new engine still has no
`require`/dynamic-import) + `force_link` to retain the `napi_*` symbols in the bin. The
`main` changes in `rts-ast/src/ast.rs` and `rts-primitives/src/function/ops.rs`
(part of #1545) were the `.node` integration in the OLD engine — they will be redone as
new-engine lowering, not ported verbatim.

### ❌ NOT PORTED — perf/fixes of the OLD engine (`rts-codegen`, deleted) — #1556–#1564
Codegen optimizations in the old engine. Not applicable (the new engine has a different
model: PolyValue + the Cranelift egraph as sole optimizer). Reimplement IF/
WHEN the new engine needs it, as new-engine work — not as a port.

| Commit | Content | Why N/A |
|---|---|---|
| `44329312` | atomic RMW intrinsic (parallel-async race fix) | intrinsic in the old codegen; touches `rts-shared/collections/{vec,map}.rs` (the RMW helper) |
| `4be63ddc` | int32 bitwise conformance + native array storage | old codegen (`RTS_ARRAY_INLINE`); bitwise conformance in the new engine is separate (`repr`/genops_arith) |
| `fe85cc75` | array inline (size via const + top-level) | `RTS_ARRAY_INLINE` in the old codegen |
| `ae6ca45d` | inlining of small user-fn at the call-site | `RTS_INLINE_AST` in the old codegen (the Cranelift egraph does inlining in the new one) |
| `ddea75b9` | inline accepts assign in the prelude | ditto |
| `1279480b` | caps native array at 1024 elems (anti-stack-overflow) | guard of the old codegen's native storage |
| tests `tests/claude-*` + `bench/*` | fixtures/benches of those features | exercise the old engine |

### ⏭️ IGNORED — auto-generated docs/parity-badge
`docs: auto-update cross-runtime parity badge (…%)` (several) +
`cross_runtime_history/*.json` + `cross_runtime_report.json` + snapshot 2026-06-15.
Data/badges measured on the OLD engine (parity ~70.7-71.1%) — irrelevant to the new
engine (which measures via `measure_new.sh`, baseline 274/630 today). Not brought over.

## Summary
- **The important part (N-API): ported and working.**
- **The rest: old engine (deleted) or auto-generated docs → listed here, not ported**
  (explicit owner decision: "no need to implement, only the n-api").
- History reconciled with `git merge -s ours main`.
