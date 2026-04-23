# FEATURES

## Direcao
RTS deve ser uma dependencia de runtime/compilacao para executar TS/JS em linguagem de maquina.
O foco e previsibilidade, portabilidade e controle de runtime, nao compatibilidade estrita com V8.

## Prioridade P0

### F000 - Exportar toda superficie `std::*` via modulo `"rts"`
Objetivo:
- expor no `"rts"` todas as funcoes de runtime no estilo `std::*`, para TS/JS criar apps, CLIs, servicos e bibliotecas como linguagem completa.

Diretriz:
- manter API por namespaces (`io`, `fs`, `process`, `crypto`, `net`, `time`, `task`, etc), evitando API plana.
- garantir contratos estaveis de tipos em `builtin/rts-types/rts.d.ts`.
- tratar `std::*` como superficie oficial do RTS para userland.
- compilar/linkar apenas namespaces e funcoes realmente usados no projeto (runtime slicing), evitando incluir todo o `"rts"`.

Valor:
- experiencia de desenvolvimento de linguagem "first-class" para apps reais.
- menor friccao para migrar codigo de scripts para projetos maiores.

Criterios de aceite:
- comando `rts apis` lista toda a superficie `std::*` disponivel.
- exemplos cobrindo app CLI, IO/FS, processo e async task.
- documentacao de cada namespace no README e em types.

### F001 - GC automatico deterministico (sem modelo "preguicoso")
Objetivo:
- liberar memoria rapido e de forma previsivel, sem esperar pausas opportunisticas do host.

Proposta tecnica:
- heap proprio do RTS com `Handle` (`u64`) em vez de ponteiros expostos;
- ARC eager (retain/release imediato) para liberar no instante em que `rc == 0`;
- coletor de ciclos periodico para grafos ciclicos (RC sozinho nao resolve ciclo);
- roots explicitas (stack de execucao, globais, promises/tarefas em voo, modulos carregados).

Integracao sugerida:
- `src/runtime/state.rs`: adicionar `heap: BTreeMap<u64, HeapNode>`, `roots`, contadores e metricas;
- `src/runtime/bootstrap_lang/value.rs`: introduzir variante de valor por handle (`HeapRef(u64)`);
- `src/runtime/namespaces/*`: alocacao e acesso de objetos/arrays/string grandes via API de heap.

Politica de coleta:
- coleta imediata em `release()` quando `rc == 0`;
- varredura de ciclos a cada `N` alocacoes ou quando `heap_bytes >= threshold`;
- modo de teste `stress`: roda ciclo em toda alocacao.

Observabilidade:
- `RTS_GC_MODE=deterministic|stress|off`
- `RTS_GC_CYCLE_EVERY=<N>`
- `RTS_GC_HEAP_LIMIT_MB=<N>`
- `RTS_GC_LOG=1`

Criterios de aceite:
- teste de ciclo (`a -> b -> a`) deve ser coletado;
- heap estabiliza em benchmark de longa execucao;
- sem regressao funcional em `cargo test`.

### F002 - Semantica correta de modulos (efeitos colaterais top-level)
Objetivo:
- executar top-level de modulos importados uma unica vez, em ordem topologica.

Valor:
- comportamento previsivel para pacotes TS/JS reais;
- evita diferenca entre "compila" e "executa".

### F003 - Pipeline sem stubs de funcao
Objetivo:
- carregar corpo real de funcoes no AST/HIR/MIR (nao apenas assinatura + `ret`).

Valor:
- JIT/AOT refletem logica real do programa;
- base correta para otimizacoes futuras.

## Prioridade P1

### F004 - Link fallback robusto para multiplos objetos
Objetivo:
- backend de fallback precisa aceitar `N` objetos, nao apenas 1.

Valor:
- build continua funcionando em projetos multi-modulo quando linker do sistema falha.

### F005 - Runtime de promises sem vazamento
Objetivo:
- remover promises do estado apos settle/await e adicionar estrategia de expiracao opcional.

Valor:
- evita crescimento infinito de memoria em execucoes longas.

## Notas de implementacao
- manter API interna de memoria pequena e testavel (`alloc`, `retain`, `release`, `set_field`, `get_field`);
- expor metricas de runtime para benchmark (`alloc_count`, `free_count`, `cycle_runs`, `heap_bytes`);
- priorizar testes de regressao antes de ampliar superficie publica do modulo `"rts"`.
