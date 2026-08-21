# Modularizar o `rts-dom` — o plano, e a régua que o valida

O `CLAUDE.md` fixa **500 linhas** por ficheiro fora do codegen. Estes passam:

| ficheiro | linhas | quantas vezes o teto |
|---|---|---|
| `layout.rs` | 9 961 | **20×** |
| `dom.rs` | 5 974 | 12× |
| `style/newprops_tests.rs` | 1 826 | 3,7× |
| `inline_box.rs` | 1 299 | 2,6× |
| `style/stylesheet.rs` | 1 101 | 2,2× |
| `style/values.rs` | 1 073 | 2,1× |

## Porquê agora, e a razão não é estética

**Um ficheiro grande serializa trabalho que podia ser paralelo.** Numa sessão com
vários agentes, cada lote que tocou o `layout.rs` obrigou a congelar toda a gente
— "pare de escrever nesse ficheiro até eu confirmar o commit" — porque um `git
add` a meio de uma edição alheia commita meio lote, e um build sobre a árvore
partilhada compila um estado que nunca existiu. Ambos aconteceram no mesmo dia.

E há um efeito medido na qualidade: **o mesmo defeito de forma apareceu em três
sítios do mesmo ficheiro** — `is_block_level`, `cria_caixa_apesar_de_inline` e
`is_inline_block`, todos a perguntar *"não é inline?"* onde a pergunta era *"é de
bloco?"*. Cada um foi encontrado por medição separada, em lotes separados. Com a
decisão num módulo só, teria sido um.

## A régua que valida um refactor de mover código

**Uma modularização pura tem de mover ZERO pixels.** O dump da página tem de
sair **idêntico byte a byte** ao de antes:

```bash
cp scripts/parity/out-img/rts.jsonl /tmp/antes.jsonl
cargo build --release -p rts-host --example run_fixture
target/release/examples/run_fixture.exe examples/claude-parity-rts.ts > scripts/parity/out-img/rts.jsonl
cmp /tmp/antes.jsonl scripts/parity/out-img/rts.jsonl   # TEM de ser silencioso
```

Se diferir num byte, não foi um `move`: foi uma alteração de comportamento
disfarçada de arrumação, e é a forma mais fácil de perder trabalho medido. A
verificação custa ~90 s de build e 10 s de medição, e é mais forte que a suite —
os testes provam que o que eles cobrem continua igual; o dump prova que **16 813
elementos** continuam iguais.

`cargo test -p rts-dom` verde é condição necessária, não suficiente.

## A decomposição proposta para `layout.rs`

77 funções de topo, **3 010 linhas de teste (30,1%)** a partir da 6978. Os nomes
já declaram as fronteiras.

**AVISO: a tabela abaixo tinha dois números errados, e o defeito era do
contador.** Estava escrito `bounding_rect` **828** e `collect_geometry` **314**.
O `bounding_rect` tem **nove linhas** — cinco delas de doc — e o
`collect_geometry` tem **31**. A função de 807 linhas é o `layout_block`, que
não aparecia de todo.

O mecanismo, porque o número errado vale menos que a razão:

```
awk '/^(pub )?fn [a-z_]+/{ ... print prevline, NR-prevline, prev }'
```

O padrão **não apanha `pub(crate) fn`** — e é assim que o `layout_block` está
declarado — nem os métodos dentro de um `impl`, que são indentados. E o tamanho
que ele imprime é **a distância até à declaração seguinte que o padrão apanha**,
não o corpo da função. Um defeito, dois sintomas, os dois na mesma direção:
inflacionar quem vem antes de algo que o contador não vê.

O custo não era cosmético. O plano desenhava um módulo à volta de um problema
que não existe, **enquanto o problema verdadeiro ficava dentro de outro sem
ninguém contar com ele** — e só apareceria a meio do passo mais arriscado.

É a mesma forma dos dois defeitos de régua apanhados no mesmo dia: o instrumento
a reportar com confiança um número que era artefacto dele próprio. **Um contador
de linhas é um instrumento e obedece às mesmas regras que uma régua de pixels.**

