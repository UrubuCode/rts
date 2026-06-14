# Benchmark: simulação multi-usuário (multiuser_sim.ts)

Simula 5000 usuários × 2000 eventos = **10M eventos**. Cada evento muta estado
agregado (saldos/ações) em arrays paralelos. PRNG determinístico (Park-Miller
LCG, portável — não depende de wrap int32). Mesma corretude nos 3 runtimes
(`sum_balances=28927970` idêntico).

## Resultados (2026-06-13)

| Runtime | Tempo  | Eventos/seg | vs Node |
|---------|--------|-------------|---------|
| Node 22 | 546 ms | 18.3M       | 1.0×    |
| Bun 1.3 | 622 ms | 16.1M       | 0.88×   |
| **RTS** | 3182 ms| 3.1M        | **0.17× (6× mais lento)** |

## Por que o RTS perde AQUI (e ganha no Monte Carlo)

- **Monte Carlo (RTS ganha ~5×):** workload de CPU puro, variáveis f64 em
  registradores Cranelift, zero acesso a heap no loop quente.
- **multiuser_sim (RTS perde ~6×):** workload **array-heavy**. Cada
  `balances[u]`, `actions[u]`, `balances[target]` é uma **chamada extern**
  (`VEC_GET`/`VEC_SET`) com lock de shard. O loop interno tem ~muitas dessas por
  evento. V8/JSC têm arrays inline (acesso direto à memória, sem call, sem lock).

**Conclusão honesta:** o RTS é mais rápido em CPU-bound aritmético, mas mais
lento em código dominado por acesso a array/objeto, por causa do overhead de
chamada extern + lock por acesso. Esse é o gargalo a atacar para workloads
realistas (gateways de evento, simulações, processamento de coleções).

## Caminhos de otimização identificados

1. **Inline de array access no codegen** — `arr[i]` sobre Vec conhecido poderia
   emitir load/store direto no ptr do Vec em vez de `call VEC_GET`, eliminando o
   overhead de call + lock para o caso single-thread comum. Maior ganho.
2. **Escape analysis** (Phase 4 pendente) — arrays locais que não escapam podem
   virar memória nativa sem HandleTable.
3. O `apply_rmw` (PR #1556) já provou o ganho: colapsar calls reduziu
   `arr[i]+=1` de 6020ms→973ms (6×). Estender o princípio a get/set puro.

## Bug de conformidade descoberto (separado)

Shift bitwise sobre VARIÁVEL não trunca para int32 como o JS exige:
`s << 13` com `s` number dá resultado 64-bit no RTS (`1011358015488`) vs int32
no Node (`2040700928`). Literais const-folded (`1 << 31`) funcionam. Precisa de
ToInt32 antes de bitwise/shift sobre operando dinâmico no codegen. (O
`apply_rmw` do PR #1556 já faz `as i32` corretamente; o caminho normal de `<<`
não.) Tracking: relacionado a #305.

## Como rodar

```bash
target/release/rts.exe run bench/multiuser_sim.ts
node bench/multiuser_sim.ts
bun bench/multiuser_sim.ts
```
