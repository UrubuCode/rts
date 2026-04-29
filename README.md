# RTS

Compilador e runtime TypeScript-to-native baseado em Cranelift. Compila TS/JS
para binarios nativos com runtime minimo em Rust e um contrato ABI unico para
os namespaces builtin. Ha dois caminhos de execucao:

- **JIT** (`rts run`) — compila direto para memoria executavel via `cranelift_jit`,
  sem disco e sem linker externo. Latencia de startup drasticamente menor, ideal
  para dev loop.
- **AOT** (`rts compile`) — emite object file, linka com o linker do sistema,
  produz executavel standalone.

Namespaces ativos (32): `io`, `fs`, `gc`, `math`, `num`, `bigfloat`, `time`, `env`,
`path`, `buffer`, `string`, `process`, `os`, `collections`, `hash`, `fmt`, `crypto`,
`net`, `tls`, `thread`, `atomic`, `sync`, `parallel`, `mem`, `hint`, `ptr`, `ffi`,
`regex`, `runtime`, `test`, `trace`, `ui`, `alloc`. Cobre `std::*`, paralelismo,
HTTPS, UI nativa.

## Silent parallelism (zero esforço do user)

```ts
// User escreve isso:
const arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let sum = 0;
for (const x of arr) {
  sum = sum + x;
}
// Compilador detecta o padrão e roda em paralelo via rayon:
//   sum = parallel.reduce(arr, 0, __par_reduce_0);
```

Funciona pra `for...of` puros, `arr.map(fn)`/`.forEach(fn)`/`.reduce(fn, init)`,
e o padrão clássico de acumulador. Cobre top-level e bodies de fns. Detalhes
em `CLAUDE.md` § Silent parallelism.

## Stack runtime

- **TCP/UDP** via `net.*` — `std::net` puro, sem deps externas
- **HTTPS** via `tls.*` — `rustls` + `webpki-roots` (pure Rust, single binary)
- **Threading** via `thread/atomic/sync/parallel` — `std::thread` + `std::sync` +
  `rayon`. HandleTable shard-aware (32 shards lock-free entre si)
- **UI nativa** via `ui.*` — FLTK 1.x bindings

**Observação de perf (Windows, 100 runs com 5 warmups):**

`bench/rts_simple.ts` (LCG + primes + bigint-like):

| Runner          | Mediana | vs Bun           |
|-----------------|---------|------------------|
| **RTS AOT**     | **15,4 ms** | **4,45× mais rápido** |
| RTS JIT         | 21,3 ms | 3,36× mais rápido |
| Bun             | 70,9 ms | —                |
| Node            | 98,1 ms | 0,72× (mais lento) |

`bench/monte_carlo_pi.ts` (10M iters Monte Carlo):

| Runner          | Mediana | vs Bun           |
|-----------------|---------|------------------|
| **RTS AOT**     | **50,4 ms** | **1,41× mais rápido** |
| RTS JIT         | 57,3 ms | 1,22× mais rápido |
| Bun             | 71,5 ms | —                |
| Node            | 95,6 ms | 0,75× (mais lento) |

Binário AOT do `rts_simple` tem **~3 KB** (sem runtime DLL — funciona em
qualquer Windows sem instalar nada).

## Arquitetura

