# Storage nativo de array (stack-slot inline) — RTS_ARRAY_INLINE

> Plano de implementação (PR-1, fatia segura). Decidido por painel de design.
> Objetivo: eliminar o overhead de call extern + lock em `arr[i]` para arrays
> locais não-escapantes de tamanho fixo, fechando a maior parte do gap de 6× vs
> V8 medido em `bench/multiuser_sim.ts`.

## Diagnóstico (verificado no IR real)

O gap de array access (RTS 6× mais lento que Node no `multiuser_sim`) é
~100% **overhead de chamada extern** — não o lock. Cada `arr[i]` (read/RMW) é
uma `call __RTS_FN_NS_COLLECTIONS_VEC_*` com decode de handle + lock do shard +
bounds. O PR #1556 já fundiu o RMW em 1 lock; o que resta é a call. A única
forma de matar a call é **não chamar** = inline em memória.

## A fatia segura: stack slot

Arrays **locais, tamanho estático conhecido, sem push, não-escapantes** vão para
um `StackSlot` Cranelift de `len*8` bytes. `arr[i]` vira `load/store
[stack_addr + i*8]` puro. Zero call, zero lock, zero handle.

**Por que é safe-by-construction (os 4 constraints caem):**
| Constraint | Por que cai |
|---|---|
| realloc | tamanho fixo, sem push → nunca realoca |
| race async | array não-escapante → nunca compartilhado entre threads |
| deadlock GC | nenhum lock de shard; nem está no HandleTable |
| UAF de handle no array | o scanner de GC é **conservador** e varre toda a stack (`collector/scan.rs:84-110`) → handles num stack slot são marcados automaticamente, igual a qualquer local |

O caminho `Box<[i64]>` heap foi **descartado**: `trace_children`
(handles.rs:957) não alcança buffer fora do HandleTable → UAF de handles
internos. Só o stack slot é seguro sem um registry escaneável novo.

## Plano de implementação

1. **Escape analysis intraprocedural** — novo
   `crates/rts-codegen/src/codegen/lower/passes/escape.rs`.
   `non_escaping_local_arrays(body) -> HashSet<String>`, **default-DENY**: o
   array escapa se aparecer em: arg de call, return, assign a target não-local,
   captura por arrow/fn (incl. async), ou método que vaza
   (`.map/.reduce/.forEach/.slice/.concat/.push/...`). Conservador: na dúvida,
   escapa (fica no caminho atual).

2. **Qualificação para stack slot** — `non_escaping` ∧ sem
   `push/pop/splice/unshift` ∧ tamanho estático conhecido (`new Array(N)` com N
   literal, ou `[lit, lit, ...]`). O padrão `for i<N { arr[i]=v }` de
   pré-dimensionamento é follow-up (mais análise).

3. **Estado no FnCtx** (`ctx.rs`, junto de `local_array_vars`):
   `native_arrays: HashMap<String, NativeArrayStorage>` com
   `{ slot: StackSlot, len: i64, elem_clty: cl::Type, elem_ty: ValTy }`.

4. **Alocação** (`statements/decls.rs`, onde hoje faz `VEC_NEW`): se qualifica,
   `create_sized_stack_slot(ExplicitSlot, len*8, 3)` + registra em
   `native_arrays`, em vez de `VEC_NEW`.

5. **Read inline** (`members.rs:~1827`, antes de `INDEX_GET_AUTO`): se `m.obj` é
   Ident ∈ `native_arrays`, emite `stack_addr` + `imul_imm(idx,8)` + `iadd` +
   `load(elem_clty, MemFlags::trusted())` com bounds-check
   (`idx<len && idx>=0 ? load : default`). Devolve `TypedVal` com `elem_ty`
   exato (sem ambiguidade).

6. **Write inline** (`mod.rs:~1456`, ramo `arr[i]=v`): Ident ∈ `native_arrays`
   → `store` direto.

7. **RMW inline** (`mod.rs:~1063`, colapso VEC_RMW): Ident ∈ `native_arrays` →
   `load; op; store` em registrador. **Este é o ponto que fecha o gap** (mata os
   VEC_RMW/iteração).

8. **Bounds-check**: `idx<len && idx>=0 ? acesso : default` (JS-correto). Elision
   via range analysis é follow-up.

## Gate

Tudo atrás de `RTS_ARRAY_INLINE=1` (env var). Default OFF (caminho atual,
bit-idêntico). Quando provado em produção, vira default.

## Resultados medidos (v1, 2026-06-13)

Micro-bench `arr[k%16] += 1` em loop de 50M (100% array access, pior caso):

| | Tempo | vs OFF |
|---|---|---|
| RTS OFF (caminho atual) | 21548 ms | 1× |
| **RTS ON (inline)** | **199 ms** | **108× mais rápido** |
| Node 22 | 112 ms | — |

O RTS sai de 192× mais lento que o Node para **1.8× mais lento** neste workload.
IR confirma: com ON, o loop tem 0 calls de coleção (só `stack_addr`/`load`/
`store`); com OFF, tem `VEC_GET`/`VEC_SET`/`VEC_RMW`.

**Cobertura da v1:** só arrays LOCAIS a uma fn, tamanho literal (`new Array(N)`/
`[...]`), não-escapantes, sem push. O `bench/multiuser_sim.ts` tem ganho mínimo
(5085→4898ms) porque seus arrays são **top-level** — coberto no follow-up.

**Segurança validada:** o race-test do #1556 com flag ON dá 4000000
determinístico (arrays async-compartilhados escapam → caminho atom-RMW, não
inline). Suite 1720/1720 com flag ON e OFF.

## Verificação obrigatória

- `target/release/rts.exe test` 1712/1712 (com flag ON e OFF).
- Monte Carlo não regride.
- **Stress do race-test do #1556 sob carga async** — garantir que nenhum array
  async-compartilhado caiu no caminho nativo (a escape analysis DEVE forçá-los
  ao caminho atual).
- Micro-bench: `arr[i]+=1` em loop (array local fixo) com flag ON vs OFF —
  medir o ganho real.
- `bench/multiuser_sim.ts` — medir (o ganho aqui depende do top-level +
  loop-init, que é follow-up; o micro-bench valida o mecanismo).

## Follow-up (épico)

- Top-level arrays (`__RTS_MAIN`) + tamanho-via-loop-init (`for i<N{arr[i]=v}`).
- `Box<[i64]>` heap para tamanho dinâmico (precisa registry escaneável pelo GC).
- Arrays com push (cap variável).
- Escape analysis interprocedural (call-graph).
- Bounds-check elision (range analysis).
