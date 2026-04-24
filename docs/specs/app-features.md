# APP_FEATURES.MD
> Documento vivo de estado atual, plano de evolução e benchmarks do RTS.
> Atualizar a cada feature concluída. Data de criação: 2026-04-11.
>
> **Ultima revisao: 2026-04-24** — alinhado com branch `feat/remake-namespaces`.
> Status de etapas anteriores pode estar desatualizado; consultar `NEXT_STEPS.md`
> e `ROAD_MAP.md` para direcao vigente.

---

## Estado Atual do Projeto

### Pipeline de execução

| Comando | Rota | Usa disco? |
|---|---|---|
| `rts run <file>` | Source → Parser(SWC) → type_system → **Codegen (Cranelift)** → Object → Linker (ou carga in-memory) | Sim (em `node_modules/.rts/objs/runtime/`) |
| `rts compile <file> <out>` | Source → Parser(SWC) → type_system → **Codegen (Cranelift)** → Object (.o + .m cache) → Linker → `.exe` | Sim (em `node_modules/.rts/objs/compile/`) |

Pipeline canonico: `Source TS → Parser(SWC) → type_system → codegen(Cranelift, src/codegen/lower/) → Object → Linker → .exe`.

As camadas HIR e MIR foram removidas da branch `feat/remake-namespaces`: o codegen consome o AST do SWC diretamente via `src/codegen/lower/`. Cache: grava `<stem>.o` (objeto Cranelift) e `<stem>.m` (JSON com hash do fonte, flags, versao RTS). Na proxima execucao, se o `.m` bate com o fonte atual, o `.o` e reusado sem recompilar.

### Flags CLI atuais

| Flag | Aliases | Efeito |
|---|---|---|
| `--debug` | `-D` | Ativa timeline de estágios, métricas de dispatch e JIT report |
| `--development` | `-d` | Profile dev (padrão) |
| `--production` | `-p` | Profile prod — otimizações Cranelift, fingerprint de erro |
| `--native` | — | Frontend modo nativo (padrão) |
| `--compat` | — | Frontend modo compat |
| `--eval` | `-e` | Avalia código inline |

### Estrutura de diretorios atual

```
<project>/
  node_modules/
    .rts/
      objs/
        runtime/
          <module>.o     — objects completos do builtin (rts run)
          <module>.m     — cache metadata (JSON)
        compile/
          <module>.o     — objects AOT, somente modulos usados (rts compile)
          <module>.m
      modules/           — dependencias resolvidas
  release/               — apenas em rts compile (binario final)
    <project_name>(.exe|.dll|.so|.node)
```

### Contrato ABI centralizado (`src/abi/`)

O antigo dispatch unificado (`rts_namespace_dispatch.o`, `__rts_call_dispatch`, `JsValue` no limite) foi substituido por um contrato ABI estatico em `src/abi/`:

- **`NamespaceSpec` / `NamespaceMember`**: tabelas estaticas descrevendo cada namespace, seus membros, assinaturas e tipos ABI.
- **`AbiType`**: enum dos tipos primitivos do limite (`I64`, `F64`, `Bool`, `StringSlice (ptr,len)`, `Handle`).
- **Simbolos**: cada funcao de namespace exporta `__RTS_FN_NS_<NS>_<NAME>` como `#[unsafe(no_mangle)] pub extern "C"` com tipos nativos — sem boxing, sem `JsValue`, sem dispatch generico em runtime.
- Codegen emite chamadas diretas ao simbolo `__RTS_FN_NS_<NS>_<NAME>` com base no `NamespaceSpec` — nao ha mais `__rts_dispatch(fn_id, a0..a5)` como intermediario.
- Handles (`u64`) continuam sendo o transporte para recursos heap (buffers, sockets, promises, strings dinamicas).

### GC Arena

- Implementação: `gc-arena` (determinístico, não preguiçoso)
- Alocação: `GcPool` thread-local com `Vec<Option<Gc<'gc, GcBlob>>>` e free-list
- Coleta: `exit_scope()` dispara `collect_all()` (pressure ≥ 512 alocações) ou `collect_debt()` (amortizado)
- Cobertura: apenas valores alocados via `gc.alloc()` explícito — o `ValueStore` interno (handles de bindings, strings dinâmicas, objetos) **não passa pelo gc-arena**, usando `Vec<RuntimeValue>` direto
- Problema conhecido: em benchmarks com múltiplas requisições (`examples/site/server.ts`), RAM cresce constantemente porque o `ValueStore` não é coletado pelo gc-arena — ele vive em `RefCell<ValueStore>` por thread sem compactação

### Geração de tipos (.d.ts)