```
src/
  parser/            SWC parse + AST interno
  codegen/           Cranelift codegen direto sobre o AST (sem HIR/MIR)
    lower/           Lowering de expressoes/statements
    emit.rs          Object emitter (AOT)
    jit.rs           JIT emitter (rts run)
  abi/               Contrato ABI unico
    member.rs        NamespaceMember / NamespaceSpec / Intrinsic
    types.rs         AbiType
    signature.rs     Assinaturas Cranelift
    symbols.rs       Nomes oficiais dos simbolos
    guards.rs        Validacoes estaticas
    mod.rs           SPECS - registro dos namespaces ativos
  namespaces/        Implementacoes dos namespaces builtin
    io/              print, eprint, stdout/stderr/stdin
    fs/              read, write, metadata, dir, copy, rename, ...
    gc/              HandleTable (slab + generation) + string pool;
                     Entry enumera String/BigFixed/Buffer/ProcessChild/Map/Vec
    math/            f64/i64 intrinsics + xorshift64 PRNG + constantes
    bigfloat/        decimal fixed-point via i128 (~30 digitos)
    time/            monotonic clock, wall clock, sleep
    env/             get/set/remove vars, argv, cwd
    path/            join, parent, file_name, stem, ext, normalize, with_ext
    buffer/          Vec<u8> via HandleTable (alloc/read/write/copy/fill)
    string/          contains, trim, to_upper/lower, replace, char_count, find
    process/         exit/abort/pid, argv aliases, spawn/wait/kill
    os/              platform, arch, family, home/temp/config/cache_dir
    collections/     HashMap<string, i64> e Vec<i64> via HandleTable
    hash/            SipHash-2-4 deterministico (str/i64/bytes)
    fmt/             parse_i64/f64, fmt_hex/oct/bin, fmt_f64_prec
    crypto/          SHA-256 inline, base64/hex encode/decode, CSPRNG
    <ns>/mod.rs      import map
    <ns>/abi.rs      tabela estatica de NamespaceMember
    <ns>/rt.rs       re-exports para o runtime staticlib
  linker/            Link nativo (linker do sistema com fallback object)
  runtime_objects.rs Resolucao dos objetos de runtime support (.o/.obj)
  pipeline.rs        Orquestra compile/link/run (inclui run_jit)
  cli/               CLI (run, compile, apis, init)

builtin/
  console/           Package TS sobre o modulo "rts"
  globals/           Globais compartilhadas
  rts-types/
    rts.d.ts         Declaracoes TS geradas a partir de abi::SPECS
```

Pipeline AOT: `Source TS -> Parser (SWC) -> Codegen Cranelift -> Object -> Link -> binario`
Pipeline JIT: `Source TS -> Parser (SWC) -> Codegen Cranelift -> JITModule in-memory -> call __RTS_MAIN`

## Contrato ABI

- Fonte unica: `src/abi/`.
- `abi::SPECS` lista 35 namespaces ativos: `io`, `fs`, `gc`, `math`, `bigfloat`,
  `time`, `env`, `path`, `buffer`, `string`, `process`, `os`, `collections`,
  `hash`, `fmt`, `crypto`, `net`, `tls`, `thread`, `atomic`, `sync`, `parallel`,
  `mem`, `hint`, `ptr`, `ffi`, `regex`, `runtime`, `test`, `trace`, `ui`,
  `alloc`, `num`, `json`, `date`.
- Cada membro declara nome, parametros, retorno via `AbiType`, e opcionalmente
  um `Intrinsic` que permite ao codegen emitir IR inline ao inves de call extern
  (usado em `math.sqrt`, `math.random_f64`, etc).
- Cada funcao de namespace vira um simbolo nativo:
  `__RTS_FN_NS_<NS>_<NAME>` (uppercase ASCII).
- Dados expostos ao codegen (ex: estado do PRNG): `__RTS_DATA_NS_<NS>_<NAME>`.
- Codegen consulta `SPECS` para resolver simbolo e assinatura; nao existe
  dispatcher central nem boxing no limite `extern "C"`.
- `rts.d.ts` em `builtin/rts-types/` e gerado a partir dos `SPECS`.

Tipos primitivos no limite ABI:

| TS       | ABI          | Convencao                                                |
|----------|--------------|----------------------------------------------------------|
| `number` | `i64` / `f64`| bits nativos, sem boxing                                 |
| `bool`   | `i64`        | 0 = false, 1 = true                                      |
| `string` | `(i64, i64)` | `(ptr, len)` UTF-8 — literal estatica OU handle GC       |
| handle   | `u64`        | indice opaco para recursos (strings dinamicas, bigfloat) |

