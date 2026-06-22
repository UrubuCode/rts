# GC do motor novo — fase weak agora, geracional copying (nursery) depois

> **Status:** DESIGN / DECISÃO. A fase weak é o próximo passo bounded; o
> geracional copying é o salto de longo prazo, projeto dedicado **DEFERIDO até
> ~90% cross-runtime funcionando** (CLAUDE.md / `rts-codegen-new-design.md` §5.7).
> Este documento registra a direção decidida e o porquê — o RTS tem uma vantagem
> arquitetural rara que define o melhor design.

## A vantagem arquitetural do RTS: handle indirection

`PolyValue` guarda um **índice de slot da `HandleTable`**, não um ponteiro cru
(design doc §5.4, Pilar 1). O payload de 48 bits de um `TAG_STR`/`OBJECT`/
`FUNCTION` é `slot+shard`, resolvido para o endereço real pela tabela.

Isso muda o cálculo de um GC móvel. Num GC copying/compacting normal o custo
mortal é **achar e reescrever TODO ponteiro** que aponta para o objeto movido
(pointer-patching). No RTS o objeto move no backing store e você atualiza **só o
slot→endereço na tabela** — o índice dentro de cada `PolyValue` não muda. Mover
objeto fica quase de graça. O calcanhar de Aquiles de todo GC móvel já está
neutralizado pela indireção que já existe.

## Estado atual (mark+sweep preciso)

`crates/rts-std/src/collector/` (motor novo) / o `collector.rs` documentado em
`.claude/rules/02-runtime.md`. Mark+sweep com `UserStackMap` do Cranelift +
scanner conservador (`SuspendThread`+`GetThreadContext`) para threads
registradas. `GC_TICK_INTERVAL` allocs → `finish_cycle()` = `mark_stack_roots()`
+ `sweep_all_shards()`. Stack scanner reconhece palavras NaN-boxed `PolyValue`
(`(w & BOX_BASE)==BOX_BASE` e `tag(w) ∈ {STR,OBJECT,FUNCTION}` → root = slot 48
bits; int/float/singleton inline NÃO são roots).

## Passo 1 (próximo, pequeno) — fase weak no mark+sweep atual

`#217` (WeakMap/WeakSet reais + WeakRef + FinalizationRegistry) NÃO precisa
reescrever a GC. Uma fase nova entre mark e sweep resolve, bounded:

1. **Mark normal** — mas NÃO marca através das CHAVES de um `WeakMap`/elementos
   de `WeakSet` (a chave fica "candidata a morrer"); o valor só sobrevive se a
   chave sobreviver por outra referência forte.
2. **Fase weak** (pós-mark, pré-sweep): para cada `WeakMap`/`WeakSet`/`WeakRef`,
   se o target não foi marcado → remove a entrada / `deref`→`undefined`. Para
   cada `FinalizationRegistry` cujo target morreu → enfileira o callback de
   finalização (drenado pelo event loop).
3. **Sweep normal.**

A handle indirection ajuda de novo: um `WeakRef` guarda `(slot, generation)`. Se
o slot foi liberado/reusado (a generation de 16 bits do slab bumpou), `deref`
retorna `undefined` — detecção **O(1) sem scan**. (A generation não cabe no
payload de 48 bits do `PolyValue`; só WeakRef/FinalizationRegistry precisam do
handle completo de 64 bits — design doc §5.5.)

Hoje `WeakMap`/`WeakSet` são `.ts` com semântica STRONG-ref (interino). A fase
weak os torna REAIS sem trocar a arquitetura.

## Passo 2 (longo prazo, grande) — geracional copying (nursery)

A hipótese geracional é fortíssima em JS: a esmagadora maioria dos objetos morre
jovem (temporários de loop, `{}`/`[]` intermediários). O design recomendado:

- **Young gen (nursery):** bump-allocate (alloc = incrementa um ponteiro,
  rapidíssimo). Cheia → **minor GC**: copia só os sobreviventes para o old gen.
  Escaneia só o young + o remembered set. A maioria dos temporários morre aqui →
  nunca promovido → coleta baratíssima.
- **Old gen:** mark-sweep / mark-compact para os sobreviventes, roda raro
  (**major GC**).
- **Mover = trivial** (handle indirection) → sem fragmentação, sem
  pointer-patching: atualiza só o slot→endereço.
- **Write barrier:** registra referência old→young (remembered set) para o minor
  GC não varrer o old gen inteiro. Custo pequeno num property-write de objeto
  velho.
- **Multi-thread:** nursery por-thread (TLAB) → alloc lock-free. A `HandleTable`
  shard-aware já casa com isso.
- **Roots precisas:** os stack maps do Cranelift já existem.

Por que é o melhor para o RTS em máquina nativa: alloc rápido (bump), minor GC
barato (o caso comum), major GC raro, compacta (sem fragmentação), e o custo de
mover — o problema de todo GC móvel — já sai de graça pela indireção. É o que
V8/JSC fazem; no RTS a parte cara é grátis.

## Caminho prático