- `packages/rts-types/rts.d.ts` — `declare module "rts"` com todos os namespaces
- `packages/rts-types/<namespace>.d.ts` — arquivos split por namespace
- Gerados por `render_typescript_declarations()` e `emit_split_typescript_declarations()`
- Problema: o `rts.d.ts` contém namespaces `rts.natives`, `rts.hotops`, `rts.debug` com sintaxe `export namespace rts.natives { ... }` que **não é TypeScript válido** (namespace aninhado com ponto no nome não é suportado pelo compilador TS)

### HIR / MIR

> [obsoleto — nao se aplica a arquitetura atual]
>
> As camadas HIR (`src/hir/`) e MIR (`src/mir/`) foram removidas na branch
> `feat/remake-namespaces`. O codegen agora consome diretamente o AST do SWC
> via `src/codegen/lower/`, eliminando o ciclo parse→string→re-parse que gerava
> a maior parte dos fallbacks `RuntimeEval`. Conteudo historico sobre
> `typed_build.rs`, `typed_codegen.rs`, `VRegKind`, `CALLEE_FN_IDS` e
> `RuntimeEval` nao reflete o codepath atual.

### Codegen (Cranelift)

- `src/codegen/lower/` — conjunto de modulos que faz AST (SWC) → Cranelift IR diretamente.
- Integra com o contrato ABI em `src/abi/`: cada chamada de namespace resolve para o simbolo `__RTS_FN_NS_<NS>_<NAME>` com assinatura tipada.
- Sem boxing no limite — tipos nativos (`i64`, `f64`, `(ptr,len)`, `u64` handle) circulam direto nos registradores.

---

## Benchmarks de Features Atuais

> Referência: medido com `bench/benchmark.ps1`, hardware local, Windows 11.

| Cenário | RTS run | RTS compile (exec) | Bun | Node |
|---|---|---|---|---|
| `examples/rts_simple.ts` (hello world) | ~15ms | ~2ms (cache hit) | ~25ms | ~45ms |
| `examples/counter.ts` (loop 1M) | ~45ms | ~8ms | ~35ms | ~60ms |
| `examples/site/server.ts` (req/s) | baseline | +40% vs run | ~3x vs run | ~2x vs run |

> Nota: benchmarks são aproximados. RAM em server.ts cresce ~2MB/100req por vazamento no ValueStore.

---

## Etapas de Evolução Planejadas

---

### ETAPA 0 — Sistema de diagnósticos estruturado ✅ (núcleo entregue 2026-04-11)

**Entregue na branch `feat/http-server`:**
- `DiagnosticEngine` global (`src/diagnostics/reporter.rs`) com `RichDiagnostic` + rendering estilo rustc (código, arquivo:linha:col, snippet com seta, notas, sugestão, ANSI colors)
- `SourceStore` (`src/diagnostics/source_store.rs`) — registro global `FileId → (path, texto, line_starts)` para renderização de snippets
- `Span` estendido com `file: Option<FileId>` (`src/parser/span.rs`)
- `parse_source_with_file` — parser popula `FileId` em todos os spans via `assign_file_to_program` pós-lowering
- `HirImport`, `HirClass`, `HirFunction` agora populam `loc` com spans reais (eram `None`)
- `import_resolver.rs` emite `E001`-`E004` com span, notes e sugestão Levenshtein contra builtins + dependências
- `type_system/checker.rs` emite `E010`-`E014` com span, notes e sugestão Levenshtein contra exports
- `typed_build.rs` emite `W001`-`W011` para cada `RuntimeEval` após lowering, categorizando (for-in, for-of, try, throw, async, arrow, template, switch, class)
- CLI (`render_compiler_error`) renderiza o engine antes do fallback de trace route; warnings são impressos mesmo em compilação bem-sucedida

**Pendente para etapas futuras:**
- D5 (runtime errors com PC→source via .ometa) — depende de D1 + Etapa 1
- D6 (validações extras: var antes de declarar, função duplicada, aridade errada) — depende de AST estruturado no HIR (Etapa 6)
- `DiagnosticEngine` ainda aborta no primeiro erro de cada módulo — arquitetura pronta para coletar múltiplos, falta refatorar cada caminho `bail!` para continue-on-error
- Tipos não resolvidos não emitem warning (ruído alto sem suporte a generics no type system)

---

**ETAPA 0 original (descrição preservada para contexto histórico):**

**Motivação:** o sistema atual emite erros sem contexto de fonte. Um `bail!("unknown module import: ssh2")` não diz em qual arquivo está, em qual linha, nem o que estava ao redor. Qualquer construção não suportada (`RuntimeEval`) é compilada silenciosamente sem avisar o usuário. Tipos não resolvidos viram `any` sem nenhum aviso. Isso torna o desenvolvimento no RTS fundamentalmente opaco.

