# Especificacoes e Notas Tecnicas

Indice de documentos de design, especificacoes de features e decisoes
arquiteturais. Para direcao de alto nivel do projeto consulte
`../../NEXT_STEPS.md` e `../../ROAD_MAP.md` na raiz do repositorio.

## Guias ativos

- [Como criar um namespace](namespace-creation-guide.md) — Processo atual
  baseado em `src/abi/` (SPECS centralizado, simbolos `__RTS_FN_NS_*`,
  `AbiType`). Reflete a branch `feat/remake-namespaces`.
- [Silent parallelism (Level-1)](silent-parallelism.md) — Como o codegen
  detecta padroes `for...of`, reduces, e `arr.map/forEach/reduce` e
  reescreve transparentemente para `parallel.*`. Pipeline dos 3 passes,
  criterio de pureza, infra de suporte (HandleTable shard-aware,
  callconv), limitacoes.

## Historico / pendente de reescrita

Os documentos abaixo descrevem versoes anteriores do runtime e ainda nao
foram reescritos para o novo contrato ABI. Use-os apenas como referencia
historica; nao os tome como guia para novo codigo.

- [app-features.md](app-features.md) — Roadmap de features do runtime. Muitos
  itens foram reorganizados; alinhar com `ROAD_MAP.md` antes de consultar.
- [perf-hot-path-optimization.md](perf-hot-path-optimization.md) — Notas da
  otimizacao do hot path (`rts_simple.ts`) antes da remocao de
  `__rts_call_dispatch`. Os numeros permanecem validos como marcador
  historico, mas o caminho descrito nao e mais o atual.
- [rtslib-external-namespaces.md](rtslib-external-namespaces.md) — Design de
  pacotes `.rtslib` externos. Depende da nova ABI estabilizar antes de ser
  retomado.

## Pendencias conhecidas

Itens acompanhados em `NEXT_STEPS.md` / `ROAD_MAP.md`:

- GC deterministico (gc-arena) nos pontos de quiescencia documentados em
  `CLAUDE.md`.
- Semantica de modulos top-level.
- Pipeline sem stubs de funcao.
- Link fallback multi-objeto.
- Promises sem vazamento.
