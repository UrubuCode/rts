# A repartição de largura entre colunas — a forma do algoritmo do Blink

A spec é genuinamente vaga aqui (CSS 2.1 §17.5.2.2 descreve intenções, não um
procedimento), e toda a gente implementa o que o Blink implementa. Esta página
guarda **a forma** do algoritmo, lida da source, porque nenhuma das nossas
réguas de pixels lhe podia chegar.

**O sintoma que a motivou:** 339 células divergentes com **sinal misto** — 207
largas demais contra 132 estreitas demais, **aos pares na mesma linha**. Uma
coluna a mais e a vizinha a menos, somando quase zero. Um sinal assim quase
nunca é um valor errado: é um **critério de repartição** diferente.

## Onde vive, e um aviso sobre nomes

`core/layout/table/table_layout_utils.cc`, em
`DistributeInlineSizeToComputedInlineSizeAuto` — 2 083 linhas, o algoritmo
inteiro num só ficheiro.

**`table_layout_algorithm_auto.cc` e `table_layout_algorithm_fixed.cc` NÃO
existem nesta árvore**: o LayoutNG fundiu os dois. Uma procura por esses nomes
devolve zero, e zero ali não significa que o cálculo não exista — significa que
o nome envelheceu. Fica escrito porque a busca falhada é indistinguível de uma
ausência quando não se sabe disto.

## A escada de quatro palpites

O Blink **não interpola**: escolhe um regime e só uma classe de coluna cresce
nele.

**1. Classificar.** Cada coluna carrega `percent`, `is_constrained`,
`is_collapsed`, `is_table_fixed`, `is_mergeable` e `percent_border_padding`,
além do par min/max. **A classe decide tudo o que vem a seguir.**

**2. Acumular quatro somas hipotéticas**, cada uma uma pergunta "e se":

| palpite | percent | declaradas | auto |
|---|---|---|---|
| `kMinGuess` | mínimo | mínimo | mínimo |
| `kPercentageGuess` | a sua % | mínimo | mínimo |
| `kSpecifiedGuess` | a sua % | **máximo** | mínimo |
| `kMaxGuess` | a sua % | máximo | **máximo** |

**3. Escolher o PRIMEIRO palpite cuja soma já chega ao alvo.**

**4. Só a classe que cresce NESSE degrau recebe o excedente**, proporcionalmente
ao aumento que ela própria contribuiu para o degrau — `increase da coluna /
increase total do degrau`. **As outras classes ficam congeladas** no valor que
tinham no degrau anterior.

**5. O resto do arredondamento vai INTEIRO para a última coluna que cresceu.**

Acima do máximo há três ramos: com colunas auto, só elas crescem, e
**proporcionalmente ao seu máximo, não à folga**; sem auto, as declaradas só
crescem se o alvo for a largura da tabela e não uma célula com `colspan`; e só
com percent, cada uma recebe uma fatia proporcional à sua própria percentagem.

## O que nós fazemos, e porque produz exatamente aquele sinal

**Três regimes por SOMA** (abaixo do mínimo / entre mínimo e máximo / acima do
máximo) e **uma interpolação linear sobre a folga de todas as colunas ao mesmo
tempo.** O Blink tem cinco regimes **por CLASSE**.

A consequência é o sinal misto, e vê-se degrau a degrau:

- no `kSpecifiedGuess`, o Blink deixa as colunas de texto **presas no mínimo** e
  nós damos-lhes a fatia proporcional da folga → **as auto saem largas demais e
  as declaradas estreitas demais, no mesmo par**;
- no `kMaxGuess` é o simétrico: as declaradas param no máximo e nós continuamos
  a dar-lhes folga, roubando-a às auto;
- uma **coluna `mergeable`** — sem largura, sem percentagem e que nenhuma célula
  tocou — é **saltada por inteiro** e fica a zero no Chrome; nós damos-lhe uma
  fatia da sobra. Num par, isso é literalmente uma larga demais e outra estreita
  demais.

**Não temos colunas percent de todo**, portanto o segundo degrau não existe e
uma `<td width="30%">` reparte como se fosse texto livre.

## Dois detalhes que parecem cosméticos e não são

**O caso exato.** Quando o alvo bate exatamente na soma dos máximos, o Blink
**não usa a matemática de distribuição**: atribui literalmente o máximo a cada
coluna. O comentário na source diz porquê — a matemática introduz erro de
arredondamento e **causa quebra de linha não pretendida**. É o caso normal de
uma tabela sem `width`, o mais comum da página; e meio pixel a mais ali faz uma
célula quebrar linha e muda a **altura** da linha inteira.

**O défice fecha numa coluna só.** O resto vai inteiro para a última que cresceu,
garantindo que a soma iguala o alvo ao 1/64 de pixel. Sem isso, a soma das
nossas colunas não fecha com a largura da tabela e o erro **espalha-se por
todas** em vez de ficar numa.

Ele calcula em `LayoutUnit::MulDiv` — inteiro de 64 bits em 1/64 px — e não em
vírgula flutuante, precisamente para o défice fechar. Nós trabalhamos em `f32`;
está registado como ruído provável abaixo do pixel, **para não ser confundido
com uma causa real**.

## O que isto quer dizer para nós

O trabalho não é afinar a nossa interpolação: é **dar classe às colunas**. Sem
classe não há como tratar uma coluna com largura declarada de forma diferente de
uma de texto livre, e essa é a raiz de quase todos os candidatos. Está tudo em
`scripts/parity/calculos/tabela.jsonl` — 66 registos, 43 em falta.

Como sempre: isto é a lista de candidatos. **Quanto vale, e se entra, diz a
régua de geometria** — medida por par, nunca por soma, que é o que um sinal
misto exige.
