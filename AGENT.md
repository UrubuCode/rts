# CLAUDE.md

## Projeto

RTS e um compilador/runtime TypeScript-to-native usando Cranelift como backend de codegen.
O objetivo e compilar TS/JS para binarios nativos com runtime minimo em Rust.

## Arquitetura

```
src/
  parser/       — SWC parse + AST interno
  hir/          — High-level IR (lower + optimize)
  mir/          — Mid-level IR tipado (typed_build + optimize)
  codegen/      — Cranelift codegen (object builder, typed codegen)
  linker/       — Link nativo (system lld, fallback object)
  namespaces/   — Runtime APIs: io, fs, net, process, crypto, global, buffer, promise, task
  runtime/      — Builtin module "rts", exports, bundle format
  module/       — Resolver de modulos, grafo de dependencias
  type_system/  — Type checker, registry, resolver
  diagnostics/  — Erros estruturados
  pipeline.rs   — Orquestra build/run
  lib.rs        — API publica (compile_and_link, eval)
  cli/          — CLI (eval, repl)
```

Pipeline: `Source TS → Parser(SWC) → HIR → MIR tipado → Cranelift codegen → Object → Link → .exe`

## Estrutura de Arquivos por Namespace

Cada namespace fragmenta suas funcoes por tipo em arquivos separados:

```
src/namespaces/<name>/
  mod.rs         — import map: re-exporta tudo dos submodulos, define SPEC/MEMBERS/dispatch()
  tcp.rs         — funcoes TCP (ex: listen, connect, accept, send, recv)
  udp.rs         — funcoes UDP
  <fn_type>.rs   — grupo logico de funcoes relacionadas
```

E utilitarios compartilhados entre namespaces:

```
src/namespaces/utils/
  net.rs         — utils do namespace net
  fs.rs          — utils do namespace fs
  <namespace>.rs — utils especificos do namespace
```

Regras de fragmentacao:
- `mod.rs` e apenas o import map — nao contem logica de negocio
- Cada arquivo agrupa funcoes por tipo/protocolo/responsabilidade
- Utils compartilhados vao em `utils/<namespace>.rs`, nao dentro do namespace

## Convencoes

- Linguagem do codigo: Rust (ingles nos identificadores)
- Linguagem de comunicacao: portugues
- Commits seguem conventional commits: `feat:`, `fix:`, `perf:`, `refactor:`, `docs:`
- Namespaces de runtime ficam em `src/namespaces/<name>/mod.rs`
- Cada namespace tem: `SPEC` (const), `MEMBERS` (const), `dispatch()` (fn)
- Novo namespace precisa ser registrado em: `SPECS`, `dispatch()` chain, `RTS_EXPORTS`
- O `rts.d.ts` e gerado automaticamente por `render_typescript_declarations()`

## Como testar

```bash
cargo test                                      # testes unitarios
cargo build --release                           # build release
target/release/rts.exe run file.ts             # executar (runtime)
target/release/rts.exe compile -p file.ts output  # compilar nativo (AOT)
target/release/rts.exe apis                    # listar APIs disponiveis
```

