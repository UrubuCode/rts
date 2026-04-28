# RTS — Compilando TypeScript para nativo, mais rápido que Bun

**TL;DR:** RTS é um compilador TypeScript-to-native usando Cranelift como
backend. Ele compila TS direto pra binário nativo (AOT) ou roda via JIT
in-memory. Em benchmarks atuais, **RTS é 4,4× mais rápido que Bun e 6,1×
mais rápido que Node** em workloads de inteiros, e **1,4× mais rápido que
Bun** em Monte Carlo. Tudo isso com runtime mínimo em Rust e zero
dependência de V8/JSCore.

## O problema

JavaScript/TypeScript é a linguagem mais usada do mundo e tem dois grandes
runtimes: V8 (Node, Chrome) e JavaScriptCore (Bun, Safari). Ambos são JIT
sofisticados que aquecem em milissegundos e atingem perto de C em hot
loops. Mas:

1. **Startup é caro.** Bun leva ~70ms só pra carregar e parsear o
   `rts_simple.ts` antes de começar a executar. Para CLIs e scripts
   curtos, esse warmup é overhead puro.
2. **Distribuir é difícil.** Empacotar um app Node ou Bun exige enviar o
   runtime junto. Binários standalone existem (`bun build --compile`,
   `pkg`) mas geram executáveis de 50–100MB.
3. **Não dá pra prever desempenho.** JIT pode deoptimizar a qualquer
   momento. O mesmo código pode rodar 10× mais lento em um cenário
   ligeiramente diferente.

RTS tenta resolver os três: **compilação ahead-of-time direto pra
binário nativo**, sem runtime JS embutido, com semântica TS pragmática
(não 100% spec) que prioriza performance previsível.

## Como funciona

```
Source TS → SWC (parser) → AST → type_system → codegen (Cranelift) → object → linker → .exe
```

Não há HIR/MIR. O codegen consome direto o AST do SWC e emite IR
Cranelift, que vira código de máquina via:

- **AOT** (`rts compile`): `cranelift_object::ObjectModule` produz um
  `.o`/`.obj`, system linker (rust-lld) faz o final → binário nativo
  standalone (~3KB pro bench mínimo, sem runtime support DLL).
- **JIT** (`rts run`): `cranelift_jit::JITModule` aloca memória
  executável em-memória, resolve `__RTS_MAIN`, faz transmute pra
  `extern "C" fn() -> i32` e chama. Sem disk, sem linker, sem warmup
  visível.

O runtime é Rust. Cada namespace (`io`, `fs`, `math`, `string`, `net`,
`thread`, ...) declara um `NamespaceSpec` em `src/abi/` com membros
tipados e símbolos `__RTS_FN_NS_<NS>_<NAME>`. O codegen emite `call
<symbol>` direto — sem dispatch, sem boxing, sem JsValue. ABI 100%
extern "C" tipado (`i64`, `f64`, `i32`, ponteiros raw).

### Decisões interessantes

- **Vars top-level escapam só se referenciadas por user fn.** Análise
  conservadora elimina globals desnecessários — `let i = 0; while (i <
  N) i++` em top-level vira Cranelift Variable (registrador), não data
  global (memória). 5× speedup em loops puros.
- **Branchless if-to-select.** `if (cond) { var = expr }` (single-stmt,
  sem else) vira `var = select(cond, expr, var)`. Elimina branch
  imprevisível em hot loops como Monte Carlo. **20% speedup imediato**
  no MC clássico.
- **Silent parallelism.** `arr.map/forEach/reduce` com user fn são
  reescritas pra `parallel.map/...` em compile time, rodando em rayon
  thread pool. Sem `await`, sem `Worker`. User não pensa em threads.
  Funciona porque RTS tem `pure: true` flag em 96 fns nativas — codegen
  prova ausência de side effects.
- **HandleTable shard-aware.** Recursos GC-tracked (strings, buffers,
  TCP sockets, ...) vivem em uma tabela com 32 shards lock-free. Cada
  alloc round-robin, cada handle decodifica em O(1) qual shard sob
  Mutex. Sem contention em paralelismo, sem ABA sob reuso.
