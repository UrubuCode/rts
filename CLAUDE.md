# CLAUDE.md

## Projeto

RTS e um compilador/runtime TypeScript-to-native usando Cranelift como backend de codegen.
O objetivo e compilar TS/JS para binarios nativos com runtime minimo em Rust, distribuido como
toolchain standalone (sem runtime support library externa).

A camada de runtime e organizada em torno do contrato `src/abi/` + `SPECS`, com pipeline
por grafo de modulos + cache incremental. Dois caminhos de execucao coexistem: AOT via
`cranelift_object::ObjectModule` (linker externo) e JIT via `cranelift_jit::JITModule`
(memoria executavel direta, ativado por `RTS_JIT=1`).

Consultar `NEXT_STEPS.md` e `ROAD_MAP.md` para a direcao vigente.

## Arquitetura

```
src/
  abi/          — contrato unico de ABI (SPECS, tipos, simbolos, guards, assinaturas, Intrinsic)
  codegen/      — Cranelift codegen
    emit.rs     — ObjectModule emitter (AOT)
    jit.rs      — JITModule emitter (rts run com RTS_JIT=1)
    lower/      — lower de expr/stmt/func sobre &mut dyn Module
  linker/       — link nativo (system linker com fallback object backend)
  namespaces/   — implementacoes dos namespaces runtime: io, fs, gc, math, bigfloat
  runtime/      — builtin module "rts" + submodulos "rts:<ns>"
  module/       — resolver de modulos e grafo de dependencias
  parser/       — SWC parse + AST interno; converte arrow/fn expressions em Item::Function
                  top-level
  type_system/  — type checker, registry, resolver
  diagnostics/  — erros estruturados
  cli/          — CLI (run, compile, apis, repl, eval)
  pipeline.rs   — orquestra build/run; inclui run_jit para path JIT
  lib.rs        — API publica
  runtime_objects.rs — resolucao dos objetos de runtime support (.o/.obj, AOT)
  main.rs       — entrypoint do binario `rts`
```

Pipeline AOT: `Source TS → Parser(SWC) → type_system → codegen(Cranelift) → Object → Linker → .exe`
Pipeline JIT: `Source TS → Parser(SWC) → type_system → codegen(Cranelift) → JITModule → call __RTS_MAIN`

`FnCtx.module` e `&mut dyn Module` para servir ambos os paths sem duplicar codegen. Nao
existem mais camadas HIR/MIR separadas — o codegen consome direto a AST (com tipagem
resolvida) e emite Cranelift IR em `src/codegen/lower/`.

## ABI (`src/abi/`) — contrato unico

Toda a superficie entre codegen e runtime passa por `src/abi/`. Nao existe mais
`SPEC/MEMBERS/dispatch()` por namespace e nao existe mais `__rts_call_dispatch`.

- `abi::SPECS` (`mod.rs`) — slice estatico com a `NamespaceSpec` de cada namespace registrado
  (`io`, `fs`, `gc`, `math`, `bigfloat`). Fonte unica consumida por codegen, runtime, JIT e
  gerador de `rts.d.ts`.
- `abi::lookup(qualified)` — resolve `"io.print"` → `&NamespaceMember` com simbolo e assinatura.
- `member.rs` — `NamespaceSpec`, `NamespaceMember` (const estaticos) e `Intrinsic` (enum das
  ops inlinaveis). Cada membro declara `name`, `kind` (Function|Constant), `symbol`, `args[]`,
  `returns`, `doc`, `ts_signature`, `intrinsic: Option<Intrinsic>`. Quando `intrinsic` e
  `Some`, codegen emite IR Cranelift direto em vez de `call <symbol>`.
- `types.rs` — `AbiType`: `Void | Bool | I32 | I64 | U64 | F64 | StrPtr | Handle`. `StrPtr`
  expande para dois slots Cranelift (`ptr` + `len`).