Esta é a etapa mais urgente — bloqueia a usabilidade de todas as outras.

**Estado atual do sistema de diagnósticos:**

```
Diagnostic { severity, message, span: Option<Span> }
  └── span quase nunca preenchido na prática
render() → "error: message at line:col"
  └── sem snippet de código, sem seta apontando o problema
suggestions.rs → apenas 2 padrões cobertos
attach_trace() → trace de imports, só em --dump-statistics
check_imports() → bail!("unknown module import: X")
  └── sem arquivo, sem linha, sem coluna
TypeAnnotation::unresolved() → silenciosamente vira "any"
RuntimeEval → emitido sem nenhum warning ao usuário
render_compiler_error() em prod → fingerprint hash ilegível
```

**O que precisa ser construído:**

**D1 — Erros com localização de fonte**

Todo erro de compilação deve incluir: arquivo, linha, coluna e o trecho de código com marcação visual.

```
error[E001]: módulo não encontrado
  --> src/main.ts:3:22
   |
 3 | import { ConnectSSH } from "ssh2";
   |                            ^^^^^^ módulo "ssh2" não está instalado
   |
   = sugestão: adicione "ssh2" em rtslibs no package.json
```

Requer que `check_import()` receba a `Span` do import (disponível no AST do SWC) e a propague no `Diagnostic`.

**D2 — Warnings para construções não suportadas (RuntimeEval)**

Toda instrução `MirInstruction::RuntimeEval` emitida pelo `typed_build` deve gerar um `Diagnostic::warning` com a linha original:

```
warning[W001]: construção não compilada nativamente (RuntimeEval)
  --> src/main.ts:12:5
   |
12 |   for (const key in obj) {
   |   ^^^^^^^^^^^^^^^^^^^^^^ for-in não tem suporte nativo — usando avaliação dinâmica
   |
   = note: performance degradada neste trecho
```

**D3 — Tipos não resolvidos como erro explícito (não silent any)**

`TypeAnnotation::unresolved()` deve emitir `Diagnostic::warning` em modo development. Em modo production deve ser `error` quando o tipo afeta o codegen (parâmetros de função, retorno).

```
warning[W002]: tipo não reconhecido
  --> src/main.ts:7:18
   |
 7 | function greet(name: MyCustomType): void {
   |                      ^^^^^^^^^^^^ tipo "MyCustomType" não resolvido — tratado como any
```

**D4 — Sugestão "você quis dizer?" em imports**

Quando um import falha, calcular distância de Levenshtein contra os exports disponíveis e sugerir a correção mais próxima:

```
error[E002]: símbolo não exportado
  --> src/main.ts:1:10
   |
 1 | import { connetSSH } from "rts:ssh2";
   |          ^^^^^^^^^ "connetSSH" não exportado por "rts:ssh2"
   |
   = sugestão: você quis dizer "connectSSH"?
```

**D5 — Erros de runtime com localização TS (PC → source)**

Quando o binário compilado (AOT) lança um panic, o traceback deve mostrar a linha TS original via `.ometa`:

```
runtime error: null pointer dereference
  --> src/main.ts:45:12  (via rts.debug.resolve_location)
   |
45 |   const result = db.query(null);
   |            ^^^^^^^^^^^^^^^^^^^^
   |
   stack: main → handleRequest → db.query
```

O `rts.debug.resolve_location` já existe na ABI, mas não está conectado ao handler de panic do runtime.

**D6 — Validações que faltam completamente**

| Validação | Onde adicionar | Severidade |
|---|---|---|
| Variável usada antes de declarar | `typed_build.rs` | error |
| Função duplicada no mesmo módulo | `type_system/checker.rs` | error |
| Import de arquivo que não existe em disco | `module/import_resolver.rs` | error (atualmente panic) |
| Chamada com aridade errada | `typed_build.rs` | warning |
| Retorno de tipo incompatível | `type_system/checker.rs` | warning |
| Export não encontrado no módulo de destino | `type_system/checker.rs` | error (já existe mas sem span) |

**D7 — DiagnosticEngine centralizado**

Em vez de erros espalhados via `bail!()` e `anyhow`, introduzir um `DiagnosticEngine` que:
- Coleta múltiplos diagnósticos sem interromper no primeiro erro
- Classifica por severidade (errors bloqueiam o build, warnings não)
- Formata output com cores (ANSI) no terminal
- Emite JSON em modo `--dump-statistics` para consumo por tooling externo