## Capacidades da linguagem

Suportadas no codegen:

- **Controle de fluxo**: if/else, while, do-while, for, switch (com jump table
  nativa quando todos os cases sao literais inteiros), break/continue.
- **Expressoes**: aritmetica (`+ - * / % **`), bitwise (`& | ^ ~ << >> >>>`),
  ternario (`a ? b : c`), logicos (`&& || ??`), comparacoes, assignment,
  compound assignment (`+= -= *= ... **=`), template literals (com
  interpolacao de qualquer tipo), `typeof` / `void` / `delete`, optional
  call (`fn?.()`), exponenciacao (`a ** b` via libc pow), modulo f64
  (`a % b` via libc fmod).
- **Funcoes**: declaracao, `function` expression, arrow functions (bloco ou
  expressao), tail call optimization (`return f(x)` vira `return_call`),
  ponteiros de funcao como valores de primeira classe (callbacks, higher-
  order functions como `apply(fn, x)` e `compose(f, g, x)`).
- **Escopo**: `let`/`const`/`var` com semantica de bloco; `const` impede
  reassignment.
- **Namespaces**: `import { io, math, ... } from "rts"` + `io.print(...)`,
  `math.sqrt(x)`, etc. Constantes via `math.PI`, `math.E`, sem parens.
- **Big decimal**: `bigfloat.add/sub/mul/div/sqrt` com handles de ~30 digitos
  decimais. Suficiente para calcular pi com 29 digitos corretos via Machin.
- **Containers**: `collections.map_*` e `collections.vec_*` via handles
  (HashMap<string, i64> e Vec<i64>), `buffer.alloc/read/write` pra bytes.
- **Object/array literals**: `{ k: v }` desugar em `map_*`, `[1, 2, 3]` em
  `vec_*`. Member access (`obj.x`, `obj["x"]`, `arr[i]`) e atribuicao
  (`obj.x = v`) suportados. Aninhamento livre.
- **Classes**: `class C { constructor(...) {...} method() {...} field: T }`,
  `new C(args)`, `this`, `extends`/`super(args)`, `super.method(args)`,
  static methods (`static m()` chamados via `C.m()`), getters/setters
  (`get x()`, `set x(v)`), dispatch virtual real (instancia armazena
  `__rts_class`; metodos overrideados em subclasses sao despachados via
  string-eq sobre o tag de runtime). Operator overload Rust-style: `a + b`
  vira `a.add(b)` em compile-time quando classe define o metodo
  (`add`/`sub`/`mul`/`div`/`rem`/`eq`/`ne`/`lt`/`le`/`gt`/`ge`/`bit_*`/`shl`/`shr`).
- **for...of**: itera sobre arrays (`vec_*`); bind herda classe quando
  array tem anotacao `: C[]` para habilitar dispatch de metodo.