| Passo | Esforço | Ganho |
|-------|---------|-------|
| Fase weak no mark+sweep atual | pequeno | `#217` real (WeakMap/WeakSet/WeakRef/FinalizationRegistry) sem reescrever a GC |
| Geracional copying (nursery)  | grande  | throughput + latência (pausas curtas), sem fragmentação — o salto de longo prazo |

**Ordem:** fase weak quando `#217` entrar na pauta (fecha o weak honestamente,
destrava WeakMap/WeakSet reais); o geracional como projeto dedicado **depois de
~90% cross-runtime** — é o upgrade certo, e o RTS está unicamente posicionado
para ele.

## Por que NÃO antes

Trocar a GC enquanto o motor novo ainda preenche semântica de linguagem só
adiciona uma variável instável no caminho crítico. O mark+sweep atual é correto
e suficiente até lá; a fase weak é aditiva (não troca a arquitetura). O
geracional só compensa quando o motor já roda a maioria dos programas reais e o
gargalo passa a ser throughput/pausa de GC, não cobertura de feature.

## Plano executável faseado (A1 → A2)

> Cada passo abaixo é **compila + suíte-verde + reversível** isolado, com a
> guarda do piso de honestidade (NADA que crashe/trave commitado como "pass";
> build sempre compila). O collector vivo (`rts-engine/src/collector/collector.rs`
> mark+sweep + `rts-std/src/collector/`) só é tocado de forma ADITIVA até A2.
> Estado de partida (2026-06-22): correção ~51% (323/626); legacy gc.* já drenado.

### A1 — fase weak (#217), bounded, aditiva

A1 NÃO reescreve a GC: adiciona uma fase entre mark e sweep + move a storage de
WeakMap/WeakSet do `.ts` strong-ref para a `Entry` nativa que o collector entende.

- **A1.0 (infra, verde):** garantir `Entry::WeakMap(HashMap<u64,i64>)` /
  `WeakSet(HashSet<u64>)` / `WeakRef(u64 handle 64-bit)` /
  `FinalizationRegistry{callback,entries}` existem (já existem no enum) e que o
  scanner do collector NÃO marca através do conteúdo dessas variantes (hoje, se
  não alocadas como roots, já não marca — confirmar com teste unit do collector).
- **A1.1 (WeakRef deref O(1), o mais bounded):** `WeakRef` guarda o handle
  COMPLETO de 64 bits `(gen<<48 | slot)`. `deref()` chama
  `POLY_TO_HANDLE(slot)` e compara a generation reconstruída com a guardada: se
  difere (slot liberado/reusado) → `undefined`. Sem fase de collector nova; é
  detecção de staleness pura. Entregável isolado, testável com unit
  (alloc → collect → deref vira undefined). Wire mínimo no TS (`WeakRef`/`deref`).
- **A1.2 (storage nativa de WeakMap/WeakSet):** novos externs ABI
  `__RTS_FN_NS_GC_WEAKMAP_*`/`WEAKSET_*` (new/set/get/has/delete) sobre
  `Entry::WeakMap`/`WeakSet`; reescrever `rts-shared/src/stdlib/weakmap_set.ts`
  para delegar a eles em vez de arrays PolyValue strong-ref. AINDA strong até A1.3
  (a storage existe, mas o collector ainda não tem a fase weak) — comportamento
  inalterado, suíte verde.
- **A1.3 (fase weak no collector):** entre `mark_stack_roots()` e
  `sweep_all_shards()` em `finish_cycle()`, varrer cada `Entry::WeakMap`/`WeakSet`:
  remover entradas cujo KEY-handle não foi marcado; cada `FinalizationRegistry`
  com target morto → enfileira callback no event loop. SÓ aqui o comportamento
  vira REAL weak. Teste: weakmap perde entrada quando a única ref forte à chave
  some + collect roda.
- **A1.4 (FinalizationRegistry drain):** o event loop drena a fila de callbacks
  enfileirada por A1.3. Teste: callback dispara após coleta.

### A2 — geracional copying (nursery), DEFERIDO até ~90% cross-runtime

> NÃO iniciar antes de ~90% por design (seção "Por que NÃO antes"). Ordem só
> quando liberado. Cada passo atrás de flag `RTS_GC_GENERATIONAL` (default OFF) —
> o mark+sweep atual continua sendo o caminho de produção até o flag virar.

- **A2.0:** flag + dual-path no `finish_cycle` (OFF = mark+sweep atual, intacto).
- **A2.1:** nursery bump-alloc por-thread (TLAB) atrás do flag; alloc novo cai no
  nursery quando ON.
- **A2.2:** write barrier em property-write de objeto velho → remembered set.
- **A2.3:** minor GC = copia sobreviventes do nursery pro old gen, escaneia só
  nursery + remembered set; mover = atualizar slot→endereço na HandleTable (a
  indireção torna isso ≈ grátis, sem pointer-patching).
- **A2.4:** old gen mark-compact (major GC), roda raro.
- **A2.5:** A/B contra o mark+sweep (correção idêntica) + bench de pausa/throughput
  antes de flip do default.

**Recomendação:** executar A1 agora (sancionado, bounded) na ordem A1.1 → A1.4;
manter A2 atrás do flag e só ligar pós-~90% cross-runtime, com A2.5 como gate.