- **Operator overload Rust-style.** `a + b` em classe RTS vira
  `a.add(b)` quando o método existe. `==` vira `.eq`, `<` vira `.lt`,
  `[]` vira `.index`. Conhecido em Rust/C++, raro em JS.

## Performance

Benchmarks executados em Windows 11, x86-64 (CPU desktop), 100 runs com
5 warmups, comparando `rts compile -p` (AOT), `rts run` (JIT), Bun
1.x, Node 20.x.

### `bench/rts_simple.ts` (LCG + primes + bigint-like)

| Runner          | Mediana   | vs Bun           | vs Node           |
|-----------------|-----------|------------------|-------------------|
| **RTS AOT**     | **15,4 ms**   | **4,45× mais rápido** | **6,17× mais rápido** |
| RTS JIT         | 21,3 ms   | 3,36×            | 4,57×             |
| Bun (run)       | 70,9 ms   | 1×               | 1,38×             |
| Node (run)      | 98,1 ms   | 0,72×            | 1×                |

Workload integer-heavy. RTS dominante pelo overhead de startup
JS-runtime que Bun/Node carregam mesmo pra código simples.

### `bench/monte_carlo_pi.ts` (10M Monte Carlo, xorshift inline)

| Runner          | Mediana   | vs Bun           |
|-----------------|-----------|------------------|
| **RTS AOT**     | **50,4 ms**   | **1,41× mais rápido** |
| RTS JIT         | 57,3 ms   | 1,22×            |
| Bun (run)       | 71,5 ms   | 1×               |
| Node (run)      | 95,6 ms   | 0,75×            |

Esse era o calcanhar de Aquiles. Bun ganhava ~20-30% por causa do
autovec do V8 (SIMD em hot loops). RTS virou o jogo com **branchless
if-to-select**: o `if (x*x + y*y <= 1.0) inside++` virou um `select`
linear sem branch — sem stall do branch predictor (que falha ~50% no
caso uniforme de Monte Carlo).

### Tamanho de binário

- `bench/rts_simple.ts` AOT: **3,1 KB** (`target\rts_app.exe`)
- Mesmo programa Bun compiled: 65+ MB
- Node + esbuild bundled: 50+ MB

3KB pra um programa de inteiros com I/O. Sem runtime DLL. Roda em
qualquer máquina Windows sem instalar nada.

### Capacidades de linguagem

```ts
// Classes com layout nativo (opt-in #147)
class Point {
    x: f64; y: f64;
    constructor(x: f64, y: f64) { this.x = x; this.y = y; }
    distance(other: Point): f64 {
        const dx = this.x - other.x;
        const dy = this.y - other.y;
        return math.sqrt(dx*dx + dy*dy);
    }
}

// Operator overload Rust-style
class Vec2 {
    add(other: Vec2): Vec2 { /* ... */ }
}
const c = a + b;  // vira a.add(b) em compile time

// Silent parallelism — usuário não menciona threads
const doubled = numbers.map(n => n * 2);   // → parallel.map (rayon)
const sum = numbers.reduce((a, b) => a + b, 0);  // → parallel.reduce

// Tail call optimization
function factorial(n: i64, acc: i64): i64 {
    if (n <= 1) return acc;
    return factorial(n - 1, acc * n);  // vira return_call (sem stack)
}
```

Cobre TS suficiente pra escrever runtime libraries. Não cobre tudo:
async/await ainda não existe, generators idem, prototype chain
parcial. Mas o que cobre, cobre rápido.

## Arquitetura do runtime

```
src/
  abi/          — Contrato único de ABI (SPECS, tipos, símbolos, guards)
  codegen/      — Cranelift codegen (emit AOT, jit JIT, lower compartilhado)
  linker/       — System linker com fallback object backend
  namespaces/   — 32 namespaces runtime (io, fs, gc, math, ...)
  runtime/      — Builtin module "rts" + submódulos "rts:<ns>"
  module/       — Resolver de módulos + grafo de dependências
  parser/       — SWC parse + AST interno
  type_system/  — Type checker, registry, resolver
  pipeline.rs   — Orquestra build/run; inclui run_jit pra path JIT
```

Sem HIR/MIR — codegen consome AST direto. ABI cross-namespace é uma
única tabela (`abi::SPECS`) — codegen, runtime, JIT e gerador de
`rts.d.ts` consultam a mesma fonte. Nenhuma camada de dispatch.

