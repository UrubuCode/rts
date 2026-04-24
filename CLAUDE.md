# CLAUDE.md

## Projeto

RTS e um compilador/runtime TypeScript-to-native usando Cranelift como backend de codegen.
O objetivo e compilar TS/JS para binarios nativos com runtime minimo em Rust, distribuido como
toolchain standalone (sem runtime support library externa).

A branch atual (`feat/remake-namespaces`) reorganiza a camada de runtime em torno do novo
contrato `src/abi/` + `SPECS`, e converge o pipeline com o modelo da `main` (grafo de modulos
+ cache incremental), preservando a superficie nova da ABI.

Consultar `NEXT_STEPS.md` e `ROAD_MAP.md` para a direcao vigente.

## Arquitetura

```
src/
  abi/          — contrato unico de ABI (SPECS, tipos, simbolos, guards, assinaturas)
  codegen/      — Cranelift codegen (object emit, lower de expr/stmt/func)
  linker/       — link nativo (system linker com fallback)
  namespaces/   — implementacoes dos namespaces runtime: io, fs, gc
  runtime/      — builtin module "rts" + submodulos "rts:<ns>"
  module/       — resolver de modulos e grafo de dependencias
  parser/       — SWC parse + AST interno
  type_system/  — type checker, registry, resolver
  diagnostics/  — erros estruturados
  cli/          — CLI (run, compile, apis, repl, eval)
  pipeline.rs   — orquestra build/run
  lib.rs        — API publica
  runtime_lib.rs — resolucao do artefato de runtime support (librts.a / rts.lib)
  main.rs       — entrypoint do binario `rts`
```

Pipeline: `Source TS → Parser(SWC) → type_system → codegen(Cranelift) → Object → Linker → .exe`

Nao existem mais camadas HIR/MIR separadas nesta branch — o codegen consome direto a AST
(com tipagem resolvida) e emite Cranelift IR em `src/codegen/lower/`.

## ABI (`src/abi/`) — contrato unico

Toda a superficie entre codegen e runtime passa por `src/abi/`. Nao existe mais
`SPEC/MEMBERS/dispatch()` por namespace e nao existe mais `__rts_call_dispatch`.

- `abi::SPECS` (`mod.rs`) — slice estatico com a `NamespaceSpec` de cada namespace registrado
  (`io`, `fs`, `gc`). E a fonte unica consumida por codegen, runtime e gerador de `rts.d.ts`.
- `abi::lookup(qualified)` — resolve `"io.print"` → `&NamespaceMember` com simbolo e assinatura.
- `member.rs` — `NamespaceSpec` e `NamespaceMember` como tabelas `const` estaticas. Cada membro
  declara `name`, `kind` (Function|Constant), `symbol`, `args[]`, `returns`, `doc`, `ts_signature`.
- `types.rs` — `AbiType`: `Void | Bool | I32 | I64 | U64 | F64 | StrPtr | Handle`. `StrPtr`
  expande para dois slots Cranelift (`ptr` + `len`).
- `signature.rs` — `lower_member()` converte a spec em `LoweredSignature` Cranelift.
- `symbols.rs` — convencao `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` (ex: `__RTS_FN_NS_IO_PRINT`).
  Macro `rts_sym!` gera simbolos em compile-time; `validate_symbol()` impoe uppercase ASCII.
- `guards.rs` — `guard_for(expected, caller)` decide passthrough/coerce/trap em call sites
  com argumentos de tipo `any`.

Codegen emite `call <symbol>` direto via Cranelift, sem intermediarios.

## Estrutura de Arquivos por Namespace

```
src/namespaces/<ns>/
  mod.rs         — re-exporta submodulos e publica a NamespaceSpec
  abi.rs         — declaracao dos NamespaceMember (tabela estatica)
  <grupo>.rs     — impl operacional (ex: read.rs, write.rs, dir.rs, print.rs, stdout.rs, ...)
```

Regras:
- `mod.rs` e apenas o import map + export do `NamespaceSpec`
- `abi.rs` e a fonte da verdade dos membros do namespace (nome, simbolo, args, return, doc, ts)
- Cada arquivo operacional agrupa funcoes por responsabilidade (io/r-w/dir/metadata/…)
- Nao existe `dispatch()` por namespace — cada funcao e um `#[no_mangle] extern "C"` direto

Namespaces ativos nesta branch: `io`, `fs`, `gc`.
Os demais (net, process, crypto, buffer, promise, task, global) estao removidos aqui e serao
reintroduzidos sobre o contrato novo a medida que o pipeline principal estabiliza.

### Namespaces existentes

- `io/` — print, eprint, stdout_{write,flush}, stderr_{write,flush}, stdin_{read,read_line}
- `fs/` — read, read_all, write, append, exists, is_file, is_dir, size, modified_ms,
  create_dir(_all), remove_dir(_all), remove_file, rename, copy
- `gc/` — handles e string pool: string_from_{i64,f64,static}, string_{new,concat,len,ptr,free},
  `HandleTable` slab-based com 16-bit geracao + 48-bit slot (`u64` handle)

## Convencoes

- Linguagem do codigo: Rust (ingles nos identificadores)
- Linguagem de comunicacao: portugues
- Commits seguem conventional commits: `feat:`, `fix:`, `perf:`, `refactor:`, `docs:`, `chore:`
- Novo namespace precisa ser registrado em: `abi::SPECS` (e o `rts.d.ts` gerado a partir dai)
- O `rts.d.ts` e gerado a partir de `abi::SPECS` — CI lintao committed file contra o gerador
- Build e via `cargo` direto — `xtask` foi removido

## Como testar