## Benchmarks

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```

Compara RTS (run), RTS (compiled), Bun e Node.

## Regras

- Nao implementar APIs de alto nivel em Rust — Rust so expoe primitivas raw via `"rts"`
- Packages TS em `packages/*` constroem APIs ergonomicas sobre o `"rts"`
- `rts.d.ts` so contem `declare module "rts"` — nao adicionar outros modulos
- Handles numericos (u64) para recursos runtime (buffers, sockets, promises)

## Sem Codigo Legacy

**Regra absoluta: codigo morto e removido imediatamente. Nunca comentar, nunca deixar "por precaucao".**

- Qualquer codigo que nao e chamado por nenhum caminho vivo deve ser deletado no mesmo commit que o tornou morto
- Stubs `todo!()` / `unimplemented!()` sao aceitaveis como marcador temporario de WIP; codigo comentado nao
- Warnings de dead_code sao tratados como erros — o build nao pode terminar com warnings

## ABI de Maquina — sem JsValue no limite

`JsValue` e uma abstracao de alto nivel (semântica JS) que nao tem lugar no limite entre codegen e runtime. O caminho correto:

```
JsValue (LEGADO)              Maquina (CORRETO)
─────────────────────────────────────────────────
dispatch(&[JsValue]) → JsValue   extern "C" fn tipado(i64, f64, ...) → i64
__rts_call_dispatch + handles    __rts_<ns>_<fn>(ptr, len, ...) direto
boxing/unboxing em cada call     zero overhead — tipos nativos no registrador
```

### Convencao ABI para tipos primitivos

| Tipo TS  | Tipo Rust ABI | Convencao                          |
|----------|---------------|------------------------------------|
| `number` | `i64` / `f64` | bits nativos, sem boxing            |
| `bool`   | `i64`         | 0 = false, 1 = true                |
| `string` | `(i64, i64)`  | `(ptr, len)` — dados UTF-8 estaticos do codegen |
| handle   | `u64`         | indice opaco para recursos heap (buffers, sockets, promises, strings dinamicas) |

### Regras de implementacao

- Cada funcao de namespace vira um simbolo `#[unsafe(no_mangle)] pub extern "C" fn __rts_<ns>_<fn>(...)`
- Nenhuma funcao de namespace aceita `JsValue` como argumento ou retorno no limite `extern "C"`
- `dispatch(&[JsValue])` e mantido apenas internamente como roteador temporario enquanto a migracao ocorre — nao e parte da ABI publica
- Strings de retorno dinamico (ex: resultado de `fs.read`) sao alocadas no heap e retornam um handle `u64`; o caller chama `__rts_string_ptr(handle)` e `__rts_string_len(handle)` para ler

## Runtime vs Compile (AOT)

Runtime e AOT sao unificados — ambos geram `.o`/`.m` objects via Cranelift. A diferenca e de escopo:

- **Runtime (`rts run`)**: gera objects completos de todos os modulos builtin, mesmo com `-p`. O builtin sempre tem todos os namespaces presentes nos objects.
- **Compile (`rts compile`)**: gera apenas os objects dos modulos efetivamente usados (slicing). Produce o binario final em `target/release/`.

Nao ha divergencia de codepath entre runtime e AOT — o mesmo pipeline de codegen e usado. Runtime e inerentemente mais pesado por incluir todos os builtins; AOT e otimizado por slicing.

Convencao de nomes dos objects: `<module>.o` (e `.m` se houver metadata associado).

## GC — gc-arena (coleta deterministica)

Usar o crate `gc-arena` como sistema de GC deterministico. Coleta e disparada apos:
- Retorno de funcoes
- Execucao de metodos de classe
- Fim de escopo de closures

Principio: `safe_collect()` e chamado em pontos de quiescencia bem definidos, nao de forma periodica/assincrona.

## Estrutura de Projeto do Usuario

```
<project>/
  src/
    main.ts
  package.json
  tsconfig.json

  target/
    modules/          — node_modules resolvidos
    objs/
      runtime/        — objects completos do builtin (todos os modulos)
      compile/        — objects AOT (apenas presente em rts compile)

  release/            — apenas em rts compile
    <project_name>    — .exe / .dll / .so / .node conforme target
```

## State

Estado de namespace usa `Arc<Mutex<T>>` direto quando necessario, ou `thread_local!` para caches por-thread. Nao ha sistema centralizado de state — cada namespace gerencia seu proprio estado com os patterns padrao de Rust.

### Pattern para estado compartilhado

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

static NET_STATE: std::sync::OnceLock<Arc<Mutex<NetState>>> = std::sync::OnceLock::new();

fn net_state() -> Arc<Mutex<NetState>> {
    NET_STATE.get_or_init(|| Arc::new(Mutex::new(NetState::default()))).clone()
}

#[derive(Default)]
struct NetState {
    tcp_listeners: HashMap<u64, TcpListener>,
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
Consultar o indice em `docs/specs/INDEX.md`.
