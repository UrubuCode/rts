# Silent Parallelism (Level-1)

Conjunto de passes que reescrevem padrões TS comuns para chamadas
`parallel.*` automaticamente — user não precisa mencionar threads,
workers, ou qualquer coisa relacionada a concorrência.

## Pipeline dos passes

Em ordem (cada um vê o AST já transformado pelos anteriores):

### 1. `array_methods_pass`

Detecta `arr.METHOD(fn[, init])` e reescreve para `parallel.METHOD(arr, ...)`:

| User escreve | Compilador gera |
|---|---|
| `arr.map(fn)` | `parallel.map(arr, fn)` |
| `arr.forEach(fn)` | `parallel.for_each(arr, fn)` |
| `arr.reduce(fn, init)` | `parallel.reduce(arr, init, fn)` |

Requisito: `fn` deve ser `Ident` apontando pra user fn top-level.

### 2. `reduce_pass`

Detecta o padrão clássico de acumulador:

```ts
let s = INIT;
for (const x of arr) {
  s = s + EXPR;       // ou: s += EXPR
}
```

Reescreve para `let s = parallel.reduce(arr, INIT, __par_reduce_N);` com
`__par_reduce_N(acc, x)` retornando `acc + EXPR`. Aceita ops associativas
(`+`, `*`). `EXPR` precisa ser puro.

### 3. `purity_pass`

Detecta `for...of` cujo body é puro (só chama membros `pure: true` de
namespaces, sem assignments, sem control flow escapes) e reescreve para
`parallel.for_each(arr, __par_forof_N)`.

## Cobertura de escopo

Os 3 passes operam em:
- **Top-level** (`program.items`)
- **Body de cada user fn** (`Item::Function.body`)

Counters compartilhados garantem nomes sintéticos sem colisão.

## O que conta como "puro"

Membro de namespace marcado `pure: true` em `abi.rs`. Hoje 96 fns:
math (30), string (16), num (21), fmt (10), path (8), mem (7), hash (4).

Critério: determinístico nos args + sem side effect observável fora do
escopo paralelo. Allocadoras de handle são impuras pelo critério atual.

## Fontes de array suportadas

`parallel.*` aceita: array literal inline, variável local, retorno de fn,
e outro `parallel.map` result. Não aceita `Buffer`, typed arrays, slices
de string (follow-up).

## Limitações conhecidas

| Limitação | Workaround |
|---|---|
| `for(let i=0;i<n;i++)` não detectado | Use `for...of [...]` |
| `while` não detectado | — |
| Reduces f64 caem serial | Use `parallel.reduce` direto |
| INIT não-literal em reduce | Hoist literal pra const |
| Body com >1 stmt em reduce | Combinar em uma expressão |
| `for...of` em método de classe | Extrair pra fn top-level |
| Spawn dentro de arrow lifted | Spawn top-level |

## Infra de suporte

- **HandleTable shard-aware**: 32 shards lock-free entre si. `alloc_entry`
  distribui round-robin por thread; `shard_for_handle` decodifica O(1)
- **Callconv SystemV/Win64** pras user fns address-taken (não Tail).
  `call_indirect` casa essa callconv via `ctx.module.isa().default_call_conv()`
- **Param `__rts_spawn_arg_f64`** detectado em `compile_user_fn` faz bind
  com bitcast i64→f64 — preserva bit-pattern de `thread.spawn(fp, 3.14)`

## Referências

- Issues: epic #247, pedaços #248 (A) #249 (F) #250 (bug A) #251 (D)
  #252 (B) #253 (C)
- Tests: `tests/parallel_*.test.ts`
- Implementação: `src/codegen/lower/func.rs` (`array_methods_pass`,
  `reduce_pass`, `purity_pass`)