### Namespaces ativos (32)

`io`, `fs`, `gc`, `math`, `num`, `bigfloat`, `time`, `env`, `path`,
`buffer`, `string`, `process`, `os`, `collections`, `hash`, `fmt`,
`crypto`, `net`, `tls`, `thread`, `atomic`, `sync`, `parallel`,
`mem`, `hint`, `ptr`, `ffi`, `regex`, `runtime`, `test`, `trace`,
`ui`, `alloc`, `json`, `date`.

Cobre std::* equivalente, paralelismo (rayon), HTTPS (rustls com CAs
embutidos), UI (FLTK), JSON (serde_json), Date (calendário Hinnant).

## O que está faltando

- **Loop vectorizer.** Cranelift não tem auto-vectorization. Em loops
  tight, V8/JIT do Java/LLVM ganham por SIMD. Issue #92 documenta —
  fora de escopo sem reimplementar.
- **async/await/Promise.** Modelo de cooperative scheduling não
  implementado. Plano: state machine desugar similar a generators.
- **Generators (`function*`).** Mesmo padrão.
- **Prototype chain JS-completa.** Array/String/Object methods cobertos
  parcialmente; cobertura grande de `Array.prototype.*` segue.
- **Source maps.** Stack traces atuais mostram fn names mas não linhas
  TS. Issue #96.

Roadmap completo em `NEXT_STEPS.md` e `ROAD_MAP.md`. Issue tracker
mestre é #226 (paridade JS/TS).

## Quando RTS faz sentido

- **Scripts curtos / CLIs.** Startup 70× mais rápido que Bun em
  workloads pequenos.
- **Compute-heavy single-threaded ou rayon-parallel.** Inteiros,
  números, hashes, parsers — RTS empata ou vence Bun.
- **Distribuir tooling.** Binário 3KB pra "hello world", sem
  dependências runtime.
- **Embedded / sandbox.** Runtime Rust deterministico, sem heap GC
  global, sem JIT no produto final.

## Quando NÃO faz sentido (ainda)

- **Web frontend.** Sem DOM, sem fetch nativo de browser, sem JSX
  (parser tem mas codegen não).
- **APIs heavy-async.** Sem Promise/await — workloads I/O-bound em
  pattern callback é forçado.
- **NPM ecosystem.** Sem prototype chain completa, ~80% das libs
  npm não roda. Issue #208 trabalha nisso.

## Como tentar

```bash
git clone https://github.com/UrubuCode/rts
cd rts
cargo build --release

# JIT em-memória
target/release/rts.exe run examples/hello_world.ts

# AOT — binário standalone
target/release/rts.exe compile -p examples/hello_world.ts hello
./hello.exe

# Suite de testes
target/release/rts.exe test
```

Examples em `examples/`. Benchmarks em `bench/`. Specs de design em
`docs/specs/`.

## O que mudou recentemente

Últimos commits trouxeram:

- **#222** Map/Set v0 (sem iteradores)
- **#220** Date API v0 (calendário UTC)
- **#215** JSON.parse/stringify
- **#213** Module exports/imports cross-file
- **#214** Catch param tipado por inferência
- **#221** console.log/error/warn como builtin global
- **#235** SIGSEGV em `String.indexOf` corrigido
- **#237** Aritmética de ponteiro u64 corrigida (#237 — type confusion entre Handle
  e U64)
- **Branchless if-to-select** em codegen (Monte Carlo +20%)
- **Globais top-level → Cranelift Variables** quando seguro (loops 5× mais
  rápidos)

## Filosofia

RTS é um experimento sobre **quanto da semântica TS é compilável
diretamente pra código nativo eficiente**. Não tenta ser um substituto
drop-in pra Node — tenta ser um runtime alternativo onde performance
previsível e binários pequenos importam mais que compatibilidade total
com npm.

Se você escreve TS pra rodar em backend, ferramentas internas, scripts
de automação, ou system tools — vale testar. Performance de C, sintaxe
de TS, sem runtime de 60MB.

---

**Status:** Pre-1.0, ainda evoluindo. Issues abertas em
github.com/UrubuCode/rts/issues. PRs bem-vindos.

**Licença:** MIT.

**Contato:** ver `package.json` / GitHub issues.