- `signature.rs` — `lower_member()` converte a spec em `LoweredSignature` Cranelift.
- `symbols.rs` — convencao `__RTS_<KIND>_<SCOPE>_<NS>_<NAME>` (ex: `__RTS_FN_NS_IO_PRINT`).
  Macro `rts_sym!` gera simbolos em compile-time; `validate_symbol()` impoe uppercase ASCII.
- `guards.rs` — `guard_for(expected, caller)` decide passthrough/coerce/trap em call sites
  com argumentos de tipo `any`.

Codegen emite `call <symbol>` direto via Cranelift, sem intermediarios.

## Estrutura de Arquivos por Namespace

```
src/namespaces/<ns>/
  mod.rs         — re-exporta submodulos e publica a NamespaceSpec
  abi.rs         — declaracao dos NamespaceMember (tabela estatica)
  <grupo>.rs     — impl operacional (ex: read.rs, write.rs, dir.rs, print.rs, stdout.rs, ...)
```

Regras:
- `mod.rs` e apenas o import map + export do `NamespaceSpec`
- `abi.rs` e a fonte da verdade dos membros do namespace (nome, simbolo, args, return, doc, ts)
- Cada arquivo operacional agrupa funcoes por responsabilidade (io/r-w/dir/metadata/…)
- Nao existe `dispatch()` por namespace — cada funcao e um `#[no_mangle] extern "C"` direto

Namespaces ativos: `io`, `fs`, `gc`, `math`, `bigfloat`.
Demais (net, process, crypto, buffer, thread, etc) serao reintroduzidos sobre o contrato
atual a medida que o pipeline estabiliza. Ver issues #12-#39 para o backlog.

### Namespaces existentes

- `io/` — print, eprint, stdout_{write,flush}, stderr_{write,flush}, stdin_{read,read_line}
- `fs/` — read, read_all, write, append, exists, is_file, is_dir, size, modified_ms,
  create_dir(_all), remove_dir(_all), remove_file, rename, copy
- `gc/` — handles e string pool: string_from_{i64,f64,static}, string_{new,concat,len,ptr,free},
  `HandleTable` slab-based com 16-bit geracao + 48-bit slot (`u64` handle); `Entry` enumera
  tipos armazenados (`String`, `BigFixed`, `Free`)
- `math/` — basic (floor/ceil/round/trunc/sqrt/cbrt/pow/exp/ln/log2/log10/abs_f64/abs_i64),
  trig (sin/cos/tan/asin/acos/atan/atan2), minmax (min/max/clamp_f64/i64), consts
  (PI/E/INFINITY/NAN como `MemberKind::Constant`), random (xorshift64 com estado em
  `__RTS_DATA_NS_MATH_RNG_STATE`). Intrinsics: sqrt/abs_f64/min_f64/max_f64/abs_i64/
  min_i64/max_i64/random_f64
- `bigfloat/` — decimal fixed-point via i128 (scale decimal ate 36). Operacoes:
  zero/from_f64/from_i64/from_str/to_f64/to_string/add/sub/mul/div/neg/sqrt/free.
  Usado para pi com 29+ digitos via Machin + atan de Maclaurin

## Convencoes

- Linguagem do codigo: Rust (ingles nos identificadores)
- Linguagem de comunicacao: portugues
- Commits seguem conventional commits: `feat:`, `fix:`, `perf:`, `refactor:`, `docs:`, `chore:`
- Novo namespace precisa ser registrado em: `abi::SPECS` (e o `rts.d.ts` gerado a partir dai)
- O `rts.d.ts` e gerado a partir de `abi::SPECS` — CI lintao committed file contra o gerador
- Build e via `cargo` direto — `xtask` foi removido

## Como testar

```bash
cargo test                                        # testes unitarios + fixtures
cargo build --release                             # build release
target/release/rts.exe run file.ts                # executar (AOT default)
RTS_JIT=1 target/release/rts.exe run file.ts      # executar via JIT in-memory
target/release/rts.exe compile -p file.ts output  # compilar nativo (AOT)
target/release/rts.exe apis                       # listar APIs disponiveis
```

Fixtures de codegen vivem em `tests/fixtures/*.{ts,out}`. O teste
`codegen_fixtures` compila o `.ts` e compara stdout com o `.out`
byte-a-byte. Para adicionar nova fixture:

