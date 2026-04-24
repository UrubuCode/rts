# RTS

Compilador e runtime TypeScript-to-native baseado em Cranelift. O objetivo e
compilar TS/JS para binarios nativos com runtime minimo em Rust e um contrato
ABI unico para os namespaces builtin.

Branch atual: `feat/remake-namespaces` — reconstrucao dos namespaces sobre a
camada `src/abi/`. Somente `io`, `fs` e `gc` estao ativos nesta branch; os
demais serao reintroduzidos gradualmente sobre o novo contrato.

## Arquitetura

```
src/
  parser/          SWC parse + AST interno
  codegen/         Cranelift codegen direto a partir do AST (sem HIR/MIR)
    lower/         Lowering de expressoes e statements
  abi/             Contrato ABI unico
    member         NamespaceMember / NamespaceSpec
    types          AbiType
    signature      Assinaturas de chamada
    symbols        Nomes oficiais dos simbolos
    guards         Validacoes estaticas
    mod.rs         SPECS - registro dos namespaces ativos
  namespaces/      Namespaces builtin
    io/            print, println, ...
    fs/            read, write, ...
    gc/            primitivas de coleta
    <ns>/mod.rs    import map
    <ns>/abi.rs    tabela estatica de NamespaceMember
    <ns>/<op>.rs   implementacao operacional (um arquivo por grupo de funcao)
  linker/          Link nativo (linker do sistema com fallback)
  runtime_objects.rs Resolucao dos objetos de runtime support (.o/.obj)
  pipeline.rs      Orquestra build/run
  cli/             CLI (run, compile, apis, init)

builtin/
  console/         Package TS sobre o modulo "rts"
  globals/         Globais compartilhadas
  rts-types/
    rts.d.ts       Declaracoes TS geradas a partir de abi::SPECS (CI linta)
```

Pipeline: `Source TS -> Parser (SWC) -> Codegen Cranelift -> Object -> Link -> binario`.

## Contrato ABI

- Fonte unica: `src/abi/`.
- `abi::SPECS` lista os namespaces ativos (`io`, `fs`, `gc`).
- Cada membro declara nome, parametros e retorno via `AbiType`.
- Cada funcao de namespace vira um simbolo nativo:
  `__RTS_FN_NS_<NS>_<NAME>` (uppercase ASCII).
- Codegen consulta `SPECS` para resolver simbolo e assinatura da chamada; nao
  existe dispatcher central nem boxing no limite extern "C".
- `rts.d.ts` em `builtin/rts-types/` e gerado a partir dos `SPECS`. Somente
  `declare module "rts"`.

Tipos primitivos no limite ABI:

| TS       | ABI          | Convencao                                       |
|----------|--------------|-------------------------------------------------|
| `number` | `i64` / `f64`| bits nativos, sem boxing                        |
| `bool`   | `i64`        | 0 = false, 1 = true                             |
| `string` | `(i64, i64)` | `(ptr, len)` UTF-8 estatica do codegen          |
| handle   | `u64`        | indice opaco para recursos (buffers, strings dinamicas) |

## CLI

```bash
rts run file.ts                       # executa (runtime, todos os builtins)
rts compile -p file.ts output         # AOT com slicing por modulo usado
rts apis                              # lista APIs registradas em abi::SPECS
rts init                              # gera projeto base
rts init my-app
```

Tambem funciona via Cargo:

```bash
cargo run -- run examples/console.ts
cargo run -- compile -p examples/console.ts target/console
cargo run -- apis
```

## Runtime vs Compile (AOT)

Runtime e AOT compartilham o mesmo pipeline de codegen. A diferenca e o escopo:

- `rts run` gera objects completos dos modulos builtin (todos os namespaces
  ativos presentes).
- `rts compile` aplica slicing e gera somente os objects efetivamente usados,
  emitindo o binario final.

Runtime support e resolvido por objetos `.o/.obj` precompilados
(`runtime_objects.rs`). Nao ha download de runtime support e nao ha fallback
para `cargo build --lib` no ambiente do usuario.

Artefatos auxiliares vivem em `node_modules/.rts/`:

```
node_modules/.rts/
  objs/            cache de objetos (.o) + metadata por modulo
  modules/         modulos resolvidos
```

(Layout alvo da Fase 1 do road map; ver `ROAD_MAP.md`.)

## Pacotes TS suportados

- import relativo (`./`, `../`)
- import de pacote do workspace (`import { log } from "console"`)
- import builtin (`import { print } from "rts"`)
- import de URL externa (`https://...`)
- dependencia em `package.json` (`npm:<versao>`, URL externa, path local)

## Modos de compilacao

- `--development` / `-d`: trace detalhado de imports/modulos em erros.
- `--production` / `-p`: erros resumidos por codigo (`RTSXXXXXXXX`).
- `--debug` / `-D`: detalhes extras em cima do modo selecionado.

## Linker nativo

Estrategia via `RTS_LINKER_BACKEND`:

- `auto` (padrao): tenta linker do sistema e cai para backend manual (`object`).
- `system`: exige linker do sistema.
- `object`: usa apenas o backend manual.

Configuracoes auxiliares:

- `RTS_TARGET=<target-triple>` escolhe target explicitamente.
- `RTS_TOOLCHAINS_PATH=<path>` altera o cache local de toolchains.

## GC

Usa o crate `gc-arena` como sistema deterministico. `safe_collect()` e chamado
em pontos de quiescencia (retorno de funcoes, fim de metodos, fim de escopo de
closures), nao de forma periodica.

## Build e testes

```bash
cargo test                                    # testes unitarios
cargo build --release                         # build release
target/release/rts.exe run file.ts            # executar
target/release/rts.exe compile -p file.ts o   # compilar AOT
target/release/rts.exe apis                   # listar APIs
```

Build padrao e via `cargo` puro — sem `xtask`.

## Benchmarks

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```

Compara `rts run`, `rts compile`, Bun e Node.

## Direcao

Ver `NEXT_STEPS.md` e `ROAD_MAP.md`. Resumo:

- Fase 1: reancorar no fluxo da `main` — pipeline por grafo de modulos
  (`compile_graph`), cache incremental de `.o` + metadata, runtime support
  interno, artefatos em `node_modules/.rts`.
- Fase 2: consolidar a API nova — `abi::SPECS` como fonte unica para codegen,
  runtime e tipos; `io`, `fs`, `gc` completos no fluxo novo; `rts.d.ts`
  sincronizado.
- Fase 3: migracao gradual das melhorias de codegen da bench nova, medindo
  impacto por lote.

Guardrails: sem `xtask`, sem download de runtime support, sem dependencia de
Rust/Cargo no ambiente de uso final.
