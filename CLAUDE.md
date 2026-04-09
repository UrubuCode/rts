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
cargo test                              # testes unitarios
cargo build --release                   # build release
target/release/rts.exe run file.ts      # executar interpretado
target/release/rts.exe build -p file.ts output  # compilar nativo
target/release/rts.exe apis             # listar APIs disponiveis
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
- Runtime slicing: so compila/linka namespaces efetivamente usados
- Handles numericos (u64) para recursos runtime (buffers, sockets, promises)

## State — REGRA PRINCIPAL DE CONSTRUCAO

**Todo estado de runtime DEVE ser gerenciado via `src/namespaces/state/central.rs`.**
O CentralState e o controlador unico de TODOS os estados do RTS runtime, permitindo controle completo pelo GC.

### Sistema Central de Estado
- **UNICO ponto de entrada**: `central()` retorna a instancia global do CentralState
- **Rastreamento de alocacoes**: Todo estado e alocacao e rastreada para o GC futuro
- **Thread-safe**: Sistema baseado em Arc<Mutex<T>> para acesso seguro entre threads
- **Handle numericos**: Recursos como sockets, promises, buffers usam handles u64

### APIs do sistema central

```rust
use crate::namespaces::state::central;

// Estado de namespace (um por namespace, Default trait required)
let state = central().namespace_state::<NetState>("net");
let mut guard = state.lock().unwrap();

// Cache compartilhado (multiplos por ID string)
let cache = central().cache::<String>("my-cache");

// Handles tipados para recursos 
let handle_id = central().create_handle(resource);
let value = central().get_handle::<ResourceType>(handle_id);
central().with_handle_mut(handle_id, |resource| { /* modify */ });
```

### Pattern para namespaces

```rust
use crate::namespaces::state::central;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MyNamespaceState {
    // namespace state fields
}

pub fn with_namespace_state<R>(f: impl FnOnce(&mut MyNamespaceState) -> R) -> R {
    let state = central().namespace_state::<MyNamespaceState>("my_namespace");
    let mut guard = state.lock().unwrap();
    f(&mut *guard)
}
```

### O que NAO fazer - PROIBIDO
- **NAO criar `OnceLock`, `Mutex`, `RefCell`, `static` dentro dos namespaces**
- **NAO criar estado local fora do sistema central**
- **NAO acessar `std::sync::*` diretamente para storage**
- **NAO implementar logica de negocio dentro de `state/*.rs`**

### Separacao de responsabilidades
- `state/central.rs` → CentralState, allocation tracking, handles, cache/namespace management
- `state/mod.rs` → public API wrappers, helpers, legacy compatibility
- `<namespace>/mod.rs` → logica do namespace (usa central() para storage)

## Docs e especificacoes

A pasta `docs/specs/` contem especificacoes de features, decisoes de design e notas tecnicas.
Consultar o indice em `docs/specs/INDEX.md`.
