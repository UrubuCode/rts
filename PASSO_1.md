# PASSO 1 — Namespace `rust`: Estrutura Base

## Objetivo

Criar o namespace `rust` em `src/namespaces/rust/` com as primitivas brutas de máquina:
escopo de variáveis, declaração de funções, constantes e memória.

Registrar o namespace no sistema (SPECS, dispatch, RTS_EXPORTS).

## Arquivos a criar

```
src/namespaces/rust/
  mod.rs         — SPEC, MEMBERS, dispatch(), re-exporta submodulos
  functions.rs   — rts_declare_fn, rts_call_fn, rts_return
  scope.rs       — rts_scope_push, rts_scope_pop, rts_set_var, rts_get_var
  constants.rs   — rts_declare_const, rts_get_const
  memory.rs      — rts_alloc, rts_free, rts_mem_copy, rts_i64_add, rts_f64_mul
```

## Arquivos a modificar

- `src/namespaces/mod.rs` — adicionar `pub mod rust`, registrar em `SPECS` e `dispatch()`

## Primitivas por arquivo

### functions.rs
- `rts.declare_fn(name_ptr: u64, arity: u64, body_ptr: u64)` — declara função
- `rts.call_fn(name_ptr: u64, args_ptr: u64, args_len: u64) -> u64` — invoca função
- `rts.return_val(value: u64) -> u64` — retorna valor

### scope.rs
- `rts.scope_push()` — empilha novo escopo
- `rts.scope_pop()` — desempilha escopo
- `rts.set_var(name_hash: u64, value: u64)` — define variável no escopo atual
- `rts.get_var(name_hash: u64) -> u64` — lê variável (percorre stack)

### constants.rs
- `rts.declare_const(name_hash: u64, value: u64)` — declara constante global
- `rts.get_const(name_hash: u64) -> u64` — lê constante global

### memory.rs
- `rts.alloc(size: u64) -> u64` — aloca `size` bytes, retorna ponteiro
- `rts.free(ptr: u64)` — libera memória
- `rts.mem_copy(dst: u64, src: u64, len: u64)` — copia `len` bytes
- `rts.i64_add(a: i64, b: i64) -> i64` — soma inteiros (sem JS overhead)
- `rts.f64_mul(a: f64, b: f64) -> f64` — multiplica floats

## Estado

- `scope.rs` usa `thread_local! { SCOPE_STACK: RefCell<Vec<HashMap<u64, u64>>> }`
- `functions.rs` usa `OnceLock<Arc<Mutex<HashMap<u64, FnEntry>>>>`
- `constants.rs` usa `OnceLock<Arc<Mutex<HashMap<u64, u64>>>>`
- `memory.rs` — sem estado (delega para allocator do sistema)

## Tipo de retorno no dispatch

O namespace `rust` usa `JsValue::Number` para handles/ponteiros (u64 como f64) e
`JsValue::Undefined` para operações void.
