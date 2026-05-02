<div align="center">

<img src="assets/logo.png" alt="RTS — TypeScript que voa" width="220" />

# `rts_`

### **TypeScript compilado pra binário nativo. Sem runtime. Sem GC pesado. Sem desculpa.**

*Um urubu de óculos escuros não tem pressa — ele já chegou.*

[![Cranelift](https://img.shields.io/badge/backend-Cranelift-orange?style=flat-square)](https://cranelift.dev)
[![Rust](https://img.shields.io/badge/runtime-Rust-black?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Single Binary](https://img.shields.io/badge/output-single%20binary-blue?style=flat-square)](#)

</div>

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

## ⚡ Performance — números reais, não slides de marketing

> Windows 11, 100 runs com 5 warmups, mediana.

```
┌─────────────────────────┬───────────┬───────────┬─────────┬─────────┐
│         Bench           │  RTS AOT  │  RTS JIT  │   Bun   │  Node   │
├─────────────────────────┼───────────┼───────────┼─────────┼─────────┤
│ Monte Carlo  10M iter   │   16.9ms  │   26.8ms  │  91.8ms │ 113.9ms │
│ Monte Carlo (8 workers) │     —     │   30.3ms  │ 147.6ms │    —    │
│ rts_simple (LCG+primes) │   15.4ms  │   21.3ms  │  70.9ms │  98.1ms │
│ Machin π — 30 dígitos   │   48.0ms  │   47.0ms  │ 109.0ms │ 108.0ms │
└─────────────────────────┴───────────┴───────────┴─────────┴─────────┘
```

**RTS AOT vs Bun**: 5.14× em Monte Carlo · 4.45× em rts_simple
**RTS multithread vs Bun Workers**: 4.66×
**HTTP server**: pico **29k req/s** (78% do actix puro Rust em Rust, 2× do `Bun.serve`)

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

```
src/
├─ parser/         SWC parse → AST interno
├─ type_system/    type checker, registry, resolver
├─ codegen/        Cranelift codegen direto sobre o AST (sem HIR/MIR)
│  ├─ lower/       lowering de expr/stmt/func
│  ├─ emit.rs      ObjectModule (AOT)
│  └─ jit.rs       JITModule (rts run)
├─ abi/            ⚡ contrato único — SPECS, AbiType, Intrinsic, símbolos
├─ namespaces/     37 namespaces builtin (impl operacional)
├─ runtime/        bootstrap, async_rt (tokio compartilhado), eval
├─ module/         resolver + grafo de dependências
├─ linker/         link nativo (system linker + fallback object)
├─ pipeline.rs     orquestra build/run/JIT
└─ cli/            run · compile · apis · init · repl · eval · ir
```

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

## 📚 Documentação

- 📘 [`BLOG_POST.md`](BLOG_POST.md) — visão geral pra dev externo
- 🛠️ [`CLAUDE.md`](CLAUDE.md) — arquitetura interna + regras do codebase
- 📖 [`docs/specs/`](docs/specs/) — specs técnicas de features
- 🗺️ [`ROAD_MAP.md`](ROAD_MAP.md) — direção estratégica
- ✅ [`NEXT_STEPS.md`](NEXT_STEPS.md) — próximos passos concretos
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
