# FEATURES

> Alinhado com branch `feat/remake-namespaces` (2026-04).
> Consultar `NEXT_STEPS.md` e `ROAD_MAP.md` para a direcao atual.
>
> Este arquivo e mantido como backlog historico. Itens marcados `OBSOLETO` ou
> `HISTORICO` refletem decisoes anteriores que foram substituidas pela arquitetura
> atual (contrato ABI unico em `src/abi/`, sem HIR/MIR, sem dispatch por namespace,
> sem `JsValue` no limite extern "C").

## Direcao
RTS deve ser uma dependencia de runtime/compilacao para executar TS/JS em linguagem de maquina.
O foco e previsibilidade, portabilidade e controle de runtime, nao compatibilidade estrita com V8.

## Prioridade P0

### F000 - Exportar superficie de runtime via modulo `"rts"`
Status atual: **parcial** — apenas `io`, `fs`, `gc` estao ativos nesta branch.

Objetivo:
- expor no `"rts"` as funcoes de runtime no estilo `std::*`, para TS/JS criar apps, CLIs,
  servicos e bibliotecas como linguagem completa.

Ativos hoje:
- `io` — print, eprint, stdout/stderr write+flush, stdin read/read_line
- `fs`  — read, write, append, exists, is_file, is_dir, size, modified_ms, dir ops, rename, copy
- `gc`  — handles e string pool (string_from_*, string_new/concat/len/ptr/free, HandleTable)

Pendentes de reintroducao sobre o novo contrato ABI (`src/abi/`): `process`, `crypto`, `net`,
`time`, `task`, `buffer`, `promise`, `global`. Serao redesenhados como `NamespaceSpec` com
membros `extern "C"` tipados — sem `dispatch()` nem `JsValue`.

Diretriz:
- manter API por namespaces, evitando API plana
- contratos de tipos em `builtin/rts-types/rts.d.ts` sao gerados a partir de `abi::SPECS`
- compilar/linkar apenas namespaces e funcoes realmente usados no projeto (runtime slicing
  aplicado por `rts compile`)

Criterios de aceite:
- comando `rts apis` lista toda a superficie disponivel a partir de `abi::SPECS`
- exemplos cobrindo app CLI, IO/FS, processo e async task
- documentacao gerada automaticamente a partir do `doc` de cada `NamespaceMember`

### F001 - GC deterministico [HISTORICO — substituido]
Proposta original previa heap proprio com ARC eager + coletor de ciclos em
`src/runtime/state.rs` e `src/runtime/bootstrap_lang/value.rs`, arquivos que nao
existem mais nesta branch.

Direcao atual: `gc-arena` para objetos gerenciados + `HandleTable` do namespace `gc`
para handles opacos (`u64` com gen:16 + slot:48). Coleta em pontos de quiescencia
(retorno de funcao, fim de metodo, fim de escopo de closure). Ver secao "GC" em `CLAUDE.md`.

Permanece no backlog: estrategia formal para ciclos, metricas e modos (`RTS_GC_*`).

### F002 - Semantica correta de modulos (efeitos colaterais top-level)
Status: em aberto. Continua relevante.

Objetivo:
- executar top-level de modulos importados uma unica vez, em ordem topologica

Valor:
- comportamento previsivel para pacotes TS/JS reais
- evita diferenca entre "compila" e "executa"

### F003 - Pipeline sem stubs de funcao [OBSOLETO]
Formulacao original ("carregar corpo real de funcoes no AST/HIR/MIR") nao se aplica:
HIR/MIR foram removidos nesta branch e o codegen consome AST direto em
`src/codegen/lower/`. A preocupacao equivalente (garantir que corpos reais sejam
emitidos, nao apenas assinaturas com `ret`) vive hoje como trabalho continuo no
codegen e no pipeline de modulos; acompanhar via `NEXT_STEPS.md`.

## Prioridade P1

### F004 - Link fallback robusto para multiplos objetos
Status: em aberto. Continua relevante — o backend de fallback precisa aceitar `N`
objetos em projetos multi-modulo quando o linker do sistema falha.

### F005 - Runtime de promises sem vazamento [PENDENTE DE REINTRODUCAO]
O namespace `promise` nao esta ativo nesta branch. Quando for reintroduzido sobre o
novo contrato ABI, este item volta ao escopo: remover promises do estado apos
settle/await e adicionar estrategia de expiracao opcional para evitar crescimento
infinito de memoria em execucoes longas.

## Notas de implementacao

- Manter API interna de memoria pequena e testavel (operacoes do namespace `gc`:
  alocacao de string, retain/release implicito por handle, leitura via `string_ptr`/`string_len`).
- Expor metricas de runtime para benchmark quando fizer sentido (`alloc_count`,
  `free_count`, `cycle_runs`, `heap_bytes`).
- Priorizar testes de regressao antes de ampliar superficie publica do modulo `"rts"`.
- Qualquer novo namespace precisa ser registrado em `abi::SPECS` — e a fonte unica de
  verdade para codegen, runtime e gerador de `rts.d.ts`.