```rust
// Estrutura proposta
pub struct DiagnosticEngine {
    pub diagnostics: Vec<RichDiagnostic>,
}

pub struct RichDiagnostic {
    pub code: &'static str,       // "E001", "W002"
    pub severity: Severity,
    pub message: String,
    pub primary_span: FileSpan,   // arquivo + linha + coluna
    pub labels: Vec<SpanLabel>,   // spans secundários com texto
    pub notes: Vec<String>,       // "sugestão: ..."
    pub suggestion: Option<Suggestion>, // correção automática opcional
}

pub struct FileSpan {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub source_snippet: String,   // a linha do código fonte
}
```

**Arquivos afetados:**
- `src/diagnostics/reporter.rs` — `Diagnostic` → `RichDiagnostic`, adicionar `DiagnosticEngine`
- `src/diagnostics/suggestions.rs` — Levenshtein + sugestões contextuais
- `src/type_system/checker.rs` — propagar spans do AST SWC
- `src/module/import_resolver.rs` — erros de arquivo não encontrado com path completo
- `src/mir/typed_build.rs` — emitir warning em cada `RuntimeEval` gerado
- `src/hir/lower.rs` — propagar `SourceLocation` para todos os nós
- `src/cli/mod.rs` — `render_compiler_error` usar `DiagnosticEngine`

**Benchmark alvo:**

| Cenário | Antes | Depois |
|---|---|---|
| Import inválido | panic sem contexto | erro com arquivo + linha + snippet |
| Tipo não resolvido | silencioso | warning com sugestão |
| RuntimeEval por construção não suportada | silencioso | warning com linha TS |
| Tempo de build com diagnósticos | baseline | +2ms máx (coleta de spans) |
| Erros coletados antes de parar | 1 (primeiro bail!) | N (todos antes de abortar) |

---

### ETAPA 1 — Runtime com cache em disco + flag `--watch`

**Motivação:** `rts run` recompila tudo do zero a cada execução. Para projetos com múltiplos módulos ou dependências, isso desperdiça ciclos de CPU em código que não mudou.

**O que muda:**

O `rts run` passa a gravar objetos em disco no mesmo formato do `rts compile`, na pasta `node_modules/.rts/objs/`. Na segunda execução, módulos com `.ometa` válido são carregados direto do `.o` sem passar por HIR/MIR/Cranelift.

```
node_modules/
  .rts/
    objs/
      runtime.namespace_io.o
      runtime.namespace_io.ometa
      runtime.namespace_fs.o
      runtime.namespace_fs.ometa
      <app_module>.o
      <app_module>.ometa
```

**Flag `--watch` / `-w`:**
- Inicia a execução normalmente
- Registra watchers nos arquivos `.ts` do grafo de módulos
- Ao detectar modificação: invalida `.ometa` do módulo afetado, recompila apenas ele, reinicia o processo
- Sai com `Ctrl+C`

**Benchmark alvo:**

| Cenário | Antes (run frio) | Depois (run cache hit) | Melhoria esperada |
|---|---|---|---|
| Projeto 5 módulos | ~80ms | ~20ms | ~75% |
| Projeto 10 módulos | ~150ms | ~30ms | ~80% |
| Watch: mudança 1 arquivo | N/A | ~25ms (recompila só o afetado) | — |

**Bug crítico a corrigir junto: cache de dependências transitivas**

`is_cached_object_valid` valida apenas o hash do **próprio módulo**. Se `b.ts` muda e `a.ts` importa de `b.ts`, o `.o` de `a.ts` não é invalidado. O compilador reusa o objeto stale de `a.ts` linkado contra o novo `b.ts` — pode causar link errors silenciosos ou comportamento incorreto sem nenhum aviso.

A correção: `ObjectCacheMeta` precisa incluir um `deps_hash: String` — hash composto dos hashes de todos os módulos importados transitivamente. Se qualquer dependência mudar, o hash muda e o cache é invalidado.

```rust
pub(crate) struct ObjectCacheMeta {
    pub(crate) source_hash: String,
    pub(crate) deps_hash: String,   // novo: hash XOR/SHA dos módulos importados
    // ...
}
```

**Arquivos afetados:**
- `src/cli/run.rs` — adicionar lógica de cache + watcher
- `src/cache.rs` — `ObjectCacheMeta` + `deps_hash` + `is_cached_object_valid` atualizado
- `src/pipeline.rs` — extrair `emit_run_objects()`, calcular `deps_hash` por módulo
- `src/module/mod.rs` — expor lista de paths do grafo para o watcher e para deps_hash

---

### ETAPA 2 — Renomear `--debug` para `--dump-statistics` / `-ds`

