# PASSO 3 — `rust/hotops.rs`: Inline Expansion + Tabelas Pré-computadas

## Objetivo

Implementar otimizações de performance para operações frequentes:

1. **Inline expansion**: operadores comuns (+, -, *, /, ==, !=, <, >) com tipos já conhecidos
   emitem código mínimo sem overhead de coerção.
2. **Precomputed tables**: `TO_STRING_TABLE` para inteiros 0..=255 evita alocação em 99%
   dos casos de `toString()` em inteiros pequenos.

## Por que separar de `natives.rs`?

- `natives` lida com coerção (tipos desconhecidos em compile time)
- `hotops` lida com operações onde os tipos JÁ são conhecidos pelo MIR
- Separação permite ao MIR decidir: tipo conhecido → `hotops`, tipo ambíguo → `natives`

## Arquivo a criar

```
src/namespaces/rust/hotops.rs
```

## Primitivas

### Operações tipadas (i64)
- `rts.hotops.i64_sub(a: i64, b: i64) -> i64`
- `rts.hotops.i64_div(a: i64, b: i64) -> i64`
- `rts.hotops.i64_mod(a: i64, b: i64) -> i64`
- `rts.hotops.i64_eq(a: i64, b: i64) -> bool`
- `rts.hotops.i64_lt(a: i64, b: i64) -> bool`
- `rts.hotops.i64_le(a: i64, b: i64) -> bool`

### Operações tipadas (f64)
- `rts.hotops.f64_add(a: f64, b: f64) -> f64`
- `rts.hotops.f64_sub(a: f64, b: f64) -> f64`
- `rts.hotops.f64_div(a: f64, b: f64) -> f64`
- `rts.hotops.f64_eq(a: f64, b: f64) -> bool`
- `rts.hotops.f64_lt(a: f64, b: f64) -> bool`

### Conversões otimizadas
- `rts.hotops.i64_to_string(n: i64) -> u64` — usa `TO_STRING_TABLE` para n < 256
- `rts.hotops.f64_to_string(n: f64) -> u64` — formata float para string

## TO_STRING_TABLE

```rust
static TO_STRING_TABLE: [&str; 256] = ["0", "1", "2", ..., "255"];
```

Para `n in 0..=255`: retorna da tabela sem alocação.
Para outros valores: formata via `format!`.

## Registro

Adicionar `HOTOPS_SPEC` e `HOTOPS_MEMBERS` em `rust/mod.rs`, chain em `dispatch()`.
