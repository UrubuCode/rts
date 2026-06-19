# Auto-paralelismo por regiões (motor novo) — design, base teórica e limites

> **Status:** DESIGN / RE-JUSTIFICATIVA. Nada implementado no motor novo.
> Este documento existe para registrar a direção decidida e a base bibliográfica
> antes de qualquer código. Pré-requisito de implementação: **pós-P5** do
> `rts-codegen-new-design.md` (motor serial correto, paridade real ≥ tag
> `v0.0-202606072107`). Paralelizar antes de serial-correto mistura bug de race
> com bug de codegen — proibido.

## Por que este documento existe

O paralelismo silencioso do motor velho (`silent-parallelism.md`:
`array_methods_pass` / `reduce_pass` / `purity_pass`) está **congelado** e o
design do motor novo (`rts-codegen-new-design.md`) só permite ressuscitá-lo "se
re-justificado". A re-justificativa exigida é trocar **chute por forma-de-AST**
(`is_map_get_call` e amigos, que o design mata) por **prova de segurança**. Este
spec é essa re-justificativa: um modelo de regiões com base teórica publicada,
onde a decisão de paralelizar é uma prova sobre a HIR, não um padrão sintático.

## Base teórica (literatura)

Quatro frameworks estabelecidos. "Puro-numérico" é apenas o conjunto
*trivialmente* provável; estas técnicas estendem o conjunto seguro muito além.

1. **Escape / points-to analysis.** Classifica cada alocação: escapa para estado
   compartilhado ou não. Compiladores auto-par (Intel, Oracle) fazem dois passes
   — *é seguro?* e *vale a pena?* — e **nunca** paralelizam o que provam inseguro.
2. **Type-and-effect / region systems (Deterministic Parallel Java).** Cada
   objeto pertence a uma **região**; só paraleliza tarefas cujos efeitos tocam
   regiões **disjuntas** (`writes r1 ∥ writes r2` quando `r1 ≠ r2`). Determinismo
   garantido em compile-time.
3. **Commutativity analysis (Rinard & Diniz, MIT, PLDI'96 / TOPLAS'97).** Se
   *todas* as operações sobre um objeto **comutam** (mesmo resultado final
   independente da ordem), o compilador gera código paralelo — **mesmo com estado
   compartilhado e estruturas baseadas em ponteiro** (grafos inclusive). Não
   exige pureza; exige comutatividade. **O `VEC_RMW` do RTS (abaixo) é um caso
   particular disto.**
4. **Thread-Level Speculation / privatization (Privateer).** Executa otimista,
   valida em runtime, faz rollback em conflito (geomean 11.4× em C/C++ geral).
   **Rejeitado para o RTS** — ver §"Por que não é útil".

Referências:
- Rinard & Diniz, *Commutativity Analysis* — PLDI'96
  <https://people.csail.mit.edu/rinard/paper/pldi96.pdf>, TOPLAS'97
  <https://people.csail.mit.edu/rinard/paper/toplas97.pdf>
- *Deterministic Parallel Java* (type-and-effect, regiões)
  <https://www.cs.cornell.edu/courses/cs6120/2020fa/blog/parallel-java/>
