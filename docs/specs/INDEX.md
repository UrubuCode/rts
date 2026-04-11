# Especificacoes e Notas Tecnicas

Indice de documentos de design, especificacoes de features e decisoes arquiteturais.

## Guias

- [Como criar um namespace](namespace-creation-guide.md) — Checklist obrigatorio para novos namespaces

## Namespaces Runtime

- [net — TCP nativo](net-namespace.md) — Sockets TCP via std::net, handles no runtime state

## Pipeline

- [Otimização do hot path de execução](perf-hot-path-optimization.md) — Como o bench `rts_simple.ts` saiu de ~2300 ms para 66 ms (AOT) / 73 ms (JIT), batendo Bun em 1.6×. Análise, decisões e armadilhas encontradas.

## Pendencias

- GC deterministico (F001)
- Semantica de modulos top-level (F002)
- Pipeline sem stubs de funcao (F003)
- Link fallback multi-objeto (F004)
- Promises sem vazamento (F005)