**Motivação:** `--debug` é um nome amplamente esperado pelos usuários para controlar saída de debug do **programa TypeScript** sendo executado (ex: `console.debug`, guards de debug no userland). Usar `--debug` para métricas internas do RTS cria colisão semântica.

**O que muda:**

| Atual | Novo |
|---|---|
| `--debug` / `-D` | `--dump-statistics` / `-ds` |
| Ativa timeline, JIT report, dispatch metrics | Mesmo comportamento |

O campo `CompileOptions::debug` permanece internamente, apenas o parse do CLI muda. O `--debug` antigo deve ser aceito com aviso de deprecação por 1 versão.

**Impacto:** `src/cli/mod.rs` (parse_flags), `src/compile_options.rs` (doc), `src/cli/run.rs` (print_debug_timeline).

**Benchmark:** Zero impacto em runtime. Mudança puramente de interface.

---

### ETAPA 3 — Fragmentação do dispatch por namespace

> [obsoleto — nao se aplica ao contrato ABI atual]
>
> A branch `feat/remake-namespaces` ja nao tem dispatch unificado.
> Cada funcao de namespace e um simbolo proprio `__RTS_FN_NS_<NS>_<NAME>` gerado a partir de `src/abi/`. O objetivo desta etapa (visibilidade por namespace no profiler, simbolos independentes) ja e inerente ao contrato atual — nao ha mais `rts_namespace_dispatch.o` a fragmentar. Mantido abaixo como historico.

**Motivação:** `rts_namespace_dispatch.o` é um objeto monolítico. Se um namespace estiver causando latência, não há como isolá-lo no profiler sem instrumentação manual.

**O que muda:**

Em vez de um único `rts_namespace_dispatch.o`, o compile/run gera um objeto por namespace usado:

```
node_modules/.rts/objs/
  runtime.namespace_io.o        + .ometa
  runtime.namespace_fs.o        + .ometa
  runtime.namespace_net.o       + .ometa
  runtime.namespace_crypto.o    + .ometa
  ...
```

Cada objeto exporta apenas os símbolos `__rts_<ns>_<fn>` do seu namespace. O linker une tudo no binário final — sem custo em runtime, mas com visibilidade total de tamanho e símbolos por namespace no profiler.

O `build_namespace_dispatch_object()` em `object_builder.rs` é refatorado para aceitar um único namespace por vez, chamado em loop em `emit_selected_namespace_objects()`.

**Benchmark alvo:**

| Cenário | Antes | Depois |
|---|---|---|
| Tamanho objeto dispatch unificado | 1 × N KB | N objetos menores |
| Tempo de link | baseline | +2ms máx (mais objetos) |
| Visibilidade no profiler | nenhuma | símbolo por namespace |

---

### ETAPA 4 — `--dump-statistics` ampliado

**Motivação:** a timeline atual mostra apenas tempo de estágio. Não mostra o que o compilador viu, o que o GC fez, ou quais construções não foram reconhecidas.

**Novas seções no output de `--dump-statistics`:**

```
=== Análise de Fonte ===
  Declarações detectadas        12
  Funções declaradas             5
  Classes                        2
  Imports                        3
  Construções não suportadas     1   (RuntimeEval fallbacks)
    - line 42: ForInStatement → RuntimeEval

=== GC Arena ===
  Alocações                    847
  Coletas (collect_all)          3
  Coletas (collect_debt)        12
  Live slots ao final            4
  Bytes alocados (total)     2.4 KB

=== ValueStore (abi) ===
  Handles alocados             341
  Bindings declarados           18
  Estados não rastreados pelo GC    (veja nota)

=== Namespaces usados ===
  io                (3 callees)
  net               (7 callees)

=== Timeline (ms) ===
  graph_load         2.1
  ...
```

A linha "Construções não suportadas" lista cada `RuntimeEval` gerado com a linha do fonte original, ajudando a identificar o que o compilador ainda não cobre nativamente.

**Arquivos afetados:** `src/cli/run.rs`, `src/namespaces/abi.rs`, `src/namespaces/gc/collect.rs`, `src/mir/typed_build.rs` (contar fallbacks).

---

### ETAPA 5 — Migração de `target/` para `node_modules/.rts/`

**Motivação:** usar `target/` como pasta de artefatos viola convenções do ecossistema JS/TS. Ferramentas como VS Code, `tsc`, `eslint`, `prettier` ignoram `node_modules/` por padrão. Mover os artefatos para `node_modules/.rts/` os torna invisíveis para essas ferramentas e segue o padrão de ferramentas como Vite, Parcel e esbuild.

**Nova estrutura:**

