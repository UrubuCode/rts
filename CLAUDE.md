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

**Estado compartilhado DEVE usar `central().namespace_state()`. Thread-local DEVE usar `thread_local!`.**
Sistema otimizado para performance com separacao clara de responsabilidades.

### Sistema de Estado Otimizado
- **Estados compartilhados**: `central().namespace_state<T>("name")` para estado cross-thread
- **Caches thread-local**: `thread_local! { static CACHE: RefCell<T> }` para performance
- **Thread-safe**: Arc<Mutex<T>> apenas quando necessario compartilhamento
- **Zero overhead**: Thread-local para casos single-thread

### APIs otimizadas

```rust
// Para estado compartilhado entre threads (ex: net sockets)
use crate::namespaces::state::central;

let state = central().namespace_state::<NetState>("net");
let mut guard = state.lock().unwrap();

// Para caches thread-local (ex: parser cache)
use std::cell::RefCell;

thread_local! {
    static CACHE: RefCell<HashMap<u64, ParseResult>> = RefCell::new(HashMap::new());
}

CACHE.with(|cache| {
    cache.borrow_mut().insert(key, value);
});
```

### Pattern para namespaces compartilhados

```rust
use crate::namespaces::state::central;

#[derive(Default)]
struct NetState {
    tcp_listeners: HashMap<u64, TcpListener>,
    // shared state fields
}

pub fn with_net_state_mut<R>(f: impl FnOnce(&mut NetState) -> R) -> R {
    let state = central().namespace_state::<NetState>("net");
    let mut guard = state.lock().unwrap();
    f(&mut *guard)
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

### O que NAO fazer - PROIBIDO
- **NAO usar `central()` para caches thread-local** (use `thread_local!` para performance)
- **NAO usar `thread_local!` para estado compartilhado** (use `central().namespace_state()`)
- **NAO criar `OnceLock`, `static Mutex` soltos** (usar patterns acima)
- **NAO implementar logica de negocio dentro de `state/*.rs`**

### Separacao de responsabilidades
- `state/central.rs` → CentralState simplificado, apenas namespace_state()
- `state/mod.rs` → buffers, promises, globals (usando central state interno)
- `<namespace>/mod.rs` → logica do namespace (escolhe pattern apropriado)

## Docs e especificacoes

A pasta `docs/specs/` contem especificacoes de features, decisoes de design e notas tecnicas.
Consultar o indice em `docs/specs/INDEX.md`.
