# Fase 1.2 — crates `rts-linker` e `rts-runtime`

**Branch:** `refactor/fase-1`  
**Pré-requisito:** etapa 1.1 commitada (rts-abi + rts-parser verde)  
**Meta:** extrair linker e todos os namespaces em crates independentes, sem mudança funcional.

---

## Escopo

### `rts-linker`

Mover:
- `src/linker/mod.rs`
- `src/linker/object_linker.rs`
- `src/linker/system_linker.rs`
- `src/linker/toolchain.rs`
- `src/runtime_objects.rs`

Sem deps `crate::` — esses arquivos só usam `std`, `anyhow`, `object`,
`target-lexicon` e `rts-abi` (para `AbiType` em `runtime_objects`).
Verificar antes de mover.

### `rts-runtime`

Mover:
- `src/namespaces/` inteiro, **exceto**:
  - `src/namespaces/runtime/eval_jit.rs` — fica no monolito (usa
    `crate::parser`, `crate::codegen`, `crate::compile_options`)
  - `src/namespaces/globals/function/eval_compile.rs` — idem

A `rt_all.rs` (usada pelo build para gerar `runtime_support.a`)
vai junto para `rts-runtime`.

---

## Deps de cada crate

### `crates/rts-linker/Cargo.toml`

```toml
[package]
name = "rts-linker"
version = "0.1.0"
edition = "2024"

[dependencies]
rts-abi = { path = "../rts-abi" }
anyhow = "1.0"
object = { version = "0.39", default-features = false, features = [
    "write", "read", "std", "coff", "elf", "macho", "pe"
] }
target-lexicon = "0.13"
```

### `crates/rts-runtime/Cargo.toml`

```toml
[package]
name = "rts-runtime"
version = "0.1.0"
edition = "2024"

[dependencies]
rts-abi = { path = "../rts-abi" }
anyhow = "1.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }
rayon = "1.10"
sha2 = "0.10"
regex = "1"
rustls = { version = "0.23", default-features = false, features = [
    "std", "ring", "tls12"
] }
webpki-roots = "1"
actix-web = { version = "4", default-features = false, features = ["macros"] }
actix-rt = "2"
fltk = { version = "1", features = ["fltk-bundled"] }
indexmap = "2.14.0"
colored = "2.1"
notify = "6.1"
flate2 = "1.1"
tar = "0.4"
ureq = { version = "2.12", default-features = true }
slotmap = "1"
rustc-hash = "1.1"
```

---

## Problema: SPECS / GLOBAL\_CLASS\_SPECS

`src/abi/mod.rs` declara `SPECS` e `GLOBAL_CLASS_SPECS` referenciando
`crate::namespaces::*`. Após mover namespaces para `rts-runtime`, essas
referências mudam para `rts_runtime::namespaces::*`.

**Solução para Fase 1 (sem mudança arquitetural):** manter `SPECS` e
`GLOBAL_CLASS_SPECS` em `src/abi/mod.rs` no monolito, trocando
`crate::namespaces` por `rts_runtime::namespaces`. O monolito já depende
de `rts-runtime`.

Não mover para `rts-abi` — criaria dep circular
(`rts-abi` → `rts-runtime` → `rts-abi`).

---

## Problema: arquivos que ficam no monolito

Dois arquivos do namespace têm deps no pipeline completo do compilador:

| Arquivo | Deps problemáticas |
|---|---|
| `src/namespaces/runtime/eval_jit.rs` | `crate::parser`, `crate::codegen`, `crate::compile_options` |
| `src/namespaces/globals/function/eval_compile.rs` | `crate::compile_options`, `crate::codegen` |

**Ação:** não mover para `rts-runtime`. Manter em `src/namespaces/` no
monolito. O módulo `runtime` em `rts-runtime` exporta os símbolos de
`eval.rs` e `hot_reload.rs`; `eval_jit.rs` é registrado separadamente
pelo `jit.rs` do monolito.

Mesma lógica para `eval_compile.rs` — registrado pelo `jit.rs`.

---

## Passos de implementação

### Passo 1 — rts-linker (simples, sem surpresas)

1. Criar `crates/rts-linker/Cargo.toml`
2. Copiar `src/linker/*.rs` para `crates/rts-linker/src/`
3. Criar `crates/rts-linker/src/lib.rs`:
   ```rust
   pub mod mod_linker; // ou inline mod.rs
   pub mod object_linker;
   pub mod system_linker;
   pub mod toolchain;
   pub mod runtime_objects;
   ```
   **Atenção:** `runtime_objects.rs` está em `src/` (não em `src/linker/`) —
   mover também para `crates/rts-linker/src/runtime_objects.rs`.
4. Ajustar imports internos (nenhum `crate::` externo esperado).
5. Monolito: `src/linker/mod.rs` → `pub use rts_linker::*;`
            `src/runtime_objects.rs` → `pub use rts_linker::runtime_objects::*;`
6. Adicionar ao workspace e ao `[dependencies]` do monolito.
7. `cargo build` verde antes de continuar.

### Passo 2 — rts-runtime (maior, verificar deps antes)

1. Mapear todos os `use crate::` em `src/namespaces/**/*.rs` que não
   sejam `crate::namespaces` nem `crate::abi`:
   ```
   grep -rn "use crate::" src/namespaces/ --include="*.rs" \
     | grep -v "crate::namespaces\|crate::abi"
   ```
   Resultado esperado: apenas `eval_jit.rs` e `eval_compile.rs`.

2. Criar `crates/rts-runtime/Cargo.toml`.

3. Copiar `src/namespaces/` para `crates/rts-runtime/src/namespaces/`,
   **excluindo** `runtime/eval_jit.rs` e
   `globals/function/eval_compile.rs`.

4. Criar `crates/rts-runtime/src/lib.rs`:
   ```rust
   pub mod namespaces;
   ```

5. Copiar `src/namespaces/rt_all.rs` para
   `crates/rts-runtime/src/namespaces/rt_all.rs` (sem mudanças).

6. Ajustar imports: `use crate::abi::` → `use rts_abi::` nos arquivos
   copiados (gc/handles.rs e outros que referenciam abi).

7. Monolito:
   - `src/namespaces/mod.rs` → re-exportar de `rts_runtime::namespaces`
   - `src/abi/mod.rs` → trocar `crate::namespaces::` por
     `rts_runtime::namespaces::`
   - `src/namespaces/runtime/eval_jit.rs` — **manter no lugar**,
     garantir que `mod eval_jit` ainda é declarado em
     `src/namespaces/runtime/mod.rs` (mas não copiado para rts-runtime)
   - Idem `eval_compile.rs`.

8. Adicionar ao workspace e ao `[dependencies]` do monolito.

9. `cargo build` e `cargo test --lib` verdes.

---

## Verificação final

```bash
cargo build                  # zero erros
cargo test --lib             # mesmas 3 falhas json pre-existentes
cargo test --workspace       # todos os crates
```

Confirmar invariante: `grep -rn "use cranelift" crates/rts-abi crates/rts-parser crates/rts-linker crates/rts-runtime` retorna zero linhas.

---

## Commit esperado

```
feat(refactor): etapa 1.2 — crates rts-linker e rts-runtime

- rts-linker: linker/, toolchain, runtime_objects sem deps cranelift
- rts-runtime: todos os namespaces (exceto eval_jit/eval_compile que
  precisam do pipeline completo — ficam no monolito)
- src/abi/mod.rs: SPECS/GLOBAL_CLASS_SPECS agora referenciam rts_runtime
- build verde, 3 falhas json pre-existentes
```
