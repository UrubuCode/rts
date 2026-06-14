# Atomicidade de RMW sob async paralelo (atomic-rmw-intrinsic)

## Problema

`async function f` é reescrita para `promise.create(__async_inner_f, args)`, que
faz `rt().spawn_blocking(invoke + settle)` — o corpo da async fn roda numa
**worker thread tokio**. Disparar N async fns antes de `await` = N threads
paralelas tocando o heap compartilhado.

`shared[0] = shared[0] + 1` compilava para **duas chamadas extern separadas**:
- `__RTS_FN_NS_COLLECTIONS_VEC_GET(h, 0)` — trava o shard, lê, **destrava**.
- `__RTS_FN_NS_COLLECTIONS_VEC_SET(h, 0, novo)` — trava o shard, escreve, destrava.

Entre o GET (destrava) e o SET (trava) o lock do shard está **solto**. Outro
thread lê o valor velho → incremento perdido. Read-modify-write não-atômico.

**Medido:** 4 async fns incrementando `shared[0]` 1M vezes cada. Esperado
4000000 (determinístico, como no Node — que serializa). RTS dava ~2M
não-determinístico (race). Node sempre 4000000 porque o event loop single-thread
serializa os corpos.

## Solução: colapsar GET+OP+SET num único extern atômico

O codegen reconhece o padrão `arr[i] OP= expr` e `arr[i] = arr[i] OP expr`
(índice trivial) e emite **uma** chamada
`__RTS_FN_NS_COLLECTIONS_VEC_RMW(h, index, op, operand)` que faz read+op+write
dentro de **uma só closure `with_vec_mut`** — um único lock do shard, sem janela.
Idêntico em segurança ao `VEC_PUSH`/`VEC_POP` que já são atômicos.

### Por que NÃO um lock segurado entre duas calls

As abordagens alternativas (reentrant object lock, shard-lock amplification)
segurariam o `MutexGuard` do shard através de duas chamadas. Isso cria um
**deadlock permanente** com o GC: o scanner (`gc/collector/scan.rs:120-159`) faz
`SuspendThread(worker)` e **depois** `mark_handle` → `shard_for_handle().lock()`.
Se o GC suspende uma worker que segura o guard, e então tenta travar o mesmo
shard, trava para sempre (a worker suspensa nunca solta). O atomic-rmw-intrinsic
não toca essa superfície — o lock é tomado e solto dentro do mesmo
`with_entry_mut`, sem janela.

### Por que o paralelismo é preservado

O `spawn_blocking` **continua existindo**. 4 tarefas isoladas (cada uma com seu
próprio array) seguem rodando em 4 threads reais — `par.ts` mantém tempo ≈ 1
tarefa. Só o RMW do slot compartilhado vira atômico. A abordagem "serializar
async" (rodar tudo no event loop) eliminaria o race mas **mataria o paralelismo**
de tarefas isoladas — rejeitada por isso.

## Implementação

- `rts-shared/src/collections/vec.rs`: `apply_rmw(op, cur, operand)` (op-codes
  0-10 inteiros com `as i32`/`& 31` = semântica JS ToInt32; 16-19 float via
  from_bits/to_bits) + `__RTS_FN_NS_COLLECTIONS_VEC_RMW`.
- `rts-shared/src/collections/map.rs`: `__RTS_FN_NS_COLLECTIONS_MAP_RMW_KH`
  (RMW por key-handle, reusa `apply_rmw`).
- `rts-codegen/.../jit.rs`: `add_fn!` dos dois (AOT vem do staticlib).
- `rts-codegen/.../lower/expressions/mod.rs`: helpers (`rmw_op_code`,
  `rmw_same_index`, `rmw_same_array`, `rmw_expr_reads_slot`) + ramo de
  reconhecimento no bloco `MemberProp::Computed`. **Puramente aditivo com
  fall-through**: qualquer condição que não casa → comportamento atual.

## Cobertura (honesto sobre os limites)

**Cobre** (RMW atômico): `arr[i] += x`, `arr[i] -= x`, `*=`, `/=`, `%=`, `&=`,
`|=`, `^=`, `<<=`, `>>=`, `>>>=` e a forma explícita `arr[i] = arr[i] OP expr`,
com índice literal-num ou ident e operando que não relê o slot. Inteiro e float.
Map por key string-literal.

**NÃO cobre** (cai no fall-through, segue como antes — sem fingir resolver):
- `arr[i] = f(arr[i])` com `f` user-fn — segurar o lock através de uma user-fn
  arbitrária seria o deadlock que esta abordagem evita por design. Fora de
  escopo intencionalmente.
- `m[a] += m[b]` (dois slots) — só o slot de destino é travado.
- `arr[idx()] += 1` (índice com efeito colateral).

Esses casos mantêm a semântica atual (read-modify-write em 2 calls). O fix cobre
o padrão comum de contador/acumulador compartilhado, que é o caso medido.

## Verificação

- `tests/claude-async-rmw-race.test.ts` — 4 async fns paralelas incrementando um
  array compartilhado (forma explícita E compound) → total exato determinístico.
- `/tmp/asynctest/race.ts` (manual): 4000000 nas 5 execuções (era ~2M).
- `/tmp/asynctest/par.ts`: 4 tarefas isoladas ≈ tempo de 1 (paralelismo
  preservado).
- Suite TS 1710/1710, sem regressão (ramo aditivo com fall-through).
- Sem deadlock/hang (lock não segurado fora de `with_entry_mut`).