| módulo | o que leva | maiores funções |
|---|---|---|
| `layout/mod.rs` | tipos públicos, `layout_document`, `layout_cached`, re-exports | `layout_document` 98 |
| `layout/display.rs` | `DisplayItem`, `DisplayList`, `Corners`, `Rect`, `ScrollRegion` | `collect_geometry` 314 |
| `layout/medida.rs` | `TextMeasurer`, `ApproxMeasurer`, `intrinsic_content_width` | `intrinsic_content_width` 205 |
| `layout/bloco.rs` | fluxo de blocos, empilhamento, reuso | `layout_children_vertical` 370, `layout_block_reusing` 214 |
| `layout/flex.rs` | eixo horizontal e coluna | `layout_children_horizontal` 296, `layout_children_column` 235 |
| `layout/grid.rs` | grid | `layout_children_grid` 259, `resolve_tracks` 91 |
| `layout/linha.rs` | fluxo inline, runs, quebra | `wrap_runs` 412, `layout_inline_flow` 343, `collect_runs` 280 |
| `layout/replaced.rs` | imagem, svg, input, widget | `layout_input` 144, `layout_image` 75 |
| `layout/consulta.rs` | geometria pedida de fora | `bounding_rect` **828** |

Nem todos ficam abaixo de 500 à primeira — `wrap_runs` sozinho tem 412 e
`bounding_rect` 828. Onde não couber, **o que não cabe é dito e datado**, não
arredondado: um ficheiro que fica em 700 com uma linha a explicar porquê é
honesto; um que fica em 700 em silêncio repõe o problema.

## A ordem, e porque é que ela importa

1. **Primeiro os testes.** Tirar `#[cfg(test)]` para `layout/tests/` corta 3 010
   linhas sem tocar numa única linha de lógica — é o movimento com melhor razão
   risco/benefício de todos, e sozinho tira o ficheiro de 9 961 para ~6 950.
2. **Depois as folhas** — `display.rs`, `medida.rs`, `replaced.rs`, `consulta.rs`:
   pouco acopladas, cada uma verificável à parte.
3. **Por fim o núcleo** — `bloco.rs`, `linha.rs`, `flex.rs`, `grid.rs`, que
   partilham estado de passagem e são onde um `move` se pode tornar uma
   alteração sem ninguém dar por ela. **Cada um é um commit próprio com o `cmp`
   a passar.**

Um refactor grande num commit só é indistinguível de um refactor grande com um
bug lá dentro.

## O que NÃO se faz nesta arrumação

Não se corrige nada, não se renomeia nada, não se "melhora de passagem". Cada
uma dessas coisas quebra a única verificação forte que temos — o dump idêntico —
e passa a exigir medição por elemento, que é o custo que a arrumação existe para
evitar. Havendo algo a corrigir, anota-se e faz-se **depois**, num lote medido.

---

## O `dom.rs`, feito — e três correções que o próprio refactor trouxe

5 974 → **3 336 linhas** em dois passos, com **599 testes antes e depois de cada
um** e o `--numstat` no corpo de cada commit a provar que só se moveu.

| passo | resultado | linhas adicionadas ao `dom.rs` |
|---|---|---|
| 1 — testes para `dom/tests/` | 5 974 → 4 215 | **1** (`mod tests;`) |
| 2 — seis folhas | 4 215 → 3 336 | **13** (seis `mod`, dois `pub use`, três `use`) |

**1. Os testes eram 1 761 linhas, 29,5% — não "~3 000".** Este documento dizia
"~3 000" para o `layout.rs` e não media os do `dom.rs`. Um plano com números
redondos é um plano por medir, e a extração real também desmentiu a
decomposição: o módulo de testes da cascata saiu com 616 linhas e foi partido em
três em vez de entregue acima do teto.

**2. A visibilidade custou dez itens, não cinquenta e quatro — e a diferença era
culpa do plano.** Pôr o `struct Dom` num módulo próprio **cria** a necessidade de
abrir os 44 campos aos irmãos. Deixá-lo no `dom/mod.rs` custa zero: dez funções
livres passam a `pub(in crate::dom)`, que é exactamente o alcance que já tinham,
e **nenhum campo muda**.

Uma saiu da lista por medição — `closes_open_p` só é chamada dentro do próprio
ficheiro, e foi o compilador que o disse com um `unused import`. **Alargar "por
precaução" inventa uma fronteira que não existe.**

E os re-exports internos são `use` simples e não `pub(crate) use`: este último
republicava as dez em `crate::dom::*`, e o `layout.rs` passava a vê-las.

**3. Privado em Rust não é privado ao ficheiro.** É visível no módulo que define
**e em todos os descendentes**. Três blocos `impl Dom` em ficheiros próprios
leem campos privados e chamam métodos privados sem uma única anotação. Quem
partir o `layout.rs` precisa de saber isto antes de começar: metade do medo de
um refactor destes vem de assumir o contrário.

