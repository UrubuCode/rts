# RTS Threading Model — multithread na engine + heap regional (v0, proposta)

> **Status: PROPOSTA** (2026-07-05). Companheiro de
> `rts-std-surface.md` (§rts:thread) e do design canônico
> `rts-codegen-new-design.md`. Não existe doc anterior sobre threading de
> engine — a única documentação era a tabela de mecanismos em
> `crates/rts-runtime/src/namespaces/thread/abi.rs` (agora superfície
> `rts:thread`). Este doc registra o modelo-alvo e por que o value model
> atual o comporta.

## Tese

O RTS terá multithread NA ENGINE (não só threads de runtime chamando fns
soltas): objetos JS cruzando threads com segurança, coleta regional sem
stop-the-world global, e paralelismo em áreas específicas do motor. O
modelo-alvo é **regiões por thread + heap compartilhado com promoção**
(meio-termo Java-G1 / Erlang), porque o value model PolyValue foi
construído com as propriedades que isso exige.

## Por que o PolyValue comporta isso (as 3 propriedades)

1. **Payload = índice de slot, nunca ponteiro.** O word NaN-box (STR/
   OBJECT/FUNCTION) carrega um slot da HandleTable (48 bits). Mover o
   OBJETO entre regiões/heaps não invalida nenhum word vivo — só o slot é
   atualizado (a indireção que o doc do GC geracional já anotava como
   "mover ≈ grátis"). TLABs, regiões, compaction e promoção viram updates
   de slot, sem read barriers no código gerado.
2. **Shards já são proto-regiões.** A HandleTable tem 32 shards lock-free
   com afinidade round-robin por thread (`alloc_entry`). Evoluir para
   "região da thread" = afinidade determinística alloc→shard(s) da thread
   + coleta local desse shard. `shard_for_handle` já decodifica O(1).
3. **Word de 64 bits = load/store atômico.** Um PolyValue compartilhado
   nunca sofre tearing; o tag-check é válido cross-thread.

## O modelo (alvo)

```
┌───────────── Thread A ─────────────┐   ┌───────────── Thread B ────────────┐
│ região A (shards afinados)         │   │ região B                          │
│  - alocação bump/slab local        │   │                                   │
│  - GC LOCAL: só pausa A            │   │                                   │
└─────────────┬──────────────────────┘   └───────────────┬───────────────────┘
              │ publicação (escrita em global/channel/    │
              │ SharedArrayBuffer/objeto shared)          │
              ▼                                           ▼
        ┌──────────────────── heap COMPARTILHADO ────────────────────┐
        │ objetos promovidos; coleta global rara (paridade atual)    │
        └─────────────────────────────────────────────────────────────┘
```

- **Nascimento local**: todo objeto nasce na região da thread criadora.
  Coleta local barata (scanner já suspende por-thread via SuspendThread +
  stack maps; limitar o sweep aos shards da região).
- **Promoção na publicação**: a PRIMEIRA vez que um valor da região
  escapa para outra thread (escrita em gcell/global compartilhado, envio
  por channel, captura por `thread.spawn`, retorno de worker), o subgrafo
  é PROMOVIDO ao heap compartilhado. Detectável barato: toda escrita já
  passa pelos caminhos de slot (`obj_set`/`VEC_SET`/gcell) — é um check de
  "região do destino ≠ região do valor".
- **Promoção = mover slots** (propriedade 1): re-alojar as entries nos
  shards compartilhados e atualizar os slots; os words vivos não mudam de
  significado (o handle é estável se a promoção reusar o mesmo slot-id em
  shard compartilhado — decisão de encoding: reservar bits de shard OU
  tabela de forwarding por slot).

## Pré-requisitos (bloqueadores mapeados, cada um vira issue)

| # | Bloqueador | Estado hoje | Correção |
|---|---|---|---|
| 1 | **GCELLS thread-local** | globais de módulo são por-thread (hack do setInterval; memória `project_test_100_grind`) | promover gcells a células compartilhadas (heap shared) com escrita sincronizada; é o que torna "escrita em global" um ponto de promoção em vez de um bug |
| 2 | **Data ICs (`PropIcCell`)** | célula mutável sem atomicidade | estados mono→poly→mega em `AtomicU64` (shape+slot empacotados num word) ou ICs por-thread |
| 3 | **String pool / interning** | pool global com lock | interning por-região + merge na promoção; strings imutáveis facilitam |
| 4 | **Shape registry** | `Mutex` global (ok p/ leitura rara) | leitura via `RwLock`/snapshot lock-free; ids são append-only |
| 5 | **Event loop / microtasks** | fila single-thread (drain no main) | definir: cada thread com região tem SEU microtask queue; timers globais roteiam pra thread dona do callback |
| 6 | **Codegen/JIT state** | `reset_codegen_state` global, 1 programa por processo | ok para runtime multi-thread; JIT continua single-compile |
| 7 | **Scanner GC NaN-box** | conservador por thread, já reconhece words (design §5.4) | por-região: marcar só roots da thread + remembered set das referências shared→local (write barrier na promoção evita shared→local: promover fecha o subgrafo) |

Invariante-chave escolhida: **nunca existe referência shared→local**. A
promoção fecha transitivamente o subgrafo publicado. Elimina remembered
sets entre regiões; o custo é promoção eager do subgrafo (aceitável: quem
publica um objeto raramente publica metade dele).

## Paralelismo em áreas específicas do motor (independente das regiões)

Alvos de curto prazo que NÃO dependem do modelo regional:
- `parallelMap/Reduce` (rayon) — já existe; superfície em `rts:thread`.
- Parse/HIR de módulos independentes em paralelo no build (compile-time).
- AOT: emissão de objetos por módulo em paralelo (ObjectModule por
  módulo já é o desenho do slicing).
- GC: sweep de shards em paralelo (shards são independentes por design).

## Superfície de usuário (resumo; detalhe em rts-std-surface.md §rts:thread)

- `spawn(fn, arg)` — thread real; captura promove o subgrafo capturado.
- `channel<T>()` — mpsc; `send(v)` promove `v`.
- `Mutex`/`RwLock`/atomics — células compartilhadas explícitas.
- `SharedArrayBuffer` + `Atomics` — memória crua compartilhada (já
  primordial).
- Workers estilo web (futuro): thread + região + módulo isolado +
  postMessage (= channel com structuredClone-ou-promoção).

## Fases

- **T0** (agora): doc aprovado; issues dos bloqueadores 1–5.
- **T1**: gcells compartilhados (#1) — também conserta a classe de bugs
  do setInterval/thread atual.
- **T2**: ICs atômicas (#2) + audit de estado global do runtime.
- **T3**: afinidade thread→shard determinística + sweep paralelo.
- **T4**: promoção na publicação (write-barrier de região) + GC local.
- **T5**: workers/channels na superfície.

Dependência cruzada: o GC geracional (nursery copying,
`gc-generational-design.md`, deferido até ~90% cross-runtime) COMPÕE com
isto — nursery é o caso "região = thread única". Implementar T4 antes ou
junto do geracional; nunca dois modelos de movimentação distintos.
