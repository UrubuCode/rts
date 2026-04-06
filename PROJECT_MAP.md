# PROJECT_MAP

## Root
- `Cargo.toml`: crate principal do compilador RTS.
- `packages/rts-types/rts.d.ts`: declaracoes do modulo builtin `"rts"` (types base + namespaces).
- `src/`: codigo do compilador e runtime bootstrap.
- `examples/`: exemplos `.ts`.
- `packages/`: pacotes TS (`console`, `process`, `std`, `fs`, `tests`, `rts-types`).

## Compiler (`src/`)
- `main.rs`: entrypoint CLI.
- `lib.rs`: pipeline principal de compilacao.
- `module_system/`: grafo de modulos e resolucao de imports.
- `parser/`: parser proprio com AST.
- `type_system/`: registry/checker/resolver/metadata.
- `hir/`: lowering AST -> HIR + optimize.
- `mir/`: lowering HIR -> MIR + optimize.
- `codegen/`: JIT/CLIF (Cranelift).
- `linker/`: backend `object`.
- `runtime/`: runtime bootstrap e modulo builtin `"rts"`.

## Runtime (`src/runtime`)
- `bootstrap.rs`: execucao bootstrap e binding com avaliador.
- `bootstrap_lang/`: lexer/parser/ast/evaluator de expressoes.
- `bootstrap_utils.rs`: helpers de parse textual.
- `namespaces/`: implementacao separada por namespace (`io`, `fs`, `process`, `crypto`, `global`, `buffer`, `promise`, `task`).
- `state.rs`: estado global runtime, buffers, promises e executor async.
- `bundle.rs` / `runner.rs`: empacotamento e execucao.

## Runtime API (`"rts"`)
- Tipos base: inteiros/floats/aliases, `WritableStream`, `ReadableStream`, `FileHandle`.
- Namespaces:
  - `io`
  - `fs`
  - `process`
  - `crypto`
  - `global`
  - `buffer`
  - `promise`
  - `task`

## Examples
- `examples/console.ts`: uso de `io` + `process` + pacote `console`.
- `examples/global_buffer_promise.ts`: uso de `global`/`buffer`/`task`/`promise`.
- `examples/mixed/main.ts`: modulo local + `io`.