### O que não foi tocado, e porquê

Um `use super::*;` duplicado e um teste atrás de `#[cfg(feature = "metrics")]`
— este último explica 98 `#[test]` declarados contra 97 a correr, e vale saber
antes de contar testes num passo futuro. Ambos anotados, nenhum corrigido: uma
arrumação que corrige de passagem deixa de ser verificável, e a inocência de uma
mudança é precisamente o que ninguém consegue confirmar depois.

E a indentação de 4 espaços foi mantida nos ficheiros de teste, de propósito:
vários têm literais multi-linha em que o espaço à esquerda é **conteúdo**.

### O portão passou nos quatro passos: 16 813 elementos idênticos byte a byte

Quatro passos (`b91ae530`, `dc1ee62e`, `f6bbe566`, `13d12217`), **5 974 linhas
num ficheiro → 26 ficheiros, o maior com 488**, e o dump da página construído do
commit final é **byte a byte igual** ao de antes do refactor. Nenhum dos 16 813
elementos mudou um pixel.

O `dom.rs` deixou de existir. `lib.rs` está intacto — nenhum utilizador do crate
mudou uma linha. **33 itens de visibilidade em todo o refactor, todos
`pub(in crate::dom)`: zero `pub`, zero `pub(crate)`, zero campos.** O plano
original previa 54, e 44 desses eram uma necessidade que o próprio plano criava.

Isso é o que uma arrumação tem de conseguir provar, e a suite não o prova: os
604 testes dizem que o que eles cobrem continua igual; o dump diz que **a página
inteira** continua igual. `cargo test` verde continua a ser condição necessária,
não suficiente.

Uma nota sobre como o portão foi corrido, porque a árvore partilhada torna isto
não-óbvio: **os binários foram construídos em worktrees isolados nos commits**,
nunca da árvore de trabalho. Durante os três passos houve, em momentos
diferentes, um `style/parse.rs` com erro de sintaxe a meio da edição de outro
agente e dois testes vermelhos de um terceiro. Um binário feito da árvore nessas
alturas compila um estado que nunca existiu, e a medição que sair dele não é de
ninguém.

### O que este refactor rendeu além das linhas

**Uma fronteira que não existia.** O `matcher.rs` juntou as sete funções que
respondem *"este nó casa com este seletor?"*, e ao pô-las lado a lado apareceu a
regra que nenhuma delas escrevia: **três são aproximações, e podem responder
"sim" a mais, nunca "não" a mais.** Um falso positivo custa uma cascade extra;
um falso negativo perde um resultado do `querySelectorAll` ou deixa um nó com
estilo velho depois de um hover. Cada uma dizia isso à sua maneira; nenhuma dizia
que era a mesma regra nas três.

É a forma do defeito que o `layout.rs` tem **cinco vezes** — *"não é inline?"*
onde a pergunta é *"é de bloco?"* erra para o lado errado, e por isso cada cópia
falha sozinha e é corrigida sozinha, ao preço de um lote e uma medição cada.

### "Testes primeiro" é uma hipótese, não uma regra

Esta ordem veio do `dom.rs`, onde cortava 29,5% do ficheiro. **Nas outras duas
áreas foi medida antes de seguida e não transferiu:**

- no **`style/`** os testes já vivem em ficheiros próprios; as 449 linhas que
  restam dentro da lógica (4,0%) estão em ficheiros que **já cumprem o teto**, e
  extraí-las não tirava um único da lista;
- em **`table/mod.rs` e `block.rs`** não há teste nenhum — são 100% lógica.

E há o caso invertido: o **`inline_box.rs` é 61,4% teste**, e um único passo
deixa a lógica em 521 linhas.

A regra que sobrevive é a de partida: **medir o ficheiro antes de lhe aplicar um
plano feito para outro.**

---

## As provas de uma extração, por ordem de força

Nenhuma destas substitui as outras. Foram todas ganhas por terem falhado
primeiro, e cada uma responde a uma pergunta que as anteriores não respondem.

### 1. As fronteiras somam o total, sem resto

Antes de cortar: cada linha do original cai numa fatia, e a soma das fatias é o
total. **"Sem resto" é o que prova que nada caiu entre dois pedaços** — e num
caso apanhou 14 linhas que se perdiam, que eram os comentários `// ── …` a
dividir o ficheiro em secções. Uma guarda que contasse só funções não teria
visto nada.

