# Remoção da lista manual em `jit.rs` via `linkme`

## Problema atual

`src/codegen/jit.rs` contém `runtime_symbol_table()` — 421 entradas `add_fn!`
listadas manualmente, uma por símbolo `__RTS_FN_NS_*`. Cada novo namespace exige
edição neste arquivo. Omissões causam falha silenciosa em `finalize_definitions()`.

## Solução: `linkme` distributed_slice

### Dependência

```toml
# Cargo.toml
linkme = "0.3"
```

Nenhuma outra dep muda. `linkme` usa seções de linker (`.rdata$linkme_*` no COFF,
`.data.rel.ro` no ELF) — compatível com Windows MSVC, Linux e macOS.
`strip = "symbols"` do profile release não afeta seções de dados, logo funciona.

### Novo arquivo: `src/namespaces/jit_symbols.rs`

```rust
use linkme::distributed_slice;

/// Tabela de símbolos JIT populada por cada namespace via `#[distributed_slice]`.
/// Registrada em `jit.rs` em substituição ao `runtime_symbol_table()` manual.
#[distributed_slice]
pub static JIT_SYMBOLS: [(&'static str, usize)] = [..];
```

### Padrão por namespace (exemplo: `src/namespaces/io/rt.rs`)

```rust
pub mod print;
pub mod stderr;
pub mod stdin;
pub mod stdout;

use linkme::distributed_slice;
use crate::namespaces::jit_symbols::JIT_SYMBOLS;

#[distributed_slice(JIT_SYMBOLS)]
static _IO_SYMBOLS: &[(&str, usize)] = &[
    ("__RTS_FN_NS_IO_PRINT",        print::__RTS_FN_NS_IO_PRINT        as usize),
    ("__RTS_FN_NS_IO_EPRINT",       print::__RTS_FN_NS_IO_EPRINT       as usize),
    ("__RTS_FN_NS_IO_STDOUT_WRITE", stdout::__RTS_FN_NS_IO_STDOUT_WRITE as usize),
    // ...
];
```

Repete o padrão nos 18 `rt.rs` de namespace (io, fs, gc, math, bigfloat, time,
env, path, buffer, string, process, os, collections, hash, fmt, crypto, regex, ui).

Para o namespace `runtime`, registrar as versões JIT fast-path:

```rust
#[distributed_slice(JIT_SYMBOLS)]
static _RUNTIME_SYMBOLS: &[(&str, usize)] = &[
    ("__RTS_FN_NS_RUNTIME_EVAL",      eval_jit::runtime_eval_src_jit  as usize),
    ("__RTS_FN_NS_RUNTIME_EVAL_FILE", eval_jit::runtime_eval_file_jit as usize),
];
```

### `jit.rs` após a mudança

```rust
fn build_jit_module() -> Result<JITModule> {
    // ... flags, isa (sem alteração) ...

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

    // Todos os símbolos de namespace auto-registrados.
    for (name, ptr) in crate::namespaces::jit_symbols::JIT_SYMBOLS {
        jit_builder.symbol(name, *ptr as *const u8);
    }

    // Entradas manuais que não são membros de abi::SPECS:
    // 1. Estado PRNG (dado mutable, não função)
    {
        let ptr = &raw const crate::namespaces::math::random::__RTS_DATA_NS_MATH_RNG_STATE
            as *const u8;
        jit_builder.symbol("__RTS_DATA_NS_MATH_RNG_STATE", ptr);
    }
    // 2. Slot de erro de runtime
    {
        use crate::namespaces::gc::error::*;
        jit_builder.symbol("__RTS_FN_RT_ERROR_SET",   __RTS_FN_RT_ERROR_SET   as *const u8);
        jit_builder.symbol("__RTS_FN_RT_ERROR_GET",   __RTS_FN_RT_ERROR_GET   as *const u8);
        jit_builder.symbol("__RTS_FN_RT_ERROR_CLEAR", __RTS_FN_RT_ERROR_CLEAR as *const u8);
    }
    // 3. fmod (libc)
    unsafe extern "C" { fn fmod(a: f64, b: f64) -> f64; }
    jit_builder.symbol("fmod", fmod as *const u8);

    Ok(JITModule::new(jit_builder))
}
```

`runtime_symbol_table()` é deletada inteira. O `debug_assert` de contagem
passa a comparar `JIT_SYMBOLS.len()` contra `abi::SPECS` member count.

## Impacto

| Arquivo | Mudança |
|---|---|
| `Cargo.toml` | +1 dep (`linkme = "0.3"`) |
| `src/codegen/jit.rs` | ~880 linhas removidas; ~30 linhas ficam |
| `src/namespaces/jit_symbols.rs` | novo (8 linhas) |
| `src/namespaces/mod.rs` | + `pub mod jit_symbols;` |
| 18 arquivos `src/namespaces/*/rt.rs` | + bloco `#[distributed_slice]` por namespace |
| Todo o resto | inalterado |

## O que NÃO muda

- `src/abi/` — inalterado
- `src/codegen/emit.rs` (AOT) — inalterado
- `src/codegen/lower/` — inalterado
- `src/pipeline.rs` — inalterado
- `runtime_support.a` / `build.rs` — inalterados
- Funções `extern "C"` em si — inalteradas

## Entradas que permanecem manuais (3)

| Símbolo | Motivo |
|---|---|
| `__RTS_DATA_NS_MATH_RNG_STATE` | Dado (`*const u64`), não função |
| `__RTS_FN_RT_ERROR_{SET,GET,CLEAR}` | Fora de `abi::SPECS` (runtime error slot) |
| `fmod` | Extern libc, não é namespace member |
