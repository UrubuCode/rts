# PROJECT_MAP

## Visao geral
RTS e um compilador/runtime TS com dois caminhos de execucao:
- `build` (AOT): gera objeto nativo com Cranelift e linka binario final.
- `run` (JIT): compila e executa em memoria com Cranelift JIT.

O runtime builtin `"rts"` e exposto por namespaces (`io`, `fs`, `process`, `crypto`, `global`, `buffer`, `promise`, `task`), e o `rts.d.ts` e gerado a partir do catalogo desses namespaces.

## Root
- `Cargo.toml`: crate principal.
- `PROJECT_MAP.md`: mapa estrutural do projeto.
- `PROJECT_PLAN.md`: status, decisoes e roadmap.
- `packages/rts-types/rts.d.ts`: declaracoes TS do modulo `"rts"`.
- `src/`: compilador, linker, runtime, namespaces.
- `examples/`: exemplos TS para validacao rapida.
- `bench/`: scripts e cenarios de benchmark.

## CLI (`src/cli`)
- `mod.rs`: dispatch de comandos.
- `build.rs`: pipeline de build AOT + resumo de artefatos.
- `run.rs`: pipeline JIT.
- `init.rs`: bootstrap de projeto.
- `apis.rs`: listagem de APIs builtin.
- `repl.rs`: REPL bootstrap.

Comandos atuais:
- `rts build`
- `rts run`
- `rts repl`
- `rts init`
- `rts apis`

## Pipeline de compilacao (`src/lib.rs`)
Fluxo principal:
1. `module_system`: resolve grafo de modulos (entry, local, package, builtin).
2. `parser`: AST.
3. `type_system`: registro, checker, resolver, metadata.
4. `hir`: lowering + optimize.
5. `mir`: build/monomorphize/optimize.
6. `codegen`:
   - AOT: `src/codegen/cranelift/object_builder.rs`
   - JIT: `src/codegen/cranelift/jit.rs`
7. `linker`: gera binario final.

## MIR e codegen nativo
Estado atual:
- MIR ainda e textual em boa parte, com CFG linear.
- Lowering Cranelift atual interpreta statements e chamadas diretas.

ABI interna de chamadas:
- assinatura de funcao: `(argc, a0, a1, a2, a3, a4, a5) -> i64`
- retorno `i64` representa handle de valor runtime.
- argumentos de chamada sao avaliados em runtime via `__rts_eval_expr`.
- chamadas de namespace usam `__rts_call_dispatch`.

Arquivos-chave:
- `src/codegen/cranelift/object_builder.rs`
- `src/codegen/cranelift/jit.rs`

## Namespaces (`src/namespaces`)
- `mod.rs`: catalogo, docs, dispatch e geracao de `rts.d.ts`.
- `abi.rs`: camada ABI (`__rts_eval_expr`, `__rts_call_dispatch`) + value store por handle.
- `state.rs`: estado compartilhado (buffers, globals, promises, executor async).
- submodulos por namespace:
  - `io`
  - `fs`
  - `process`
  - `crypto`
  - `global`
  - `buffer`
  - `promise`
  - `task`

## Runtime bootstrap (`src/runtime`)
- `bootstrap.rs`: interpretacao bootstrap e bind de runtime calls.
- `bootstrap_lang/`: lexer/parser/ast/evaluator de expressoes.
- `runtime_object.rs`: payloads/sections runtime para objetos.
- `runner.rs`: execucao de programa embutido.

## Linker e toolchains (`src/linker`)
- `mod.rs`: escolha backend (`auto`, `system`, `object`).
- `system_linker.rs`: integra linker do sistema (lld/link/clang etc).
- `toolchain.rs`: resolucao e download de toolchain/linker.

Resolucao de linker (resumo):
1. cache legacy `~/.rts/toolchains/<target>/bin`
2. cache novo `~/.rts/toolchains/rust-lld/<target>/...`
3. cache novo `~/.rts/toolchains/<tool>/<target>/...`
4. proximo ao `rts.exe`
5. `rustup` e `rustc` sysroot
6. `PATH`
7. download web (env + Rust dist), com cache em `~/.rts/toolchains/...`

## Artefatos gerados
- `target/.deps/*.o` e `*.m`: objetos e metadados de cache.
- `target/.deps/builtin_rts_<callee>.o|.m`: wrappers apenas das funcoes de namespace usadas.
- `target/.launcher/rts_namespace_catalog.json`: catalogo usado pelo launcher/runtime.

## Runtime support library
No AOT, o link usa tambem runtime support library:
- Windows: `rts.lib` ou `librts.lib`
- Unix-like: `librts.a` ou `rts.a`

Busca atual:
1. ao lado do binario RTS
2. `target/release` e `target/debug`
3. `target/.deps`
4. cache toolchain de runtime (quando configurado por download)
5. fallback `cargo build --lib` se necessario

## Typescript declarations
`packages/rts-types/rts.d.ts` e emitido a partir do catalogo de namespaces, com:
- comentario/doc por namespace e funcao
- assinaturas TS de cada API
- tipos base (`i8/u8/...`, `str`, streams, handles, etc)

## Exemplos
- `examples/console.ts`: `io.print` + `process.arch`.
- `examples/global_buffer_promise.ts`: `global`, `buffer`, `promise`, `task`.
- `examples/mixed/main.ts`: modulo local + runtime builtin.