### 2. Nenhuma fatia acaba a meio de um item

Um corte entre um `#[test]` e o `fn` que ele anota compila até deixar de
compilar: `expected item after attributes`. O script recusa antes de escrever.

### 3. Reconstruir e comparar — e normalizar o fim de linha

```bash
git show HEAD:caminho/X.rs > /tmp/orig.rs
cat parte1 parte2 parte3 > /tmp/reass.rs        # pela ordem ORIGINAL
cmp /tmp/orig.rs /tmp/reass.rs                   # silencioso = idêntico
```

O `--numstat` prova que só se acrescentaram `mod` e `use`. **Isto prova outra
coisa: que o conteúdo movido é o mesmo conteúdo** — nem um espaço reescrito, nem
uma chave perdida, nem uma linha em branco a mais entre blocos.

**A armadilha, que apanhou à primeira: a árvore está em CRLF e o `git show`
responde LF.** O `cmp` cru acusa DIFERE em *todas* as linhas. Normalize as duas
pontas. Este repositório já teve um falso drift de symbol table pela mesma
razão, e quem usar a técnica num Windows sem isto conclui que perdeu o ficheiro.

**Quando a ordem não é preservada** — uma concatenação alfabética de um ficheiro
que não era alfabético — o `cmp` diz "diferente" sem nada estar errado. Aí a
pergunta certa é o **multiconjunto de linhas** mais a contagem e o conjunto de
nomes das funções: nada perdido, nada inventado. **Diga sempre qual usou.**

**E a forma mais útil não é o binário "IGUAL": é "igual excepto N, e aqui
estão".** Num passo deu três linhas, todas do mesmo tipo — três `pub(in …)`
acrescentados —, e ver as três é o que permite dizer que o custo era o previsto
em vez de esperar que fosse.

### 4. O que NENHUMA delas prova

**Que o item continua a ser visto por quem o usa.** Duas funções e um `enum`
mudaram de ficheiro dentro do mesmo módulo, todas as guardas de texto passaram, e
o build partiu-se: quem os usava ficou **irmão** e privado não chega a irmãos.
O compilador disse; nenhuma prova de texto podia dizer.

**E que o mesmo número de testes continua a ser compilado.** Um teste que deixa
de ser compilado não falha — desaparece. Só a suite responde, e quando a árvore
partilhada não compila, essa prova fica **em dívida declarada**, nunca inferida.

**A extração garante que o texto é o mesmo; só o `cargo check` garante que ainda
se vê; e só a suite garante que ainda corre.**

### 5. Duas cegueiras do mesmo instrumento, com custos diferentes

O chunker parte por blocos que fecham numa linha exactamente `    }`. **Não vê
um `const`, um `static`, um `use` nem um `type`** — nenhum fecha assim. Um
`const` ao nível do módulo foi absorvido pelo bloco seguinte e viajou para o
ficheiro errado.

É a mesma forma do defeito do contador de linhas deste documento, que não via
`pub(crate) fn`: **o instrumento não vê uma forma e atribui o que ela contém a
quem está ao lado.**

A diferença está no custo, e é o argumento para correr a suite a cada passo em
vez de só no fim: o contador errava **em silêncio** e o número saía com
confiança; o chunker erra com um `E0425` que não deixa compilar. **Um
instrumento que falha alto é de outra categoria.**

### 6. Uma referência relativa não se reescreve — cria-se o nome no pai

Um corpo movido que dizia `super::lengths::…` passa a ter outro `super`. **Não
altere a linha** — isso é conteúdo movido a ser alterado, que é precisamente o
que esta arrumação existe para não fazer. Ponha `use super::lengths;` no pai: o
nome passa a existir e o corpo continua a dizer exactamente o que dizia. Uma
linha no pai em vez de uma linha alterada no filho.

### 7. A contagem de anotações mede a QUALIDADE do corte

Não é só um custo a pagar no fim: **é um sinal sobre a fronteira, disponível
antes de entregar.**

Num passo, a primeira tentativa foram quatro ficheiros e o `cargo check` pediu
**nove** anotações. A contagem dos consumidores mostrou que as nove tinham o
mesmo ficheiro de um lado — ele continha dois `impl` que consumiam de vizinhos
diferentes. **Não era uma folha entre dois vizinhos: era uma fronteira a passar
pelo meio de duas coisas.** Refeito em três, com cada `impl` do lado de quem ele
usa, ficaram sete e o ficheiro do meio deixou de existir.