1. `tests/fixtures/<name>.ts` — programa
2. `tests/fixtures/<name>.out` — saida esperada (LF, sem CRLF)
3. `#[test] fn fixture_<name>() { run_fixture("<name>") }` em
   `tests/codegen_fixtures.rs`

## Benchmarks

Benches canonicos em `bench/`:

- `monte_carlo_pi.ts` — estimacao de pi por Monte Carlo 10M (xorshift64 inline)
- `pi_bigfloat.ts` — pi via Machin 30 digitos usando `bigfloat`
- `pi_machin.ts` — pi via Machin em f64 (16 digitos)

Placar atual (AOT + JIT vs Bun/Node, medianas):

| Bench            | RTS JIT | RTS AOT | Bun    | Node   |
|------------------|---------|---------|--------|--------|
| Monte Carlo 10M  | 119 ms  | 156 ms  | 173 ms | 281 ms |
| Machin bigfloat  | 47 ms   | 48 ms   | 109 ms | 108 ms |

Suite completa:

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```

## Regras

- Nao implementar APIs de alto nivel em Rust — Rust so expoe primitivas raw via `"rts"`
- Packages TS em `builtin/*` constroem APIs ergonomicas sobre o `"rts"`
  (nesta branch: `console/`, `globals/`, `rts-types/`)
- `rts.d.ts` so contem `declare module "rts"` — nao adicionar outros modulos
- Handles numericos (u64) para recursos runtime (buffers, sockets, strings dinamicas, etc)
- Distribuicao standalone: runtime support resolvido por objetos `.o/.obj`
  precompilados (via `RTS_RUNTIME_OBJECTS_DIR` ou pasta `runtime-objects` ao lado do `rts`);
  nao dependemos de download externo em tempo de build

## Sem Codigo Legacy

**Regra absoluta: codigo morto e removido imediatamente. Nunca comentar, nunca deixar "por precaucao".**

- Qualquer codigo que nao e chamado por nenhum caminho vivo deve ser deletado no mesmo commit
  que o tornou morto
- Stubs `todo!()` / `unimplemented!()` sao aceitaveis como marcador temporario de WIP;
  codigo comentado nao
- Warnings de `dead_code` sao tratados como erros — o build nao pode terminar com warnings

## ABI de Maquina — extern "C" tipado, sem dispatch

Nao ha `JsValue`, nem `__rts_call_dispatch`, nem boxing no limite entre codegen e runtime.
Cada funcao de namespace e um simbolo `extern "C"` tipado.

### Convencao ABI por tipo

| Tipo TS  | `AbiType`    | Representacao Cranelift         | Observacao                                              |
|----------|--------------|---------------------------------|---------------------------------------------------------|
| `number` | `I64` / `F64`| `i64` / `f64`                   | bits nativos, sem boxing                                |
| `bool`   | `Bool`       | `i8` (0/1)                      | 0 = false, 1 = true                                     |
| `string` | `StrPtr`     | 2 slots: `(i64 ptr, i64 len)`   | UTF-8; ptr estatica do codegen, ou buffer via handle GC |
| handle   | `Handle`     | `u64`                           | `HandleTable` (gen:16 + slot:48)                        |
| void    | `Void`       | —                               | sem retorno                                             |
| inteiros| `I32` / `U64`| `i32` / `u64`                   | usados em contagens, status, tamanhos                   |

### Regras de implementacao

- Cada membro de namespace vira um `#[unsafe(no_mangle)] pub extern "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`
- Nenhuma funcao de namespace aceita/retorna `JsValue` no limite `extern "C"`
- Strings dinamicas (ex: resultado de leitura) sao alocadas pelo `gc` e retornam um handle `u64`;
  leitura via `gc::string_ptr(handle)` + `gc::string_len(handle)`
- Call sites com argumentos `any` passam por `abi::guards::guard_for(...)` para decidir coerce/trap

## Runtime vs Compile (AOT) vs JIT

Tres rotas de execucao compartilhando o mesmo codegen Cranelift:

- **`rts run` (AOT, default)**: inclui todos os modulos builtin nos objects; escreve `.o`,
  chama linker do sistema, executa o binario linkado. Cache em `node_modules/.rts/`.
- **`rts run` com `RTS_JIT=1`**: compila direto para memoria executavel via `JITModule`.
  Sem disco, sem linker externo. Startup drasticamente mais rapido para dev loop. Todos os
  simbolos do ABI sao registrados em `JITBuilder::symbol` no startup do modulo JIT
  (`src/codegen/jit.rs`).
- **`rts compile`**: aplica slicing por uso, gera apenas os objects dos modulos efetivamente
  utilizados, produz binario final.

`FnCtx.module` e `&mut dyn Module` — `ObjectModule` e `JITModule` implementam o mesmo trait
e passam pelo mesmo pipeline de `compile_program`.

Convencao de nomes de object: `<module>.o` (e `.m` quando houver metadata para cache
incremental).

## Otimizacoes de codegen notaveis

- **Intrinsics inline** (`abi::Intrinsic`): `sqrt`, `abs_f64`, `min/max_f64`, `abs_i64`,
  `min/max_i64`, `random_f64` — emitidos como IR Cranelift direto em `lower_intrinsic`
- **Tail call optimization**: user functions em `CallConv::Tail`; `return f(x)` em posicao de
  tail emite `return_call` (exige `preserve_frame_pointers=true` em x86-64)
- **First-class function pointers** (#97 fase 1): `Expr::Ident` resolvendo a user fn
  materializa `func_addr` como i64; call via ident local/param faz `call_indirect` com
  signature provisoria Tail
- **Jump table switch**: quando todos os non-default cases sao literais inteiros, usa
  `cranelift_frontend::Switch` (backend decide `br_table` vs binary search)
- **Imm forms**: `x + N` / `x & MASK` / `x << K` emitem `iadd_imm` / `band_imm` / `ishl_imm`
  sem iconst intermediario
- **MemFlags::trusted** em loads/stores de globals e RNG state
- **f64 modulo** via libc `fmod` (antes truncava via i64 perdendo a parte fracionaria)
- **Constantes como propriedades** (`math.PI` sem parens) via `MemberKind::Constant` +
  `emit_constant_load`

## Otimizacoes pendentes / backlog

Ver issues abertas #90, #96, #97 (fases 2/3). #92 autovec foi fechada como inviavel sem
loop vectorizer proprio (Cranelift nao tem um); Bun ganha em Monte Carlo >1B iter por
autovec do V8.

## Layout de Artefatos do Usuario

Alvo da Fase 1 do roadmap (em progresso):

```
<project>/
  src/main.ts
  package.json
  tsconfig.json

  node_modules/.rts/
    objs/
      runtime/        — objects completos do builtin (todos os modulos)
      compile/        — objects AOT com slicing (apenas em rts compile)
    modules/          — modulos resolvidos e cacheados (com metadata .ometa)

  release/            — apenas em rts compile
    <project_name>    — .exe / .dll / .so / .node conforme target
```

## GC — gc-arena (coleta deterministica)

Usar o crate `gc-arena` como sistema de GC deterministico. Coleta e disparada apos:
- Retorno de funcoes
- Execucao de metodos de classe
- Fim de escopo de closures

Principio: `safe_collect()` e chamado em pontos de quiescencia bem definidos, nao de forma
periodica/assincrona. O namespace `gc` publica a API de alocacao de strings e o `HandleTable`.

## State

Estado de namespace usa `Arc<Mutex<T>>` direto quando necessario, ou `thread_local!` para caches
por-thread. Nao ha sistema centralizado de state — cada namespace gerencia o seu.

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

## Docs e especificacoes

A pasta `docs/specs/` contem especificacoes de features, decisoes de design e notas tecnicas.
Consultar o indice em `docs/specs/INDEX.md`. Direcao de alto nivel fica em `NEXT_STEPS.md` e
`ROAD_MAP.md` na raiz.
