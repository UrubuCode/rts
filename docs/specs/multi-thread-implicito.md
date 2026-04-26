# Multi-thread implícito

## Promessa

> Escreva código com semântica single-thread. Receba paralelismo real entre cores.

RTS detecta padrões de uso paralelo no AST e injeta primitivas atômicas
automaticamente. Race conditions silenciosas viram impossíveis por construção
nos casos suportados. O dev escreve código JS-like; o compilador faz o trabalho.

## Status atual

| Tipo | Suportado | Estratégia |
|---|---|---|
| `let x: number` mutável | ✅ Fase 2 | `atomic.i64` |
| `let x: boolean` mutável | ✅ Fase 3 | `atomic.bool` |
| `this.field` em método | ✅ Fase 1 (#227) | userdata trampolim |
| Loops apertados de incremento | ✅ Fase 3 (otim) | thread-local accumulation |
| `let s: string` | ❌ | Fase futura — refcount + handle swap |
| `let arr: number[]` | ❌ | Fase 4 (#229) — `Mutex<Vec>` |
| `Map<K,V>` mutável | ❌ | Fase 4 — `Mutex<HashMap>` ou CHM |
| Class instances mutáveis | ❌ | Fase futura — `@SharedClass` opt-in |

## Como funciona

### 1. Análise (compile time)

Para cada função / método, o lifter coleta:
- **Locais declaradas** (`let`/`const` no body) e parâmetros
- **Capturas em arrows passadas pra `thread.spawn`**

A interseção é o conjunto de capturas atômicas candidatas.

### 2. Detecção de tipo

```rust
// Heurística de detecção:
//  1. Anotação explícita: `let x: boolean` → AtomicKind::Bool
//  2. Init literal: `let x = true` → Bool
//  3. Default: AtomicKind::I64
```

### 3. Reescrita (compile time)

```ts
// O dev escreve:
let counter = 0;
let stop = false;

thread.spawn(() => {
    while (!stop) {
        counter = counter + 1;
    }
}, 0);
```

Compilador transforma internamente em:

```ts
// Após auto-promote:
const counter = atomic.i64_new(0);
const stop = atomic.bool_new(false);

thread.spawn(() => {
    while (!atomic.bool_load(stop)) {
        atomic.i64_fetch_add(counter, 1);
    }
}, 0);
```

### 4. Otimização: thread-local accumulation

Em trampolins de `thread.spawn`, loops apertados de `fetch_add(x, lit)`
sofrem contention severa: cada iteração paga LOCK XADD (~1ns) + cache
line ping-pong entre cores. Solução: detectar o padrão e acumular
localmente.

```ts
// Após auto-promote (sem otim):
while (k < N) {
    atomic.i64_fetch_add(counter, 1);   // contention!
    k++;
}

// Após otim (apenas dentro do trampolim):
let __ta_counter = 0;
while (k < N) {
    __ta_counter = __ta_counter + 1;     // op normal, sem cache thrash
    k++;
}
atomic.i64_fetch_add(counter, __ta_counter);  // 1× só
```

**Aplicabilidade:** loop só com `fetch_add(var, literal)` — sem leituras
de `var`, sem outras escritas, sem outras atomic ops. Conservador por
segurança; detecta o padrão de counter agregado, que é o caso 90%.

**Speedup medido**: 8 threads × 50M iter
- Sem otim: 31484ms (12 M ops/s) — limitado por contention
- Com otim: 46ms (8.6 G ops/s) — limitado pela CPU mesmo
- **~684×**

## Reescritas suportadas

### Para `number` (i64)

| Padrão original | Reescrita |
|---|---|
| `let x = N` | `const x = atomic.i64_new(N)` |
| `x` (leitura) | `atomic.i64_load(x)` |
| `x = e` | `atomic.i64_store(x, e)` |
| `x = x + N` | `atomic.i64_fetch_add(x, N)` (otim self-add) |
| `x = x - N` | `atomic.i64_fetch_add(x, -N)` (otim self-sub) |
| `x += N` | `atomic.i64_fetch_add(x, N)` |
| `x -= N` | `atomic.i64_fetch_add(x, -N)` |
| `x++` | `atomic.i64_fetch_add(x, 1)` |
| `x--` | `atomic.i64_fetch_add(x, -1)` |

### Para `boolean`

| Padrão | Reescrita |
|---|---|
| `let x = true/false` | `const x = atomic.bool_new(...)` |
| `x` (leitura) | `atomic.bool_load(x)` |
| `x = e` | `atomic.bool_store(x, e)` |
| `!x` | `!atomic.bool_load(x)` (via load) |

## Limitações conhecidas

### Operações não suportadas (no MVP)
- `x *= 2`, `x /= 2`, `x %= 5` em locais atomic — codegen falha com erro
- `x = x * 2` (não-self-add) — vira `atomic.i64_store(x, atomic.i64_load(x) * 2)`,
  mas isso **não é atômico** (load + multiply + store racy). Warning futuro.

### Falsos positivos da otim de accumulation
A otim **não** aplica quando:
- Há `atomic.i64_load(x)` no loop (precisa ler valor atualizado em tempo real)
- Há outra atomic op em `x` (CAS, swap)
- Múltiplas variáveis atomic no mesmo loop
- Delta não é literal numérico (ex: `fetch_add(x, k)` onde `k` é variável)

Conservador. Pode-se relaxar futuramente.

### Não captura locais arbitrárias
A captura via global atômico funciona, mas é frágil para múltiplas chamadas
da fn (estado compartilhado entre invocações). Solução real: env-record
real (#195).

## Não-objetivo

- **Auto-locking de estruturas complexas no MVP** — Map, array, class
  instances exigem `Mutex` real, com risco de deadlock por nested locks
  e ordenação. Fase 4 (#229) endereça com warning estático.
- **Detecção 100% de race** — análise estática de aliasing é indecidível;
  casos não-decidíveis viram erro de compilação, não silent fallback.
- **Substituir `atomic`/`sync.*` API explícita** — quem quer controle
  fino, escreve manual. Auto-promote é açúcar pra caso comum.

## Issues e roadmap

- ✅ #206 — `thread.spawn` estável em qualquer contexto
- ✅ #227 fase 1 — captura de `this` em método via userdata
- ✅ #229 fase 2 — auto-atomic em `number` mutável
- ✅ #229 fase 3 — auto-atomic em `bool` + thread-local accumulation
- 🔲 #229 fase 4 — `array<T>` / `Map<K,V>` com `Mutex` auto
- 🔲 #195 — env-record real (closures arbitrárias)
- 🔲 #228 — `parallel async function` + Promise
- 🔲 #230 — Express-compatible HTTP server multi-thread

## Inspirações

- **Pony lang** — capabilities + atomicas implícitas
- **Rust `Send`/`Sync`** — análise de borrow + ownership
- **Project Loom** (Java) — virtual threads
- **Erlang/Elixir** — atores isolados (mais radical)

RTS pega o pragmatismo de Rust + ergonomia de Erlang dentro do envelope JS.
