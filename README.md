# RTS

Bootstrap inicial do compilador RTS com modulo builtin `"rts"` fornecido pelo runtime em Rust.

## Direcao atual

1. `rts` compila entradas `.ts` (com fallback legado para `.rts`).
2. O modulo `"rts"` e builtin da linguagem (nao vem de arquivos TS).
3. Runtime em Rust expoe somente chamadas base (`process`, `print`, `panic`, `clockNow`, `alloc`, `dealloc`).
4. Bibliotecas de mais alto nivel devem ser implementadas em TS por cima dessas chamadas base.
5. Linkagem e escrita de binario usam backend 100% Rust (`object` crate), sem chamar comandos externos.

## Pipeline atual

1. Loader de modulos (`ModuleGraph`) resolve imports e monta grafo.
2. Imports para `"rts"` sao resolvidos para modulo builtin (virtual) em Rust.
3. Parser proprio preserva tipos em AST (`import`, `class`, `interface`, `function`).
4. Type checker valida imports/exports e coleta tipos.
5. AST -> HIR -> MIR -> scaffold Cranelift (AOT/JIT).
6. Linker gera executavel PE no Windows e artefato nativo (ELF/Mach-O object container) em Linux/macOS, tudo via dependency Rust.

## Comandos

- `cargo run -- build examples/console.ts target/console`
- `cargo run -- build examples/mixed/main target/mixed`
- `cargo run -- run examples/mixed/main`