- *Permission Regions for Race-Free Parallelism* (Rice, ECOOP'12)
  <https://www.cs.rice.edu/~zoran/Publications_files/ECOOP12.pdf>
- *Compiler-Driven Software Speculation for TLP* (ACM TOPLAS)
  <https://dl.acm.org/doi/10.1145/2821505>
- *Automatic Parallelization* (visão geral)
  <https://en.wikipedia.org/wiki/Automatic_parallelization>

## O modelo de 3 regiões (síntese aplicada ao RTS)

A HIR carregada classifica cada site que produz um handle em uma região. Esse é o
"ponto de ponteiro que pode colidir" — registrado na análise, não chutado.

| Região | O que é | Regra de thread |
|---|---|---|
| **Thread-local** | handle alocado na tarefa, escape analysis prova que não escapa | ✅ muta livre em paralelo |
| **Shared-imutável** | globais, imports base, `Object.freeze`, código de função, constantes | ✅ compartilha **leitura**; fica no thread principal |
| **Shared-mutável** | objeto/array/Map que escapa **e** é escrito por >1 tarefa | ⚠️ **ponto de colisão** — ver decisão fina |

A decisão fina mora na 3ª linha, e **commutativity analysis** resolve o caso
comum:

- escrita = **op comutativa** (`+=`, `*=`, `|=`, `^=`, `Map.set` de keys
  disjuntas) → atômica via intrinsic de **uma só call** (estilo `VEC_RMW`).
  Seguro em paralelo.
- escrita = **não-comutativa**, ou índice/key podem colidir, ou alvo é
  user-fn arbitrária → **não paraleliza esse ponto**. Serial no thread principal.

### O que a HIR/infra nova já fornece (sem inventar análise do zero)

- **Repr lattice** (`repr.rs`) — `Tagged`/`Ref` = pode escapar/alocar;
  `Int32/Float64/Bool` = não toca heap. Primeiro filtro de graça.
- **Shapes** (`shape.rs`) — write a slot de objeto = `shape_id` + offset
  conhecido → granularidade de região **por slot** (estilo DPJ) de graça; sabe
  *qual campo* colide, não só "o objeto colide".
- **Captura de closure** — a HIR já lista vars capturadas (box_captures);
  captura mutável compartilhada ⇒ região shared-mutável.
- **`global_class_lookup` / refs globais** — marca shared-imutável automático.

Análise nova que falta (trabalho real, padrão e bem documentado): **escape
analysis** (handle alocado aqui vaza para parâmetro/retorno/global?) e
**may-alias** entre tarefas.

## O gate (os dois passes da literatura)

```
pode_paralelizar(loop/região) =
  SEGURO:  toda escrita ou é (a) thread-local
                         ou (b) shared-imutável só-leitura
                         ou (c) shared-mutável com op COMUTATIVA via intrinsic 1-call
           E nenhum lock de shard segurado através de uma call (ver trava do GC)
           E sem efeito observável ordenado (io.print, etc) no corpo
  E VALE:  carga estimada > limiar (trip-count estático OU profile do JIT)
           — só dispara thread se trabalho > custo de spawn/join
```

A linha (c) é o salto além de puro-numérico: acumulador compartilhado, reduce
sobre objetos, atualização de grafo com op comutativa. É exatamente Rinard.

## A trava dura do GC (inegociável)

O scanner do coletor faz `SuspendThread(worker)` e **depois** trava o shard
(`gc/collector/scan.rs`). Logo:

> Uma região paralela **nunca** pode segurar o `MutexGuard` de um shard através
> de uma 2ª call. Se o GC suspende uma worker que segura o guard e então tenta
> travar o mesmo shard → **deadlock permanente** (a worker suspensa nunca solta).

Consequência de design: shared-mutável só paraleliza via **intrinsic atômico de
uma-call** (RMW comutativo, lock tomado-e-solto dentro de um único
`with_entry_mut`). Qualquer coisa que precise segurar lock através de user-fn =
**fora**, serial. Isto poda o conjunto seguro, mas é física do coletor atual —
não há como contornar sem reescrever o GC. No motor novo é **pior**: shapes/ICs
mutam (`PropIcCell: uninit → mono → poly → mega`) sem lock; paralelizar um corpo
que dispara um site de IC = race no IC. Por isso shared-mutável paralelo se
restringe a slots de dados via intrinsic atômico, nunca a caminhos que mutam IC.

---

# Async RMW atômico (já existe no motor VELHO — referência)

> Este mecanismo está implementado no motor **velho** (`main`, commit `44329312`,
> PR #1556). Aqui ele serve de **referência e prova viva**: é commutativity
> analysis aplicada a um caso, e é a evidência concreta de que paralelismo
> implícito sobre heap compartilhado morde. Spec original (na main):
> `docs/specs/async-rmw-atomic.md`.

## O que é

`async function f` é reescrita para `promise.create(__async_inner_f, args)`, que
faz `rt().spawn_blocking(invoke + settle)` — o corpo roda numa **worker thread
tokio**. Disparar N async fns antes de `await` = **N threads reais tocando o heap
compartilhado**. Isso já é "multithread em vários lugares" — mas implícito, caído
por cima do async, **sem o motor provar segurança**.

## O bug que apareceu (a fatura do paralelismo implícito)

`shared[0] = shared[0] + 1` compilava para duas chamadas extern separadas:

- `VEC_GET(h, 0)` — trava o shard, lê, **destrava**.
- `VEC_SET(h, 0, novo)` — trava o shard, escreve, destrava.

Entre o GET (destrava) e o SET (trava) o lock fica **solto** → outra thread lê o
valor velho → incremento perdido. Read-modify-write **não-atômico**. Medido: 4
async fns incrementando 1M vezes cada davam **~2M não-determinístico** em vez de
4000000. Node sempre dá 4000000 porque o event loop single-thread serializa.

## O fix (commutatividade de uma-call)

O codegen reconhece `arr[i] OP= expr` e `arr[i] = arr[i] OP expr` (índice
trivial, operando que não relê o slot) e emite **uma** chamada
`VEC_RMW(h, index, op, operand)` que faz read+op+write dentro de **uma só
closure `with_vec_mut`** — um único lock, sem janela. Idem `MAP_RMW_KH` por key.
Ramo puramente aditivo com fall-through: o que não casa o padrão mantém o
comportamento atual.

- **Por que não lock-por-objeto:** segurar o guard através de 2 calls = o
  deadlock do GC descrito acima. O RMW não toca essa superfície.
- **Paralelismo preservado:** `spawn_blocking` continua; 4 tarefas isoladas
  seguem em 4 threads. `par.ts` ≈ tempo de 1 tarefa.
- **Acelera de quebra:** loop `arr[i]+=1` 50M → RMW 973ms vs GET+SET 6020ms (6×),
  porque colapsa 2 locks+2 calls em 1. Monte Carlo 10M intocado (não usa
  `arr[i] OP=`).

## Cobertura honesta

**Cobre:** `arr[i] += x` (e `-= *= /= %= &= |= ^= <<= >>= >>>=`) e a forma
explícita `arr[i] = arr[i] OP expr`, índice literal/ident, operando que não relê
o slot, int e float, Map por key string. **NÃO cobre** (cai no fall-through, sem
fingir resolver): `arr[i] = f(arr[i])` (user-fn sob lock = o deadlock),
`m[a] += m[b]` (dois slots), `arr[idx()] += 1` (índice com efeito).

## A lição para o motor novo

O `VEC_RMW` precisou existir **porque** algo paralelizou estado compartilhado sem
gate de segurança. Se o gate de §"O gate" só liberasse regiões cujas escritas
compartilhadas são comutativas-atômicas (ou inexistentes), **o race nunca
nasce**. O async-rmw é o patch reativo; o modelo de regiões é o preventivo.

---

# Por que NÃO é útil (limites, e o que rejeitar)

Registro honesto do que **não** vale a pena, para ninguém tentar "consertar" pelo
caminho errado.

## 1. Paralelismo implícito por async (o do motor velho) — não é o modelo certo

`spawn_blocking` por async fn dá paralelismo **sem prova**. A segurança vira
responsabilidade do usuário (e o RTS difere do Node, que serializa). O race do
`shared[0]` é o sintoma. Útil como acelerador oportunista, **inútil como base de
um modelo seguro** — não decide nada, só corre e torce.

## 2. "Paraleliza todo lugar de alto carregamento" — falso-atalho

A maioria dos pontos de alto carregamento **toca heap/objeto/string** → caem em
shared-mutável ou produzem `Tagged`. O conjunto realmente seguro é **menor do que
parece**. Espalhar threads por "todo lugar pesado" reintroduz exatamente o race
que o `VEC_RMW` teve que remendar. O ganho real está no subconjunto provado, não
na cobertura ampla.

## 3. Thread-Level Speculation / privatization — rejeitado no RTS

Apesar de potente (Privateer: 11.4× em C/C++ geral), TLS exige **rollback** e
**validação em runtime** de cada acesso especulado. No RTS isso colide de frente
com o coletor: rollback de estado mutável + `SuspendThread`/stack-scan
conservador + HandleTable = complexidade e risco de deadlock altíssimos, com
buffers de versão que o GC teria que entender. Custo de engenharia ≫ ganho sobre
o caminho provado. **Fora de escopo.**

## 4. Op não-comutativa sobre estado compartilhado — não paraleliza, ponto

Ordem importa (push em ordem observável, `arr[i] = f(arr[i])` com efeito,
`m[a] += m[b]` em dois slots). Não há intrinsic atômico de uma-call que preserve
semântica. Tentar = segurar lock através de call = deadlock do GC. **Serial,
sempre.** O gate nega por construção (conservador, nunca silenciosamente errado).

## 5. Antes do P5 — inútil e perigoso

Sobre um motor que ainda não é serial-correto, qualquer divergência paralela é
ambígua: bug de race **ou** bug de codegen? Debug intratável. O valor de
paralelizar é zero enquanto a base serial não fecha paridade. **Pré-requisito
duro.**

## Resumo do veredito

- **Viável e fundamentado** — escape + region + commutatividade cobrem bem além
  de puro-numérico (acumuladores, reduce sobre objetos, grafos comutativos).
- **Custo real** — escape analysis + region inference + may-alias é trabalho
  substancial (semanas).
- **Teto duro** — o GC (`SuspendThread` + lock) limita shared-mutável a
  comutativo-atômico de uma-call; não dá para contornar sem reescrever o coletor.
- **Sequência** — pós-P5, opt-in, caso a caso medido. Não "espalhar por toda
  parte".
