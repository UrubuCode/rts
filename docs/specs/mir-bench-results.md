# MIR vs AST: medições de performance

Documenta o impacto medido do caminho `RTS_USE_MIR` ativo (default) vs
desativado (`RTS_USE_MIR=off`) em benches reais.

## Resultados (2026-05-07)

Cada bench rodado 3x com cada configuração. Medição: wall clock do
processo `target/release/rts.exe run <bench>` em milissegundos.

| Bench | MIR (default) | AST only | Delta |
|---|---|---|---|
| `bench/monte_carlo_pi.ts` | 117/118/120 ms | 118/119/120 ms | ≈ 0% |
| `bench/bun_simple.ts` | 65/67/65 ms | 65/65/66 ms | ≈ 0% |

## Por que zero ganho

MIR atualmente cobre **438 user fns** na suite TS (~3% do total). As
fns hot do programa típico não estão entre elas:

- **monte_carlo_pi.ts**: hot loop está no top-level (`while (i < N) { ... }`).
  Top-level é compilado pelo `compile_main` (puro AST). User fns no
  arquivo (`toFloat`) são chamadas só 2 vezes no total.

- **bun_simple.ts**: 5 user fns. Algumas vão pelo MIR (são whitelisted),
  mas não são chamadas em hot path crítico.

## Onde MIR daria ganho

Programs com **fns helper pequenas chamadas em hot loops** beneficiariam
de:

- **inline pass** (etapa 4.2 + 4.3 + 4.7): elimina overhead de chamada
- **fold + cse + dce** (etapas 4.5, fma 4.8): reduz inst count
- **fma fusion**: hot float math `a*b+c` em uma instrução nativa

Exemplo construído (`/tmp/chain4.ts`):

```ts
function h(x: i64): i64 { return x + 1; }
function g(x: i64): i64 { return h(x) * 2; }
function f(x: i64): i64 { return g(x) + 5; }
function top(x: i64): i64 { return f(x) - 1; }
```

Com MIR ativo, `top(10)` é colapsado em **uma única expressão constante
ou alguns adds** (cadeia inteira inlined). Ganho concreto difícil de
medir em microbench Windows (overhead startup ~50ms domina).

## Interpretação

A Fase 4 entregou **infra de otimização sólida** (atomics, inline com
fixed-point, CSE, FMA, escape analysis future). O ganho prático em
benches existentes é zero porque benches têm hot loop no top-level.

**Para o trabalho dar retorno mensurável** seria necessário:

1. Routar **top-level (`compile_main`)** pelo MIR também — refator no
   `compile_program` que junta o body main + user fns num pipeline
   único. Sub-projeto da Fase 5.

2. Mais cobertura HIR: member access em `this`/objetos, classes,
   async — que liberam ~70% das fns que hoje bail por synthetic/
   placeholders.

3. Benches dedicados com helpers pequenos no hot path — para
   confirmar que o inline integrado funciona em escala.

## Referência

- Métricas brutas: `target/release/rts.exe run bench/monte_carlo_pi.ts`
  com e sem `RTS_USE_MIR=off`.
- Cobertura MIR: 438 fns reais via MIR (medido em
  `RTS_MIR_DEBUG=1 target/release/rts.exe test`).
- Pipeline `optimize()`: fold → fma → cse → dce; mais inline em
  fixed-point.
