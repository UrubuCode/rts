# PROJECT_MAP

## Root
- `Cargo.toml`: crate principal do compilador RTS.
- `rts.d.ts`: declaracoes do modulo builtin `"rts"`.
- `src/`: codigo do compilador e runtime minimo.
- `examples/`: exemplos `.ts`.
- `tests/`: suites de teste.

## Compiler (`src/`)
- `main.rs`: entrypoint CLI.
- `lib.rs`: pipeline principal de compilacao.
- `module_system/`: grafo de modulos, resolucao de import e detecao `.ts`/`.rts`.
- `parser/`: parser proprio com AST preservando tipos.
- `type_system/`: registry/checker/resolver/metadata.
- `hir/`: lowering AST -> HIR.
- `mir/`: lowering HIR -> MIR + passes.
- `codegen/`: CLIF textual + scaffold de JIT.
- `linker/`: backend 100% Rust via crate `object` (PE executavel no Windows; container nativo em Linux/macOS).
- `runtime/`: modulo builtin `"rts"` + intrinsecos base em Rust.

## Runtime API
- `process`
- `print`
- `panic`
- `clockNow`
- `alloc`
- `dealloc`

## Examples
- `examples/console.ts`: classe local usando `process` importado de `"rts"`.
- `examples/mixed/main.ts`: modulo local usando `print` de `"rts"`.