**Se um pedaço precisa de muitas anotações, provavelmente está do lado errado da
fronteira.** Vale a pena refazer o corte antes de as escrever.

O mesmo raciocínio, em números, decidiu duas fronteiras noutro refactor: manter
as definições no pai custava **1 anotação contra ~15**, porque descer os tipos
levava os campos todos atrás.

E a regra irmã: **anotar o item errado inventa uma fronteira que não existe.**
Num caso havia duas funções com o mesmo nome em `impl` diferentes e só uma era
chamada de fora; a outra foi revertida a privada em vez de ficar aberta "já que
estava lá".

### 8. Auditar um instrumento PELO COMPLEMENTO

A pergunta certa a fazer a um scanner não é *"ele apanha X?"* — é:

> **quais linhas o instrumento NÃO apanha?**

Um scanner de itens de topo foi auditado assim: *quais linhas começam em coluna
0 e não casam com o padrão?* Resposta: nenhuma. **Isso audita o instrumento sem
depender do instrumento**, e é o único método que encontra o que não se sabe que
se procura.

As três cegueiras deste refactor sobreviveriam todas à pergunta direta: o
contador que não via `pub(crate) fn`, o chunker que não vê um `const`, o
verificador que contava elementos onde devia contar linhas. **Nenhuma sobrevive
à pergunta pelo complemento.** Confirmar pelo padrão confirma o padrão.

E a inversa vale como aviso: os cinco `macro_rules!` do `layout.rs` estão
indentados dentro de funções e **fecham com `    }` a quatro espaços**. Um
chunker de `    }` fecharia blocos falsos **no meio de uma função** — e onde os
blocos são corpos de função, uma fronteira falsa não dá erro de compilação.

### A regra 7, refinada: zero anotações não prova nada

Muitas anotações denunciam um corte errado **quando aparecem por a fronteira
atravessar algo que devia ficar inteiro**.

**A recíproca é falsa.** Num caso, a alternativa a três anotações era juntar
aritmética de matrizes com criação de texturas de GPU num ficheiro só: zero
anotações, porque **a fronteira não existia**. Tudo no mesmo ficheiro nunca
precisa de alcance.

Três anotações para separar dois assuntos distintos são um preço; zero para os
manter juntos é um custo disfarçado de poupança.

### Onde a rede é fina, o corte é pequeno

Nem todos os crates têm régua. O `rts-egui` tem **seis testes em 5 528 linhas**,
cinco deles de aritmética de matrizes num só ficheiro — um "6 → 6" prova quase
só que compila, e não há dump de página que cubra a pintura.

Aí a reconstrução deixa de ser confirmação e passa a ser **a prova principal**,
e entre um corte mais limpo e um corte mais pequeno escolhe-se o **mais
pequeno**: quanto menos se mexe, menos há para correr mal.

E quem entrega tem de dizer isto à cabeça, para o "verde" não ser lido com o
peso que teria noutro crate.

### 9. O `--numstat` deixa de servir quando as remoções são muitas e dispersas

Ele prova "só se acrescentaram `mod` e `use`" **enquanto as remoções forem um
bloco contíguo, ou dois.** Num passo que tirou nove blocos de um ficheiro de
7 000 linhas, respondeu:

    155  2524  crates/rts-dom/src/layout.rs

**155 adicionadas quando só 29 eram novas.** As outras 126 são código que
ninguém tocou, contado como removido-e-readicionado porque o algoritmo de diff
realinhou as fronteiras à volta dos nove buracos.

Entregue sem ser lido, esse número afirmaria "155 linhas de lógica novas" sobre
um passo que não tem nenhuma. **A técnica não avisa quando deixa de valer** — é
preciso saber que ela tem esta forma de falhar.

A prova que resta e que basta: **o pai reconstruído idêntico byte a byte**, cada
pedaço idêntico descontando as promoções declaradas, e a soma a fechar com o
total original.

### 10. Reportar o NÚMERO, não a conclusão

Num só passo, três instrumentos falharam — um índice `[-1]` que apanhou o
elemento errado, um verificador que procurou uma marca no ficheiro inteiro em vez
do bloco, e um erro de sintaxe num literal. **Nenhum chegou ao resultado, e
todos foram apanhados por o número não fazer sentido**: "bloco adicionado: 4 566
linhas" é absurdo à vista.

Um `PASSA` não teria mostrado nada. **Um instrumento que imprime o número que
mediu denuncia-se sozinho; um que imprime só o veredicto, não.**

