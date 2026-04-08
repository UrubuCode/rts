# PROJECT_PLAN

## Objetivo atual
Consolidar o RTS como runtime/app builder TS com:
- compilacao nativa (AOT) com Cranelift + linker;
- execucao JIT para ciclo rapido de desenvolvimento;
- API builtin `"rts"` orientada a namespaces;
- distribuicao multiplataforma com cache de toolchains e fallback de download.

## Estado atual (2026-04-08)

### 1) CLI e fluxo principal
- Concluido: `build`, `run`, `repl`, `init`, `apis`.
- Concluido: perfis `development` e `production`, mais `--debug`.
- Concluido: resumo de build com dados de objetos, cache e linking.
- Parcial: erros ainda podem ser melhorados com spans/sourcemap.

### 2) Frontend do compilador
- Concluido: parser/AST proprietarios para subset atual.
- Concluido: `type_system` com registro/check/resolver/metadata.
- Parcial: cobertura TS ainda longe de linguagem completa.

### 3) HIR e MIR
- Concluido: lowering AST -> HIR -> MIR.
- Concluido: otimizacoes basicas em HIR/MIR.
- Parcial: MIR ainda textual em varios pontos; falta IR tipado mais rico.

### 4) Codegen Cranelift
- Concluido: JIT funcional em `src/codegen/cranelift/jit.rs`.
- Concluido: AOT funcional em `src/codegen/cranelift/object_builder.rs`.
- Concluido: ABI interna por handle com assinatura:
  - `(argc, a0..a5) -> i64`
- Concluido: avaliacao de expressoes de argumentos via `__rts_eval_expr`.
- Concluido: dispatch de builtin via `__rts_call_dispatch`.
- Parcial: limite atual de ate 6 argumentos por chamada ABI.

### 5) Namespaces e API `"rts"`
- Concluido: namespaces em `src/namespaces/*` (fora de `src/runtime`).
- Concluido: estado compartilhado centralizado em `src/namespaces/state.rs`.
- Concluido: geracao de `packages/rts-types/rts.d.ts` por catalogo + comentarios.
- Concluido: `io`, `fs`, `process`, `crypto`, `global`, `buffer`, `promise`, `task`.
- Parcial: ainda faltam APIs de runtime para casos mais avancados de app builder.

### 6) Build seletivo de runtime
- Concluido: compilacao por uso real:
  - gera apenas `builtin_rts_<callee>.o/.m` necessarios.
- Concluido: limpeza de cache de objetos runtime nao usados.
- Concluido: catalogo de namespaces em `target/.launcher/rts_namespace_catalog.json`.

### 7) Linker e toolchains
- Concluido: backend `system` com fallback `object`.
- Concluido: resolucao de linker com cache e busca em multiplas fontes.
- Concluido: padrao de cache em `~/.rts/toolchains`.
- Concluido: suporte a estrutura:
  - `~/.rts/toolchains/rust-lld/<target>/...`
  - `~/.rts/toolchains/<tool>/<target>/...`
- Concluido: download automatico de linker se nao encontrado.
- Parcial: experiencia offline ainda depende de cache pre-populado.

### 8) Runtime support library
- Concluido: AOT inclui runtime support library no link final.
- Concluido: busca local + fallback de build (`cargo build --lib`).
- Concluido: opcao de download por template (via env) para runtime lib.
- Parcial: estrategia de distribuicao prebuilt por release/plataforma pode ser refinada.

### 9) Testes
- Concluido: suite unitaria cobrindo JIT, AOT, bootstrap runtime, namespaces e linker.
- Concluido: exemplos executaveis para validacao rapida.
- Parcial: falta suite de regressao maior (cenarios multi-modulo e stress de ABI).

## Riscos tecnicos atuais
1. MIR textual aumenta fragilidade de parsing/lowering.
2. ABI por handle ainda simples (sem tipagem rica no boundary).
3. limite de args (6) exige expansao para APIs mais complexas.
4. semantica TS/JS completa ainda nao esta implementada.

## Prioridades recomendadas (proximas fases)

### Fase A - Robustez de IR
- [ ] Migrar MIR textual para instrucoes tipadas.
- [ ] Melhorar modelagem de CFG e controle de fluxo.
- [ ] Reduzir parsing textual no codegen.

### Fase B - ABI e runtime core
- [ ] Evoluir ABI de valores (mais tipos nativos e menos roundtrip textual).
- [ ] Remover limite fixo de 6 args.
- [ ] Definir contrato estavel de FFI/runtime para longo prazo.

### Fase C - Toolchain/distribuicao
- [ ] Fechar fluxo oficial de distribuicao sem Rust preinstalado.
- [ ] Publicar artefatos prebuilt por target para runtime lib/linker.
- [ ] Melhorar diagnostico quando download/cache falhar.

### Fase D - Linguagem e apps
- [ ] Expandir suporte TS/JS (objetos, arrays, closures e mais semantica).
- [ ] Evoluir APIs de app builder (processo, rede, FS, async).
- [ ] Fortalecer contratos de pacote em `packages/*`.

## Relacao com PROJECT_MAP
- `PROJECT_MAP.md`: estrutura atual dos modulos e arquivos.
- `PROJECT_PLAN.md`: status, riscos e direcao de evolucao.