- **try / catch / throw / finally** (fase 1): captura via slot de erro
  thread-local checado ao fim do try. Sem unwind real ainda — `throw` nao
  interrompe o fluxo (#128 rastreia fase 2 com Cranelift invoke).
- **String equality**: `s1 == s2` compara conteudo via `gc.string_eq`
  quando ambos os operandos sao Handle.

Cobertos parcial ou totalmente em commits recentes:
- **enum** numerico/string com auto-incremento (#212)
- **destructuring** array/object simples + nested + rename (#210 parcial — sem rest/default ainda)
- **spread** em array literal e object literal (#209 — call spread pendente)
- **regex** via `regex` namespace (`regex.compile/test/find/replace`)
- **abstract classes** com erro em `new`
- **default parameters** em fns
- **module exports**: `export function/class/const` + `import { x } from "./mod"` (#213)
- **JSON.parse/stringify** + namespace `json` (#215)
- **Date.now/parse** + namespace `date` (#220 v0)
- **console.log/error/warn/info/debug** sem import (#221)
- **Map/Set v0** com set/get/has/delete/clear/size (#222 — sem iteradores)
- **Array.prototype**: push/pop/length/at/clear (#208 v0)
- **String.prototype**: indexOf/includes/startsWith/endsWith/toLowerCase/etc (#235 fix)
- **catch param tipado** por inferência ou anotação (#214 parcial)

Não suportado ainda: `async/await`, generators, destructuring com defaults/rest,
call spread `f(...args)`, decorators, generics completos, satisfies. Closures
com captura mutável estão em fase 1 (#195 — env-record real pendente).

## CLI

```bash
rts run file.ts                       # executa via JIT in-memory
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

## Benchmarks

`bench/monte_carlo_pi.ts` (10M pontos, xorshift64 PRNG inline) e
`bench/pi_bigfloat.ts` (pi via formula de Machin com 30 digitos):

```
┌──────────────────┬──────────┬──────────┬────────┬────────┐
│       Bench      │ RTS JIT  │ RTS AOT  │  Bun   │  Node  │
├──────────────────┼──────────┼──────────┼────────┼────────┤
│ Monte Carlo 10M  │   57 ms  │   50 ms  │  72 ms │  96 ms │
├──────────────────┼──────────┼──────────┼────────┼────────┤
│ Machin bigfloat  │   47 ms  │   48 ms  │ 109 ms │ 108 ms │
└──────────────────┴──────────┴──────────┴────────┴────────┘
```

- Monte Carlo: RTS AOT **41% mais rápido que Bun** após otims de
  branchless if-to-select (commit 437095e). O `if (x*x + y*y <= 1.0)
  inside++` vira `select`, eliminando branch imprevisível em hot loop
  (~50% misprediction).
- Machin: pi = 16·atan(1/5) - 4·atan(1/239), atan via serie de Maclaurin em
  `bigfloat`. Resultado `3.141592653589793238462643383280` (29 digitos corretos,
  f64 entrega 16).

Suite `bench/benchmark.ps1` compara 4 runners (RTS compiled / RTS JIT /
Bun / Node) em `bench/rts_simple.ts`:

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1
```

Resultado típico (Windows, 100 runs): `RTS AOT ~15 ms`, `RTS JIT ~21 ms`,
`Bun ~71 ms`, `Node ~98 ms`. **RTS AOT 4,45× mais rápido que Bun.**

### Otimizações de codegen aplicadas

- **Branchless if-to-select** (#perf): `if (cond) { var = expr }` vira
  `var = select(cond, expr, var)` — sem branch, sem stall do branch
  predictor. Cobre compound assigns (`+=`, `*=`, etc).
- **Globais top-level → Cranelift Variables**: vars não referenciadas
  por user fns são promovidas a registradores. `let i = 0; while (i < N)
  i++` em top-level fica 5× mais rápido (sem load/store por iter).
- **`uextend` redundante eliminado** em comparações que vão direto pro
  `brif` — Bool nativo de 8 bits passa direto.
- **Lower duplicado em binops corrigido**: `try_operator_overload` /
  `try_bin_imm` faziam `lower_expr` da subexpr antes de saber se iam
  usar — geravam IR duplicado em todo binop não-overload.
- **f64 lit `1.0` direto** (sem conversão I32→F64): respeita `raw` do
  literal pra preservar tipo no source.
- **Cache de `FuncRef`/`GlobalValue`** por símbolo no FnCtx — Cranelift
  não dedupa entre `declare_*_in_func` calls separadas.
- **RNG state caching** entre calls consecutivas de `random_f64` no
  mesmo block.

## Runtime vs Compile (AOT)

JIT (`rts run`) e AOT (`rts compile`) compartilham o mesmo pipeline de
codegen. Diferencas:

- `rts run`: compila para memoria executavel, sem disco. Todos os simbolos do
  ABI sao registrados em `JITBuilder::symbol` no startup; nao passa pelo
  linker do sistema.
- `rts compile`: aplica slicing por uso, emite objects + binario final.

Runtime support AOT e resolvido por objetos `.o/.obj` precompilados
(`runtime_objects.rs` + `runtime_support.a` produzido por `build.rs`). Nao ha
download de runtime support.

Artefatos auxiliares vivem em `node_modules/.rts/`:

```
node_modules/.rts/
  objs/              cache de objetos (.o) + metadata por modulo
  modules/           modulos resolvidos
```

## Codegen — otimizacoes notaveis

- **Intrinsics inline** (`abi::Intrinsic`): `sqrt`, `abs`, `min`, `max`,
  `random_f64` emitidos como IR Cranelift direto no call site.
- **Tail call optimization**: user functions usam `CallConv::Tail`;
  `return f(x)` emite `return_call`. Recursao profunda nao estoura stack.
- **First-class function pointers**: `const f = double; f(5)` funciona.
  Indireto via `call_indirect` com signature provisoria Tail.
- **Jump table switch**: cases com inteiros literais viram `br_table` via
  `cranelift_frontend::Switch`.
- **Imm forms**: `x + 1` emite `iadd_imm` direto.
- **MemFlags::trusted**: loads/stores de globals e estado runtime.
- **f64 modulo**: via libc `fmod` (antes truncava via i64).
- **Constantes como propriedades**: `math.PI` (sem parens) resolve em
  `MemberKind::Constant`.

## Pacotes TS suportados

- import relativo (`./`, `../`)
- import de pacote do workspace (`import { log } from "console"`)
- import builtin (`import { io } from "rts"`)
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

## GC / Handles

`src/namespaces/gc/handles.rs` implementa uma `HandleTable` slab-based com
generation de 16 bits + slot de 48 bits. Usada para strings dinamicas, big
decimals e qualquer recurso runtime que precise de handle opaco `u64`.
Desalocacao explicita via `*_free(handle)`.

Para codigo que precisa de escopo deterministico em memoria nativa, o crate
`gc-arena` e dependencia disponivel mas o runtime principal nao faz uso
periodico — coleta acontece em pontos de quiescencia quando e aplicavel.

## Build e testes

```bash
cargo test                                    # testes unitarios + fixtures
cargo build --release                         # build release
target/release/rts.exe run file.ts            # executar (JIT)
target/release/rts.exe compile -p file.ts o   # compilar AOT
target/release/rts.exe apis                   # listar APIs
```

Fixtures de codegen vivem em `tests/fixtures/*.{ts,out}` — o teste compila o
`.ts` e compara stdout com o `.out` byte-a-byte.

## Direcao

Ver `NEXT_STEPS.md` e `ROAD_MAP.md`. Issues de codegen priorizadas em trilhas
paralelas (A peephole, B loops/TCO, C closures, D modulo, E perf avancada) —
ver comentarios nas issues para dependencias e ordem.

Guardrails: sem `xtask`, sem download de runtime support em tempo de build,
sem dependencia de Rust/Cargo no ambiente de uso final do binario AOT.

## Documentação adicional

- **`BLOG_POST.md`** — visão geral pra dev de fora: o que é, performance,
  quando faz sentido usar, decisões interessantes de design.
- **`CLAUDE.md`** — instruções pro AI assistant + arquitetura interna,
  regras do codebase, ABI, modos de debug. Inclui seção sobre
  `rts ir` pra inspecionar IR Cranelift gerado.
- **`docs/specs/`** — specs detalhadas de features e decisões de design.
- **Issues abertas** em github.com/UrubuCode/rts/issues. Tracker mestre
  de paridade JS/TS é #226.

### Debug rápido

```bash
target/release/rts.exe ir file.ts 2>&1 | head -50
```

Imprime IR Cranelift de cada user fn + `__RTS_MAIN` sem executar o programa.
Útil pra ver duplicações no codegen, loads/stores redundantes em hot loops,
oportunidades de otim. Ver `CLAUDE.md` § Debug do codegen.