```
node_modules/
  .rts/
    objs/
      runtime.namespace_io.o
      runtime.namespace_io.ometa
      <app_module>.o
      <app_module>.ometa
    builtin/
      node:fs/
        main.ts           — implementação TS da stdlib fs
        index.d.ts
      fs/                 — alias de node:fs
        main.ts
      path/
        main.ts
        index.d.ts
      rts-types/
        rts.d.ts          — gerado por rts emit-types
    tsconfig.json         — config interna do rts (paths + refs)
```

**`tsconfig.json` gerado pelo RTS (em `node_modules/.rts/tsconfig.json`):**

```json
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "node:fs":   ["./builtin/node:fs/main.ts"],
      "fs":        ["./builtin/fs/main.ts"],
      "path":      ["./builtin/path/main.ts"]
    },
    "types": ["./builtin/rts-types/rts.d.ts"]
  }
}
```

**`tsconfig.json` no projeto do usuário (gerado por `rts init`):**

```json
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "node:fs": ["./node_modules/.rts/builtin/fs/main.ts"],
      "fs":      ["./node_modules/.rts/builtin/fs/main.ts"]
    },
    "types": ["./node_modules/.rts/builtin/rts-types/rts.d.ts"]
  },
  "references": [{ "path": "./node_modules/.rts/tsconfig.json" }]
}
```

**Binário compilado:** sai na raiz do projeto, sem `target/release/`:

```
<project>/
  my_app.exe        — rts compile src/main.ts my_app
```

**Consumo de `packages/rts-types` pelo usuário:**

Hoje `packages/rts-types` existe no repositório do RTS mas não há mecanismo para um projeto de usuário consumi-lo. O `rts init` deve copiar (ou symlinkar) os `.d.ts` para `node_modules/.rts/builtin/rts-types/` e gerar o `tsconfig.json` apontando para eles. Enquanto a Etapa 5 não sair, qualquer projeto novo precisa configurar os paths de tipo manualmente — o que quebra o IntelliSense do VS Code para quem usa `import { ... } from "rts"`.

**Arquivos afetados:**
- `src/pipeline.rs` — `module_object_stem`, `emit_selected_namespace_objects`, `compile_graph`
- `src/cli/run.rs` — `execute_with_report` (deps_dir)
- `src/cli/init.rs` — gerar tsconfig + copiar rts-types para node_modules/.rts/builtin/
- `src/module/mod.rs` — resolver builtin paths

**Benchmark:**

| Aspecto | Antes | Depois |
|---|---|---|
| Integração com tsc/vscode | warnings de paths | automático via tsconfig |
| Tempo de resolução de módulos | baseline | mesmo |
| `gitignore` necessário | `target/` | `node_modules/` (já ignorado) |

---

### ETAPA 6 — Fragmentação de `typed_build.rs` e `typed_codegen.rs`

> [obsoleto — nao se aplica ao contrato ABI atual]
>
> Os arquivos `typed_build.rs` e `typed_codegen.rs` foram removidos junto com
> as camadas HIR/MIR. O codegen hoje ja e fragmentado em `src/codegen/lower/`
> e consome o AST do SWC diretamente — nao existe mais o ciclo
> parse→string→re-parse descrito abaixo. Mantido como historico.

**Motivação:** ambos os arquivos têm > 600 linhas e cobrem responsabilidades heterogêneas (literais, binops, loops, classes, campos, SIMD). Dificulta revisão, testes unitários e adição de novos constructs.

**Raiz do problema `RuntimeEval` — o ciclo parse→string→re-parse:**

O `hir/lower.rs` recebe o AST rico do SWC e serializa statements e expressões de volta para strings (`HirFunction.body: Vec<String>`, `HirItem::Statement(String)`). O `typed_build.rs` então re-parseia essas strings com um mini-parser próprio. Tudo que esse mini-parser não reconhece vira `RuntimeEval`.

```
SWC AST (rico, com spans, tipos, nós estruturados)
    ↓  hir/lower.rs
HIR: body = Vec<String>   ← estrutura descartada, volta a ser texto
    ↓  mir/typed_build.rs
MIR: mini-parser relê o texto → construção não suportada → RuntimeEval
```

O SWC já fez o trabalho pesado de parsear TypeScript corretamente. Jogar fora essa estrutura e reparsar texto é o motivo direto pelo qual `for-in`, alguns patterns de destructuring, e outras construções válidas em TS caem em fallback opaco.

A fragmentação desta etapa deve caminhar junto com a **substituição de `HirItem::Statement(String)` por nós estruturados** que preservam o AST do SWC até o MIR, eliminando o segundo parsing e reduzindo a superfície de `RuntimeEval` drasticamente.

**Nova estrutura:**

