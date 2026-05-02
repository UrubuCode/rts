# Runtime — HandleTable, tokio, GC, State

## HandleTable shard-aware

`HandleTable` esta dividido em 32 shards lock-free entre si.
`alloc_entry` distribui round-robin por thread; `shard_for_handle`
decodifica O(1) o shard de qualquer handle (encoded nos low bits).
Todos os 17+ namespaces handle-based migrados pra essa API — sem
contencao em workloads paralelos.

## Runtime tokio compartilhado (issue #399)

`src/runtime/async_rt.rs` exporta `rt()` —
`OnceLock<tokio::runtime::Runtime>` multi-thread global. Hooks
`on_thread_start`/`on_thread_stop` registram cada worker no
`gc/thread_registry` para o GC scanner ver handles vivos em tasks
tokio (sem isso o sweep coletava indevidamente sob carga
concorrente).

Toda feature async deve reusar este runtime em vez de criar um
proprio:

- `http_server::serve` chama `rt().block_on(...)`
- `thread::spawn_async*` usa `rt().handle().spawn_blocking(...)`
- `runtime::tokio_ctx` oferece "id u64 opaco + shard map por
  TypeId" como bridge sync↔async generico (substitui `slots()`
  ad-hoc do http_server)
- `promise.create` (drysius design, #437) chama
  `rt.spawn_blocking(...)` para invocar fn handle e settle Promise

Convencao: o que cruza o JIT (extern "C") eh apenas u64 opaco. Tipos
Rust-rich (Arc<T>, Channel, JoinHandle, JITModule) ficam no shard
map indexado por esse id — ou em handles GC com lifetime guard
(`Entry::Function::keep_alive`).

## GC stack scanner Win32

`mark_stack_roots()` em `src/namespaces/gc/collector.rs` usa
`GetCurrentThreadStackLimits` (API Win32 oficial) em vez de
`gs:[0x10]` da TIB. O TIB.StackBase em alguns contextos retornava
valor < RSP, deixando o scanner sem marcar nada e o sweep coletando
handles vivos (bug encontrado em 2026-05-01 testando http_server
sob carga). Mesmo caminho usado para varrer threads no
`thread_registry` via `SuspendThread + GetThreadContext` + scan de
registers callee-saved.

## GC — mark+sweep com Cranelift stack maps

**Estado atual:** o crate `gc-arena = "0.5"` esta declarado no
`Cargo.toml` mas **nao esta integrado de fato**. O sistema real eh
mark+sweep preciso usando `UserStackMap` do Cranelift, com scanner
conservativo via `SuspendThread + GetThreadContext` para cobrir
todas as threads RTS registradas no `thread_registry`. Detalhes:

- Codegen chama `builder.declare_value_needs_stack_map(val)` para
  cada handle
- `jit.rs` extrai `UserStackMap` apos `define_function` e registra
  return-PC absolutos no `stack_map_registry`
- A cada N alocacoes (`GC_TICK_INTERVAL = 256`), `finish_cycle()`
  roda `mark_stack_roots()` (varre stack da thread atual + stacks
  de outras threads via SuspendThread) e `sweep_all_shards()` libera
  o que nao foi marcado
- `mark_stack_roots()` no Windows usa `GetCurrentThreadStackLimits`
  (API Win32 oficial). Nao usar `gs:[0x10]` — em alguns contextos
  retorna StackBase < RSP, deixando o scanner sem marcar nada e o
  sweep coletando handles vivos (bug PR #400)

**Migracao real para gc-arena** (issue #393) seria refator grande:
todas as 25+ variantes de `Entry` precisariam derivar `Collect`,
com `Mutation<'gc>` token cruzando o JIT — incompativel com a ABI
extern "C" plana atual. Adiada.

## Runtime vs Compile

Dois caminhos de execucao compartilhando o mesmo codegen Cranelift:

- **`rts run`**: compila direto para memoria executavel via
  `JITModule`. Sem disco, sem linker externo. Todos os simbolos do
  ABI sao registrados em `JITBuilder::symbol` no startup do modulo
  JIT (`src/codegen/jit.rs`).
- **`rts compile`**: aplica slicing por uso, gera apenas os objects
  dos modulos efetivamente utilizados, produz binario final.

`FnCtx.module` eh `&mut dyn Module` — `ObjectModule` e `JITModule`
implementam o mesmo trait e passam pelo mesmo pipeline de
`compile_program`.

Convencao de nomes de object: `<module>.o` (e `.m` quando houver
metadata para cache incremental).

## State

Estado de namespace usa `Arc<Mutex<T>>` direto quando necessario,
ou `thread_local!` para caches por-thread. Nao ha sistema
centralizado de state — cada namespace gerencia o seu.

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

## Sem Codigo Legacy

**Regra absoluta: codigo morto eh removido imediatamente. Nunca
comentar, nunca deixar "por precaucao".**

- Qualquer codigo que nao eh chamado por nenhum caminho vivo deve
  ser deletado no mesmo commit que o tornou morto
- Stubs `todo!()` / `unimplemented!()` sao aceitaveis como marcador
  temporario de WIP; codigo comentado nao
- Warnings de `dead_code` sao tratados como erros — o build nao
  pode terminar com warnings
