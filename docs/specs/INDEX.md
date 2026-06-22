# Especificações e Notas Técnicas

Índice de documentos de design, especificações de features e decisões
arquiteturais.

**A direção canônica do motor é
[`rts-codegen-new-design.md`](rts-codegen-new-design.md)** — o redesign
ground-up (PolyValue NaN-box, Repr lattice, shapes + data ICs, lowering único
HIR→Cranelift, dispatch data-driven). Leia antes de qualquer trabalho no motor.
O motor velho (`crates/rts-codegen-old/`) está CONGELADO e é deletado no cutover;
docs que descreviam a lógica dele (silent parallelism, hot-path pré-ABI, plano de
paridade 100% no motor velho, core-engine antigo, app-features) foram REMOVIDOS —
não eram guia para o motor novo.

## Canônico

- [Redesign do motor (rts-codegen-new)](rts-codegen-new-design.md) — **a
  direção canônica.** PolyValue, lattice de Repr, shapes + data ICs, lowering
  único, dispatch data-driven + ABI gerado. Fases de migração P0→P5.
- [GC do motor novo](gc-generational-design.md) — fase weak (`#217`, bounded, no
  mark+sweep atual) agora; geracional copying (nursery) como salto de longo prazo
  deferido até ~90% cross-runtime. A vantagem da handle indirection (mover ≈
  grátis).
- [Plano — GC novo + API/ABI gc modernizada](gc-new-api-plan.md) — PLANO DE
  EXECUÇÃO: remover toda a API gc do motor antigo (pool manual de string
  `gc.string_*` etc.), migrar as ~110 fixtures legacy para string nativa,
  auditar/enxugar `Entry`, refazer a ABI gc PolyValue-nativa, e preparar o terreno
  para a fase weak + o geracional. Objetivo: melhor GC + melhor API/ABI, zero
  legacy do motor antigo.

## Guias ativos

- [Cross-runtime parity testing](cross-runtime-testing.md) — Sistema que valida
  RTS vs Bun vs Node em fixtures TS standalone. Diff de stdout linha-a-linha.
- [Cross-runtime coverage roadmap](cross-runtime-roadmap.md) — Lista viva das
  fixtures planejadas.
- [Como criar um namespace](namespace-creation-guide.md) — Processo baseado em
  `rts-engine::abi` (SPECS centralizado, símbolos `__RTS_FN_NS_*`, `AbiType`).
- [Epic #226 — paridade JS/TS](js-parity-epic-226.md) — Catálogo das ~60 APIs
  JS (Array/Object/Math/String/URL/Date/Boolean/parseInt/destructuring). Define
  as SEMÂNTICAS que o motor novo deve cobrir (a implementação migra para o
  modelo PolyValue/shapes; não tome a lista de PRs como caminho do motor novo).
- [Reflect + Proxy](reflect-proxy.md) — Design de referência da API Reflect (13
  métodos) + Proxy (13 traps) com `Entry::Proxy { target, handler }`. Alvo do
  `#218` no motor novo via o callback-from-runtime bridge.
- [async / Promise / Function](async-promise-function.md) — Subsistema async/
  await + Promise + Function class do motor velho (referência). **O motor novo
  tem async SÍNCRONO interino** (event loop / suspensão real são redesign limpo,
  `#207`) — este doc descreve o modelo Promise-centric anterior.
- [Suporte a `.node` (Node native addons)](node-format/README.md) — Estudo da
  ABI N-API → `HandleTable` sem V8. Implementado em `crates/rts-napi/`.
- [N-API then-chained crash study](napi-then-chained-crash-study.md) — Nota
  técnica de um crash em `.then` encadeado com N-API.
- [N-API implementation](napi-implementation.md) — Spec da implementação N-API
  (159 fns, loader, HandleTable bridge).
- [Cranelift — explicações](cranelift-explications.md) — Notas sobre o backend
  Cranelift (egraph, stack maps, callconv).
- [Divergência main ↔ cutover](main-divergence.md) — Nota da divergência no
  cutover P5 (deleção do motor velho + tier MIR).

## Histórico / pendente de reescrita

Referência histórica; não tome como guia para código novo.

- [rtslib-external-namespaces.md](rtslib-external-namespaces.md) — Design de
  pacotes `.rtslib` externos. Depende da ABI estabilizar antes de ser retomado.

## Regras vinculantes

As regras de processo ficam em [`.claude/rules/`](../../.claude/rules/)
(`00-meta` → `05-codegen-notes`), cada uma vinculante. `CLAUDE.md` na raiz é o
meta-índice.
