# RTS

Compilador e runtime TypeScript-to-native baseado em Cranelift. Compila TS/JS
para binarios nativos com runtime minimo em Rust e um contrato ABI unico para
os namespaces builtin. Ha dois caminhos de execucao:

- **JIT** (`rts run`) — compila direto para memoria executavel via `cranelift_jit`,
  sem disco e sem linker externo. Latencia de startup drasticamente menor, ideal
  para dev loop.
- **AOT** (`rts compile`) — emite object file, linka com o linker do sistema,
  produz executavel standalone.

Namespaces ativos (18): `io`, `fs`, `gc`, `math`, `bigfloat`, `time`, `env`, `path`,
`buffer`, `string`, `process`, `os`, `collections`, `hash`, `fmt`, `crypto`,
`thread`, `atomic`.

## Multi-thread implícito (diferencial)

RTS é o único runtime de TS que entrega **paralelismo real com semântica
single-thread observável**. O dev escreve código JS-like normal; o compilador
detecta capturas mutáveis e injeta primitivas atômicas / locks
automaticamente.

```ts
let counter = 0;

const handles: number[] = [];
for (let i = 0; i < 8; i++) {
    handles.push(thread.spawn(() => {
        for (let k = 0; k < 1_000_000; k++) {
            counter = counter + 1;  // ← compilador injeta atomic.i64_fetch_add
        }
    }, 0));
}
for (const h of handles) thread.join(h);
console.log(counter);  // 8000000 exato — sem race condition
```

**Sem** o compilador, esse código teria race em qualquer outra runtime —
em Node/Bun/Deno, `let counter` simplesmente **não pode ser compartilhado**
entre Worker threads (precisa `SharedArrayBuffer` + `Atomics`).

**Otimização thread-local accumulation** detecta loops apertados de
`fetch_add(x, lit)` e transforma em acumulador local + 1 fetch_add no fim,
eliminando contention de cache. Em bench medido (8 threads × 50M iter),
speedup vs atomic puro: **~684×**.

Veja [`docs/specs/multi-thread-implicito.md`](docs/specs/multi-thread-implicito.md)
para o modelo completo, garantias e limitações.

**Observação de perf (Windows, `rts_simple.ts`, 10 runs):**

| Runner          | Mediana | vs Bun           |
|-----------------|---------|------------------|
| RTS (compiled)  |  15 ms  | **5.9× faster**  |
| RTS (run, JIT)  |  22 ms  | **4.1× faster**  |
| Bun             |  88 ms  | —                |
| Node            | 118 ms  | 0.75× slower     |

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
- `abi::SPECS` lista os 16 namespaces ativos: `io`, `fs`, `gc`, `math`, `bigfloat`,
  `time`, `env`, `path`, `buffer`, `string`, `process`, `os`, `collections`,
  `hash`, `fmt`, `crypto`.
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

Nao suportado ainda: `async/await`, generators, destructuring, spread/rest,
regex, decorators, generics, abstract classes, satisfies, enum, default
parameters. Closures com captura de variaveis externas estao em fase 1
(ponteiros de funcao sem env).

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
│ Monte Carlo 10M  │  119 ms  │  156 ms  │ 173 ms │ 281 ms │
├──────────────────┼──────────┼──────────┼────────┼────────┤
│ Machin bigfloat  │   47 ms  │   48 ms  │ 109 ms │ 108 ms │
└──────────────────┴──────────┴──────────┴────────┴────────┘
```

- Monte Carlo: xorshift64 inline via intrinsic (`math.random_f64`). Mesmo
  `inside = 7854393` deterministico em AOT e JIT.
- Machin: pi = 16·atan(1/5) - 4·atan(1/239), atan via serie de Maclaurin em
  `bigfloat`. Resultado `3.141592653589793238462643383280` (29 digitos corretos,
  f64 entrega 16).

Suite `bench/benchmark.ps1` compara 5 runners (RTS compiled / RTS run
JIT / RTS run AOT / Bun / Node) em `bench/rts_simple.ts`:

```bash
powershell.exe -ExecutionPolicy Bypass -File bench/benchmark.ps1 -Runs 10 -Warmup 2
```

Numeros tipicos (Windows, 10 runs): `RTS compiled ~15 ms`, `RTS JIT ~22 ms`,
`Bun ~88 ms`, `Node ~118 ms`. **RTS JIT ~4× mais rapido que Bun.**

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
