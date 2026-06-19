# Divergência `main` ↔ `feat/rts-codegen-new` (cutover) — 2026-06-19

Esta branch (`feat/rts-codegen-new`) DELETOU o motor antigo (`rts-codegen-old`) +
o tier MIR (`rts-mir`) no cutover P5. A `main` ainda está no layout pré-strangler-
fig (o motor é `crates/rts-codegen`). Por isso um `git merge` normal conflitaria em
~todo o motor. Reconciliação feita via `git merge -s ours main`: a `main` vira
ancestral (histórico unificado), a ÁRVORE desta branch fica intacta, e **só o N-API
foi portado manualmente** (a parte importante). Este doc lista o que a `main` tinha
e o destino de cada mudança.

## Os 30 commits da `main` à frente desta branch

### ✅ PORTADO — N-API (.node native addons) — #1545–#1555
Trazido via `crates/rts-napi/` + suporte no `rts-engine` (commit `201c3d4b` desta
branch). Crate completo (159 fns), `Entry::ArrayBuffer`/`BigInt`/`NapiExternal` +
fila de finalizers no engine, `rts-runtime` re-exporta `napi`. Lib 59/59 verde.

| Commit | Conteúdo |
|---|---|
| `413a9923` | base .node — N-API paridade Node |
| `3df281c4` | Fase 2 — Buffer/Date/Symbol/wrap/classes |
| `2bcc8df7`/`fb2e1d40` | docs/tracking (#1547/#1548) |
| `090f0416` | `Entry::ArrayBuffer` ptr estável + 15 fns ArrayBuffer/TypedArray |
| `1d4a8389` | async-work síncrono + callback scopes (144/159) |
| `e3337a56` | BigInt real + threadsafe + Promise↔async (158/159) |
| `c7d06c20` | fix `.then()` encadeado em Promise N-API |
| `6798aae5` | `napi_module_register` legado — 159/159 |

**PENDENTE no motor novo** (documentado em `napi-implementation.md`): lowering de
`require('x.node')` → `__RTS_FN_NS_NAPI_LOAD_ADDON` (motor novo ainda sem
`require`/dynamic-import) + `force_link` p/ reter os símbolos `napi_*` no bin. As
mudanças da `main` em `rts-ast/src/ast.rs` e `rts-primitives/src/function/ops.rs`
(parte do #1545) eram a integração `.node` no motor ANTIGO — serão refeitas como
lowering do motor novo, não portadas verbatim.

### ❌ NÃO PORTADO — perf/fix do motor ANTIGO (`rts-codegen`, deletado) — #1556–#1564
Otimizações de codegen no motor antigo. Não aplicáveis (o motor novo tem outro
modelo: PolyValue + egraph do Cranelift como único otimizador). Reimplementar SE/
QUANDO o motor novo precisar, como trabalho do motor novo — não como porte.

| Commit | Conteúdo | Por que N/A |
|---|---|---|
| `44329312` | atomic RMW intrinsic (fix race async paralelo) | intrinsic no codegen antigo; toca `rts-shared/collections/{vec,map}.rs` (helper do RMW) |
| `4be63ddc` | int32 bitwise conformance + storage nativo de array | codegen antigo (`RTS_ARRAY_INLINE`); a conformidade bitwise no motor novo é separada (`repr`/genops_arith) |
| `fe85cc75` | array inline (tamanho via const + top-level) | `RTS_ARRAY_INLINE` no codegen antigo |
| `ae6ca45d` | inline de user-fn pequena no call-site | `RTS_INLINE_AST` no codegen antigo (o egraph do Cranelift faz inlining no novo) |
| `ddea75b9` | inline aceita assign no prelude | idem |
| `1279480b` | limita array nativo a 1024 elems (anti-stack-overflow) | guard do storage nativo do codegen antigo |
| tests `tests/claude-*` + `bench/*` | fixtures/benches dessas features | exercitam o motor antigo |

### ⏭️ IGNORADO — docs/parity-badge auto-gerados
`docs: auto-update cross-runtime parity badge (…%)` (vários) +
`cross_runtime_history/*.json` + `cross_runtime_report.json` + snapshot 2026-06-15.
Dados/badges medidos no motor ANTIGO (paridade ~70.7-71.1%) — irrelevantes pro motor
novo (que mede via `measure_new.sh`, baseline 274/630 hoje). Não trazidos.

## Resumo
- **Importante (N-API): portado e funcional.**
- **Resto: motor antigo (deletado) ou docs auto-gerados → listado aqui, não portado**
  (decisão explícita do dono: "não precisa implementar, apenas o n-api").
- Histórico reconciliado com `git merge -s ours main`.