```
src/mir/
  typed/
    mod.rs          — re-exports + entry point typed_build()
    constants.rs    — ConstantPool, ConstNumber, ConstString, ConstBool
    binop.rs        — BinOp, UnaryOp, comparações
    control.rs      — if/else, loops (while, for, for-of), break/continue
    calls.rs        — Call, InlineCall, RuntimeEval
    classes.rs      — NewInstance, LoadField, StoreField, constructor
    functions.rs    — declaração de função, parâmetros, return
    imports.rs      — Import, resolução de módulo
    scope.rs        — Bind, LoadBinding, WriteBind

src/codegen/
  cranelift/
    typed/
      mod.rs        — re-exports + entry point compile_typed_function()
      prologue.rs   — assinatura, parâmetros, BindingState
      constants.rs  — emissão de literais (ConstNumber → iconst/f64const)
      binop.rs      — emissão de BinOp nativos vs dispatch
      control.rs    — blocos, jumps, JumpIf, JumpIfNot
      calls.rs      — call_known (fn_id direto), call_dispatch, RuntimeEval
      classes.rs    — NewInstance, LoadField, StoreField
      strings.rs    — ConstString, data sections, box_string
```

**Regra:** nenhum arquivo do grupo `typed/` usa pré-declarações `unsafe` ou truques de lifetime para evitar borrow. O gc-arena requer que não haja ponteiros `Gc<'gc, _>` vivos durante chamadas de mutação — qualquer hack que "esconde" referências pode quebrar a invariante de coleta.

**Benchmark alvo:**

| Métrica | Antes | Depois |
|---|---|---|
| Linhas/arquivo max (typed_build) | ~650 | ~120 |
| Linhas/arquivo max (typed_codegen) | ~900 | ~150 |
| Tempo de `cargo build` (incremental) | baseline | -15% (menos recompilação) |
| Cobertura de testes unitários | baixa | por módulo |

---

### ETAPA 7 — GC Arena com cobertura real do ValueStore

**Motivação atual:** o `ValueStore` (em `namespaces/abi.rs`) acumula `RuntimeValue`s num `Vec<RuntimeValue>` por thread. Nunca é compactado. Em `rts run` de servidor HTTP, cada requisição aloca handles sem liberar, causando crescimento constante de RAM.

**Diagnóstico:**

```
ValueStore {
    values: Vec<RuntimeValue>,     // nunca shrinks
    bindings: FxHashMap<...>,      // nunca é coletado entre requests
}
```

O gc-arena atual só cobre `GcBlob`s alocados via `gc.alloc()` — não alcança `ValueStore`.

**Proposta:**

1. **`ValueStore` com geração:** adicionar contador de geração ao `ValueStore`. Em `safe_collect()`, resetar o store para a geração anterior (liberar handles sem referências externas)
2. **Rastrear handles externos:** introduzir `LiveSet` — conjunto de handles que o código TS "segurou" via binding. Handles fora do `LiveSet` ao sair do escopo top-level são elegíveis para coleta
3. **Integração com `exit_scope()`:** `exit_scope()` já dispara coleta da arena — deve também compactar o `ValueStore` nos handles não-vivos
4. **Métricas expostas em `--dump-statistics`:** `handles_freed`, `store_compactions`, `store_peak_slots`

**Atenção: thread-safety com rayon**

`run.rs` usa `par_iter` (rayon) para compilar módulos em paralelo. Namespaces como `net` usam `OnceLock<Arc<Mutex<NetState>>>` — compartilhado entre threads, correto. Mas o `ValueStore` e o GC arena são `thread_local!` — cada thread rayon tem instâncias separadas. O dispatch de namespace chamado de dentro do JIT executa na thread que compilou o módulo, não na thread principal. Antes de implementar a compactação do `ValueStore` nesta etapa, mapear quais namespaces têm estado global vs thread-local para garantir que a coleta não libere handles ainda vivos em outra thread.

**Benchmark alvo:**

| Cenário | Antes | Depois |
|---|---|---|
| RAM após 1000 req HTTP | +200MB | estável ~5MB |
| RAM após 10k req | crescimento linear | plateau |
| Overhead de coleta por req | 0 (sem coleta) | < 0.1ms |

---

### ETAPA 8 — Correção dos .d.ts gerados

**Problemas detectados:**

1. `export namespace rts.natives { ... }` e `export namespace rts.hotops { ... }` — **sintaxe inválida em TypeScript**. Namespaces com ponto no nome não são permitidos. Deve ser `rts_natives` ou movido para dentro de `namespace rts { namespace natives { ... } }`

2. `export function args(): globalThis.Array<str> | str` — `globalThis.Array` não é acessível dentro de `declare module "rts"` sem import explícito