```bash
cargo test                                        # testes unitarios
cargo build --release                             # build release
target/release/rts.exe run file.ts                # executar (runtime)
target/release/rts.exe compile -p file.ts output  # compilar nativo (AOT)
target/release/rts.exe apis                       # listar APIs disponiveis
```

## Benchmarks

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```

Compara RTS (run), RTS (compiled), Bun e Node.

## Regras

- Nao implementar APIs de alto nivel em Rust — Rust so expoe primitivas raw via `"rts"`
- Packages TS em `builtin/*` constroem APIs ergonomicas sobre o `"rts"`
  (nesta branch: `console/`, `globals/`, `rts-types/`)
- `rts.d.ts` so contem `declare module "rts"` — nao adicionar outros modulos
- Handles numericos (u64) para recursos runtime (buffers, sockets, strings dinamicas, etc)
- Distribuicao standalone: runtime support resolvido por `runtime_lib.rs` (cache toolchain
  local `~/.rts/...` ou artefato em `target/{debug,release}/`); nao dependemos de download
  externo em tempo de build

## Sem Codigo Legacy

**Regra absoluta: codigo morto e removido imediatamente. Nunca comentar, nunca deixar "por precaucao".**

- Qualquer codigo que nao e chamado por nenhum caminho vivo deve ser deletado no mesmo commit
  que o tornou morto
- Stubs `todo!()` / `unimplemented!()` sao aceitaveis como marcador temporario de WIP;
  codigo comentado nao
- Warnings de `dead_code` sao tratados como erros — o build nao pode terminar com warnings

## ABI de Maquina — extern "C" tipado, sem dispatch

Nao ha `JsValue`, nem `__rts_call_dispatch`, nem boxing no limite entre codegen e runtime.
Cada funcao de namespace e um simbolo `extern "C"` tipado.

### Convencao ABI por tipo

| Tipo TS  | `AbiType`    | Representacao Cranelift         | Observacao                                              |
|----------|--------------|---------------------------------|---------------------------------------------------------|
| `number` | `I64` / `F64`| `i64` / `f64`                   | bits nativos, sem boxing                                |
| `bool`   | `Bool`       | `i8` (0/1)                      | 0 = false, 1 = true                                     |
| `string` | `StrPtr`     | 2 slots: `(i64 ptr, i64 len)`   | UTF-8; ptr estatica do codegen, ou buffer via handle GC |
| handle   | `Handle`     | `u64`                           | `HandleTable` (gen:16 + slot:48)                        |
| void    | `Void`       | —                               | sem retorno                                             |
| inteiros| `I32` / `U64`| `i32` / `u64`                   | usados em contagens, status, tamanhos                   |

### Regras de implementacao

- Cada membro de namespace vira um `#[unsafe(no_mangle)] pub extern "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`
- Nenhuma funcao de namespace aceita/retorna `JsValue` no limite `extern "C"`
- Strings dinamicas (ex: resultado de leitura) sao alocadas pelo `gc` e retornam um handle `u64`;
  leitura via `gc::string_ptr(handle)` + `gc::string_len(handle)`
- Call sites com argumentos `any` passam por `abi::guards::guard_for(...)` para decidir coerce/trap

## Runtime vs Compile (AOT)

Runtime e AOT sao unificados — ambos geram `.o` via Cranelift. A diferenca e de escopo:

- **Runtime (`rts run`)**: inclui todos os modulos builtin nos objects
- **Compile (`rts compile`)**: aplica slicing, gera apenas os objects dos modulos efetivamente
  usados; produz o binario final

O pipeline de codegen e o mesmo nos dois casos. Convencao de nomes: `<module>.o` (e `.m` quando
houver metadata associado para cache incremental).

## Layout de Artefatos do Usuario

Alvo da Fase 1 do roadmap (em progresso):

```
<project>/
  src/main.ts
  package.json
  tsconfig.json

  node_modules/.rts/
    objs/
      runtime/        — objects completos do builtin (todos os modulos)
      compile/        — objects AOT com slicing (apenas em rts compile)
    modules/          — modulos resolvidos e cacheados (com metadata .ometa)

  release/            — apenas em rts compile
    <project_name>    — .exe / .dll / .so / .node conforme target
```

## GC — gc-arena (coleta deterministica)

Usar o crate `gc-arena` como sistema de GC deterministico. Coleta e disparada apos:
- Retorno de funcoes
- Execucao de metodos de classe
- Fim de escopo de closures

Principio: `safe_collect()` e chamado em pontos de quiescencia bem definidos, nao de forma
periodica/assincrona. O namespace `gc` publica a API de alocacao de strings e o `HandleTable`.

## State

Estado de namespace usa `Arc<Mutex<T>>` direto quando necessario, ou `thread_local!` para caches
por-thread. Nao ha sistema centralizado de state — cada namespace gerencia o seu.

### Pattern para estado compartilhado

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

static FS_STATE: std::sync::OnceLock<Arc<Mutex<FsState>>> = std::sync::OnceLock::new();

fn fs_state() -> Arc<Mutex<FsState>> {
    FS_STATE.get_or_init(|| Arc::new(Mutex::new(FsState::default()))).clone()
}

#[derive(Default)]
struct FsState {
    open_files: HashMap<u64, std::fs::File>,
}
```

### Pattern para caches thread-local

```rust
use std::cell::RefCell;

thread_local! {
    static EXPR_CACHE: RefCell<HashMap<u64, Expression>> = RefCell::new(HashMap::new());
}

pub fn reset_cache() {
    EXPR_CACHE.with(|cache| cache.borrow_mut().clear());
}
```

## Docs e especificacoes

A pasta `docs/specs/` contem especificacoes de features, decisoes de design e notas tecnicas.
Consultar o indice em `docs/specs/INDEX.md`. Direcao de alto nivel fica em `NEXT_STEPS.md` e
`ROAD_MAP.md` na raiz.
