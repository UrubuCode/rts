# PROJECT_PLAN

## Objetivo Atual
Consolidar o RTS como compilador/runtime TypeScript com:
- pipeline modular (`parser -> type_system -> hir -> mir -> codegen -> linker -> runtime`);
- execucao funcional no modo bootstrap;
- API builtin `"rts"` alinhada ao estilo `std::` do Rust (por namespaces e tipos de resultado);
- base preparada para backend nativo completo e runtime mais robusto.

## Estado Atual (Abril/2026)

### 1) CLI e Fluxo de Build
- `Concluido`: comandos `run`, `build`, `repl`, `init`, `apis`.
- `Concluido`: perfis `development` / `production` / `debug`.
- `Concluido`: pipeline principal em `src/lib.rs`.
- `Parcial`: mensagens de erro estao boas, mas ainda sem diagnostico/sourcemap completo.

### 2) Modulos e Imports
- `Concluido`: resolver de modulos locais (`.ts/.rts`), pacotes em `packages/`, builtin (`"rts"`), URL e cache.
- `Concluido`: leitura de `package.json` com `dependencies`.
- `Concluido`: grafo de modulos em `src/module_system`.
- `Parcial`: politica de cache/publicacao ainda bootstrap.

### 3) Parser / AST
- `Concluido`: AST propria com itens principais (import/interface/class/function/statement).
- `Concluido`: parse de assinaturas e metadados basicos de tipo.
- `Parcial`: parser ainda simplificado para TS completo (sem cobertura total da linguagem).

### 4) Type System
- `Concluido`: `TypeRegistry`, `TypeResolver`, `checker`, `metadata`.
- `Concluido`: primitivos registrados e validacao basica de imports/export.
- `Parcial`: inferencia e checagem semantica profunda ainda limitada.

### 5) HIR
- `Concluido`: lowering AST -> HIR.
- `Concluido`: pass de otimizacao HIR (`src/hir/optimize.rs`) com:
  - deduplicacao de imports,
  - simplificacao de declaracoes,
  - inferencia basica de literais,
  - inlining simples de chamadas triviais.
- `Parcial`: HIR ainda sem modelagem semantica rica de expressoes/controle.

### 6) MIR
- `Concluido`: build HIR -> MIR e CFG linear.
- `Concluido`: pass de otimizacao MIR com limpeza de `noop`, dedupe de imports, propagacao/inferencia basica e inlining textual.
- `Parcial`: MIR ainda textual em varios pontos (nao totalmente estruturado/SSA).

### 7) Codegen / Cranelift
- `Concluido`: integracao real de JIT com Cranelift em `src/codegen/cranelift/jit.rs`.
- `Concluido`: execucao de entry JIT e testes de JIT.
- `Parcial`: caminho AOT ainda usa CLIF textual + container/linker bootstrap.

### 8) Linker
- `Concluido`: backend Rust com crate `object` (`PE/ELF/Mach-O` containerizados).
- `Concluido`: empacotamento de payload bootstrap no binario.
- `Parcial`: linkagem nativa completa do codigo gerado ainda em evolucao.

### 9) Runtime Bootstrap
- `Concluido`: runtime builtin `"rts"` com varios intrinsecos (`print`, IO basico, fs basico, env/process).
- `Concluido`: avaliador de expressoes com precedencia e coercao basica.
- `Concluido`: suporte a `const/let/var`, chamadas e `return` em fluxo bootstrap.
- `Parcial`: semantica JS/TS completa (objetos/closures/heap completo) ainda nao finalizada.

### 10) Paralelismo (Multithread)
- `Concluido`: paralelizacao com `rayon` no lowering/otimizacao por modulo em `compile_graph` e `cli run`.
- `Parcial`: type-check global ainda sequencial.

### 11) Testes
- `Concluido`: testes unitarios de runtime, JIT, HIR optimize e MIR optimize.
- `Concluido`: exemplos de execucao em `examples/` e pacote `packages/tests`.
- `Parcial`: suites de regressao/benchmark mais amplas ainda pendentes.

### 12) Superficie de API "rts" (Padrao std::)
- `Concluido`: Fase 1 ativa com API `std::` por namespaces (`rts.io`, `rts.fs`, `rts.process`, `rts.crypto`, `rts.global`, `rts.buffer`, `rts.promise`, `rts.task`).
- `Concluido`: funcoes legadas removidas da superficie builtin do modulo `"rts"` (sem API plana).
- `Concluido`: operacoes de FS ligadas diretamente ao `std::fs` no runtime Rust.
- `Concluido`: `io.Result<T>` agora usa estrutura nativa de objeto (`ok/tag/value/error`) no bootstrap.

## Fase 1 Ativa (Sem Legado)

### Contrato Oficial
- API publica de filesystem:
  - `import { fs, io } from "rts"`
  - `fs.read_to_string<P>(path: P): io.Result<string>`
  - `fs.read<P>(path: P): io.Result<Uint8Array>`
  - `fs.write<P>(path: P, data: string | Uint8Array): io.Result<void>`
- API publica de resultado:
  - `io.is_ok(result)`
  - `io.is_err(result)`
  - `io.unwrap_or(result, fallback)`

### Decisoes de Fase 1
- Nao manter funcoes legadas de FS no builtin `"rts"`.
- Nao adicionar camada de compatibilidade para `readTextFile/writeTextFile/...`.
- Todo tratamento de arquivo desta fase passa por `std::fs` diretamente no runtime Rust.

### Legado Removido do Builtin "rts"
- `readTextFile`
- `writeTextFile`
- `appendTextFile`
- `createDir`
- `removeDir`
- `removeFile`
- `fileExists`

## Roadmap Atualizado

### Fase A - Normalizacao da API std:: em "rts"
- [x] Definir `io.Result<T>` e `io.Error` em `packages/rts-types/rts.d.ts`.
- [x] Criar namespace `rts.fs` com `read_to_string`, `read`, `write`.
- [x] Remover funcoes legadas de FS do builtin `"rts"` (sem compat layer).
- [x] Atualizar `packages/fs` para consumir `rts.fs` e `rts.io`.
- [x] Evoluir `io.Result<T>` para representacao estruturada (sem encoding textual).

### Fase B - Curto Prazo
- [ ] Estruturar MIR sem formato textual (instrucoes tipadas e CFG mais rico).
- [ ] Ampliar type-check para inferencia e validacao semantica mais forte.
- [ ] Paralelizar parte do type-check com merge seguro de registries.
- [ ] Melhorar diagnosticos com spans mais precisos.

### Fase C - Backend Nativo
- [ ] Fechar lowering MIR -> Cranelift IR nativo (AOT real).
- [ ] Reduzir dependencia do caminho bootstrap para execucao final.
- [ ] Melhorar estrategia de linkagem por plataforma.

### Fase D - Runtime
- [ ] Evoluir heap/gerenciamento automatico de memoria para objetos de alto nivel.
- [ ] Expandir semantica JS/TS suportada (arrays/objetos/closures/controle).
- [ ] Definir API de concorrencia/runtime (`spawn/join/channels`) no modulo `"rts"`.

### Fase E - Qualidade e Distribuicao
- [ ] Expandir cobertura de testes de compilacao e execucao.
- [ ] Adicionar benchmarks comparativos (tempo de build/run).
- [ ] Documentar contrato estavel para pacotes `packages/*` e cache de modulos.

## Relacao com PROJECT_MAP
Este `PROJECT_PLAN.md` descreve **status e direcao**.
O `PROJECT_MAP.md` descreve **estrutura de arquivos e componentes**.