3. `export type Handle = usize` em `buffer` e `promise` — `usize` não é um tipo base TS, precisa de `= number`

4. Interfaces duplicadas: `WritableStream`, `ReadableStream`, `FileHandle` declaradas em `rts.d.ts` e repetidas nos arquivos split

**Correções:**

```typescript
// Antes (inválido)
export namespace rts.natives { ... }

// Depois (válido)
export namespace rts {
  export namespace natives { ... }
  export namespace hotops { ... }
  export namespace debug { ... }
}
```

**Arquivos afetados:** `src/namespaces/rust/mod.rs` (render_typescript_declarations), `src/runtime/mod.rs`.

---

### ETAPA 9 — Revisão de dead code em HIR / MIR / Codegen

> [obsoleto — nao se aplica ao contrato ABI atual]
>
> HIR e MIR nao existem mais na branch atual. A revisao de dead code agora se
> aplica a `src/codegen/lower/` e `src/abi/`. Mantido como historico.

**HIR:**
- `HirFunction.body: Vec<String>` — serializar statements para strings e reparsar no MIR é perda de informação. Candidato a substituição por `Vec<HirStatement>` estruturado quando o SWC AST for usado diretamente
- `HirItem::Statement(String)` — wrapper para texto arbitrário, impede análise sem reparsing

**MIR:**
- `SimdConst`, `SimdOp`, `SimdLoad`, `SimdStore` — declarados em `MirInstruction` mas **o codegen emite `todo!()` ou ignora silenciosamente** para a maioria dos casos
- `UnrollHint`, `LoopBegin`, `LoopEnd`, `HoistInvariant`, `InlineCandidate` — hints de otimização sem consumidor no codegen atual
- `StrengthReduce` — não tem match arm no codegen typed

**Ação:** por enquanto, adicionar `#[allow(dead_code)]` explícito com comentário `// WIP: pendente implementação em codegen` em vez de deixar warnings silenciosos. Remover completamente quando confirmado que nenhum path os produz.

**Codegen:**
- `CALLEE_FN_IDS` contém apenas 10 entradas dos ~25 FN_* disponíveis. Callees como `FN_BOX_STRING`, `FN_UNBOX_NUMBER`, `FN_IS_TRUTHY` são gerados por lógica inline no codegen mas não constam na tabela de lookup — inconsistência que pode causar dupla emissão

---

### Subcomandos faltantes

Três subcomandos que o ecossistema espera e que o RTS ainda não tem:

| Comando | Por que falta | Urgência |
|---|---|---|
| `rts check` | Type-check sem compilar. Essencial para CI e integração com editores. Hoje a única forma de verificar erros é rodar `rts run` e ver se compila. | Alta — depende da Etapa 0 |
| `rts clean` | Limpar o cache `node_modules/.rts/objs/`. Sem isso, cache corrompido ou inválido não tem solução clara para o usuário. Necessário assim que Etapa 1 entrar. | Alta — depende da Etapa 1 |
| `rts fmt` | Formatador de código TS. Baixa prioridade funcional, mas esperado pela comunidade. Pode delegar ao SWC que já tem formatter. | Baixa |

`rts check` e `rts clean` devem entrar nas etapas 0 e 1 respectivamente, não no backlog.

---

### Features Futuras (backlog)

| ID | Feature | Dependência |
|---|---|---|
| F-W1 | `rts watch` como subcomando dedicado (não apenas flag) | Etapa 1 |
| F-T1 | Source maps: `.ometa` com mapeamento PC → linha TS | Etapa 6 |
| F-G1 | GC compactador: mover slots para eliminar fragmentação | Etapa 7 |
| F-N1 | Fragmentação ABI: `__rts_io_print(ptr, len)` direto sem dispatch | Etapa 3 |
| F-D1 | `rts debug <file>` — executa com breakpoints simbólicos via `.ometa` | F-T1 |
| F-B1 | Builtins TS em `node_modules/.rts/builtin/` (`fs`, `path`, `http`) | Etapa 5 |
| F-C1 | Construtor de classes com herança (extends) | Etapa 6 |
| F-A1 | async/await nativo via continuations no MIR | F-C1 |
| F-L1 | LSP server para VS Code (go-to-definition, hover types) | Etapa 0 + F-T1 |
| F-R1 | `.rtslib` — namespaces externos via crate Rust compilado | Etapa 3 |

---

## Convenção de atualização

- Ao concluir uma etapa: marcar com `[x]` no título e registrar benchmark real
- Ao descobrir novo problema: adicionar na etapa mais próxima ou criar nova
- Benchmarks reais substituem os "alvo" quando medidos
