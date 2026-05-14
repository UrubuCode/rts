<div align="center">

<img src="assets/logo.png" alt="RTS — TypeScript que voa" width="220" />

# `rts_`

### **TypeScript compilado pra binário nativo. Sem runtime. Sem GC pesado. Sem desculpa.**

*Um urubu de óculos escuros não tem pressa — ele já chegou.*

[![Cranelift](https://img.shields.io/badge/backend-Cranelift-orange?style=flat-square)](https://cranelift.dev)
[![Rust](https://img.shields.io/badge/runtime-Rust-black?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Single Binary](https://img.shields.io/badge/output-single%20binary-blue?style=flat-square)](#)
<!-- CROSS_RUNTIME_BADGE_START -->
[![Bun/Node parity](https://img.shields.io/badge/Bun%2FNode%20parity-45.6%25-orange?style=flat-square)](docs/specs/cross-runtime-testing.md)
<!-- CROSS_RUNTIME_BADGE_END -->

</div>

<!-- CROSS_RUNTIME_STATS_START -->
## 🌐 Cross-runtime parity

Compatibilidade JS spec validada contra **Bun** e **Node** em 312 fixtures TS standalone.

```
[▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱] 45.6%   141/309 fixtures passam
```

| Métrica | Valor |
|---|---|
| **Paridade** | **45.6%** (141/309) |
| ✅ RTS = Bun = Node | 141 |
| ❌ RTS diverge | 65 |
| 💥 RTS runtime error | 103 |
| 🛠️  **Falta corrigir** | **168** |
| ⚠️ Bun ≠ Node (skip) | 3 |
| 🚫 Rejeitados (RTS-only) | 0 |
| 📦 Total fixtures | 312 |

_Atualizado: 2026-05-14 — [como adicionar fixture](docs/specs/cross-runtime-testing.md)_

<!-- CROSS_RUNTIME_STATS_END -->

---

## 🦅 O que é

**RTS** é um compilador + runtime que pega seu `.ts` e cospe um `.exe` nativo.
Não é transpilador, não é bundler, não é wrapper em volta do V8 — é Cranelift
gerando código de máquina direto a partir do AST do SWC, com um runtime mínimo
em Rust e ABI tipado sem boxing.

Dois caminhos, mesmo codegen:

| Modo | Comando | O que faz |
|------|---------|-----------|
| 🚀 **JIT** | `rts run app.ts` | Compila pra memória executável e roda. Zero disco. |
| 📦 **AOT** | `rts compile -p app.ts out` | Object file → linker → binário standalone (~3 KB). |

---

## ⚡ Performance — RTS vs Bun vs Node

Benchmarks executados no Windows 11 (100 runs, 5 warmups, mediana).

### Monte Carlo π — 10M iterações

<table>
<tr>
<td align="center" width="33%">
<b>Bun</b><br/>
<code>91.8 ms</code><br/>
<sub>baseline</sub>
</td>
<td align="center" width="33%">
<b>Node.js</b><br/>
<code>113.9 ms</code><br/>
<sub>1.24× mais lento que Bun</sub>
</td>
<td align="center" width="33%">
<b>RTS AOT</b> 🦅<br/>
<code>16.9 ms</code><br/>
<sub><b>5.43× mais rápido que Bun</b><br/><b>6.74× mais rápido que Node</b></sub>
</td>
</tr>
</table>

### Monte Carlo π — 10M iterações (8 workers)

<table>
<tr>
<td align="center" width="50%">
<b>Bun Workers</b><br/>
<code>147.6 ms</code>
</td>
<td align="center" width="50%">
<b>RTS multi-thread</b> 🦅<br/>
<code>30.3 ms</code><br/>
<sub><b>4.87× mais rápido que Bun Workers</b></sub>
</td>
</tr>
</table>

### HTTP Server — req/s (carga sustentada)

<table>
<tr>
<td align="center" width="50%">
<b>Bun.serve</b><br/>
<code>~14k req/s</code>
</td>
<td align="center" width="50%">
<b>RTS http_server</b> 🦅<br/>
<code>29k req/s</code><br/>
<sub><b>2.07× mais rápido que Bun.serve</b><br/>78% do actix puro Rust</sub>
</td>
</tr>
</table>

### Resumo

| Bench                          | Bun       | Node      | **RTS AOT** | RTS vs Bun | RTS vs Node |
|--------------------------------|-----------|-----------|-------------|-----------:|------------:|
| Monte Carlo 10M (1 thread)     | 91.8 ms   | 113.9 ms  | **16.9 ms** | **5.43×**  | **6.74×**   |
| Monte Carlo 10M (8 threads)    | 147.6 ms  | —         | **30.3 ms** | **4.87×**  | —           |
| HTTP throughput                | ~14k req/s| —         | **29k req/s** | **2.07×** | —           |

**Por que mais rápido?** RTS compila TS para binário nativo via Cranelift —
sem JIT warmup, sem GC pause, sem dispatch dinâmico nos hot paths. Loops
comuns reescrevem automaticamente para `parallel.*` (rayon) sem o user
mencionar threads (silent parallelism). HandleTable shard-aware (32 shards
lock-free) escala alocação em paralelo.

---

## 🔮 Silent Parallelism — o usuário não pede, o compilador entrega

Você escreve isso:

```ts
const arr = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
let sum = 0;
for (const x of arr) sum = sum + x;
```

E o codegen vê o padrão de acumulador associativo e reescreve, antes de baixar IR:

```ts
sum = parallel.reduce(arr, 0, __par_reduce_0);  // rayon, transparente
```

Cobre `for...of` puro, `arr.map/.forEach/.reduce`, e o padrão clássico
`s = s + EXPR`. 96 funções já marcadas como `pure: true` (math, string, num,
fmt, path, hash, mem) alimentam o reconhecimento. Detalhes em
[`docs/specs/silent-parallelism.md`](docs/specs/silent-parallelism.md).

---

## 🧰 A pilha runtime — `std::*` inteira, em pure Rust

37 namespaces. Sem dependência de OpenSSL, schannel, libuv ou qualquer
runtime externo.

| Família | Namespaces |
|---|---|
| **I/O & FS** | `io` `fs` `path` `process` `env` `os` |
| **Compute** | `math` `num` `bigfloat` `fmt` `hash` `crypto` |
| **Memória** | `gc` `buffer` `mem` `alloc` `ptr` `ffi` |
| **Concorrência** | `thread` `atomic` `sync` `parallel` |
| **Rede** | `net` `tls` `http_server` (actix-web embutido) |
| **Dados** | `collections` `string` `regex` `json` `date` |
| **Async** | `events` (EventEmitter), Promise + Function nativos |
| **UI** | `ui` (FLTK 1.x bindings) |
| **Meta** | `runtime` `test` `trace` `hint` |

🌐 **HTTPS sem dor**: `rustls` + `webpki-roots` (Mozilla CAs embutidos no binário)
🧵 **Threading**: 4 mecanismos coexistindo — `spawn/join`, `spawn_async`,
   `spawn_detached` (pool 8 workers, 5M spawn/s), `scope` auto-join
🔒 **HandleTable shard-aware**: 32 shards lock-free entre si

---

## 🎯 O que a linguagem entende hoje

✅ **Controle de fluxo** — `if/else`, `while`, `do-while`, `for`, `switch`
   (jump table nativa via `br_table` quando todos os cases são literais inteiros)

✅ **Funções** — declaração, expression, arrow, **tail call optimization**
   (`return f(x)` vira `return_call`), ponteiros de função first-class

✅ **Classes** — `constructor`, métodos, `this`, `extends`, `super(...)`,
   `super.method(...)`, static, getters/setters, **dispatch virtual real**,
   **operator overload Rust-style** (`a + b` vira `a.add(b)` em compile-time)

✅ **async / await** — pipeline Promise-centric com tokio compartilhado.
   `Promise.create` faz `spawn_blocking`, settle automático via thread-local
   error slot. Function class completa (`call/apply/bind/toString/new Function`).

✅ **Big decimal** — `bigfloat` em i128 fixed-point, ~30 dígitos. π via Machin
   bate 29 dígitos corretos (f64 entrega 16).

✅ **Containers** — object/array literals via `collections.map_*`/`vec_*`,
   member access, atribuição, aninhamento livre

✅ **try/catch/finally** (fase 1) — slot de erro thread-local; unwind real
   ainda não (#128)

✅ **Outros** — `enum`, destructuring nested+rename, spread em literals,
   regex, default params, exports/imports, JSON, Date, console.*, Map/Set v0,
   Array/String prototypes essenciais

❌ **Não suportado ainda** — generators, decorators, generics completos,
   `satisfies`, call spread `f(...args)`, closures com captura mutável real
   (#195 em fase 1)

---

## 🏗️ Arquitetura

Workspace Cargo com 10 crates em `crates/`. O diretório `src/` continua existindo
como fachada do bin `rts` (re-exporta os crates) — `src/main.rs` chama
`rts_codegen::register_runtime_artifacts` + `rts_cli::cli::dispatch`. Paths
reais estão sob `crates/<crate>/src/`.

```
crates/
├─ rts-ast/          AST interno
├─ rts-parser/       SWC parse → AST
├─ rts-diagnostics/  erros estruturados
├─ rts-abi/          ⚡ contrato único — SPECS, AbiType, Intrinsic, símbolos
├─ rts-hir/          HIR tipado (HirType I8..I128, F32/F64, Bool, Str, Handle, Array, Class, Object)
├─ rts-mir/          MIR SSA (60+ Insts, Terminators, passes fold/fma/cse/dce/narrow/verify/inline)
├─ rts-codegen/      Cranelift codegen (AST autoritativo + mir_codegen MIR→Cranelift)
│  └─ src/codegen/   lower/, emit.rs (AOT), jit.rs (rts run), mir_codegen/
├─ rts-runtime/      builtin "rts" + 40+ namespaces (io, fs, gc, math, ...) + async_rt
├─ rts-linker/       link nativo (system linker + fallback object)
└─ rts-cli/          run · compile · apis · init · repl · eval · ir
```

### Pipeline atual

```
TS → SWC → AST → HIR (rts-hir) → MIR (rts-mir) → inline (fixed-point, max 4 iters)
                                              → optimize (fold → fma → cse → dce)
                                              → mir_codegen → Cranelift → JIT/AOT
                                              ↘ AST autoritativo (fallback)
```

MIR está **ativo por default** (commits `f7b924b`, `23dd4b7`). Routing
híbrido: cada user fn tenta o caminho HIR→MIR→Cranelift; se bate em
construct ainda não modelado (member em `this`/objetos, classes,
async/await, address-taken fns, string em params/ret), cai
automaticamente no codegen AST sem perder semântica. Fase 4 (baixo
nível + extensões) em progresso, **5/8 entregues**: atomics (4.1),
inline + integração + fixed-point (4.2/4.3/4.7), CSE (4.5), FMA
(4.8), arr[i]=v + smoke e2e (4.4/4.6). Métricas atuais:

- 438 user fns reais da suite TS rodam pelo MIR
- `cargo test --release --workspace`: **100/100** verde
- `target/release/rts.exe test`: **1015/1015** (cobertura completa, zero falhas)

Variável `RTS_USE_MIR` controla o routing:

| Valor | Comportamento |
|---|---|
| unset / `1` / `on` / `all` | MIR ON (default) |
| `0` / `off` / `none` | AST only |
| `fn1,fn2,...` | MIR só pras fns listadas |

**ABI sem boxing**: cada função de namespace é um símbolo
`#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`. Nada de `JsValue`,
nada de dispatcher central. `i64`/`f64` em bits nativos, strings como
`(ptr, len)` UTF-8, handles `u64` opacos para recursos.

---

## 🚀 Comece em 30 segundos

```bash
# Instalar
git clone https://github.com/UrubuCode/rts && cd rts
cargo build --release

# Rodar
./target/release/rts run examples/console.ts

# Compilar pra binário (~3 KB, sem runtime DLL)
./target/release/rts compile -p examples/console.ts hello
./hello
```

### CLI

```bash
rts run file.ts                  # JIT in-memory
rts compile -p file.ts out       # AOT com slicing por uso
rts apis                         # listar APIs registradas em abi::SPECS
rts ir file.ts                   # dump do IR Cranelift (pra debug de codegen)
rts init my-app                  # scaffolding de projeto
```

---

## 🔬 Debug do codegen

Quer ver exatamente o que o Cranelift está gerando?

```bash
rts ir file.ts 2>&1 | head -50
```

Imprime o IR de cada user fn + `__RTS_MAIN` sem executar. Bom pra caçar
loads/stores redundantes em hot loops, calls extern desnecessários, e
oportunidades de intrinsic. Ver `CLAUDE.md` § Debug do codegen.

---

## 🎯 Compatibilidade JS/TS

Suite TS atual: **1015/1015 testes passando**. Cobertura ampla:

- **Sintaxe core**: classes (extends/super/static/getters/setters), generics,
  destructuring (array/objeto/nested/defaults/rest/params), spread, optional
  chaining (3+ níveis), nullish coalescing, IIFE, function expressions, arrow
  functions, template literals
- **Modules**: named/default/star imports + re-exports + alias
  (`import { x as y }`, `export * as ns`)
- **Async**: Promise, async/await, fetch global, async generators básicos
- **Tipagem**: `i8..i64/u8..u64/f32/f64/bool/string/handle`, narrow types,
  intersection types, union types simples
- **JS globals**: Object/Array/String/Number/Math/Date/RegExp/Error family
  (TypeError/RangeError/etc), Map/Set, WeakMap/WeakSet, Symbol, JSON,
  Function (call/apply/bind/.length/.name), Reflect (13 métodos), Proxy
  (todas as 13 traps: get/set/has/delete/ownKeys/apply/construct/
  getPrototypeOf/setPrototypeOf/defineProperty/getOwnPropertyDescriptor/
  isExtensible/preventExtensions)
- **Operadores**: divisão JS spec (`/` sempre f64), comparações, ternário,
  bitwise, shifts
- **Diagnóstico**: identificadores não-resolvidos viram erro de compilação
  (não segfault)

- **Timers**: `setTimeout`, `setInterval`, `setImmediate` + `clear*`
  (via `thread::spawn` + AtomicBool de cancelamento)
- **GC**: scanner conservativo com transitive marking em Map/Vec/Function/Proxy
  + globals top-level registrados como roots — servidores de longa duração
  com estado em memória são suportados

Cobertura node:* parcial: `node:fs` (readFileSync/writeFileSync/exists/
append/mkdir), `node:path/os/process/util`, `node:crypto` (sha256
streaming via `createHash`/`update`/`digest`, `randomUUID`, hex/base64).
Compatibilidade completa com Node está em #226 (epic tracking).

---

## 📚 Documentação

- 📘 [`BLOG_POST.md`](BLOG_POST.md) — visão geral pra dev externo
- 🛠️ [`CLAUDE.md`](CLAUDE.md) — arquitetura interna + regras do codebase
- 📖 [`docs/specs/`](docs/specs/) — specs técnicas de features
- 🗺️ [`RTS_REFACTOR.md`](RTS_REFACTOR.md) — plano canônico do refator em workspace de crates
- 🐛 Issues: tracker mestre de paridade JS/TS em [#226](https://github.com/UrubuCode/rts/issues/226)

---

## 🛡️ Guardrails

- ✋ Sem `xtask` — build é `cargo` puro
- ✋ Sem download de runtime support em build time
- ✋ Sem dependência de Rust/Cargo no ambiente final do binário AOT
- ✋ Single binary distribuído, roda em qualquer Windows/Linux/macOS sem instalar nada

---

<div align="center">

**Feito com 🦅 por [UrubuCode](https://github.com/UrubuCode)**

*Se Bun é foguete, RTS é ave de rapina.*

</div>
