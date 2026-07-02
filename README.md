<div align="center">

<img src=".github/imgs/logo.png" alt="RTS — TypeScript que voa" width="220" />

# `rts_`

### **TypeScript compilado pra binário nativo. Sem runtime. Sem GC pesado. Sem desculpa.**

*Um urubu de óculos escuros não tem pressa — ele já chegou.*

[![Cranelift](https://img.shields.io/badge/backend-Cranelift-orange?style=flat-square)](https://cranelift.dev)
[![Rust](https://img.shields.io/badge/runtime-Rust-black?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![Single Binary](https://img.shields.io/badge/output-single%20binary-blue?style=flat-square)](#)
<!-- CROSS_RUNTIME_BADGE_START -->
[![Bun/Node parity](https://img.shields.io/badge/Bun%2FNode%20parity-45.1%25-orange?style=flat-square)](docs/specs/cross-runtime-testing.md)
<!-- CROSS_RUNTIME_BADGE_END -->

</div>

<!-- CROSS_RUNTIME_STATS_START -->
## 🌐 Cross-runtime parity

Compatibilidade JS spec validada contra **Bun** e **Node** em 609 fixtures TS standalone.

```
[▰▰▰▰▰▰▰▰▰▱▱▱▱▱▱▱▱▱▱▱] 45.1%   267/592 fixtures passam
```

| Métrica | Valor |
|---|---|
| **Paridade** | **45.1%** (267/592) |
| ✅ RTS = Bun = Node | 267 |
| ❌ RTS diverge | 137 |
| 💥 RTS runtime error | 188 |
| 🛠️  **Falta corrigir** | **325** |
| ⚠️ Bun ≠ Node (skip) | 17 |
| 🚫 Rejeitados (RTS-only) | 0 |
| 📦 Total fixtures | 609 |

_Atualizado: 2026-07-02 — [como adicionar fixture](docs/specs/cross-runtime-testing.md)_

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

> ⚠️ **Motor antigo (congelado).** Os 3 passes de reescrita silenciosa vivem no
> `rts-codegen-old` e NÃO são carregados no motor novo sem rejustificação. Descrito
> aqui como capacidade histórica; o motor novo prioriza o piso de solidez primeiro.

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
| **Meta** | `runtime` `test` `trace` `hint` |

🌐 **HTTPS sem dor**: `rustls` + `webpki-roots` (Mozilla CAs embutidos no binário)
🧵 **Threading**: 4 mecanismos coexistindo — `spawn/join`, `spawn_async`,
   `spawn_detached` (pool 8 workers, 5M spawn/s), `scope` auto-join
🔒 **HandleTable shard-aware**: 32 shards lock-free entre si

---

## 🕸️ Motor de render HTML/CSS nativo

<div align="center">
<img src=".github/imgs/urubu-mascote.png" alt="O UrubuCode — o mascote, desenhado só com caixas CSS e animado pelo motor de render do RTS" width="560" />

*O mascote acima é uma página HTML — nenhuma imagem: só caixas CSS + `@keyframes`, renderizado e animado pelo motor.*
</div>

O RTS tem um **motor de render HTML+CSS próprio, em Rust puro** (`crates/rts-dom`),
seguindo o pipeline canônico de browser: `DOM → cascade CSS → layout (x,y,w,h) →
display list → paint`. O backend (egui/wgpu) só **pinta** — o DOM é dono de tudo,
headless e testável sem janela.

O que já renderiza fiel (validado **número a número contra o Chrome real**,
`getBoundingClientRect` elemento a elemento — o cover e o grid `.row`/`.col` do
Bootstrap saem **pixel-perfect**):

- **Cascade completa** (especificidade, `!important`, herança, `@media` por viewport)
  com **custom properties por elemento** (`var()` com override por componente)
- **Flexbox**: `grow`/`shrink`/`basis`, `align-self`, `order`, stretch real,
  column com `margin:auto` absorvendo, gap, wrap
- **Unidades**: `rem`/`em`/`%`/`vw`/`vh` e **`calc()`** (tipografia fluida),
  margens negativas, `box-sizing`
- **Fluxo**: inline rico (links/negrito fluindo no parágrafo), `float`,
  `position: absolute/fixed` v1, scroll containers com barras próprias
- **Animações CSS**: `@keyframes` + `transition` com easing completo —
  cores, tamanhos e margens interpolando (o urubu acima bate asa com isso)
- **Recursos externos**: `<link rel="stylesheet">` + `@import` locais

Teste qualquer página local com uma linha:

```bash
rts run examples/view.ts examples/urubu.html                            # o mascote
rts run examples/view.ts examples/bootstrap-5.3.8-examples/cover/index.html  # Bootstrap real
rts run examples/view.ts caminho/para/seu/index.html                    # o seu site
```

Estado e backlog técnico completo: [issue #1793](https://github.com/UrubuCode/rts/issues/1793).

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

> **Redesign em andamento (strangler-fig).** O motor de codegen está sendo
> reescrito do zero atrás do antigo, congelado. O motor **novo** ativo é
> `crates/rts-codegen-new/` (caminho único HIR→Cranelift, sem MIR; valor
> `PolyValue` NaN-boxed; shapes + inline caches de dado; dispatch data-driven). O
> antigo `crates/rts-codegen-old/` (dual HIR→MIR / AST, valor `i64` sobrecarregado)
> está **congelado** e some no cutover. Plano canônico:
> [`docs/specs/rts-codegen-new-design.md`](docs/specs/rts-codegen-new-design.md).

Workspace Cargo em `crates/`. O `src/` é a fachada do bin `rts` (re-exporta os
crates); paths reais sob `crates/<crate>/src/`.

```
crates/
├─ rts-ast/          AST interno
├─ rts-parser/       SWC parse → AST
├─ rts-diagnostics/  erros estruturados
├─ rts-engine/       ⚡ heap GC + contrato ABI (SPECS, AbiType, Intrinsic, símbolos) + Registry
├─ rts-hir/          HIR tipado (I8..I128/F32/F64/Bool/Str/Handle/Array/Function/Class/Object/Any)
├─ rts-mir/          MIR SSA — usado SÓ pelo rts-codegen-old (congelado); some no cutover
├─ rts-codegen-old/  motor CONGELADO (dual MIR/AST, switchboard, add_fn! manual)
├─ rts-codegen-new/  motor ATIVO — value.rs (PolyValue), repr.rs, shape.rs, ic.rs,
│                    dispatch.rs (data-driven), abi_gen.rs (ABI gerada de SPECS), lower/ (single path)
├─ rts-primitives/   classes PRIMORDIAIS (String/Object/Array/Function/Promise/Boolean/Number/Error)
├─ rts-shared/       não-primordial universal (math/num/collections(Map/Set)/json/globals + stdlib/*.ts)
├─ rts-std/          backend (io/net/tokio/console/promise/audio)
├─ rts-runtime/      fachada fina (pub use dos quatro acima) + staticlib AOT
├─ rts-node/         shims node:* (fs, os, path, process, crypto, util)
├─ rts-napi/         N-API (.node addons) via libloading + HandleTable
├─ rts-linker/       link nativo (system linker + fallback object)
└─ rts-cli/          run · compile · apis · init · repl · eval · ir
```

### Pipeline (motor novo — caminho único, sem MIR)

```
TS → SWC → AST → HIR (rts-hir) → lower/ (HIR → Cranelift IR, UM caminho) → egraph Cranelift → JIT/AOT
```

Não há tier MIR nem dual AST/MIR no motor novo. O **egraph do Cranelift**
(`use_egraphs=true`) é o ÚNICO otimizador (const-fold, CSE, DCE, FMA, strength
reduction, inline intraprocedural). O front-end só faz o que o Cranelift não pode
(semântica JS): coerções `ToNumber/ToString/ToBoolean`, o `+` polimórfico,
inserção de box/unbox (IR pura que o egraph dobra), emissão de sites de shape/IC,
wrap de int estreito, arestas de exceção. AOT/JIT compartilham `compile_program`
(`FnCtx.module` é `&mut dyn Module`).

**Doutrina PRIMORDIAL-vs-Registry (central ao motor novo):** o motor NOMEIA só as
classes primordiais; todo o resto resolve via Registry data-driven — **nada de
nome não-primordial hardcoded no front, nem em "allow-list"** (ver
[`CLAUDE.md`](CLAUDE.md) § anti-hardcode). Os globais não-primordiais (console,
Map/Set, JSON, Date) vivem como `.ts` de prelude (`rts-shared/stdlib/*.ts`) e
chamam pontes privadas `engine.*`; o front não os nomeia.

**ABI sem boxing**: cada função de namespace é um símbolo
`#[no_mangle] extern "C" fn __RTS_FN_NS_<NS>_<NAME>(...)`. Nada de `JsValue`,
nada de dispatcher central. `i64`/`f64` em bits nativos, strings como
`(ptr, len)` UTF-8, handles `u64` opacos para recursos. No motor novo a tabela de
símbolos JIT é DERIVADA de `SPECS` (`abi_gen.rs`), não `add_fn!` manual.

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

> **Número honesto:** a paridade cross-runtime real do motor **novo** é a do bloco
> [🌐 Cross-runtime parity](#-cross-runtime-parity) no topo (gerado pelo CI contra
> Bun+Node). O motor antigo chegou a 100% (372/372) na tag `v0.0-202606072107` — um
> máximo local de uma abordagem hardcoded sobre um modelo de valor insólido; o
> redesign existe para furar essa parede, não para repetir o número. NÃO cite
> "1015/1015"/"100%" como estado atual.

O que o motor **novo** já cobre (em construção, paridade subindo):

- **Sintaxe core**: classes (extends/super/static/getters/setters),
  destructuring, spread em literais, optional chaining, nullish coalescing,
  arrow/function expressions, template literals
- **Async**: Promise + async/await (caminho síncrono sem `await`; event loop real
  ainda aberto, #207)
- **JS globals como `.ts` de prelude (data-driven)**: Object + statics,
  Boolean/Number/String prototypes, Error family, console.*, Map/Set, JSON, Date —
  nenhum nomeado no front
- **Operadores**: divisão JS spec (`/` SEMPRE f64 — `44100/48000 === 0.91875`,
  inclusive atribuído a `const`), comparações, ternário, bitwise, shifts
- **try/catch/finally** fase 1 (slot de erro thread-local; finally roda e
  re-propaga o erro corretamente)
- **Diagnóstico**: identificador não-resolvido vira erro de compilação, nunca
  segfault — e nunca um valor errado (o piso de solidez do redesign)

Itens pesados ainda abertos (alguns em fase de redesign): event loop async real
(#207), closures com captura mutável (#195), TCO, Proxy (#218), typed
arrays/DataView/ArrayBuffer, Symbol/Reflect/BigInt (#216/#219). Tracker mestre de
paridade JS/TS: [#226](https://github.com/UrubuCode/rts/issues/226).

---

## 📚 Documentação

- 🛠️ [`CLAUDE.md`](CLAUDE.md) — arquitetura interna + regras do codebase (inclui § anti-hardcode)
- 📖 [`docs/specs/`](docs/specs/) — specs técnicas de features
- 🗺️ [`docs/specs/rts-codegen-new-design.md`](docs/specs/rts-codegen-new-design.md) — plano canônico do redesign do motor
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
