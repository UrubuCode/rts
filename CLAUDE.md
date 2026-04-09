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

**Todo estado de runtime DEVE ser gerenciado via `src/namespaces/state/`.**
O state e o centralizador de estados da aplicacao, controlado pelo GC.

### O que o state gerencia
- Mutex nomeados (cada namespace registra o seu por nome)
- Controle de estados do app (globals, buffers, handles, etc.)
- Base para o GC deterministico futuro (rastreamento de alocacoes)

### Como usar nos namespaces

```rust
use crate::namespaces::state::{State, Mutex};

fn lock_net() -> std::sync::MutexGuard<'static, NetState> {
    let state = Mutex.get_or_init("net", Mutex::new(NetState::default()));
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
```

### Exports base do state

```rust
use crate::namespaces::state::{State, Mutex, Globals};
```

### O que NAO fazer
- **NAO criar `OnceLock`/`Mutex` soltos dentro dos namespaces** — usar o state centralizado
- **NAO adicionar funcoes de logica de namespace dentro de `state/*.rs`** — o state so expoe primitivas de gerenciamento (Mutex, State, Globals)
- **NAO acessar `std::sync::OnceLock` diretamente** — sempre via `crate::namespaces::state`

### Separacao de responsabilidades
- `state/*.rs` → primitivas de gerenciamento: Mutex nomeado, State, Globals, rastreamento GC
- `<namespace>/mod.rs` → logica do namespace: SPEC, dispatch, operacoes (usa state para storage)

## Docs e especificacoes

A pasta `docs/specs/` contem especificacoes de features, decisoes de design e notas tecnicas.
Consultar o indice em `docs/specs/INDEX.md`.
